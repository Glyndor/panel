use crate::state::AppState;
use std::time::Duration;

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
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build reqwest client");

    for agent in agents {
        let url = format!("http://{}:{}/heartbeat", agent.wg_ip, agent.api_port);
        let id = agent.id;
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await;

        let new_status = match resp {
            Ok(r) if r.status().is_success() => "online",
            Ok(r) if r.status().as_u16() == 423 => "lockdown",
            _ => "offline",
        };

        if let Err(e) = sqlx::query!(
            "UPDATE agents SET status=$1, last_heartbeat=NOW() WHERE id=$2",
            new_status,
            id
        )
        .execute(&state.db)
        .await
        {
            tracing::warn!(agent_id = %id, err = %e, "heartbeat scheduler: failed to update agent status");
        } else {
            tracing::debug!(agent_id = %id, status = new_status, "heartbeat polled");
        }
    }
}
