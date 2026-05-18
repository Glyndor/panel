use crate::{
    auth::{PermissionLevel, VerifiedCommand},
    error::AgentError,
    podman,
};
use serde_json::{json, Value};

pub fn handle_container_list(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let containers = podman::list_containers(&tenant_id)?;
    Ok(json!({ "containers": containers }))
}

pub fn handle_tenant_ensure(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "tenant.ensure requires write permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    podman::ensure_tenant_user(&tenant_id)?;
    Ok(json!({ "ok": true, "tenant_id": tenant_id }))
}

pub fn handle_container_deploy(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.deploy requires write permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let project_id = require_str(&cmd.command, "project_id")?;
    let compose_yaml = require_str(&cmd.command, "compose_yaml")?;

    podman::compose_deploy(podman::DeployOptions {
        tenant_id: &tenant_id,
        project_id: &project_id,
        compose_yaml: &compose_yaml,
    })?;

    Ok(json!({ "ok": true }))
}

pub fn handle_container_start(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.start requires write permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    podman::container_start(&tenant_id, &name)?;
    Ok(json!({ "ok": true }))
}

pub fn handle_container_stop(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.stop requires write permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    podman::container_stop(&tenant_id, &name)?;
    Ok(json!({ "ok": true }))
}

pub fn handle_container_remove(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission != PermissionLevel::Destructive {
        return Err(AgentError::Forbidden(
            "container.remove requires destructive permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
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
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
    podman::container_restart(&tenant_id, &name)?;
    Ok(json!({ "ok": true }))
}

pub fn handle_container_update(cmd: &VerifiedCommand) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "container.update requires write permission",
        ));
    }
    let tenant_id = require_str(&cmd.command, "tenant_id")?;
    let name = require_str(&cmd.command, "name")?;
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
