pub mod divergence;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::process::Command;

const TABLE: &str = "lynx-agent";

/// Full structure of the managed ruleset.
/// lynx-base holds the immutable invariants.
/// lynx-global / lynx-local hold dashboard-pushed input rules.
/// lynx-global-output / lynx-local-output hold dashboard-pushed output rules.
pub struct Ruleset {
    /// WireGuard UDP port for management plane
    pub wireguard_port: u16,
    /// Dashboard panel port opened in lynx-base (Some(19443) on dashboard VPS, None on remote agents).
    pub dashboard_port: Option<u16>,
    /// Per-org blocked subnets (org isolation — inter-org traffic blocked)
    pub org_networks: Vec<OrgNetwork>,
    /// Input rules body for the lynx-global chain (dashboard-pushed, applies to all agents)
    pub global_body: String,
    /// Input rules body for the lynx-local chain (dashboard-pushed, this agent only)
    pub local_body: String,
    /// Output rules body for the lynx-global-output chain (dashboard-pushed, applies to all agents)
    pub global_output_body: String,
    /// Output rules body for the lynx-local-output chain (dashboard-pushed, this agent only)
    pub local_output_body: String,
}

pub struct OrgNetwork {
    pub org_id: String,
    pub subnet: String,
}

/// Apply the full lynx-agent nftables ruleset atomically.
/// Replaces the entire table on every call — never incremental.
/// Returns the rendered ruleset string so callers can store it for restore.
pub fn apply(ruleset: &Ruleset) -> Result<String> {
    let nft = render_ruleset(ruleset);
    run_nft(&nft).context("nftables apply")?;
    persist_ruleset(&nft);
    Ok(nft)
}

/// Re-apply a previously rendered ruleset string directly (used for restore).
pub fn apply_raw(nft: &str) -> Result<()> {
    run_nft(nft).context("nftables apply_raw")?;
    persist_ruleset(nft);
    Ok(())
}

/// Apply a minimal emergency ruleset when normal restore fails.
/// Allows only WireGuard inbound from the dashboard + established + loopback.
/// Everything else dropped — VPS stays reachable only from dashboard.
pub fn apply_emergency() -> Result<()> {
    run_nft(EMERGENCY_RULESET).context("nftables apply_emergency")?;
    persist_ruleset(EMERGENCY_RULESET);
    Ok(())
}

/// Persist the active ruleset to disk so nftables.service can reload it on boot.
fn persist_ruleset(nft: &str) {
    if let Err(e) = std::fs::write("/etc/nftables-lynx-agent.conf", nft) {
        tracing::warn!(error = %e, "failed to persist nftables ruleset to disk");
    }
}

const EMERGENCY_RULESET: &str = r#"
destroy table inet lynx-agent
add table inet lynx-agent
table inet lynx-agent {
    chain lynx-base {
        type filter hook input priority 0; policy drop;
        ct state established,related accept
        iifname "lo" accept
        udp dport 51820 accept
        drop
    }
    chain lynx-forward {
        type filter hook forward priority 0; policy drop;
    }
    chain lynx-output {
        type filter hook output priority 0; policy accept;
    }
}
"#;

/// Compute checksum of the live lynx-agent table for divergence detection.
pub fn current_checksum() -> Result<String> {
    chain_checksum_raw(&["list", "table", "inet", TABLE])
}

/// Compute checksum of a single chain for per-chain divergence detection.
pub fn chain_checksum(chain: &str) -> Result<String> {
    chain_checksum_raw(&["list", "chain", "inet", TABLE, chain])
}

