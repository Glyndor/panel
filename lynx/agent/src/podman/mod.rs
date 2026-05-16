use anyhow::{Context, Result};
use std::process::Command;

/// Tenant isolation: each org gets a `lynx-tenant-{id}` system user
/// with dedicated subuid/subgid range for rootless Podman.
pub fn ensure_tenant_user(tenant_id: &str) -> Result<()> {
    let username = format!("lynx-tenant-{tenant_id}");

    // Check if user already exists
    let exists = Command::new("id")
        .arg(&username)
        .status()
        .context("run id")?
        .success();

    if !exists {
        // Create system user (no login shell, no home)
        let status = Command::new("useradd")
            .args([
                "--system",
                "--no-create-home",
                "--shell",
                "/usr/sbin/nologin",
                &username,
            ])
            .status()
            .context("useradd")?;

        if !status.success() {
            anyhow::bail!("useradd failed for {username}");
        }

        // Assign subuid/subgid range (65536 IDs per tenant)
        add_subid_range(&username)?;
    }

    Ok(())
}

/// Run a Podman command as a specific tenant user via `runuser`.
pub fn podman_as_tenant(tenant_id: &str, args: &[&str]) -> Result<std::process::Output> {
    let username = format!("lynx-tenant-{tenant_id}");
    Command::new("runuser")
        .args(["-l", &username, "-c"])
        .arg(format!("podman {}", args.join(" ")))
        .output()
        .context("runuser podman")
}

/// Create an isolated Podman network for an organization.
pub fn ensure_org_network(tenant_id: &str, network_name: &str) -> Result<()> {
    let out = podman_as_tenant(
        tenant_id,
        &["network", "exists", network_name],
    )?;

    if !out.status.success() {
        let out = podman_as_tenant(
            tenant_id,
            &["network", "create", "--internal", network_name],
        )?;
        if !out.status.success() {
            anyhow::bail!(
                "podman network create failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    Ok(())
}

/// List running containers for a tenant.
pub fn list_containers(tenant_id: &str) -> Result<Vec<ContainerInfo>> {
    let out = podman_as_tenant(
        tenant_id,
        &["ps", "--format", "json", "--no-trunc"],
    )?;

    if !out.status.success() {
        anyhow::bail!(
            "podman ps failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let containers: Vec<serde_json::Value> =
        serde_json::from_slice(&out.stdout).context("parse podman ps JSON")?;

    Ok(containers
        .into_iter()
        .filter_map(|c| {
            Some(ContainerInfo {
                id: c["Id"].as_str()?.to_string(),
                name: c["Names"]
                    .as_array()?
                    .first()?
                    .as_str()?
                    .to_string(),
                status: c["Status"].as_str()?.to_string(),
                image: c["Image"].as_str()?.to_string(),
            })
        })
        .collect())
}

#[derive(Debug, serde::Serialize)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub image: String,
}

fn add_subid_range(username: &str) -> Result<()> {
    // Each tenant gets a 65536-ID range.
    // usermod --add-subuids / --add-subgids auto-assigns from available pool.
    for flag in ["--add-subuids", "--add-subgids"] {
        let status = Command::new("usermod")
            .args([flag, "65536", username])
            .status()
            .context("usermod subid")?;
        if !status.success() {
            anyhow::bail!("usermod {flag} failed for {username}");
        }
    }
    Ok(())
}
