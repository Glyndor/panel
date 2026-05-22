use super::super::{DeployContainerRequest, UpdateResourcesRequest};
use crate::{
    auth::middleware::AuthUser, crypto::cmd::sign_command, error::AppError, state::AppState,
};
use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use uuid::Uuid;

async fn relay_project_command(
    state: &AppState,
    org_id: Uuid,
    proj_id: Uuid,
    user_id: Uuid,
    permission: &str,
    command: serde_json::Value,
) -> Result<impl IntoResponse, AppError> {
    let project = sqlx::query!(
        "SELECT agent_id FROM projects WHERE id = $1 AND organization_id = $2",
        proj_id,
        org_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let agent = sqlx::query!(
        "SELECT wg_ip, api_port, status FROM agents WHERE id = $1",
        project.agent_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if agent.status == "lockdown" || agent.status == "offline" {
        return Err(AppError::AgentUnavailable);
    }

    let signed = sign_command(
        &state.config,
        project.agent_id,
        user_id,
        permission,
        &command,
    )?;

    let url = format!("https://{}:{}/cmd", agent.wg_ip, agent.api_port);
    let tok = &*state.config.internal_token;
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {tok}"))
        .json(&signed)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| AppError::BadGateway)?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(json!({}));

    Ok((
        axum::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(axum::http::StatusCode::BAD_GATEWAY),
        Json(body),
    ))
}

pub async fn update_container_resources(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((org_id, proj_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateResourcesRequest>,
) -> Result<impl IntoResponse, AppError> {
    let role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if role == "viewer" {
        return Err(AppError::Forbidden);
    }

    let project = sqlx::query!(
        "SELECT agent_id FROM projects WHERE id = $1 AND organization_id = $2",
        proj_id,
        org_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let agent = sqlx::query!(
        "SELECT wg_ip, api_port, status FROM agents WHERE id = $1",
        project.agent_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if agent.status == "lockdown" || agent.status == "offline" {
        return Err(AppError::AgentUnavailable);
    }

    let command = json!({
        "type": "container.update",
        "tenant_id": org_id.to_string(),
        "name": req.container_name,
        "cpus": req.cpus,
        "memory_mb": req.memory_mb,
    });

    let signed = sign_command(
        &state.config,
        project.agent_id,
        user.user_id,
        "write",
        &command,
    )?;

    let url = format!("https://{}:{}/cmd", agent.wg_ip, agent.api_port);
    let tok = &*state.config.internal_token;
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {tok}"))
        .json(&signed)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| AppError::BadGateway)?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap_or(json!({}));

    Ok((
        axum::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(axum::http::StatusCode::BAD_GATEWAY),
        Json(body),
    ))
}

pub async fn deploy_container(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((org_id, proj_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<DeployContainerRequest>,
) -> Result<impl IntoResponse, AppError> {
    let role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if role == "viewer" {
        return Err(AppError::Forbidden);
    }

    let command = json!({
        "type": "container.deploy",
        "tenant_id": org_id.to_string(),
        "name": req.name,
        "image": req.image,
        "ports": req.ports.unwrap_or_default(),
        "env": req.env.unwrap_or_default(),
        "cpus": req.cpus,
        "memory_mb": req.memory_mb,
    });

    relay_project_command(&state, org_id, proj_id, user.user_id, "write", command).await
}

pub async fn list_containers(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((org_id, proj_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .is_some();

    if !is_member {
        return Err(AppError::NotFound);
    }

    let command = json!({
        "type": "container.list",
        "tenant_id": org_id.to_string(),
    });

    relay_project_command(&state, org_id, proj_id, user.user_id, "read", command).await
}

pub async fn container_action(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((org_id, proj_id, name, action)): Path<(Uuid, Uuid, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if role == "viewer" {
        return Err(AppError::Forbidden);
    }

    let cmd_type = match action.as_str() {
        "start" => "container.start",
        "stop" => "container.stop",
        "restart" => "container.restart",
        "remove" => "container.remove",
        _ => {
            return Err(AppError::BadRequest(
                "action must be start, stop, restart, or remove",
            ))
        }
    };

    let command = json!({
        "type": cmd_type,
        "tenant_id": org_id.to_string(),
        "name": name,
    });

    relay_project_command(&state, org_id, proj_id, user.user_id, "write", command).await
}
