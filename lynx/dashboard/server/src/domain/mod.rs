pub mod handlers;
pub mod router;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DomainConfig {
    pub id: i32,
    pub domain: Option<String>,
    pub cert_type: String,
    pub cert_expires_at: Option<DateTime<Utc>>,
    pub hsts_enabled: bool,
    pub port_19443_open: bool,
    pub status: String,
    pub error_message: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SetDomainRequest {
    pub domain: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct SetHstsRequest {
    pub enabled: bool,
}

/// Upload a Cloudflare Origin Certificate (cert only, no key required by nginx
/// since Cloudflare terminates TLS) or a custom cert+key pair.
#[derive(Debug, Deserialize)]
pub struct UploadCertRequest {
    /// PEM-encoded certificate (chain). Max 64 KB.
    pub cert_pem: String,
    /// PEM-encoded private key. Required for custom certs; omit for Cloudflare Origin.
    pub key_pem: Option<String>,
    /// "cloudflare" or "custom"
    pub cert_type: String,
}
