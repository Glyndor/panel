use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::net::IpAddr;
use std::path::PathBuf;
use uuid::Uuid;

const BINARY_PATH: &str = "/etc/lynx/bin/lynx-dashboard-backend";
const FRONTEND_BINARY: &str = "/etc/lynx/frontend/lynx-dashboard-frontend";
const FRONTEND_DIR: &str = "/etc/lynx/frontend";
const FRONTEND_CONTAINER: &str = "lynx-dashboard-frontend";
const PODMAN_SOCKET: &str = "/run/podman/podman.sock";
const MAX_DOWNLOAD_BYTES: usize = 200 * 1024 * 1024;

pub async fn perform_dashboard_update(
    version: String,
    backend_url: String,
    backend_sig_url: String,
    frontend_url: String,
    frontend_sig_url: String,
    frontend_assets_url: String,
    frontend_assets_sig_url: String,
    log_id: Uuid,
    db: sqlx::PgPool,
) {
    let result = run_full_update(
        &version,
        &backend_url,
        &backend_sig_url,
        &frontend_url,
        &frontend_sig_url,
        &frontend_assets_url,
        &frontend_assets_sig_url,
    )
    .await;

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
        tracing::info!(
            version,
            "dashboard update complete — exiting for Podman restart"
        );
        std::process::exit(0);
    }
}

async fn run_full_update(
    version: &str,
    backend_url: &str,
    backend_sig_url: &str,
    frontend_url: &str,
    frontend_sig_url: &str,
    frontend_assets_url: &str,
    frontend_assets_sig_url: &str,
) -> Result<()> {
    // Download and verify everything before touching any files.
    tracing::info!(version, "downloading and verifying dashboard artifacts");
    let backend_binary =
        download_and_verify(backend_url, backend_sig_url, "backend binary").await?;
    let frontend_binary =
        download_and_verify(frontend_url, frontend_sig_url, "frontend binary").await?;
    let frontend_assets = download_and_verify(
        frontend_assets_url,
        frontend_assets_sig_url,
        "frontend assets",
    )
    .await?;

    tracing::info!(version, "all signatures verified — applying update");

    // Update frontend first (backend stays alive to orchestrate).
    swap_frontend(&frontend_binary, &frontend_assets).await?;

    // Swap backend binary last; process::exit triggers Podman restart with new binary.
    swap_backend_binary(&backend_binary)?;

    Ok(())
}

async fn download_and_verify(url: &str, sig_url: &str, label: &str) -> Result<Vec<u8>> {
    validate_github_url(url)?;
    validate_github_url(sig_url)?;
    let data = download_bytes(url)
        .await
        .with_context(|| format!("download {label}"))?;
    let sig = download_bytes(sig_url)
        .await
        .with_context(|| format!("download {label} signature"))?;
    verify_signature(&data, &sig).with_context(|| format!("{label} signature invalid"))?;
    tracing::info!(label, bytes = data.len(), "signature verified");
    Ok(data)
}

async fn swap_frontend(binary: &[u8], assets: &[u8]) -> Result<()> {
    // Stop container so the binary file is not in use during swap.
    podman_request(&format!("/containers/{FRONTEND_CONTAINER}/stop"))
        .await
        .context("stop frontend container")?;

    // Swap binary.
    let target = PathBuf::from(FRONTEND_BINARY);
    let prev = PathBuf::from(format!("{FRONTEND_BINARY}.prev"));
    let tmp = PathBuf::from(format!("{FRONTEND_BINARY}.new"));

    std::fs::write(&tmp, binary).context("write frontend binary")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms)?;
    }
    if target.exists() {
        std::fs::copy(&target, &prev).context("backup frontend binary to .prev")?;
    }
    std::fs::rename(&tmp, &target).context("atomic rename frontend binary")?;

    // Extract static assets (overwrites .next/static and public/ in-place).
    let assets_owned = assets.to_vec();
    tokio::task::spawn_blocking(move || extract_assets(&assets_owned, FRONTEND_DIR))
        .await
        .context("spawn_blocking extract assets")??;

    // Start container with the new binary.
    podman_request(&format!("/containers/{FRONTEND_CONTAINER}/start"))
        .await
        .context("start frontend container")?;

    Ok(())
}

fn swap_backend_binary(binary: &[u8]) -> Result<()> {
    let target = PathBuf::from(BINARY_PATH);
    let prev = PathBuf::from(format!("{BINARY_PATH}.prev"));
    let tmp = PathBuf::from(format!("{BINARY_PATH}.new"));

    std::fs::write(&tmp, binary).context("write backend binary")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms)?;
    }
    if target.exists() {
        std::fs::copy(&target, &prev).context("backup backend binary to .prev")?;
    }
    std::fs::rename(&tmp, &target).context("atomic rename backend binary")?;

    Ok(())
}

