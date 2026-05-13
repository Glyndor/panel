#!/usr/bin/env bash
# =============================================================================
# install-dashboard.sh
# =============================================================================
# Description: Installs the Lynx Dashboard on this VPS.
#              Handles fresh installation and version updates.
#              Installs to /opt/lynx/dashboard.
#
# Dependencies:
#   - detect-os.sh must be sourced first (provides PKG_MANAGER, PKG_INSTALL, etc.)
#   - install-podman.sh must run first (Podman required)
#   - install-nftables.sh must run first (nftables required)
#   - Colors must be exported from install.sh
# =============================================================================
set -euo pipefail

DASHBOARD_DIR="/opt/lynx/dashboard"

install_dashboard() {
    echo -e "${CYAN}Installing Lynx Dashboard...${RESET}"

    if [[ -d "$DASHBOARD_DIR" ]]; then
        echo -e "${YELLOW}Existing installation detected at ${BOLD}${DASHBOARD_DIR}${RESET}"
        echo -e "${CYAN}Checking version...${RESET}"
        # TODO: version check and update logic
    else
        echo -e "${CYAN}No existing installation found. Proceeding with fresh install...${RESET}"
        mkdir -p "$DASHBOARD_DIR"
        # TODO: fresh install logic
    fi
}
