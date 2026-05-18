use anyhow::{Context, Result};
use base64ct::{Base64UrlUnpadded, Encoding};
use josekit::{
    jwe::{self, JweHeader, ECDH_ES_A256KW},
    jwk::Jwk,
    jws::{self, EdDSA, JwsHeader},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use zeroize::ZeroizeOnDrop;

pub struct AccessClaims {
    pub sub: Uuid,
    pub jti: Uuid,
    pub session_id: Uuid,
    pub ip_hash: String,
    pub ua_hash: String,
}

#[derive(ZeroizeOnDrop)]
pub struct JwtKeys {
    /// Ed25519 raw seed (32 bytes) — private signing key
    pub sign_private_seed: [u8; 32],
    /// Ed25519 raw public key (32 bytes)
    #[zeroize(skip)]
    pub sign_public_bytes: [u8; 32],
    /// X25519 raw private key (32 bytes)
    pub enc_private_bytes: [u8; 32],
    /// X25519 raw public key (32 bytes)
    #[zeroize(skip)]
    pub enc_public_bytes: [u8; 32],
}

pub fn issue_access_token(
    keys: &JwtKeys,
    user_id: Uuid,
    jti: Uuid,
    session_id: Uuid,
    ip_hash: &str,
    ua_hash: &str,
) -> Result<String> {
    let now = unix_now();
    let exp = now + 900;

    let payload = serde_json::json!({
        "iss": "lynx-dashboard",
        "sub": user_id.to_string(),
        "aud": "lynx-dashboard",
        "exp": exp,
        "nbf": now,
        "iat": now,
        "jti": jti.to_string(),
        "session_id": session_id.to_string(),
        "ip_hash": ip_hash,
        "ua_hash": ua_hash,
    });
    let payload_bytes = serde_json::to_vec(&payload).context("serialize JWT payload")?;

    // Sign (JWS — EdDSA/Ed25519)
    let mut jws_header = JwsHeader::new();
    jws_header.set_token_type("JWT");
    let signer_jwk = ed25519_private_jwk(&keys.sign_private_seed, &keys.sign_public_bytes)?;
    let signer = EdDSA
        .signer_from_jwk(&signer_jwk)
        .context("create Ed25519 signer")?;
    let inner_jws =
        jws::serialize_compact(&payload_bytes, &jws_header, &signer).context("JWS sign")?;

    // Encrypt (JWE — ECDH-ES+A256KW / X25519 / A256GCM)
    let mut jwe_header = JweHeader::new();
    jwe_header.set_content_encryption("A256GCM");
    jwe_header.set_content_type("JWT");
    let public_jwk = x25519_public_jwk(&keys.enc_public_bytes)?;
    let encrypter = ECDH_ES_A256KW
        .encrypter_from_jwk(&public_jwk)
        .context("create X25519 encrypter")?;
    let outer_jwe = jwe::serialize_compact(inner_jws.as_bytes(), &jwe_header, &encrypter)
        .context("JWE encrypt")?;

    Ok(outer_jwe)
}

pub fn verify_access_token(keys: &JwtKeys, token: &str) -> Result<AccessClaims> {
    // Decrypt (JWE)
    let private_jwk = x25519_private_jwk(&keys.enc_private_bytes, &keys.enc_public_bytes)?;
    let decrypter = ECDH_ES_A256KW
        .decrypter_from_jwk(&private_jwk)
        .context("create X25519 decrypter")?;
    let (inner_bytes, _) = jwe::deserialize_compact(token, &decrypter).context("JWE decrypt")?;

    // Verify (JWS)
    let verifier_jwk = ed25519_public_jwk(&keys.sign_public_bytes)?;
    let verifier = EdDSA
        .verifier_from_jwk(&verifier_jwk)
        .context("create Ed25519 verifier")?;
    let (payload_bytes, _) =
        jws::deserialize_compact(&inner_bytes, &verifier).context("JWS verify")?;

    let claims: serde_json::Value =
        serde_json::from_slice(&payload_bytes).context("parse JWT claims")?;

    validate_claims(&claims)?;

    let sub = parse_uuid(&claims, "sub")?;
    let jti = parse_uuid(&claims, "jti")?;
    let session_id = parse_uuid(&claims, "session_id")?;
    let ip_hash = parse_str(&claims, "ip_hash")?.to_string();
    let ua_hash = parse_str(&claims, "ua_hash")?.to_string();

    Ok(AccessClaims {
        sub,
        jti,
        session_id,
        ip_hash,
        ua_hash,
    })
}

fn validate_claims(c: &serde_json::Value) -> Result<()> {
    if c["iss"].as_str() != Some("lynx-dashboard") {
        anyhow::bail!("invalid issuer");
    }
    if c["aud"].as_str() != Some("lynx-dashboard") {
        anyhow::bail!("invalid audience");
    }
    let now = unix_now();
    let exp = c["exp"].as_u64().context("missing exp")?;
    if exp <= now {
        anyhow::bail!("token expired");
    }
    if let Some(nbf) = c["nbf"].as_u64() {
        if nbf > now {
            anyhow::bail!("token not yet valid");
        }
    }
    Ok(())
}

fn parse_uuid(c: &serde_json::Value, key: &str) -> Result<Uuid> {
    let s = c[key]
        .as_str()
        .with_context(|| format!("missing claim: {key}"))?;
    Uuid::parse_str(s).with_context(|| format!("{key} not a UUID"))
}

fn parse_str<'a>(c: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    c[key]
        .as_str()
        .with_context(|| format!("missing claim: {key}"))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn ed25519_private_jwk(seed: &[u8; 32], pub_bytes: &[u8; 32]) -> Result<Jwk> {
    serde_json::from_value(serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": Base64UrlUnpadded::encode_string(pub_bytes),
        "d": Base64UrlUnpadded::encode_string(seed),
    }))
    .context("build Ed25519 private JWK")
}

