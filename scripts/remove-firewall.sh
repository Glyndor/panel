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

    # Delete all nftables tables created by ufw / iptables-nft (ip/ip6 families).
    # On Ubuntu 24.04+, iptables is iptables-nft — its tables live in nftables.
    # Delete entirely so only table inet lynx-agent remains after install.
    for _nft_table in \
        "ip filter" "ip nat" "ip mangle" "ip raw" "ip security" \
        "ip6 filter" "ip6 nat" "ip6 mangle" "ip6 raw" "ip6 security" \
        "bridge filter" "arp filter"; do
        nft delete table "$_nft_table" 2>/dev/null || true
    done

    # Legacy iptables kernel module cleanup — older distros only, not Ubuntu 24.04+.
    for _ipt in iptables-legacy ip6tables-legacy; do
        if command -v "$_ipt" &>/dev/null; then
            "$_ipt" -P INPUT   ACCEPT 2>/dev/null || true
            "$_ipt" -P FORWARD ACCEPT 2>/dev/null || true
            "$_ipt" -P OUTPUT  ACCEPT 2>/dev/null || true
            "$_ipt" -F              2>/dev/null || true
            "$_ipt" -X              2>/dev/null || true
            "$_ipt" -t nat    -F    2>/dev/null || true
            "$_ipt" -t nat    -X    2>/dev/null || true
            "$_ipt" -t mangle -F    2>/dev/null || true
            "$_ipt" -t mangle -X    2>/dev/null || true
        fi
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
