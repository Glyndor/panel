use anyhow::{Context, Result};
use base64ct::{Base64, Encoding};
use zeroize::Zeroizing;

pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub internal_token: Zeroizing<String>,
    pub kek: Zeroizing<[u8; 32]>,
    pub pepper: Zeroizing<String>,
    /// Ed25519 seed (32 bytes) — private signing key
    pub jwt_sign_private_seed: Zeroizing<[u8; 32]>,
    /// Ed25519 public key (32 bytes)
    pub jwt_sign_public_bytes: [u8; 32],
    /// X25519 private key (32 bytes)
    pub jwt_enc_private_bytes: Zeroizing<[u8; 32]>,
    /// X25519 public key (32 bytes)
    pub jwt_enc_public_bytes: [u8; 32],
}

impl Config {
    pub fn load() -> Result<Self> {
        let internal_token = load_secret("INTERNAL_API_TOKEN", "INTERNAL_API_TOKEN_FILE")?;
        let kek = load_key32("KEK", "KEK_FILE")?;
        let pepper = load_secret("PEPPER", "PEPPER_FILE")?;
        let (jwt_sign_private_seed, jwt_sign_public_bytes) = load_or_gen_ed25519()?;
        let (jwt_enc_private_bytes, jwt_enc_public_bytes) = load_or_gen_x25519()?;

        Ok(Config {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL required")?,
            redis_url: std::env::var("REDIS_URL").context("REDIS_URL required")?,
            internal_token,
            kek,
            pepper,
            jwt_sign_private_seed,
            jwt_sign_public_bytes,
            jwt_enc_private_bytes,
            jwt_enc_public_bytes,
        })
    }
}

fn load_secret(env: &str, file_env: &str) -> Result<Zeroizing<String>> {
    if let Ok(path) = std::env::var(file_env) {
        let val = std::fs::read_to_string(&path)
            .with_context(|| format!("read {file_env}={path}"))?;
        return Ok(Zeroizing::new(val.trim().to_string()));
    }
    let val = std::env::var(env).with_context(|| format!("{env} or {file_env} required"))?;
    Ok(Zeroizing::new(val))
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
