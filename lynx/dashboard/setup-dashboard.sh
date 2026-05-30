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
COMPOSE_FILE="/etc/lynx/docker-compose.yml"
LISTEN_PORT=19443
AGENT_WG_PORT=51820
AGENT_WG_IP="10.100.0.2"
DASHBOARD_WG_IP="10.100.0.1"

# Podman network subnets — fixed to prevent stale DNAT when containers restart.
# Container static IPs are hardcoded in the compose YAML below (.1 = gateway, .2+ = containers):
#   lynx-dashboard-db    10.89.0.0/24  postgres=10.89.0.2  backend=10.89.0.3
#   lynx-dashboard-cache 10.89.1.0/24  valkey=10.89.1.2    backend=10.89.1.3
#   lynx-dashboard-app   10.89.2.0/24  backend=10.89.2.2   frontend=10.89.2.3  nginx=10.89.2.4
DASHBOARD_DB_SUBNET="10.89.0.0/24"
DASHBOARD_CACHE_SUBNET="10.89.1.0/24"
DASHBOARD_APP_SUBNET="10.89.2.0/24"

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

    # Remove networks. Also purge stale aardvark-dns config files so the
    # next network create starts with clean DNS state (stale files cause the
    # DNS gateway to reference the old subnet, breaking hostname resolution).
    for net in lynx-dashboard-db lynx-dashboard-cache lynx-dashboard-app; do
        podman network rm "$net" 2>/dev/null || true
        rm -f "/run/containers/networks/aardvark-dns/$net" 2>/dev/null || true
    done

    # Remove known secrets then sweep any remaining lynx-* secrets
    for secret in lynx-dashboard-pg-root lynx-dashboard-pg-pass lynx-dashboard-redis-pass \
                  lynx-dashboard-database-url lynx-dashboard-redis-url \
                  lynx-dashboard-api-token lynx-dashboard-kek lynx-dashboard-pepper \
                  lynx-dashboard-jwt-sign-private lynx-dashboard-jwt-sign-public \
                  lynx-dashboard-jwt-enc-private lynx-dashboard-jwt-enc-public \
                  lynx-dashboard-ca-private lynx-dashboard-ca-public \
                  lynx-dashboard-x509-ca-cert lynx-dashboard-x509-ca-key \
                  lynx-dashboard-setup-token \
                  lynx-dashboard-local-agent-psk; do
        podman secret rm "$secret" 2>/dev/null || true
    done
    podman secret ls --format '{{.Name}}' 2>/dev/null | grep '^lynx-' | xargs -r podman secret rm 2>/dev/null || true
    rm -rf /etc/lynx/secrets

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

    # Flush nftables tables managed by the dashboard install
    nft delete table inet lynx-dashboard 2>/dev/null || true
    rm -f /etc/nftables-lynx-dashboard.conf
    nft delete table inet lynx-agent 2>/dev/null || true

    rm -rf "$LYNX_DIR"
    log_ok "Cleanup complete"
}

# --- Stop stray containers from partial installs ----------------------------
# Run unconditionally so a manually-cleaned /etc/lynx does not leave containers
# running that would hold volumes open and block removal.
for _ctr in lynx-dashboard-postgres lynx-dashboard-valkey lynx-dashboard-backend \
             lynx-dashboard-frontend lynx-dashboard-nginx; do
    podman stop --time 5 "$_ctr" 2>/dev/null || true
    podman rm -f "$_ctr" 2>/dev/null || true
done
# Purge stale aardvark-dns config files for dashboard networks. When containers
# are force-removed, aardvark-dns sometimes retains phantom entries that cause
# the next network create to use a stale gateway IP, breaking DNS resolution.
for _net in lynx-dashboard-db lynx-dashboard-cache lynx-dashboard-app; do
    rm -f "/run/containers/networks/aardvark-dns/$_net" 2>/dev/null || true
done
unset _net

# --- RAM check --------------------------------------------------------------

log_section "Checking system resources"

TOTAL_RAM_MB=$(free -m | awk '/^Mem:/{print $2}')
if [[ "$TOTAL_RAM_MB" -lt 512 ]]; then
    log_error "Insufficient RAM: ${TOTAL_RAM_MB} MB detected, minimum 512 MB required"
    log_info  "Lynx Dashboard requires at least 512 MB RAM for PostgreSQL to operate correctly"
    exit 1
fi
log_ok "RAM: ${TOTAL_RAM_MB} MB (minimum 512 MB satisfied)"

# Disk pre-check (§1.4) — PostgreSQL container, Podman images and the agent
# binaries together easily exceed 2 GB; bail out early instead of failing mid-
# install when a `pull` or `cp` exhausts the volume.
FREE_DISK_MB=$(df -BM --output=avail / 2>/dev/null | tail -1 | tr -dc '0-9')
if [[ -z "$FREE_DISK_MB" ]] || [[ "$FREE_DISK_MB" -lt 2048 ]]; then
    log_error "Insufficient disk: ${FREE_DISK_MB:-0} MB free on /, minimum 2048 MB required"
    log_info  "Free up space (e.g. \`podman system prune -a\`) and re-run."
    exit 1
