use anyhow::{Context, Result};
use base64ct::{Base64, Encoding};
use zeroize::Zeroizing;

use crate::crypto::pki;

pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub internal_token: Zeroizing<String>,
    pub kek: Zeroizing<[u8; 32]>,
    pub pepper: Zeroizing<String>,
    /// One-time bootstrap token for creating the first admin. None after bootstrap completes.
    pub setup_token: Option<Zeroizing<String>>,
    /// Ed25519 seed (32 bytes) — private signing key
    pub jwt_sign_private_seed: Zeroizing<[u8; 32]>,
    /// Ed25519 public key (32 bytes)
    pub jwt_sign_public_bytes: [u8; 32],
    /// X25519 private key (32 bytes)
    pub jwt_enc_private_bytes: Zeroizing<[u8; 32]>,
    /// X25519 public key (32 bytes)
    pub jwt_enc_public_bytes: [u8; 32],
    /// CA Ed25519 seed (32 bytes) — signs agent JSON certificates
    pub ca_private_seed: Zeroizing<[u8; 32]>,
    /// CA Ed25519 public key (32 bytes) — distributed to agents for JSON cert verification
    pub ca_public_bytes: [u8; 32],
    /// X.509 CA certificate DER — stable trust anchor distributed to agents for mTLS
    pub x509_ca_cert_der: Vec<u8>,
    /// X.509 CA private key DER (PKCS#8) — used to sign agent/client X.509 certs
    pub x509_ca_key_der: Zeroizing<Vec<u8>>,
    /// X.509 dashboard client certificate DER — presented to agents during mTLS handshake
    pub x509_client_cert_der: Vec<u8>,
    /// X.509 dashboard client private key DER (PKCS#8)
    pub x509_client_key_der: Zeroizing<Vec<u8>>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let internal_token = load_secret("INTERNAL_API_TOKEN", "INTERNAL_API_TOKEN_FILE")?;
        let kek = load_key32("KEK", "KEK_FILE")?;
        let pepper = load_secret("PEPPER", "PEPPER_FILE")?;
        let setup_token = load_secret_opt("SETUP_TOKEN", "SETUP_TOKEN_FILE");
        let (jwt_sign_private_seed, jwt_sign_public_bytes) = load_or_gen_ed25519()?;
        let (jwt_enc_private_bytes, jwt_enc_public_bytes) = load_or_gen_x25519()?;
        let (ca_private_seed, ca_public_bytes) = load_or_gen_ca_ed25519()?;
        let (x509_ca_cert_der, x509_ca_key_der) = load_or_gen_x509_ca()?;
        let (x509_client_cert_der, x509_client_key_der) =
            pki::issue_x509_dashboard_client_cert(&x509_ca_cert_der, &x509_ca_key_der)
                .context("issue dashboard client cert")?;

        let database_url = load_secret("DATABASE_URL", "DATABASE_URL_FILE")
            .map(|s| s.as_str().to_owned())
            .context("DATABASE_URL or DATABASE_URL_FILE required")?;
        let redis_url = load_secret("REDIS_URL", "REDIS_URL_FILE")
            .map(|s| s.as_str().to_owned())
            .context("REDIS_URL or REDIS_URL_FILE required")?;

        Ok(Config {
            database_url,
            redis_url,
            internal_token,
            kek,
            pepper,
            setup_token,
            jwt_sign_private_seed,
            jwt_sign_public_bytes,
            jwt_enc_private_bytes,
            jwt_enc_public_bytes,
            ca_private_seed,
            ca_public_bytes,
            x509_ca_cert_der,
            x509_ca_key_der,
            x509_client_cert_der,
            x509_client_key_der,
        })
    }
}

fn load_secret(env: &str, file_env: &str) -> Result<Zeroizing<String>> {
    if let Ok(path) = std::env::var(file_env) {
        let val =
            std::fs::read_to_string(&path).with_context(|| format!("read {file_env}={path}"))?;
        return Ok(Zeroizing::new(val.trim().to_string()));
    }
    let val = std::env::var(env).with_context(|| format!("{env} or {file_env} required"))?;
    Ok(Zeroizing::new(val))
}

fn load_secret_opt(env: &str, file_env: &str) -> Option<Zeroizing<String>> {
    if let Ok(path) = std::env::var(file_env) {
        if let Ok(val) = std::fs::read_to_string(&path) {
            return Some(Zeroizing::new(val.trim().to_string()));
        }
    }
    std::env::var(env).ok().map(|v| Zeroizing::new(v))
}

fn load_key32(env: &str, file_env: &str) -> Result<Zeroizing<[u8; 32]>> {
    let raw = load_secret(env, file_env)?;
    let bytes = Base64::decode_vec(raw.trim()).context("key must be base64-encoded 32 bytes")?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("key must be exactly 32 bytes"))?;
    Ok(Zeroizing::new(arr))
}

