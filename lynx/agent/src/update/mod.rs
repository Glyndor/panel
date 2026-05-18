pub mod fallback;

use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::path::{Path, PathBuf};

/// Download new binary, verify Ed25519 signature, atomic swap, then exec into new process.
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
    let bin_client = build_ssrf_safe_client(download_url).await.context("SSRF check for binary URL")?;
    let sig_client = build_ssrf_safe_client(sig_url).await.context("SSRF check for sig URL")?;

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

    // Write new binary to a temp path beside the current executable
    let current_exe = std::env::current_exe().context("resolve current exe")?;
    let tmp_path = tmp_path(&current_exe);

    std::fs::write(&tmp_path, &binary_bytes).with_context(|| format!("write to {tmp_path:?}"))?;

    // Make it executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp_path, perms)?;
    }

    // Atomic rename: tmp → current exe path (POSIX atomic on same filesystem)
    std::fs::rename(&tmp_path, &current_exe)
        .with_context(|| format!("rename {tmp_path:?} → {current_exe:?}"))?;

    tracing::info!(version, "binary swapped — restarting via systemd");

    // Systemd will restart us because the unit has Restart=on-failure (or always).
    // We exit with code 0 so systemd treats it as a clean restart.
    std::process::exit(0);
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
    let bytes =
        Base64::decode_vec(RELEASE_VERIFY_KEY_B64).context("decode hardcoded release verify key")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("release verify key must be 32 bytes"))
}

fn tmp_path(exe: &Path) -> PathBuf {
    let mut p = exe.to_path_buf();
    let name = exe
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("lynx-agent");
    p.set_file_name(format!("{name}.new"));
    p
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

    let addrs: Vec<std::net::SocketAddr> =
        tokio::net::lookup_host(format!("{host}:{port}"))
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
                || (v6.segments()[0] & 0xff00) == 0xfe80  // fe80::/10 link-local
        }
    }
}
