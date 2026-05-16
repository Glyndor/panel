use crate::state::AppState;
use serde::Serialize;
use sqlx::PgPool;
use tracing::{error, info, warn};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: uuid::Uuid,
    pub agent_id: uuid::Uuid,
    pub organization_id: Option<uuid::Uuid>,
    pub user_id: Option<uuid::Uuid>,
    pub command_type: String,
    pub result: String,
    pub error: Option<String>,
    pub previous_hash: String,
    pub entry_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const BATCH_SIZE: i64 = 100;
const SYNC_INTERVAL_SECS: u64 = 60;

pub async fn run_sync_task(state: AppState) {
    let Some(dashboard_url) = &state.config.dashboard_url else {
        warn!("DASHBOARD_URL not set — audit log sync disabled");
        return;
    };
    let Some(sync_token) = &state.config.sync_token else {
        warn!("SYNC_TOKEN not set — audit log sync disabled");
        return;
    };

    let sync_url = format!(
        "{}/agents/{}/audit-sync",
        dashboard_url.trim_end_matches('/'),
        state.config.agent_id
    );
    let token = sync_token.clone();
    let db = state.db.clone();

    info!(sync_url = %sync_url, "audit sync task started");

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(SYNC_INTERVAL_SECS));
    loop {
        interval.tick().await;
        if let Err(e) = sync_batch(&db, &sync_url, &token).await {
            error!(error = %e, "audit log sync failed");
        }
    }
}

async fn sync_batch(db: &PgPool, url: &str, token: &str) -> anyhow::Result<()> {
    let last_synced = sqlx::query_scalar!("SELECT last_synced_at FROM sync_state WHERE id = 1")
        .fetch_one(db)
        .await?;

    let entries = sqlx::query_as!(
        AuditEntry,
        r#"
        SELECT id, agent_id, organization_id, user_id, command_type,
               result, error, previous_hash, entry_hash, created_at
        FROM audit_log
        WHERE created_at > $1
        ORDER BY created_at ASC
        LIMIT $2
        "#,
        last_synced,
        BATCH_SIZE
    )
    .fetch_all(db)
    .await?;

    if entries.is_empty() {
        return Ok(());
    }

    let count = entries.len();
    let last_at = entries.last().unwrap().created_at;

    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {token}"))
        .json(&entries)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "dashboard returned {}: {}",
            status,
            &body[..body.len().min(200)]
        );
    }

    sqlx::query!(
        "UPDATE sync_state SET last_synced_at = $1 WHERE id = 1",
        last_at
    )
    .execute(db)
    .await?;

    info!(count, "audit entries synced to dashboard");
    Ok(())
}
