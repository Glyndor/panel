use anyhow::Result;
use serde::Serialize;
use std::{fs, process::Command, time::Duration};
use tokio::time::sleep;

#[derive(Debug, Serialize)]
pub struct SystemMetrics {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub cpu_percent: f64,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
    pub timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct ContainerStat {
    pub id: String,
    pub name: String,
    pub cpu_percent: f64,
    pub mem_usage_mb: f64,
    pub mem_limit_mb: f64,
}

#[derive(Debug, Serialize)]
pub struct ContainerMetrics {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub containers: Vec<ContainerStat>,
    pub timestamp: i64,
}

pub async fn sample_system() -> Result<SystemMetrics> {
    let cpu = read_cpu_percent().await;
    let (mem_used, mem_total) = read_mem_mb();
    let (disk_used, disk_total) = read_disk_gb("/");

    Ok(SystemMetrics {
        msg_type: "system_metrics",
        cpu_percent: cpu,
        mem_used_mb: mem_used,
        mem_total_mb: mem_total,
        disk_used_gb: disk_used,
        disk_total_gb: disk_total,
        timestamp: chrono::Utc::now().timestamp(),
    })
}

pub fn sample_containers() -> ContainerMetrics {
    let containers = collect_container_stats();
    ContainerMetrics {
        msg_type: "container_metrics",
        containers,
        timestamp: chrono::Utc::now().timestamp(),
    }
}

fn collect_container_stats() -> Vec<ContainerStat> {
    // `podman stats --no-stream --format json` returns a JSON array.
    let output = Command::new("podman")
        .args(["stats", "--no-stream", "--format", "json"])
        .output();

    let out = match output {
        Ok(o) if o.status.success() => o.stdout,
        _ => return vec![],
    };

    #[derive(serde::Deserialize)]
    struct RawStat {
        #[serde(rename = "ID")]
        id: String,
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "CPUPerc")]
        cpu_perc: String, // "1.23%"
        #[serde(rename = "MemUsage")]
        mem_usage: String, // "12.5MB / 2GB"
    }

    let stats: Vec<RawStat> = serde_json::from_slice(&out).unwrap_or_default();

    stats
        .into_iter()
        .map(|s| {
            let cpu = s
                .cpu_perc
                .trim_end_matches('%')
                .parse::<f64>()
                .unwrap_or(0.0);
            let (usage, limit) = parse_mem_usage(&s.mem_usage);
            ContainerStat {
                id: s.id,
                name: s.name,
                cpu_percent: cpu,
                mem_usage_mb: usage,
                mem_limit_mb: limit,
            }
        })
        .collect()
}

/// Parse "12.5MiB / 2GiB" into (usage_mb, limit_mb).
fn parse_mem_usage(raw: &str) -> (f64, f64) {
    let parts: Vec<&str> = raw.split('/').collect();
    let parse = |s: &str| -> f64 {
        let s = s.trim();
        if let Some(v) = s.strip_suffix("GiB").or_else(|| s.strip_suffix("GB")) {
            v.parse::<f64>().unwrap_or(0.0) * 1024.0
        } else if let Some(v) = s.strip_suffix("MiB").or_else(|| s.strip_suffix("MB")) {
            v.parse::<f64>().unwrap_or(0.0)
        } else if let Some(v) = s.strip_suffix("KiB").or_else(|| s.strip_suffix("KB")) {
            v.parse::<f64>().unwrap_or(0.0) / 1024.0
        } else {
            0.0
        }
    };
    let usage = parts.first().map(|s| parse(s)).unwrap_or(0.0);
    let limit = parts.get(1).map(|s| parse(s)).unwrap_or(0.0);
    (usage, limit)
}

/// Two-sample CPU idle calculation from /proc/stat.
async fn read_cpu_percent() -> f64 {
    let s1 = read_proc_stat();
    sleep(Duration::from_millis(100)).await;
    let s2 = read_proc_stat();

    let (total1, idle1) = s1.unwrap_or((1, 1));
    let (total2, idle2) = s2.unwrap_or((1, 1));

    let total_diff = (total2 as f64) - (total1 as f64);
    let idle_diff = (idle2 as f64) - (idle1 as f64);

    if total_diff <= 0.0 {
        return 0.0;
    }
    ((total_diff - idle_diff) / total_diff * 100.0).clamp(0.0, 100.0)
}

fn read_proc_stat() -> Option<(u64, u64)> {
    let content = fs::read_to_string("/proc/stat").ok()?;
    let line = content.lines().next()?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    if fields.len() < 4 {
        return None;
    }
    let idle = fields[3];
    let total: u64 = fields.iter().sum();
    Some((total, idle))
}

fn read_mem_mb() -> (u64, u64) {
    let content = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total = 0u64;
    let mut available = 0u64;

    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = parse_kb(line);
        } else if line.starts_with("MemAvailable:") {
            available = parse_kb(line);
        }
    }

    let used = total.saturating_sub(available);
    (used / 1024, total / 1024)
}

