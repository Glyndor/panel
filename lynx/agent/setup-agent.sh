#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# setup-agent.sh — Lynx Agent install script
#
# Description:
#   Installs the Lynx Agent on a VPS. Sets up:
#     - System user: lynx-agent (privileged, not a login shell)
#     - subuid/subgid ranges for rootless Podman tenant isolation
#     - PostgreSQL container (via podman run, lynx-agent-db network)
#     - lynx-agent binary as a systemd service with required capabilities
#     - WireGuard tunnel to the Lynx Dashboard
#     - nftables: allows only WireGuard inbound, blocks everything else
#
# Usage:
#   sudo ./setup-agent.sh
#
# Requirements:
#   - Debian/Ubuntu or RHEL-based Linux (amd64 / arm64)
#   - Run as root
#   - Dashboard WireGuard pubkey and PSK (shown at dashboard install completion)
# -----------------------------------------------------------------------------

set -euo pipefail

# --- Colors -----------------------------------------------------------------

RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

# --- Logging ----------------------------------------------------------------

log_info()    { echo -e "${CYAN}[INFO]${RESET}  $*"; }
log_ok()      { echo -e "${GREEN}[OK]${RESET}    $*"; }
log_warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
log_error()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; }
log_section() { echo -e "\n${BOLD}${CYAN}=== $* ===${RESET}"; }

# --- Constants --------------------------------------------------------------

LYNX_DIR="/etc/lynx"
AGENT_CONF="$LYNX_DIR/agent.env"
LYNX_WG_DIR="$LYNX_DIR/wireguard"
LYNX_WG_CONF="$LYNX_WG_DIR/lynx-wg.conf"   # source of truth (spec path)
WG_DIR="/etc/wireguard"
WG_CONF_LINK="$WG_DIR/wg-lynx-agent.conf"   # symlink for wg-quick compatibility
WG_IFACE="wg-lynx-agent"
AGENT_WG_IP="10.100.0.2"
DASHBOARD_WG_IP="10.100.0.1"
WG_SUBNET="10.100.0.0/24"
WG_PORT=51820
AGENT_PORT=9090
LYNX_AGENT_USER="lynx-agent"
PG_NETWORK="lynx-agent-db"
PG_CONTAINER="lynx-agent-postgres"
PG_IMAGE="docker.io/library/postgres@sha256:bfae840554bdbd4e9f8d097d8e23ffda8aac82866e04ea0d6bc09647234dd359"
PG_DB="lynx_agent"
# Agent UUID v7 — generated on first install, persists across updates
AGENT_ID=""

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="/etc/lynx/bin"
BINARY_PATH="$BIN_DIR/lynx-agent"

# --- Root check -------------------------------------------------------------

if [[ $EUID -ne 0 ]]; then
    log_error "Must run as root: sudo $0"
    exit 1
fi

# --- Cleanup function -------------------------------------------------------

_cleanup_existing() {
    log_section "Removing existing agent installation"

    systemctl disable --now lynx-agent.service 2>/dev/null || true
    systemctl disable --now lynx-agent-postgres.service 2>/dev/null || true
    rm -f /etc/systemd/system/lynx-agent-postgres.service

    # Remove WireGuard
    if ip link show "$WG_IFACE" &>/dev/null; then
        wg-quick down "$WG_IFACE" 2>/dev/null || ip link delete "$WG_IFACE" 2>/dev/null || true
    fi
    rm -f "$WG_CONF_LINK" "$LYNX_WG_CONF"

    # Remove PostgreSQL container + data
    podman rm -f "$PG_CONTAINER" 2>/dev/null || true
    podman volume rm lynx-agent-pg-data 2>/dev/null || true
    podman network rm "$PG_NETWORK" 2>/dev/null || true

    # Remove Podman secrets
    for s in lynx-agent-pg-root lynx-agent-pg-pass lynx-agent-internal-token lynx-agent-database-url; do
        podman secret rm "$s" 2>/dev/null || true
    done

    # Remove systemd units
    rm -f /etc/systemd/system/lynx-agent.service
    systemctl daemon-reload

    # Remove user (tenant users cleaned separately)
    userdel -r "$LYNX_AGENT_USER" 2>/dev/null || true

    # Remove nftables table
    nft delete table inet lynx-agent 2>/dev/null || true
    rm -f /etc/nftables-lynx-agent.conf

    rm -rf "$LYNX_DIR"
    log_ok "Cleanup complete"
}

# --- RAM check --------------------------------------------------------------

log_section "Checking system resources"

TOTAL_RAM_MB=$(free -m | awk '/^Mem:/{print $2}')
if [[ "$TOTAL_RAM_MB" -lt 512 ]]; then
    log_error "Insufficient RAM: ${TOTAL_RAM_MB} MB detected, minimum 512 MB required"
    log_info  "Lynx Agent requires at least 512 MB RAM for the local PostgreSQL container"
    exit 1
