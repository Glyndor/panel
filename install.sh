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
    1|2)
        if [[ "$OPTION" == "1" ]]; then
            echo -e "\n${GREEN}Starting Dashboard installation...${RESET}"
        else
            echo -e "\n${GREEN}Starting Agent installation...${RESET}"
        fi

        echo
        echo -e "${RED}${BOLD}IMPORTANT:${RESET} Before proceeding, make sure you have backed up:"
        echo -e "  ${YELLOW}•${RESET} Docker volumes and container data"
        echo -e "  ${YELLOW}•${RESET} Current firewall rules (ufw/iptables/nftables)"
        echo -e "  ${YELLOW}•${RESET} Existing Podman images, containers and volumes"
        echo -e "  ${YELLOW}•${RESET} Any other data you want to keep"
        echo -e "${RED}Everything will be permanently deleted or overwritten. We are not responsible for data loss.${RESET}"
        echo
        read -rp "I have made a backup and want to continue [y/N]: " BACKUP_CONFIRM
        BACKUP_CONFIRM="${BACKUP_CONFIRM:-N}"

        if [[ ! "$BACKUP_CONFIRM" =~ ^[yY]$ ]]; then
            echo -e "${RED}Installation cancelled. Please make a backup first.${RESET}"
            exit 0
        fi

        echo
        echo -e "${YELLOW}${BOLD}WARNING:${RESET} This installer will make the following changes to your system:"
        echo -e "  ${RED}✖${RESET} Remove Docker and all its components completely (including configs from all user home directories)"
        echo -e "  ${RED}✖${RESET} Remove ufw and iptables completely"
        echo -e "  ${GREEN}✔${RESET} Install Podman as container runtime"
        echo -e "  ${GREEN}✔${RESET} Install nftables as firewall"
        echo

        read -rp "Do you want to proceed? [y/N]: " CONFIRM
        CONFIRM="${CONFIRM:-N}"

        if [[ ! "$CONFIRM" =~ ^[yY]$ ]]; then
            echo -e "${RED}Installation cancelled.${RESET}"
            exit 0
        fi

        if [[ "$OPTION" == "1" ]]; then
            exec "$SCRIPT_DIR/lynx/dashboard/setup-dashboard.sh"
        else
            exec "$SCRIPT_DIR/lynx/agent/setup-agent.sh"
        fi
        ;;
    *)
        echo -e "${RED}Invalid option. Exiting.${RESET}" >&2
        exit 1
        ;;
esac
