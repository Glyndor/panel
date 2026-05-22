use crate::state::AppState;
use anyhow::Context as _;
use std::sync::atomic::Ordering;
use tokio::time::{interval, Duration};

/// How long the dashboard must be unreachable before the agent polls GitHub directly.
const ABSENT_THRESHOLD_SECS: u64 = 6 * 3600;

/// How often the fallback updater checks if an update is needed.
const CHECK_INTERVAL_SECS: u64 = 3600;

const GITHUB_API: &str = "https://api.github.com/repos/Jaro-c/Lynx/releases";

pub async fn run_fallback_updater(state: AppState) {
    let mut ticker = interval(Duration::from_secs(CHECK_INTERVAL_SECS));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;

        // No dashboard URL = agent not yet onboarded. Skip fallback updates entirely:
        // without a dashboard the agent is in setup mode and the WS connection will
        // never establish last_contact, which would otherwise immediately satisfy the
        // absence threshold on every restart and cause an infinite update loop.
        if state.config.dashboard_url.is_none() {
            continue;
        }

        let last_contact = state.last_dashboard_contact.load(Ordering::SeqCst);
        let now = epoch_secs();

        // 0 = never connected. If we have never connected, use a past epoch so the absence
        // threshold is immediately satisfied — agent might have been offline since install.
        let absent_secs = if last_contact == 0 {
            ABSENT_THRESHOLD_SECS + 1
        } else {
            now.saturating_sub(last_contact)
        };

        if absent_secs <= ABSENT_THRESHOLD_SECS {
            continue;
        }

        tracing::info!(
            absent_secs,
            "dashboard absent — checking GitHub for agent update"
        );

        if let Err(e) = check_and_apply(&state).await {
            tracing::warn!(error = %e, "fallback updater check failed");
        }
    }
}

async fn check_and_apply(state: &AppState) -> anyhow::Result<()> {
    let current_version = &state.config.version;
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        a => a,
    };

    let latest = fetch_latest_agent_version().await?;

    if !is_newer(&latest, current_version) {
        tracing::debug!(current = current_version, latest, "agent is up to date");
        return Ok(());
    }

    tracing::info!(
        current = current_version,
        latest,
        "fallback: applying agent update"
    );

    let download_url = format!(
        "https://github.com/Jaro-c/Lynx/releases/download/agent@{latest}/lynx-agent-linux-{arch}"
    );
    let sig_url = format!("{download_url}.sig");

    super::perform_update(&latest, &download_url, &sig_url).await
}

async fn fetch_latest_agent_version() -> anyhow::Result<String> {
    let client = super::build_ssrf_safe_client(GITHUB_API)
        .await
        .context("SSRF check for GitHub API")?;

    let releases: serde_json::Value = client
        .get(GITHUB_API)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let releases = releases
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("GitHub API returned non-array"))?;

    for release in releases {
        let tag = release
            .get("tag_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(ver) = tag.strip_prefix("agent@") {
            return Ok(ver.to_string());
        }
    }

    anyhow::bail!("no agent@* release found in GitHub releases")
}

/// Returns true if `latest` is strictly newer than `current` (semver comparison).
fn is_newer(latest: &str, current: &str) -> bool {
    parse_semver(latest) > parse_semver(current)
}

fn parse_semver(v: &str) -> (u64, u64, u64) {
    let parts: Vec<u64> = v
        .trim_start_matches('v')
        .splitn(3, '.')
        .map(|s| s.parse().unwrap_or(0))
        .collect();
    (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    )
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_semver ---

    #[test]
    fn parse_semver_normal() {
        assert_eq!(parse_semver("1.2.3"), (1, 2, 3));
    }

    #[test]
    fn parse_semver_v_prefix() {
        assert_eq!(parse_semver("v2.0.0"), (2, 0, 0));
    }

    #[test]
    fn parse_semver_missing_patch() {
        assert_eq!(parse_semver("1.0"), (1, 0, 0));
    }

    #[test]
    fn parse_semver_empty() {
        assert_eq!(parse_semver(""), (0, 0, 0));
    }

    #[test]
    fn parse_semver_garbage() {
        assert_eq!(parse_semver("garbage"), (0, 0, 0));
    }

    #[test]
    fn parse_semver_zeros() {
        assert_eq!(parse_semver("0.0.0"), (0, 0, 0));
    }

    // --- is_newer ---

    #[test]
    fn is_newer_patch_bump() {
        assert!(is_newer("1.2.0", "1.1.0"));
    }

    #[test]
    fn is_newer_older_version_is_false() {
        assert!(!is_newer("1.1.0", "1.2.0"));
    }

    #[test]
    fn is_newer_equal_versions_is_false() {
        assert!(!is_newer("1.0.0", "1.0.0"));
    }

    #[test]
    fn is_newer_major_bump() {
        assert!(is_newer("2.0.0", "1.9.9"));
    }

    #[test]
    fn is_newer_v_prefix_stripped() {
        assert!(is_newer("v1.2.0", "1.1.0"));
    }

    #[test]
    fn is_newer_both_v_prefix() {
        assert!(is_newer("v2.1.0", "v2.0.9"));
    }

    #[test]
    fn is_newer_minor_rollback_is_false() {
        assert!(!is_newer("1.0.0", "1.0.1"));
    }
}