fi
log_ok "RAM: ${TOTAL_RAM_MB} MB (minimum 512 MB satisfied)"

# --- Incompatible software --------------------------------------------------

log_section "Checking for incompatible software"

log_info "Lynx uses Podman for containers and nftables for firewall."
log_info "The following software is incompatible and will be removed if found:"
log_info "  Docker, containerd (standalone), firewalld, ufw, iptables (legacy)"
log_info "Reason: these programs add their own firewall/network rules outside"
log_info "        table inet lynx-agent, silently exposing ports Lynx considers closed."

_detect_distro() {
    if command -v apt-get &>/dev/null;   then echo "debian"
    elif command -v dnf &>/dev/null;     then echo "rhel"
    elif command -v yum &>/dev/null;     then echo "rhel"
    else                                      echo "unknown"
    fi
}

DISTRO=$(_detect_distro)

_pkg_installed() {
    local pkg="$1"
    case "$DISTRO" in
        debian) dpkg -l "$pkg" 2>/dev/null | grep -q '^ii' ;;
        rhel)   rpm -q "$pkg" &>/dev/null ;;
        *)      return 1 ;;
    esac
}

_remove_pkg() {
    local pkg="$1" reason="$2"
    log_warn "Removing incompatible package: ${pkg}"
    log_info "  Reason: ${reason}"
    case "$DISTRO" in
        debian) apt-get purge -y "$pkg" 2>/dev/null || true ;;
        rhel)   { dnf remove -y "$pkg" 2>/dev/null || yum remove -y "$pkg" 2>/dev/null; } || true ;;
        *)      log_warn "Unknown distro — remove ${pkg} manually before continuing" ;;
    esac
    log_ok "Removed: $pkg"
}

_incompatible_found=false

_check_remove() {
    local pkg="$1" reason="$2"
    if _pkg_installed "$pkg"; then
        _incompatible_found=true
        _remove_pkg "$pkg" "$reason"
    fi
}

_REASON_DOCKER="manages own container network and firewall, bypasses lynx-agent nftables"
_REASON_CTR="manages own container network, conflicts with Podman network isolation"
_REASON_FW="manages own firewall rules outside table inet lynx-agent"

for pkg in docker-ce docker-ce-cli docker.io docker-compose-plugin moby-engine; do
    _check_remove "$pkg" "$_REASON_DOCKER"
done

for pkg in containerd containerd.io; do
    _check_remove "$pkg" "$_REASON_CTR"
done

_check_remove firewalld "$_REASON_FW"
_check_remove ufw       "$_REASON_FW"

# iptables — only block the legacy binary, not the nftables compat layer (iptables-nft)
if command -v iptables &>/dev/null && ! iptables --version 2>/dev/null | grep -q 'nf_tables'; then
    _incompatible_found=true
    log_warn "Removing incompatible: iptables (legacy binary, not nftables-compat)"
    log_info "  Reason: ${_REASON_FW}"
    case "$DISTRO" in
        debian) apt-get purge -y iptables 2>/dev/null || true ;;
        rhel)   { dnf remove -y iptables 2>/dev/null || yum remove -y iptables 2>/dev/null; } || true ;;
        *)      log_warn "Unknown distro — remove iptables manually" ;;
    esac
    log_ok "Removed: iptables (legacy)"
fi

if $_incompatible_found; then
    if command -v iptables-legacy &>/dev/null; then
        iptables-legacy -F              2>/dev/null || true
        iptables-legacy -X              2>/dev/null || true
        iptables-legacy -t nat    -F    2>/dev/null || true
        iptables-legacy -t nat    -X    2>/dev/null || true
        iptables-legacy -t mangle -F    2>/dev/null || true
        iptables-legacy -t mangle -X    2>/dev/null || true
    fi
    log_ok "Incompatible software removed — residual firewall rules cleared"
else
    log_ok "No incompatible software found"
fi

unset _REASON_DOCKER _REASON_CTR _REASON_FW

# --- Detect existing installation -------------------------------------------

log_section "Checking for existing installation"

existing=false
if [[ -d "$LYNX_DIR" ]] || id "$LYNX_AGENT_USER" &>/dev/null || \
   systemctl list-unit-files lynx-agent.service &>/dev/null 2>&1 | grep -q lynx-agent; then
    existing=true
fi

