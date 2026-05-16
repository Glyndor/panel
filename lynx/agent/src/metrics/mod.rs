use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Serialize)]
pub struct SystemMetrics {
    pub cpu_percent: f64,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
    pub timestamp: i64,
}

/// Continuously sample and yield metrics. Called from WS handler.
pub async fn stream_metrics(
    mut send: impl FnMut(SystemMetrics) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>,
) -> Result<()> {
    loop {
        let metrics = sample().await?;
        send(metrics).await?;
        sleep(Duration::from_secs(2)).await;
    }
}

pub async fn sample() -> Result<SystemMetrics> {
    let cpu = read_cpu_percent().await;
    let (mem_used, mem_total) = read_mem_mb();
    let (disk_used, disk_total) = read_disk_gb("/");

    Ok(SystemMetrics {
        cpu_percent: cpu,
        mem_used_mb: mem_used,
        mem_total_mb: mem_total,
        disk_used_gb: disk_used,
        disk_total_gb: disk_total,
        timestamp: chrono::Utc::now().timestamp(),
    })
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
