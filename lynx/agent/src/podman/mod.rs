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

// ---------------------------------------------------------------------------
// Container lifecycle operations (all scoped to a tenant user)
// ---------------------------------------------------------------------------

pub struct DeployOptions<'a> {
    pub tenant_id: &'a str,
    pub project_id: &'a str,
    pub compose_yaml: &'a str,
}

/// Write compose file to stable project dir, then run `podman compose up -d`.
pub fn compose_deploy(opts: DeployOptions<'_>) -> Result<()> {
    let project_dir = project_dir(opts.tenant_id, opts.project_id);
    std::fs::create_dir_all(&project_dir)
        .with_context(|| format!("create project dir {project_dir}"))?;

    let compose_path = format!("{project_dir}/compose.yml");
    std::fs::write(&compose_path, opts.compose_yaml)
        .context("write compose.yml")?;

    // Chown the project dir tree to the tenant user so they can read it.
    let uid = tenant_uid(opts.tenant_id)?;
    Command::new("chown")
        .args(["-R", &format!("{uid}:{uid}"), &project_dir])
        .status()
        .context("chown project dir")?;

    let out = run_as_tenant(opts.tenant_id, &["compose", "-f", &compose_path, "up", "-d"])?;
    if !out.status.success() {
        anyhow::bail!(
            "podman compose up failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Tear down a project's compose stack.
pub fn compose_down(tenant_id: &str, project_id: &str) -> Result<()> {
    let compose_path = format!("{}/compose.yml", project_dir(tenant_id, project_id));
    if !std::path::Path::new(&compose_path).exists() {
        return Ok(());
    }
    let out = run_as_tenant(tenant_id, &["compose", "-f", &compose_path, "down", "--remove-orphans"])?;
    if !out.status.success() {
        anyhow::bail!(
            "podman compose down failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

pub fn container_start(tenant_id: &str, name: &str) -> Result<()> {
    let out = run_as_tenant(tenant_id, &["start", name])?;
    if !out.status.success() {
        anyhow::bail!(
            "podman start failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

pub fn container_stop(tenant_id: &str, name: &str) -> Result<()> {
    let out = run_as_tenant(tenant_id, &["stop", "--time", "10", name])?;
    if !out.status.success() {
        anyhow::bail!(
            "podman stop failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

pub fn container_remove(tenant_id: &str, name: &str, force: bool) -> Result<()> {
    let mut args = vec!["rm"];
    if force {
        args.push("--force");
    }
    args.push(name);
    let out = run_as_tenant(tenant_id, &args)?;
    if !out.status.success() {
        anyhow::bail!(
            "podman rm failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

pub fn container_restart(tenant_id: &str, name: &str) -> Result<()> {
    let out = run_as_tenant(tenant_id, &["restart", name])?;
    if !out.status.success() {
        anyhow::bail!(
            "podman restart failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Update resource limits on a running container (vertical scaling).
pub fn container_update(
    tenant_id: &str,
    name: &str,
    cpus: Option<f64>,
    memory_mb: Option<u64>,
) -> Result<()> {
    let mut args = vec!["update".to_string()];
    if let Some(c) = cpus {
        args.push(format!("--cpus={c}"));
    }
    if let Some(m) = memory_mb {
        args.push(format!("--memory={m}m"));
    }
    if args.len() == 1 {
        return Ok(());
    }
    args.push(name.to_string());
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = run_as_tenant(tenant_id, &arg_refs)?;
    if !out.status.success() {
        anyhow::bail!(
            "podman update failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn project_dir(tenant_id: &str, project_id: &str) -> String {
    format!("/var/lib/lynx/projects/{tenant_id}/{project_id}")
}

fn tenant_uid(tenant_id: &str) -> Result<u32> {
    let username = format!("lynx-tenant-{tenant_id}");
    let out = Command::new("id")
        .args(["-u", &username])
        .output()
        .context("id -u")?;
    if !out.status.success() {
        anyhow::bail!("user {username} not found");
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .context("parse uid")
}

/// Run a Podman command as the tenant user via runuser.
fn run_as_tenant(tenant_id: &str, podman_args: &[&str]) -> Result<std::process::Output> {
    podman_as_tenant(tenant_id, podman_args)
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
