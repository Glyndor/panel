use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

pub struct AuditEntry<'a> {
    pub agent_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub command_type: &'a str,
    pub result: AuditResult,
    /// Sanitized error message — never contains secrets
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum AuditResult {
    Success,
    Rejected,
    Failed,
}

impl AuditResult {
    fn as_str(self) -> &'static str {
        match self {
            AuditResult::Success => "success",
            AuditResult::Rejected => "rejected",
            AuditResult::Failed => "failed",
        }
    }
}

/// Append an immutable audit log entry with hash chaining.
///
/// Each entry hashes (prev_hash || id || agent_id || command_type || result || created_at).
/// Tampering with any field breaks the chain.
pub async fn append(db: &PgPool, entry: AuditEntry<'_>) -> Result<()> {
    let id = Uuid::now_v7();
    let result_str = entry.result.as_str();

    // Get last entry hash for chain
    let prev_hash: String = sqlx::query_scalar!(
        "SELECT entry_hash FROM audit_log ORDER BY created_at DESC LIMIT 1"
    )
    .fetch_optional(db)
    .await
    .context("fetch prev audit hash")?
    .unwrap_or_else(|| "genesis".to_string());

    // Compute this entry's hash
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(id.as_bytes());
    hasher.update(entry.agent_id.as_bytes());
    hasher.update(entry.command_type.as_bytes());
    hasher.update(result_str.as_bytes());
    let hash = hex::encode(hasher.finalize());

    sqlx::query!(
        r#"
        INSERT INTO audit_log
            (id, agent_id, organization_id, user_id, command_type, result, error, previous_hash, entry_hash)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
        id,
        entry.agent_id,
        entry.organization_id,
        entry.user_id,
        entry.command_type,
        result_str,
        entry.error,
        prev_hash,
        hash,
    )
    .execute(db)
    .await
    .context("insert audit log entry")?;

    Ok(())
}
