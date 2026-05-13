#!/usr/bin/env bash
# =============================================================================
# install-podman.sh
# =============================================================================
# Description: Installs Podman as the container runtime for Lynx.
#              Enables the Podman socket for API compatibility.
#
# Dependencies:
#   - detect-os.sh must be sourced first (provides PKG_MANAGER, PKG_INSTALL, etc.)
#   - Colors must be exported from install.sh
# =============================================================================
set -euo pipefail

install_podman() {
    echo -e "${CYAN}Installing Podman...${RESET}"

    # Skip if already installed
    if command -v podman &>/dev/null; then
        EXISTING_VERSION=$(podman --version)
        echo -e "${YELLOW}Podman already installed: ${BOLD}${EXISTING_VERSION}${RESET}"
        echo -e "${CYAN}Skipping installation, configuring registries...${RESET}"
    else
        # Update package index
        eval "$PKG_UPDATE"

    case "$PKG_MANAGER" in
        apt-get)
            $PKG_INSTALL podman
            ;;
        dnf)
            $PKG_INSTALL podman
            ;;
        pacman)
            $PKG_INSTALL podman
            ;;
        *)
            echo -e "${RED}Error: unsupported package manager: ${PKG_MANAGER}${RESET}" >&2
            exit 1
            ;;
        esac
    fi

    # Enable and start Podman socket
    systemctl enable --now podman.socket

    # -----------------------------------------------------------------------------
    # Registries
    # -----------------------------------------------------------------------------
    echo -e "${CYAN}Configuring Podman registries...${RESET}"
    mkdir -p /etc/containers
    cat > /etc/containers/registries.conf <<'EOF'
# Lynx — Podman registry configuration
# Base registries — organizations can add private registries via the dashboard.

unqualified-search-registries = ["docker.io", "ghcr.io", "quay.io"]

[[registry]]
prefix = "docker.io"
location = "docker.io"

[[registry]]
prefix = "ghcr.io"
location = "ghcr.io"

[[registry]]
prefix = "quay.io"
location = "quay.io"
EOF
    echo -e "${GREEN}Registries configured: docker.io, ghcr.io, quay.io${RESET}"

    # -----------------------------------------------------------------------------
    # Storage
    # -----------------------------------------------------------------------------
    echo -e "${CYAN}Configuring Podman storage...${RESET}"
    mkdir -p /opt/lynx/storage
    cat > /etc/containers/storage.conf <<'EOF'
# Lynx — Podman storage configuration
# Dedicated storage path to avoid filling the system root disk.

[storage]
driver = "overlay"
runroot = "/run/containers/storage"
graphroot = "/opt/lynx/storage"

[storage.options]
additionalimagestores = []

[storage.options.overlay]
mountopt = "nodev,metacopy=on"
EOF
    echo -e "${GREEN}Storage configured at: /opt/lynx/storage${RESET}"

    # -----------------------------------------------------------------------------
    # Logging
    # -----------------------------------------------------------------------------
    echo -e "${CYAN}Configuring Podman logging...${RESET}"
    mkdir -p /etc/containers
    cat > /etc/containers/containers.conf <<'EOF'
# Lynx — Podman containers configuration
# Logs go to journald so the Lynx dashboard can read them via systemd.

[containers]
log_driver = "journald"

[engine]
# Rootless containers are managed per-organization by the dashboard
cgroup_manager = "systemd"
events_logger = "journald"
EOF
    echo -e "${GREEN}Logging configured: journald${RESET}"

    # -----------------------------------------------------------------------------
    # Image policy
    # -----------------------------------------------------------------------------
    echo -e "${CYAN}Configuring image policy...${RESET}"
    cat > /etc/containers/policy.json <<'EOF'
{
    "default": [
        {
            "type": "insecureAcceptAnything"
        }
    ],
    "transports": {
        "docker": {},
        "docker-daemon": {
            "": [{ "type": "insecureAcceptAnything" }]
        }
    }
}
EOF
    echo -e "${GREEN}Image policy configured.${RESET}"

    # -----------------------------------------------------------------------------
    # Verify
    # -----------------------------------------------------------------------------
    if ! command -v podman &>/dev/null; then
        echo -e "${RED}Error: Podman installation failed.${RESET}" >&2
        exit 1
    fi

    PODMAN_VERSION=$(podman --version)
    echo -e "${GREEN}Podman installed successfully: ${BOLD}${PODMAN_VERSION}${RESET}"
}
