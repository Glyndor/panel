//! Internal PKI — dashboard CA issues Ed25519-signed certificates to agents.
//!
//! A "certificate" here is a JSON payload signed by the CA's Ed25519 key.
//! It is not X.509. Agents verify the CA signature to authenticate commands
//! from the dashboard even if the WireGuard PSK were compromised.

use anyhow::{Context, Result};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature, SigningKey, Signer, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

pub const CERT_VALIDITY_DAYS: i64 = 90;

/// Certificate payload — signed by the CA key.
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentCert {
    pub agent_id: Uuid,
    pub issued_at: i64,  // Unix timestamp
    pub expires_at: i64, // Unix timestamp
}

/// Serialized, signed certificate returned to agent at registration.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SignedCert {
    /// Base64url-encoded JSON payload
    pub payload: String,
    /// Base64url-encoded Ed25519 signature
    pub signature: String,
}

/// Issue a new certificate for an agent, signed by the CA private key.
pub fn issue_cert(ca_private_seed: &[u8; 32], agent_id: Uuid) -> Result<SignedCert> {
    let now = Utc::now();
    let cert = AgentCert {
        agent_id,
        issued_at: now.timestamp(),
        expires_at: (now + Duration::days(CERT_VALIDITY_DAYS)).timestamp(),
    };

    let payload_bytes = serde_json::to_vec(&cert).context("serialize cert payload")?;
    let payload = Base64UrlUnpadded::encode_string(&payload_bytes);

    let signing_key = SigningKey::from_bytes(ca_private_seed);
    let sig = signing_key.sign(&payload_bytes);
    let signature = Base64UrlUnpadded::encode_string(&sig.to_bytes());

    Ok(SignedCert { payload, signature })
}

/// Verify a certificate against the CA public key.
/// Returns the decoded payload if valid and not expired.
pub fn verify_cert(
    ca_public_bytes: &[u8; 32],
    cert: &SignedCert,
    expected_agent_id: Uuid,
) -> Result<AgentCert> {
    let payload_bytes = Base64UrlUnpadded::decode_vec(&cert.payload)
        .context("base64url decode payload")?;
    let sig_bytes = Base64UrlUnpadded::decode_vec(&cert.signature)
        .context("base64url decode signature")?;

    let verifying_key = VerifyingKey::from_bytes(ca_public_bytes)
        .context("parse CA public key")?;

    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must be 64 bytes"))?;
    let sig = Signature::from_bytes(&sig_arr);

    verifying_key
        .verify(&payload_bytes, &sig)
        .context("CA signature invalid")?;

    let payload: AgentCert =
        serde_json::from_slice(&payload_bytes).context("deserialize cert")?;

    if payload.agent_id != expected_agent_id {
        anyhow::bail!("cert agent_id mismatch");
    }

    let now = Utc::now().timestamp();
    if now > payload.expires_at {
        anyhow::bail!("cert expired");
    }

    Ok(payload)
}

/// Generate a new CA Ed25519 keypair (for startup when not configured).
pub fn gen_ca_keypair() -> (Zeroizing<[u8; 32]>, [u8; 32]) {
    let mut seed = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut seed);
    let signing_key = SigningKey::from_bytes(&seed);
    let pub_bytes = signing_key.verifying_key().to_bytes();
    (Zeroizing::new(seed), pub_bytes)
}
