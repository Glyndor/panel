#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# setup-dashboard.sh — Lynx Dashboard install / reinstall script
#
# Description:
#   Installs the Lynx Dashboard on a VPS. Sets up:
#     - Podman networks (3 isolated: db, cache, app)
#     - Podman secrets (randomly generated, no trace)
#     - PostgreSQL 18 container with isolated app user
#     - Valkey 8 container with password auth
#     - Backend (Rust) + Frontend (Next.js) containers
#     - nftables rules (ports 22 + 19443)
#     - Self-signed TLS certificate (90-day, auto-rotated via systemd timer)
#     - WireGuard tunnel to local agent
#
# Usage:
#   curl -sSL https://get.lynx.example/dashboard | bash
#   OR
#   ./setup-dashboard.sh
#
# Requirements:
#   - Debian/Ubuntu or RHEL-based Linux (amd64 / arm64)
#   - Run as root or with sudo
#   - Internet access for container images
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
CERTS_DIR="/etc/lynx/tls"
WG_DIR="/etc/wireguard"
COMPOSE_FILE="$(cd "$(dirname "${BASH_SOURCE[0]:-}")" 2>/dev/null && pwd)/docker-compose.yml"
LISTEN_PORT=19443
AGENT_WG_PORT=51820
AGENT_WG_IP="10.100.0.2"
DASHBOARD_WG_IP="10.100.0.1"
WG_SUBNET="10.100.0.0/16"
CONTAINER_RUNTIME="podman"

# --- Root check -------------------------------------------------------------

if [[ $EUID -ne 0 ]]; then
    log_error "Must run as root: sudo $0"
    exit 1
fi

# --- Cleanup function (used by reinstall) -----------------------------------

_cleanup_existing() {
    log_section "Removing existing installation"

    # Gracefully stop containers before removal so volumes are not locked
    log_info "Stopping containers..."
    for ctr in lynx-dashboard-nginx lynx-dashboard-frontend lynx-dashboard-backend lynx-dashboard-postgres lynx-dashboard-valkey; do
        podman stop --time 5 "$ctr" 2>/dev/null || true
    done
    # Catch any stray lynx-dashboard-* containers from partial prior installs
    podman ps -q --filter name=lynx-dashboard 2>/dev/null | xargs -r podman stop --time 5 2>/dev/null || true

    # Remove named containers then any remaining lynx-dashboard-* matches
    for ctr in lynx-dashboard-nginx lynx-dashboard-frontend lynx-dashboard-backend lynx-dashboard-postgres lynx-dashboard-valkey; do
        if podman container exists "$ctr" 2>/dev/null; then
            log_info "Removing container: $ctr"
            podman rm -f "$ctr" 2>/dev/null || true
        fi
    done
    podman ps -aq --filter name=lynx-dashboard 2>/dev/null | xargs -r podman rm -f 2>/dev/null || true

    # Fail explicitly if any container could not be removed — leftover containers
    # hold volumes open and leave secrets mounted, causing password mismatches on reinstall
    if podman ps -aq --filter name=lynx-dashboard 2>/dev/null | grep -q .; then
        log_error "Failed to remove all lynx-dashboard-* containers — manual cleanup required:"
        podman ps -a --format '{{.Names}}\t{{.Status}}' --filter name=lynx-dashboard 2>/dev/null
        exit 1
    fi

    # Remove volumes — project name is forced to lynx-dashboard (-p flag) so
    # postgres_data → lynx-dashboard_postgres_data, frontend_next_cache →
    # lynx-dashboard_frontend_next_cache. The extra patterns catch volumes from
    # runs before the -p flag was added (e.g. lynx-install_postgres_data).
    podman volume rm lynx-dashboard_postgres_data 2>/dev/null || true
    podman volume rm lynx-dashboard_frontend_next_cache 2>/dev/null || true
    podman volume rm dashboard_postgres_data 2>/dev/null || true
    # Broad pattern sweep for installs run from any directory name
    podman volume ls --format '{{.Name}}' 2>/dev/null \
        | grep -E 'postgres_data|frontend_next_cache' \
        | xargs -r podman volume rm 2>/dev/null || true

    # Remove networks
    for net in lynx-dashboard-db lynx-dashboard-cache lynx-dashboard-app; do
        podman network rm "$net" 2>/dev/null || true
    done

    # Remove known secrets then sweep any remaining lynx-* secrets
    for secret in lynx-dashboard-pg-root lynx-dashboard-pg-pass lynx-dashboard-redis-pass \
                  lynx-dashboard-database-url lynx-dashboard-redis-url \
                  lynx-dashboard-api-token lynx-dashboard-kek lynx-dashboard-pepper \
                  lynx-dashboard-jwt-sign-private lynx-dashboard-jwt-sign-public \
                  lynx-dashboard-jwt-enc-private lynx-dashboard-jwt-enc-public \
                  lynx-dashboard-ca-private lynx-dashboard-ca-public \
                  lynx-dashboard-setup-token \
                  lynx-dashboard-local-agent-psk; do
        podman secret rm "$secret" 2>/dev/null || true
    done
    podman secret ls --format '{{.Name}}' 2>/dev/null | grep '^lynx-' | xargs -r podman secret rm 2>/dev/null || true

    # Remove WireGuard interface
    if ip link show wg-lynx-dash &>/dev/null; then
        ip link delete wg-lynx-dash 2>/dev/null || true
    fi
    rm -f "$WG_DIR/wg-lynx-dash.conf"

    # Remove systemd units
    systemctl disable --now lynx-dashboard-containers.service 2>/dev/null || true
    systemctl disable --now lynx-dashboard-rotate-certs.timer 2>/dev/null || true
    rm -f /etc/systemd/system/lynx-dashboard-containers.service
    rm -f /etc/systemd/system/lynx-dashboard-rotate-certs.{service,timer}
    systemctl daemon-reload

    # Flush nftables table so container DNS queries are not blocked during reinstall
    nft delete table inet lynx-dashboard 2>/dev/null || true
    rm -f /etc/nftables-lynx-dashboard.conf

    rm -rf "$LYNX_DIR"
    log_ok "Cleanup complete"
}

