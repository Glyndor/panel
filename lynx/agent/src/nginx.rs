use crate::{
    audit::{self, AuditEntry, AuditResult},
    state::AppState,
};
use std::time::Duration;
use tokio::time::interval;

const CONTAINER_NAME: &str = "lynx-nginx";
const HEALTH_CHECK_INTERVAL_SECS: u64 = 60;
const HEALTH_URL: &str = "http://127.0.0.1:80/_health";
const MAX_REDEPLOY_ATTEMPTS: u32 = 3;

pub async fn run_nginx_watchdog(state: AppState) {
    let mut ticker = interval(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        check_nginx(&state).await;
    }
}

async fn check_nginx(state: &AppState) {
    let container_exists = is_container_running(CONTAINER_NAME).await;

    if !container_exists {
        // Container is gone — restart: always can't recover a removed container.
        // Check if it was running recently (podman ps -a includes exited).
        let ever_existed = container_ever_existed(CONTAINER_NAME).await;
        if ever_existed {
            tracing::warn!("nginx container missing — re-deploying");
            redeploy_nginx(state).await;
        }
        return;
    }

    // Container running — check HTTP health.
    if !http_health_ok().await {
        tracing::warn!("nginx health check failed — restoring config from DB");
        restore_nginx_config(state).await;
    }
}

async fn is_container_running(name: &str) -> bool {
    let out = std::process::Command::new("podman")
        .args([
            "ps",
            "--filter",
            &format!("name={name}"),
            "--filter",
            "status=running",
            "--format",
            "{{.Names}}",
        ])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(name),
        Err(_) => false,
    }
}

async fn container_ever_existed(name: &str) -> bool {
    let out = std::process::Command::new("podman")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("name={name}"),
            "--format",
            "{{.Names}}",
        ])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).contains(name),
        Err(_) => false,
    }
}

async fn http_health_ok() -> bool {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get(HEALTH_URL)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn redeploy_nginx(state: &AppState) {
    for attempt in 1..=MAX_REDEPLOY_ATTEMPTS {
        let backoff = Duration::from_secs(2u64.pow(attempt));

        let status = std::process::Command::new("podman")
            .args(["run", "--restart=always", "-d", "--name", CONTAINER_NAME,
                   "-p", "80:80", "-p", "443:443",
                   "docker.io/library/nginx@sha256:ceba1c7f1e2c42e5f43c9fa55e74ef90a1d08e7fde12f25e2a6706f4c80e0428"])
            .status();

        match status {
            Ok(s) if s.success() => {
                tracing::info!(attempt, "nginx re-deployed successfully");
                audit_nginx_event(
                    state,
                    "nginx_unexpected_stop",
                    "re-deployed after container removal",
                )
                .await;
                return;
            }
            _ => {
                tracing::warn!(attempt, "nginx re-deploy attempt failed");
                if attempt < MAX_REDEPLOY_ATTEMPTS {
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }

    tracing::error!(
        "nginx re-deploy failed after {} attempts — manual intervention required",
        MAX_REDEPLOY_ATTEMPTS
    );
    audit_nginx_event(
        state,
        "nginx_unexpected_stop",
        "re-deploy failed after 3 attempts",
    )
    .await;
}

async fn restore_nginx_config(state: &AppState) {
    // Load config from agent DB (stored by dashboard when domain was configured).
    let config_row = sqlx::query_scalar!(
        "SELECT config_content FROM nginx_configs ORDER BY updated_at DESC LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await;

    let config = match config_row {
        Ok(Some(c)) => c,
        _ => {
            tracing::warn!("no nginx config in DB — cannot restore");
            return;
        }
    };

    let config_path = "/etc/nginx/conf.d/lynx.conf";
    if let Err(e) = std::fs::write(config_path, config) {
        tracing::error!("failed to write nginx config: {e}");
        return;
    }

    // Reload nginx inside the container.
    let reload = std::process::Command::new("podman")
        .args(["exec", CONTAINER_NAME, "nginx", "-s", "reload"])
        .status();

    match reload {
        Ok(s) if s.success() => {
            tracing::info!("nginx config restored and reloaded");
            audit_nginx_event(state, "nginx_config_tampered", "config restored from DB").await;
        }
        _ => {
            tracing::error!("nginx reload failed after config restore");
        }
    }
}

async fn audit_nginx_event(state: &AppState, event: &str, detail: &str) {
    let _ = audit::append(
        &state.db,
        AuditEntry {
            agent_id: state.config.agent_id,
            organization_id: None,
            user_id: None,
            command_type: event,
            result: AuditResult::Success,
            error: Some(detail.to_string()),
        },
    )
    .await;
}
