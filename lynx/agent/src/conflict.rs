use crate::state::AppState;
use std::{process::Command, sync::atomic::Ordering, time::Duration};
use tokio::time::interval;

const CHECK_INTERVAL_SECS: u64 = 300;

/// Conflicting software list — anything that manages its own firewall or container network
/// can silently bypass nftables rules managed by Lynx.
static INCOMPATIBLE: &[IncompatibleSoftware] = &[
    IncompatibleSoftware {
        name: "docker",
        packages: &["docker-ce", "docker.io", "docker-engine"],
        process: Some("dockerd"),
    },
    IncompatibleSoftware {
        name: "containerd",
        packages: &["containerd", "containerd.io"],
        process: Some("containerd"),
    },
    IncompatibleSoftware {
        name: "firewalld",
        packages: &["firewalld"],
        process: Some("firewalld"),
    },
    IncompatibleSoftware {
        name: "ufw",
        packages: &["ufw"],
        process: None,
    },
    IncompatibleSoftware {
        name: "iptables",
        packages: &["iptables"],
        process: None,
    },
];

struct IncompatibleSoftware {
    name: &'static str,
    packages: &'static [&'static str],
    process: Option<&'static str>,
}

pub async fn run_conflict_check(state: AppState) {
    let mut ticker = interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        check_and_remove(&state).await;
    }
}

async fn check_and_remove(state: &AppState) {
    for software in INCOMPATIBLE {
        if is_present(software) {
            tracing::warn!(software = software.name, "conflicting software detected");

            notify_dashboard(state, software.name, "detected").await;

            match remove(software) {
                Ok(()) => {
                    tracing::info!(software = software.name, "conflicting software removed");
                    notify_dashboard(state, software.name, "removed").await;
                    record_audit(state, software.name, "removed").await;
                }
                Err(e) => {
                    tracing::error!(
                        software = software.name,
                        err = %e,
                        "failed to remove conflicting software — entering lockdown"
                    );
                    notify_dashboard(state, software.name, &format!("removal_failed: {e}")).await;
                    record_audit(state, software.name, &format!("removal_failed: {e}")).await;
                    state.lockdown.store(true, Ordering::SeqCst);
                    return;
                }
            }
        }
    }
}

fn is_present(sw: &IncompatibleSoftware) -> bool {
    // Check if the process is running.
    if let Some(proc_name) = sw.process {
        let running = Command::new("pgrep")
            .args(["-x", proc_name])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if running {
            return true;
        }
    }

    // Check if any matching package is installed.
    // Try dpkg first (Debian/Ubuntu), then rpm (RHEL/CentOS).
    for pkg in sw.packages {
        let installed_dpkg = Command::new("dpkg")
            .args(["-s", pkg])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if installed_dpkg {
            return true;
        }

        let installed_rpm = Command::new("rpm")
            .args(["-q", pkg])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if installed_rpm {
            return true;
        }
    }

    // Special case: iptables — only flag if the direct (non-nft compat) iptables binary
    // is present and pointing to a non-nftables backend.
    if sw.name == "iptables" {
        return is_legacy_iptables();
    }

    false
}

fn is_legacy_iptables() -> bool {
    let out = Command::new("iptables")
        .args(["--version"])
        .output()
        .unwrap_or_else(|_| std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: vec![],
            stderr: vec![],
        });
    let version_str = String::from_utf8_lossy(&out.stdout);
    // If it says "(legacy)" the host uses direct iptables, not the nft compat layer.
    version_str.contains("(legacy)")
}

fn remove(sw: &IncompatibleSoftware) -> anyhow::Result<()> {
    // Try apt-get purge (Debian/Ubuntu).
    if command_exists("apt-get") {
        for pkg in sw.packages {
            let status = Command::new("apt-get")
                .args(["-y", "purge", pkg])
                .status()
                .map_err(|e| anyhow::anyhow!("apt-get: {e}"))?;
            if status.success() {
                return Ok(());
            }
        }
    }

    // Try dnf remove (RHEL/CentOS/Fedora).
    if command_exists("dnf") {
        for pkg in sw.packages {
            let status = Command::new("dnf")
                .args(["-y", "remove", pkg])
                .status()
                .map_err(|e| anyhow::anyhow!("dnf: {e}"))?;
            if status.success() {
                return Ok(());
            }
        }
    }

    anyhow::bail!("no package manager succeeded removing {}", sw.name)
}

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn notify_dashboard(state: &AppState, software: &str, detail: &str) {
    // Best-effort: write to agent_events via audit log sync if dashboard is reachable.
    // This is fire-and-forget — lockdown/removal path doesn't block on this.
    let _ = crate::audit::append(
        &state.db,
        crate::audit::AuditEntry {
            agent_id: state.config.agent_id,
            organization_id: None,
            user_id: None,
            command_type: "conflicting_software_detected",
            result: crate::audit::AuditResult::Failed,
            error: Some(format!("{software}: {detail}")),
        },
    )
    .await;
}

async fn record_audit(state: &AppState, software: &str, action: &str) {
    let _ = crate::audit::append(
        &state.db,
        crate::audit::AuditEntry {
            agent_id: state.config.agent_id,
            organization_id: None,
            user_id: None,
            command_type: "conflicting_software_removed",
            result: crate::audit::AuditResult::Success,
            error: Some(format!("{software}: {action}")),
        },
    )
    .await;
}
