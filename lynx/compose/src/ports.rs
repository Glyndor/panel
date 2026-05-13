//! Port mapping parser.
//!
//! Handles all docker-compose port format variants and converts them to
//! bollard's `PortBinding` structures.

use std::collections::HashMap;
use bollard::models::PortBinding;

use crate::compose::types::{PortMapping, StringOrU16};
use crate::error::{ComposeError, Result};

/// A parsed, normalized port binding.
#[derive(Debug, Clone)]
pub struct ParsedPort {
    /// Container port number.
    pub container_port: u16,
    /// Protocol (`tcp` or `udp`).
    pub protocol: String,
    /// Host IP (may be empty to mean all interfaces).
    pub host_ip: String,
    /// Host port (`None` means random / ephemeral).
    pub host_port: Option<u16>,
}

/// Parse all port mappings in a service, expanding ranges.
pub fn parse_ports(ports: &[PortMapping]) -> Result<Vec<ParsedPort>> {
    let mut result = Vec::new();
    for mapping in ports {
        result.extend(parse_one(mapping)?);
    }
    Ok(result)
}

/// Convert parsed ports into bollard's `PortBindings` and `ExposedPorts` maps.
///
/// Returns `(port_bindings, exposed_ports)`.
pub fn to_bollard(
    ports: &[ParsedPort],
) -> (
    HashMap<String, Option<Vec<PortBinding>>>,
    HashMap<String, HashMap<(), ()>>,
) {
    let mut port_bindings: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    let mut exposed_ports: HashMap<String, HashMap<(), ()>> = HashMap::new();

    for p in ports {
        let key = format!("{}/{}", p.container_port, p.protocol);
        let host_ip = if p.host_ip.is_empty() {
            "0.0.0.0".to_string()
        } else {
            p.host_ip.clone()
        };
        let host_port = p.host_port.map(|p| p.to_string());
        let binding = PortBinding {
            host_ip: Some(host_ip),
            host_port,
        };
        let bindings = port_bindings
            .entry(key.clone())
            .or_insert_with(|| Some(Vec::new()));
        if let Some(v) = bindings {
            v.push(binding);
        }
        exposed_ports.entry(key).or_default();
    }

    (port_bindings, exposed_ports)
}

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

fn parse_one(mapping: &PortMapping) -> Result<Vec<ParsedPort>> {
    match mapping {
        PortMapping::Short(s) => parse_short(s),
        PortMapping::Long {
            target,
            published,
            protocol,
            host_ip,
        } => {
            let proto = protocol.clone().unwrap_or_else(|| "tcp".into());
            let hip = host_ip.clone().unwrap_or_default();
            let host_port = published.as_ref().map(|p| match p {
                StringOrU16::Number(n) => *n,
                StringOrU16::String(s) => s.parse::<u16>().unwrap_or(*target),
            });
            Ok(vec![ParsedPort {
                container_port: *target,
                protocol: proto,
                host_ip: hip,
                host_port,
            }])
        }
    }
}

/// Parse a short-form port string.
///
/// Formats:
/// - `container`
/// - `container/proto`
/// - `host:container`
/// - `host:container/proto`
/// - `ip:host:container`
/// - `ip:host:container/proto`
/// - `host_start-host_end:container_start-container_end`
fn parse_short(s: &str) -> Result<Vec<ParsedPort>> {
    // Split off protocol suffix.
    let (rest, proto) = if let Some(idx) = s.rfind('/') {
        (&s[..idx], s[idx + 1..].to_string())
    } else {
        (s, "tcp".to_string())
    };

    // Count colons to determine format.
    let colon_count = rest.chars().filter(|&c| c == ':').count();

    match colon_count {
        0 => {
            // Just container port (possibly a range).
            let ports = expand_port_range(rest)?;
            Ok(ports
                .into_iter()
                .map(|cp| ParsedPort {
                    container_port: cp,
                    protocol: proto.clone(),
                    host_ip: String::new(),
                    host_port: None,
                })
                .collect())
        }
        1 => {
            let (left, right) = split_last_colon(rest);
            // left = host_port (or range), right = container_port (or range)
            let host_ports = expand_port_range(left)?;
            let container_ports = expand_port_range(right)?;
            if host_ports.len() != container_ports.len() && host_ports.len() != 1 {
                return Err(ComposeError::InvalidPort(format!(
                    "port range mismatch: {s}"
                )));
            }
            Ok(host_ports
                .into_iter()
                .zip(container_ports.into_iter())
                .map(|(hp, cp)| ParsedPort {
                    container_port: cp,
                    protocol: proto.clone(),
                    host_ip: String::new(),
                    host_port: Some(hp),
                })
                .collect())
        }
        _ => {
            // ip:host:container or ip:host_range:container_range
            // Split into at most 3 parts from left.
            let parts: Vec<&str> = rest.splitn(3, ':').collect();
            if parts.len() < 3 {
                return Err(ComposeError::InvalidPort(format!("invalid port spec: {s}")));
            }
            let ip = parts[0];
            let host_ports = expand_port_range(parts[1])?;
            let container_ports = expand_port_range(parts[2])?;
            if host_ports.len() != container_ports.len() && host_ports.len() != 1 {
                return Err(ComposeError::InvalidPort(format!(
                    "port range mismatch: {s}"
                )));
            }
            Ok(host_ports
                .into_iter()
                .zip(container_ports.into_iter())
                .map(|(hp, cp)| ParsedPort {
                    container_port: cp,
                    protocol: proto.clone(),
                    host_ip: ip.to_string(),
                    host_port: Some(hp),
                })
                .collect())
        }
    }
}

