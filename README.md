<div align="center">
  <img src="lynx/dashboard/ui/public/logo.webp" alt="Lynx" width="140" /><br /><br />

  # Lynx

  **Self-hosted VPS & container manager.**<br />
  Containers · Firewall · VPN — from one dashboard, across any number of servers.

  <br />

  [![CI — Agent](https://github.com/Jaro-c/Lynx/actions/workflows/agent.yml/badge.svg)](https://github.com/Jaro-c/Lynx/actions/workflows/agent.yml)
  [![CI — Dashboard](https://github.com/Jaro-c/Lynx/actions/workflows/dashboard-server.yml/badge.svg)](https://github.com/Jaro-c/Lynx/actions/workflows/dashboard-server.yml)
  ![Rust](https://img.shields.io/badge/Agent-Rust-orange?logo=rust)
  ![Next.js](https://img.shields.io/badge/Dashboard-Next.js-black?logo=next.js)

  <br />

  [Install](#-install) · [Architecture](#-architecture) · [vs Alternatives](#-vs-alternatives) · [Security](#-security)

</div>

---

> **The cPanel/Plesk/Coolify alternative built for people who care about security.**  
> One binary per VPS. All traffic encrypted over WireGuard. No SaaS. No cloud lock-in. No Docker daemon.

---

## ✨ Features

**📦 Containers** — Podman rootless, per-organization isolation, survive VPS reboots without Lynx running  
**🔥 Firewall** — Full nftables control from the dashboard, three-layer hierarchy, atomic apply, auto-restore on any tampering  
**🔒 Networking** — All dashboard → agent traffic over WireGuard + mTLS. Cross-VPS scaling via direct agent tunnels — no relay through dashboard  
**🔑 Encryption** — PostgreSQL AES-256 at rest (pg_tde) + per-user envelope encryption (KEK/DEK)  
**📁 Single binary** — No runtime dependencies on the server. No Node.js, no Bun, no Docker Engine. Install one binary, uninstall one binary  
**🔄 Auto-update** — Hourly GitHub Releases check, Ed25519 signature verification before any swap, automatic rollback if the new binary fails to start

---

## 🏗 Architecture

```
Dashboard VPS
├── Frontend ── Next.js (compiled binary, no runtime)
├── Backend  ── Rust
│    ├── WireGuard ──► Agent (local, same VPS)
│    ├── WireGuard ──► Agent (remote VPS #1)
│    └── WireGuard ──► Agent (remote VPS #2)
│
└── Each agent: Podman + nftables + WireGuard
```

Each agent connects to the dashboard over a **1:1 WireGuard tunnel** with its own PSK. Agents never talk to each other through the dashboard — cross-VPS scaling uses direct agent-to-agent tunnels.

<details>
<summary><strong>Firewall hierarchy (nftables)</strong></summary>
<br />

```
table inet lynx-agent {
    chain lynx-base    ← Lynx invariants. Never editable. Auto-restored instantly on any change.
    chain lynx-global  ← Rules pushed to ALL agents simultaneously
    chain lynx-local   ← Per-VPS rules for this agent only
}
```

- **`lynx-base`** — default deny, WireGuard allowlist, inter-org isolation, anti-spoofing
- **`lynx-global`** — IP blocklists, protocol restrictions — propagated to all agents in parallel; agents offline receive pending rules on reconnect
- **`lynx-local`** — per-VPS port rules, IP allowlists

</details>

<details>
<summary><strong>Horizontal scaling — cross-VPS</strong></summary>
<br />

```
Internet → 80/443
    ↓
lynx-nginx (Agent-1, entry point)
    ├── replica:1  (Agent-1, local Podman network)
    └── WireGuard ──► Agent-2
                          ├── replica:2
                          └── replica:3
```

Agent-2 never exposes public ports for the project. All traffic enters through Agent-1 via WireGuard.

</details>

---

## ⚡ Install

### Dashboard

```bash
curl -fsSL https://raw.githubusercontent.com/Glyndor/panel/main/install.sh | sudo bash
```

The installer handles everything:
1. Detects and removes incompatible software (Docker, firewalld, ufw, iptables)
2. Installs Podman, WireGuard, nftables
3. Generates all secrets — never written to disk in plaintext
4. Starts PostgreSQL → Redis → Backend → Frontend
5. Prints a one-time setup URL:
   ```
   https://YOUR-IP:19443/register?setup_token=<token>
   ```

### Agent (additional VPS)

1. Dashboard → **Connect new VPS** → copy the displayed keypair + PSK
2. On the new VPS, run the same installer and paste the dashboard data when prompted
3. Done — the tunnel is up and the agent appears online in the dashboard

### Requirements

| | |
|---|---|
| **OS** | Ubuntu 22.04+, Debian 12+, Fedora 39+, CentOS/RHEL 9+, Rocky/AlmaLinux 9+ |
| **SSH port** | Auto-detected — any port works |
| **Fixed ports** | `19443/TCP` (dashboard) · `51820/UDP` (WireGuard) — opened automatically. Must be free and allowed by your VPS provider's external firewall if applicable. |
| **Root access** | Required for install |

---

## 🆚 vs Alternatives

| | **Lynx** | Coolify | Dokploy | cPanel / Plesk |
|---|---|---|---|---|
| Container runtime | Podman (rootless) | Docker | Docker | varies |
| Firewall management | ✅ Full nftables | ❌ | ❌ | Partial |
| VPN between servers | ✅ WireGuard | ❌ | ❌ | ❌ |
| Encryption at rest | ✅ AES-256 (pg_tde) | ❌ | ❌ | ❌ |
| Per-user encryption | ✅ KEK/DEK | ❌ | ❌ | ❌ |
| Signed binary updates | ✅ Ed25519 | ❌ | ❌ | ❌ |
| Runtime dependencies | None | Docker Engine | Docker Engine | Heavy |
| Pricing | Free / self-hosted | Free tier + paid | Free / self-hosted | Paid license |
| SaaS / cloud | Never | Optional | Optional | Optional |

---

## 🔐 Security

<details>
<summary><strong>Transport &amp; cryptography</strong></summary>
<br />

- **WireGuard + mTLS** — double-layer encryption on all dashboard ↔ agent traffic
- **TLS 1.3 minimum** — no TLS 1.0/1.1/1.2 accepted anywhere
- **Ed25519** — JWT signing, agent command signing, and binary update verification
- **Per-agent PSK** — each tunnel has its own unique preshared key, rotated automatically

</details>

<details>
<summary><strong>Signed commands &amp; immutable audit log</strong></summary>
<br />

Every command the dashboard sends to an agent is Ed25519-signed. The agent verifies signature, nonce (replay prevention), and timestamp (< 30s window) before executing anything.

All executed and rejected commands are stored in a **hash-chained append-only audit log** on the agent, synced to dashboard PostgreSQL in real time. Tampering with any entry is mathematically detectable.

</details>

<details>
<summary><strong>Reporting a vulnerability</strong></summary>
<br />

See the [security policy](https://github.com/Glyndor/panel/security/policy) and
the [security architecture](docs/security-architecture.md) for threat modeling.

</details>

---

## 🛠 Development

Contribution model, branch flow and code style live in the
[organization contributing guide](https://github.com/Glyndor/.github/blob/main/CONTRIBUTING.md).
Repo-specific setup:

**Dashboard backend (Rust):**

```bash
cd lynx
SQLX_OFFLINE=true cargo build -p lynx-dashboard-server
SQLX_OFFLINE=true cargo test -p lynx-dashboard-server
```

`sqlx` compile-time checks use the committed `.sqlx` cache. To run against a
real database, see `lynx/dashboard/server/.env` and start PostgreSQL locally.

**Dashboard frontend (Next.js):**

```bash
cd lynx/dashboard/ui
bun install
bun dev
```

**Shell lint:** `bash scripts/lint.sh` (shellcheck on all `.sh` files).

The agent and the compose translator live in
[panel-agent](https://github.com/Glyndor/panel-agent) and
[podman-compose](https://github.com/Glyndor/podman-compose).

<details>
<summary><strong>VM test matrix</strong></summary>
<br />

Some features cannot be tested in CI (nftables, WireGuard, Podman, systemd).
Changes in these areas require local VMs — note in your PR which scenarios you ran:

| Area | Environment |
|------|-------------|
| nftables rules, divergence detection | VM local |
| WireGuard tunnel setup, PSK rotation | VM local (CAP_NET_ADMIN) |
| Podman containers, org isolation | VM local |
| Auto-update binary swap | VM local |
| Installation + incompatible software | VM local |
| Dashboard ↔ agent connectivity | 2 VMs |
| Migration (dashboard or agent) | 2–3 VMs |

</details>

<details>
<summary><strong>Out of scope — do not contribute</strong></summary>
<br />

- Docker support — incompatible by design (nftables/network isolation conflict)
- Rollback / downgrade mechanisms — hotfix + auto-update is the model
- Metrics persistence — metrics are real-time WebSocket only
- SMTP integration — not planned
- Changes that break backwards compatibility of migrations (additive-only)

</details>

---

## 📄 License

[Apache-2.0](LICENSE) — © 2026 [Jaro-c](https://github.com/Jaro-c)

<div align="center">
  <br />
  <sub>Made with ❤️ by <a href="https://github.com/Jaro-c">Jaroc</a></sub>
</div>
