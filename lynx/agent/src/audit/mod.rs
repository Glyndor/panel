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

/// Compute the SHA-256 hash for an audit log entry.
///
/// Covers all explicitly written fields (excludes created_at which is DB-generated
/// and can lose sub-millisecond precision on EXTRACT round-trips).
/// Input: prev_hash || id || agent_id || org_id || user_id || command_type || result || error
#[allow(clippy::too_many_arguments)]
fn compute_entry_hash(
    prev_hash: &str,
    id: Uuid,
    agent_id: Uuid,
    organization_id: Option<Uuid>,
    user_id: Option<Uuid>,
    command_type: &str,
    result: &str,
    error: Option<&str>,
) -> String {
    let mut h = Sha256::new();
    h.update(prev_hash.as_bytes());
    h.update(id.as_bytes());
    h.update(agent_id.as_bytes());
    h.update(organization_id.map(|u| *u.as_bytes()).unwrap_or([0u8; 16]));
    h.update(user_id.map(|u| *u.as_bytes()).unwrap_or([0u8; 16]));
    h.update(command_type.as_bytes());
    h.update(result.as_bytes());
    h.update(error.unwrap_or("").as_bytes());
    hex::encode(h.finalize())
}

/// Append an immutable audit log entry with hash chaining.
///
/// Before inserting, verifies that the last entry's stored `entry_hash` still
/// matches a re-computation from its data — detects any tampering of previous
/// entries. If a mismatch is found, logs a critical alert and returns an error
/// rather than silently continuing with a broken chain.
pub async fn append(db: &PgPool, entry: AuditEntry<'_>) -> Result<()> {
    let id = Uuid::now_v7();
    let result_str = entry.result.as_str();

    // Fetch full last entry for chain verification
    let last = sqlx::query!(
        r#"SELECT entry_hash, previous_hash,
               id as "id: Uuid",
               agent_id as "agent_id: Uuid",
               organization_id as "organization_id: Uuid",
               user_id as "user_id: Uuid",
               command_type, result, error
           FROM audit_log ORDER BY created_at DESC LIMIT 1"#
    )
    .fetch_optional(db)
    .await
    .context("fetch last audit entry")?;

    let prev_hash = if let Some(ref last) = last {
        // Verify last entry's hash integrity before extending the chain
        let expected = compute_entry_hash(
            &last.previous_hash,
            last.id,
            last.agent_id,
            last.organization_id,
            last.user_id,
            &last.command_type,
            &last.result,
            last.error.as_deref(),
        );
        if expected != last.entry_hash {
            tracing::error!(
                stored_hash = %last.entry_hash,
                computed_hash = %expected,
                last_entry_id = %last.id,
                "AUDIT LOG INTEGRITY VIOLATION — hash chain broken, last entry was tampered"
            );
            anyhow::bail!("audit log integrity violation: hash chain broken");
        }
        last.entry_hash.clone()
    } else {
        "genesis".to_string()
    };

    let hash = compute_entry_hash(
        &prev_hash,
        id,
        entry.agent_id,
        entry.organization_id,
        entry.user_id,
        entry.command_type,
        result_str,
        entry.error.as_deref(),
    );

    sqlx::query!(
        r#"
        INSERT INTO audit_log
            (id, agent_id, organization_id, user_id, command_type, result, error,
             previous_hash, entry_hash)
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
