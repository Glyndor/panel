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
AGENT_WG_IP=""           # set from dashboard-assigned IP during onboarding prompt
DASHBOARD_WG_IP="10.100.0.1"
WG_PORT=51820
AGENT_PORT=9090
LYNX_AGENT_USER="lynx-agent"
PG_NETWORK="lynx-agent-db"
PG_CONTAINER="lynx-agent-postgres"
PG_IMAGE="docker.io/library/postgres@sha256:bfae840554bdbd4e9f8d097d8e23ffda8aac82866e04ea0d6bc09647234dd359"
PG_DB="lynx_agent"
PG_SUBNET="172.20.100.0/24"
PG_STATIC_IP="172.20.100.2"  # Fixed IP — agent binary (root) connects directly, no host port mapping
# Agent UUID v7 — generated on first install, persists across updates
AGENT_ID=""

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

    # Remove PostgreSQL container + data.
    # Use stop+rm (not rm -f) so netavark tears down iptables port-forwarding rules
    # before the network is removed. Force removal skips this cleanup and leaves
    # stale DNAT rules that silently capture traffic for the next install.
    podman stop --time 10 "$PG_CONTAINER" 2>/dev/null || true
    podman rm "$PG_CONTAINER" 2>/dev/null || true
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

    # /etc/lynx is shared with the dashboard on co-located VPSes.
    # Preserve files that belong to the dashboard so the dashboard containers
    # are not disrupted by an agent reinstall. The agent-specific content is
    # removed explicitly; the shared directory is removed only if empty.
    _SAVED_DASH_SIGN_PUBKEY=""
    [[ -r "$LYNX_DIR/dashboard-sign-pubkey" ]] && _SAVED_DASH_SIGN_PUBKEY=$(< "$LYNX_DIR/dashboard-sign-pubkey")

    rm -rf "$LYNX_WG_DIR" "$LYNX_DIR/credentials"
    rm -f  "$AGENT_CONF" "$LYNX_DIR/agent-id"
    rm -f  "$BIN_DIR/lynx-agent" "$BIN_DIR/lynx-agent.prev" "$BIN_DIR/lynx-agent-version"
    rmdir  "$BIN_DIR" "$LYNX_DIR" 2>/dev/null || true

    if [[ -n "$_SAVED_DASH_SIGN_PUBKEY" ]]; then
        mkdir -p "$LYNX_DIR"
        printf '%s' "$_SAVED_DASH_SIGN_PUBKEY" > "$LYNX_DIR/dashboard-sign-pubkey"
        chmod 644 "$LYNX_DIR/dashboard-sign-pubkey"
    fi
    unset _SAVED_DASH_SIGN_PUBKEY

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

# Disk pre-check (§1.4) — PostgreSQL image, agent binary, lynx-compose and
# org/container data easily exceed 2 GB; bail out early instead of failing mid-
# install when a `pull` or container start exhausts the volume.
FREE_DISK_MB=$(df -BM --output=avail / 2>/dev/null | tail -1 | tr -dc '0-9')
if [[ -z "$FREE_DISK_MB" ]] || [[ "$FREE_DISK_MB" -lt 2048 ]]; then
    log_error "Insufficient disk: ${FREE_DISK_MB:-0} MB free on /, minimum 2048 MB required"
    log_info  "Free up space (e.g. \`podman system prune -a\`) and re-run."
    exit 1
fi
log_ok "Disk:  ${FREE_DISK_MB} MB free on / (minimum 2048 MB satisfied)"

# --- Collect dashboard bootstrap data ---------------------------------------
# Prompt FIRST — before anything that may consume stdin (systemctl, userdel,
# package installs with debconf Teletype fallback, podman, wg-quick, etc.).
# Dashboard-sign-pubkey is auto-detected here; it is preserved across agent
# reinstall by _cleanup_existing so the local-agent default still works.

log_section "Dashboard connection setup"

echo ""
echo -e "${YELLOW}You need the values shown when you registered this VPS in the dashboard.${RESET}"
echo ""

read -rp "  Dashboard WireGuard endpoint (IP:PORT, e.g. 1.2.3.4:51820): " DASHBOARD_ENDPOINT
read -rp "  Dashboard WireGuard public key: " DASHBOARD_PUBKEY
read -rsp "  Preshared key (PSK): " PSK
echo ""
read -rp "  Agent WireGuard IP assigned by dashboard (e.g. 10.100.0.3): " AGENT_WG_IP_INPUT
echo ""
read -rsp "  Sync token (shown once when registering this VPS in the dashboard): " SYNC_TOKEN
echo ""

