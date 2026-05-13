#!/usr/bin/env bash
# =============================================================================
# remove-firewall.sh
# =============================================================================
# Description: Completely removes ufw and iptables from the system.
#              Flushes all rules from memory, resets default policies to ACCEPT,
#              removes all config files and network hooks.
#
# Dependencies:
#   - detect-os.sh must be sourced first (provides PKG_MANAGER, PKG_REMOVE, etc.)
#   - Colors must be exported from install.sh
#
# Note: This script is non-destructive on failure — all commands use || true
# =============================================================================
set -euo pipefail

remove_firewall() {
    echo -e "${CYAN}Removing ufw and iptables...${RESET}"

    # Stop and disable ufw
    systemctl stop ufw 2>/dev/null || true
    systemctl disable ufw 2>/dev/null || true
    ufw disable 2>/dev/null || true

    # Remove via package manager
    UFW_PKGS=(ufw gufw)
    IPTABLES_PKGS=(iptables iptables-persistent iptables-services netfilter-persistent)

    case "$PKG_MANAGER" in
        apt-get)
            $PKG_REMOVE "${UFW_PKGS[@]}" "${IPTABLES_PKGS[@]}" 2>/dev/null || true
            $PKG_PURGE "${UFW_PKGS[@]}" "${IPTABLES_PKGS[@]}" 2>/dev/null || true
            $PKG_AUTOREMOVE 2>/dev/null || true
            ;;
        dnf)
            $PKG_REMOVE "${UFW_PKGS[@]}" "${IPTABLES_PKGS[@]}" 2>/dev/null || true
            $PKG_AUTOREMOVE 2>/dev/null || true
            ;;
        pacman)
            $PKG_REMOVE "${UFW_PKGS[@]}" "${IPTABLES_PKGS[@]}" 2>/dev/null || true
            ;;
    esac

    # Flush all iptables rules from memory
    for table in filter nat mangle raw security; do
        iptables -t "$table" -F 2>/dev/null || true
        iptables -t "$table" -X 2>/dev/null || true
        iptables -t "$table" -Z 2>/dev/null || true
        ip6tables -t "$table" -F 2>/dev/null || true
        ip6tables -t "$table" -X 2>/dev/null || true
        ip6tables -t "$table" -Z 2>/dev/null || true
    done

    # Reset default policies to ACCEPT
    for chain in INPUT FORWARD OUTPUT; do
        iptables -P "$chain" ACCEPT 2>/dev/null || true
        ip6tables -P "$chain" ACCEPT 2>/dev/null || true
    done

    # Remove all config files
    rm -rf \
        /etc/ufw \
        /etc/iptables \
        /etc/iptables.rules \
        /etc/iptables/rules.v4 \
        /etc/iptables/rules.v6 \
        /etc/sysconfig/iptables \
        /etc/sysconfig/ip6tables \
        /lib/ufw \
        /var/lib/ufw \
        2>/dev/null || true

    # Remove saved rules from network hooks
    rm -f \
        /etc/network/if-pre-up.d/iptables \
        /etc/network/if-up.d/ufw \
        /etc/network/if-down.d/ufw \
        2>/dev/null || true

    systemctl daemon-reload 2>/dev/null || true

    echo -e "${GREEN}ufw and iptables removed completely.${RESET}"
}
