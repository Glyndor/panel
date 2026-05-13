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

    # Skip if already installed
    if command -v nft &>/dev/null; then
        EXISTING_VERSION=$(nft --version)
        echo -e "${YELLOW}nftables already installed: ${BOLD}${EXISTING_VERSION}${RESET}"
        echo -e "${CYAN}Skipping installation, applying base ruleset...${RESET}"
    else
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
    fi

    # -----------------------------------------------------------------------------
    # Detect SSH port
    # -----------------------------------------------------------------------------
    SSH_PORT=$(grep -E "^Port " /etc/ssh/sshd_config 2>/dev/null | awk '{print $2}' | head -1 || true)
    SSH_PORT="${SSH_PORT:-22}"
    echo -e "${CYAN}Detected SSH port: ${BOLD}${SSH_PORT}${RESET}"
    echo -e "${YELLOW}This port will be allowed through the firewall.${RESET}"

    # -----------------------------------------------------------------------------
    # Base ruleset — applied atomically to avoid firewall gap
    # -----------------------------------------------------------------------------
    echo -e "${CYAN}Applying base nftables ruleset...${RESET}"

    cat > /tmp/lynx-nftables.conf << EOF
flush ruleset

table inet filter {
    chain input {
        type filter hook input priority 0; policy drop;
        ct state established,related accept
        iif lo accept
        tcp dport $SSH_PORT accept
    }
    chain forward {
        type filter hook forward priority 0; policy drop;
    }
    chain output {
        type filter hook output priority 0; policy accept;
    }
}
EOF

    # Apply atomically — kernel loads all rules at once, no gap
    nft -f /tmp/lynx-nftables.conf
    rm -f /tmp/lynx-nftables.conf

    # Save ruleset
    nft list ruleset > /etc/nftables.conf

    # Enable and start nftables
    if [[ "$(cat /proc/1/comm 2>/dev/null)" == "systemd" ]]; then
        systemctl enable --now nftables
    else
        echo -e "${YELLOW}Warning: systemd not running as PID 1. Skipping service activation.${RESET}"
        echo -e "${YELLOW}Run after boot: systemctl enable --now nftables${RESET}"
    fi

    # Verify installation
    if ! command -v nft &>/dev/null; then
        echo -e "${RED}Error: nftables installation failed.${RESET}" >&2
        exit 1
    fi

    NFT_VERSION=$(nft --version)
    echo -e "${GREEN}nftables installed successfully: ${BOLD}${NFT_VERSION}${RESET}"
    echo -e "${YELLOW}Base ruleset applied: SSH allowed, everything else blocked.${RESET}"
}
