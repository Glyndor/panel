use anyhow::{Context, Result};
use std::process::Command;

/// Tenant isolation: each org gets a `lynx-tenant-{id}` system user
/// with dedicated subuid/subgid range for rootless Podman.
pub fn ensure_tenant_user(tenant_id: &str) -> Result<()> {
    let username = format!("lynx-tenant-{tenant_id}");
    let home_dir = format!("/var/lib/lynx/orgs/{tenant_id}");

    // Check if user already exists
    let exists = Command::new("id")
        .arg(&username)
        .status()
        .context("run id")?
        .success();

    if !exists {
        // Parent dir must exist before useradd --create-home runs.
        std::fs::create_dir_all("/var/lib/lynx/orgs").context("create /var/lib/lynx/orgs")?;

        // Create system user with a real home dir so rootless Podman can store its
        // images/containers under ~/.local/share/containers/ and find its socket.
        let status = Command::new("useradd")
            .args([
                "--system",
                "--create-home",
                "--home-dir",
                &home_dir,
                "--shell",
                "/usr/sbin/nologin",
                &username,
            ])
            .status()
            .context("useradd")?;

        if !status.success() {
            anyhow::bail!("useradd failed for {username}");
        }

        // Assign subuid/subgid range (65536 IDs per tenant).
        add_subid_range(&username)?;

        // Enable lingering and start the user session immediately so that
        // XDG_RUNTIME_DIR (/run/user/{uid}) is created right away.
        let uid = tenant_uid(tenant_id)?;
        let _ = Command::new("loginctl")
            .args(["enable-linger", &username])
            .status();
        let _ = Command::new("systemctl")
            .args(["start", &format!("user@{uid}.service")])
            .status();
    }

    Ok(())
}

/// Run a Podman command as a specific tenant user via `runuser`.
///
/// Uses `-u` (not `-l -c`) so each argument is passed directly to the OS without
/// shell interpretation — prevents command injection through container/image names.
/// Sets HOME and XDG_RUNTIME_DIR so rootless Podman finds its storage and socket.
pub fn podman_as_tenant(tenant_id: &str, args: &[&str]) -> Result<std::process::Output> {
    let username = format!("lynx-tenant-{tenant_id}");
    let uid = tenant_uid(tenant_id)?;
    // Start in / so the tenant user can always access the cwd (their home or
    // a restricted directory might not be world-readable).
    Command::new("runuser")
        .args(["-u", &username, "--", "podman"])
        .args(args)
        .env("HOME", format!("/var/lib/lynx/orgs/{tenant_id}"))
        .env("XDG_RUNTIME_DIR", format!("/run/user/{uid}"))
        .current_dir("/")
        .output()
        .context("runuser podman")
}

/// Create an isolated Podman network for an organization.
#[allow(dead_code)]
pub fn ensure_org_network(tenant_id: &str, network_name: &str) -> Result<()> {
    let out = podman_as_tenant(tenant_id, &["network", "exists", network_name])?;

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
    let out = podman_as_tenant(tenant_id, &["ps", "--format", "json", "--no-trunc"])?;

    if !out.status.success() {
        anyhow::bail!("podman ps failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    let containers: Vec<serde_json::Value> =
        serde_json::from_slice(&out.stdout).context("parse podman ps JSON")?;

    Ok(containers
        .into_iter()
        .filter_map(|c| {
            Some(ContainerInfo {
                id: c["Id"].as_str()?.to_string(),
                name: c["Names"].as_array()?.first()?.as_str()?.to_string(),
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
/// Returns the compose.yml path so the caller can persist it for startup recovery.
pub fn compose_deploy(opts: DeployOptions<'_>) -> Result<String> {
    let project_dir = project_dir(opts.tenant_id, opts.project_id);
    std::fs::create_dir_all(&project_dir)
        .with_context(|| format!("create project dir {project_dir}"))?;

    let compose_path = format!("{project_dir}/compose.yml");
    std::fs::write(&compose_path, opts.compose_yaml).context("write compose.yml")?;

    // Chown the project dir tree to the tenant user so they can read it.
    let uid = tenant_uid(opts.tenant_id)?;
    Command::new("chown")
        .args(["-R", &format!("{uid}:{uid}"), &project_dir])
        .status()
        .context("chown project dir")?;

    let out = run_as_tenant(
        opts.tenant_id,
        &["compose", "-f", &compose_path, "up", "-d"],
    )?;
    if !out.status.success() {
        anyhow::bail!(
            "podman compose up failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(compose_path)
}

/// Start containers for an existing compose project without recreating running ones.
/// Used on agent startup to recover containers that should be running after a reboot.
pub fn compose_up_no_recreate(tenant_id: &str, compose_path: &str) -> Result<()> {
    if !std::path::Path::new(compose_path).exists() {
        anyhow::bail!("compose file not found: {compose_path}");
    }
    let out = run_as_tenant(
        tenant_id,
        &["compose", "-f", compose_path, "up", "-d", "--no-recreate"],
    )?;
    if !out.status.success() {
        anyhow::bail!(
            "podman compose up --no-recreate failed: {}",
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
    let out = run_as_tenant(
        tenant_id,
        &["compose", "-f", &compose_path, "down", "--remove-orphans"],
    )?;
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
        anyhow::bail!("podman rm failed: {}", String::from_utf8_lossy(&out.stderr));
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
    let start = next_subid_start().context("find next subid range")?;
    let end = start + 65535;
    let range = format!("{start}-{end}");

    for flag in ["--add-subuids", "--add-subgids"] {
        let status = Command::new("usermod")
            .args([flag, &range, username])
            .status()
            .context("usermod subid")?;
        if !status.success() {
            anyhow::bail!("usermod {flag} failed for {username}");
        }
    }
    Ok(())
}

/// Find the next available subid start across /etc/subuid and /etc/subgid.
/// Allocations start at 100,000 (standard) and each tenant takes 65,536 IDs.
fn next_subid_start() -> Result<u64> {
    const MIN_START: u64 = 100_000;
    // /etc/subuid format: username:start:count — find the highest occupied end
    let max_end = ["/etc/subuid", "/etc/subgid"]
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .flat_map(|content| {
            content
                .lines()
                .filter_map(|line| {
                    let mut parts = line.splitn(3, ':');
                    let _ = parts.next(); // username
                    let start: u64 = parts.next()?.parse().ok()?;
                    let count: u64 = parts.next()?.parse().ok()?;
                    Some(start + count)
                })
                .collect::<Vec<_>>()
        })
        .max();

    Ok(max_end.unwrap_or(MIN_START).max(MIN_START))
}
