use super::super::{DomainConfig, SetDomainRequest, SetHstsRequest, UploadCertRequest};
use super::nginx::{
    custom_cert_path, custom_key_path, nginx_conf, nginx_conf_with_cert, NGINX_IMAGE,
};
use crate::{
    agents::client::build_agent_client, auth::middleware::AuthUser, crypto::cmd, error::AppError,
    state::AppState,
};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::net::ToSocketAddrs;
use uuid::Uuid;

struct LocalAgent {
    id: Uuid,
    wg_ip: String,
    api_port: i32,
}

async fn get_local_agent(state: &AppState) -> Result<LocalAgent, AppError> {
    let row = sqlx::query!(
        "SELECT id, wg_ip::text AS wg_ip, api_port FROM agents WHERE is_local_agent = true LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::BadRequest("no local agent registered"))?;

    Ok(LocalAgent {
        id: row.id,
        wg_ip: row.wg_ip,
        api_port: row.api_port,
    })
}

async fn send_cmd(
    state: &AppState,
    agent: &LocalAgent,
    triggered_by: Uuid,
    command: &serde_json::Value,
) -> Result<reqwest::Response, AppError> {
    let signed = cmd::sign_command(&state.config, agent.id, triggered_by, "write", command)
        .map_err(AppError::Internal)?;

    let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
    let client = build_agent_client(&state.config);

    client
        .post(&url)
        .header(
            "Authorization",
            format!("Bearer {}", &*state.config.internal_token),
        )
        .json(&signed)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("agent request failed: {e}")))
}

pub async fn get_domain(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query_as!(
        DomainConfig,
        "SELECT id, domain, cert_type, cert_expires_at, hsts_enabled, port_19443_open, status, error_message, updated_at FROM domain_config WHERE id = 1"
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(cfg))
}

pub async fn set_domain(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<SetDomainRequest>,
) -> Result<impl IntoResponse, AppError> {
    let domain = req.domain.trim().to_lowercase();

    if domain.is_empty() || domain.contains(' ') {
        return Err(AppError::Validation("invalid domain".into()));
    }

    if !req.email.contains('@') || req.email.contains(' ') {
        return Err(AppError::Validation(
            "invalid email for Let's Encrypt".into(),
        ));
    }

    sqlx::query!(
        "UPDATE domain_config SET domain=$1, status='pending', error_message=NULL, updated_at=NOW() WHERE id=1",
        domain
    )
    .execute(&state.db)
    .await?;

    let state_clone = state.clone();
    let domain_clone = domain.clone();
    let email = req.email.clone();
    let user_id = user.user_id;

    tokio::spawn(async move {
        let result = configure_domain_via_agent(&state_clone, &domain_clone, &email, user_id).await;
        match result {
            Ok(()) => {
                let _ = sqlx::query!(
                    "UPDATE domain_config SET status='active', cert_type='lets_encrypt', cert_expires_at=NOW() + INTERVAL '90 days', updated_at=NOW() WHERE id=1"
                )
                .execute(&state_clone.db)
                .await;
            }
            Err(e) => {
                let msg = e.to_string();
                let _ = sqlx::query!(
                    "UPDATE domain_config SET status='error', error_message=$1, updated_at=NOW() WHERE id=1",
                    msg
                )
                .execute(&state_clone.db)
                .await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "status": "pending", "domain": domain })),
    ))
}

async fn configure_domain_via_agent(
    state: &AppState,
    domain: &str,
    email: &str,
    user_id: Uuid,
) -> Result<(), AppError> {
    let agent = get_local_agent(state).await?;

    // 1. Deploy nginx with HTTP-only config for ACME challenge.
    let initial_config = nginx_conf(domain, false, false);
    let deploy_cmd = json!({
        "type": "nginx.deploy",
        "image": NGINX_IMAGE,
        "config": initial_config,
    });
    let resp = send_cmd(state, &agent, user_id, &deploy_cmd).await?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "nginx.deploy failed: {}",
            resp.status()
        )));
    }

    // 2. Obtain Let's Encrypt cert via certbot (webroot challenge).
    let certbot_cmd = json!({
        "type": "certbot.obtain",
        "domain": domain,
        "email": email,
    });
    let resp = send_cmd(state, &agent, user_id, &certbot_cmd).await?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "certbot.obtain failed: {}",
            resp.status()
        )));
    }

    // 3. Reload nginx with TLS config.
    let tls_config = nginx_conf(domain, true, false);
    let update_cmd = json!({
        "type": "nginx.update_config",
        "config": tls_config,
    });
    let resp = send_cmd(state, &agent, user_id, &update_cmd).await?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "nginx.update_config failed: {}",
            resp.status()
        )));
    }

    Ok(())
}

