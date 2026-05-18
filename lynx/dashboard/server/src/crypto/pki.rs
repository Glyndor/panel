//! Internal PKI — dashboard CA issues certificates to agents.
//!
//! Two certificate systems in parallel:
//! - JSON-signed certs (Ed25519): lightweight, used to verify dashboard command authority
//! - X.509 certs (rcgen/Ed25519): used for mTLS between dashboard and agent HTTP endpoints

use anyhow::{Context, Result};
use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    SanType, PKCS_ED25519,
};
use rustls::pki_types::CertificateDer as RustlsCertDer;
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
    let payload_bytes =
        Base64UrlUnpadded::decode_vec(&cert.payload).context("base64url decode payload")?;
    let sig_bytes =
        Base64UrlUnpadded::decode_vec(&cert.signature).context("base64url decode signature")?;

    let verifying_key = VerifyingKey::from_bytes(ca_public_bytes).context("parse CA public key")?;

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

    let now = Utc::now().timestamp();
    if now > payload.expires_at {
        anyhow::bail!("cert expired");
    }

    Ok(payload)
}

/// Generate a new CA Ed25519 keypair (for startup when not configured).
pub fn gen_ca_keypair() -> (Zeroizing<[u8; 32]>, [u8; 32]) {
    let mut seed = [0u8; 32];
    {
        use rand::Rng;
        rand::rng().fill_bytes(&mut seed);
    }
    let signing_key = SigningKey::from_bytes(&seed);
    let pub_bytes = signing_key.verifying_key().to_bytes();
    (Zeroizing::new(seed), pub_bytes)
}

// ---------------------------------------------------------------------------
// X.509 mTLS certificate functions
// ---------------------------------------------------------------------------

/// Generate a self-signed X.509 CA certificate (Ed25519).
/// Returns `(cert_der, key_der_pkcs8)`.
pub fn generate_x509_ca() -> Result<(Vec<u8>, Zeroizing<Vec<u8>>)> {
    let key = KeyPair::generate_for(&PKCS_ED25519).context("generate CA Ed25519 key")?;

    let mut params = CertificateParams::new(vec![]).context("create CA cert params")?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(DnType::CommonName, "Lynx Internal CA");
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Lynx");

    let cert = params.self_signed(&key).context("self-sign CA cert")?;
    let cert_der = cert.der().to_vec();
    let key_der = Zeroizing::new(key.serialize_der());

    Ok((cert_der, key_der))
}

/// Issue an X.509 server certificate for an agent (Ed25519).
/// The cert includes the agent's WireGuard IP as a SAN.
/// Returns `(cert_der, key_der_pkcs8)`.
pub fn issue_x509_agent_cert(
    ca_cert_der: &[u8],
    ca_key_der: &[u8],
    agent_id: Uuid,
    wg_ip: &str,
) -> Result<(Vec<u8>, Zeroizing<Vec<u8>>)> {
    let ca_key = KeyPair::try_from(ca_key_der).context("load CA key from DER")?;
    let cert_der_owned = RustlsCertDer::from(ca_cert_der.to_vec());
    let issuer =
        Issuer::from_ca_cert_der(&cert_der_owned, ca_key).context("parse CA cert as issuer")?;

    let leaf_key = KeyPair::generate_for(&PKCS_ED25519).context("generate agent Ed25519 key")?;

    let mut params = CertificateParams::new(vec![]).context("create agent cert params")?;
    params
        .distinguished_name
        .push(DnType::CommonName, format!("lynx-agent-{agent_id}"));
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ServerAuth,
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    if let Ok(ip) = wg_ip.parse::<std::net::IpAddr>() {
        params.subject_alt_names.push(SanType::IpAddress(ip));
    }

    let cert = params
        .signed_by(&leaf_key, &issuer)
        .context("sign agent cert")?;
    let cert_der = cert.der().to_vec();
    let key_der = Zeroizing::new(leaf_key.serialize_der());

    Ok((cert_der, key_der))
}

/// Issue an X.509 client certificate for the dashboard (Ed25519).
/// Used by the dashboard's reqwest client when connecting to agent TLS endpoints.
/// Returns `(cert_der, key_der_pkcs8)`.
pub fn issue_x509_dashboard_client_cert(
    ca_cert_der: &[u8],
    ca_key_der: &[u8],
) -> Result<(Vec<u8>, Zeroizing<Vec<u8>>)> {
    let ca_key = KeyPair::try_from(ca_key_der).context("load CA key from DER")?;
    let cert_der_owned = RustlsCertDer::from(ca_cert_der.to_vec());
    let issuer =
        Issuer::from_ca_cert_der(&cert_der_owned, ca_key).context("parse CA cert as issuer")?;

    let leaf_key =
        KeyPair::generate_for(&PKCS_ED25519).context("generate dashboard client Ed25519 key")?;

    let mut params =
        CertificateParams::new(vec![]).context("create dashboard client cert params")?;
    params
        .distinguished_name
        .push(DnType::CommonName, "lynx-dashboard");
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];

    let cert = params
        .signed_by(&leaf_key, &issuer)
        .context("sign dashboard client cert")?;
    let cert_der = cert.der().to_vec();
    let key_der = Zeroizing::new(leaf_key.serialize_der());

    Ok((cert_der, key_der))
}
