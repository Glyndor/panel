use crate::{
    admin::handlers::rotation,
    crypto::cmd,
    state::AppState,
};
use std::time::Duration;
use tokio::time::interval;
use uuid::Uuid;

const GITHUB_REPO: &str = "Jaro-c/Lynx";
const GITHUB_API_RELEASES: &str = "https://api.github.com/repos/Jaro-c/Lynx/releases";

const CHECK_INTERVAL_SECS: u64 = 3600;
const ROTATION_INTERVAL_DAYS: i64 = 90;

/// Top-level scheduler: runs hourly GitHub release check + periodic cert/key rotation.
pub async fn run(state: AppState) {
    let mut ticker = interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;

        check_releases(&state).await;
        check_rotation(&state).await;
    }
}

// ---------------------------------------------------------------------------
// GitHub release check
// ---------------------------------------------------------------------------

async fn check_releases(state: &AppState) {
    let client = match reqwest::Client::builder()
        .user_agent("lynx-dashboard/1.0")
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("scheduler: failed to build HTTP client: {e}");
            return;
        }
    };

    let releases: Vec<serde_json::Value> = match client
        .get(GITHUB_API_RELEASES)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map(|r| r.json())
    {
        Ok(f) => match f.await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("scheduler: failed to parse GitHub releases: {e}");
                return;
            }
        },
        Err(e) => {
            tracing::warn!("scheduler: GitHub API request failed: {e}");
            return;
        }
    };

    let latest_agent = releases
        .iter()
        .filter_map(|r| r["tag_name"].as_str())
        .filter(|t| t.starts_with("agent@"))
        .map(|t| t.trim_start_matches("agent@"))
        .next()
        .map(|s| s.to_string());

    let latest_dashboard = releases
        .iter()
        .filter_map(|r| r["tag_name"].as_str())
        .filter(|t| t.starts_with("dashboard@"))
        .map(|t| t.trim_start_matches("dashboard@"))
        .next()
        .map(|s| s.to_string());

    if let Some(ref ver) = latest_agent {
        tracing::info!(version = %ver, "scheduler: latest agent release detected");
        *state.latest_agent_version.write().await = Some(ver.clone());
        dispatch_updates_if_needed(state, ver).await;
    }

    if let Some(ref ver) = latest_dashboard {
        let current = env!("CARGO_PKG_VERSION");
        if ver.as_str() != current {
            tracing::info!(latest = %ver, current, "scheduler: dashboard update available");
            trigger_dashboard_update(state, ver).await;
        }
    }
}

async fn dispatch_updates_if_needed(state: &AppState, latest: &str) {
    let outdated = match sqlx::query!(
        "SELECT id, wg_ip::text AS wg_ip, api_port FROM agents \
         WHERE status = 'online' AND (version IS NULL OR version != $1)",
        latest
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("scheduler: failed to query outdated agents: {e}");
            return;
        }
    };

    if outdated.is_empty() {
        return;
    }

    tracing::info!(
        count = outdated.len(),
        version = %latest,
        "scheduler: dispatching update.self to outdated agents"
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("build reqwest client");

    // Use a system user_id sentinel (nil UUID) for scheduler-triggered commands.
    let system_user_id = Uuid::nil();

    for agent in &outdated {
        let download_url = format!(
            "https://github.com/{GITHUB_REPO}/releases/download/agent@{latest}/lynx-agent-linux-x86_64"
        );
        let sig_url = format!(
            "https://github.com/{GITHUB_REPO}/releases/download/agent@{latest}/lynx-agent-linux-x86_64.sig"
        );
        let command = serde_json::json!({
            "type": "update.self",
            "version": latest,
            "download_url": download_url,
            "sig_url": sig_url,
        });

        let signed = match cmd::sign_command(
            &state.config,
            agent.id,
            system_user_id,
            "write",
            &command,
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(agent_id = %agent.id, "scheduler: sign_command failed: {e}");
                continue;
            }
        };

        let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
        let result = client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", &*state.config.internal_token),
            )
            .json(&signed)
            .send()
            .await;

        let log_id = Uuid::now_v7();
        let status = match result {
            Ok(r) if r.status().is_success() => "success",
            _ => "failed",
        };

        let _ = sqlx::query!(
            r#"
            INSERT INTO update_log (id, triggered_by, version, channel, scope, agent_id, status)
            VALUES ($1, NULL, $2, 'stable', 'agent', $3, $4)
            "#,
            log_id,
            latest,
            agent.id,
            status,
        )
        .execute(&state.db)
        .await;

        tracing::info!(agent_id = %agent.id, status, "scheduler: update.self dispatched");
    }
}

