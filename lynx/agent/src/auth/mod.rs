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
    // Purge nonces older than 5 minutes. Per spec: timestamp window is 30s, but nonces
    // are retained for 5 minutes to account for clock skew before the 30s window kicks in.
    sqlx::query!("DELETE FROM used_nonces WHERE created_at < NOW() - INTERVAL '5 minutes'")
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- verify_bearer ---

    #[test]
    fn bearer_correct_token_accepted() {
        assert!(verify_bearer("secret-token-123", "secret-token-123"));
    }

    #[test]
    fn bearer_wrong_token_rejected() {
        assert!(!verify_bearer("wrong-token", "secret-token-123"));
    }

    #[test]
    fn bearer_different_length_rejected() {
        // Different length must fail without comparing bytes (length side-channel).
        assert!(!verify_bearer("short", "secret-token-123"));
    }

    #[test]
    fn bearer_empty_strings_match() {
        assert!(verify_bearer("", ""));
    }

    #[test]
    fn bearer_one_char_off_rejected() {
        assert!(!verify_bearer("secret-token-124", "secret-token-123"));
    }

    // --- PermissionLevel ordering ---

    #[test]
    fn permission_read_less_than_write() {
        assert!(PermissionLevel::Read < PermissionLevel::Write);
    }

    #[test]
    fn permission_write_less_than_destructive() {
        assert!(PermissionLevel::Write < PermissionLevel::Destructive);
    }

    #[test]
    fn permission_read_less_than_destructive() {
        assert!(PermissionLevel::Read < PermissionLevel::Destructive);
    }

    #[test]
    fn permission_equal_levels() {
        assert!(PermissionLevel::Write == PermissionLevel::Write);
    }

    // --- Timestamp skew ---

    #[test]
    fn timestamp_within_window_passes() {
        let now = chrono::Utc::now().timestamp();
        let skew = (now - (now - 10)).abs(); // 10s ago — well within 30s
        assert!(skew <= MAX_TIMESTAMP_SKEW_SECS);
    }

    #[test]
    fn timestamp_outside_window_fails() {
        let now = chrono::Utc::now().timestamp();
        let old = now - 60; // 60s ago — outside 30s window
        let skew = (now - old).abs();
        assert!(skew > MAX_TIMESTAMP_SKEW_SECS);
    }

    #[test]
    fn timestamp_future_outside_window_fails() {
        let now = chrono::Utc::now().timestamp();
        let future = now + 60; // 60s in the future
        let skew = (now - future).abs();
        assert!(skew > MAX_TIMESTAMP_SKEW_SECS);
    }

    // --- Crypto round-trip: sign then verify signature ---

    #[test]
    fn signed_command_signature_verifies() {
        use base64ct::{Base64UrlUnpadded, Encoding};
        use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};

        let seed = [0x42u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key: VerifyingKey = signing_key.verifying_key();

        let payload_bytes = br#"{"agent_id":"test","nonce":"abc","timestamp":1}"#;
        let payload_b64 = Base64UrlUnpadded::encode_string(payload_bytes);

        let sig = signing_key.sign(payload_bytes);
        let sig_b64 = Base64UrlUnpadded::encode_string(&sig.to_bytes());

        // Decode and verify just like verify_command does
        let decoded_payload = Base64UrlUnpadded::decode_vec(&payload_b64).unwrap();
        let decoded_sig_bytes = Base64UrlUnpadded::decode_vec(&sig_b64).unwrap();
        let sig_arr: [u8; 64] = decoded_sig_bytes.try_into().unwrap();
        let sig2 = ed25519_dalek::Signature::from_bytes(&sig_arr);

        assert!(verifying_key.verify(&decoded_payload, &sig2).is_ok());
    }

    #[test]
    fn tampered_payload_fails_verification() {
        use base64ct::{Base64UrlUnpadded, Encoding};
        use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};

        let seed = [0x42u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key: VerifyingKey = signing_key.verifying_key();

        let payload_bytes = br#"{"agent_id":"test","nonce":"abc","timestamp":1}"#;
        let sig = signing_key.sign(payload_bytes);
        let sig_b64 = Base64UrlUnpadded::encode_string(&sig.to_bytes());

        // Tamper the payload
        let tampered = br#"{"agent_id":"evil","nonce":"abc","timestamp":1}"#;
        let tampered_b64 = Base64UrlUnpadded::encode_string(tampered);

        let decoded_payload = Base64UrlUnpadded::decode_vec(&tampered_b64).unwrap();
        let decoded_sig_bytes = Base64UrlUnpadded::decode_vec(&sig_b64).unwrap();
        let sig_arr: [u8; 64] = decoded_sig_bytes.try_into().unwrap();
        let sig2 = ed25519_dalek::Signature::from_bytes(&sig_arr);

        assert!(verifying_key.verify(&decoded_payload, &sig2).is_err());
    }
}