# --- RAM check --------------------------------------------------------------

log_section "Checking system resources"

TOTAL_RAM_MB=$(free -m | awk '/^Mem:/{print $2}')
if [[ "$TOTAL_RAM_MB" -lt 512 ]]; then
    log_error "Insufficient RAM: ${TOTAL_RAM_MB} MB detected, minimum 512 MB required"
    log_info  "Lynx Dashboard requires at least 512 MB RAM for PostgreSQL to operate correctly"
    exit 1
fi
log_ok "RAM: ${TOTAL_RAM_MB} MB (minimum 512 MB satisfied)"

# --- Incompatible software --------------------------------------------------

log_section "Checking for incompatible software"

log_info "Lynx manages containers via Podman and firewall via nftables."
log_info "The following software is incompatible and will be removed if found:"
log_info "  Docker / Docker Engine, containerd (standalone), firewalld, ufw, iptables (legacy)"
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
    # Flush residual kernel rules left behind by Docker / ufw / iptables.
    # On Ubuntu 24.04+, 'iptables' is iptables-nft and flushes nftables ip/ip6
    # filter tables (the ones ufw and Docker create). Also flush iptables-legacy
    # if present (older distros or explicitly installed).
    for _ipt in iptables ip6tables iptables-legacy ip6tables-legacy; do
        if command -v "$_ipt" &>/dev/null; then
            "$_ipt" -P INPUT  ACCEPT 2>/dev/null || true
            "$_ipt" -P FORWARD ACCEPT 2>/dev/null || true
            "$_ipt" -P OUTPUT ACCEPT 2>/dev/null || true
            "$_ipt" -F              2>/dev/null || true
            "$_ipt" -X              2>/dev/null || true
            "$_ipt" -t nat    -F    2>/dev/null || true
            "$_ipt" -t nat    -X    2>/dev/null || true
            "$_ipt" -t mangle -F    2>/dev/null || true
            "$_ipt" -t mangle -X    2>/dev/null || true
        fi
    done
    # Also nuke any lingering nftables filter tables (ufw/Docker on systems where
    # iptables-nft maps to nft tables named 'filter').
    nft delete table ip  filter 2>/dev/null || true
    nft delete table ip6 filter 2>/dev/null || true
    log_ok "Incompatible software removed — residual firewall rules cleared"
else
    log_ok "No incompatible software found"
fi

unset _REASON_DOCKER _REASON_CTR _REASON_FW

# --- Detect existing installation -------------------------------------------

log_section "Checking for existing installation"

existing=false
existing_reason=""

if podman network ls --format '{{.Name}}' 2>/dev/null | grep -q '^lynx-dashboard'; then
    existing=true
    existing_reason+=" Podman networks lynx-dashboard-* found."
fi
if podman ps -a --format '{{.Names}}' 2>/dev/null | grep -q '^lynx-dashboard'; then
    existing=true
    existing_reason+=" Containers lynx-dashboard-* found."
fi
if podman secret ls --format '{{.Name}}' 2>/dev/null | grep -q '^lynx-'; then
    existing=true
    existing_reason+=" Secrets lynx-* found."
fi
if [[ -d "$LYNX_DIR" ]]; then
    existing=true
    existing_reason+=" Directory $LYNX_DIR exists."
fi

if $existing; then
    log_warn "Existing installation detected:${existing_reason}"
    echo ""
    echo -e "  ${BOLD}1)${RESET} Abort (default)"
    echo -e "  ${BOLD}2)${RESET} Update → runs auto-update instead"
    echo -e "  ${BOLD}3)${RESET} Reinstall clean → destroys all data"
    echo ""
    read -rp "Choice [1/2/3]: " choice
    choice="${choice:-1}"

    case "$choice" in
        2)
            log_info "Redirecting to auto-update..."
            exec "$(dirname "${BASH_SOURCE[0]:-}")/update-dashboard.sh"
            ;;
        3)
            echo ""
            log_warn "This will permanently destroy all Lynx data on this machine."
            read -rp "Type 'reinstall lynx-dashboard' to confirm: " confirm
            if [[ "$confirm" != "reinstall lynx-dashboard" ]]; then
                log_error "Confirmation phrase mismatch. Aborting."
                exit 1
            fi
            log_info "Proceeding with clean reinstall..."
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
    # Replace symlink (systemd-resolved stub) with a static file using a known-good resolver
    rm -f /etc/resolv.conf
    echo 'nameserver 8.8.8.8' > /etc/resolv.conf
    # Final check
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

