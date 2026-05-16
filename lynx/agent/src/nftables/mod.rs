use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::process::Command;

const TABLE: &str = "lynx-agent";

/// Full ruleset applied atomically. Never incremental.
pub struct Ruleset {
    /// WireGuard UDP port for management plane
    pub wireguard_port: u16,
    /// Per-org blocked subnets (org isolation — inter-org traffic blocked)
    pub org_networks: Vec<OrgNetwork>,
}

pub struct OrgNetwork {
    pub org_id: String,
    pub subnet: String,
}

/// Apply the full lynx-agent nftables ruleset atomically.
/// Replaces the entire table on every call — never incremental.
pub fn apply(ruleset: &Ruleset) -> Result<()> {
    let nft = render_ruleset(ruleset);
    run_nft(&nft).context("nftables apply")?;
    Ok(())
}

/// Compute checksum of the live lynx-agent table for divergence detection.
pub fn current_checksum() -> Result<String> {
    let out = Command::new("nft")
        .args(["-j", "list", "table", "inet", TABLE])
        .output()
        .context("nft list table")?;

    if !out.status.success() {
        anyhow::bail!(
            "nft list table failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let mut hasher = Sha256::new();
    hasher.update(&out.stdout);
    Ok(hex::encode(hasher.finalize()))
}

/// Expected checksum after last apply() call. Stored in AppState.
pub fn checksum_of(ruleset: &Ruleset) -> String {
    let mut hasher = Sha256::new();
    hasher.update(render_ruleset(ruleset).as_bytes());
    hex::encode(hasher.finalize())
}

fn render_ruleset(r: &Ruleset) -> String {
    let mut out = format!(
        r#"
table inet {TABLE} {{
    chain input {{
        type filter hook input priority 0; policy drop;

        # Established/related connections
        ct state established,related accept

        # Loopback
        iif lo accept

        # WireGuard management plane
        udp dport {wg} accept

        # Drop everything else
    }}

    chain forward {{
        type filter hook forward priority 0; policy drop;
"#,
        TABLE = TABLE,
        wg = r.wireguard_port,
    );

    // Block inter-org traffic (nftables can't do "not same subnet" easily,
    // so we mark each org's subnet and drop cross-org forwarding)
    for org in &r.org_networks {
        out.push_str(&format!(
            "        # org {} isolation\n        ip saddr {} ip daddr != {} drop;\n",
            org.org_id, org.subnet, org.subnet
        ));
    }

    out.push_str("    }\n\n    chain output {\n        type filter hook output priority 0; policy accept;\n    }\n}\n");
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
