//! Agent certificate verification.
//!
//! The dashboard CA issues an Ed25519-signed certificate at agent registration.
//! Agents store it and can verify it to confirm commands come from a trusted dashboard.

use anyhow::{Context, Result};
use base64ct::{Base64UrlUnpadded, Encoding};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignedCert {
    pub payload: String,
    pub signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentCert {
    pub agent_id: Uuid,
    pub issued_at: i64,
    pub expires_at: i64,
}

/// Load CA public key from env (CA_PUBLIC_KEY or CA_PUBLIC_KEY_FILE).
/// Returns None if not configured (cert verification disabled in dev mode).
pub fn load_ca_public_key() -> Option<[u8; 32]> {
    let raw = std::env::var("CA_PUBLIC_KEY_FILE")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .or_else(|| std::env::var("CA_PUBLIC_KEY").ok())?;

    let bytes = Base64UrlUnpadded::decode_vec(raw.trim()).ok()?;
    bytes.try_into().ok()
}

/// Verify a cert from the dashboard. Returns Ok if valid and not expired.
pub fn verify(cert: &SignedCert, ca_public: &[u8; 32], expected_agent_id: Uuid) -> Result<()> {
    let payload_bytes =
        Base64UrlUnpadded::decode_vec(&cert.payload).context("base64url decode payload")?;
    let sig_bytes =
        Base64UrlUnpadded::decode_vec(&cert.signature).context("base64url decode signature")?;

    let verifying_key = VerifyingKey::from_bytes(ca_public).context("parse CA public key")?;

    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must be 64 bytes"))?;
    let sig = Signature::from_bytes(&sig_arr);

    verifying_key
        .verify(&payload_bytes, &sig)
        .context("CA signature invalid")?;

    let payload: AgentCert = serde_json::from_slice(&payload_bytes).context("deserialize cert")?;

    if payload.agent_id != expected_agent_id {
        anyhow::bail!("cert agent_id mismatch");
    }

    let now = chrono::Utc::now().timestamp();
    if now > payload.expires_at {
        anyhow::bail!("cert expired");
    }

    Ok(())
}
