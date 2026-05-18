pub mod handlers;
pub mod router;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MigrationState {
    pub id: i32,
    pub status: String,
    pub role: String,
    pub target_url: Option<String>,
    pub agents_total: i32,
    pub agents_confirmed: i32,
    pub error_message: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// Sent by VPS-A to initiate migration to VPS-B.
#[derive(Debug, Deserialize)]
pub struct StartMigrationRequest {
    /// Public URL of VPS-B dashboard (e.g. "https://1.2.3.4:19443")
    pub target_url: String,
    /// One-time migration token displayed on VPS-B
    pub migration_token: String,
}

/// Sent by admin on VPS-B to put it in receive mode.
#[derive(Debug, Serialize, Deserialize)]
pub struct PrepareMigrationResponse {
    /// One-time token to enter into VPS-A
    pub migration_token: String,
}

/// Payload VPS-A sends to VPS-B's /migration/receive endpoint.
/// Body is a raw gzipped pg_dump streamed as multipart.
#[derive(Debug, Deserialize)]
pub struct AgentConfirmRequest {
    pub agent_id: String,
}