fi
log_ok "Disk:  ${FREE_DISK_MB} MB free on / (minimum 2048 MB satisfied)"

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

# iptables package must NOT be removed — netavark 1.15.2 still calls the iptables
# binary internally even when firewall_driver = nftables is configured. On Ubuntu
# 24.04+ the 'iptables' package is actually iptables-nft which routes all calls
# through nftables; no legacy kernel module is involved. What is incompatible is
# software that *manages* iptables rules (Docker, ufw, firewalld), not the binary.

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
# podman-compose replaced by `lynx-compose` (Rust binary shipped with the release),
# which removes the python3 / pip3 runtime dependency entirely.
# openssl replaced by `lynx-dashboard-backend` subcommands for random/keypair/cert ops.
_apt_ensure nft            nftables
_apt_ensure wg             wireguard-tools
_apt_ensure curl           curl
_apt_ensure python3        python3
# python3-cryptography provides the Ed25519 signature verification needed for
# binary downloads. Once binaries are installed all crypto runs in Rust; this is
# only the bootstrap dependency. Use the apt-shipped package rather than pip3
# so the host never needs python3-pip.
_require_cmd systemctl "systemd required"
_require_cmd free      "procps required"

# Netavark + aardvark-dns: required for container-to-container DNS resolution.
# Podman defaults to the older CNI backend which lacks DNS on Ubuntu 24.04.
# These packages live under /usr/lib/podman/ — not in PATH, so _apt_ensure can't check by command.
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

# Netavark 1.10+ supports a native nftables firewall driver — older versions
# fall back to iptables-nft. Lynx requires the nftables driver so the iptables
# package can be dropped entirely (it remains in the incompatible-software list).
# Upgrade netavark from upstream when the distro ships a version < 1.10.
NETAVARK_REQUIRED="1.10.0"
NETAVARK_UPSTREAM_VER="1.15.2"
_netavark_bin=""
for _candidate in /usr/lib/podman/netavark /usr/libexec/podman/netavark; do
    if [[ -x "$_candidate" ]]; then
        _netavark_bin="$_candidate"
        break
    fi
done

if [[ -z "$_netavark_bin" ]]; then
    log_error "netavark binary not found in /usr/lib/podman or /usr/libexec/podman"
    exit 1
fi

_netavark_ver="$("$_netavark_bin" --version 2>&1 | awk '/netavark/ {print $2; exit}')"
log_info "netavark on disk: ${_netavark_ver}"

_version_lt() {
    # Returns 0 (true) when $1 < $2 in semver dotted-numeric order.
    [[ "$1" = "$2" ]] && return 1
    [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" = "$1" ]]
}

if _version_lt "$_netavark_ver" "$NETAVARK_REQUIRED"; then
    log_warn "netavark $_netavark_ver < $NETAVARK_REQUIRED — upgrading from upstream"
    _uname_m="$(uname -m)"
    case "$_uname_m" in
        x86_64|amd64)   _na_asset="netavark.gz" ;;
        aarch64|arm64)  _na_asset="netavark.aarch64.gz" ;;
        *) log_error "Unsupported arch for netavark upgrade: $_uname_m"; exit 1 ;;
    esac
    NETAVARK_DL="https://github.com/containers/netavark/releases/download/v${NETAVARK_UPSTREAM_VER}/${_na_asset}"
    NETAVARK_TMP="$(mktemp /tmp/lynx-netavark.XXXXXX.gz)"
    if ! curl -fsSL --max-time 120 "$NETAVARK_DL" -o "$NETAVARK_TMP"; then
        log_error "Failed to download netavark from $NETAVARK_DL"
        rm -f "$NETAVARK_TMP"
        exit 1
    fi
    gunzip -f "$NETAVARK_TMP"
    NETAVARK_BIN_TMP="${NETAVARK_TMP%.gz}"
    chmod 755 "$NETAVARK_BIN_TMP"
    install -m 755 "$NETAVARK_BIN_TMP" "$_netavark_bin"
    rm -f "$NETAVARK_BIN_TMP"
    log_ok "netavark upgraded to upstream v${NETAVARK_UPSTREAM_VER}"
fi
unset _netavark_bin _netavark_ver _candidate _na_asset _uname_m

# Configure Podman to use Netavark with the native nftables firewall driver
# (instead of iptables-nft), removing the iptables-package dependency.
if ! grep -q 'firewall_driver.*nftables' /etc/containers/containers.conf 2>/dev/null; then
    mkdir -p /etc/containers
    {
        grep -v 'network_backend\|firewall_driver\|\[network\]' /etc/containers/containers.conf 2>/dev/null || true
        printf '\n[network]\nnetwork_backend = "netavark"\nfirewall_driver = "nftables"\n'
    } > /tmp/lynx-containers.conf
    mv /tmp/lynx-containers.conf /etc/containers/containers.conf
    log_ok "Podman configured: netavark backend, nftables firewall driver"
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

