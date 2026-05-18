<div align="center">
  <img src="lynx/dashboard/ui/public/logo.webp" alt="Lynx" width="140" /><br /><br />

  # Lynx

  **Self-hosted VPS & container manager.**<br />
  Containers · Firewall · VPN — from one dashboard, across any number of servers.

  <br />

  [![CI — Agent](https://github.com/Jaro-c/Lynx/actions/workflows/agent.yml/badge.svg)](https://github.com/Jaro-c/Lynx/actions/workflows/agent.yml)
  [![CI — Dashboard](https://github.com/Jaro-c/Lynx/actions/workflows/dashboard-server.yml/badge.svg)](https://github.com/Jaro-c/Lynx/actions/workflows/dashboard-server.yml)
  [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
  ![Rust](https://img.shields.io/badge/Agent-Rust-orange?logo=rust)
  ![Next.js](https://img.shields.io/badge/Dashboard-Next.js-black?logo=next.js)

  <br />

  [Install](#-install) · [Architecture](#-architecture) · [Features](#-features) · [vs alternatives](#-vs-alternatives) · [Security](#-security)

</div>

---

> **The cPanel/Plesk/Coolify alternative built for people who care about security.**
> One binary per VPS. All traffic encrypted over WireGuard. No SaaS. No cloud lock-in. No Docker daemon.

---

## ✨ Features

<table>
<tr>
<td width="50%">

**🐳 Containers**
- Podman rootless — no daemon, no root processes
- Per-organization isolation with user namespaces
- Containers survive reboot independently of Lynx

</td>
<td width="50%">

**🔥 Firewall**
- Full nftables control from the dashboard
- Three-layer hierarchy: base invariants → global rules → per-VPS rules
- Auto-restore on any tampering — atomic apply

</td>
</tr>
<tr>
<td>

**🔒 Networking**
- All dashboard → agent traffic over WireGuard
- Per-agent PSK, mTLS on top of WireGuard
- Cross-VPS scaling via direct agent tunnels (no relay through dashboard)

</td>
<td>

**🔑 Encryption**
- PostgreSQL AES-256 at rest (pg_tde)
- Per-user envelope encryption (KEK/DEK)
- Ed25519 signed binary updates — verified before any swap

</td>
</tr>
<tr>
<td>

**📦 Single binary**
- Agent = one Rust binary. Dashboard backend = one Rust binary. Frontend = one compiled binary
- No Node.js, no Bun, no runtime deps on the server
- Install one binary, uninstall one binary

</td>
<td>

**🔄 Auto-update**
- Scheduler checks GitHub Releases every hour
- Atomic binary swap with automatic rollback if new binary fails to start
- Agents update themselves on `update.self` command — no SSH needed

</td>
</tr>
</table>

---

## 🏗 Architecture

```
Dashboard VPS
├── Frontend  ─────────────── Next.js (compiled binary, no runtime)
├── Backend   ─────────────── Rust
│    └── WireGuard ────────── Agent (local, same VPS)
│    └── WireGuard ────────── Agent (remote VPS #1)
│    └── WireGuard ────────── Agent (remote VPS #2)
│
└── All agents: Podman + nftables + WireGuard
```

Each agent is a **1:1 tunnel** with the dashboard. Agents never communicate with each other through the dashboard — cross-VPS scaling uses direct WireGuard tunnels between agents.

<details>
<summary><strong>Firewall hierarchy (nftables)</strong></summary>

```
table inet lynx-agent {
    chain lynx-base    ← Lynx invariants. Never editable. Auto-restored on any change.
    chain lynx-global  ← Rules pushed to ALL agents simultaneously
    chain lynx-local   ← Per-VPS rules for this agent only
}
```

- `lynx-base`: default deny, WireGuard allowlist, inter-org isolation, anti-spoofing
- `lynx-global`: IP blocklists, protocol restrictions — propagated to all agents in parallel
- `lynx-local`: per-VPS port rules, IP allowlists

</details>

<details>
<summary><strong>Scaling — horizontal cross-VPS</strong></summary>

```
Internet → 80/443
    ↓
lynx-nginx (Agent-1, entry point)
    ├── replica:1  (Agent-1, local)
    └── WireGuard data plane ──► Agent-2
                                     ├── replica:2
                                     └── replica:3
```

Agent-2 never exposes public ports for the project. Traffic enters only through Agent-1 via WireGuard.

</details>

---

## ⚡ Install

### Dashboard

```bash
curl -fsSL https://raw.githubusercontent.com/Jaro-c/Lynx/main/install.sh | sudo bash
```

The installer:
1. Detects and removes incompatible software (Docker, firewalld, ufw, iptables)
2. Installs Podman, WireGuard, nftables
3. Generates all secrets — never written to disk
4. Starts PostgreSQL → Redis → Backend → Frontend
5. Prints a one-time setup URL:
   ```
   https://IP:19443/register?setup_token=<token>
   ```

### Agent (additional VPS)

1. Dashboard → **Connect new VPS** → copy keypair + PSK
2. On the new VPS:
   ```bash
   curl -fsSL https://raw.githubusercontent.com/Jaro-c/Lynx/main/install.sh | sudo bash
   ```
3. Paste the dashboard data when prompted → done

### Requirements

- **OS:** Ubuntu 22.04+, Debian 12+, Fedora 39+, CentOS/RHEL 9+, Rocky/AlmaLinux 9+
- **Ports:** `22/TCP` (SSH) · `19443/TCP` (dashboard) · `51820/UDP` (WireGuard)
- **Root access** for install

---

## 🆚 vs Alternatives

| | **Lynx** | Coolify | Dokploy | cPanel / Plesk |
|---|---|---|---|---|
| Container runtime | Podman (rootless) | Docker | Docker | varies |
| Firewall control | ✅ Full nftables | ❌ | ❌ | Partial |
| VPN between servers | ✅ WireGuard | ❌ | ❌ | ❌ |
| Encryption at rest | ✅ AES-256 (pg_tde) | ❌ | ❌ | ❌ |
| Per-user encryption | ✅ KEK/DEK | ❌ | ❌ | ❌ |
| Signed binary updates | ✅ Ed25519 | ❌ | ❌ | ❌ |
| Runtime dependencies | None (single binary) | Docker Engine | Docker Engine | Heavy |
| Pricing | Free / self-hosted | Free tier + paid | Free / self-hosted | Paid license |
| SaaS / cloud | Never | Optional | Optional | Optional |

---

## 🔐 Security

<details>
<summary><strong>Transport security</strong></summary>

- **WireGuard** with per-agent PSK — all dashboard ↔ agent traffic
- **mTLS** on top of WireGuard — second barrier if WireGuard is compromised
- **TLS 1.3 minimum** — no TLS 1.0/1.1/1.2
- **Ed25519** for JWT signing, command signing, and binary verification

</details>

<details>
<summary><strong>Signed commands</strong></summary>

Every command sent to an agent is signed with Ed25519. The agent verifies:
- Valid signature
- Nonce not seen before (replay prevention)
- Timestamp < 30s old (replay prevention)
- Sufficient permission level for the action

Rejected commands are logged in the immutable audit log.

</details>

<details>
<summary><strong>Immutable audit log</strong></summary>

Hash-chained append-only log on every agent. Each entry includes the hash of the previous entry — tampering is mathematically detectable. Synced to dashboard PostgreSQL in real time.

</details>

<details>
<summary><strong>Reporting a vulnerability</strong></summary>

See [SECURITY.md](SECURITY.md).

</details>

---

## 🛠 Diagnostics

If something fails after install:

```bash
# Agent
lynx-agent logs --errors

# Dashboard backend
lynx-dashboard-backend logs --errors
```

---

## 📄 License

[MIT](LICENSE) — © 2026 [Jaro-c](https://github.com/Jaro-c)

<div align="center">
  <br />
  <sub>Made with ❤️ by <a href="https://github.com/Jaro-c">Jaroc</a></sub>
</div>