fn chain_checksum_raw(args: &[&str]) -> Result<String> {
    let out = Command::new("nft")
        .arg("-j")
        .args(args)
        .output()
        .context("nft list")?;

    if !out.status.success() {
        anyhow::bail!("nft list failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    let mut hasher = Sha256::new();
    hasher.update(&out.stdout);
    Ok(hex::encode(hasher.finalize()))
}

fn render_ruleset(r: &Ruleset) -> String {
    let dashboard_port_rule = match r.dashboard_port {
        Some(port) => format!(
            "\n        # Dashboard panel port\n        tcp dport {port} ct state new accept\n"
        ),
        None => String::new(),
    };

    // Container DNS (aardvark-dns on Netavark bridges) — dashboard VPS only.
    // Rootless org containers on remote agents use user-namespace networking
    // that doesn't hit the host INPUT chain for DNS.
    let dashboard_dns_rules = if r.dashboard_port.is_some() {
        "\n        # DNS for container networks (aardvark-dns on Netavark bridge interfaces)\n        iifname \"podman*\" udp dport 53 accept\n        iifname \"podman*\" tcp dport 53 accept\n"
    } else {
        ""
    };

    // Netavark DNAT rewrites the destination from the host IP to the container IP
    // (10.89.x.x) in PREROUTING. Without a forward rule, lynx-forward policy drop
    // kills these packets before they reach the container. This applies to ALL agents:
    // the agent's own PostgreSQL container is also published via DNAT.
    let container_forward_rules = "\n        # New connections to published container ports (Netavark DNAT rewrites dst to 10.89.x.x)\n        ip daddr 10.89.0.0/16 ct state new accept\n\n        # Outbound traffic from Podman containers (package installs, GitHub, cert renewals, etc.)\n        iifname \"podman*\" accept\n";

    // WireGuard forward rules — dashboard VPS only.
    // Backend container needs to route through wg-lynx-dash to reach remote agents.
    let dashboard_wg_forward_rules = if r.dashboard_port.is_some() {
        "\n        # Backend container traffic to/from WireGuard (dashboard <-> agents)\n        oifname \"wg-lynx-dash\" accept\n        iifname \"wg-lynx-dash\" accept\n"
    } else {
        ""
    };

    let mut out = format!(
        r#"
destroy table inet {TABLE}
add table inet {TABLE}
table inet {TABLE} {{
    # Immutable invariants — never editable from dashboard
    chain lynx-base {{
        type filter hook input priority 0; policy drop;

        # Established/related
        ct state established,related accept

        # Loopback
        iif lo accept

        # ICMP — path MTU, diagnostics, reachability
        ip protocol icmp accept
        ip6 nexthdr icmpv6 accept

        # SSH — emergency admin access (per-source-IP rate limit)
        tcp dport 22 ct state new meter ssh_throttle {{ ip saddr limit rate 10/minute burst 20 packets }} accept

        # WireGuard management plane
        udp dport {wg} accept

        # Dashboard backend (management plane — WireGuard only)
        ip saddr 10.100.0.1 accept
{dashboard_port}
{dashboard_dns}
        # Run global and local rule chains
        jump lynx-global
        jump lynx-local

        drop
    }}

    # Dashboard global rules — input, apply to all agents
    chain lynx-global {{
{global}
    }}

    # Dashboard local rules — input, apply to this agent only
    chain lynx-local {{
{local}
    }}

    chain lynx-forward {{
        type filter hook forward priority 0; policy drop;

        ct state established,related accept
{container_forward}
{dashboard_wg_forward}
"#,
        TABLE = TABLE,
        wg = r.wireguard_port,
        dashboard_port = dashboard_port_rule,
        dashboard_dns = dashboard_dns_rules,
        container_forward = container_forward_rules,
        dashboard_wg_forward = dashboard_wg_forward_rules,
        global = r.global_body,
        local = r.local_body,
    );

    // Block inter-org traffic
    for org in &r.org_networks {
        out.push_str(&format!(
            "        # org {} isolation\n        ip saddr {} ip daddr != {} drop;\n",
            org.org_id, org.subnet, org.subnet
        ));
    }

    out.push_str(&format!(
        r#"    }}

    chain lynx-output {{
        type filter hook output priority 0; policy accept;

        # Dashboard global output rules — apply to all agents
{global_out}

        # Dashboard local output rules — apply to this agent only
{local_out}
    }}
}}
"#,
        global_out = r.global_output_body,
        local_out = r.local_output_body,
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- render_ruleset — pure string generation, no I/O ---

    fn minimal_ruleset() -> Ruleset {
        Ruleset {
            wireguard_port: 51820,
            dashboard_port: None,
            org_networks: vec![],
            global_body: String::new(),
            local_body: String::new(),
            global_output_body: String::new(),
            local_output_body: String::new(),
        }
    }

    #[test]
    fn render_contains_table_name() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            out.contains("table inet lynx-agent"),
            "table declaration missing"
        );
    }

    #[test]
    fn render_contains_wireguard_port() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(out.contains("51820"), "WireGuard port missing from ruleset");
    }

    #[test]
    fn render_custom_wireguard_port() {
        let mut r = minimal_ruleset();
        r.wireguard_port = 12345;
        let out = render_ruleset(&r);
        assert!(out.contains("12345"), "custom WireGuard port not rendered");
    }

    #[test]
    fn render_contains_lynx_base_chain() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(out.contains("chain lynx-base"), "lynx-base chain missing");
    }

    #[test]
    fn render_contains_lynx_global_chain() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            out.contains("chain lynx-global"),
            "lynx-global chain missing"
        );
    }

    #[test]
    fn render_contains_lynx_local_chain() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(out.contains("chain lynx-local"), "lynx-local chain missing");
    }

    #[test]
    fn render_contains_lynx_forward_chain() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            out.contains("chain lynx-forward"),
            "lynx-forward chain missing"
        );
    }

    #[test]
    fn render_contains_lynx_output_chain() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            out.contains("chain lynx-output"),
            "lynx-output chain missing"
        );
    }

    #[test]
    fn render_contains_default_deny() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(out.contains("policy drop"), "default deny policy missing");
    }

    #[test]
    fn render_contains_dashboard_management_ip() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        // The dashboard backend is always at 10.100.0.1
        assert!(
            out.contains("10.100.0.1"),
            "dashboard management IP missing"
        );
    }

    #[test]
    fn render_global_body_included() {
        let mut r = minimal_ruleset();
        r.global_body = "        tcp dport 443 accept".to_string();
        let out = render_ruleset(&r);
        assert!(
            out.contains("tcp dport 443 accept"),
            "global_body not included"
        );
    }

    #[test]
    fn render_local_body_included() {
        let mut r = minimal_ruleset();
        r.local_body = "        tcp dport 8080 accept".to_string();
        let out = render_ruleset(&r);
        assert!(
            out.contains("tcp dport 8080 accept"),
            "local_body not included"
        );
    }

    #[test]
    fn render_org_isolation_rules_included() {
        let mut r = minimal_ruleset();
        r.org_networks = vec![OrgNetwork {
            org_id: "org-abc".to_string(),
            subnet: "172.20.0.0/24".to_string(),
        }];
        let out = render_ruleset(&r);
        assert!(
            out.contains("172.20.0.0/24"),
            "org subnet missing from isolation rules"
        );
        assert!(
            out.contains("org-abc"),
            "org id missing from isolation comment"
        );
    }

    #[test]
    fn render_multiple_orgs_all_present() {
        let mut r = minimal_ruleset();
        r.org_networks = vec![
            OrgNetwork {
                org_id: "org-1".to_string(),
                subnet: "172.20.1.0/24".to_string(),
            },
            OrgNetwork {
                org_id: "org-2".to_string(),
                subnet: "172.20.2.0/24".to_string(),
            },
        ];
        let out = render_ruleset(&r);
        assert!(out.contains("172.20.1.0/24"));
        assert!(out.contains("172.20.2.0/24"));
    }

    #[test]
    fn render_output_is_non_empty() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(!out.is_empty(), "rendered ruleset should not be empty");
    }

    #[test]
    fn render_has_destroy_add_prefix() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            out.contains("destroy table inet lynx-agent"),
            "idempotent prefix missing: destroy table"
        );
        assert!(
            out.contains("add table inet lynx-agent"),
            "idempotent prefix missing: add table"
        );
    }

    #[test]
    fn render_lynx_base_contains_ssh() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            out.contains("tcp dport 22"),
            "SSH accept missing from lynx-base"
        );
        assert!(
            out.contains("ssh_throttle"),
            "SSH rate-limit meter missing from lynx-base"
        );
    }

    #[test]
    fn render_lynx_base_contains_icmp() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            out.contains("ip protocol icmp accept"),
            "ICMP v4 accept missing from lynx-base"
        );
        assert!(
            out.contains("ip6 nexthdr icmpv6 accept"),
            "ICMP v6 accept missing from lynx-base"
        );
    }

    #[test]
    fn render_dashboard_port_included_when_set() {
        let mut r = minimal_ruleset();
        r.dashboard_port = Some(19443);
        let out = render_ruleset(&r);
        assert!(
            out.contains("tcp dport 19443"),
            "dashboard port not included when Some"
        );
    }

    #[test]
    fn render_dashboard_port_absent_when_none() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            !out.contains("19443"),
            "dashboard port should not appear when None"
        );
    }

    #[test]
    fn render_dashboard_dns_included_when_set() {
        let mut r = minimal_ruleset();
        r.dashboard_port = Some(19443);
        let out = render_ruleset(&r);
        assert!(
            out.contains("iifname \"podman*\" udp dport 53 accept"),
            "container DNS UDP missing when dashboard_port set"
        );
        assert!(
            out.contains("iifname \"podman*\" tcp dport 53 accept"),
            "container DNS TCP missing when dashboard_port set"
        );
    }

    #[test]
    fn render_dashboard_dns_absent_when_none() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            !out.contains("udp dport 53"),
            "container DNS should not appear when dashboard_port is None"
        );
    }

    #[test]
    fn render_dashboard_forward_rules_included_when_set() {
        let mut r = minimal_ruleset();
        r.dashboard_port = Some(19443);
        let out = render_ruleset(&r);
        assert!(
            out.contains("ip daddr 10.89.0.0/16 ct state new accept"),
            "Netavark published port forward rule missing when dashboard_port set"
        );
        assert!(
            out.contains("iifname \"podman*\" accept"),
            "container outbound forward rule missing when dashboard_port set"
        );
        assert!(
            out.contains("oifname \"wg-lynx-dash\" accept"),
            "WireGuard outbound forward rule missing when dashboard_port set"
        );
        assert!(
            out.contains("iifname \"wg-lynx-dash\" accept"),
            "WireGuard inbound forward rule missing when dashboard_port set"
        );
    }

    #[test]
    fn render_dashboard_wg_forward_rules_absent_when_none() {
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            !out.contains("wg-lynx-dash"),
            "WireGuard forward rules should not appear when dashboard_port is None"
        );
    }

    #[test]
    fn render_container_forward_rules_always_present() {
        // These rules are required on ALL agents (not just dashboard VPS) because
        // the agent's own PostgreSQL container is published via Netavark DNAT.
        let r = minimal_ruleset();
        let out = render_ruleset(&r);
        assert!(
            out.contains("ip daddr 10.89.0.0/16 ct state new accept"),
            "Netavark forward rule must be present on all agents"
        );
        assert!(
            out.contains("iifname \"podman*\" accept"),
            "Podman outbound forward rule must be present on all agents"
        );
    }

    // --- Emergency ruleset constant ---

    #[test]
    fn emergency_ruleset_is_non_empty() {
        assert!(!EMERGENCY_RULESET.is_empty());
        assert!(EMERGENCY_RULESET.contains("policy drop"));
        assert!(EMERGENCY_RULESET.contains("51820"));
        assert!(EMERGENCY_RULESET.contains("lynx-agent"));
    }

    #[test]
    fn emergency_ruleset_has_destroy_add_prefix() {
        assert!(EMERGENCY_RULESET.contains("destroy table inet lynx-agent"));
        assert!(EMERGENCY_RULESET.contains("add table inet lynx-agent"));
    }
}

fn run_nft(ruleset: &str) -> Result<()> {
    let mut child = Command::new("nft")
        .args(["-f", "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("spawn nft")?;

    use std::io::Write;
    if let Some(stdin) = child.stdin.take() {
        let mut stdin = stdin;
        stdin
            .write_all(ruleset.as_bytes())
            .context("write nft stdin")?;
    }

    let status = child.wait().context("wait nft")?;
    if !status.success() {
        anyhow::bail!("nft exited with: {status}");
    }
    Ok(())
}