# Dashboard Ed25519 signing public key — required for the agent to verify
# every dashboard-signed command (heartbeat ACK, container ops, nftables push,
# update.self, ...).  Without it the agent rejects every command and enters
# lockdown after the 5-minute heartbeat timeout.
#
# Local-agent path: if running on the same host as the dashboard, the install
# script already wrote it to /etc/lynx/dashboard-sign-pubkey — auto-detect that
# default to avoid an unnecessary prompt.
DEFAULT_DASHBOARD_SIGN_PUBKEY=""
if [[ -r /etc/lynx/dashboard-sign-pubkey ]]; then
    DEFAULT_DASHBOARD_SIGN_PUBKEY=$(< /etc/lynx/dashboard-sign-pubkey)
fi
if [[ -n "$DEFAULT_DASHBOARD_SIGN_PUBKEY" ]]; then
    read -rp "  Dashboard signing public key (Ed25519, base64) [default: detected]: " DASHBOARD_SIGN_PUBKEY
    DASHBOARD_SIGN_PUBKEY="${DASHBOARD_SIGN_PUBKEY:-$DEFAULT_DASHBOARD_SIGN_PUBKEY}"
else
    read -rp "  Dashboard signing public key (Ed25519, base64): " DASHBOARD_SIGN_PUBKEY
fi
unset DEFAULT_DASHBOARD_SIGN_PUBKEY
echo ""

if [[ -z "$DASHBOARD_ENDPOINT" || -z "$DASHBOARD_PUBKEY" || -z "$PSK" || -z "$AGENT_WG_IP_INPUT" || -z "$DASHBOARD_SIGN_PUBKEY" || -z "$SYNC_TOKEN" ]]; then
    log_error "All six values are required (endpoint, WG pubkey, PSK, agent WG IP, dashboard signing pubkey, sync token)."
    exit 1
fi

AGENT_WG_IP="$AGENT_WG_IP_INPUT"

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

# iptables package must NOT be removed — netavark 1.15.2 still calls the iptables
# binary internally even when firewall_driver = nftables is configured. On Ubuntu
# 24.04+ the 'iptables' package is actually iptables-nft which routes all calls
# through nftables; no legacy kernel module is involved. What is incompatible is
# software that *manages* iptables rules (Docker, ufw, firewalld), not the binary.

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
# Check for agent-specific markers only — /etc/lynx is shared with the dashboard
# on VPSes that host both. /etc/lynx alone does not mean the agent is installed.
if id "$LYNX_AGENT_USER" &>/dev/null || \
   systemctl list-unit-files lynx-agent.service 2>/dev/null | grep -q lynx-agent || \
   [[ -f "$AGENT_CONF" ]] || podman container exists "$PG_CONTAINER" 2>/dev/null; then
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
            exec "$(dirname "${BASH_SOURCE[0]:-}")/update-agent.sh"
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

# Podman: use Ubuntu 24.04 noble-updates package (4.9.3+). The kubic/libcontainers
# upstream repo does not publish packages for Ubuntu 24.04 yet. When an official
# upstream repo with a verifiable GPG fingerprint becomes available for noble,
# replace this with repo-pinned install + fingerprint check.
_apt_ensure podman         podman
# openssl replaced by `lynx-agent` subcommands for random/keypair ops.
_apt_ensure nft            nftables
_apt_ensure wg             wireguard-tools
_apt_ensure curl           curl
_apt_ensure python3        python3
# python3-cryptography is the bootstrap Ed25519 verifier; installed below only
# when missing. Drop python3-pip from the dependency set entirely.
# newuidmap/newgidmap: required for rootless Podman user namespaces
_apt_ensure newuidmap      uidmap
# slirp4netns: required for rootless Podman networking (user-space TCP/IP stack)
_apt_ensure slirp4netns    slirp4netns
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

# Netavark 1.10+ supports a native nftables firewall driver — older versions
# require iptables-nft. Lynx upgrades from upstream so the iptables package can
# be dropped entirely (it remains on the incompatible-software list).
NETAVARK_REQUIRED="1.10.0"
_netavark_bin=""
for _candidate in /usr/lib/podman/netavark /usr/libexec/podman/netavark; do
    [[ -x "$_candidate" ]] && _netavark_bin="$_candidate" && break
done
if [[ -z "$_netavark_bin" ]]; then
    log_error "netavark binary not found in /usr/lib/podman or /usr/libexec/podman"
    exit 1
fi
_netavark_ver="$("$_netavark_bin" --version 2>&1 | awk '/netavark/ {print $2; exit}')"
log_info "netavark on disk: ${_netavark_ver}"

