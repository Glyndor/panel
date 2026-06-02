pub mod fallback;

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::path::PathBuf;

const AGENT_BINARY: &str = "/etc/lynx/bin/lynx-agent";
const CRITICAL_FILE: &str = "/etc/lynx/CRITICAL";

/// Download new binary, verify Ed25519 signature, backup to .prev, atomic swap, restart via systemd.
///
/// The release verify key (`RELEASE_VERIFY_KEY_B64`) is compiled into the binary and is distinct
/// from the dashboard command-signing key. The corresponding private key lives only in GitHub
/// Actions secrets — compromising the repo or the dashboard does not allow forging signatures.
pub async fn perform_update(version: &str, download_url: &str, sig_url: &str) -> Result<()> {
    validate_github_url(download_url)?;
    validate_github_url(sig_url)?;

    tracing::info!(version, "starting self-update");

    // Build separate SSRF-safe clients per URL: resolves DNS once, validates
    // the resolved IP is not RFC1918/loopback, then pins the hostname to that
    // IP for the actual request (prevents DNS TOCTOU rebinding attacks).
    let bin_client = build_ssrf_safe_client(download_url)
        .await
        .context("SSRF check for binary URL")?;
    let sig_client = build_ssrf_safe_client(sig_url)
        .await
        .context("SSRF check for sig URL")?;

    // Download binary
    let binary_bytes = download_bytes(&bin_client, download_url)
        .await
        .context("download binary")?;

    // Download signature
    let sig_bytes = download_bytes(&sig_client, sig_url)
        .await
        .context("download signature")?;

    // Verify Ed25519 signature
    verify_signature(&binary_bytes, &sig_bytes)
        .context("signature verification failed — update aborted")?;

    tracing::info!(version, bytes = binary_bytes.len(), "signature verified");

    let target = PathBuf::from(AGENT_BINARY);
    let prev = PathBuf::from(format!("{AGENT_BINARY}.prev"));
    let tmp = PathBuf::from(format!("{AGENT_BINARY}.new"));

    std::fs::write(&tmp, &binary_bytes).with_context(|| format!("write to {tmp:?}"))?;

    // Make it executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms)?;
    }

    // Back up current binary to .prev before swap
    if target.exists() {
        std::fs::copy(&target, &prev).context("backup agent binary to .prev")?;
    }

    // Atomic rename: tmp → canonical path (POSIX atomic on same filesystem)
    std::fs::rename(&tmp, &target).with_context(|| format!("rename {tmp:?} → {target:?}"))?;

    tracing::info!(version, "binary swapped — restarting via systemd");

    // Systemd will restart the unit (Restart=always in the service unit).
    // Exit 0 so systemd records a clean restart, not a failure.
    std::process::exit(0);
}

/// Spawn a background task that monitors agent startup health.
///
/// Polls `http://127.0.0.1:9090/health` every 2s for 30s.
/// If still unhealthy → attempt `.prev` restore and exit 1 (systemd restarts with old binary).
/// If `.prev` unavailable or restore fails → write `/etc/lynx/CRITICAL` and exit 1.
/// On healthy startup → delete `/etc/lynx/CRITICAL` if present (recovery from prior critical state).
pub fn spawn_startup_health_guard() {
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
                .get("http://127.0.0.1:9090/health") // audit-urls: ok — self health check, not a download
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false)
            {
                // Healthy — clear any leftover CRITICAL file from a previous failed startup.
                let _ = std::fs::remove_file(CRITICAL_FILE);
                return;
            }
        }

        // Still unhealthy after 30s — attempt .prev restore.
        tracing::error!("startup health check failed — restoring .prev binary");
        let target = PathBuf::from(AGENT_BINARY);
        let prev = PathBuf::from(format!("{AGENT_BINARY}.prev"));

        let restore_ok = if prev.exists() {
            // Atomic rename to avoid ETXTBSY — the current binary is a running executable,
            // so copy() with O_TRUNC fails. Write to .new first, then rename (POSIX atomic).
            let tmp = PathBuf::from(format!("{AGENT_BINARY}.restoring"));
            std::fs::copy(&prev, &tmp).is_ok() && std::fs::rename(&tmp, &target).is_ok()
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
            format!("timestamp={ts}\ncomponent=lynx-agent\nreason={reason}\n"),
        );

        tracing::error!(reason, "critical state — exiting for systemd restart");
        std::process::exit(1);
    });
}

async fn download_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {} for {url}", resp.status());
    }

    // content_length is a hint; we still cap to 200 MiB
    if let Some(len) = resp.content_length() {
        if len > 200 * 1024 * 1024 {
            anyhow::bail!("Content-Length {len} exceeds 200 MiB safety limit");
        }
    }

    let bytes = resp.bytes().await.context("read response body")?;
    if bytes.len() > 200 * 1024 * 1024 {
        anyhow::bail!("download exceeded 200 MiB safety limit");
    }
    Ok(bytes.to_vec())
}

fn verify_signature(binary: &[u8], sig_bytes: &[u8]) -> Result<()> {
    let key_bytes = load_verify_key()?;
    let key = VerifyingKey::from_bytes(&key_bytes).context("parse DASHBOARD_VERIFY_KEY")?;

    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("signature must be 64 bytes, got {}", sig_bytes.len()))?;
    let sig = Signature::from_bytes(&sig_arr);

    key.verify(binary, &sig)
        .context("Ed25519 signature invalid")
}

const RELEASE_VERIFY_KEY_B64: &str = "OsBV4t+vQSn10FAI8UzAJEBS0IUqp8D2bZtlQYD8j+Q=";

fn load_verify_key() -> Result<[u8; 32]> {
    use base64ct::{Base64, Encoding};
    let bytes = Base64::decode_vec(RELEASE_VERIFY_KEY_B64)
        .context("decode hardcoded release verify key")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("release verify key must be 32 bytes"))
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

/// Builds an HTTP client with SSRF protection:
/// 1. Resolves the hostname of `url` via DNS (once).
/// 2. Rejects if any resolved IP is RFC1918, loopback, or link-local.
/// 3. Pins the hostname to the validated IP so reqwest never re-resolves it
///    (prevents DNS rebinding / TOCTOU attacks).
async fn build_ssrf_safe_client(url: &str) -> Result<reqwest::Client> {
    let parsed = url::Url::parse(url).context("parse URL for SSRF check")?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("URL has no host: {url}"))?
        .to_string();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| anyhow::anyhow!("URL has unknown port: {url}"))?;

    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(format!("{host}:{port}"))
        .await
        .with_context(|| format!("DNS lookup for {host}"))?
        .collect();

    if addrs.is_empty() {
        anyhow::bail!("DNS lookup for {host} returned no addresses");
    }

    for addr in &addrs {
        if is_private_ip(addr.ip()) {
            anyhow::bail!(
                "SSRF protection: {host} resolved to private/reserved IP {}",
                addr.ip()
            );
        }
    }

    reqwest::Client::builder()
        .user_agent(format!("lynx-agent/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(300))
        .resolve(&host, addrs[0])
        .build()
        .context("build SSRF-safe HTTP client")
}

fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00  // fc00::/7 ULA
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
        }
    }
}