fn ed25519_public_jwk(pub_bytes: &[u8; 32]) -> Result<Jwk> {
    serde_json::from_value(serde_json::json!({
        "kty": "OKP",
        "crv": "Ed25519",
        "x": Base64UrlUnpadded::encode_string(pub_bytes),
    }))
    .context("build Ed25519 public JWK")
}

fn x25519_private_jwk(priv_bytes: &[u8; 32], pub_bytes: &[u8; 32]) -> Result<Jwk> {
    serde_json::from_value(serde_json::json!({
        "kty": "OKP",
        "crv": "X25519",
        "x": Base64UrlUnpadded::encode_string(pub_bytes),
        "d": Base64UrlUnpadded::encode_string(priv_bytes),
    }))
    .context("build X25519 private JWK")
}

fn x25519_public_jwk(pub_bytes: &[u8; 32]) -> Result<Jwk> {
    serde_json::from_value(serde_json::json!({
        "kty": "OKP",
        "crv": "X25519",
        "x": Base64UrlUnpadded::encode_string(pub_bytes),
    }))
    .context("build X25519 public JWK")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_keys() -> JwtKeys {
        use ed25519_dalek::SigningKey;
        use x25519_dalek::{PublicKey, StaticSecret};

        let sign_seed: [u8; 32] = [0x42u8; 32];
        let signing = SigningKey::from_bytes(&sign_seed);
        let sign_pub: [u8; 32] = signing.verifying_key().to_bytes();

        let enc_priv: [u8; 32] = [0x77u8; 32];
        let enc_pub: [u8; 32] = *PublicKey::from(&StaticSecret::from(enc_priv)).as_bytes();

        JwtKeys {
            sign_private_seed: sign_seed,
            sign_public_bytes: sign_pub,
            enc_private_bytes: enc_priv,
            enc_public_bytes: enc_pub,
        }
    }

    fn dummy_uuid() -> Uuid {
        Uuid::now_v7()
    }

    #[test]
    fn roundtrip_valid_token() {
        let keys = test_keys();
        let user_id = dummy_uuid();
        let jti = dummy_uuid();
        let session_id = dummy_uuid();

        let token =
            issue_access_token(&keys, user_id, jti, session_id, "ip_hash_val", "ua_hash_val")
                .expect("issue token");

        let claims = verify_access_token(&keys, &token).expect("verify token");
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.jti, jti);
        assert_eq!(claims.session_id, session_id);
        assert_eq!(claims.ip_hash, "ip_hash_val");
        assert_eq!(claims.ua_hash, "ua_hash_val");
    }

    #[test]
    fn wrong_sign_key_rejected() {
        let keys = test_keys();
        let token = issue_access_token(
            &keys,
            dummy_uuid(),
            dummy_uuid(),
            dummy_uuid(),
            "ip",
            "ua",
        )
        .expect("issue");

        let mut bad_keys = test_keys();
        bad_keys.sign_public_bytes = [0u8; 32]; // invalid pub key → verify fails
        assert!(verify_access_token(&bad_keys, &token).is_err());
    }

    #[test]
    fn wrong_enc_key_rejected() {
        let keys = test_keys();
        let token = issue_access_token(
            &keys,
            dummy_uuid(),
            dummy_uuid(),
            dummy_uuid(),
            "ip",
            "ua",
        )
        .expect("issue");

        let mut bad_keys = test_keys();
        bad_keys.enc_private_bytes = [0u8; 32]; // wrong private key → decrypt fails
        let enc_pub: [u8; 32] = {
            use x25519_dalek::{PublicKey, StaticSecret};
            let secret = StaticSecret::from([0u8; 32]);
            *PublicKey::from(&secret).as_bytes()
        };
        bad_keys.enc_public_bytes = enc_pub;
        assert!(verify_access_token(&bad_keys, &token).is_err());
    }

    #[test]
    fn tampered_token_rejected() {
        let keys = test_keys();
        let token = issue_access_token(
            &keys,
            dummy_uuid(),
            dummy_uuid(),
            dummy_uuid(),
            "ip",
            "ua",
        )
        .expect("issue");

        // Corrupt a character near the middle of the compact JWE string
        let mut bytes = token.into_bytes();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0x01;
        let corrupted = String::from_utf8_lossy(&bytes).into_owned();
        assert!(verify_access_token(&keys, &corrupted).is_err());
    }
}
