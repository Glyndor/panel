# Security Policy

---

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Report via GitHub's private vulnerability disclosure:  
[**Report a vulnerability →**](https://github.com/Jaro-c/Lynx/security/advisories/new)

Include:
- Component affected (`dashboard` / `agent` / installer)
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (optional)

Responsible disclosure is appreciated. If the report leads to a fix, you'll be credited in the release notes (unless you prefer anonymity).

---

## Response Timeline

| Stage | Target |
|-------|--------|
| Acknowledgement | 48 hours |
| Initial assessment | 5 business days |
| Fix + release | Depends on severity |

**Critical** (RCE, auth bypass, crypto break, privilege escalation) — target fix within 7 days.  
**High** (data leak, firewall bypass, replay attack) — target fix within 14 days.  
**Medium / Low** — addressed in next regular release.

---

## Supported Versions

Only the latest release of each component is supported. Lynx auto-updates itself — there is no manual rollback. If a critical bug is found, a new release is published and deployed automatically.

| Component | Support |
|-----------|---------|
| `dashboard@latest` | ✅ Supported |
| `agent@latest` | ✅ Supported |
| Older versions | ❌ No patches — update via auto-update |

---

## Scope

**In scope:**
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

**Out of scope:**
- Vulnerabilities requiring physical access to the VPS
- Issues in third-party dependencies (report upstream — we track them via `cargo-audit` / `bun audit`)
- Theoretical attacks with no practical exploit path
- Social engineering

---

## Security Architecture

Key properties for threat modeling:

- **Transport** — WireGuard + mTLS on all dashboard ↔ agent traffic. Agent never accepts plain connections.
- **Command integrity** — every dashboard → agent command is Ed25519-signed with a nonce and 30s timestamp window. Replay attacks rejected even if transport is compromised.
- **Binary integrity** — Ed25519 signature verified before any binary swap during auto-update. Partial downloads fail verification automatically.
- **Audit log** — hash-chained, append-only, synced to dashboard PostgreSQL in real time. Any tampered entry breaks the chain.
- **Firewall** — nftables default deny. `lynx-base` chain is invariant — auto-restored silently if modified, even by root.
- **Containers** — rootless Podman under per-org system users. UID 0 inside a container maps to an unprivileged UID on the host.
