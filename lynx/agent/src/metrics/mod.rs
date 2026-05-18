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
        cpu_perc: String,   // "1.23%"
        #[serde(rename = "MemUsage")]
        mem_usage: String,  // "12.5MB / 2GB"
    }

    let stats: Vec<RawStat> = serde_json::from_slice(&out).unwrap_or_default();

    stats.into_iter().map(|s| {
        let cpu = s.cpu_perc.trim_end_matches('%').parse::<f64>().unwrap_or(0.0);
        let (usage, limit) = parse_mem_usage(&s.mem_usage);
        ContainerStat {
            id: s.id,
            name: s.name,
            cpu_percent: cpu,
            mem_usage_mb: usage,
            mem_limit_mb: limit,
        }
    }).collect()
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

fn read_disk_gb(mount: &str) -> (f64, f64) {
    use nix::sys::statvfs::statvfs;
    match statvfs(mount) {
        Ok(stat) => {
            let block = stat.block_size() as u64;
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
