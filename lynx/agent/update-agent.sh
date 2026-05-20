#!/usr/bin/env bash
# -----------------------------------------------------------------------------
# update-agent.sh — Lynx Agent update script
#
# Description:
#   Updates the Lynx Agent to the latest available release.
#   Downloads the binary from GitHub Releases, verifies Ed25519 signature,
#   swaps atomically with .prev backup, and restarts the systemd service.
#   Preserves all data, secrets, WireGuard config, and nftables rules.
#
# Usage:
#   sudo ./update-agent.sh
#   sudo ./update-agent.sh --force   (update even if already at latest)
#
# Requirements:
#   - Lynx Agent already installed (run setup-agent.sh first)
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
BINARY_PATH="$BIN_DIR/lynx-agent"
GITHUB_REPO="Jaro-c/Lynx"
VERSION_FILE="$BIN_DIR/lynx-agent-version"
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

if [[ ! -f "$BINARY_PATH" ]]; then
    log_error "Lynx Agent not installed — run setup-agent.sh first"
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

log_info "Fetching latest agent release..."
LATEST_TAG=$(curl -fsSL \
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

if [[ -z "$LATEST_TAG" ]]; then
    log_error "No agent release found in ${GITHUB_REPO}"
    exit 1
fi

LATEST_VERSION="${LATEST_TAG#agent@}"
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

# --- Download agent binary --------------------------------------------------

log_section "Downloading agent binary"

AGENT_TMP="${BIN_DIR}/lynx-agent.new"

curl -fsSL --max-time 300 \
    "${RELEASE_BASE}/lynx-agent-linux-${ARCH}" \
    -o "$AGENT_TMP"
curl -fsSL --max-time 30 \
    "${RELEASE_BASE}/lynx-agent-linux-${ARCH}.sig" \
    -o "${AGENT_TMP}.sig"

log_info "Verifying agent signature..."
if ! _verify_release_sig "$AGENT_TMP" "${AGENT_TMP}.sig"; then
    log_error "Agent signature verification FAILED — aborting, current version intact"
    rm -f "$AGENT_TMP" "${AGENT_TMP}.sig"
    exit 1
fi
rm -f "${AGENT_TMP}.sig"
chmod 755 "$AGENT_TMP"
log_ok "Agent binary verified"

# --- Swap binary and restart service ----------------------------------------

log_section "Deploying agent binary"

cp -f "$BINARY_PATH" "${BINARY_PATH}.prev" 2>/dev/null || true
mv "$AGENT_TMP" "$BINARY_PATH"
log_ok "Agent binary swapped"

log_info "Restarting lynx-agent service..."
if ! systemctl restart lynx-agent.service; then
    log_error "Service failed to restart after update"
    if [[ -f "${BINARY_PATH}.prev" ]]; then
        log_warn "Restoring previous binary..."
        mv "${BINARY_PATH}.prev" "$BINARY_PATH"
        systemctl restart lynx-agent.service || true
        log_error "Previous version restored — investigate before retrying"
    fi
    journalctl -u lynx-agent.service --no-pager -n 30 2>/dev/null || true
    exit 1
fi

sleep 3

if ! systemctl is-active --quiet lynx-agent.service; then
    log_error "lynx-agent is not running after update"
    if [[ -f "${BINARY_PATH}.prev" ]]; then
        log_warn "Restoring previous binary..."
        mv "${BINARY_PATH}.prev" "$BINARY_PATH"
        systemctl restart lynx-agent.service || true
        log_error "Previous version restored — investigate before retrying"
    fi
    systemctl status lynx-agent.service --no-pager 2>/dev/null || true
    exit 1
fi

log_ok "lynx-agent running with new binary"

# --- Write version file -----------------------------------------------------

printf '%s' "$LATEST_VERSION" > "$VERSION_FILE"

# --- Done -------------------------------------------------------------------

log_section "Update complete"

echo ""
echo -e "${GREEN}${BOLD}Lynx Agent updated to v${LATEST_VERSION}${RESET}"
if [[ -n "$CURRENT_VERSION" ]]; then
    echo -e "  ${BOLD}Previous version:${RESET} $CURRENT_VERSION"
fi
echo -e "  ${BOLD}Current version:${RESET}  $LATEST_VERSION"
echo ""
if [[ -f "${BINARY_PATH}.prev" ]]; then
    echo -e "  ${BOLD}Recovery:${RESET} previous binary saved as ${BINARY_PATH}.prev"
    echo -e "            auto-removed on next successful update"
fi
echo ""
echo -e "  If something fails:"
echo -e "    ${BOLD}lynx-agent logs --errors${RESET}"
echo ""
echo -e "  ${BOLD}Made with love by Jaroc${RESET} — https://github.com/Jaro-c/Lynx"
echo ""
