# Contributing to Lynx (panel)

Lynx is a self-hosted VPS & container manager built in Rust and Next.js.

Start with the [organization-wide contributing guide](https://github.com/Glyndor/.github/blob/main/CONTRIBUTING.md) —
it covers the contribution model (invitation-only write access), the branch
flow (`topic → develop → main`, **all** merges via PR, no direct pushes), commit
conventions and the English-only policy. This file adds what is specific to
this repository.

The agent and the compose translator live in their own repositories:
[panel-agent](https://github.com/Glyndor/panel-agent) and
[podman-compose](https://github.com/Glyndor/podman-compose).

---

## Before You Start

- Search existing issues and PRs before opening new ones
- For large changes, open an issue first to discuss the approach

---

## Development Setup

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

**Lint:**

```bash
bash scripts/lint.sh   # shellcheck on all .sh files
```

---

## Pull Requests

- Target `develop`. Squash merge only — one commit per PR
- Commits must be **GPG or SSH signed** — unsigned commits are rejected
- Keep PRs focused: one concern per PR
- Fill out the PR template completely — changes touching the installer or
  firewall must be tested on a real VM/VPS

---

## Commit Messages

Conventional Commits format:

```
feat(dashboard): add PSK rotation without tunnel restart
fix(dashboard): correct nonce cleanup interval
chore(ci): update ubuntu runner to 24.04
```

Subject ≤ 50 characters. Body only when the "why" isn't obvious from the code.

---

## Versioning

This repository releases as `dashboard@x.y.z` (backend + frontend). Tags
trigger the release workflow. The agent has its own track in
[panel-agent](https://github.com/Glyndor/panel-agent).

---

## Code Style

**Rust (dashboard backend):**
- `cargo fmt` before committing
- `cargo clippy -- -D warnings` must pass
- No `unwrap()` in production paths — use proper error handling
- UTC everywhere — no local timestamps
- UUID v7 for all table IDs
- Queries via `sqlx` with bound parameters only — no string interpolation in SQL
- Shell commands via `std::process::Command::arg()` — never `sh -c "...{input}..."`

**Next.js (dashboard frontend):**
- `biome format` + `biome lint` before committing
- Server Components by default — `"use client"` only when required
- All user-visible text in i18n files (`en.json`, `es.json`) — never hardcoded strings
- Zod schemas for all input validation

**Shell scripts:**
- `shellcheck` must pass (`bash scripts/lint.sh`)
- Header block required (description, usage, requirements)
- ANSI colors for output

---

## Tests

**CI (GitHub Actions) — runs automatically on every PR:**
- Rust unit tests (`cargo test`)
- Dashboard integration tests (PostgreSQL + Redis containers)
- Frontend tests (Vitest + Playwright)
- `cargo-audit` — fails on known CVEs
- `bun audit` — fails on high/critical npm vulnerabilities
- `shellcheck` on all `.sh` files

**VM tests — required for certain changes:**

Some features cannot be tested in CI (nftables, WireGuard, Podman, systemd).
These require local VMs:

| Area | Environment |
|------|-------------|
| nftables rules, divergence detection | VM local |
| WireGuard tunnel setup, PSK rotation | VM local (CAP_NET_ADMIN) |
| Podman containers, org isolation | VM local |
| Auto-update binary swap | VM local |
| Installation + incompatible software | VM local |
| Dashboard ↔ agent connectivity | 2 VMs |
| Migration (dashboard or agent) | 2–3 VMs |

If your change affects these areas, note in your PR which VM scenarios you ran.

---

## What Not to Contribute

- Docker support — incompatible by design (nftables/network isolation conflict)
- Rollback / downgrade mechanisms — hotfix + auto-update is the model
- Metrics persistence — metrics are real-time WebSocket only
- SMTP integration — not planned
- Changes that break backwards compatibility of migrations (additive-only)

<details>
<summary><strong>File organization reference</strong></summary>
<br />

Never accumulate many files in one folder. Always use subdirectories by responsibility.

Rust pattern:
```
agents/
├── mod.rs
├── router.rs
├── heartbeat.rs
└── handlers/
    ├── mod.rs   ← re-exports only
    ├── crud.rs
    └── commands.rs
```

Frontend mirrors the URL structure:
```
src/components/(dashboard)/agents/
├── list/
├── detail/
└── nftables/
```

</details>
