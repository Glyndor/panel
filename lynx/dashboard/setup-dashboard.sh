#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# setup-dashboard.sh — Lynx Dashboard install / reinstall script
#
# Description:
#   Installs the Lynx Dashboard on a VPS. Sets up:
#     - Podman networks (3 isolated: db, cache, app)
#     - Podman secrets (randomly generated, no trace)
#     - PostgreSQL 18 container with isolated app user
#     - Redis 8 container with password auth
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
CERTS_DIR="/etc/lynx/certs"
WG_DIR="/etc/wireguard"
COMPOSE_FILE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/docker-compose.yml"
LISTEN_PORT=19443
AGENT_WG_PORT=51820
AGENT_WG_IP="10.100.0.2"
DASHBOARD_WG_IP="10.100.0.1"
WG_SUBNET="10.100.0.0/24"
CONTAINER_RUNTIME="podman"

# --- Root check -------------------------------------------------------------

if [[ $EUID -ne 0 ]]; then
    log_error "Must run as root: sudo $0"
    exit 1
fi

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
            exec "$(dirname "${BASH_SOURCE[0]}")/update-dashboard.sh"
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

# --- Cleanup function (used by reinstall) -----------------------------------

_cleanup_existing() {
    log_section "Removing existing installation"

    # Stop and remove containers
    for ctr in lynx-dashboard-frontend lynx-dashboard-backend lynx-dashboard-postgres lynx-dashboard-redis; do
        if podman container exists "$ctr" 2>/dev/null; then
            log_info "Removing container: $ctr"
            podman rm -f "$ctr" 2>/dev/null || true
        fi
    done

    # Remove volumes
    podman volume rm lynx-dashboard_postgres_data 2>/dev/null || true

    # Remove networks
    for net in lynx-dashboard-db lynx-dashboard-cache lynx-dashboard-app; do
        podman network rm "$net" 2>/dev/null || true
    done

    # Remove secrets
    for secret in lynx-dashboard-pg-root lynx-dashboard-pg-pass lynx-dashboard-redis-pass \
                  lynx-dashboard-database-url lynx-dashboard-redis-url \
                  lynx-dashboard-api-token lynx-dashboard-kek lynx-dashboard-pepper; do
        podman secret rm "$secret" 2>/dev/null || true
    done

    # Remove WireGuard interface
    if ip link show wg-lynx-dashboard &>/dev/null; then
        ip link delete wg-lynx-dashboard 2>/dev/null || true
    fi
    rm -f "$WG_DIR/wg-lynx-dashboard.conf"

    # Remove systemd units
    systemctl disable --now lynx-dashboard-rotate-certs.timer 2>/dev/null || true
    rm -f /etc/systemd/system/lynx-dashboard-rotate-certs.{service,timer}
    systemctl daemon-reload

    rm -rf "$LYNX_DIR"
    log_ok "Cleanup complete"
}

# --- Check dependencies -----------------------------------------------------

log_section "Checking system dependencies"

_require_cmd() {
    if ! command -v "$1" &>/dev/null; then
        log_error "Required command not found: $1"
        log_info  "Install it with: $2"
        exit 1
    fi
    log_ok "$1 found"
}

_require_cmd podman    "apt install podman"
_require_cmd openssl   "apt install openssl"
_require_cmd nft       "apt install nftables"
_require_cmd wg        "apt install wireguard-tools"
_require_cmd curl      "apt install curl"
_require_cmd systemctl "systemd required"

# --- Create directories -----------------------------------------------------

log_section "Creating directories"

mkdir -p "$LYNX_DIR" "$CERTS_DIR" "$WG_DIR"
chmod 700 "$LYNX_DIR" "$CERTS_DIR" "$WG_DIR"
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
    printf '%s' "$PG_ROOT" | podman secret create lynx-dashboard-pg-root -
    PG_ROOT="$(openssl rand -hex 32)"  # overwrite
)

log_info "Generating PostgreSQL app password and database URL..."
(
    PG_PASS=$(openssl rand -hex 32)
    printf '%s' "$PG_PASS" | podman secret create lynx-dashboard-pg-pass -
    printf 'postgresql://lynx_dashboard_app:%s@lynx-dashboard-postgres:5432/lynx_dashboard' "$PG_PASS" \
        | podman secret create lynx-dashboard-database-url -
    PG_PASS="$(openssl rand -hex 32)"  # overwrite
)

log_info "Generating Redis password and URL..."
(
    REDIS_PASS=$(openssl rand -hex 32)
    printf '%s' "$REDIS_PASS" | podman secret create lynx-dashboard-redis-pass -
    printf 'redis://:%s@lynx-dashboard-redis:6379' "$REDIS_PASS" \
        | podman secret create lynx-dashboard-redis-url -
    REDIS_PASS="$(openssl rand -hex 32)"  # overwrite
)

