use crate::state::AppState;
use tracing::{info, warn};

const CHECK_INTERVAL_SECS: u64 = 60;

/// Background task: periodically compares live nftables checksum against
/// the last-known-good checksum stored in AppState. On mismatch, alerts dashboard.
pub async fn run_divergence_check(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(CHECK_INTERVAL_SECS));
    loop {
        interval.tick().await;
        check_once(&state).await;
    }
}

async fn check_once(state: &AppState) {
    let expected = match state.expected_nft_checksum() {
        Some(c) => c,
        None => return, // no ruleset applied yet — nothing to compare
    };

    let current = match super::current_checksum() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to compute nftables checksum");
            return;
        }
    };

    if current == expected {
        return; // clean
    }

    warn!(
        expected = %&expected[..16],
        current  = %&current[..16],
        "nftables divergence detected — ruleset modified outside Lynx"
    );

    notify_dashboard(state, &current, &expected).await;
}

async fn notify_dashboard(state: &AppState, current: &str, expected: &str) {
    let Some(dashboard_url) = &state.config.dashboard_url else {
        return;
    };
    let Some(sync_token) = &state.config.sync_token else {
        return;
    };

    let url = format!(
        "{}/agents/{}/events",
        dashboard_url.trim_end_matches('/'),
        state.config.agent_id
    );

    let body = serde_json::json!({
        "event": "nftables_divergence",
        "detail": format!("current={} expected={}", &current[..16], &expected[..16]),
    });

    let client = reqwest::Client::new();
    match client
        .post(&url)
        .header("Authorization", format!("Bearer {}", &**sync_token))
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {
            info!("nftables divergence event sent to dashboard");
        }
        Ok(r) => warn!(status = %r.status(), "dashboard rejected divergence event"),
        Err(e) => warn!(error = %e, "failed to send divergence event to dashboard"),
    }
}