fn extract_assets(data: &[u8], dest: &str) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("tar")
        .args(["-xz", "-C", dest])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn tar")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(data)
            .context("write tarball to tar stdin")?;
    }

    let output = child.wait_with_output().context("wait for tar")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tar extraction failed: {stderr}");
    }
    Ok(())
}

async fn podman_request(path: &str) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    let mut stream = UnixStream::connect(PODMAN_SOCKET)
        .await
        .context("connect to Podman socket")?;

    let req = format!("POST {path} HTTP/1.0\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;
    stream.shutdown().await?;

    let mut buf = Vec::with_capacity(1024);
    stream.read_to_end(&mut buf).await?;

    let resp = std::str::from_utf8(&buf).unwrap_or("");
    let status_line = resp.lines().next().unwrap_or("");

    // 2xx = success; 304 = already in target state (container already stopped/started)
    if !status_line.contains(" 2") && !status_line.contains(" 304") {
        anyhow::bail!("Podman API {path}: {status_line}");
    }

    Ok(())
}

fn validate_github_url(url: &str) -> Result<()> {
    let allowed = [
        "https://github.com/",
        "https://objects.githubusercontent.com/",
    ];
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
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("no host in URL {url}"))?;
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
    let client_builder = reqwest::Client::builder()
        .user_agent(format!("lynx-dashboard/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(300))
        .resolve(host, std::net::SocketAddr::new(pinned_ip, port));

    client_builder
        .build()
        .context("build SSRF-safe HTTP client")
}

fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            if octets[0] == 10 {
                return true;
            }
            if octets[0] == 172 && (octets[1] & 0xF0) == 16 {
                return true;
            }
            if octets[0] == 192 && octets[1] == 168 {
                return true;
            }
            if octets[0] == 127 {
                return true;
            }
            if octets[0] == 169 && octets[1] == 254 {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return true;
            }
            let segs = v6.segments();
            if (segs[0] & 0xFE00) == 0xFC00 {
                return true;
            }
            if (segs[0] & 0xFFC0) == 0xFE80 {
                return true;
            }
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

/// Called at startup to detect a failed update and restore `.prev` if needed.
/// Spawns a background task: polls `/health` every 2s for 30s.
/// If still unhealthy → restores `.prev`, writes `/etc/lynx/CRITICAL`, exits.
pub fn spawn_startup_health_guard() {
    const CRITICAL_FILE: &str = "/etc/lynx/CRITICAL";

    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
        {
            Ok(c) => c,
            Err(_) => return,
        };

        for _ in 0..15 {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if client
                .get("http://127.0.0.1:8080/health") // audit-urls: ok — self health check, not a download
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
            {
                return; // healthy — nothing to do
            }
        }

        // Still unhealthy after 30s — restore .prev
        tracing::error!("startup health check failed — restoring .prev binary");
        let target = PathBuf::from(BINARY_PATH);
        let prev = PathBuf::from(format!("{BINARY_PATH}.prev"));

        let restore_ok = if prev.exists() {
            std::fs::copy(&prev, &target).is_ok()
        } else {
            false
        };

        let reason = if restore_ok {
            "new binary failed health check; restored .prev"
        } else {
            "new binary failed health check; .prev unavailable — MANUAL RECOVERY REQUIRED"
        };

        let ts = chrono::Utc::now().to_rfc3339();
        let _ = std::fs::write(
            CRITICAL_FILE,
            format!("timestamp={ts}\ncomponent=lynx-dashboard-backend\nreason={reason}\n"),
        );

        tracing::error!(reason, "critical state — exiting");
        std::process::exit(1);
    });
}

fn verify_signature(binary: &[u8], sig_bytes: &[u8]) -> Result<()> {
    let key_bytes = load_release_verify_key()?;
    let key = VerifyingKey::from_bytes(&key_bytes).context("parse release verify key")?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must be 64 bytes, got {}", sig_bytes.len()))?;
    let sig = Signature::from_bytes(&sig_arr);
    key.verify(binary, &sig)
        .context("Ed25519 signature invalid")
}

const RELEASE_VERIFY_KEY_B64: &str = "OsBV4t+vQSn10FAI8UzAJEBS0IUqp8D2bZtlQYD8j+Q=";

fn load_release_verify_key() -> Result<[u8; 32]> {
    use base64ct::{Base64, Encoding};
    let bytes = Base64::decode_vec(RELEASE_VERIFY_KEY_B64)
        .context("decode hardcoded release verify key")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("release verify key must be 32 bytes"))
}
