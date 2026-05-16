use anyhow::{Context, Result};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::Utc;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub const MAX_TIMESTAMP_SKEW_SECS: i64 = 30;

/// Permission level required for a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    Read,
    Write,
    Destructive,
}

/// Signed command envelope sent from dashboard to agent.
#[derive(Debug, Deserialize, Serialize)]
pub struct SignedCommand {
    /// Base64url-encoded JSON payload bytes
    pub payload: String,
    /// Base64url-encoded Ed25519 signature over `payload` bytes
    pub signature: String,
}

/// Inner payload (before verification).
#[derive(Debug, Deserialize, Serialize)]
pub struct CommandPayload {
    pub nonce: String,
    pub timestamp: i64,
    pub agent_id: Uuid,
    pub user_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub permission: PermissionLevel,
    pub command: serde_json::Value,
}

/// Verified command — produced only after all checks pass.
pub struct VerifiedCommand {
    pub user_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub permission: PermissionLevel,
    pub command: serde_json::Value,
}

/// Full verification: signature → nonce dedup → timestamp freshness → agent_id match.
pub async fn verify_command(
    db: &PgPool,
    signed: &SignedCommand,
    verify_key_bytes: &[u8; 32],
    own_agent_id: Uuid,
) -> Result<VerifiedCommand> {
    // 1. Decode payload bytes + signature
    let payload_bytes =
        Base64UrlUnpadded::decode_vec(&signed.payload).context("payload: invalid base64url")?;
    let sig_bytes =
        Base64UrlUnpadded::decode_vec(&signed.signature).context("signature: invalid base64url")?;

    // 2. Verify Ed25519 signature (constant-time)
    let verifying_key =
        VerifyingKey::from_bytes(verify_key_bytes).context("invalid dashboard verify key")?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must be 64 bytes"))?;
    let sig = Signature::from_bytes(&sig_arr);
    use ed25519_dalek::Verifier;
    verifying_key
        .verify(&payload_bytes, &sig)
        .context("signature verification failed")?;

    // 3. Parse payload
    let payload: CommandPayload =
        serde_json::from_slice(&payload_bytes).context("invalid payload JSON")?;

    // 4. Check agent_id matches this agent
    if payload.agent_id != own_agent_id {
        anyhow::bail!("command not addressed to this agent");
    }

    // 5. Timestamp freshness (±30s)
    let now = Utc::now().timestamp();
    let skew = (now - payload.timestamp).abs();
    if skew > MAX_TIMESTAMP_SKEW_SECS {
        anyhow::bail!("timestamp too old or in future (skew={skew}s)");
    }

    // 6. Nonce dedup (replay protection)
    check_and_consume_nonce(db, &payload.nonce).await?;

    Ok(VerifiedCommand {
        user_id: payload.user_id,
        organization_id: payload.organization_id,
        permission: payload.permission,
        command: payload.command,
    })
}

/// Returns Ok(()) if nonce is fresh, inserts it. Returns Err if already seen.
async fn check_and_consume_nonce(db: &PgPool, nonce: &str) -> Result<()> {
    // Also purge expired nonces (>60s old) opportunistically
    sqlx::query!("DELETE FROM used_nonces WHERE created_at < NOW() - INTERVAL '60 seconds'")
        .execute(db)
        .await
        .context("purge expired nonces")?;

    let inserted = sqlx::query_scalar!(
        r#"
        INSERT INTO used_nonces (nonce) VALUES ($1)
        ON CONFLICT (nonce) DO NOTHING
        RETURNING nonce
        "#,
        nonce
    )
    .fetch_optional(db)
    .await
    .context("insert nonce")?;

    if inserted.is_none() {
        anyhow::bail!("nonce already used (replay attack)");
    }
    Ok(())
}

/// Verify internal bearer token (constant-time).
pub fn verify_bearer(provided: &str, expected: &str) -> bool {
    let a = provided.as_bytes();
    let b = expected.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}
