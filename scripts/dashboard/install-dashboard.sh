#!/usr/bin/env bash
# =============================================================================
# install-dashboard.sh
# =============================================================================
# Description: Installs the Lynx Dashboard (backend + frontend + local agent)
#              on this VPS. Downloads binaries from the latest dashboard@*
#              GitHub release and verifies Ed25519 signatures before installing.
#
# Dependencies:
#   - detect-os.sh must be sourced first
#   - install-podman.sh must run first
#   - install-nftables.sh must run first
#   - Colors must be exported from install.sh
# =============================================================================
set -euo pipefail

# --- Constants ----------------------------------------------------------------

readonly RELEASE_VERIFY_KEY="OsBV4t+vQSn10FAI8UzAJEBS0IUqp8D2bZtlQYD8j+Q="
readonly GITHUB_REPO="Jaro-c/Lynx"
readonly BIN_DIR="/etc/lynx/bin"
readonly FRONTEND_DIR="/etc/lynx/frontend"
readonly SECRETS_DIR="/etc/lynx/secrets"
readonly DEPLOY_DIR="/opt/lynx/dashboard"

# --- Helpers ------------------------------------------------------------------

_compose() {
    if podman compose version &>/dev/null 2>&1; then
        podman compose "$@"
    elif command -v podman-compose &>/dev/null; then
        podman-compose "$@"
    else
        echo -e "${RED}Error: podman compose not available. Install podman-compose.${RESET}" >&2
        exit 1
    fi
}

_wait_healthy() {
    local container="$1"
    local max_secs="${2:-90}"
    local elapsed=0
    echo -e "${CYAN}Waiting for ${container} to be healthy...${RESET}"
    while [[ $elapsed -lt $max_secs ]]; do
        local status
        status=$(podman inspect --format '{{.State.Health.Status}}' "$container" 2>/dev/null || true)
        if [[ "$status" == "healthy" ]]; then
            echo -e "${GREEN}${container} is healthy.${RESET}"
            return 0
        fi
        sleep 3
        elapsed=$((elapsed + 3))
    done
    echo -e "${RED}Timeout waiting for ${container} (last status: ${status:-unknown})${RESET}" >&2
    exit 1
}

_create_secret() {
    local name="$1"
    local value="$2"
    podman secret rm "$name" &>/dev/null || true
    printf '%s' "$value" | podman secret create "$name" -
}

_ensure_python_crypto() {
    if python3 -c "from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey" 2>/dev/null; then
        return 0
    fi
    echo -e "${CYAN}Installing Python cryptography...${RESET}"
    case "$PKG_MANAGER" in
        apt-get) apt-get install -y python3-cryptography ;;
        dnf)     dnf install -y python3-cryptography ;;
        pacman)  pacman -S --noconfirm python-cryptography ;;
    esac
}

_ensure_podman_compose() {
    if podman compose version &>/dev/null 2>&1 || command -v podman-compose &>/dev/null; then
        return 0
    fi
    echo -e "${CYAN}Installing podman-compose...${RESET}"
    case "$PKG_MANAGER" in
        apt-get) apt-get install -y podman-compose ;;
        dnf)     dnf install -y podman-compose ;;
        pacman)  pacman -S --noconfirm python-podman-compose ;;
    esac
}

_ensure_uuid_gen() {
    command -v uuidgen &>/dev/null && return 0
    case "$PKG_MANAGER" in
        apt-get) apt-get install -y uuid-runtime ;;
        dnf)     dnf install -y util-linux ;;
        pacman)  pacman -S --noconfirm util-linux ;;
    esac
}

# --- Signature verification ---------------------------------------------------

_verify_sig() {
    local file="$1"
    local sig_file="$2"
    python3 - "$file" "$sig_file" <<'PYEOF'
import base64, sys
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from cryptography.exceptions import InvalidSignature

VERIFY_KEY_B64 = "OsBV4t+vQSn10FAI8UzAJEBS0IUqp8D2bZtlQYD8j+Q="
pub_bytes = base64.b64decode(VERIFY_KEY_B64)
pub_key = Ed25519PublicKey.from_public_bytes(pub_bytes)

with open(sys.argv[1], 'rb') as f:
    data = f.read()
with open(sys.argv[2], 'rb') as f:
    sig = f.read()

try:
    pub_key.verify(sig, data)
except InvalidSignature:
    print(f"FAIL: invalid signature for {sys.argv[1]}", file=sys.stderr)
    sys.exit(1)
PYEOF
}

