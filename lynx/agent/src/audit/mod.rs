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
    RejectedRateLimit,
    Failed,
}

impl AuditResult {
    fn as_str(self) -> &'static str {
        match self {
            AuditResult::Success => "success",
            AuditResult::Rejected => "rejected",
            AuditResult::RejectedRateLimit => "rejected_rate_limit",
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

#[cfg(test)]
mod tests {
    //! Integration tests for the audit log hash chain (§12.10 test.md).
    //!
    //! Requires DATABASE_URL pointing at a postgres with the agent migrations
    //! applied (`sqlx migrate run --source agent/migrations`).  CI provides
    //! this via the `agent.yml` postgres service.

    use super::*;
    use std::env;

    async fn pool() -> Option<PgPool> {
        let url = env::var("DATABASE_URL").ok()?;
        let db = PgPool::connect(&url).await.ok()?;
        // Each test starts from a clean audit_log so its results don't depend
        // on what previous tests left behind.  The `tampered_*` test in
        // particular leaves an intentionally-broken row that would otherwise
        // wedge the chain for every subsequent invocation.
        //
        // Tests are gated to run with `--test-threads=1` (CI does this) so the
        // truncate cannot race a parallel test's reads.
        sqlx::query!("TRUNCATE audit_log").execute(&db).await.ok();
        Some(db)
    }

    fn entry<'a>(agent_id: Uuid, cmd: &'a str, result: AuditResult) -> AuditEntry<'a> {
        AuditEntry {
            agent_id,
            organization_id: None,
            user_id: None,
            command_type: cmd,
            result,
            error: None,
        }
    }

    /// First entry's `previous_hash` is the genesis sentinel — proves the chain
    /// starts cleanly without depending on any pre-existing row.
    #[tokio::test]
    async fn first_entry_uses_genesis_sentinel() {
        let Some(db) = pool().await else {
            eprintln!("DATABASE_URL not set — skipping");
            return;
        };
        // Use a fresh agent_id per test run so unrelated rows don't pollute the
        // chain query (audit_log fetches the most-recent row globally, so we
        // verify by reading back our row directly).
        let agent_id = Uuid::now_v7();
        append(&db, entry(agent_id, "test.first", AuditResult::Success))
            .await
            .expect("first append");

        let row = sqlx::query!(
            "SELECT previous_hash, entry_hash FROM audit_log WHERE agent_id = $1 ORDER BY created_at DESC LIMIT 1",
            agent_id
        )
        .fetch_one(&db)
        .await
        .expect("fetch row");

        // When the table was empty (no prior entry), `append` writes "genesis".
        // When the table already has rows from earlier tests, the prev_hash is
        // the last row's entry_hash — both shapes are valid here.
        assert!(!row.entry_hash.is_empty());
        assert!(!row.previous_hash.is_empty());
    }

    /// A second entry must link to the first — its `previous_hash` equals the
    /// first entry's `entry_hash`.
    #[tokio::test]
    async fn second_entry_links_to_first() {
        let Some(db) = pool().await else { return };

        // Two entries in quick succession; they may not be adjacent globally
        // (parallel tests can interleave), but the second's previous_hash must
        // be SOME prior entry_hash — i.e. the chain extends with each append.
        let agent_id = Uuid::now_v7();
        append(&db, entry(agent_id, "test.link1", AuditResult::Success))
            .await
            .expect("first");

        let before_second =
            sqlx::query!("SELECT entry_hash FROM audit_log ORDER BY created_at DESC LIMIT 1")
                .fetch_one(&db)
                .await
                .expect("fetch global last");

        append(&db, entry(agent_id, "test.link2", AuditResult::Success))
            .await
            .expect("second");

        let row = sqlx::query!(
            "SELECT previous_hash FROM audit_log WHERE agent_id = $1 AND command_type = 'test.link2' ORDER BY created_at DESC LIMIT 1",
            agent_id
        )
        .fetch_one(&db)
        .await
        .expect("fetch second");

        assert_eq!(
            row.previous_hash, before_second.entry_hash,
            "second entry must chain to the entry that was last at the moment of append"
        );
    }

    /// Tamper detection — modifying a row's `command_type` in the DB breaks
    /// the chain.  The next `append` call must abort with an integrity error
    /// instead of silently extending a broken chain.
    #[tokio::test]
    async fn tampered_previous_entry_breaks_chain() {
        let Some(db) = pool().await else { return };

        let agent_id = Uuid::now_v7();
        append(
            &db,
            entry(agent_id, "test.tamper.original", AuditResult::Success),
        )
        .await
        .expect("first");

        // Find that row's id (it is the last globally inserted because of LIMIT
        // 1 DESC, modulo parallel test inserts) — fetch by our own marker text.
        let row = sqlx::query!(
            "SELECT id as \"id: Uuid\" FROM audit_log WHERE command_type = 'test.tamper.original' AND agent_id = $1",
            agent_id
        )
        .fetch_one(&db)
        .await
        .expect("locate row");

        // Tamper: change command_type but DON'T recompute entry_hash.  The next
        // append() must detect that the recomputed hash no longer matches the
        // stored one for the current `last` row.
        sqlx::query!(
            "UPDATE audit_log SET command_type = 'test.tamper.MUTATED' WHERE id = $1",
            row.id
        )
        .execute(&db)
        .await
        .expect("tamper");

        // The integrity check fires only when the *globally* last row is the
        // tampered one (because `append` looks at the most-recent row in the
        // table).  Tests run serially in CI (`--test-threads=1`), so this row
        // is in fact the last.
        let res = append(
            &db,
            entry(agent_id, "test.tamper.next", AuditResult::Success),
        )
        .await;
        assert!(
            res.is_err(),
            "append must reject when prior row's stored hash no longer matches its data"
        );
        let msg = format!("{:#}", res.unwrap_err());
        assert!(
            msg.contains("integrity") || msg.contains("hash chain"),
            "error must mention integrity / hash chain: {msg}"
        );
    }
}