if $existing; then
    log_warn "Existing agent installation detected."
    echo ""
    echo -e "  ${BOLD}1)${RESET} Abort (default)"
    echo -e "  ${BOLD}2)${RESET} Update → updates binary, preserves all data"
    echo -e "  ${BOLD}3)${RESET} Reinstall clean → destroys all agent data"
    echo ""
    read -rp "Choice [1/2/3]: " choice
    choice="${choice:-1}"

    case "$choice" in
        2)
            log_info "Redirecting to update..."
            exec "$(dirname "${BASH_SOURCE[0]}")/update-agent.sh"
            ;;
        3)
            echo ""
            log_warn "This will permanently destroy all agent data on this machine."
            read -rp "Type 'reinstall lynx-agent' to confirm: " confirm
            if [[ "$confirm" != "reinstall lynx-agent" ]]; then
                log_error "Confirmation phrase mismatch. Aborting."
                exit 1
            fi
            # Preserve agent ID across reinstalls — dashboard still has the old one registered
            _SAVED_AGENT_ID=""
            if [[ -f "$LYNX_DIR/agent-id" ]]; then
                _SAVED_AGENT_ID=$(cat "$LYNX_DIR/agent-id")
                log_info "Preserving Agent ID for reinstall: $_SAVED_AGENT_ID"
            fi
            _cleanup_existing
            ;;
        *)
            log_info "Aborting. No changes made."
            exit 0
            ;;
    esac
fi

# --- DNS preflight check ----------------------------------------------------

log_section "Checking network connectivity"

if ! getent hosts archive.ubuntu.com &>/dev/null && ! getent hosts packages.fedoraproject.org &>/dev/null; then
    log_warn "DNS resolution failing — attempting to fix..."
    rm -f /etc/resolv.conf
    echo 'nameserver 8.8.8.8' > /etc/resolv.conf
    if ! getent hosts archive.ubuntu.com &>/dev/null 2>&1; then
        log_error "DNS resolution is unavailable. Please fix your network configuration and retry."
        exit 1
    fi
    log_ok "DNS resolution restored (set nameserver to 8.8.8.8)"
else
    log_ok "DNS resolution working"
fi

# --- Install dependencies ---------------------------------------------------

log_section "Checking system dependencies"

_apt_updated=false
_apt_ensure() {
    local cmd="$1" pkg="$2"
    if command -v "$cmd" &>/dev/null; then
        log_ok "$cmd found"
        return
    fi
    log_info "Installing $pkg..."
    if ! $_apt_updated; then
        if command -v add-apt-repository &>/dev/null; then
            add-apt-repository -y universe &>/dev/null || true
        fi
        DEBIAN_FRONTEND=noninteractive apt-get update -qq
        _apt_updated=true
    fi
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "$pkg" -qq
    if command -v "$cmd" &>/dev/null; then
        log_ok "$cmd installed"
    else
        log_error "Failed to install $pkg (command: $cmd)"
        exit 1
    fi
}

_require_cmd() {
    if ! command -v "$1" &>/dev/null; then
        log_error "Required command not found: $1 — $2"
        exit 1
    fi
    log_ok "$1 found"
}

_apt_ensure podman  podman
_apt_ensure openssl openssl
_apt_ensure nft     nftables
_apt_ensure wg      wireguard-tools
_apt_ensure curl    curl
_apt_ensure python3 python3
_apt_ensure pip3    python3-pip
_require_cmd systemctl "systemd required"
_require_cmd free      "procps required"

for _pkg in netavark aardvark-dns; do
    if ! dpkg -l "$_pkg" 2>/dev/null | grep -q '^ii'; then
        log_info "Installing $_pkg..."
        if ! $_apt_updated; then
            DEBIAN_FRONTEND=noninteractive apt-get update -qq
            _apt_updated=true
        fi
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "$_pkg" -qq
        dpkg -l "$_pkg" 2>/dev/null | grep -q '^ii' || { log_error "Failed to install $_pkg"; exit 1; }
        log_ok "$_pkg installed"
    else
        log_ok "$_pkg found"
    fi
done

if ! grep -q 'network_backend.*netavark' /etc/containers/containers.conf 2>/dev/null; then
    mkdir -p /etc/containers
    {
        grep -v 'network_backend\|\[network\]' /etc/containers/containers.conf 2>/dev/null || true
        printf '\n[network]\nnetwork_backend = "netavark"\n'
    } > /tmp/lynx-containers.conf
    mv /tmp/lynx-containers.conf /etc/containers/containers.conf
    log_ok "Podman configured to use Netavark network backend"
fi

# --- NTP synchronization check ----------------------------------------------
#
# The 30s timestamp window on signed agent commands requires synchronized clocks.
# Clock drift >30s causes all commands to be rejected (effective lockdown).

log_section "Checking NTP synchronization"

_ntp_active=false

if systemctl is-active --quiet systemd-timesyncd 2>/dev/null; then
    _ntp_active=true
    log_ok "systemd-timesyncd is active"
elif systemctl is-active --quiet chronyd 2>/dev/null; then
    _ntp_active=true
    log_ok "chronyd is active"
fi