async fn trigger_dashboard_update(state: &AppState, version: &str) {
    let github_repo = "Jaro-c/Lynx";
    let backend_url = format!(
        "https://github.com/{github_repo}/releases/download/dashboard@{version}/lynx-dashboard-backend-linux-x86_64"
    );
    let backend_sig = format!("{backend_url}.sig");
    let frontend_url = format!(
        "https://github.com/{github_repo}/releases/download/dashboard@{version}/lynx-dashboard-frontend-linux-x86_64"
    );
    let frontend_sig = format!("{frontend_url}.sig");

    let log_id = Uuid::now_v7();
    let _ = sqlx::query!(
        r#"
        INSERT INTO update_log (id, triggered_by, version, channel, scope, agent_id, status)
        VALUES ($1, NULL, $2, 'stable', 'dashboard', NULL, 'pending')
        "#,
        log_id,
        version,
    )
    .execute(&state.db)
    .await;

    tracing::info!(
        version,
        backend_url = %backend_url,
        backend_sig = %backend_sig,
        frontend_url = %frontend_url,
        frontend_sig = %frontend_sig,
        "scheduler: dashboard self-update initiated"
    );

    // Actual binary swap happens in crate::update (agent-side pattern).
    // For dashboard: download + verify + swap is triggered here, restart handled by Podman.
    tokio::spawn(crate::update::perform_dashboard_update(
        version.to_string(),
        backend_url,
        backend_sig,
        frontend_url,
        frontend_sig,
        log_id,
        state.db.clone(),
    ));
}

// ---------------------------------------------------------------------------
// Scheduled key / cert rotation (every 90 days)
// ---------------------------------------------------------------------------

async fn check_rotation(state: &AppState) {
    let last = sqlx::query_scalar!(
        "SELECT MAX(created_at) FROM rotation_log WHERE reason = 'scheduled'"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(None);

    let needs_rotation = match last {
        None => true,
        Some(ts) => {
            let days_since = (chrono::Utc::now() - ts).num_days();
            days_since >= ROTATION_INTERVAL_DAYS
        }
    };

    if !needs_rotation {
        return;
    }

    tracing::info!("scheduler: 90-day key rotation triggered");

    // JWT session flush — forces all users to re-login with new tokens.
    if let Err(e) = rotation::rotate_jwt_sessions(state).await {
        tracing::warn!("scheduler: JWT session rotation failed: {e}");
    }

    // WireGuard PSK rotation — coordinated without dropping tunnels.
    if let Err(e) = rotation::rotate_wireguard_psks(state, Uuid::nil()).await {
        tracing::warn!("scheduler: WireGuard PSK rotation failed: {e}");
    }

    // mTLS cert rotation — renews certs expiring within 30 days.
    if let Err(e) = rotation::rotate_expiring_certs(state, 30).await {
        tracing::warn!("scheduler: cert rotation failed: {e}");
    }

    let log_id = Uuid::now_v7();
    let _ = sqlx::query!(
        "INSERT INTO rotation_log (id, triggered_by, reason, scope) VALUES ($1, NULL, 'scheduled', 'all')",
        log_id
    )
    .execute(&state.db)
    .await;

    tracing::info!("scheduler: scheduled rotation complete");
}