for spec in \
    "lynx-dashboard-db:${DASHBOARD_DB_SUBNET}" \
    "lynx-dashboard-cache:${DASHBOARD_CACHE_SUBNET}" \
    "lynx-dashboard-app:${DASHBOARD_APP_SUBNET}"; do
    net="${spec%%:*}"
    subnet="${spec##*:}"
    if podman network exists "$net" 2>/dev/null; then
        log_warn "Network $net already exists — skipping"
    else
        podman network create "$net" --subnet "$subnet"
        log_ok "Network created: $net ($subnet)"
    fi
done

# --- Download core binaries -------------------------------------------------
#
# The backend binary is needed BEFORE secret generation: every Lynx crypto
# primitive (random tokens, Ed25519/X25519 keypairs, X.509 CA) is produced by
# `lynx-dashboard-backend` subcommands so the host does not need `openssl`.
# Binaries are signed with Ed25519; the public key is hardcoded below and the
# private key lives only in GitHub Actions secrets.

log_section "Downloading core binaries"

GITHUB_REPO="Jaro-c/Lynx"
RELEASE_VERIFY_KEY_B64="OsBV4t+vQSn10FAI8UzAJEBS0IUqp8D2bZtlQYD8j+Q="

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

log_info "Fetching latest dashboard release..."
LATEST_TAG=$(curl -fsSL \
    "https://api.github.com/repos/${GITHUB_REPO}/releases" \
    | python3 -c "
import sys, json
releases = json.load(sys.stdin)
tags = [r['tag_name'] for r in releases
        if r.get('tag_name','').startswith('dashboard@')
        and not r.get('prerelease') and not r.get('draft')]
if tags:
    def ver(t): return tuple(int(x) for x in t.split('@')[1].split('.'))
    print(max(tags, key=ver))
" 2>/dev/null)

if [[ -z "$LATEST_TAG" ]]; then
    log_error "No dashboard release found in ${GITHUB_REPO}"
    exit 1
fi
log_ok "Latest release: ${LATEST_TAG}"

# LYNX_RELEASE_BASE lets local-host testing point binary downloads at a private
# HTTP server (e.g. `python3 -m http.server`) before a real release exists.
RELEASE_BASE="${LYNX_RELEASE_BASE:-https://github.com/${GITHUB_REPO}/releases/download/${LATEST_TAG}}"
BIN_DIR="/etc/lynx/bin"
FRONTEND_DIR="/etc/lynx/frontend"

mkdir -p "$BIN_DIR" "$FRONTEND_DIR"
chmod 700 "$BIN_DIR" "$FRONTEND_DIR"

# Verify Ed25519 signature. Args: <file> <sig-file>
_verify_release_sig() {
    local file="$1" sig_file="$2"
    python3 - "$RELEASE_VERIFY_KEY_B64" "$file" "$sig_file" <<'PYEOF'
import sys, base64
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey

pub_key = Ed25519PublicKey.from_public_bytes(base64.b64decode(sys.argv[1] + "=="))

with open(sys.argv[2], "rb") as f:
    data = f.read()
with open(sys.argv[3], "rb") as f:
    sig = f.read()
try:
    pub_key.verify(sig, data)
except Exception as e:
    print(f"signature invalid: {e}", file=sys.stderr)
    sys.exit(1)
PYEOF
}

# Ensure cryptography lib is available for signature verification.
# Prefer the distro-shipped python3-cryptography package over pip so the host
# does not need python3-pip on minimal systems.
if ! python3 -c "from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey" 2>/dev/null; then
    log_info "Installing python3-cryptography..."
    case "$DISTRO" in
        debian) DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends python3-cryptography -qq ;;
        rhel)   { dnf install -y python3-cryptography 2>/dev/null || yum install -y python3-cryptography 2>/dev/null; } ;;
        *)      log_error "Cannot install python3-cryptography on unknown distro"; exit 1 ;;
    esac
    python3 -c "from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey" || {
        log_error "python3-cryptography not importable after install"
        exit 1
    }
fi

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

log_info "Downloading lynx-compose binary..."
COMPOSE_FILE_BIN="${BIN_DIR}/lynx-compose"
COMPOSE_TMP="${BIN_DIR}/lynx-compose.new"

curl -fsSL --max-time 300 \
    "${RELEASE_BASE}/lynx-compose-linux-${ARCH}" \
    -o "$COMPOSE_TMP"
curl -fsSL --max-time 30 \
    "${RELEASE_BASE}/lynx-compose-linux-${ARCH}.sig" \
    -o "${COMPOSE_TMP}.sig"

log_info "Verifying lynx-compose signature..."
if ! _verify_release_sig "$COMPOSE_TMP" "${COMPOSE_TMP}.sig"; then
    log_error "lynx-compose signature verification FAILED — aborting"
    rm -f "$COMPOSE_TMP" "${COMPOSE_TMP}.sig"
    exit 1
fi
rm -f "${COMPOSE_TMP}.sig"
chmod 755 "$COMPOSE_TMP"
mv "$COMPOSE_TMP" "$COMPOSE_FILE_BIN"
log_ok "lynx-compose installed: ${COMPOSE_FILE_BIN}"

