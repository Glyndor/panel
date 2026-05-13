#!/usr/bin/env bash
# =============================================================================
# detect-os.sh
# =============================================================================
# Description: Detects the current Linux distribution and sets the appropriate
#              package manager variables for use by other installer scripts.
#
# Exports:
#   OS_ID         — distro ID (e.g. ubuntu, debian, fedora)
#   OS_VERSION    — distro version (e.g. 22.04)
#   OS_NAME       — full distro name (e.g. "Ubuntu 22.04.3 LTS")
#   PKG_MANAGER   — package manager binary (e.g. apt-get, dnf, pacman)
#   PKG_INSTALL   — install command
#   PKG_REMOVE    — remove command
#   PKG_PURGE     — purge command
#   PKG_AUTOREMOVE— autoremove command
#   PKG_UPDATE    — update package index command
#
# Supported OS: Ubuntu, Debian, Raspbian, Fedora, CentOS, RHEL,
#               Rocky, AlmaLinux, Arch, Manjaro
# =============================================================================
set -euo pipefail

detect_os() {
    if [[ ! -f /etc/os-release ]]; then
        echo -e "${RED}Error: unable to detect operating system.${RESET}" >&2
        exit 1
    fi

    source /etc/os-release

    OS_ID="${ID}"
    OS_VERSION="${VERSION_ID:-unknown}"
    OS_NAME="${PRETTY_NAME:-unknown}"

    case "$OS_ID" in
        ubuntu|debian|raspbian)
            PKG_MANAGER="apt-get"
            PKG_INSTALL="apt-get install -y"
            PKG_REMOVE="apt-get remove -y"
            PKG_PURGE="apt-get purge -y"
            PKG_AUTOREMOVE="apt-get autoremove -y"
            PKG_UPDATE="apt-get update -y"
            ;;
        fedora)
            PKG_MANAGER="dnf"
            PKG_INSTALL="dnf install -y"
            PKG_REMOVE="dnf remove -y"
            PKG_PURGE="dnf remove -y"
            PKG_AUTOREMOVE="dnf autoremove -y"
            PKG_UPDATE="dnf check-update -y || true"
            ;;
        centos|rhel|rocky|almalinux)
            PKG_MANAGER="dnf"
            PKG_INSTALL="dnf install -y"
            PKG_REMOVE="dnf remove -y"
            PKG_PURGE="dnf remove -y"
            PKG_AUTOREMOVE="dnf autoremove -y"
            PKG_UPDATE="dnf check-update -y || true"
            ;;
        arch|manjaro)
            PKG_MANAGER="pacman"
            PKG_INSTALL="pacman -S --noconfirm"
            PKG_REMOVE="pacman -R --noconfirm"
            PKG_PURGE="pacman -Rns --noconfirm"
            PKG_AUTOREMOVE="pacman -Rns --noconfirm \$(pacman -Qdtq) 2>/dev/null || true"
            PKG_UPDATE="pacman -Sy"
            ;;
        *)
            echo -e "${RED}Error: unsupported operating system: ${OS_NAME}${RESET}" >&2
            echo -e "${YELLOW}Supported: Ubuntu, Debian, Fedora, CentOS, RHEL, Rocky, AlmaLinux, Arch, Manjaro${RESET}" >&2
            exit 1
            ;;
    esac

    export OS_ID OS_VERSION OS_NAME PKG_MANAGER PKG_INSTALL PKG_REMOVE PKG_PURGE PKG_AUTOREMOVE PKG_UPDATE
}