fn load_or_gen_ed25519() -> Result<(Zeroizing<[u8; 32]>, [u8; 32])> {
    if let (Ok(p), Ok(q)) = (
        load_secret("JWT_SIGN_PRIVATE_KEY", "JWT_SIGN_PRIVATE_KEY_FILE"),
        load_secret("JWT_SIGN_PUBLIC_KEY", "JWT_SIGN_PUBLIC_KEY_FILE"),
    ) {
        let seed: [u8; 32] = Base64::decode_vec(p.trim())
            .context("JWT_SIGN_PRIVATE_KEY base64")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("JWT_SIGN_PRIVATE_KEY must be 32 bytes"))?;
        let pub_bytes: [u8; 32] = Base64::decode_vec(q.trim())
            .context("JWT_SIGN_PUBLIC_KEY base64")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("JWT_SIGN_PUBLIC_KEY must be 32 bytes"))?;
        return Ok((Zeroizing::new(seed), pub_bytes));
    }

    tracing::warn!("JWT signing keys not configured — using ephemeral keys (dev only)");

    let mut seed = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let pub_bytes = signing_key.verifying_key().to_bytes();

    Ok((Zeroizing::new(seed), pub_bytes))
}

fn load_or_gen_x25519() -> Result<(Zeroizing<[u8; 32]>, [u8; 32])> {
    if let (Ok(p), Ok(q)) = (
        load_secret("JWT_ENC_PRIVATE_KEY", "JWT_ENC_PRIVATE_KEY_FILE"),
        load_secret("JWT_ENC_PUBLIC_KEY", "JWT_ENC_PUBLIC_KEY_FILE"),
    ) {
        let priv_bytes: [u8; 32] = Base64::decode_vec(p.trim())
            .context("JWT_ENC_PRIVATE_KEY base64")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("JWT_ENC_PRIVATE_KEY must be 32 bytes"))?;
        let pub_bytes: [u8; 32] = Base64::decode_vec(q.trim())
            .context("JWT_ENC_PUBLIC_KEY base64")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("JWT_ENC_PUBLIC_KEY must be 32 bytes"))?;
        return Ok((Zeroizing::new(priv_bytes), pub_bytes));
    }

    tracing::warn!("JWT encryption keys not configured — using ephemeral keys (dev only)");

    let mut priv_bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut priv_bytes);
    let secret = x25519_dalek::StaticSecret::from(priv_bytes);
    let public = x25519_dalek::PublicKey::from(&secret);

    Ok((Zeroizing::new(priv_bytes), public.to_bytes()))
}

fn load_or_gen_x509_ca() -> Result<(Vec<u8>, Zeroizing<Vec<u8>>)> {
    let cert_raw = load_secret_opt("X509_CA_CERT", "X509_CA_CERT_FILE");
    let key_raw = load_secret_opt("X509_CA_KEY", "X509_CA_KEY_FILE");

    if let (Some(cert_b64), Some(key_b64)) = (cert_raw, key_raw) {
        let cert_der = Base64::decode_vec(cert_b64.trim())
            .context("X509_CA_CERT base64 decode")?;
        let key_der = Base64::decode_vec(key_b64.trim())
            .context("X509_CA_KEY base64 decode")?;
        return Ok((cert_der, Zeroizing::new(key_der)));
    }

    tracing::warn!("X509_CA_CERT/KEY not configured — generating ephemeral X.509 CA (dev only; agents will reject certs after restart)");
    pki::generate_x509_ca().context("generate ephemeral X.509 CA")
}

fn load_or_gen_ca_ed25519() -> Result<(Zeroizing<[u8; 32]>, [u8; 32])> {
    if let (Ok(p), Ok(q)) = (
        load_secret("CA_PRIVATE_KEY", "CA_PRIVATE_KEY_FILE"),
        load_secret("CA_PUBLIC_KEY", "CA_PUBLIC_KEY_FILE"),
    ) {
        let seed: [u8; 32] = Base64::decode_vec(p.trim())
            .context("CA_PRIVATE_KEY base64")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("CA_PRIVATE_KEY must be 32 bytes"))?;
        let pub_bytes: [u8; 32] = Base64::decode_vec(q.trim())
            .context("CA_PUBLIC_KEY base64")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("CA_PUBLIC_KEY must be 32 bytes"))?;
        return Ok((Zeroizing::new(seed), pub_bytes));
    }

    tracing::warn!("CA keypair not configured — using ephemeral CA (dev only, agents will reject certs on restart)");

    let mut seed = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut seed);
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let pub_bytes = signing_key.verifying_key().to_bytes();
    Ok((Zeroizing::new(seed), pub_bytes))
}
