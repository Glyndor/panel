# Lynx

[![CI — Agent](https://github.com/Jaro-c/Lynx/actions/workflows/agent.yml/badge.svg)](https://github.com/Jaro-c/Lynx/actions/workflows/agent.yml)
[![CI — Dashboard](https://github.com/Jaro-c/Lynx/actions/workflows/dashboard-server.yml/badge.svg)](https://github.com/Jaro-c/Lynx/actions/workflows/dashboard-server.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Distributed infrastructure orchestrator. Self-hosted. No SaaS, no cloud lock-in.

Manage containers, firewall rules, and networking across multiple VPS instances from a single dashboard — all traffic over WireGuard.

---

## Components

| Component | Description |
|-----------|-------------|
| **Dashboard** | Next.js UI + Rust backend. PostgreSQL (AES-256 at rest). Runs on your VPS. |
| **Agent** | Rust binary. Manages Podman containers, nftables, and WireGuard on each VPS. |

```
Dashboard VPS
├── Frontend (Next.js)
├── Backend (Rust)  ──── WireGuard ──── Agent (local, same VPS)
└── WireGuard hub   ──── WireGuard ──── Agent (remote VPS)
                    ──── WireGuard ──── Agent (remote VPS)
```

All dashboard → agent traffic goes over WireGuard. No exceptions.

---

## Features

- **Containers** — Podman rootless, per-organization isolation, linger support (survive reboot without agent)
- **Firewall** — nftables with three-layer hierarchy: base invariants, global rules, per-VPS rules. Atomic apply, auto-restore on divergence.
- **Networking** — WireGuard tunnels with per-agent PSK. Horizontal scaling across VPS instances without routing through the dashboard.
- **Encryption** — PostgreSQL TDE (AES-256) + per-user envelope encryption (KEK/DEK)
- **Auto-update** — Scheduler checks GitHub Releases hourly. Ed25519 signature verification before any binary swap. Atomic swap with automatic rollback.
- **Scaling** — Vertical (CPU/RAM quotas), horizontal same-VPS (nginx load balancer), horizontal cross-VPS (direct WireGuard tunnel between agents)

---

## Requirements

- Linux VPS: Ubuntu 22.04+, Debian 12+, Fedora 39+, CentOS/RHEL 9+, Rocky/AlmaLinux 9+
- Root access for install
- Ports: `22/TCP` (SSH), `19443/TCP` (dashboard), `51820/UDP` (WireGuard)

**Incompatible software** (installer removes automatically):
- Docker / Docker Engine
- firewalld, ufw, iptables (direct)
- containerd (standalone)

---

## Install

### Dashboard

```bash
curl -fsSL https://raw.githubusercontent.com/Jaro-c/Lynx/main/install.sh | sudo bash
```

The installer will:
1. Detect and remove incompatible software
2. Install Podman, WireGuard, nftables
3. Generate all secrets (never written to disk)
4. Start PostgreSQL → Redis → Backend → Frontend
5. Print a one-time setup URL: `https://IP:19443/register?setup_token=<token>`

### Agent (additional VPS)

1. In the dashboard: **Connect new VPS** → copy the displayed keypair + PSK
2. On the new VPS:
```bash
curl -fsSL https://raw.githubusercontent.com/Jaro-c/Lynx/main/install.sh | sudo bash
```
3. Paste the dashboard data when prompted
4. Copy the agent's public key back into the dashboard

---

## Security

To report a vulnerability, see [SECURITY.md](SECURITY.md).

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## License

[MIT](LICENSE) — © 2026 Jaro-c
