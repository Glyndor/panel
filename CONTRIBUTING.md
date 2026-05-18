# Contributing to Lynx

Lynx is a self-hosted VPS & container manager built in Rust and Next.js. Contributions are welcome — bug fixes, features, tests, and documentation.

Contributions are voluntary and unpaid. If you find Lynx useful and want to support its development, [GitHub Sponsors](https://github.com/sponsors/Jaro-c) is appreciated.

---

## Before You Start

- Search existing issues and PRs before opening new ones
- For large changes, open an issue first to discuss the approach
- All contributions must be in English — code, comments, commits, PR descriptions

---

## Branches

| Branch | Purpose |
|--------|---------|
| `main` | Production. Never push directly. Merge via PR from `develop` only. |
| `develop` | Working branch. Direct push allowed for maintainers. PRs target this branch. |

---

## Development Setup

**Agent (Rust):**
```bash
cd lynx
cargo build -p lynx-agent
cargo test -p lynx-agent
```

**Dashboard backend (Rust):**
```bash
cd lynx
cargo build -p lynx-dashboard-server
cargo test -p lynx-dashboard-server
```

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

- Squash merge only — one commit per PR on `main`
- Commits must be **GPG or SSH signed** — unsigned commits are rejected
- Keep PRs focused: one concern per PR
- Fill out the PR template completely

---

## Commit Messages

Conventional Commits format:

```
feat(agent): add PSK rotation without tunnel restart
fix(dashboard): correct nonce cleanup interval
chore(ci): update ubuntu runner to 24.04
```

Subject ≤ 50 characters. Body only when the "why" isn't obvious from the code.

---

## Versioning

Two independent release tracks:

- `dashboard@x.y.z` — dashboard backend + frontend
- `agent@x.y.z` — agent binary

A release on one track does not require a release on the other. Tags trigger the corresponding release workflow.

---

## Code Style

**Rust (agent + dashboard backend):**
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

Some features cannot be tested in CI (nftables, WireGuard, Podman, systemd). These require local VMs:

| Area | Environment |
|------|-------------|
| nftables rules, divergence detection | VM local |
| WireGuard tunnel setup, PSK rotation | VM local (CAP_NET_ADMIN) |
| Podman containers, org isolation | VM local |
| Auto-update binary swap | VM local |
| Installation + incompatible software | VM local |
| Agent ↔ dashboard connectivity | 2 VMs |
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
