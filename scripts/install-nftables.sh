#!/usr/bin/env bash
# =============================================================================
# install-nftables.sh
# =============================================================================
# Description: Installs nftables as the firewall for Lynx.
#              Applies a secure base ruleset that allows SSH and blocks
#              everything else by default. Lynx will manage rules from here.
#
# Dependencies:
#   - detect-os.sh must be sourced first (provides PKG_MANAGER, PKG_INSTALL, etc.)
#   - Colors must be exported from install.sh
# =============================================================================
set -euo pipefail

install_nftables() {
    echo -e "${CYAN}Installing nftables...${RESET}"

    # Update package index
    eval "$PKG_UPDATE"

    case "$PKG_MANAGER" in
        apt-get)
            $PKG_INSTALL nftables
            ;;
        dnf)
            $PKG_INSTALL nftables
            ;;
        pacman)
            $PKG_INSTALL nftables
            ;;
        *)
            echo -e "${RED}Error: unsupported package manager: ${PKG_MANAGER}${RESET}" >&2
            exit 1
            ;;
    esac

    # Apply base ruleset — allow SSH, drop everything else
    nft flush ruleset
    nft add table inet filter
    nft add chain inet filter input  '{ type filter hook input priority 0; policy drop; }'
    nft add chain inet filter forward '{ type filter hook forward priority 0; policy drop; }'
    nft add chain inet filter output '{ type filter hook output priority 0; policy accept; }'

    # Allow established and related connections
    nft add rule inet filter input ct state established,related accept

    # Allow loopback
    nft add rule inet filter input iif lo accept

    # Allow SSH (port 22)
    nft add rule inet filter input tcp dport 22 accept

    # Save ruleset
    nft list ruleset > /etc/nftables.conf

    # Enable and start nftables
    systemctl enable --now nftables

    # Verify installation
    if ! command -v nft &>/dev/null; then
        echo -e "${RED}Error: nftables installation failed.${RESET}" >&2
        exit 1
    fi

    NFT_VERSION=$(nft --version)
    echo -e "${GREEN}nftables installed successfully: ${BOLD}${NFT_VERSION}${RESET}"
    echo -e "${YELLOW}Base ruleset applied: SSH allowed, everything else blocked.${RESET}"
}