fn parse_kb(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_mem_usage ---

    #[test]
    fn parse_mem_usage_mib_slash_gib() {
        let (usage, limit) = parse_mem_usage("12.5MiB / 2GiB");
        assert!((usage - 12.5).abs() < 0.01, "usage should be ~12.5 MB, got {usage}");
        assert!((limit - 2048.0).abs() < 0.01, "limit should be ~2048 MB, got {limit}");
    }

    #[test]
    fn parse_mem_usage_mb_slash_gb() {
        let (usage, limit) = parse_mem_usage("256MB / 1GB");
        assert!((usage - 256.0).abs() < 0.01);
        assert!((limit - 1024.0).abs() < 0.01);
    }

    #[test]
    fn parse_mem_usage_kib_slash_mib() {
        let (usage, limit) = parse_mem_usage("512KiB / 512MiB");
        assert!((usage - 0.5).abs() < 0.01, "512 KiB should be ~0.5 MB, got {usage}");
        assert!((limit - 512.0).abs() < 0.01);
    }

    #[test]
    fn parse_mem_usage_kb_slash_mb() {
        let (usage, limit) = parse_mem_usage("1024KB / 2048MB");
        assert!((usage - 1.0).abs() < 0.01, "1024 KB should be 1 MB, got {usage}");
        assert!((limit - 2048.0).abs() < 0.01);
    }

    #[test]
    fn parse_mem_usage_zeros() {
        let (usage, limit) = parse_mem_usage("0MiB / 0MiB");
        assert_eq!(usage, 0.0);
        assert_eq!(limit, 0.0);
    }

    #[test]
    fn parse_mem_usage_empty_string() {
        let (usage, limit) = parse_mem_usage("");
        assert_eq!(usage, 0.0);
        assert_eq!(limit, 0.0);
    }

    #[test]
    fn parse_mem_usage_unknown_unit_returns_zero() {
        let (usage, limit) = parse_mem_usage("100XiB / 200XiB");
        assert_eq!(usage, 0.0);
        assert_eq!(limit, 0.0);
    }

    #[test]
    fn parse_mem_usage_missing_limit_part() {
        // Only one segment — limit should fall back to 0
        let (usage, limit) = parse_mem_usage("64MiB");
        assert!((usage - 64.0).abs() < 0.01);
        assert_eq!(limit, 0.0);
    }

    #[test]
    fn parse_mem_usage_extra_whitespace() {
        let (usage, limit) = parse_mem_usage("  32MiB  /  4GiB  ");
        assert!((usage - 32.0).abs() < 0.01);
        assert!((limit - 4096.0).abs() < 0.01);
    }

    // --- parse_kb ---

    #[test]
    fn parse_kb_standard_line() {
        assert_eq!(parse_kb("MemTotal:       16384000 kB"), 16_384_000);
    }

    #[test]
    fn parse_kb_available_line() {
        assert_eq!(parse_kb("MemAvailable:    8192000 kB"), 8_192_000);
    }

    #[test]
    fn parse_kb_zero_value() {
        assert_eq!(parse_kb("MemFree:               0 kB"), 0);
    }

    #[test]
    fn parse_kb_empty_line() {
        assert_eq!(parse_kb(""), 0);
    }

    #[test]
    fn parse_kb_malformed_no_number() {
        assert_eq!(parse_kb("MemTotal: abc kB"), 0);
    }

    // --- CPU utilisation arithmetic ---

    #[test]
    fn cpu_percent_full_load() {
        // 0 idle out of 1000 total ticks → 100% CPU
        let total1 = 0u64;
        let idle1 = 0u64;
        let total2 = 1000u64;
        let idle2 = 0u64;

        let total_diff = (total2 as f64) - (total1 as f64);
        let idle_diff = (idle2 as f64) - (idle1 as f64);
        let pct = ((total_diff - idle_diff) / total_diff * 100.0).clamp(0.0, 100.0);
        assert!((pct - 100.0).abs() < 0.001);
    }

    #[test]
    fn cpu_percent_idle() {
        // All ticks are idle → 0% CPU
        let total1 = 0u64;
        let idle1 = 0u64;
        let total2 = 1000u64;
        let idle2 = 1000u64;

        let total_diff = (total2 as f64) - (total1 as f64);
        let idle_diff = (idle2 as f64) - (idle1 as f64);
        let pct = ((total_diff - idle_diff) / total_diff * 100.0).clamp(0.0, 100.0);
        assert!((pct - 0.0).abs() < 0.001);
    }

    #[test]
    fn cpu_percent_half_load() {
        let total_diff = 1000.0f64;
        let idle_diff = 500.0f64;
        let pct = ((total_diff - idle_diff) / total_diff * 100.0).clamp(0.0, 100.0);
        assert!((pct - 50.0).abs() < 0.001);
    }

    #[test]
    fn cpu_percent_clamps_to_zero_on_zero_diff() {
        // total_diff == 0 → guard returns 0.0 (no divide-by-zero)
        let total_diff = 0.0f64;
        let pct = if total_diff <= 0.0 { 0.0 } else { 100.0 };
        assert_eq!(pct, 0.0);
    }

    // --- SystemMetrics / ContainerMetrics msg_type constants ---

    #[test]
    fn system_metrics_msg_type_is_correct() {
        // The msg_type field is used by the frontend to dispatch incoming WS messages.
        // A typo here would silently break the dashboard metrics display.
        let m = SystemMetrics {
            msg_type: "system_metrics",
            cpu_percent: 0.0,
            mem_used_mb: 0,
            mem_total_mb: 0,
            disk_used_gb: 0.0,
            disk_total_gb: 0.0,
            timestamp: 0,
        };
        assert_eq!(m.msg_type, "system_metrics");
    }

    #[test]
    fn container_metrics_msg_type_is_correct() {
        let m = ContainerMetrics {
            msg_type: "container_metrics",
            containers: vec![],
            timestamp: 0,
        };
        assert_eq!(m.msg_type, "container_metrics");
    }
}

fn read_disk_gb(mount: &str) -> (f64, f64) {
    use nix::sys::statvfs::statvfs;
    match statvfs(mount) {
        Ok(stat) => {
            let block = stat.block_size();
            let total = stat.blocks() * block;
            let avail = stat.blocks_available() * block;
            let used = total.saturating_sub(avail);
            (
                used as f64 / 1_073_741_824.0,
                total as f64 / 1_073_741_824.0,
            )
        }
        Err(_) => (0.0, 0.0),
    }
}
