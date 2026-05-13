#!/usr/bin/env bash
# =============================================================================
# Lynx Installer
# =============================================================================
# Description: Master orchestrator for installing Lynx components.
#              Supports Dashboard and Agent installation.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Jaro-c/Lynx/main/install.sh | sudo bash
#   sudo bash install.sh
#
# Requirements:
#   - Must be run as root
#   - Supported OS: Ubuntu, Debian, Fedora, CentOS, RHEL, Rocky, AlmaLinux, Arch, Manjaro
# =============================================================================
set -euo pipefail

# -----------------------------------------------------------------------------
# Colors
# -----------------------------------------------------------------------------
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

export RED YELLOW GREEN CYAN BOLD RESET

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# -----------------------------------------------------------------------------
# Root check
# -----------------------------------------------------------------------------
if [[ "$EUID" -ne 0 ]]; then
    echo -e "${RED}Error: this script must be run as root.${RESET}" >&2
    echo -e "${YELLOW}Use: curl -fsSL https://raw.githubusercontent.com/Jaro-c/Lynx/main/install.sh | sudo bash${RESET}" >&2
    exit 1
fi

# -----------------------------------------------------------------------------
# Detect OS
# -----------------------------------------------------------------------------
source "$SCRIPT_DIR/scripts/detect-os.sh"
detect_os
echo -e "${CYAN}Detected OS: ${BOLD}${OS_NAME}${RESET} — using ${BOLD}${PKG_MANAGER}${RESET}"
echo

# -----------------------------------------------------------------------------
# Menu
# -----------------------------------------------------------------------------
echo -e "${BOLD}${CYAN}Lynx Installer${RESET}"
echo -e "Select what to install:\n"
echo -e "  ${BOLD}1)${RESET} Dashboard — installs the Lynx dashboard on this VPS"
echo -e "  ${BOLD}2)${RESET} Agent     — installs the Lynx agent on this VPS"
echo

read -rp "Option [1/2] (default: 1): " OPTION
OPTION="${OPTION:-1}"

case "$OPTION" in
    1)
        echo -e "\n${GREEN}Starting Dashboard installation...${RESET}"
        ;;
    2)
        echo -e "\n${GREEN}Starting Agent installation...${RESET}"
        ;;
    *)
        echo -e "${RED}Invalid option. Exiting.${RESET}" >&2
        exit 1
        ;;
esac