# --- Generate secrets -------------------------------------------------------
#
# Secrets flow directly via pipe — never stored in files or shell history.
# Subshells ensure vars don't leak to parent environment.
# All cryptographic primitives are generated by the lynx-dashboard-backend
# binary subcommands so the host does not need an `openssl` binary.

log_section "Generating secrets"

LB="$BACKEND_FILE"  # short alias for the rest of this section

# Secrets are stored as files in /etc/lynx/secrets/ (root:root, 600).
# Containers access them via bind mounts at /run/secrets/<name>.
# This is equivalent security to Podman secret store (both are files on disk,
# both root-only). Using files avoids Bollard's lack of external secret support.
SECRETS_DIR="/etc/lynx/secrets"
mkdir -p "$SECRETS_DIR"
chmod 700 "$SECRETS_DIR"

# pg_tde keyring directory — pg_tde manages the keyring file here.
# Owned by UID 26 (postgres in the Percona container). Must be backed up:
# without /etc/lynx/pg-keyring/lynx.keyring, the encrypted DB is unrecoverable.
mkdir -p /etc/lynx/pg-keyring
chown 26:26 /etc/lynx/pg-keyring
chmod 700 /etc/lynx/pg-keyring

_write_secret() {
    local name="$1" value="$2"
    printf '%s' "$value" > "$SECRETS_DIR/$name"
    chmod 600 "$SECRETS_DIR/$name"
}

log_info "Generating PostgreSQL root password..."
(
    PG_ROOT=$("$LB" gen-rand 32)
    _write_secret lynx-dashboard-pg-root "$PG_ROOT"
    # Percona PostgreSQL image runs as UID 26 (postgres) from the start — not root.
    # The bind-mounted secret file must be world-readable; the parent dir (700 root:root)
    # prevents host access from unprivileged users.
    chmod 644 "$SECRETS_DIR/lynx-dashboard-pg-root"
    PG_ROOT="$("$LB" gen-rand 32)"
)

log_info "Generating PostgreSQL app password and database URL..."
(
    PG_PASS=$("$LB" gen-rand 32)
    _write_secret lynx-dashboard-pg-pass "$PG_PASS"
    chmod 644 "$SECRETS_DIR/lynx-dashboard-pg-pass"
    _write_secret lynx-dashboard-database-url \
        "postgresql://lynx_dashboard_app:${PG_PASS}@lynx-dashboard-postgres:5432/lynx_dashboard"
    PG_PASS="$("$LB" gen-rand 32)"
)

log_info "Generating Valkey password and URL..."
(
    REDIS_PASS=$("$LB" gen-rand 32)
    _write_secret lynx-dashboard-redis-pass "$REDIS_PASS"
    _write_secret lynx-dashboard-redis-url "redis://:${REDIS_PASS}@lynx-dashboard-valkey:6379"
    REDIS_PASS="$("$LB" gen-rand 32)"
)

log_info "Generating API token..."
_write_secret lynx-dashboard-api-token "$("$LB" gen-rand 32)"
log_info "Generating KEK (Key Encryption Key)..."
_write_secret lynx-dashboard-kek "$("$LB" gen-rand 32 --encoding base64)"
log_info "Generating pepper..."
_write_secret lynx-dashboard-pepper "$("$LB" gen-rand 32)"

log_info "Generating JWT signing keypair (Ed25519)..."
DASHBOARD_SIGN_PUBKEY_FILE="$LYNX_DIR/dashboard-sign-pubkey"
DASHBOARD_SIGN_PUBKEY=""
{
    KEYPAIR=$("$LB" gen-ed25519)
    PRIV_SEED=$(printf '%s' "$KEYPAIR" | sed -n '1p')
    DASHBOARD_SIGN_PUBKEY=$(printf '%s' "$KEYPAIR" | sed -n '2p')
    _write_secret lynx-dashboard-jwt-sign-private "$PRIV_SEED"
    _write_secret lynx-dashboard-jwt-sign-public "$DASHBOARD_SIGN_PUBKEY"
    printf '%s' "$DASHBOARD_SIGN_PUBKEY" > "$DASHBOARD_SIGN_PUBKEY_FILE"
    chmod 644 "$DASHBOARD_SIGN_PUBKEY_FILE"
    PRIV_SEED=$("$LB" gen-rand 32)
}

log_info "Generating JWT encryption keypair (X25519)..."
(
    KEYPAIR=$("$LB" gen-x25519)
    _write_secret lynx-dashboard-jwt-enc-private "$(printf '%s' "$KEYPAIR" | sed -n '1p')"
    _write_secret lynx-dashboard-jwt-enc-public  "$(printf '%s' "$KEYPAIR" | sed -n '2p')"
)

log_info "Generating CA keypair (Ed25519)..."
(
    KEYPAIR=$("$LB" gen-ed25519)
    _write_secret lynx-dashboard-ca-private "$(printf '%s' "$KEYPAIR" | sed -n '1p')"
    _write_secret lynx-dashboard-ca-public  "$(printf '%s' "$KEYPAIR" | sed -n '2p')"
)