_version_lt() {
    [[ "$1" = "$2" ]] && return 1
    [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" = "$1" ]]
}

if _version_lt "$_netavark_ver" "$NETAVARK_REQUIRED"; then
    log_warn "netavark $_netavark_ver < $NETAVARK_REQUIRED — upgrading from upstream"

    log_info "Fetching latest netavark release from GitHub..."
    NETAVARK_UPSTREAM_VER=$(curl -fsSL --max-time 15 \
        "https://api.github.com/repos/containers/netavark/releases/latest" \
        | python3 -c "import sys,json; print(json.load(sys.stdin)['tag_name'].lstrip('v'))" 2>/dev/null)
    if [[ -z "$NETAVARK_UPSTREAM_VER" ]]; then
        log_error "Could not fetch latest netavark version from GitHub API"
        exit 1
    fi
    log_info "Latest netavark: v${NETAVARK_UPSTREAM_VER}"

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

    # Verify sha256 against the checksum published in the release.
    # netavark does not publish GPG signatures — sha256sum protects against
    # corruption and MITM in transit (over HTTPS to github.com).
    log_info "Verifying netavark sha256..."
    _sha256_url="https://github.com/containers/netavark/releases/download/v${NETAVARK_UPSTREAM_VER}/sha256sum"
    _expected_sha=$(curl -fsSL --max-time 15 "$_sha256_url" 2>/dev/null \
        | grep "[[:space:]]${_na_asset}$" | awk '{print $1}')
    if [[ -z "$_expected_sha" ]]; then
        log_error "Could not fetch sha256 for ${_na_asset} from ${_sha256_url}"
        rm -f "$NETAVARK_TMP"
        exit 1
    fi
    _actual_sha=$(sha256sum "$NETAVARK_TMP" | awk '{print $1}')
    if [[ "$_actual_sha" != "$_expected_sha" ]]; then
        log_error "netavark sha256 mismatch — expected ${_expected_sha}, got ${_actual_sha}"
        rm -f "$NETAVARK_TMP"
        exit 1
    fi
    log_ok "netavark sha256 verified"

    gunzip -f "$NETAVARK_TMP"
    install -m 755 "${NETAVARK_TMP%.gz}" "$_netavark_bin"
    rm -f "${NETAVARK_TMP%.gz}"
    log_ok "netavark upgraded to upstream v${NETAVARK_UPSTREAM_VER}"
fi
unset _netavark_bin _netavark_ver _candidate _na_asset _uname_m

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

mkdir -p "$LYNX_DIR"
chmod 755 "$LYNX_DIR"
log_ok "$LYNX_DIR"

# --- Download core agent binary ---------------------------------------------
#
# The agent binary is needed BEFORE secret generation and UUID generation:
# both rely on `lynx-agent gen-rand` / `lynx-agent gen-uuid-v7` so the host
# does not need `openssl` or Python's uuid module on minimal systems.

log_section "Downloading lynx-agent binary"

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

# LYNX_RELEASE_BASE lets local-host testing point binary downloads at a private
# HTTP server. Production installs use the canonical GitHub release URL.
RELEASE_BASE="${LYNX_RELEASE_BASE:-https://github.com/${GITHUB_REPO}/releases/download/${LATEST_AGENT_TAG}}"
mkdir -p "$BIN_DIR"
chmod 755 "$BIN_DIR"

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

# --- Generate agent UUID ----------------------------------------------------

log_section "Generating agent identity"

if [[ -n "${_SAVED_AGENT_ID:-}" ]]; then
    AGENT_ID="$_SAVED_AGENT_ID"
    log_ok "Reusing existing Agent ID: $AGENT_ID"
    unset _SAVED_AGENT_ID
else
    AGENT_ID=$("$BINARY_PATH" gen-uuid-v7)
fi
# Always persist so successive reinstalls preserve the ID
printf '%s' "$AGENT_ID" > "$LYNX_DIR/agent-id"
chmod 600 "$LYNX_DIR/agent-id"
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
    PG_ROOT=$("$BINARY_PATH" gen-rand 32)
    printf '%s' "$PG_ROOT" | podman secret create lynx-agent-pg-root - >/dev/null
    PG_ROOT="$("$BINARY_PATH" gen-rand 32)"
)

log_info "PostgreSQL app password..."
mkdir -p /etc/lynx/credentials
chmod 700 /etc/lynx/credentials
# PG_PASS stays in outer shell until DATABASE_URL can be written (needs container IP).
# Zeroized after writing the credential file.
PG_PASS=$("$BINARY_PATH" gen-rand 32)
printf '%s' "$PG_PASS" | podman secret create lynx-agent-pg-pass - >/dev/null