pub async fn verify_domain(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!("SELECT domain FROM domain_config WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    let domain = cfg
        .domain
        .ok_or(AppError::Validation("no domain configured".into()))?;

    let dns_ok = check_dns(&domain).await;

    Ok(Json(json!({
        "domain": domain,
        "dns_ok": dns_ok,
    })))
}

pub async fn set_hsts(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<SetHstsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!("SELECT status, domain, cert_type FROM domain_config WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    if cfg.status != "active" {
        return Err(AppError::Validation(
            "HSTS can only be enabled after domain is fully configured".into(),
        ));
    }

    sqlx::query!(
        "UPDATE domain_config SET hsts_enabled=$1, updated_at=NOW() WHERE id=1",
        req.enabled
    )
    .execute(&state.db)
    .await?;

    if let Some(domain) = cfg.domain {
        let agent = match get_local_agent(&state).await {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("HSTS change persisted but nginx reload failed: {e}");
                return Ok(Json(json!({ "hsts_enabled": req.enabled })));
            }
        };

        // Use cert paths appropriate for cert_type.
        let nginx_config = match cfg.cert_type.as_str() {
            "cloudflare" | "custom" => {
                let cert_path = custom_cert_path(&domain);
                let key_path = custom_key_path(&domain);
                nginx_conf_with_cert(&domain, &cert_path, &key_path, req.enabled)
            }
            _ => nginx_conf(&domain, true, req.enabled),
        };

        let update_cmd = json!({
            "type": "nginx.update_config",
            "config": nginx_config,
        });
        if let Err(e) = send_cmd(&state, &agent, user.user_id, &update_cmd).await {
            tracing::warn!("nginx reload after HSTS change failed: {e}");
        }
    }

    Ok(Json(json!({ "hsts_enabled": req.enabled })))
}

pub async fn close_port(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = sqlx::query!("SELECT status FROM domain_config WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    if cfg.status != "active" {
        return Err(AppError::Validation(
            "can only close port 19443 after domain is fully active".into(),
        ));
    }

    let agent = get_local_agent(&state).await?;
    let close_cmd = json!({ "type": "nftables.close_setup_port" });
    let resp = send_cmd(&state, &agent, user.user_id, &close_cmd).await?;

    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "nftables.close_setup_port failed: {}",
            resp.status()
        )));
    }

    sqlx::query!("UPDATE domain_config SET port_19443_open=false, updated_at=NOW() WHERE id=1")
        .execute(&state.db)
        .await?;

    Ok(Json(json!({ "port_19443_open": false })))
}

/// Upload a Cloudflare Origin Certificate or a custom cert+key pair.
/// Validates the certificate before sending it to the local agent.
pub async fn upload_cert(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<UploadCertRequest>,
) -> Result<impl IntoResponse, AppError> {
    const MAX_CERT_BYTES: usize = 64 * 1024; // 64 KB

    if !["cloudflare", "custom"].contains(&req.cert_type.as_str()) {
        return Err(AppError::Validation(
            "cert_type must be 'cloudflare' or 'custom'".into(),
        ));
    }

    if req.cert_pem.len() > MAX_CERT_BYTES {
        return Err(AppError::Validation("cert_pem exceeds 64 KB limit".into()));
    }

    if let Some(ref key) = req.key_pem {
        if key.len() > MAX_CERT_BYTES {
            return Err(AppError::Validation("key_pem exceeds 64 KB limit".into()));
        }
    }

    let cfg = sqlx::query!("SELECT domain, status FROM domain_config WHERE id = 1")
        .fetch_one(&state.db)
        .await?;

    let domain = cfg
        .domain
        .ok_or(AppError::Validation("no domain configured".into()))?;

    validate_cert(&req.cert_pem, &domain, req.key_pem.as_deref())?;

    let agent = get_local_agent(&state).await?;

    // Install cert on agent.
    let mut install_cmd = json!({
        "type": "nginx.install_cert",
        "domain": domain,
        "cert_pem": req.cert_pem,
    });
    if let Some(ref key) = req.key_pem {
        install_cmd["key_pem"] = json!(key);
    }
    let resp = send_cmd(&state, &agent, user.user_id, &install_cmd).await?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!(
            "nginx.install_cert failed: {}",
            resp.status()
        )));
    }

    // Update nginx config to use custom cert paths.
    let cert_path = custom_cert_path(&domain);
    let key_path = custom_key_path(&domain);
    let hsts = cfg.status == "active"; // keep existing HSTS if already active
    let nginx_cfg = nginx_conf_with_cert(&domain, &cert_path, &key_path, false);
    let update_cmd = json!({
        "type": "nginx.update_config",
        "config": nginx_cfg,
    });
    let _ = send_cmd(&state, &agent, user.user_id, &update_cmd).await;

    let _ = hsts; // suppress unused warning

    // Detect expiry from cert for DB record.
    let expires_at = cert_expires_at(&req.cert_pem);

    sqlx::query!(
        r#"UPDATE domain_config
           SET cert_type=$1, cert_expires_at=$2, status='active', error_message=NULL, updated_at=NOW()
           WHERE id=1"#,
        req.cert_type,
        expires_at,
    )
    .execute(&state.db)
    .await?;

    Ok(Json(json!({
        "ok": true,
        "cert_type": req.cert_type,
        "expires_at": expires_at,
    })))
}