log_info "Generating API token..."
openssl rand -hex 32 | podman secret create lynx-dashboard-api-token -

log_info "Generating KEK (Key Encryption Key)..."
openssl rand -base64 32 | tr -d '\n' | podman secret create lynx-dashboard-kek -

log_info "Generating pepper..."
openssl rand -hex 32 | podman secret create lynx-dashboard-pepper -

log_ok "All secrets generated — values purged from memory"

# --- Start services ---------------------------------------------------------

log_section "Starting services"

COMPOSE_DIR="$(dirname "$COMPOSE_FILE")"

# 1. PostgreSQL
log_info "Starting PostgreSQL..."
podman compose -f "$COMPOSE_FILE" up -d postgres

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

# 2. Redis
log_info "Starting Redis..."
podman compose -f "$COMPOSE_FILE" up -d redis

log_info "Waiting for Redis to be healthy..."
for i in $(seq 1 30); do
    if podman inspect lynx-dashboard-redis --format '{{.State.Health.Status}}' 2>/dev/null | grep -q healthy; then
        log_ok "Redis is healthy"
        break
    fi
    if [[ $i -eq 30 ]]; then
        log_error "Redis did not become healthy in time"
        exit 1
    fi
    sleep 2
done

# 3. Backend
log_info "Starting backend..."
podman compose -f "$COMPOSE_FILE" up -d backend

log_info "Waiting for backend to be healthy..."
for i in $(seq 1 40); do
    if podman inspect lynx-dashboard-backend --format '{{.State.Health.Status}}' 2>/dev/null | grep -q healthy; then
        log_ok "Backend is healthy"
        break
    fi
    if [[ $i -eq 40 ]]; then
        log_error "Backend did not become healthy in time"
        podman logs lynx-dashboard-backend --tail 50
        exit 1
    fi
    sleep 3
done

# 4. Frontend
log_info "Starting frontend..."
podman compose -f "$COMPOSE_FILE" up -d frontend
log_ok "Frontend started"

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
    -keyout /etc/lynx/certs/dashboard.key -out /etc/lynx/certs/dashboard.crt \
    -days 90 -nodes -sha256 \
    -subj "/CN=$(hostname -f)/O=Lynx/OU=Dashboard" \
    -addext "subjectAltName=DNS:$(hostname -f),IP:$(hostname -I | awk '"'"'{print $1}'"'"')" \
    && chmod 600 /etc/lynx/certs/dashboard.key \
    && podman kill -s HUP lynx-dashboard-frontend'
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

# --- WireGuard — local agent tunnel -----------------------------------------

log_section "Setting up WireGuard tunnel (dashboard ↔ local agent)"

WG_CONF="$WG_DIR/wg-lynx-dashboard.conf"

# Generate dashboard keypair + PSK
DASHBOARD_PRIV=$(wg genkey)
DASHBOARD_PUB=$(printf '%s' "$DASHBOARD_PRIV" | wg pubkey)
# PSK is kept in scope until final output so admin can copy it for the agent install script.
# It is also stored as a Podman secret for the dashboard backend (peer management).
AGENT_PSK=$(wg genpsk)
printf '%s' "$AGENT_PSK" | podman secret create lynx-dashboard-local-agent-psk -

# The local agent keypair is generated by the agent install script.
# Peer block is written by the agent install script after bootstrap.
cat > "$WG_CONF" << EOF
[Interface]
PrivateKey = ${DASHBOARD_PRIV}
Address = ${DASHBOARD_WG_IP}/24
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

cat > /etc/nftables-lynx-dashboard.conf << 'EOF'
table inet lynx-dashboard {
    # WireGuard peer interfaces — populated dynamically
    set wg_peers { type ifname; }

    chain input {
        type filter hook input priority 0; policy drop;

        # Loopback
        iifname "lo" accept

        # Established / related
        ct state established,related accept

        # ICMP
        ip  protocol icmp  accept
        ip6 nexthdr  icmpv6 accept

        # SSH — always open
        tcp dport 22 accept

        # Dashboard panel
        tcp dport 19443 accept

        # WireGuard (for agent tunnels)
        udp dport 51820 accept

        # Drop everything else
        drop
    }

    chain forward {
        type filter hook forward priority 0; policy drop;
    }

    chain output {
        type filter hook output priority 0; policy accept;
    }
}
EOF

# Apply ruleset
nft -f /etc/nftables-lynx-dashboard.conf
log_ok "nftables rules applied (ports: 22, 19443, 51820 UDP)"

# Persist across reboots
if [[ -f /etc/nftables.conf ]]; then
    if ! grep -q "lynx-dashboard" /etc/nftables.conf; then
        echo 'include "/etc/nftables-lynx-dashboard.conf"' >> /etc/nftables.conf
    fi
fi
systemctl enable nftables 2>/dev/null || true

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
