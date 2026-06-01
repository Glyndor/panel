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

    if resp.status().as_u16() == 422 {
        // Dashboard rejected the batch due to hash chain mismatch — our sync cursor
        // is ahead of what the dashboard has (e.g. dashboard DB was wiped or restored
        // from a backup). Reset to epoch so the next cycle resends from genesis.
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!(
            detail = &body[..body.len().min(200)],
            "audit sync: hash chain mismatch — resetting sync cursor to epoch for full resend"
        );
        let epoch = chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap_or_default();
        sqlx::query!(
            "UPDATE sync_state SET last_synced_at = $1 WHERE id = 1",
            epoch
        )
        .execute(db)
        .await?;
        return Ok(());
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    // sync/mod.rs has no pure logic functions — all paths require either a live
    // PostgreSQL connection (sync_batch) or a running Tokio runtime with network
    // access (run_sync_task).  Those are integration tests executed against real
    // containers in CI, not unit tests.
    //
    // What we CAN test here:
    //   • Module-level constants used to shape behaviour
    //   • URL construction pattern (pure string formatting)
    //   • AuditEntry struct is serialisable (compile-time guarantee via Serialize)

    #[test]
    fn batch_size_is_positive() {
        assert!(BATCH_SIZE > 0, "BATCH_SIZE must be greater than zero");
    }

    #[test]
    fn sync_interval_is_positive() {
        assert!(
            SYNC_INTERVAL_SECS > 0,
            "SYNC_INTERVAL_SECS must be greater than zero"
        );
    }

    #[test]
    fn sync_url_format_includes_agent_id_and_path() {
        // Replicate the URL construction from run_sync_task to ensure the format
        // string produces the expected shape — pure string operation, no I/O.
        let dashboard_url = "https://dashboard.example.com/";
        let agent_id = uuid::Uuid::nil(); // all-zeros UUID — no DB needed
        let sync_url = format!(
            "{}/agents/{}/audit-sync",
            dashboard_url.trim_end_matches('/'),
            agent_id
        );
        assert_eq!(
            sync_url,
            "https://dashboard.example.com/agents/00000000-0000-0000-0000-000000000000/audit-sync"
        );
    }

    #[test]
    fn sync_url_trailing_slash_stripped() {
        let base = "https://dashboard.example.com/";
        let trimmed = base.trim_end_matches('/');
        assert_eq!(trimmed, "https://dashboard.example.com");
    }

    #[test]
    fn sync_url_no_trailing_slash_unchanged() {
        let base = "https://dashboard.example.com";
        let trimmed = base.trim_end_matches('/');
        assert_eq!(trimmed, "https://dashboard.example.com");
    }

    #[test]
    fn audit_entry_result_field_is_string() {
        // Compile-time check that AuditEntry derives Serialize and has the expected
        // field types.  We construct one manually (no DB) using placeholder values.
        let entry = AuditEntry {
            id: uuid::Uuid::nil(),
            agent_id: uuid::Uuid::nil(),
            organization_id: None,
            user_id: None,
            command_type: "test_command".to_string(),
            result: "success".to_string(),
            error: None,
            previous_hash: "abc123".to_string(),
            entry_hash: "def456".to_string(),
            created_at: chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).unwrap_or_default(),
        };

        // Verify serialization produces valid JSON and key fields are present.
        let json = serde_json::to_string(&entry).expect("AuditEntry should serialize");
        assert!(json.contains("test_command"));
        assert!(json.contains("success"));
        assert!(json.contains("abc123"));
    }
}