log_info "Generating X.509 CA certificate for mTLS (Ed25519)..."
(
    CA_OUT=$("$LB" gen-x509-ca)
    _write_secret lynx-dashboard-x509-ca-cert "$(printf '%s' "$CA_OUT" | sed -n '1p')"
    _write_secret lynx-dashboard-x509-ca-key  "$(printf '%s' "$CA_OUT" | sed -n '2p')"
)

log_info "Generating setup token (one-time bootstrap)..."
SETUP_TOKEN=$("$LB" gen-rand 32)
_write_secret lynx-dashboard-setup-token "$SETUP_TOKEN"
log_ok "All secrets generated"
unset LB

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

# --- Write docker-compose.yml (embedded in script) --------------------------

cat > "$COMPOSE_FILE" << 'COMPOSE_EOF'
services:
  nginx:
    container_name: lynx-dashboard-nginx
    image: docker.io/library/nginx@sha256:65645c7bb6a0661892a8b03b89d0743208a18dd2f3f17a54ef4b76fb8e2f2a10
    ports:
      - "19443:19443"
    volumes:
      - /etc/lynx/tls:/etc/lynx/tls:ro
      - /etc/lynx/nginx/default.conf:/etc/nginx/conf.d/default.conf:ro
      - /etc/lynx/nginx/updating.html:/etc/lynx/nginx/updating.html:ro
    depends_on:
      frontend:
        condition: service_healthy
    healthcheck:
      test: ["CMD-SHELL", "pgrep nginx > /dev/null"]
      interval: 10s
      timeout: 5s
      retries: 5
      start_period: 10s
    restart: unless-stopped
    networks:
      lynx-dashboard-app:
        ipv4_address: 10.89.2.4

  frontend:
    container_name: lynx-dashboard-frontend
    image: docker.io/library/alpine@sha256:48b0309ca019d89d40f670aa1bc06e426dc0931948452e8491e3d65087abc07d
    working_dir: /etc/lynx/frontend
    command: ["/bin/sh", "-c", "apk add --no-cache libgcc libstdc++ && exec /etc/lynx/frontend/lynx-dashboard-frontend"]
    environment:
      - NODE_ENV=production
      - PORT=3000
      - HOSTNAME=0.0.0.0
      - BACKEND_URL=http://lynx-dashboard-backend:8080
      - NEXT_TELEMETRY_DISABLED=1
    volumes:
      - /etc/lynx/frontend:/etc/lynx/frontend
    depends_on:
      backend:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "wget", "-qO", "/dev/null", "http://localhost:3000"]
      interval: 15s
      timeout: 30s
      retries: 5
      start_period: 30s
    restart: unless-stopped
    networks:
      lynx-dashboard-app:
        ipv4_address: 10.89.2.3

  backend:
    container_name: lynx-dashboard-backend
    image: docker.io/library/alpine@sha256:48b0309ca019d89d40f670aa1bc06e426dc0931948452e8491e3d65087abc07d
    command: ["/etc/lynx/bin/lynx-dashboard-backend"]
    ports:
      - "10.100.0.1:8080:8080"
    environment:
      - DATABASE_URL_FILE=/run/secrets/lynx-dashboard-database-url
      - REDIS_URL_FILE=/run/secrets/lynx-dashboard-redis-url
      - INTERNAL_API_TOKEN_FILE=/run/secrets/lynx-dashboard-api-token
      - KEK_FILE=/run/secrets/lynx-dashboard-kek
      - PEPPER_FILE=/run/secrets/lynx-dashboard-pepper
      - JWT_SIGN_PRIVATE_KEY_FILE=/run/secrets/lynx-dashboard-jwt-sign-private
      - JWT_SIGN_PUBLIC_KEY_FILE=/run/secrets/lynx-dashboard-jwt-sign-public
      - JWT_ENC_PRIVATE_KEY_FILE=/run/secrets/lynx-dashboard-jwt-enc-private
      - JWT_ENC_PUBLIC_KEY_FILE=/run/secrets/lynx-dashboard-jwt-enc-public
      - CA_PRIVATE_KEY_FILE=/run/secrets/lynx-dashboard-ca-private
      - CA_PUBLIC_KEY_FILE=/run/secrets/lynx-dashboard-ca-public
      - X509_CA_CERT_FILE=/run/secrets/lynx-dashboard-x509-ca-cert
      - X509_CA_KEY_FILE=/run/secrets/lynx-dashboard-x509-ca-key
      - SETUP_TOKEN_FILE=/run/secrets/lynx-dashboard-setup-token
      - RUST_LOG=${RUST_LOG:-info}
    volumes:
      - /etc/lynx/bin:/etc/lynx/bin
      - /etc/lynx/frontend:/etc/lynx/frontend
      - /run/podman/podman.sock:/run/podman/podman.sock
      - /etc/lynx/secrets/lynx-dashboard-database-url:/run/secrets/lynx-dashboard-database-url:ro
      - /etc/lynx/secrets/lynx-dashboard-redis-url:/run/secrets/lynx-dashboard-redis-url:ro
      - /etc/lynx/secrets/lynx-dashboard-api-token:/run/secrets/lynx-dashboard-api-token:ro
      - /etc/lynx/secrets/lynx-dashboard-kek:/run/secrets/lynx-dashboard-kek:ro
      - /etc/lynx/secrets/lynx-dashboard-pepper:/run/secrets/lynx-dashboard-pepper:ro
      - /etc/lynx/secrets/lynx-dashboard-jwt-sign-private:/run/secrets/lynx-dashboard-jwt-sign-private:ro
      - /etc/lynx/secrets/lynx-dashboard-jwt-sign-public:/run/secrets/lynx-dashboard-jwt-sign-public:ro
      - /etc/lynx/secrets/lynx-dashboard-jwt-enc-private:/run/secrets/lynx-dashboard-jwt-enc-private:ro
      - /etc/lynx/secrets/lynx-dashboard-jwt-enc-public:/run/secrets/lynx-dashboard-jwt-enc-public:ro
      - /etc/lynx/secrets/lynx-dashboard-ca-private:/run/secrets/lynx-dashboard-ca-private:ro
      - /etc/lynx/secrets/lynx-dashboard-ca-public:/run/secrets/lynx-dashboard-ca-public:ro
      - /etc/lynx/secrets/lynx-dashboard-x509-ca-cert:/run/secrets/lynx-dashboard-x509-ca-cert:ro
      - /etc/lynx/secrets/lynx-dashboard-x509-ca-key:/run/secrets/lynx-dashboard-x509-ca-key:ro
      - /etc/lynx/secrets/lynx-dashboard-setup-token:/run/secrets/lynx-dashboard-setup-token:ro
    depends_on:
      postgres:
        condition: service_healthy
      valkey:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "wget", "-qO", "/dev/null", "http://localhost:8080/health"]
      interval: 10s
      timeout: 5s
      retries: 5
      start_period: 15s
    restart: unless-stopped
    networks:
      lynx-dashboard-db:
        ipv4_address: 10.89.0.3
      lynx-dashboard-cache:
        ipv4_address: 10.89.1.3
      lynx-dashboard-app:
        ipv4_address: 10.89.2.2

  postgres:
    container_name: lynx-dashboard-postgres
    image: docker.io/percona/percona-distribution-postgresql@sha256:71cce6ed329d4108461eeaa40fb0c1517bee2e0f78051cee40a4b010eed448c3
    environment:
      - POSTGRES_USER=postgres
      - POSTGRES_DB=lynx_dashboard
      - POSTGRES_PASSWORD_FILE=/run/secrets/lynx-dashboard-pg-root
      - POSTGRES_INITDB_ARGS=-c shared_preload_libraries=pg_tde
    volumes:
      - postgres_data:/data/db
      - /etc/lynx/secrets/lynx-dashboard-pg-root:/run/secrets/lynx-dashboard-pg-root:ro
      - /etc/lynx/secrets/lynx-dashboard-pg-pass:/run/secrets/lynx-dashboard-pg-pass:ro
      - /etc/lynx/pg-keyring:/var/pg-keyring
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres -d lynx_dashboard"]
      interval: 5s
      timeout: 3s
      retries: 10
    restart: unless-stopped
    networks:
      lynx-dashboard-db:
        ipv4_address: 10.89.0.2

  valkey:
    container_name: lynx-dashboard-valkey
    image: docker.io/valkey/valkey@sha256:b027235326507cfdade9b6684056ec1d0b0c0757412e628245129b5d7b788618
    command:
      - sh
      - -c
      - 'valkey-server --save "" --appendonly no --requirepass "$(cat /run/secrets/lynx-dashboard-redis-pass)"'
    volumes:
      - /etc/lynx/secrets/lynx-dashboard-redis-pass:/run/secrets/lynx-dashboard-redis-pass:ro
    healthcheck:
      test:
        - CMD-SHELL
        - 'valkey-cli -a "$(cat /run/secrets/lynx-dashboard-redis-pass)" ping'
      interval: 5s
      timeout: 3s
      retries: 10
    restart: unless-stopped
    networks:
      lynx-dashboard-cache:
        ipv4_address: 10.89.1.2