if ! $_ntp_active; then
    log_warn "No NTP service detected — enabling systemd-timesyncd..."
    if systemctl enable --now systemd-timesyncd 2>/dev/null; then
        sleep 2
        _ntp_active=true
        log_ok "systemd-timesyncd enabled and started"
    else
        log_warn "Could not enable systemd-timesyncd automatically"
        log_warn "Install chrony (apt install chrony) or enable systemd-timesyncd before adding agents"
        log_warn "Without NTP: agent commands will be rejected once clock drifts >30s"
    fi
fi

unset _ntp_active

# --- Collect dashboard bootstrap data ---------------------------------------

log_section "Dashboard connection setup"

echo ""
echo -e "${YELLOW}You need the values shown at the end of the dashboard install.${RESET}"
echo ""

read -rp "  Dashboard WireGuard endpoint (IP:PORT, e.g. 1.2.3.4:51820): " DASHBOARD_ENDPOINT
read -rp "  Dashboard WireGuard public key: " DASHBOARD_PUBKEY
read -rsp "  Preshared key (PSK): " PSK
echo ""

if [[ -z "$DASHBOARD_ENDPOINT" || -z "$DASHBOARD_PUBKEY" || -z "$PSK" ]]; then
    log_error "All three values are required."
    exit 1
fi

DASHBOARD_IP="${DASHBOARD_ENDPOINT%%:*}"
DASHBOARD_WG_LISTEN="${DASHBOARD_ENDPOINT##*:}"

# --- Create directories -----------------------------------------------------

log_section "Creating directories"

mkdir -p "$LYNX_DIR"
chmod 755 "$LYNX_DIR"
log_ok "$LYNX_DIR"

# --- Generate agent UUID ----------------------------------------------------

log_section "Generating agent identity"

if [[ -n "${_SAVED_AGENT_ID:-}" ]]; then
    AGENT_ID="$_SAVED_AGENT_ID"
    log_ok "Reusing existing Agent ID: $AGENT_ID"
    unset _SAVED_AGENT_ID
else
    # UUIDv7: time-ordered. Generate with Python or fall back to uuidgen.
    if python3 -c "import uuid; print(uuid.uuid7())" &>/dev/null 2>&1; then
        AGENT_ID=$(python3 -c "import uuid; print(uuid.uuid7())")
    elif command -v uuidgen &>/dev/null && uuidgen --version 2>&1 | grep -q "2\.4[0-9]"; then
        AGENT_ID=$(uuidgen --time)
    else
        # Construct a v7-like UUID using current time + random bytes
        TS_MS=$(date +%s%3N)
        TS_HEX=$(printf '%012x' "$TS_MS")
        RAND=$(openssl rand -hex 10)
        VARIANT=$(printf '%02x' $((0x80 | (0x$(openssl rand -hex 1) & 0x3f))))
        AGENT_ID="${TS_HEX:0:8}-${TS_HEX:8:4}-7${RAND:0:3}-${VARIANT}${RAND:4:2}-${RAND:6:12}"
    fi
    printf '%s' "$AGENT_ID" > "$LYNX_DIR/agent-id"
    chmod 600 "$LYNX_DIR/agent-id"
fi
log_ok "Agent ID: $AGENT_ID"

# --- Create system user -----------------------------------------------------

log_section "Creating system user: $LYNX_AGENT_USER"

if ! id "$LYNX_AGENT_USER" &>/dev/null; then
    useradd \
        --system \
        --no-create-home \
        --shell /usr/sbin/nologin \
        --comment "Lynx Agent service user" \
        "$LYNX_AGENT_USER"
    log_ok "User created: $LYNX_AGENT_USER"
else
    log_warn "User $LYNX_AGENT_USER already exists — skipping"
fi

# Enable lingering for rootless Podman (tenant containers persist after session)
loginctl enable-linger "$LYNX_AGENT_USER" 2>/dev/null || true

# --- subuid / subgid allocation for tenant isolation -----------------------
#
# Each tenant (lynx-tenant-{id}) gets 65536 subuids/subgids.
# The agent user itself needs a base allocation for its own Podman.

log_section "Configuring subuid/subgid ranges"

AGENT_UID=$(id -u "$LYNX_AGENT_USER")

# Agent user: 1,000,000 – 1,065,535 (65536 IDs)
if ! grep -q "^${LYNX_AGENT_USER}:" /etc/subuid 2>/dev/null; then
    echo "${LYNX_AGENT_USER}:1000000:65536" >> /etc/subuid
    log_ok "subuid: $LYNX_AGENT_USER → 1000000+65536"
fi
if ! grep -q "^${LYNX_AGENT_USER}:" /etc/subgid 2>/dev/null; then
    echo "${LYNX_AGENT_USER}:1000000:65536" >> /etc/subgid
    log_ok "subgid: $LYNX_AGENT_USER → 1000000+65536"
fi

# --- Generate agent secrets -------------------------------------------------

log_section "Generating agent secrets"

log_info "PostgreSQL root password..."
(
    PG_ROOT=$(openssl rand -hex 32)
    printf '%s' "$PG_ROOT" | podman secret create lynx-agent-pg-root - >/dev/null
    PG_ROOT="$(openssl rand -hex 32)"
)

