use crate::{
    auth::{PermissionLevel, VerifiedCommand},
    error::AgentError,
    podman,
    state::AppState,
};
use serde_json::{json, Value};

pub fn handle_container_list(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    let tenant_id = require_valid_id(&cmd.command, "tenant_id")?;
    let containers = podman::list_containers(&tenant_id)?;
    Ok(json!({ "containers": containers }))
}

pub fn handle_tenant_ensure(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "tenant.ensure requires write permission",
        ));
    }
    let tenant_id = require_valid_id(&cmd.command, "tenant_id")?;
    podman::ensure_tenant_user(&tenant_id)?;
    Ok(json!({ "ok": true, "tenant_id": tenant_id }))
}

pub async fn handle_container_deploy(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.deploy requires write permission",
        ));
    }
    let tenant_id = require_valid_id(&cmd.command, "tenant_id")?;
    let project_id = require_valid_id(&cmd.command, "project_id")?;
    let compose_yaml = require_str(&cmd.command, "compose_yaml")?;

    let compose_path = podman::compose_deploy(podman::DeployOptions {
        tenant_id: &tenant_id,
        project_id: &project_id,
        compose_yaml: &compose_yaml,
    })?;

    // Persist desired state so agent can restart on reboot (safety net).
    sqlx::query(
        r#"
        INSERT INTO container_deployments (tenant_id, project_id, compose_path, desired)
        VALUES ($1, $2, $3, 'running')
        ON CONFLICT (tenant_id, project_id)
        DO UPDATE SET compose_path = EXCLUDED.compose_path,
                      desired      = 'running',
                      updated_at   = NOW()
        "#,
    )
    .bind(&tenant_id)
    .bind(&project_id)
    .bind(&compose_path)
    .execute(&state.db)
    .await
    .map_err(|e| AgentError::Internal(anyhow::anyhow!(e)))?;

    Ok(json!({ "ok": true }))
}

pub fn handle_container_start(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.start requires write permission",
        ));
    }
    let tenant_id = require_valid_id(&cmd.command, "tenant_id")?;
    let name = require_valid_id(&cmd.command, "name")?;
    podman::container_start(&tenant_id, &name)?;
    Ok(json!({ "ok": true }))
}

pub fn handle_container_stop(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.stop requires write permission",
        ));
    }
    let tenant_id = require_valid_id(&cmd.command, "tenant_id")?;
    let name = require_valid_id(&cmd.command, "name")?;
    podman::container_stop(&tenant_id, &name)?;
    Ok(json!({ "ok": true }))
}

pub async fn handle_container_down(
    state: &AppState,
    cmd: &VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission != PermissionLevel::Destructive {
        return Err(AgentError::Forbidden(
            "container.down requires destructive permission",
        ));
    }
    let tenant_id = require_valid_id(&cmd.command, "tenant_id")?;
    let project_id = require_valid_id(&cmd.command, "project_id")?;

    podman::compose_down(&tenant_id, &project_id)?;

    // Mark desired state as stopped so agent won't restart on reboot.
    sqlx::query(
        "UPDATE container_deployments SET desired = 'stopped', updated_at = NOW() WHERE tenant_id = $1 AND project_id = $2",
    )
    .bind(&tenant_id)
    .bind(&project_id)
    .execute(&state.db)
    .await
    .map_err(|e| AgentError::Internal(anyhow::anyhow!(e)))?;

    Ok(json!({ "ok": true }))
}

pub fn handle_container_remove(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission != PermissionLevel::Destructive {
        return Err(AgentError::Forbidden(
            "container.remove requires destructive permission",
        ));
    }
    let tenant_id = require_valid_id(&cmd.command, "tenant_id")?;
    let name = require_valid_id(&cmd.command, "name")?;
    let force = cmd
        .command
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    podman::container_remove(&tenant_id, &name, force)?;
    Ok(json!({ "ok": true }))
}

pub fn handle_container_restart(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.restart requires write permission",
        ));
    }
    let tenant_id = require_valid_id(&cmd.command, "tenant_id")?;
    let name = require_valid_id(&cmd.command, "name")?;
    podman::container_restart(&tenant_id, &name)?;
    Ok(json!({ "ok": true }))
}

pub fn handle_container_update(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.update requires write permission",
        ));
    }
    let tenant_id = require_valid_id(&cmd.command, "tenant_id")?;
    let name = require_valid_id(&cmd.command, "name")?;
    let cpus = cmd.command.get("cpus").and_then(|v| v.as_f64());
    let memory_mb = cmd.command.get("memory_mb").and_then(|v| v.as_u64());
    podman::container_update(&tenant_id, &name, cpus, memory_mb)?;
    Ok(json!({ "ok": true }))
}

pub fn require_str(
    cmd: &serde_json::Value,
    key: &'static str,
) -> std::result::Result<String, AgentError> {
    cmd.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or(AgentError::BadRequest(key))
}

/// Validates a resource identifier (tenant_id, project_id, container name).
/// Allows alphanumeric, hyphens, and underscores only — no path separators or
/// shell metacharacters. Max 128 characters.
pub fn validate_id(value: &str, key: &'static str) -> std::result::Result<(), AgentError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AgentError::BadRequest(key));
    }
    Ok(())
}

pub fn require_valid_id(
    cmd: &serde_json::Value,
    key: &'static str,
) -> std::result::Result<String, AgentError> {
    let val = require_str(cmd, key)?;
    validate_id(&val, key)?;
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_id_accepts_valid() {
        assert!(validate_id("test-org-001", "k").is_ok());
        assert!(validate_id("my_project", "k").is_ok());
        assert!(validate_id("abc123", "k").is_ok());
        assert!(validate_id("a", "k").is_ok());
        assert!(validate_id(&"x".repeat(128), "k").is_ok());
    }

    #[test]
    fn validate_id_rejects_path_traversal() {
        assert!(validate_id("../../etc/passwd", "k").is_err());
        assert!(validate_id("../secret", "k").is_err());
        assert!(validate_id("org/subdir", "k").is_err());
        assert!(validate_id("org\\evil", "k").is_err());
    }

    #[test]
    fn validate_id_rejects_shell_metacharacters() {
        assert!(validate_id("org; rm -rf /", "k").is_err());
        assert!(validate_id("org$(id)", "k").is_err());
        assert!(validate_id("org`id`", "k").is_err());
        assert!(validate_id("org | cat", "k").is_err());
        assert!(validate_id("org\nrm", "k").is_err());
        assert!(validate_id("org\x00evil", "k").is_err());
    }

    #[test]
    fn validate_id_rejects_empty_and_too_long() {
        assert!(validate_id("", "k").is_err());
        assert!(validate_id(&"x".repeat(129), "k").is_err());
    }
}
