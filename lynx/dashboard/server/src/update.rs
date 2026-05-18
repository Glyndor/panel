use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::net::IpAddr;
use std::path::PathBuf;
use uuid::Uuid;

const BINARY_PATH: &str = "/etc/lynx/bin/lynx-dashboard-backend";
const MAX_DOWNLOAD_BYTES: usize = 200 * 1024 * 1024;

pub async fn perform_dashboard_update(
    version: String,
    backend_url: String,
    backend_sig_url: String,
    _frontend_url: String,
    _frontend_sig_url: String,
    log_id: Uuid,
    db: sqlx::PgPool,
) {
    let result = do_backend_swap(&version, &backend_url, &backend_sig_url).await;

    let status = match result {
        Ok(()) => "success",
        Err(ref e) => {
            tracing::error!(version, "dashboard self-update failed: {e:#}");
            "failed"
        }
    };

    let _ = sqlx::query!(
        "UPDATE update_log SET status = $1 WHERE id = $2",
        status,
        log_id
    )
    .execute(&db)
    .await;

    if result.is_ok() {
        tracing::info!(version, "dashboard backend swap complete — exiting for Podman restart");
        std::process::exit(0);
    }
}

async fn do_backend_swap(version: &str, url: &str, sig_url: &str) -> Result<()> {
    tracing::info!(version, "downloading dashboard backend binary");

    validate_github_url(url)?;
    validate_github_url(sig_url)?;

    let binary = download_bytes(url).await.context("download binary")?;
    let sig = download_bytes(sig_url).await.context("download signature")?;

    verify_signature(&binary, &sig).context("signature verification failed — update aborted")?;
    tracing::info!(version, bytes = binary.len(), "signature verified");

    let target = PathBuf::from(BINARY_PATH);
    let prev = PathBuf::from(format!("{BINARY_PATH}.prev"));
    let tmp = PathBuf::from(format!("{BINARY_PATH}.new"));

    // Write new binary to temp path
    std::fs::write(&tmp, &binary).context("write new binary to .new")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms)?;
    }

    // Back up current binary
    if target.exists() {
        std::fs::copy(&target, &prev).context("backup current binary to .prev")?;
    }

    // Atomic swap
    std::fs::rename(&tmp, &target).context("atomic rename .new → binary")?;

    Ok(())
}

fn validate_github_url(url: &str) -> Result<()> {
    let allowed = ["https://github.com/", "https://objects.githubusercontent.com/"];
    if allowed.iter().any(|prefix| url.starts_with(prefix)) {
        Ok(())
    } else {
        anyhow::bail!("download URL not on allowed domain: {url}")
    }
}

/// Resolve the hostname in `url` once, validate all returned IPs against
/// RFC1918/loopback/link-local ranges (SSRF prevention), and return a
/// reqwest Client pre-configured to connect to the first valid resolved IP.
/// This prevents TOCTOU: we resolve once and pin to that IP — no second DNS lookup.
async fn build_ssrf_safe_client(url: &str) -> Result<reqwest::Client> {
    let parsed = url::Url::parse(url).with_context(|| format!("parse URL {url}"))?;
    let host = parsed.host_str().ok_or_else(|| anyhow::anyhow!("no host in URL {url}"))?;
    let port = parsed.port_or_known_default().unwrap_or(443);

    let addrs: Vec<IpAddr> = tokio::net::lookup_host(format!("{host}:{port}"))
        .await
        .with_context(|| format!("DNS lookup for {host}"))?
        .map(|s| s.ip())
        .collect();

    if addrs.is_empty() {
        anyhow::bail!("DNS lookup returned no addresses for {host}");
    }

    for ip in &addrs {
        if is_blocked_ip(ip) {
            anyhow::bail!("DNS resolved {host} to blocked IP {ip} — SSRF check failed");
        }
    }

    // Build client with pinned resolver to avoid second DNS lookup (TOCTOU).
    let pinned_ip = addrs[0];
    let mut client_builder = reqwest::Client::builder()
        .user_agent(format!("lynx-dashboard/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(300))
        .resolve(host, std::net::SocketAddr::new(pinned_ip, port));

    // For TLS (GitHub), we still need SNI to use the original hostname.
    // reqwest handles this correctly when using .resolve().
    client_builder = client_builder.danger_accept_invalid_certs(false);

    client_builder.build().context("build SSRF-safe HTTP client")
}

fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 10.0.0.0/8
            if octets[0] == 10 { return true; }
            // 172.16.0.0/12
            if octets[0] == 172 && (octets[1] & 0xF0) == 16 { return true; }
            // 192.168.0.0/16
            if octets[0] == 192 && octets[1] == 168 { return true; }
            // 127.0.0.0/8 loopback
            if octets[0] == 127 { return true; }
            // 169.254.0.0/16 link-local
            if octets[0] == 169 && octets[1] == 254 { return true; }
            false
        }
        IpAddr::V6(v6) => {
            // ::1 loopback
            if v6.is_loopback() { return true; }
            let segs = v6.segments();
            // fc00::/7 unique local
            if (segs[0] & 0xFE00) == 0xFC00 { return true; }
            // fe80::/10 link-local
            if (segs[0] & 0xFFC0) == 0xFE80 { return true; }
            false
        }
    }
}

async fn download_bytes(url: &str) -> Result<Vec<u8>> {
    let client = build_ssrf_safe_client(url).await?;

    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error for {url}"))?;

    if let Some(len) = resp.content_length() {
        if len as usize > MAX_DOWNLOAD_BYTES {
            anyhow::bail!("Content-Length {len} exceeds safety limit");
        }
    }

    let bytes = resp.bytes().await.context("read response body")?;
    if bytes.len() > MAX_DOWNLOAD_BYTES {
        anyhow::bail!("download exceeded safety limit");
    }
    Ok(bytes.to_vec())
}

fn verify_signature(binary: &[u8], sig_bytes: &[u8]) -> Result<()> {
    let key_bytes = load_release_verify_key()?;
    let key = VerifyingKey::from_bytes(&key_bytes).context("parse release verify key")?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must be 64 bytes, got {}", sig_bytes.len()))?;
    let sig = Signature::from_bytes(&sig_arr);
    key.verify(binary, &sig).context("Ed25519 signature invalid")
}

fn load_release_verify_key() -> Result<[u8; 32]> {
    use base64ct::{Base64, Encoding};
    let raw = if let Ok(path) = std::env::var("RELEASE_VERIFY_KEY_FILE") {
        std::fs::read_to_string(&path)
            .with_context(|| format!("read RELEASE_VERIFY_KEY_FILE={path}"))?
    } else {
        std::env::var("RELEASE_VERIFY_KEY")
            .or_else(|_| std::env::var("DASHBOARD_VERIFY_KEY"))
            .context("RELEASE_VERIFY_KEY not configured")?
    };
    let bytes = Base64::decode_vec(raw.trim()).context("base64 decode release verify key")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("release verify key must be 32 bytes"))
}