log_info "PostgreSQL app password + database URL..."
mkdir -p /etc/lynx/credentials
chmod 700 /etc/lynx/credentials
(
    PG_PASS=$(openssl rand -hex 32)
    DB_URL="postgresql://lynx_agent_app:${PG_PASS}@localhost:5434/${PG_DB}"
    printf '%s' "$PG_PASS" | podman secret create lynx-agent-pg-pass - >/dev/null
    printf '%s' "$DB_URL" | podman secret create lynx-agent-database-url - >/dev/null
    # Write credential file now — only moment we have the URL in memory
    printf '%s' "$DB_URL" > /etc/lynx/credentials/database-url
    chmod 600 /etc/lynx/credentials/database-url
    PG_PASS="$(openssl rand -hex 32)"
    DB_URL="$(openssl rand -hex 32)"
)

log_info "Internal bearer token..."
INTERNAL_TOKEN=$(openssl rand -hex 32)
printf '%s' "$INTERNAL_TOKEN" | podman secret create lynx-agent-internal-token - >/dev/null

log_ok "Agent secrets generated"

# --- Podman network for agent DB -------------------------------------------

log_section "Creating Podman network: $PG_NETWORK"

if ! podman network exists "$PG_NETWORK" 2>/dev/null; then
    podman network create "$PG_NETWORK"
    log_ok "Network created: $PG_NETWORK"
else
    log_warn "Network $PG_NETWORK already exists — skipping"
fi

# --- PostgreSQL init script -------------------------------------------------

log_section "Preparing PostgreSQL init script"

PG_INIT_DIR="$LYNX_DIR/pg-init"
mkdir -p "$PG_INIT_DIR"

cat > "$PG_INIT_DIR/01-init.sql" << 'PGSQL'
\set app_pass `cat /run/secrets/lynx-agent-pg-pass`

CREATE USER lynx_agent_app WITH PASSWORD :'app_pass' NOSUPERUSER NOCREATEDB NOCREATEROLE;
GRANT CONNECT ON DATABASE lynx_agent TO lynx_agent_app;
\connect lynx_agent
GRANT USAGE, CREATE ON SCHEMA public TO lynx_agent_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO lynx_agent_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT USAGE, SELECT ON SEQUENCES TO lynx_agent_app;
PGSQL

chmod 644 "$PG_INIT_DIR/01-init.sql"
log_ok "Init script: $PG_INIT_DIR/01-init.sql"

# --- Start PostgreSQL container ---------------------------------------------

log_section "Starting PostgreSQL for agent"

podman run -d \
    --name "$PG_CONTAINER" \
    --network "$PG_NETWORK" \
    --secret lynx-agent-pg-root,target=lynx-agent-pg-root \
    --secret lynx-agent-pg-pass,target=lynx-agent-pg-pass \
    -e POSTGRES_USER=postgres \
    -e POSTGRES_DB="$PG_DB" \
    -e POSTGRES_PASSWORD_FILE=/run/secrets/lynx-agent-pg-root \
    -p 127.0.0.1:5434:5432 \
    -v lynx-agent-pg-data:/var/lib/postgresql \
    -v "$PG_INIT_DIR:/docker-entrypoint-initdb.d:ro" \
    --restart unless-stopped \
    "$PG_IMAGE"

log_info "Waiting for PostgreSQL to be healthy..."
for i in $(seq 1 40); do
    if podman exec "$PG_CONTAINER" pg_isready -U postgres -d "$PG_DB" &>/dev/null; then
        log_ok "PostgreSQL healthy"
        break
    fi
    if [[ $i -eq 40 ]]; then
        log_error "PostgreSQL did not become healthy"
        podman logs --tail 30 "$PG_CONTAINER"
        exit 1
    fi
    sleep 2
done

# --- Download agent binary from GitHub Releases ----------------------------

log_section "Downloading lynx-agent binary"

GITHUB_REPO="Jaro-c/Lynx"

_ARCH=$(uname -m)
case "$_ARCH" in
    x86_64)  ARCH="x86_64" ;;
    aarch64) ARCH="arm64" ;;
    *)
        log_error "Unsupported architecture: $_ARCH"
        exit 1
        ;;
esac
log_info "Architecture: $ARCH"

# Ensure Python cryptography library for signature verification
if ! python3 -c "from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey" 2>/dev/null; then
    log_info "Installing Python cryptography library..."
    if command -v pip3 &>/dev/null; then
        pip3 install --quiet cryptography
    else
        python3 -m pip install --quiet cryptography
    fi
fi

