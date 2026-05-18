use crate::state::AppState;
use tracing::{error, info, warn};

const CHECK_INTERVAL_SECS: u64 = 60;

pub async fn run_divergence_check(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(CHECK_INTERVAL_SECS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        check_once(&state).await;
    }
}

async fn check_once(state: &AppState) {
    let expected = match state.expected_nft_checksum() {
        Some(c) => c,
        None => return, // no ruleset applied yet
    };

    let current = match super::current_checksum() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "failed to compute nftables checksum");
            return;
        }
    };

    if current == expected {
        return;
    }

    // Detect which chains were modified for appropriate severity / logging.
    let base_diverged   = is_chain_diverged(state, "lynx-base");
    let global_diverged = is_chain_diverged(state, "lynx-global");
    let local_diverged  = is_chain_diverged(state, "lynx-local");

    if base_diverged {
        error!(
            expected = %&expected[..16],
            current  = %&current[..16],
            "CRITICAL: lynx-base chain modified outside Lynx — auto-restoring"
        );
    } else {
        warn!(
            expected = %&expected[..16],
            current  = %&current[..16],
            base_diverged,
            global_diverged,
            local_diverged,
            "nftables divergence detected — auto-restoring"
        );
    }

    // Auto-restore in all cases — PostgreSQL is the source of truth, not the VPS.
    if let Err(e) = restore(state) {
        error!(error = %e, "nftables auto-restore FAILED — applying emergency ruleset");
        if let Err(e2) = super::apply_emergency() {
            error!(error = %e2, "emergency ruleset also failed — lockdown");
        }
        state.lockdown.store(true, std::sync::atomic::Ordering::SeqCst);
    } else {
        info!("nftables auto-restored successfully");
    }

    let chain = if base_diverged {
        "lynx-base"
    } else if global_diverged {
        "lynx-global"
    } else if local_diverged {
        "lynx-local"
    } else {
        "unknown"
    };

    notify_dashboard(state, chain, base_diverged).await;
}

fn is_chain_diverged(_state: &AppState, chain: &str) -> bool {
    // We don't store per-chain expected checksums, so approximate by checking
    // if the chain is accessible. If nft fails (chain deleted), that's divergence.
    // For base specifically, any table-level divergence implies base was touched
    // if global/local weren't modified — conservative assumption.
    match super::chain_checksum(chain) {
        Ok(_) => false, // chain exists; full-table divergence already confirmed
        Err(_) => true, // chain missing or inaccessible = definitely diverged
    }
}

fn restore(state: &AppState) -> anyhow::Result<()> {
    let last = state
        .nft_last_ruleset()
        .ok_or_else(|| anyhow::anyhow!("no last ruleset to restore"))?;

    super::apply_raw(&last)?;

    // Update expected checksum to match what we just applied.
    let checksum = super::current_checksum()?;
    state.set_nft_checksum(checksum);
    Ok(())
}

async fn notify_dashboard(state: &AppState, chain: &str, critical: bool) {
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
        "detail": format!("chain={chain} critical={critical} auto_restored=true"),
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
        Ok(r) if r.status().is_success() => info!("nftables divergence event sent"),
        Ok(r) => warn!(status = %r.status(), "dashboard rejected divergence event"),
        Err(e) => warn!(error = %e, "failed to send divergence event"),
    }
}
