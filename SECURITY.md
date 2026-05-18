# Security Policy

## Supported Versions

Only the latest release of each component is supported.

| Component | Support |
|-----------|---------|
| `dashboard@latest` | ✅ Supported |
| `agent@latest` | ✅ Supported |
| Older versions | ❌ No patches — update via auto-update |

There is no manual rollback mechanism. The auto-updater handles all version transitions. If a critical bug is found, a new release is published and deployed automatically.

---

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report via GitHub's private vulnerability disclosure:
[Report a vulnerability](https://github.com/Jaro-c/Lynx/security/advisories/new)

Include:
- Component affected (dashboard / agent / installer)
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Any suggested fix (optional)

---

## Response Timeline

| Stage | Target |
|-------|--------|
| Acknowledgement | 48 hours |
| Initial assessment | 5 business days |
| Fix + release | Depends on severity |

**Critical** (RCE, auth bypass, crypto break, privilege escalation): target fix within 7 days.
**High** (data leak, firewall bypass, replay attack): target fix within 14 days.
**Medium/Low**: addressed in next regular release.

---

## Scope

In scope:
- Authentication and session handling
- WireGuard tunnel security
- nftables rule bypass
- Command signature verification (Ed25519)
- Replay / nonce attacks
- Privilege escalation via agent or containers
- Envelope encryption (KEK/DEK)
- PostgreSQL TDE key handling
- Auto-update pipeline (Ed25519 binary signature)
- SSRF in binary download flow
- SQL injection, shell injection

Out of scope:
- Vulnerabilities requiring physical access to the VPS
- Issues in third-party dependencies (report upstream; we follow `cargo-audit` / `bun audit`)
- Theoretical attacks with no practical exploit path
- Social engineering

---

## Security Architecture

Key properties relevant to threat modeling:

- **Transport:** WireGuard + mTLS (double layer). Agent never accepts plain connections.
- **Command integrity:** every dashboard → agent command is Ed25519-signed with nonce + 30s timestamp window. Replay attacks rejected even if transport is compromised.
- **Binary integrity:** Ed25519 signature verified before any binary swap during auto-update. Partial downloads fail verification.
- **Audit log:** hash-chained, append-only, synced to dashboard. Any tampered entry breaks the chain.
- **Firewall:** nftables default deny. `lynx-base` chain is invariant — auto-restored silently if modified, even by root.
- **Containers:** rootless Podman under per-org system users. UID 0 inside container maps to unprivileged UID on host.
