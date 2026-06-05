#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# update-dashboard.sh — Lynx Dashboard update script
#
# Description:
#   Updates the Lynx Dashboard to the latest available release.
#   Downloads binaries from GitHub Releases, verifies Ed25519 signatures,
#   swaps atomically with .prev backup, and restarts containers.
#   Preserves all data, secrets, WireGuard config, and TLS certificates.
#
# Usage:
#   sudo ./update-dashboard.sh
#   sudo ./update-dashboard.sh --force   (update even if already at latest)
#
# Requirements:
#   - Lynx Dashboard already installed (run setup-dashboard.sh first)
#   - Run as root
#   - Internet access to GitHub Releases
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

BIN_DIR="/etc/lynx/bin"
FRONTEND_DIR="/etc/lynx/frontend"
GITHUB_REPO="Glyndor/panel"
VERSION_FILE="$BIN_DIR/lynx-dashboard-version"
COMPOSE_FILE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/docker-compose.yml"
FORCE=false

# --- Parse args -------------------------------------------------------------

for arg in "$@"; do
    case "$arg" in
        --force) FORCE=true ;;
        *) log_error "Unknown argument: $arg"; exit 1 ;;
    esac
done

# --- Root check -------------------------------------------------------------

if [[ $EUID -ne 0 ]]; then
    log_error "Must run as root: sudo $0"
    exit 1
fi

# --- Installation check -----------------------------------------------------

if [[ ! -f "$BIN_DIR/lynx-dashboard-backend" ]]; then
    log_error "Lynx Dashboard not installed — run setup-dashboard.sh first"
    exit 1
fi

# --- Version check ----------------------------------------------------------

log_section "Checking versions"

CURRENT_VERSION=""
if [[ -f "$VERSION_FILE" ]]; then
    CURRENT_VERSION=$(cat "$VERSION_FILE")
    log_info "Current version: $CURRENT_VERSION"
else
    log_warn "No version file found — version unknown, proceeding with update"
fi

_ARCH=$(uname -m)
case "$_ARCH" in
    x86_64)  ARCH="x86_64" ;;
    aarch64) ARCH="arm64" ;;
    *)
        log_error "Unsupported architecture: $_ARCH"
        exit 1
        ;;
esac

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

LATEST_VERSION="${LATEST_TAG#dashboard@}"
log_info "Latest version:  $LATEST_VERSION"

if [[ "$CURRENT_VERSION" == "$LATEST_VERSION" ]] && ! $FORCE; then
    log_ok "Already at latest version ($LATEST_VERSION) — nothing to do"
    log_info "Use --force to reinstall the same version"
    exit 0
fi

if [[ -n "$CURRENT_VERSION" ]]; then
    log_info "Updating: $CURRENT_VERSION → $LATEST_VERSION"
else
    log_info "Installing version: $LATEST_VERSION"
fi

# --- Signature verification setup -------------------------------------------

if ! python3 -c "from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey" 2>/dev/null; then
    log_info "Installing Python cryptography library..."
    if command -v pip3 &>/dev/null; then
        pip3 install --quiet cryptography
    else
        python3 -m pip install --quiet cryptography
    fi
fi

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

RELEASE_BASE="https://github.com/${GITHUB_REPO}/releases/download/${LATEST_TAG}"

# --- Download backend -------------------------------------------------------

log_section "Downloading backend binary"

BACKEND_FILE="$BIN_DIR/lynx-dashboard-backend"
BACKEND_TMP="$BIN_DIR/lynx-dashboard-backend.new"

curl -fsSL --max-time 300 \
    "${RELEASE_BASE}/lynx-dashboard-backend-linux-${ARCH}" \
    -o "$BACKEND_TMP"
curl -fsSL --max-time 30 \
    "${RELEASE_BASE}/lynx-dashboard-backend-linux-${ARCH}.sig" \
    -o "${BACKEND_TMP}.sig"

log_info "Verifying backend signature..."
if ! _verify_release_sig "$BACKEND_TMP" "${BACKEND_TMP}.sig"; then
    log_error "Backend signature verification FAILED — aborting, current version intact"
    rm -f "$BACKEND_TMP" "${BACKEND_TMP}.sig"
    exit 1
fi
rm -f "${BACKEND_TMP}.sig"
chmod 755 "$BACKEND_TMP"
log_ok "Backend verified"

# --- Download podup --------------------------------------------------

log_section "Downloading podup binary"