# Wait up to 60s for any running apt/dpkg process to finish, then clear stale locks.
_wait_apt_lock() {
    local _deadline=$(( $(date +%s) + 60 ))
    while fuser /var/lib/apt/lists/lock /var/lib/dpkg/lock-frontend /var/lib/dpkg/lock 2>/dev/null; do
        if [[ $(date +%s) -ge $_deadline ]]; then
            log_warn "apt lock held for 60s — force-clearing stale lock files"
            rm -f /var/lib/apt/lists/lock /var/lib/dpkg/lock-frontend /var/lib/dpkg/lock
            break
        fi
        sleep 2
    done
}

_apt_updated=false
_apt_ensure() {
    local cmd="$1" pkg="$2"
    if command -v "$cmd" &>/dev/null; then
        log_ok "$cmd found"
        return
    fi
    log_info "Installing $pkg..."
    _wait_apt_lock
    if ! $_apt_updated; then
        # Enable universe repo (needed for podman on Ubuntu)
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

_apt_ensure podman         podman
_apt_ensure podman-compose podman-compose
_apt_ensure openssl        openssl
_apt_ensure nft            nftables
_apt_ensure wg             wireguard-tools
_apt_ensure curl           curl
_apt_ensure python3        python3
_apt_ensure pip3           python3-pip
_require_cmd systemctl "systemd required"
_require_cmd free      "procps required"

# Netavark + aardvark-dns: required for container-to-container DNS resolution.
# Podman defaults to the older CNI backend which lacks DNS on Ubuntu 24.04.
# These packages live under /usr/lib/podman/ — not in PATH, so _apt_ensure can't check by command.
_netavark_ok=true
for _pkg in netavark aardvark-dns; do
    if ! dpkg -l "$_pkg" 2>/dev/null | grep -q '^ii'; then
        log_info "Installing $_pkg..."
        if ! $_apt_updated; then
            DEBIAN_FRONTEND=noninteractive apt-get update -qq
            _apt_updated=true
        fi
        DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "$_pkg" -qq
        if dpkg -l "$_pkg" 2>/dev/null | grep -q '^ii'; then
            log_ok "$_pkg installed"
        else
            log_error "Failed to install $_pkg"
            exit 1
        fi
    else
        log_ok "$_pkg found"
    fi
done

# Configure Podman to use Netavark (enables container DNS via aardvark-dns)
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

# --- Create directories -----------------------------------------------------

log_section "Creating directories"

NGINX_DIR="$LYNX_DIR/nginx"
mkdir -p "$LYNX_DIR" "$CERTS_DIR" "$WG_DIR" "$NGINX_DIR"
chmod 700 "$LYNX_DIR" "$CERTS_DIR" "$WG_DIR"
chmod 755 "$NGINX_DIR"
log_ok "Directories created"

# --- Podman networks --------------------------------------------------------

log_section "Creating Podman networks"

for net in lynx-dashboard-db lynx-dashboard-cache lynx-dashboard-app; do
    if podman network exists "$net" 2>/dev/null; then
        log_warn "Network $net already exists — skipping"
    else
        podman network create "$net"
        log_ok "Network created: $net"
    fi
done

# --- Generate secrets -------------------------------------------------------
#
# Secrets flow directly via pipe — never stored in files or shell history.
# Subshells ensure vars don't leak to parent environment.
# Passwords are overwritten in memory before the subshell exits.

log_section "Generating secrets"

log_info "Generating PostgreSQL root password..."
(
    PG_ROOT=$(openssl rand -hex 32)
    printf '%s' "$PG_ROOT" | podman secret create lynx-dashboard-pg-root - >/dev/null
    PG_ROOT="$(openssl rand -hex 32)"  # overwrite
)

log_info "Generating PostgreSQL app password and database URL..."
(
    PG_PASS=$(openssl rand -hex 32)
    printf '%s' "$PG_PASS" | podman secret create lynx-dashboard-pg-pass - >/dev/null
    printf 'postgresql://lynx_dashboard_app:%s@lynx-dashboard-postgres:5432/lynx_dashboard' "$PG_PASS" \
        | podman secret create lynx-dashboard-database-url - >/dev/null
    PG_PASS="$(openssl rand -hex 32)"  # overwrite
)

log_info "Generating Valkey password and URL..."
(
    REDIS_PASS=$(openssl rand -hex 32)
    printf '%s' "$REDIS_PASS" | podman secret create lynx-dashboard-redis-pass - >/dev/null
    printf 'redis://:%s@lynx-dashboard-valkey:6379' "$REDIS_PASS" \
        | podman secret create lynx-dashboard-redis-url - >/dev/null
    REDIS_PASS="$(openssl rand -hex 32)"  # overwrite
)

log_info "Generating API token..."
openssl rand -hex 32 | podman secret create lynx-dashboard-api-token - >/dev/null
log_info "Generating KEK (Key Encryption Key)..."
openssl rand -base64 32 | tr -d '\n' | podman secret create lynx-dashboard-kek - >/dev/null
log_info "Generating pepper..."
openssl rand -hex 32 | podman secret create lynx-dashboard-pepper - >/dev/null
log_info "Generating JWT signing keypair (Ed25519)..."
(
    PRIV_PEM=$(openssl genpkey -algorithm ed25519 2>/dev/null)
    PRIV_SEED=$(printf '%s' "$PRIV_PEM" | openssl pkey -outform DER 2>/dev/null | tail -c 32 | base64 -w0)
    PUB_BYTES=$(printf '%s' "$PRIV_PEM" | openssl pkey -pubout -outform DER 2>/dev/null | tail -c 32 | base64 -w0)
    printf '%s' "$PRIV_SEED" | podman secret create lynx-dashboard-jwt-sign-private - >/dev/null
    printf '%s' "$PUB_BYTES" | podman secret create lynx-dashboard-jwt-sign-public - >/dev/null
)

log_info "Generating JWT encryption keypair (X25519)..."
(
    PRIV_PEM=$(openssl genpkey -algorithm x25519 2>/dev/null)
    PRIV_BYTES=$(printf '%s' "$PRIV_PEM" | openssl pkey -outform DER 2>/dev/null | tail -c 32 | base64 -w0)
    PUB_BYTES=$(printf '%s' "$PRIV_PEM" | openssl pkey -pubout -outform DER 2>/dev/null | tail -c 32 | base64 -w0)
    printf '%s' "$PRIV_BYTES" | podman secret create lynx-dashboard-jwt-enc-private - >/dev/null
    printf '%s' "$PUB_BYTES" | podman secret create lynx-dashboard-jwt-enc-public - >/dev/null
)

log_info "Generating CA keypair (Ed25519)..."
(
    PRIV_PEM=$(openssl genpkey -algorithm ed25519 2>/dev/null)
    PRIV_SEED=$(printf '%s' "$PRIV_PEM" | openssl pkey -outform DER 2>/dev/null | tail -c 32 | base64 -w0)
    PUB_BYTES=$(printf '%s' "$PRIV_PEM" | openssl pkey -pubout -outform DER 2>/dev/null | tail -c 32 | base64 -w0)
    printf '%s' "$PRIV_SEED" | podman secret create lynx-dashboard-ca-private - >/dev/null
    printf '%s' "$PUB_BYTES" | podman secret create lynx-dashboard-ca-public - >/dev/null
)

log_info "Generating setup token (one-time bootstrap)..."
SETUP_TOKEN=$(openssl rand -hex 32)
printf '%s' "$SETUP_TOKEN" | podman secret create lynx-dashboard-setup-token - >/dev/null
log_ok "All secrets generated — values purged from memory"

# --- Download binaries from GitHub Releases ---------------------------------
#
# Binaries are signed with Ed25519. Public key is hardcoded in each binary
# and verified here during install. The private key lives only in GitHub
# Actions secrets — never in the repo.

log_section "Downloading dashboard binaries"

GITHUB_REPO="Jaro-c/Lynx"
RELEASE_VERIFY_KEY_B64="OsBV4t+vQSn10FAI8UzAJEBS0IUqp8D2bZtlQYD8j+Q="

# Detect architecture
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

# Fetch latest dashboard release tag
log_info "Fetching latest dashboard release..."
LATEST_TAG=$(curl -fsSL \
    "https://api.github.com/repos/${GITHUB_REPO}/releases" \
    | python3 -c "
import sys, json
releases = json.load(sys.stdin)
for r in releases:
    tag = r.get('tag_name', '')
    if tag.startswith('dashboard@') and not r.get('prerelease'):
        print(tag)
        break
" 2>/dev/null)

if [[ -z "$LATEST_TAG" ]]; then
    log_error "No dashboard release found in ${GITHUB_REPO}"
    exit 1
fi
log_ok "Latest release: ${LATEST_TAG}"

RELEASE_BASE="https://github.com/${GITHUB_REPO}/releases/download/${LATEST_TAG}"
BIN_DIR="/etc/lynx/bin"
FRONTEND_DIR="/etc/lynx/frontend"

mkdir -p "$BIN_DIR" "$FRONTEND_DIR"
chmod 700 "$BIN_DIR" "$FRONTEND_DIR"

# Verify Ed25519 signature. Args: <file> <sig-file>
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

# Ensure cryptography lib is available for signature verification
if ! python3 -c "from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey" 2>/dev/null; then
    log_info "Installing Python cryptography library..."
    pip3 install --quiet cryptography
fi

# Download and verify backend binary
log_info "Downloading backend binary..."
BACKEND_FILE="${BIN_DIR}/lynx-dashboard-backend"
BACKEND_TMP="${BIN_DIR}/lynx-dashboard-backend.new"

curl -fsSL --max-time 300 \
    "${RELEASE_BASE}/lynx-dashboard-backend-linux-${ARCH}" \
    -o "$BACKEND_TMP"
curl -fsSL --max-time 30 \
    "${RELEASE_BASE}/lynx-dashboard-backend-linux-${ARCH}.sig" \
    -o "${BACKEND_TMP}.sig"

log_info "Verifying backend signature..."
if ! _verify_release_sig "$BACKEND_TMP" "${BACKEND_TMP}.sig"; then
    log_error "Backend signature verification FAILED — aborting"
    rm -f "$BACKEND_TMP" "${BACKEND_TMP}.sig"
    exit 1
fi
rm -f "${BACKEND_TMP}.sig"
chmod 755 "$BACKEND_TMP"
mv "$BACKEND_TMP" "$BACKEND_FILE"
log_ok "Backend installed: ${BACKEND_FILE}"

# Download and verify frontend binary + assets
log_info "Downloading frontend binary..."
FRONTEND_BIN_TMP="${FRONTEND_DIR}/lynx-dashboard-frontend.new"
FRONTEND_ASSETS_TMP="${FRONTEND_DIR}/assets.new.tar.gz"

curl -fsSL --max-time 300 \
    "${RELEASE_BASE}/lynx-dashboard-frontend-linux-${ARCH}" \
    -o "$FRONTEND_BIN_TMP"
curl -fsSL --max-time 30 \
    "${RELEASE_BASE}/lynx-dashboard-frontend-linux-${ARCH}.sig" \
    -o "${FRONTEND_BIN_TMP}.sig"

log_info "Verifying frontend binary signature..."
if ! _verify_release_sig "$FRONTEND_BIN_TMP" "${FRONTEND_BIN_TMP}.sig"; then
    log_error "Frontend binary signature verification FAILED — aborting"
    rm -f "$FRONTEND_BIN_TMP" "${FRONTEND_BIN_TMP}.sig"
    exit 1
fi
rm -f "${FRONTEND_BIN_TMP}.sig"
chmod 755 "$FRONTEND_BIN_TMP"

log_info "Downloading frontend assets..."
curl -fsSL --max-time 300 \
    "${RELEASE_BASE}/lynx-dashboard-frontend-assets-linux-${ARCH}.tar.gz" \
    -o "$FRONTEND_ASSETS_TMP"
curl -fsSL --max-time 30 \
    "${RELEASE_BASE}/lynx-dashboard-frontend-assets-linux-${ARCH}.tar.gz.sig" \
    -o "${FRONTEND_ASSETS_TMP}.sig"

log_info "Verifying frontend assets signature..."
if ! _verify_release_sig "$FRONTEND_ASSETS_TMP" "${FRONTEND_ASSETS_TMP}.sig"; then
    log_error "Frontend assets signature verification FAILED — aborting"
    rm -f "$FRONTEND_BIN_TMP" "$FRONTEND_ASSETS_TMP" "${FRONTEND_ASSETS_TMP}.sig"
    exit 1
fi
rm -f "${FRONTEND_ASSETS_TMP}.sig"

# Place frontend binary and extract assets into FRONTEND_DIR
# Binary runs from FRONTEND_DIR so __dirname resolves static assets correctly
mv "$FRONTEND_BIN_TMP" "${FRONTEND_DIR}/lynx-dashboard-frontend"
tar -xzf "$FRONTEND_ASSETS_TMP" -C "$FRONTEND_DIR"
rm -f "$FRONTEND_ASSETS_TMP"

log_ok "Frontend installed: ${FRONTEND_DIR}/"

# Write version file
printf '%s' "${LATEST_TAG#dashboard@}" > "$BIN_DIR/lynx-dashboard-version"
log_ok "Version: ${LATEST_TAG#dashboard@}"

# --- Start services ---------------------------------------------------------

log_section "Starting services"

COMPOSE_DIR="$(dirname "$COMPOSE_FILE")"

# Copy init SQL to a persistent location so the bind mount survives reboots.
# docker-compose.yml uses ${LYNX_DB_INIT_DIR:-./server/db/init} — this sets
# the production path; local dev falls back to the relative repo path.
LYNX_DB_INIT_DIR="/etc/lynx/db/init"
mkdir -p "$LYNX_DB_INIT_DIR"
chmod 755 "/etc/lynx/db" "$LYNX_DB_INIT_DIR"
cp "$COMPOSE_DIR/server/db/init/"*.sql "$LYNX_DB_INIT_DIR/"
chmod 644 "$LYNX_DB_INIT_DIR/"*.sql
export LYNX_DB_INIT_DIR
log_ok "Init SQL copied to $LYNX_DB_INIT_DIR"

# 1. PostgreSQL
log_info "Starting PostgreSQL..."
podman-compose -p lynx-dashboard -f "$COMPOSE_FILE" up -d postgres

log_info "Waiting for PostgreSQL to be healthy..."
for i in $(seq 1 30); do
    if podman healthcheck run lynx-dashboard-postgres 2>/dev/null | grep -q healthy ||
       podman inspect lynx-dashboard-postgres --format '{{.State.Health.Status}}' 2>/dev/null | grep -q healthy; then
        log_ok "PostgreSQL is healthy"
        break
    fi
    if [[ $i -eq 30 ]]; then
        log_error "PostgreSQL did not become healthy in time"
        exit 1
    fi
    sleep 2
done

# 2. Valkey
log_info "Starting Valkey..."
podman-compose -p lynx-dashboard -f "$COMPOSE_FILE" up -d valkey

log_info "Waiting for Valkey to be healthy..."
for i in $(seq 1 30); do
    if podman inspect lynx-dashboard-valkey --format '{{.State.Health.Status}}' 2>/dev/null | grep -q healthy; then
        log_ok "Valkey is healthy"
        break
    fi
    if [[ $i -eq 30 ]]; then
        log_error "Valkey did not become healthy in time"
        exit 1
    fi
    sleep 2
done

# 3. Backend
log_info "Starting backend..."
podman-compose -p lynx-dashboard -f "$COMPOSE_FILE" up -d backend

log_info "Waiting for backend to be healthy..."
for i in $(seq 1 40); do
    if podman inspect lynx-dashboard-backend --format '{{.State.Health.Status}}' 2>/dev/null | grep -q healthy; then
        log_ok "Backend is healthy"
        break
    fi
    if [[ $i -eq 40 ]]; then
        log_error "Backend did not become healthy in time"
        podman logs --tail 50 lynx-dashboard-backend
        exit 1
    fi
    sleep 3
done

# Resync PostgreSQL app user password to match the Podman secret.
# The backend may run a 90-day scheduled rotation on first startup (no prior
# rotation record in a fresh DB) and partially update the PostgreSQL password
# without updating the Podman secret (the `podman` binary is not available
# inside the Alpine container). The rotation runs ~30s after startup; we wait
# 15s after "healthy" to ensure the rotation has finished before we re-anchor
# PostgreSQL to the value in the Podman secret.
log_info "Waiting for any startup key rotation to settle..."
sleep 15
log_info "Synchronizing PostgreSQL app user password..."
_PG_PASS_SYNC=$(podman secret inspect lynx-dashboard-pg-pass --showsecret --format '{{.SecretData}}' 2>/dev/null)
if [[ -n "$_PG_PASS_SYNC" ]]; then
    printf "ALTER USER lynx_dashboard_app PASSWORD '%s';\n" "$_PG_PASS_SYNC" \
        | podman exec -i lynx-dashboard-postgres psql -U postgres -d lynx_dashboard \
        >/dev/null 2>&1 \
        && log_ok "PostgreSQL app user password synchronized" \
        || log_warn "Could not sync PostgreSQL password (non-critical — backend may reconnect)"
fi
unset _PG_PASS_SYNC

# 4. Frontend
log_info "Starting frontend..."
podman-compose -p lynx-dashboard -f "$COMPOSE_FILE" up -d frontend

log_info "Waiting for frontend to be healthy..."
for i in $(seq 1 40); do
    if podman inspect lynx-dashboard-frontend --format '{{.State.Health.Status}}' 2>/dev/null | grep -q healthy; then
        log_ok "Frontend is healthy"
        break
    fi
    if [[ $i -eq 40 ]]; then
        log_error "Frontend did not become healthy in time"
        podman logs --tail 30 lynx-dashboard-frontend
        exit 1
    fi
    sleep 3
done

# --- TLS certificate --------------------------------------------------------

log_section "Generating TLS certificate"

CERT="$CERTS_DIR/dashboard.crt"
KEY="$CERTS_DIR/dashboard.key"

_generate_cert() {
    local cn
    cn=$(hostname -f 2>/dev/null || hostname)
    openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
        -keyout "$KEY" -out "$CERT" \
        -days 90 -nodes -sha256 \
        -subj "/CN=${cn}/O=Lynx/OU=Dashboard" \
        -addext "subjectAltName=DNS:${cn},IP:$(hostname -I | awk '{print $1}')" \
        2>/dev/null
    chmod 600 "$KEY"
    chmod 644 "$CERT"
    log_ok "Certificate generated: $CERT (90 days, P-256)"
}

_generate_cert

# Systemd timer for certificate rotation (90-day renewal)
cat > /etc/systemd/system/lynx-dashboard-rotate-certs.service << 'EOF'
[Unit]
Description=Lynx Dashboard — rotate TLS certificate
After=network.target

[Service]
Type=oneshot
ExecStart=/bin/bash -c 'openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
    -keyout /etc/lynx/tls/dashboard.key -out /etc/lynx/tls/dashboard.crt \
    -days 90 -nodes -sha256 \
    -subj "/CN=$(hostname -f)/O=Lynx/OU=Dashboard" \
    -addext "subjectAltName=DNS:$(hostname -f),IP:$(hostname -I | awk '"'"'{print $1}'"'"')" \
    && chmod 600 /etc/lynx/tls/dashboard.key \
    && podman kill -s HUP lynx-dashboard-nginx'
EOF

cat > /etc/systemd/system/lynx-dashboard-rotate-certs.timer << 'EOF'
[Unit]
Description=Lynx Dashboard — TLS certificate rotation (every 80 days)

[Timer]
OnCalendar=*-*-* 03:00:00
Persistent=true
AccuracySec=1h
RandomizedDelaySec=1h
OnBootSec=10min

[Install]
WantedBy=timers.target
EOF

systemctl daemon-reload
systemctl enable --now lynx-dashboard-rotate-certs.timer
log_ok "Certificate rotation timer enabled (every 80 days)"

# --- nginx TLS reverse proxy ------------------------------------------------

log_section "Configuring nginx TLS reverse proxy"

# nginx config: TLS termination on 19443, proxy to frontend.
# The resolver directive points at Podman's embedded DNS (the lynx-dashboard-app
# network gateway, always the .1 of whichever subnet Netavark assigns).  Using a
# variable for proxy_pass forces nginx to re-resolve the "frontend" hostname on
# every request so it picks up the new container IP after an auto-update restart.
NGINX_RESOLVER=$(podman network inspect lynx-dashboard-app \
    --format '{{range .Subnets}}{{.Gateway}}{{end}}' 2>/dev/null \
    || echo "10.89.0.1")
cat > "$NGINX_DIR/default.conf" << NGINXEOF
server {
    listen 19443 ssl;
    listen [::]:19443 ssl;

    ssl_certificate     /etc/lynx/tls/dashboard.crt;
    ssl_certificate_key /etc/lynx/tls/dashboard.key;
    ssl_protocols       TLSv1.3;
    ssl_prefer_server_ciphers off;

    # Re-resolve the frontend hostname after each update-triggered restart.
    resolver ${NGINX_RESOLVER} valid=5s ipv6=off;

    location / {
        set \$upstream http://frontend:3000;
        proxy_pass         \$upstream;
        proxy_http_version 1.1;
        proxy_set_header   Upgrade \$http_upgrade;
        proxy_set_header   Connection 'upgrade';
        proxy_set_header   Host \$host;
        proxy_set_header   X-Real-IP \$remote_addr;
        proxy_set_header   X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto https;
        proxy_cache_bypass \$http_upgrade;
        proxy_read_timeout 120s;
    }

    error_page 502 503 /updating.html;
    location = /updating.html {
        root /etc/lynx/nginx;
    }
}
NGINXEOF

# Maintenance page served by nginx while frontend is being updated
cat > "$NGINX_DIR/updating.html" << 'EOF'
<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>Lynx — Updating</title>
<style>body{font-family:sans-serif;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0;background:#0f172a;color:#e2e8f0}
.box{text-align:center;padding:2rem}h1{font-size:1.5rem;margin-bottom:.5rem}p{color:#94a3b8}</style>
</head>
<body><div class="box"><h1>Lynx is updating</h1><p>The dashboard will be back shortly.</p></div></body>
</html>
EOF

log_ok "nginx configuration written"

# 5. nginx
# Remove any stale nginx container created earlier by podman-compose (it may have been
# created before nginx.conf/certs existed, or have stale --requires pointing to
# since-recreated container IDs). Start it fresh directly to bypass the dependency graph.
log_info "Starting nginx..."
podman rm -f lynx-dashboard-nginx 2>/dev/null || true
podman run -d \
    --name lynx-dashboard-nginx \
    --network lynx-dashboard-app \
    -p "19443:19443" \
    -v /etc/lynx/tls:/etc/lynx/tls:ro \
    -v /etc/lynx/nginx/default.conf:/etc/nginx/conf.d/default.conf:ro \
    -v /etc/lynx/nginx/updating.html:/etc/lynx/nginx/updating.html:ro \
    --restart unless-stopped \
    --health-cmd "pgrep nginx > /dev/null" \
    --health-interval 10s \
    --health-timeout 5s \
    --health-retries 5 \
    --health-start-period 10s \
    docker.io/library/nginx@sha256:65645c7bb6a0661892a8b03b89d0743208a18dd2f3f17a54ef4b76fb8e2f2a10

log_info "Waiting for nginx to be healthy..."
for i in $(seq 1 20); do
    if podman inspect lynx-dashboard-nginx --format '{{.State.Health.Status}}' 2>/dev/null | grep -q healthy; then
        log_ok "nginx is healthy"
        break
    fi
    if [[ $i -eq 20 ]]; then
        log_error "nginx did not become healthy in time"
        podman logs --tail 20 lynx-dashboard-nginx
        exit 1
    fi
    sleep 3
done

# --- WireGuard — local agent tunnel -----------------------------------------

log_section "Setting up WireGuard tunnel (dashboard ↔ local agent)"

WG_CONF="$WG_DIR/wg-lynx-dash.conf"

# Generate dashboard keypair + PSK
DASHBOARD_PRIV=$(wg genkey)
DASHBOARD_PUB=$(printf '%s' "$DASHBOARD_PRIV" | wg pubkey)
# PSK is kept in scope until final output so admin can copy it for the agent install script.
# It is also stored as a Podman secret for the dashboard backend (peer management).
AGENT_PSK=$(wg genpsk)
podman secret rm lynx-dashboard-local-agent-psk 2>/dev/null || true
printf '%s' "$AGENT_PSK" | podman secret create lynx-dashboard-local-agent-psk - >/dev/null
# The local agent keypair is generated by the agent install script.
# Peer block is written by the agent install script after bootstrap.
cat > "$WG_CONF" << EOF
[Interface]
PrivateKey = ${DASHBOARD_PRIV}
Address = ${DASHBOARD_WG_IP}/16
ListenPort = ${AGENT_WG_PORT}

# Peer block added by agent install script after bootstrap:
# [Peer]
# PublicKey = <agent pubkey>
# PresharedKey = ${AGENT_PSK}
# AllowedIPs = ${AGENT_WG_IP}/32
EOF

chmod 600 "$WG_CONF"
DASHBOARD_PRIV="$(openssl rand -hex 32)"  # overwrite in memory

log_ok "WireGuard config written: $WG_CONF"
log_ok "Dashboard WireGuard pubkey: ${DASHBOARD_PUB}"
printf '%s' "$DASHBOARD_PUB" > "$LYNX_DIR/dashboard-wg-pubkey"

# --- nftables ---------------------------------------------------------------

log_section "Configuring nftables"

# Bootstrap ruleset uses the same table/chain names as the Rust agent binary so
# nftables.service loads correct rules on every reboot.  The agent binary
# overwrites this file on first startup — this is only the boot-window ruleset
# that runs before lynx-agent.service has started.
cat > /etc/nftables-lynx-agent.conf << 'EOF'
destroy table inet lynx-agent
add table inet lynx-agent
table inet lynx-agent {
    chain lynx-base {
        type filter hook input priority 0; policy drop;

        iif lo accept
        ct state invalid drop
        ct state established,related accept

        # ICMP — path MTU, diagnostics, reachability
        ip  protocol icmp  accept
        ip6 nexthdr  icmpv6 accept

        # TCP flag anomalies
        tcp flags == 0x0 drop
        tcp flags & (fin | psh | urg) == fin | psh | urg drop
        tcp flags ack ct state new drop

        # DNS for container networks (aardvark-dns on Netavark bridge interfaces)
        iifname "podman*" udp dport 53 accept
        iifname "podman*" tcp dport 53 accept

        # SSH — per-source-IP rate limit
        tcp dport 22 ct state new meter ssh_throttle { ip saddr limit rate 10/minute burst 20 packets } accept

        # Dashboard panel
        tcp dport 19443 ct state new accept

        # WireGuard (agent tunnels)
        udp dport 51820 accept

        # Dashboard backend accessible from WireGuard management plane only
        ip saddr 10.100.0.1 accept

        jump lynx-global
        jump lynx-local

        drop
    }

    # These chains are populated by the Rust agent after startup
    chain lynx-global {}
    chain lynx-local {}

    chain lynx-forward {
        type filter hook forward priority 0; policy drop;

        ct state established,related accept

        # New connections to published container ports (Netavark DNAT rewrites dst to 10.89.x.x)
        ip daddr 10.89.0.0/16 ct state new accept

        # Outbound traffic from dashboard containers (apk installs, GitHub, cert renewals, etc.)
        iifname "podman*" accept

        # Backend container traffic to/from WireGuard (dashboard <-> agents)
        oifname "wg-lynx-dash" accept
        iifname "wg-lynx-dash" accept
    }

    chain lynx-output {
        type filter hook output priority 0; policy accept;
    }
}
EOF

# Apply bootstrap ruleset
nft -f /etc/nftables-lynx-agent.conf
log_ok "nftables rules applied (ports: 22 rate-limited, 19443, 51820 UDP)"

# Persist across reboots — migrate away from old lynx-dashboard include
if [[ -f /etc/nftables.conf ]]; then
    sed -i '/nftables-lynx-dashboard/d' /etc/nftables.conf
    if ! grep -q "nftables-lynx-agent" /etc/nftables.conf; then
        echo 'include "/etc/nftables-lynx-agent.conf"' >> /etc/nftables.conf
    fi
fi
systemctl enable nftables 2>/dev/null || true

# --- Container auto-start on reboot -----------------------------------------

log_section "Enabling container auto-start on reboot"

# Podman's podman-restart.service only handles restart-policy=always.
# Our containers use restart=unless-stopped (don't restart if manually stopped).
# This oneshot service starts all five containers at boot, after nftables are loaded.
cat > /etc/systemd/system/lynx-dashboard-containers.service << 'EOF'
[Unit]
Description=Lynx Dashboard — start containers on boot
After=network-online.target nftables.service lynx-agent.service
Wants=network-online.target lynx-agent.service

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/bin/podman start \
    lynx-dashboard-postgres \
    lynx-dashboard-valkey \
    lynx-dashboard-backend \
    lynx-dashboard-frontend \
    lynx-dashboard-nginx
ExecStop=/usr/bin/podman stop \
    lynx-dashboard-nginx \
    lynx-dashboard-frontend \
    lynx-dashboard-backend \
    lynx-dashboard-valkey \
    lynx-dashboard-postgres

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable lynx-dashboard-containers.service
log_ok "Container auto-start service enabled (lynx-dashboard-containers.service)"

# --- Done -------------------------------------------------------------------

log_section "Installation complete"

HOST_IP=$(hostname -I | awk '{print $1}')
CERT_EXPIRY=$(openssl x509 -in "$CERT" -noout -enddate | cut -d= -f2)

echo ""
echo -e "${GREEN}${BOLD}Lynx Dashboard is running!${RESET}"
echo ""
echo -e "  ${BOLD}URL:${RESET}               ${CYAN}https://${HOST_IP}:${LISTEN_PORT}${RESET}"
echo -e "  ${BOLD}Cert expires:${RESET}      ${CERT_EXPIRY}"
echo ""
echo -e "${BOLD}${YELLOW}=== Create your admin account ===${RESET}"
echo -e "  Open this URL in your browser ${BOLD}(one-time use, expires in 24 hours):${RESET}"
echo -e "  ${CYAN}https://${HOST_IP}:${LISTEN_PORT}/register?setup_token=${SETUP_TOKEN}${RESET}"
echo -e "${YELLOW}This is the only time the setup token is shown. Save the link now.${RESET}"
echo ""
SETUP_TOKEN="$(openssl rand -hex 32)"  # overwrite in memory
unset SETUP_TOKEN
echo ""
echo -e "${BOLD}${YELLOW}=== WireGuard bootstrap data (copy for agent install) ===${RESET}"
echo -e "  ${BOLD}Dashboard endpoint:${RESET}  ${HOST_IP}:${AGENT_WG_PORT}"
echo -e "  ${BOLD}Dashboard pubkey:${RESET}    ${DASHBOARD_PUB}"
echo -e "  ${BOLD}Preshared key:${RESET}       ${AGENT_PSK}"
echo -e "${YELLOW}This is the only time the PSK is shown. Copy it now.${RESET}"
echo ""
# Clear PSK from memory after display
AGENT_PSK="$(openssl rand -hex 32)"
unset AGENT_PSK DASHBOARD_PUB
echo -e "${YELLOW}Next step:${RESET} Run the agent install script on this VPS to complete the local WireGuard tunnel."
echo ""
echo -e "  ${BOLD}Made with love by Jaroc${RESET} — https://github.com/Jaro-c/Lynx"
echo ""
