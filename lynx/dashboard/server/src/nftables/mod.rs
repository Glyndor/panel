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
    pub port_end: Option<i32>,
    pub protocol: Option<String>,
    pub ip_list: Vec<String>,
    pub ip_version: String,
    pub rate_per_min: Option<i32>,
    pub description: Option<String>,
    pub priority: i32,
    pub enabled: bool,
    pub direction: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRuleRequest {
    pub kind: String,
    pub port: Option<i32>,
    pub port_end: Option<i32>,
    pub protocol: Option<String>,
    pub ip_list: Option<Vec<String>>,
    pub ip_version: Option<String>,
    pub rate_per_min: Option<i32>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub direction: Option<String>,
}

/// Convert a list of NftRules into the body of the input chain (lynx-global / lynx-local).
/// Filters to direction = 'input', sorted by priority.
pub fn rules_to_nft_chain(rules: &[NftRule]) -> String {
    let mut sorted: Vec<&NftRule> = rules
        .iter()
        .filter(|r| r.enabled && r.direction == "input")
        .collect();
    sorted.sort_by_key(|r| r.priority);

    sorted
        .iter()
        .flat_map(|r| rule_to_nft_lines(r))
        .map(|l| format!("        {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Convert a list of NftRules into the body of the output chain (lynx-global-output / lynx-local-output).
/// Filters to direction = 'output', sorted by priority.
pub fn rules_to_nft_output_chain(rules: &[NftRule]) -> String {
    let mut sorted: Vec<&NftRule> = rules
        .iter()
        .filter(|r| r.enabled && r.direction == "output")
        .collect();
    sorted.sort_by_key(|r| r.priority);

    sorted
        .iter()
        .flat_map(|r| rule_to_nft_lines(r))
        .map(|l| format!("        {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn rule_to_nft_lines(rule: &NftRule) -> Vec<String> {
    match rule.kind.as_str() {
        "allow_port" | "block_port" => port_rule_lines(rule),
        "allow_ip" | "block_ip" => ip_rule_lines(rule),
        "rate_limit" => rate_limit_lines(rule),
        "drop_invalid_state" => vec!["ct state invalid drop".into()],
        "tcp_flag_null" => vec!["tcp flags == 0x0 drop".into()],
        "tcp_flag_xmas" => vec!["tcp flags & (fin | psh | urg) == fin | psh | urg drop".into()],
        "tcp_flag_ack_new" => vec!["tcp flags & ack == ack ct state new drop".into()],
        "icmp_ping_limit" => icmp_ping_limit_lines(rule),
        "allow_icmp_errors" => allow_icmp_error_lines(rule),
        "allow_ndp" => vec![
            "ip6 nexthdr ipv6-icmp icmpv6 type { nd-router-advert, nd-neighbor-solicit, nd-neighbor-advert } accept".into(),
        ],
        "block_output_port" => output_port_block_lines(rule),
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

fn port_spec(rule: &NftRule) -> Option<String> {
    let port = rule.port?;
    Some(match rule.port_end {
        Some(end) => format!("{port}-{end}"),
        None => port.to_string(),
    })
}

fn port_rule_lines(rule: &NftRule) -> Vec<String> {
    let Some(spec) = port_spec(rule) else {
        return vec![];
    };
    let proto = rule.protocol.as_deref().unwrap_or("both");
    let verd = verdict(&rule.kind);
    let mut lines = Vec::new();

    for proto_kw in protocol_match(proto) {
        for saddr in ip_saddr_matches(rule) {
            lines.push(format!("{saddr}{proto_kw} dport {spec} {verd}"));
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
    let Some(spec) = port_spec(rule) else {
        return vec![];
    };
    let Some(rate) = rule.rate_per_min else {
        return vec![];
    };
    let proto = rule.protocol.as_deref().unwrap_or("both");
    let mut lines = Vec::new();

    for proto_kw in protocol_match(proto) {
        for saddr in ip_saddr_matches(rule) {
            lines.push(format!(
                "{saddr}{proto_kw} dport {spec} limit rate {rate}/minute accept"
            ));
        }
    }
    lines
}

fn icmp_ping_limit_lines(rule: &NftRule) -> Vec<String> {
    let rate = rule.rate_per_min.unwrap_or(3);
    let mut lines = Vec::new();
    match rule.ip_version.as_str() {
        "ipv4" => lines.push(format!(
            "ip protocol icmp icmp type echo-request ct state new limit rate {rate}/second burst 10 packets accept"
        )),
        "ipv6" => lines.push(format!(
            "ip6 nexthdr ipv6-icmp icmpv6 type echo-request ct state new limit rate {rate}/second burst 10 packets accept"
        )),
        _ => {
            lines.push(format!(
                "ip protocol icmp icmp type echo-request ct state new limit rate {rate}/second burst 10 packets accept"
            ));
            lines.push(format!(
                "ip6 nexthdr ipv6-icmp icmpv6 type echo-request ct state new limit rate {rate}/second burst 10 packets accept"
            ));
        }
    }
    lines
}

fn allow_icmp_error_lines(rule: &NftRule) -> Vec<String> {
    let mut lines = Vec::new();
    match rule.ip_version.as_str() {
        "ipv4" => lines.push(
            "ip protocol icmp icmp type { destination-unreachable, time-exceeded, parameter-problem } accept".into(),
        ),
        "ipv6" => lines.push(
            "ip6 nexthdr ipv6-icmp icmpv6 type { destination-unreachable, packet-too-big, time-exceeded, parameter-problem } accept".into(),
        ),
        _ => {
            lines.push(
                "ip protocol icmp icmp type { destination-unreachable, time-exceeded, parameter-problem } accept".into(),
            );
            lines.push(
                "ip6 nexthdr ipv6-icmp icmpv6 type { destination-unreachable, packet-too-big, time-exceeded, parameter-problem } accept".into(),
            );
        }
    }
    lines
}

fn output_port_block_lines(rule: &NftRule) -> Vec<String> {
    let Some(spec) = port_spec(rule) else {
        return vec![];
    };
    let proto = rule.protocol.as_deref().unwrap_or("both");
    protocol_match(proto)
        .into_iter()
        .map(|p| format!("{p} dport {spec} drop"))
        .collect()
}