/// Validate a PEM cert (and optional key) before sending to the agent.
fn validate_cert(cert_pem: &str, domain: &str, key_pem: Option<&str>) -> Result<(), AppError> {
    use x509_parser::extensions::GeneralName;
    use x509_parser::pem::parse_x509_pem;

    // Parse PEM → X.509 cert in one step.
    let (_, pem) = parse_x509_pem(cert_pem.as_bytes())
        .map_err(|_| AppError::Validation("cert_pem is not valid PEM".into()))?;
    let cert = pem
        .parse_x509()
        .map_err(|_| AppError::Validation("cert_pem is not a valid X.509 certificate".into()))?;

    // Check expiry.
    let now = chrono::Utc::now();
    let not_before = cert.validity().not_before.to_datetime();
    let not_after = cert.validity().not_after.to_datetime();

    // x509-parser returns OffsetDateTime; convert to chrono
    let not_before_ts = chrono::DateTime::from_timestamp(not_before.unix_timestamp(), 0)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH);
    let not_after_ts = chrono::DateTime::from_timestamp(not_after.unix_timestamp(), 0)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH);

    if now < not_before_ts {
        return Err(AppError::Validation(
            "certificate is not yet valid (not_before in future)".into(),
        ));
    }
    if now > not_after_ts {
        return Err(AppError::Validation("certificate has expired".into()));
    }

    // Check SAN or CN matches domain.
    let san_ok = cert
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|san_ext| {
            san_ext.value.general_names.iter().any(|name| match name {
                GeneralName::DNSName(dns) => {
                    *dns == domain
                        || dns.strip_prefix("*.").is_some_and(|suffix| {
                            domain.ends_with(suffix)
                                && !domain
                                    .trim_end_matches(suffix)
                                    .trim_end_matches('.')
                                    .contains('.')
                        })
                }
                _ => false,
            })
        })
        .unwrap_or(false);

    let cn_ok = cert
        .subject()
        .iter_common_name()
        .any(|cn| cn.as_str() == Ok(domain));

    if !san_ok && !cn_ok {
        return Err(AppError::Validation(format!(
            "certificate SAN/CN does not match domain '{domain}'"
        )));
    }

    // For custom certs, verify key pair.
    if let Some(key_pem_str) = key_pem {
        verify_key_pair(cert_pem, key_pem_str)
            .map_err(|e| AppError::Validation(format!("cert/key pair mismatch: {e}")))?;
    }

    Ok(())
}

/// Extract expiry timestamp from PEM cert.
fn cert_expires_at(cert_pem: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use x509_parser::pem::parse_x509_pem;
    let (_, pem) = parse_x509_pem(cert_pem.as_bytes()).ok()?;
    let cert = pem.parse_x509().ok()?;
    let ts = cert.validity().not_after.to_datetime().unix_timestamp();
    chrono::DateTime::from_timestamp(ts, 0)
}

/// Verify that cert and key are a matching pair using rcgen round-trip.
fn verify_key_pair(cert_pem: &str, key_pem: &str) -> Result<(), anyhow::Error> {
    use rcgen::KeyPair;
    use x509_parser::pem::parse_x509_pem;

    let key =
        KeyPair::from_pem(key_pem).map_err(|e| anyhow::anyhow!("invalid private key PEM: {e}"))?;

    let (_, pem) = parse_x509_pem(cert_pem.as_bytes())
        .map_err(|e| anyhow::anyhow!("invalid cert PEM: {e}"))?;
    let cert = pem
        .parse_x509()
        .map_err(|e| anyhow::anyhow!("invalid cert DER: {e}"))?;

    let cert_pubkey = cert.public_key().raw;
    let key_pubkey = key.public_key_raw();

    if cert_pubkey != key_pubkey {
        anyhow::bail!("public key in cert does not match private key");
    }

    Ok(())
}

async fn check_dns(domain: &str) -> bool {
    let domain = domain.to_string();
    tokio::task::spawn_blocking(move || {
        format!("{domain}:443")
            .to_socket_addrs()
            .map(|mut addrs| addrs.next().is_some())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}