log_info "Fetching latest agent release..."
LATEST_AGENT_TAG=$(curl -fsSL \
    "https://api.github.com/repos/${GITHUB_REPO}/releases" \
    | python3 -c "
import sys, json
releases = json.load(sys.stdin)
for r in releases:
    tag = r.get('tag_name', '')
    if tag.startswith('agent@') and not r.get('prerelease'):
        print(tag)
        break
" 2>/dev/null)

if [[ -z "$LATEST_AGENT_TAG" ]]; then
    log_error "No agent release found in ${GITHUB_REPO}"
    exit 1
fi
log_ok "Latest release: ${LATEST_AGENT_TAG}"

RELEASE_BASE="https://github.com/${GITHUB_REPO}/releases/download/${LATEST_AGENT_TAG}"
mkdir -p "$BIN_DIR"
chmod 755 "$BIN_DIR"

_verify_release_sig() {
    local file="$1" sig_file="$2"
    python3 - "$file" "$sig_file" <<'PYEOF'
import sys, base64
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

pub_b64 = "OsBV4t+vQSn10FAI8UzAJEBS0IUqp8D2bZtlQYD8j+Q="
pub_key = Ed25519PublicKey.from_public_bytes(base64.b64decode(pub_b64 + "=="))

with open(sys.argv[1], "rb") as f:
    data = f.read()
with open(sys.argv[2], "rb") as f:
    sig = f.read()
try:
    pub_key.verify(sig, data)
except Exception as e:
    print(f"signature invalid: {e}", file=sys.stderr)
    sys.exit(1)
PYEOF
}

AGENT_TMP="${BIN_DIR}/lynx-agent.new"

log_info "Downloading agent binary..."
curl -fsSL --max-time 300 \
    "${RELEASE_BASE}/lynx-agent-linux-${ARCH}" \
    -o "$AGENT_TMP"
curl -fsSL --max-time 30 \
    "${RELEASE_BASE}/lynx-agent-linux-${ARCH}.sig" \
    -o "${AGENT_TMP}.sig"

log_info "Verifying agent signature..."
if ! _verify_release_sig "$AGENT_TMP" "${AGENT_TMP}.sig"; then
    log_error "Agent signature verification FAILED — aborting"
    rm -f "$AGENT_TMP" "${AGENT_TMP}.sig"
    exit 1
fi
rm -f "${AGENT_TMP}.sig"
chmod 755 "$AGENT_TMP"
mv "$AGENT_TMP" "$BINARY_PATH"
log_ok "Agent binary installed: ${BINARY_PATH}"

# Write version file
printf '%s' "${LATEST_AGENT_TAG#agent@}" > "$BIN_DIR/lynx-agent-version"
log_ok "Version: ${LATEST_AGENT_TAG#agent@}"

# --- Write agent env file ---------------------------------------------------

log_section "Writing agent configuration"

# Detect if this is the dashboard VPS — setup-dashboard.sh leaves nginx config
IS_DASHBOARD_VPS=false
DASHBOARD_PORT_CONF=""
if [[ -f /etc/lynx/nginx/default.conf ]]; then
    IS_DASHBOARD_VPS=true
    DASHBOARD_PORT_CONF="DASHBOARD_PORT=19443"
    log_info "Dashboard VPS detected — local agent mode, will open port 19443"
fi

cat > "$AGENT_CONF" << EOF
AGENT_ID=${AGENT_ID}
DATABASE_URL_FILE=/run/credentials/lynx-agent.service/database-url
INTERNAL_TOKEN_FILE=/run/credentials/lynx-agent.service/internal-token
LISTEN_ADDR=127.0.0.1:${AGENT_PORT}
RUST_LOG=info
${DASHBOARD_PORT_CONF}
EOF

chmod 600 "$AGENT_CONF"
log_ok "Config: $AGENT_CONF"

# Write INTERNAL_TOKEN to systemd credential file (source on disk, 600 root-only;
# systemd LoadCredential exposes it at /run/credentials/... tmpfs at service start)
printf '%s' "$INTERNAL_TOKEN" > /etc/lynx/credentials/internal-token
chmod 600 /etc/lynx/credentials/internal-token

# Clear INTERNAL_TOKEN from memory
INTERNAL_TOKEN="$(openssl rand -hex 32)"
unset INTERNAL_TOKEN

# --- Create systemd service -------------------------------------------------

log_section "Installing systemd service"

# Service that starts the PostgreSQL container at boot.
# Podman's podman-restart.service only handles restart-policy=always;
# our container uses unless-stopped, so we manage boot startup explicitly.
cat > /etc/systemd/system/lynx-agent-postgres.service << 'EOF'
[Unit]
Description=Lynx Agent — PostgreSQL container
After=network.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/bin/podman start lynx-agent-postgres
ExecStop=/usr/bin/podman stop -t 30 lynx-agent-postgres

[Install]
WantedBy=multi-user.target
EOF

cat > /etc/systemd/system/lynx-agent.service << EOF
[Unit]
Description=Lynx Agent — infrastructure orchestration service
Documentation=https://github.com/Jaro-c/Lynx
After=network.target lynx-agent-postgres.service
Requires=network.target lynx-agent-postgres.service

[Service]
Type=simple
User=${LYNX_AGENT_USER}
Group=${LYNX_AGENT_USER}
EnvironmentFile=${AGENT_CONF}
ExecStart=${BINARY_PATH}
Restart=on-failure
RestartSec=5s
TimeoutStopSec=30s

# Capabilities required for nftables, Podman tenant management, VPS reboot
AmbientCapabilities=CAP_NET_ADMIN CAP_SYS_ADMIN CAP_SETUID CAP_SETGID CAP_SYS_BOOT
CapabilityBoundingSet=CAP_NET_ADMIN CAP_SYS_ADMIN CAP_SETUID CAP_SETGID CAP_SYS_BOOT

# Systemd credentials (tmpfs — never touches disk)
LoadCredential=database-url:/etc/lynx/credentials/database-url
LoadCredential=internal-token:/etc/lynx/credentials/internal-token

# Security hardening
NoNewPrivileges=no
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadWritePaths=/run/containers /var/lib/containers /home /etc/lynx/bin

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable lynx-agent-postgres.service
systemctl enable lynx-agent.service
log_ok "Services installed: lynx-agent-postgres.service, lynx-agent.service"

# --- WireGuard agent side ---------------------------------------------------

log_section "Configuring WireGuard tunnel (agent ↔ dashboard)"

# Generate agent keypair
AGENT_PRIV=$(wg genkey)
AGENT_PUB=$(printf '%s' "$AGENT_PRIV" | wg pubkey)

# --- NAT detection ---
# Extract the dashboard host (strip port if present)
DASHBOARD_HOST="${DASHBOARD_ENDPOINT%%:*}"

# IP of the local interface that would route to the dashboard
LOCAL_IFACE_IP=$(ip route get "$DASHBOARD_HOST" 2>/dev/null | grep -oP 'src \K\S+' | head -1)

# Public IP as seen from the internet
PUBLIC_IP=$(curl -sf --max-time 5 https://ifconfig.me 2>/dev/null || \
            curl -sf --max-time 5 https://api.ipify.org 2>/dev/null || true)

NAT_DETECTED=false
KEEPALIVE_LINE=""

if [[ -n "$LOCAL_IFACE_IP" && -n "$PUBLIC_IP" && "$LOCAL_IFACE_IP" != "$PUBLIC_IP" ]]; then
    NAT_DETECTED=true
    KEEPALIVE_LINE="PersistentKeepalive = 25"
    log_info "NAT detected (interface IP: ${LOCAL_IFACE_IP}, public IP: ${PUBLIC_IP})"
    log_info "Enabling PersistentKeepalive = 25 to maintain NAT table entry"
    log_warn "If your provider's NAT timeout is < 25s or blocks persistent UDP, the tunnel may be unstable"
elif [[ -z "$PUBLIC_IP" ]]; then
    # Cannot determine — enable keepalive as safe default
    NAT_DETECTED=true
    KEEPALIVE_LINE="PersistentKeepalive = 25"
    log_warn "Could not determine public IP — enabling PersistentKeepalive = 25 as safe default"
else
    log_info "No NAT detected (interface IP matches public IP: ${PUBLIC_IP})"
fi

# Build WireGuard peer block
WG_PEER_BLOCK="[Peer]
PublicKey = ${DASHBOARD_PUBKEY}
PresharedKey = ${PSK}
Endpoint = ${DASHBOARD_ENDPOINT}
AllowedIPs = ${DASHBOARD_WG_IP}/32"

if [[ -n "$KEEPALIVE_LINE" ]]; then
    WG_PEER_BLOCK="${WG_PEER_BLOCK}
${KEEPALIVE_LINE}"
fi

mkdir -p "$LYNX_WG_DIR"
cat > "$LYNX_WG_CONF" << EOF
[Interface]
PrivateKey = ${AGENT_PRIV}
Address = ${AGENT_WG_IP}/24

${WG_PEER_BLOCK}
EOF

chmod 600 "$LYNX_WG_CONF"
chown lynx-agent:lynx-agent "$LYNX_WG_CONF"

# Symlink into /etc/wireguard/ for wg-quick compatibility
mkdir -p "$WG_DIR"
ln -sf "$LYNX_WG_CONF" "$WG_CONF_LINK"

AGENT_PRIV="$(openssl rand -hex 32)"  # overwrite
PSK="$(openssl rand -hex 32)"
unset AGENT_PRIV PSK

# Bring up WireGuard
wg-quick up "$WG_IFACE"
systemctl enable "wg-quick@${WG_IFACE}"
log_ok "WireGuard interface up: $WG_IFACE"

# Test connectivity to dashboard
log_info "Testing WireGuard connectivity to dashboard (${DASHBOARD_WG_IP})..."
if ping -c 3 -W 3 "$DASHBOARD_WG_IP" &>/dev/null; then
    log_ok "Dashboard reachable via WireGuard"
else
    log_warn "Cannot reach dashboard at ${DASHBOARD_WG_IP} — add agent pubkey to dashboard first"
fi

# --- nftables — agent firewall ----------------------------------------------

log_section "Configuring nftables (agent)"

# Build optional dashboard-VPS-only rules
DASHBOARD_PORT_NFT=""
DASHBOARD_DNS_NFT=""
DASHBOARD_FORWARD_NFT=""
if [[ "$IS_DASHBOARD_VPS" == "true" ]]; then
    DASHBOARD_PORT_NFT="        # Dashboard panel port
        tcp dport 19443 ct state new accept
"
    DASHBOARD_DNS_NFT="        # DNS for container networks (aardvark-dns on Netavark bridge interfaces)
        iifname \"podman*\" udp dport 53 accept
        iifname \"podman*\" tcp dport 53 accept
"
    DASHBOARD_FORWARD_NFT="
        # New connections to published container ports (Netavark DNAT rewrites dst to 10.89.x.x)
        ip daddr 10.89.0.0/16 ct state new accept

        # Container-to-container traffic on Netavark bridge networks
        iifname \"podman*\" oifname \"podman*\" accept

        # Backend container traffic to/from WireGuard (dashboard <-> agents)
        oifname \"wg-lynx-dash\" accept
        iifname \"wg-lynx-dash\" accept"
fi

# Bootstrap ruleset — uses same chain names as the Rust agent (lynx-base, lynx-forward, lynx-output).
# The agent binary will flush and replace this on startup via render_ruleset().
# The flush prefix ensures no orphaned chains from previous installs survive.
cat > /etc/nftables-lynx-agent.conf << EOF
destroy table inet lynx-agent
add table inet lynx-agent
table inet lynx-agent {
    chain lynx-base {
        type filter hook input priority 0; policy drop;

        # Loopback
        iif lo accept

        # Established / related
        ct state established,related accept

        # ICMP
        ip  protocol icmp  accept
        ip6 nexthdr  icmpv6 accept

        # SSH — per-source-IP rate limit
        tcp dport 22 ct state new meter ssh_throttle { ip saddr limit rate 10/minute burst 20 packets } accept

        # WireGuard inbound (dashboard connects here)
        udp dport ${WG_PORT} accept

${DASHBOARD_PORT_NFT}
${DASHBOARD_DNS_NFT}
        # Agent API — only from WireGuard interface
        iifname "${WG_IFACE}" tcp dport ${AGENT_PORT} accept

        drop
    }

    chain lynx-global {}
    chain lynx-local {}

    chain lynx-forward {
        type filter hook forward priority 0; policy drop;

        ct state established,related accept
${DASHBOARD_FORWARD_NFT}
    }

    chain lynx-output {
        type filter hook output priority 0; policy accept;
    }
}
EOF

nft -f /etc/nftables-lynx-agent.conf
log_ok "nftables rules applied"

if [[ -f /etc/nftables.conf ]]; then
    if ! grep -q "lynx-agent" /etc/nftables.conf; then
        echo 'include "/etc/nftables-lynx-agent.conf"' >> /etc/nftables.conf
    fi
fi
systemctl enable nftables 2>/dev/null || true

# --- Start agent service ----------------------------------------------------

log_section "Starting lynx-agent service"

systemctl start lynx-agent.service
sleep 3

if systemctl is-active --quiet lynx-agent.service; then
    log_ok "lynx-agent is running"
else
    log_error "lynx-agent failed to start"
    systemctl status lynx-agent.service --no-pager
    exit 1
fi

# --- Done -------------------------------------------------------------------

log_section "Agent installation complete"

echo ""
echo -e "${GREEN}${BOLD}Lynx Agent is running!${RESET}"
echo ""
echo -e "${BOLD}${YELLOW}=== Add this agent to your dashboard ===${RESET}"
echo -e "  ${BOLD}Agent ID:${RESET}      ${AGENT_ID}"
echo -e "  ${BOLD}Agent pubkey:${RESET}  ${AGENT_PUB}"
echo -e "  ${BOLD}Agent WG IP:${RESET}   ${AGENT_WG_IP}"
echo ""
echo -e "  In the Lynx Dashboard → Agents → Add Agent → paste the pubkey above."
echo -e "  The dashboard will add this agent as a WireGuard peer to complete the tunnel."
echo ""
echo -e "${YELLOW}Note:${RESET} The agent API is only reachable via WireGuard (${DASHBOARD_WG_IP} → ${AGENT_WG_IP}:${AGENT_PORT})."
echo ""
echo -e "  ${BOLD}Made with love by Jaroc${RESET} — https://github.com/Jaro-c/Lynx"
echo ""
