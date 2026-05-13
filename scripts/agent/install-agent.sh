#!/usr/bin/env bash
# =============================================================================
# install-agent.sh
# =============================================================================
# Description: Installs the Lynx Agent on this VPS.
#              Handles fresh installation and version updates via auto-update
#              system that queries the Dashboard for the latest binary.
#              Installs to /opt/lynx/agent.
#
# Dependencies:
#   - detect-os.sh must be sourced first (provides PKG_MANAGER, PKG_INSTALL, etc.)
#   - install-podman.sh must run first (Podman required)
#   - install-nftables.sh must run first (nftables required)
#   - Colors must be exported from install.sh
# =============================================================================
set -euo pipefail

AGENT_DIR="/opt/lynx/agent"

install_agent() {
    echo -e "${CYAN}Installing Lynx Agent...${RESET}"

    if [[ -d "$AGENT_DIR" ]]; then
        echo -e "${YELLOW}Existing installation detected at ${BOLD}${AGENT_DIR}${RESET}"
        echo -e "${CYAN}Checking version...${RESET}"
        # TODO: version check and update logic
    else
        echo -e "${CYAN}No existing installation found. Proceeding with fresh install...${RESET}"
        mkdir -p "$AGENT_DIR"
        # TODO: fresh install logic
    fi
}
