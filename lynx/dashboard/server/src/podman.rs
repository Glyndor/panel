use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const PODMAN_SOCK: &str = "/run/podman/podman.sock";

/// Make a raw HTTP/1.1 request over the Podman Unix socket.
/// Returns the HTTP status code.
async fn podman_http(method: &str, path: &str, body: &[u8]) -> Result<u16> {
    let mut stream = UnixStream::connect(PODMAN_SOCK)
        .await
        .context("connect to podman socket")?;

    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: d\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .context("write request headers")?;
    if !body.is_empty() {
        stream.write_all(body).await.context("write request body")?;
    }

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .context("read response")?;

    let status = response
        .split(|&b| b == b'\n')
        .next()
        .and_then(|l| std::str::from_utf8(l).ok())
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    Ok(status)
}

/// Create or replace a Podman secret via the socket API.
/// Podman 4.9.x has a bug where `replace=true` returns 500 when the secret
/// doesn't yet exist. Work around it by deleting first (ignoring 404) then creating.
pub async fn secret_replace(name: &str, data: &[u8]) -> Result<()> {
    // Best-effort delete; ignore 404 (secret doesn't exist yet).
    let del_path = format!("/v4.0.0/libpod/secrets/{name}");
    let _ = podman_http("DELETE", &del_path, &[]).await;

    let create_path = format!("/v4.0.0/libpod/secrets/create?name={name}");
    let status = podman_http("POST", &create_path, data)
        .await
        .context("podman socket: secret create")?;

    if status != 200 && status != 201 {
        anyhow::bail!("podman socket: secret create returned HTTP {status}");
    }
    Ok(())
}

/// Delete a Podman secret via the socket API. Errors are logged but not propagated.
pub async fn secret_delete(name: &str) {
    let path = format!("/v4.0.0/libpod/secrets/{name}");
    if let Err(e) = podman_http("DELETE", &path, &[]).await {
        tracing::warn!(%name, "podman socket: secret delete failed: {e}");
    }
}