volumes:
  postgres_data:

networks:
  lynx-dashboard-db:
    external: true
  lynx-dashboard-cache:
    external: true
  lynx-dashboard-app:
    external: true

COMPOSE_EOF

chmod 644 "$COMPOSE_FILE"
log_ok "docker-compose.yml written: ${COMPOSE_FILE}"

printf '%s' "${LATEST_TAG#dashboard@}" > "$BIN_DIR/lynx-dashboard-version"
log_ok "Version: ${LATEST_TAG#dashboard@}"

# --- Start services ---------------------------------------------------------

log_section "Starting services"

# Remove any stale postgres_data volume from a partial previous install.
# lynx-compose does not prefix named volumes with the project name so the
# volume is always called 'postgres_data'. Stale data causes postgres to skip
# init on the next clean install, leaving lynx_dashboard_app with no password.
# Use --force and a direct directory removal as belt-and-suspenders: Podman
# sometimes keeps a ghost reference that makes 'volume rm' fail silently.
podman stop lynx-dashboard-postgres 2>/dev/null || true
podman rm -f lynx-dashboard-postgres 2>/dev/null || true
podman volume rm --force postgres_data 2>/dev/null || true
rm -rf /var/lib/containers/storage/volumes/postgres_data 2>/dev/null || true

# 1. PostgreSQL
log_info "Starting PostgreSQL..."
"$BIN_DIR/lynx-compose" -p lynx-dashboard -f "$COMPOSE_FILE" up -d postgres

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

