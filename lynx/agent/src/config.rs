use anyhow::{Context, Result};
use base64ct::{Base64, Encoding};
use zeroize::Zeroizing;

pub struct Config {
    pub database_url: String,
    pub agent_id: uuid::Uuid,
    pub version: String,
    /// Ed25519 public key bytes (32) — dashboard's signing key, used to verify commands
    pub dashboard_verify_key: [u8; 32],
    /// Bearer token for dashboard→agent API calls (internal, WireGuard-only)
    pub internal_token: Zeroizing<String>,
    pub listen_addr: String,
    /// Dashboard API base URL via WireGuard (e.g. http://10.100.0.1:8080). Optional.
    pub dashboard_url: Option<String>,
    /// Sync token for agent→dashboard audit log sync. Optional — sync disabled if absent.
    pub sync_token: Option<Zeroizing<String>>,
    /// X.509 TLS server certificate DER — for mTLS listener. None = plain HTTP.
    pub tls_cert_der: Option<Vec<u8>>,
    /// X.509 TLS server private key DER (PKCS#8).
    pub tls_key_der: Option<Zeroizing<Vec<u8>>>,
    /// X.509 CA certificate DER — used to verify dashboard client certs.
    pub tls_ca_cert_der: Option<Vec<u8>>,
    /// Dashboard panel port to open in nftables (Some(19443) on dashboard VPS, None on remote agents).
    pub dashboard_port: Option<u16>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let database_url = load_secret("DATABASE_URL")
            .map(|s| s.as_str().to_owned())
            .context("DATABASE_URL or DATABASE_URL_FILE required")?;

        let agent_id_str = std::env::var("AGENT_ID").context("AGENT_ID required")?;
        let agent_id = uuid::Uuid::parse_str(&agent_id_str).context("AGENT_ID must be UUID v7")?;

        // The dashboard signing public key (Ed25519) is mandatory: every command
        // from the dashboard (heartbeat ACK, container ops, nftables push,
        // update.self, ...) is verified against it.  A missing or wrong key
        // makes the agent reject every command and lock down within 5 minutes
        // — fail fast at startup instead of silently entering lockdown later.
        let dashboard_verify_key = load_key32("DASHBOARD_VERIFY_KEY")
            .context("DASHBOARD_VERIFY_KEY (or DASHBOARD_VERIFY_KEY_FILE) is required — supply the dashboard's Ed25519 signing pubkey from setup-dashboard.sh output")?;
        let internal_token = load_secret("INTERNAL_TOKEN")?;
        let listen_addr =
            std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:9090".to_string());
        let dashboard_url = std::env::var("DASHBOARD_URL").ok();
        let sync_token = load_secret_opt("SYNC_TOKEN");
        let version = std::env::var("AGENT_VERSION")
            .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());

        let tls_cert_der = load_der_file_opt("TLS_CERT_DER_FILE");
        let tls_key_der = load_der_file_zeroize_opt("TLS_KEY_DER_FILE");
        let tls_ca_cert_der = load_der_file_opt("TLS_CA_CERT_DER_FILE");

        let dashboard_port = std::env::var("DASHBOARD_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok());

        Ok(Config {
            database_url,
            agent_id,
            dashboard_verify_key,
            internal_token,
            listen_addr,
            dashboard_url,
            sync_token,
            version,
            tls_cert_der,
            tls_key_der,
            tls_ca_cert_der,
            dashboard_port,
        })
    }
}

fn load_secret(env: &str) -> Result<Zeroizing<String>> {
    let file_env = format!("{env}_FILE");
    if let Ok(path) = std::env::var(&file_env) {
        let val =
            std::fs::read_to_string(&path).with_context(|| format!("read {file_env}={path}"))?;
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

fn load_der_file_opt(env: &str) -> Option<Vec<u8>> {
    let path = std::env::var(env).ok()?;
    std::fs::read(&path).ok()
}

fn load_der_file_zeroize_opt(env: &str) -> Option<Zeroizing<Vec<u8>>> {
    load_der_file_opt(env).map(Zeroizing::new)
}
