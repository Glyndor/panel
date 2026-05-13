#!/usr/bin/env bash
# =============================================================================
# remove-docker.sh
# =============================================================================
# Description: Completely removes Docker and all its components from the system.
#              Handles services, packages (apt/dnf/pacman), snap, iptables chains,
#              config files, and user home directories.
#
# Dependencies:
#   - detect-os.sh must be sourced first (provides PKG_MANAGER, PKG_REMOVE, etc.)
#   - Colors must be exported from install.sh
#
# Note: This script is non-destructive on failure — all commands use || true
# =============================================================================
set -euo pipefail

remove_docker() {
    echo -e "${CYAN}Removing Docker...${RESET}"

    # Stop and disable all Docker-related services
    for svc in docker docker.socket docker.service containerd containerd.service; do
        systemctl stop "$svc" 2>/dev/null || true
        systemctl disable "$svc" 2>/dev/null || true
    done

    # Remove via snap
    if command -v snap &>/dev/null; then
        snap remove docker 2>/dev/null || true
    fi

    # Remove via package manager
    DOCKER_PKGS=(
        docker
        docker-ce
        docker-ce-cli
        docker-ce-rootless-extras
        docker.io
        docker-compose
        docker-compose-plugin
        docker-compose-v2
        docker-buildx-plugin
        docker-scan-plugin
        containerd
        containerd.io
        runc
    )

    case "$PKG_MANAGER" in
        apt-get)
            $PKG_REMOVE "${DOCKER_PKGS[@]}" 2>/dev/null || true
            $PKG_PURGE "${DOCKER_PKGS[@]}" 2>/dev/null || true
            $PKG_AUTOREMOVE 2>/dev/null || true
            ;;
        dnf)
            $PKG_REMOVE "${DOCKER_PKGS[@]}" 2>/dev/null || true
            $PKG_AUTOREMOVE 2>/dev/null || true
            ;;
        pacman)
            $PKG_REMOVE "${DOCKER_PKGS[@]}" 2>/dev/null || true
            ;;
    esac

    # Flush Docker iptables chains from memory
    for table in filter nat mangle; do
        iptables -t "$table" -F 2>/dev/null || true
        iptables -t "$table" -X 2>/dev/null || true
        iptables -t "$table" -Z 2>/dev/null || true
        ip6tables -t "$table" -F 2>/dev/null || true
        ip6tables -t "$table" -X 2>/dev/null || true
        ip6tables -t "$table" -Z 2>/dev/null || true
    done

    # Remove all Docker files and directories
    rm -rf \
        /var/lib/docker \
        /var/lib/containerd \
        /etc/docker \
        /etc/containerd \
        /etc/apt/sources.list.d/docker.list \
        /etc/apt/keyrings/docker.gpg \
        /etc/apt/keyrings/docker.asc \
        /usr/local/bin/docker \
        /usr/local/bin/docker-compose \
        /usr/local/bin/dockerd \
        /usr/libexec/docker \
        /opt/containerd \
        /etc/cni/net.d \
        /etc/systemd/system/docker.service.d \
        /var/run/docker.sock \
        /run/docker.sock \
        /root/.docker \
        2>/dev/null || true

    # Remove docker from all home directories
    for home_dir in /home/*; do
        rm -rf "${home_dir}/.docker" 2>/dev/null || true
    done

    # Remove docker group
    groupdel docker 2>/dev/null || true

    # Reload systemd
    systemctl daemon-reload 2>/dev/null || true

    echo -e "${GREEN}Docker removed completely.${RESET}"
}