# Initialize app user and privileges. Runs every install — idempotent.
# Direct psql avoids the docker-entrypoint-initdb.d mechanism which only runs
# on a completely empty PGDATA directory and silently skips on reinstalls.
log_info "Initializing PostgreSQL app user and encryption..."
_PG_PASS=$(cat "$SECRETS_DIR/lynx-dashboard-pg-pass")
podman exec -i lynx-dashboard-postgres psql -U postgres -d lynx_dashboard << SQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'lynx_dashboard_app') THEN
    CREATE USER lynx_dashboard_app WITH NOSUPERUSER NOCREATEDB NOCREATEROLE;
  END IF;
END
\$\$;
ALTER USER lynx_dashboard_app PASSWORD '${_PG_PASS}';
GRANT CONNECT ON DATABASE lynx_dashboard TO lynx_dashboard_app;
GRANT USAGE, CREATE ON SCHEMA public TO lynx_dashboard_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO lynx_dashboard_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT USAGE, SELECT ON SEQUENCES TO lynx_dashboard_app;

-- Enable transparent storage encryption via pg_tde (AES-256).
-- The keyring file is created and managed by pg_tde at /var/pg-keyring/lynx.keyring.
-- All future tables in lynx_dashboard will be transparently encrypted (tde_heap).
-- BACKUP REQUIREMENT: /etc/lynx/pg-keyring/lynx.keyring must be backed up --
-- without it, the encrypted database is unrecoverable even with a valid pg_dump.
CREATE EXTENSION IF NOT EXISTS pg_tde;
SELECT pg_tde_add_database_key_provider_file('lynx-keyring', '/var/pg-keyring/lynx.keyring');
SELECT pg_tde_create_key_using_database_key_provider('lynx-dashboard-key', 'lynx-keyring');
SELECT pg_tde_set_key_using_database_key_provider('lynx-dashboard-key', 'lynx-keyring');
ALTER DATABASE lynx_dashboard SET default_table_access_method = tde_heap;
SQL
unset _PG_PASS
log_ok "PostgreSQL app user and encryption initialized"

# 2. Valkey
log_info "Starting Valkey..."
"$BIN_DIR/lynx-compose" -p lynx-dashboard -f "$COMPOSE_FILE" up --no-recreate -d valkey

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

# --- WireGuard interface must be up before backend binds to 10.100.0.1 ------

log_section "Setting up WireGuard tunnel (dashboard ↔ local agent)"

WG_CONF="$WG_DIR/wg-lynx-dash.conf"

DASHBOARD_PRIV=$(wg genkey)
DASHBOARD_PUB=$(printf '%s' "$DASHBOARD_PRIV" | wg pubkey)
AGENT_PSK=$(wg genpsk)
# Save PSK to a secret file for the backend to use when managing peers.
_write_secret lynx-dashboard-local-agent-psk "$AGENT_PSK"

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
DASHBOARD_PRIV="$("$BACKEND_FILE" gen-rand 32)"

log_ok "WireGuard config written: ${WG_CONF}"
log_ok "Dashboard WireGuard pubkey: ${DASHBOARD_PUB}"
printf '%s' "$DASHBOARD_PUB" > "$LYNX_DIR/dashboard-wg-pubkey"

wg-quick up wg-lynx-dash
systemctl enable "wg-quick@wg-lynx-dash"
log_ok "WireGuard interface up: wg-lynx-dash (10.100.0.1/16)"

# Ensure DNS from dashboard containers is accepted.
# The agent binary (if already running from a prior install) regenerates
# table inet lynx-agent on startup and omits the DNS accept rules from the
# input chain — blocking aardvark-dns from dashboard containers.
# Insert the rules BEFORE the trailing drop so they survive any dynamic
# chain updates the agent makes during normal operation.
_nft_ensure_container_dns() {
    # No-op if table/chain doesn't exist yet (first install, bootstrap applies it below)
    nft list chain inet lynx-agent lynx-base &>/dev/null || return 0
    # If rules already present, skip
    nft list chain inet lynx-agent lynx-base 2>/dev/null | grep -q 'iifname.*podman.*dport 53.*accept' && return 0
    # Insert just before the terminal drop rule — find its handle
    local drop_handle
    drop_handle=$(nft -a list chain inet lynx-agent lynx-base 2>/dev/null | grep '^\s*drop' | grep -o 'handle [0-9]*' | head -1 | awk '{print $2}')
    if [[ -n "$drop_handle" ]]; then
        nft insert rule inet lynx-agent lynx-base handle "$drop_handle" iifname "podman*" udp dport 53 accept 2>/dev/null || true
        nft insert rule inet lynx-agent lynx-base handle "$drop_handle" iifname "podman*" tcp dport 53 accept 2>/dev/null || true
    else
        nft add rule inet lynx-agent lynx-base iifname "podman*" udp dport 53 accept 2>/dev/null || true
        nft add rule inet lynx-agent lynx-base iifname "podman*" tcp dport 53 accept 2>/dev/null || true
    fi
    log_ok "DNS rules injected into lynx-agent.lynx-base for container aardvark-dns"
}
_nft_ensure_container_dns