# --- Download + verify --------------------------------------------------------

_download_verify() {
    local base_url="$1"
    local artifact="$2"
    local dest_dir="$3"

    echo -e "${CYAN}  Downloading ${artifact}...${RESET}"
    curl -fsSL --max-time 300 \
        -o "${dest_dir}/${artifact}" \
        "${base_url}/${artifact}"
    curl -fsSL --max-time 30 \
        -o "${dest_dir}/${artifact}.sig" \
        "${base_url}/${artifact}.sig"

    echo -e "${CYAN}  Verifying ${artifact}...${RESET}"
    _verify_sig "${dest_dir}/${artifact}" "${dest_dir}/${artifact}.sig"
    echo -e "${GREEN}  ✔ ${artifact}${RESET}"
}

# --- Secret generation --------------------------------------------------------

_gen_secrets() {
    local pg_root pg_pass redis_pass api_token setup_token pepper kek

    pg_root=$(openssl rand -hex 32)
    pg_pass=$(openssl rand -hex 32)
    redis_pass=$(openssl rand -hex 32)
    api_token=$(openssl rand -hex 32)
    setup_token=$(openssl rand -hex 32)
    pepper=$(openssl rand -base64 32 | tr -d '\n')
    kek=$(openssl rand -base64 32 | tr -d '\n')

    # Ed25519 + X25519 key pairs via Python
    local jwt_sign_priv jwt_sign_pub jwt_enc_priv jwt_enc_pub ca_priv ca_pub
    read -r jwt_sign_priv jwt_sign_pub < <(python3 - <<'PYEOF'
import base64, secrets
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
seed = secrets.token_bytes(32)
priv = Ed25519PrivateKey.from_private_bytes(seed)
pub_bytes = priv.public_key().public_bytes_raw()
print(base64.b64encode(seed).decode(), base64.b64encode(pub_bytes).decode())
PYEOF
)

    read -r jwt_enc_priv jwt_enc_pub < <(python3 - <<'PYEOF'
import base64
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey
priv = X25519PrivateKey.generate()
priv_bytes = priv.private_bytes_raw()
pub_bytes = priv.public_key().public_bytes_raw()
print(base64.b64encode(priv_bytes).decode(), base64.b64encode(pub_bytes).decode())
PYEOF
)

    read -r ca_priv ca_pub < <(python3 - <<'PYEOF'
import base64, secrets
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
seed = secrets.token_bytes(32)
priv = Ed25519PrivateKey.from_private_bytes(seed)
pub_bytes = priv.public_key().public_bytes_raw()
print(base64.b64encode(seed).decode(), base64.b64encode(pub_bytes).decode())
PYEOF
)

    local database_url="postgresql://lynx_dashboard_app:${pg_pass}@lynx-dashboard-postgres:5432/lynx_dashboard"
    local redis_url="redis://:${redis_pass}@lynx-dashboard-redis:6379"

    # Persist secrets to Podman
    _create_secret lynx-dashboard-pg-root       "$pg_root"
    _create_secret lynx-dashboard-pg-pass       "$pg_pass"
    _create_secret lynx-dashboard-redis-pass    "$redis_pass"
    _create_secret lynx-dashboard-api-token     "$api_token"
    _create_secret lynx-dashboard-setup-token   "$setup_token"
    _create_secret lynx-dashboard-pepper        "$pepper"
    _create_secret lynx-dashboard-kek           "$kek"
    _create_secret lynx-dashboard-jwt-sign-private "$jwt_sign_priv"
    _create_secret lynx-dashboard-jwt-sign-public  "$jwt_sign_pub"
    _create_secret lynx-dashboard-jwt-enc-private  "$jwt_enc_priv"
    _create_secret lynx-dashboard-jwt-enc-public   "$jwt_enc_pub"
    _create_secret lynx-dashboard-ca-private    "$ca_priv"
    _create_secret lynx-dashboard-ca-public     "$ca_pub"
    _create_secret lynx-dashboard-database-url  "$database_url"
    _create_secret lynx-dashboard-redis-url     "$redis_url"

    # Save critical secrets to /etc/lynx/secrets (owner root, mode 600)
    # These MUST be backed up — without them, data is irrecoverable.
    mkdir -p "$SECRETS_DIR"
    chmod 700 "$SECRETS_DIR"
    printf '%s' "$kek"         > "${SECRETS_DIR}/lynx-dashboard-kek"
    printf '%s' "$pg_root"     > "${SECRETS_DIR}/lynx-dashboard-pg-root"
    printf '%s' "$pg_pass"     > "${SECRETS_DIR}/lynx-dashboard-pg-pass"
    printf '%s' "$ca_priv"     > "${SECRETS_DIR}/lynx-dashboard-ca-private"
    printf '%s' "$ca_pub"      > "${SECRETS_DIR}/lynx-dashboard-ca-public"
    printf '%s' "$jwt_sign_priv" > "${SECRETS_DIR}/lynx-dashboard-jwt-sign-private"
    printf '%s' "$jwt_sign_pub"  > "${SECRETS_DIR}/lynx-dashboard-jwt-sign-public"
    printf '%s' "$setup_token" > "${SECRETS_DIR}/lynx-dashboard-setup-token"
    chmod 600 "${SECRETS_DIR}"/*

    # Return setup token for display
    SETUP_TOKEN="$setup_token"
}

# --- Main install function ----------------------------------------------------

install_dashboard() {
    echo -e "${CYAN}Installing Lynx Dashboard...${RESET}"

    # Detect arch
    case "$(uname -m)" in
        x86_64)  ARCH="x86_64" ;;
        aarch64) ARCH="arm64" ;;
        *)
            echo -e "${RED}Unsupported architecture: $(uname -m)${RESET}" >&2
            exit 1
            ;;
    esac

    # Ensure dependencies
    _ensure_python_crypto
    _ensure_podman_compose
    _ensure_uuid_gen

    # Check existing installation
    if [[ -d "$DEPLOY_DIR" ]] || podman container exists lynx-dashboard-backend 2>/dev/null; then
        echo -e "${YELLOW}Existing installation detected.${RESET}"
        echo -e "  ${BOLD}1)${RESET} Abort (default)"
        echo -e "  ${BOLD}2)${RESET} Update → use auto-update instead"
        echo -e "  ${BOLD}3)${RESET} Reinstall clean"
        read -rp "Option [1/2/3]: " OPT
        case "${OPT:-1}" in
            3)
                echo -e "${YELLOW}Stopping existing containers...${RESET}"
                (cd "$DEPLOY_DIR" && _compose -f compose.yml down --volumes 2>/dev/null) || true
                rm -rf "$DEPLOY_DIR"
                ;;
            *)
                echo -e "${CYAN}Aborting. Use the dashboard auto-update for upgrades.${RESET}"
                exit 0
                ;;
        esac
    fi

    # Fetch latest release tag
    echo -e "${CYAN}Fetching latest dashboard release...${RESET}"
    LATEST_TAG=$(curl -fsSL "https://api.github.com/repos/${GITHUB_REPO}/releases" \
        | python3 -c "
import sys, json
releases = json.load(sys.stdin)
for r in releases:
    if r['tag_name'].startswith('dashboard@') and not r.get('prerelease', False):
        print(r['tag_name']); break
")
    if [[ -z "${LATEST_TAG:-}" ]]; then
        echo -e "${RED}Failed to find a dashboard release.${RESET}" >&2
        exit 1
    fi
    VERSION="${LATEST_TAG#dashboard@}"
    echo -e "${GREEN}Latest: ${BOLD}${LATEST_TAG}${RESET}"

    # Download and verify binaries
    echo -e "${CYAN}Downloading binaries...${RESET}"
    TMPDIR_DL=$(mktemp -d)
    trap 'rm -rf "$TMPDIR_DL"' EXIT

    RELEASE_BASE="https://github.com/${GITHUB_REPO}/releases/download/${LATEST_TAG}"
    _download_verify "$RELEASE_BASE" "lynx-dashboard-backend-linux-${ARCH}"               "$TMPDIR_DL"
    _download_verify "$RELEASE_BASE" "lynx-dashboard-frontend-linux-${ARCH}"              "$TMPDIR_DL"
    _download_verify "$RELEASE_BASE" "lynx-dashboard-frontend-assets-linux-${ARCH}.tar.gz" "$TMPDIR_DL"

    # Install binaries
    echo -e "${CYAN}Installing binaries...${RESET}"
    mkdir -p "$BIN_DIR" "$FRONTEND_DIR" "$DEPLOY_DIR"

    install -m 755 \
        "${TMPDIR_DL}/lynx-dashboard-backend-linux-${ARCH}" \
        "${BIN_DIR}/lynx-dashboard-backend"
    install -m 755 \
        "${TMPDIR_DL}/lynx-dashboard-frontend-linux-${ARCH}" \
        "${FRONTEND_DIR}/lynx-dashboard-frontend"
    tar -xzf "${TMPDIR_DL}/lynx-dashboard-frontend-assets-linux-${ARCH}.tar.gz" \
        -C "$FRONTEND_DIR"

    # Write version file so the backend scheduler knows the current version on startup.
    printf '%s' "$VERSION" > "${BIN_DIR}/dashboard-version"
    echo -e "${GREEN}Binaries installed.${RESET}"

    # Generate and create all secrets
    echo -e "${CYAN}Generating secrets...${RESET}"
    _gen_secrets
    echo -e "${GREEN}Secrets created.${RESET}"

    # Podman networks
    echo -e "${CYAN}Creating Podman networks...${RESET}"
    for net in lynx-dashboard-db lynx-dashboard-cache lynx-dashboard-app; do
        podman network exists "$net" 2>/dev/null || podman network create "$net"
    done

    # Deploy compose file and init SQL
    cp "${SCRIPT_DIR}/lynx/dashboard/docker-compose.yml" "${DEPLOY_DIR}/compose.yml"
    mkdir -p "${DEPLOY_DIR}/server/db/init"
    cp -r "${SCRIPT_DIR}/lynx/dashboard/server/db/init/." "${DEPLOY_DIR}/server/db/init/"

    # Fix relative path in compose to absolute
    sed -i "s|./server/db/init|${DEPLOY_DIR}/server/db/init|g" "${DEPLOY_DIR}/compose.yml"

    # Start containers
    echo -e "${CYAN}Starting containers...${RESET}"
    cd "$DEPLOY_DIR"
    _compose -f compose.yml up -d

    # Wait for services
    _wait_healthy lynx-dashboard-postgres 60
    _wait_healthy lynx-dashboard-redis    30
    _wait_healthy lynx-dashboard-backend  90
    _wait_healthy lynx-dashboard-frontend 60

    # Detect public IP
    VPS_IP=$(curl -fsSL --max-time 5 https://ifconfig.me 2>/dev/null \
        || hostname -I | awk '{print $1}')

    # Done
    echo
    echo -e "${GREEN}${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo -e "${GREEN}${BOLD} Lynx Dashboard ${VERSION} installed successfully!${RESET}"
    echo -e "${GREEN}${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo
    echo -e "${BOLD}Create your admin account:${RESET}"
    echo -e "  ${CYAN}https://${VPS_IP}:19443/register?setup_token=${SETUP_TOKEN}${RESET}"
    echo
    echo -e "${YELLOW}${BOLD}IMPORTANT — Back up these files now:${RESET}"
    echo -e "  ${YELLOW}${SECRETS_DIR}/${RESET}"
    echo -e "  Without KEK and pg-root, data is irrecoverable."
    echo
    echo -e "${BOLD}If something fails:${RESET}"
    echo -e "  ${CYAN}lynx-dashboard-backend logs --errors${RESET}"
    echo
}
