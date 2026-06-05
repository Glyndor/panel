# Security Architecture

Key properties of Lynx for threat modeling. The reporting process and response
targets are in the
[organization security policy](https://github.com/Glyndor/panel/security/policy).

## Properties

- **Transport** — WireGuard + mTLS on all dashboard ↔ agent traffic. The agent
  never accepts plain connections. TLS 1.3 minimum everywhere.
- **Command integrity** — every dashboard → agent command is Ed25519-signed
  with a nonce and a 30-second timestamp window. Replay attacks are rejected
  even if the transport is compromised.
- **Binary integrity** — Ed25519 signature verified before any binary swap
  during auto-update. Partial downloads fail verification automatically.
- **Audit log** — hash-chained, append-only, synced to dashboard PostgreSQL in
  real time. Any tampered entry breaks the chain.
- **Firewall** — nftables default deny. The `lynx-base` chain is invariant —
  auto-restored silently if modified, even by root.
- **Containers** — rootless Podman under per-org system users. UID 0 inside a
  container maps to an unprivileged UID on the host.
- **Secrets at rest** — envelope encryption (KEK/DEK); PostgreSQL TDE key
  handling.

## Areas of special interest for reports

- Authentication and session handling
- WireGuard tunnel security, PSK rotation
- nftables rule bypass
- Command signature verification (Ed25519), replay/nonce attacks
- Privilege escalation via agent or containers
- Auto-update pipeline (binary signature, SSRF in download flow)
- SQL injection, shell injection

## Supported versions

Only the latest release of each component is supported. Lynx auto-updates
itself — there is no manual rollback. If a critical bug is found, a new
release is published and deployed automatically.

| Component | Support |
|-----------|---------|
| `dashboard@latest` | ✅ Supported |
| `panel-agent` latest ([releases](https://github.com/Glyndor/panel-agent/releases)) | ✅ Supported |
| Older versions | ❌ No patches — update via auto-update |
