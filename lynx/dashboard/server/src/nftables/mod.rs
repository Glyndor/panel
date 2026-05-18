pub mod handlers;
pub mod router;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NftRule {
    pub id: Uuid,
    pub scope: String,
    pub agent_id: Option<Uuid>,
    pub kind: String,
    pub port: Option<i32>,
    pub protocol: Option<String>,
    pub ip_list: Vec<String>,
    pub ip_version: String,
    pub rate_per_min: Option<i32>,
    pub description: Option<String>,
    pub priority: i32,
    pub enabled: bool,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRuleRequest {
    pub kind: String,
    pub port: Option<i32>,
    pub protocol: Option<String>,
    pub ip_list: Option<Vec<String>>,
    pub ip_version: Option<String>,
    pub rate_per_min: Option<i32>,
    pub description: Option<String>,
    pub priority: Option<i32>,
}

/// Convert individual rule fields into nft rule lines.
/// Used for system-level command generation without constructing a full NftRule.
pub fn rule_line(
    kind: &str,
    port: Option<u16>,
    protocol: Option<&str>,
    ip_list: &[String],
    rate_per_min: Option<u32>,
) -> String {
    let rule = NftRule {
        id: Uuid::nil(),
        scope: "global".into(),
        agent_id: None,
        kind: kind.to_string(),
        port: port.map(|p| p as i32),
        protocol: protocol.map(|s| s.to_string()),
        ip_list: ip_list.to_vec(),
        ip_version: "both".into(),
        rate_per_min: rate_per_min.map(|r| r as i32),
        description: None,
        priority: 0,
        enabled: true,
        created_by: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    rule_to_nft_lines(&rule).join("\n")
}

/// Convert a list of NftRules into the body of an nftables chain.
/// Returns lines suitable for insertion inside `chain <name> { ... }`.
pub fn rules_to_nft_chain(rules: &[NftRule]) -> String {
    let mut lines: Vec<String> = Vec::new();

    let mut sorted: Vec<&NftRule> = rules.iter().filter(|r| r.enabled).collect();
    sorted.sort_by_key(|r| r.priority);

    for rule in sorted {
        for line in rule_to_nft_lines(rule) {
            lines.push(format!("        {line}"));
        }
    }

    lines.join("\n")
}

fn rule_to_nft_lines(rule: &NftRule) -> Vec<String> {
    match rule.kind.as_str() {
        "allow_port" | "block_port" => port_rule_lines(rule),
        "allow_ip" | "block_ip" => ip_rule_lines(rule),
        "rate_limit" => rate_limit_lines(rule),
        _ => vec![],
    }
}

fn verdict(kind: &str) -> &str {
    match kind {
        "allow_port" | "allow_ip" => "accept",
        _ => "drop",
    }
}

fn protocol_match(protocol: &str) -> Vec<String> {
    match protocol {
        "tcp" => vec!["tcp".into()],
        "udp" => vec!["udp".into()],
        _ => vec!["tcp".into(), "udp".into()],
    }
}

fn ip_saddr_matches(rule: &NftRule) -> Vec<String> {
    if rule.ip_list.is_empty() {
        return vec![String::new()];
    }
    rule.ip_list
        .iter()
        .map(|ip| {
            let family = if ip.contains(':') { "ip6" } else { "ip" };
            format!("{family} saddr {ip} ")
        })
        .collect()
}

fn port_rule_lines(rule: &NftRule) -> Vec<String> {
    let Some(port) = rule.port else { return vec![] };
    let proto = rule.protocol.as_deref().unwrap_or("both");
    let verd = verdict(&rule.kind);
    let mut lines = Vec::new();

    for proto_kw in protocol_match(proto) {
        for saddr in ip_saddr_matches(rule) {
            lines.push(format!("{saddr}{proto_kw} dport {port} {verd}"));
        }
    }
    lines
}

fn ip_rule_lines(rule: &NftRule) -> Vec<String> {
    let verd = verdict(&rule.kind);
    rule.ip_list
        .iter()
        .map(|ip| {
            let family = if ip.contains(':') { "ip6" } else { "ip" };
            format!("{family} saddr {ip} {verd}")
        })
        .collect()
}

fn rate_limit_lines(rule: &NftRule) -> Vec<String> {
    let Some(port) = rule.port else { return vec![] };
    let Some(rate) = rule.rate_per_min else { return vec![] };
    let proto = rule.protocol.as_deref().unwrap_or("both");
    let mut lines = Vec::new();

    for proto_kw in protocol_match(proto) {
        for saddr in ip_saddr_matches(rule) {
            lines.push(format!(
                "{saddr}{proto_kw} dport {port} limit rate {rate}/minute accept"
            ));
        }
    }
    lines
}