log_info "Internal bearer token..."
INTERNAL_TOKEN=$("$BINARY_PATH" gen-rand 32)
printf '%s' "$INTERNAL_TOKEN" | podman secret create lynx-agent-internal-token - >/dev/null

log_ok "Agent secrets generated"

# --- Podman network for agent DB -------------------------------------------

log_section "Creating Podman network: $PG_NETWORK"

if ! podman network exists "$PG_NETWORK" 2>/dev/null; then
    podman network create "$PG_NETWORK" --subnet "$PG_SUBNET"
    log_ok "Network created: $PG_NETWORK ($PG_SUBNET)"
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
    --ip "$PG_STATIC_IP" \
    --secret lynx-agent-pg-root,target=lynx-agent-pg-root \
    --secret lynx-agent-pg-pass,target=lynx-agent-pg-pass \
    -e POSTGRES_USER=postgres \
    -e POSTGRES_DB="$PG_DB" \
    -e POSTGRES_PASSWORD_FILE=/run/secrets/lynx-agent-pg-root \
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

# Write DATABASE_URL using the container's static IP on the internal Podman network.
# The agent binary runs as root and can reach the container network directly — no host
# port mapping needed (which would create iptables DNAT rules that survive reinstalls).
(
    DB_URL="postgresql://lynx_agent_app:${PG_PASS}@${PG_STATIC_IP}:5432/${PG_DB}"
    printf '%s' "$DB_URL" > /etc/lynx/credentials/database-url
    chmod 600 /etc/lynx/credentials/database-url
    DB_URL="$("$BINARY_PATH" gen-rand 32)"
)
PG_PASS="$("$BINARY_PATH" gen-rand 32)"

# --- Download agent binary from GitHub Releases ----------------------------

# Agent binary already downloaded earlier — version file gets written below.
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
DASHBOARD_VERIFY_KEY_FILE=/run/credentials/lynx-agent.service/lynx-dashboard-pubkey
SYNC_TOKEN_FILE=/run/credentials/lynx-agent.service/sync-token
LISTEN_ADDR=127.0.0.1:${AGENT_PORT}
DASHBOARD_URL=http://${DASHBOARD_WG_IP}:8080
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
INTERNAL_TOKEN="$("$BINARY_PATH" gen-rand 32)"
unset INTERNAL_TOKEN

# Persist the dashboard's Ed25519 signing public key as a credential — every
# command from the dashboard (heartbeat ACK, container ops, nftables push,
# update.self, ...) is verified against this key.  Without it, the agent
# rejects every command and enters lockdown after 5 minutes.
printf '%s' "$DASHBOARD_SIGN_PUBKEY" > /etc/lynx/credentials/lynx-dashboard-pubkey
chmod 600 /etc/lynx/credentials/lynx-dashboard-pubkey
unset DASHBOARD_SIGN_PUBKEY

# Persist the sync token — used to authenticate the agent→dashboard WebSocket
# connection and audit log sync.  Shown once when registering the VPS.
printf '%s' "$SYNC_TOKEN" > /etc/lynx/credentials/sync-token
chmod 600 /etc/lynx/credentials/sync-token
SYNC_TOKEN="$("$BINARY_PATH" gen-rand 32)"
unset SYNC_TOKEN

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
User=root
Group=root
EnvironmentFile=${AGENT_CONF}
ExecStart=${BINARY_PATH}
Restart=always
RestartSec=5s
TimeoutStopSec=30s

# Systemd credentials (tmpfs — never touches disk)
LoadCredential=database-url:/etc/lynx/credentials/database-url
LoadCredential=internal-token:/etc/lynx/credentials/internal-token
LoadCredential=lynx-dashboard-pubkey:/etc/lynx/credentials/lynx-dashboard-pubkey
LoadCredential=sync-token:/etc/lynx/credentials/sync-token

# Minimal hardening — agent is a privileged system daemon (package management,
# nftables, system user creation, binary self-update all require root).
PrivateTmp=yes

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
log_info "Agent WireGuard public key: ${AGENT_PUB}"
log_info "    Register this VPS in the dashboard with the above public key"
log_info "    The dashboard will provide the PSK, WG IP, and sync token"

# --- NAT detection ---
# Extract the dashboard host (strip port if present)
DASHBOARD_HOST="${DASHBOARD_ENDPOINT%%:*}"

