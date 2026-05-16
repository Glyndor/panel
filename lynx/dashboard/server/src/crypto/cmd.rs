use crate::config::Config;
use anyhow::Result;
use base64ct::{Base64Url, Encoding};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use uuid::Uuid;

/// Signed command payload sent to the agent's /cmd endpoint.
#[derive(serde::Serialize)]
pub struct SignedCommand {
    /// base64url(JSON payload)
    pub payload: String,
    /// base64url(Ed25519 signature over payload bytes)
    pub signature: String,
}

pub fn sign_command(
    config: &Config,
    agent_id: Uuid,
    user_id: Uuid,
    permission: &str,
    command: &Value,
) -> Result<SignedCommand> {
    let nonce = Uuid::now_v7().to_string();
    let timestamp = chrono::Utc::now().timestamp();

    let payload_json = json!({
        "agent_id": agent_id,
        "user_id": user_id,
        "permission": permission,
        "nonce": nonce,
        "timestamp": timestamp,
        "command": command,
    });

    let payload_bytes = serde_json::to_vec(&payload_json)?;
    let payload_b64 = Base64Url::encode_string(&payload_bytes);

    let signing_key = SigningKey::from_bytes(&*config.jwt_sign_private_seed);
    let signature = signing_key.sign(&payload_bytes);
    let sig_b64 = Base64Url::encode_string(&signature.to_bytes());

    Ok(SignedCommand {
        payload: payload_b64,
        signature: sig_b64,
    })
}
