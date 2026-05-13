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

    # Enable and start Podman socket
    systemctl enable --now podman.socket

    # Verify installation
    if ! command -v podman &>/dev/null; then
        echo -e "${RED}Error: Podman installation failed.${RESET}" >&2
        exit 1
    fi

    PODMAN_VERSION=$(podman --version)
    echo -e "${GREEN}Podman installed successfully: ${BOLD}${PODMAN_VERSION}${RESET}"
}
