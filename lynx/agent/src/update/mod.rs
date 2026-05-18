use anyhow::{Context, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::path::{Path, PathBuf};

/// Download new binary, verify Ed25519 signature, atomic swap, then exec into new process.
///
/// The public key used for signature verification is the same Ed25519 key
/// the dashboard uses to sign commands (DASHBOARD_VERIFY_KEY env var).
/// Release binaries are signed with the corresponding private key at release time.
pub async fn perform_update(version: &str, download_url: &str, sig_url: &str) -> Result<()> {
    tracing::info!(version, "starting self-update");

    let client = reqwest::Client::builder()
        .user_agent(format!("lynx-agent/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .context("build HTTP client")?;

    // Download binary
    let binary_bytes = download_bytes(&client, download_url)
        .await
        .context("download binary")?;

    // Download signature
    let sig_bytes = download_bytes(&client, sig_url)
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

fn load_verify_key() -> Result<[u8; 32]> {
    use base64ct::{Base64, Encoding};

    // Reuse DASHBOARD_VERIFY_KEY env / file — same key signs commands and release binaries.
    let raw = if let Ok(path) = std::env::var("DASHBOARD_VERIFY_KEY_FILE") {
        std::fs::read_to_string(&path)
            .with_context(|| format!("read DASHBOARD_VERIFY_KEY_FILE={path}"))?
    } else {
        std::env::var("DASHBOARD_VERIFY_KEY").context("DASHBOARD_VERIFY_KEY not configured")?
    };

    let bytes = Base64::decode_vec(raw.trim()).context("base64 decode DASHBOARD_VERIFY_KEY")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("DASHBOARD_VERIFY_KEY must be 32 bytes"))
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
