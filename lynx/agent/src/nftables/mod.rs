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
    Ok(nft)
}

/// Re-apply a previously rendered ruleset string directly (used for restore).
pub fn apply_raw(nft: &str) -> Result<()> {
    run_nft(nft).context("nftables apply_raw")
}

/// Apply a minimal emergency ruleset when normal restore fails.
/// Allows only WireGuard inbound from the dashboard + established + loopback.
/// Everything else dropped — VPS stays reachable only from dashboard.
pub fn apply_emergency() -> Result<()> {
    let emergency = r#"
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
    run_nft(emergency).context("nftables apply_emergency")
}

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
    let mut out = format!(
        r#"
table inet {TABLE} {{
    # Immutable invariants — never editable from dashboard
    chain lynx-base {{
        type filter hook input priority 0; policy drop;

        # Established/related
        ct state established,related accept

        # Loopback
        iif lo accept

        # WireGuard management plane — dashboard VPS only
        udp dport {wg} accept

        # Dashboard backend port (on WG interface only)
        ip saddr 10.100.0.1 accept

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
"#,
        TABLE = TABLE,
        wg = r.wireguard_port,
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
