use anyhow::{Context, Result};
use base64ct::{Base64, Encoding};
use zeroize::Zeroizing;

pub struct Config {
    pub database_url: String,
    pub agent_id: uuid::Uuid,
    /// Ed25519 public key bytes (32) — dashboard's signing key, used to verify commands
    pub dashboard_verify_key: [u8; 32],
    /// Bearer token for dashboard→agent API calls (internal, WireGuard-only)
    pub internal_token: Zeroizing<String>,
    pub listen_addr: String,
    /// Dashboard API base URL via WireGuard (e.g. http://10.100.0.1:8080). Optional.
    pub dashboard_url: Option<String>,
    /// Sync token for agent→dashboard audit log sync. Optional — sync disabled if absent.
    pub sync_token: Option<Zeroizing<String>>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let database_url =
            std::env::var("DATABASE_URL").context("DATABASE_URL required")?;

        let agent_id_str =
            std::env::var("AGENT_ID").context("AGENT_ID required")?;
        let agent_id =
            uuid::Uuid::parse_str(&agent_id_str).context("AGENT_ID must be UUID v7")?;

        let dashboard_verify_key = load_key32_or_dev("DASHBOARD_VERIFY_KEY")?;
        let internal_token = load_secret("INTERNAL_TOKEN")?;
        let listen_addr =
            std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:9090".to_string());
        let dashboard_url = std::env::var("DASHBOARD_URL").ok();
        let sync_token = load_secret_opt("SYNC_TOKEN");

        Ok(Config {
            database_url,
            agent_id,
            dashboard_verify_key,
            internal_token,
            listen_addr,
            dashboard_url,
            sync_token,
        })
    }
}

fn load_secret(env: &str) -> Result<Zeroizing<String>> {
    let file_env = format!("{env}_FILE");
    if let Ok(path) = std::env::var(&file_env) {
        let val = std::fs::read_to_string(&path)
            .with_context(|| format!("read {file_env}={path}"))?;
        return Ok(Zeroizing::new(val.trim().to_string()));
    }
    let val = std::env::var(env).with_context(|| format!("{env} required"))?;
    Ok(Zeroizing::new(val))
}

fn load_secret_opt(env: &str) -> Option<Zeroizing<String>> {
    let file_env = format!("{env}_FILE");
    if let Ok(path) = std::env::var(&file_env) {
        if let Ok(val) = std::fs::read_to_string(&path) {
            return Some(Zeroizing::new(val.trim().to_string()));
        }
    }
    std::env::var(env).ok().map(Zeroizing::new)
}

fn load_key32(env: &str) -> Result<[u8; 32]> {
    let raw = load_secret(env)?;
    let bytes = Base64::decode_vec(raw.trim()).with_context(|| format!("{env}: not base64"))?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("{env} must be exactly 32 bytes"))
}

fn load_key32_or_dev(env: &str) -> Result<[u8; 32]> {
    if std::env::var(env).is_err() && std::env::var(format!("{env}_FILE")).is_err() {
        tracing::warn!("{env} not configured — using ephemeral dev key (INSECURE)");
        let mut key = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut key);
        return Ok(key);
    }
    load_key32(env)
}