# podup ships from its own repository since the extraction — resolve its
# latest release independently of the dashboard release.
COMPOSE_REPO="Glyndor/podup"
COMPOSE_TAG=$(curl -fsSL \
    "https://api.github.com/repos/${COMPOSE_REPO}/releases" \
    | python3 -c "
import sys, json
releases = json.load(sys.stdin)
tags = [r['tag_name'] for r in releases
        if r.get('tag_name','').startswith('v')
        and not r.get('prerelease') and not r.get('draft')]
if tags:
    def ver(t): return tuple(int(x) for x in t.lstrip('v').split('.'))
    print(max(tags, key=ver))
" 2>/dev/null)

if [[ -z "$COMPOSE_TAG" ]]; then
    log_error "No podup release found in ${COMPOSE_REPO}"
    exit 1
fi
log_ok "Latest podup release: ${COMPOSE_TAG}"
COMPOSE_RELEASE_BASE="https://github.com/${COMPOSE_REPO}/releases/download/${COMPOSE_TAG}"

COMPOSE_FILE_BIN="$BIN_DIR/podup"
COMPOSE_TMP="$BIN_DIR/podup.new"

curl -fsSL --max-time 300 \
    "${COMPOSE_RELEASE_BASE}/podup-linux-${ARCH}" \
    -o "$COMPOSE_TMP"
curl -fsSL --max-time 30 \
    "${COMPOSE_RELEASE_BASE}/podup-linux-${ARCH}.sig" \
    -o "${COMPOSE_TMP}.sig"

log_info "Verifying podup signature..."
if ! _verify_release_sig "$COMPOSE_TMP" "${COMPOSE_TMP}.sig"; then
    log_error "podup signature verification FAILED — aborting, current version intact"
    rm -f "$BACKEND_TMP" "$COMPOSE_TMP" "${COMPOSE_TMP}.sig"
    exit 1
fi
rm -f "${COMPOSE_TMP}.sig"
chmod 755 "$COMPOSE_TMP"
log_ok "podup verified"

# --- Download frontend ------------------------------------------------------

log_section "Downloading frontend binary and assets"

FRONTEND_BIN_FILE="$FRONTEND_DIR/lynx-dashboard-frontend"
FRONTEND_BIN_TMP="$FRONTEND_DIR/lynx-dashboard-frontend.new"
FRONTEND_ASSETS_TMP="$FRONTEND_DIR/assets.new.tar.gz"

curl -fsSL --max-time 300 \
    "${RELEASE_BASE}/lynx-dashboard-frontend-linux-${ARCH}" \
    -o "$FRONTEND_BIN_TMP"
curl -fsSL --max-time 30 \
    "${RELEASE_BASE}/lynx-dashboard-frontend-linux-${ARCH}.sig" \
    -o "${FRONTEND_BIN_TMP}.sig"

log_info "Verifying frontend binary signature..."
if ! _verify_release_sig "$FRONTEND_BIN_TMP" "${FRONTEND_BIN_TMP}.sig"; then
    log_error "Frontend binary signature verification FAILED — aborting, current version intact"
    rm -f "$BACKEND_TMP" "$FRONTEND_BIN_TMP" "${FRONTEND_BIN_TMP}.sig"
    exit 1
fi
rm -f "${FRONTEND_BIN_TMP}.sig"
chmod 755 "$FRONTEND_BIN_TMP"

curl -fsSL --max-time 300 \
    "${RELEASE_BASE}/lynx-dashboard-frontend-assets-linux-${ARCH}.tar.gz" \
    -o "$FRONTEND_ASSETS_TMP"
curl -fsSL --max-time 30 \
    "${RELEASE_BASE}/lynx-dashboard-frontend-assets-linux-${ARCH}.tar.gz.sig" \
    -o "${FRONTEND_ASSETS_TMP}.sig"

log_info "Verifying frontend assets signature..."
if ! _verify_release_sig "$FRONTEND_ASSETS_TMP" "${FRONTEND_ASSETS_TMP}.sig"; then
    log_error "Frontend assets signature verification FAILED — aborting, current version intact"
    rm -f "$BACKEND_TMP" "$FRONTEND_BIN_TMP" "$FRONTEND_ASSETS_TMP" "${FRONTEND_ASSETS_TMP}.sig"
    exit 1
fi
rm -f "${FRONTEND_ASSETS_TMP}.sig"
log_ok "Frontend verified"

# --- Swap backend binary ----------------------------------------------------
#
# Atomic mv replaces the binary the Podman volume serves.
# Backend container has restart: always — Podman restarts it automatically.
# New binary runs DB migrations on startup before serving requests.

log_section "Deploying backend"

cp -f "$BACKEND_FILE" "${BACKEND_FILE}.prev" 2>/dev/null || true
mv "$BACKEND_TMP" "$BACKEND_FILE"
cp -f "$COMPOSE_FILE_BIN" "${COMPOSE_FILE_BIN}.prev" 2>/dev/null || true
mv "$COMPOSE_TMP" "$COMPOSE_FILE_BIN"
log_ok "Backend binary swapped — waiting for Podman to restart container..."

for i in $(seq 1 40); do
    if podman inspect lynx-dashboard-backend --format '{{.State.Health.Status}}' 2>/dev/null | grep -q healthy; then
        log_ok "Backend healthy"
        break
    fi
    if [[ $i -eq 40 ]]; then
        log_error "Backend did not become healthy after update"
        if [[ -f "${BACKEND_FILE}.prev" ]]; then
            log_warn "Restoring previous backend binary..."
            mv "${BACKEND_FILE}.prev" "$BACKEND_FILE"
            podman restart lynx-dashboard-backend 2>/dev/null || true
            log_error "Previous version restored — investigate before retrying"
        fi
        podman logs lynx-dashboard-backend --tail 50 2>/dev/null || true
        rm -f "$FRONTEND_BIN_TMP" "$FRONTEND_ASSETS_TMP"
        exit 1
    fi
    sleep 3
done

# --- Swap frontend binary ---------------------------------------------------

log_section "Deploying frontend"

log_info "Stopping frontend container (nginx serves updating.html during swap)..."
podman stop lynx-dashboard-frontend 2>/dev/null || true

cp -f "$FRONTEND_BIN_FILE" "${FRONTEND_BIN_FILE}.prev" 2>/dev/null || true
mv "$FRONTEND_BIN_TMP" "$FRONTEND_BIN_FILE"

tar -xzf "$FRONTEND_ASSETS_TMP" -C "$FRONTEND_DIR"
rm -f "$FRONTEND_ASSETS_TMP"

log_info "Starting frontend container..."
if ! podman start lynx-dashboard-frontend 2>/dev/null; then
    /etc/lynx/bin/podup -p lynx-dashboard -f "$COMPOSE_FILE" up -d frontend 2>/dev/null || {
        log_error "Failed to start frontend container"
        if [[ -f "${FRONTEND_BIN_FILE}.prev" ]]; then
            log_warn "Restoring previous frontend binary..."
            mv "${FRONTEND_BIN_FILE}.prev" "$FRONTEND_BIN_FILE"
            podman start lynx-dashboard-frontend 2>/dev/null || true
            log_error "Previous frontend version restored — investigate before retrying"
        fi
        exit 1
    }
fi

log_info "Waiting for frontend to become healthy..."
for i in $(seq 1 30); do
    if podman inspect lynx-dashboard-frontend --format '{{.State.Health.Status}}' 2>/dev/null | grep -q healthy; then
        log_ok "Frontend healthy"
        break
    fi
    if [[ $i -eq 30 ]]; then
        log_error "Frontend did not become healthy after update"
        if [[ -f "${FRONTEND_BIN_FILE}.prev" ]]; then
            log_warn "Restoring previous frontend binary..."
            podman stop lynx-dashboard-frontend 2>/dev/null || true
            mv "${FRONTEND_BIN_FILE}.prev" "$FRONTEND_BIN_FILE"
            podman start lynx-dashboard-frontend 2>/dev/null || true
            log_error "Previous frontend version restored — investigate before retrying"
        fi
        podman logs lynx-dashboard-frontend --tail 30 2>/dev/null || true
        exit 1
    fi
    sleep 3
done
log_ok "Frontend deployed"

# --- Write version file -----------------------------------------------------

printf '%s' "$LATEST_VERSION" > "$VERSION_FILE"

# --- Done -------------------------------------------------------------------

log_section "Update complete"

echo ""
echo -e "${GREEN}${BOLD}Lynx Dashboard updated to v${LATEST_VERSION}${RESET}"
if [[ -n "$CURRENT_VERSION" ]]; then
    echo -e "  ${BOLD}Previous version:${RESET} $CURRENT_VERSION"
fi
echo -e "  ${BOLD}Current version:${RESET}  $LATEST_VERSION"
echo ""
echo -e "${YELLOW}Note:${RESET} JWT keys are rotated on update — all active sessions invalidated."
echo -e "      Users will need to log in again."
echo ""
if [[ -f "${BACKEND_FILE}.prev" || -f "${FRONTEND_BIN_FILE}.prev" ]]; then
    echo -e "  ${BOLD}Recovery:${RESET} previous binaries saved as .prev in ${BIN_DIR}/"
    echo -e "            and ${FRONTEND_DIR}/ — auto-removed on next successful update"
fi
echo ""
echo -e "  If something fails:"
echo -e "    ${BOLD}lynx-dashboard-backend logs --errors${RESET}"
echo ""
echo -e "  ${BOLD}Made with love by Jaroc${RESET} — https://github.com/Glyndor/panel"
echo ""