# 3. Backend
log_info "Starting backend..."
"$BIN_DIR/lynx-compose" -p lynx-dashboard -f "$COMPOSE_FILE" up --no-recreate -d backend

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


# 4. Frontend
log_info "Starting frontend..."
"$BIN_DIR/lynx-compose" -p lynx-dashboard -f "$COMPOSE_FILE" up --no-recreate -d frontend

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
    "$BACKEND_FILE" cert-self-signed \
        --cn "$cn" \
        --days 90 \
        --cert-out "$CERT" \
        --key-out "$KEY"
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
ExecStart=/bin/bash -c '/etc/lynx/bin/lynx-dashboard-backend cert-self-signed \
    --cn "$(hostname -f)" \
    --days 90 \
    --cert-out /etc/lynx/tls/dashboard.crt \
    --key-out /etc/lynx/tls/dashboard.key \
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
    || echo "10.89.2.1")
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
        set \$upstream http://lynx-dashboard-frontend:3000;
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

        # Allow agents -> dashboard backend (management plane)
        ip saddr 10.100.0.0/16 ip daddr 10.100.0.1 tcp dport 8080 ct state new accept

        # Block agent-to-agent traffic within management subnet
        ip saddr 10.100.0.0/16 ip daddr 10.100.0.0/16 drop

        # Dashboard WireGuard interface can reach itself
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

# Dashboard-specific nftables table — separate from table inet lynx-agent so the
# agent binary never overwrites these rules. The agent manages only lynx-agent;
# this table persists across agent nftables reloads.
# Without this, aardvark-dns (on podman* bridges) is unreachable from containers
# after the agent binary starts and re-renders its ruleset without DNS accept rules.
cat > /etc/nftables-lynx-dashboard.conf << 'NFT_DASH'
destroy table inet lynx-dashboard
table inet lynx-dashboard {
    chain allow-container-dns {
        type filter hook input priority filter - 1; policy accept;
        iifname "podman*" udp dport 53 accept
        iifname "podman*" tcp dport 53 accept
    }
}
NFT_DASH

nft -f /etc/nftables-lynx-dashboard.conf
log_ok "Dashboard nftables (container DNS) applied"

# Persist across reboots — migrate away from old lynx-dashboard include
if [[ -f /etc/nftables.conf ]]; then
    sed -i '/nftables-lynx-dashboard/d' /etc/nftables.conf
    if ! grep -q "nftables-lynx-agent" /etc/nftables.conf; then
        echo 'include "/etc/nftables-lynx-agent.conf"' >> /etc/nftables.conf
    fi
    if ! grep -q "nftables-lynx-dashboard" /etc/nftables.conf; then
        echo 'include "/etc/nftables-lynx-dashboard.conf"' >> /etc/nftables.conf
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
CERT_EXPIRY=$("$BACKEND_FILE" cert-expiry "$CERT")

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
SETUP_TOKEN="$("$BACKEND_FILE" gen-rand 32)"  # overwrite in memory
unset SETUP_TOKEN
echo ""
echo -e "${BOLD}${YELLOW}=== WireGuard bootstrap data (copy for agent install) ===${RESET}"
echo -e "  ${BOLD}Dashboard endpoint:${RESET}      ${HOST_IP}:${AGENT_WG_PORT}"
echo -e "  ${BOLD}Dashboard WG pubkey:${RESET}     ${DASHBOARD_PUB}"
echo -e "  ${BOLD}Preshared key:${RESET}           ${AGENT_PSK}"
echo -e "  ${BOLD}Dashboard signing key:${RESET}   ${DASHBOARD_SIGN_PUBKEY}"
echo -e "${YELLOW}This is the only time the PSK is shown. Copy it now.${RESET}"
echo -e "${YELLOW}The signing key is also at: ${DASHBOARD_SIGN_PUBKEY_FILE}${RESET}"
echo ""
# Clear PSK from memory after display
AGENT_PSK="$("$BACKEND_FILE" gen-rand 32)"
unset AGENT_PSK DASHBOARD_PUB DASHBOARD_SIGN_PUBKEY
echo -e "${YELLOW}Next step:${RESET} Run the agent install script on this VPS to complete the local WireGuard tunnel."
echo ""
echo -e "${BOLD}${RED}=== BACKUP REQUIRED ===${RESET}"
echo -e "  Back up these files — loss means permanent data loss:"
echo -e "  ${BOLD}/etc/lynx/pg-keyring/lynx.keyring${RESET}  ← pg_tde encryption keyring"
echo -e "  ${BOLD}/etc/lynx/secrets/lynx-dashboard-kek${RESET}  ← application KEK"
echo ""
echo -e "  ${BOLD}Made with love by Jaroc${RESET} — https://github.com/Jaro-c/Lynx"
echo ""