/// Split at the LAST colon (to avoid splitting IPv6 addresses incorrectly).
fn split_last_colon(s: &str) -> (&str, &str) {
    if let Some(idx) = s.rfind(':') {
        (&s[..idx], &s[idx + 1..])
    } else {
        ("", s)
    }
}

/// Expand `start-end` or a single port string.
fn expand_port_range(s: &str) -> Result<Vec<u16>> {
    let s = s.trim();
    if let Some(idx) = s.find('-') {
        let start: u16 = s[..idx]
            .parse()
            .map_err(|_| ComposeError::InvalidPort(format!("bad port: {s}")))?;
        let end: u16 = s[idx + 1..]
            .parse()
            .map_err(|_| ComposeError::InvalidPort(format!("bad port: {s}")))?;
        if start > end {
            return Err(ComposeError::InvalidPort(format!(
                "start > end in range: {s}"
            )));
        }
        Ok((start..=end).collect())
    } else {
        let p: u16 = s
            .parse()
            .map_err(|_| ComposeError::InvalidPort(format!("bad port: {s}")))?;
        Ok(vec![p])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn short(s: &str) -> PortMapping {
        PortMapping::Short(s.to_string())
    }

    #[test]
    fn container_only() {
        let ports = parse_ports(&[short("80")]).unwrap();
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].container_port, 80);
        assert_eq!(ports[0].protocol, "tcp");
        assert!(ports[0].host_port.is_none());
    }

    #[test]
    fn host_colon_container() {
        let ports = parse_ports(&[short("8080:80")]).unwrap();
        assert_eq!(ports[0].container_port, 80);
        assert_eq!(ports[0].host_port, Some(8080));
    }

    #[test]
    fn ip_host_container() {
        let ports = parse_ports(&[short("127.0.0.1:8080:80")]).unwrap();
        assert_eq!(ports[0].container_port, 80);
        assert_eq!(ports[0].host_port, Some(8080));
        assert_eq!(ports[0].host_ip, "127.0.0.1");
    }

    #[test]
    fn udp_protocol() {
        let ports = parse_ports(&[short("514:514/udp")]).unwrap();
        assert_eq!(ports[0].protocol, "udp");
    }

    #[test]
    fn range_expansion() {
        let ports = parse_ports(&[short("8000-8002:8000-8002")]).unwrap();
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].container_port, 8000);
        assert_eq!(ports[1].container_port, 8001);
        assert_eq!(ports[2].container_port, 8002);
    }

    #[test]
    fn container_only_udp() {
        let ports = parse_ports(&[short("53/udp")]).unwrap();
        assert_eq!(ports[0].container_port, 53);
        assert_eq!(ports[0].protocol, "udp");
        assert!(ports[0].host_port.is_none());
    }

    #[test]
    fn to_bollard_produces_correct_keys() {
        let ports = parse_ports(&[short("8080:80")]).unwrap();
        let (bindings, exposed) = to_bollard(&ports);
        assert!(bindings.contains_key("80/tcp"));
        assert!(exposed.contains_key("80/tcp"));
        let b = bindings["80/tcp"].as_ref().unwrap();
        assert_eq!(b[0].host_port.as_deref(), Some("8080"));
    }
}