# IP of the local interface that would route to the dashboard
LOCAL_IFACE_IP=$(ip route get "$DASHBOARD_HOST" 2>/dev/null | grep -oP 'src \K\S+' | head -1)

# Public IP as seen from the internet
PUBLIC_IP=$(curl -4 -sf --max-time 5 https://ifconfig.me 2>/dev/null || \
            curl -4 -sf --max-time 5 https://api.ipify.org 2>/dev/null || true)
if [[ -z "$PUBLIC_IP" ]]; then
    PUBLIC_IP=$(curl -6 -sf --max-time 5 https://ifconfig.me 2>/dev/null || \
                curl -6 -sf --max-time 5 https://api6.ipify.org 2>/dev/null || true)
fi

KEEPALIVE_LINE=""

if [[ -n "$LOCAL_IFACE_IP" && -n "$PUBLIC_IP" && "$LOCAL_IFACE_IP" != "$PUBLIC_IP" ]]; then
    KEEPALIVE_LINE="PersistentKeepalive = 25"
    log_info "NAT detected (interface IP: ${LOCAL_IFACE_IP}, public IP: ${PUBLIC_IP})"
    log_info "Enabling PersistentKeepalive = 25 to maintain NAT table entry"
    log_warn "If your provider's NAT timeout is < 25s or blocks persistent UDP, the tunnel may be unstable"
elif [[ -z "$PUBLIC_IP" ]]; then
    # Cannot determine — enable keepalive as safe default
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
Address = ${AGENT_WG_IP}/32

${WG_PEER_BLOCK}
EOF

chmod 600 "$LYNX_WG_CONF"
chown lynx-agent:lynx-agent "$LYNX_WG_CONF"

# Symlink into /etc/wireguard/ for wg-quick compatibility
mkdir -p "$WG_DIR"
ln -sf "$LYNX_WG_CONF" "$WG_CONF_LINK"

# For local agent (same VPS as dashboard): add this agent as a peer in the
# dashboard's WireGuard config so the tunnel is fully bi-directional immediately.
# PSK and AGENT_PUB are available here; PSK is zeroized after this block.
_DASH_WG_CONF="/etc/wireguard/wg-lynx-dash.conf"
if [[ -f "$_DASH_WG_CONF" ]]; then
    # Remove old placeholder comment + any existing [Peer] blocks for this agent
    sed -i '/^# Peer block added by agent/,/^# AllowedIPs.*$/d' "$_DASH_WG_CONF" 2>/dev/null || true
    sed -i '/^\[Peer\]/,/^[[:space:]]*$/{/PublicKey.*'"$AGENT_PUB"'/,/^[[:space:]]*$/d}' "$_DASH_WG_CONF" 2>/dev/null || true
    # Append real peer block
    printf '\n[Peer]\nPublicKey = %s\nPresharedKey = %s\nAllowedIPs = %s/32\n' \
        "$AGENT_PUB" "$PSK" "$AGENT_WG_IP" >> "$_DASH_WG_CONF"
    # Live-update the running WireGuard interface (no restart needed)
    if wg set wg-lynx-dash peer "$AGENT_PUB" preshared-key <(printf '%s' "$PSK") allowed-ips "$AGENT_WG_IP/32" 2>/dev/null; then
        log_ok "Agent added as peer to dashboard WireGuard (wg-lynx-dash)"
    else
        log_warn "Could not live-add peer to wg-lynx-dash — add agent pubkey to dashboard manually"
    fi
fi
unset _DASH_WG_CONF

AGENT_PRIV="$("$BINARY_PATH" gen-rand 32)"  # overwrite
PSK="$("$BINARY_PATH" gen-rand 32)"
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
DASHBOARD_FORWARD_WG_NFT=""
if [[ "$IS_DASHBOARD_VPS" == "true" ]]; then
    DASHBOARD_PORT_NFT="        # Dashboard panel port
        tcp dport 19443 ct state new accept
"
    DASHBOARD_DNS_NFT="        # DNS for container networks (aardvark-dns on Netavark bridge interfaces)
        iifname \"podman*\" udp dport 53 accept
        iifname \"podman*\" tcp dport 53 accept
"
    DASHBOARD_FORWARD_WG_NFT="
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

        # Netavark DNAT rewrites destination to 10.89.x.x for published container ports.
        # Without this rule the DNAT'd packets are dropped here (policy drop).
        ip daddr 10.89.0.0/16 ct state new accept

        # Outbound traffic from Podman containers (package installs, GitHub, cert renewals, etc.)
        iifname "podman*" accept
${DASHBOARD_FORWARD_WG_NFT}
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
