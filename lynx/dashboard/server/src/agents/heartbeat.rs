use crate::{crypto::cmd, state::AppState};
use std::time::Duration;
use uuid::Uuid;

const HEARTBEAT_INTERVAL_SECS: u64 = 30;

pub async fn run_scheduler(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        poll_agents(&state).await;
    }
}

async fn poll_agents(state: &AppState) {
    let agents = match sqlx::query!(
        "SELECT id, wg_ip::text AS wg_ip, api_port FROM agents WHERE status != 'lockdown'"
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(err = %e, "heartbeat scheduler: failed to fetch agents");
            return;
        }
    };

    let token = &*state.config.internal_token;
    let client = super::client::build_agent_client_with_timeout(
        &state.config,
        Duration::from_secs(5),
    );

    let latest = state.latest_agent_version.read().await.clone();

    for agent in agents {
        let url = format!("http://{}:{}/heartbeat", agent.wg_ip, agent.api_port);
        let id = agent.id;
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await;

        let (new_status, reported_version) = match resp {
            Ok(r) if r.status().is_success() => {
                let ver = r
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| v["version"].as_str().map(|s| s.to_string()));
                ("online", ver)
            }
            Ok(r) if r.status().as_u16() == 423 => ("lockdown", None),
            _ => ("offline", None),
        };

        // Build update query: always update status/heartbeat, conditionally update version.
        if let Some(ref ver) = reported_version {
            let _ = sqlx::query!(
                "UPDATE agents SET status=$1, last_heartbeat=NOW(), version=$2 WHERE id=$3",
                new_status,
                ver,
                id
            )
            .execute(&state.db)
            .await;
        } else {
            let _ = sqlx::query!(
                "UPDATE agents SET status=$1, last_heartbeat=NOW() WHERE id=$2",
                new_status,
                id
            )
            .execute(&state.db)
            .await;
        }

        tracing::debug!(agent_id = %id, status = new_status, version = ?reported_version, "heartbeat polled");

        // Trigger update.self if agent is online, version known, and outdated.
        if new_status == "online" {
            if let Some(ref current) = reported_version {
                if let Some(ref target) = latest {
                    if current != target {
                        dispatch_update(state, id, &agent.wg_ip, agent.api_port, target).await;
                    }
                }
            }
        }
    }
}

async fn dispatch_update(
    state: &AppState,
    agent_id: Uuid,
    wg_ip: &str,
    api_port: i32,
    version: &str,
) {
    let github_repo = "Jaro-c/Lynx";
    let download_url = format!(
        "https://github.com/{github_repo}/releases/download/agent@{version}/lynx-agent-linux-x86_64"
    );
    let sig_url = format!(
        "https://github.com/{github_repo}/releases/download/agent@{version}/lynx-agent-linux-x86_64.sig"
    );
    let command = serde_json::json!({
        "type": "update.self",
        "version": version,
        "download_url": download_url,
        "sig_url": sig_url,
    });

    let signed = match cmd::sign_command(
        &state.config,
        agent_id,
        Uuid::nil(),
        "write",
        &command,
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(agent_id = %agent_id, "heartbeat: sign_command failed: {e}");
            return;
        }
    };

    let client = super::client::build_agent_client_with_timeout(
        &state.config,
        Duration::from_secs(10),
    );

    let url = format!("http://{wg_ip}:{api_port}/cmd");
    let result = client
        .post(&url)
        .header(
            "Authorization",
            format!("Bearer {}", &*state.config.internal_token),
        )
        .json(&signed)
        .send()
        .await;

    match result {
        Ok(r) if r.status().is_success() => {
            tracing::info!(agent_id = %agent_id, version, "heartbeat: update.self dispatched");
        }
        Ok(r) => {
            tracing::warn!(agent_id = %agent_id, status = %r.status(), "heartbeat: update.self rejected");
        }
        Err(e) => {
            tracing::warn!(agent_id = %agent_id, "heartbeat: update.self delivery failed: {e}");
        }
    }
}
