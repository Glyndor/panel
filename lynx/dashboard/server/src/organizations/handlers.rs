use super::{
    CreateOrgRequest, CreateProjectRequest, DataPlaneTunnel, DeployContainerRequest,
    HorizontalScaleRequest, InviteMemberRequest, OrgMember, OrgWithMemberCount, Organization,
    Project, UpdateResourcesRequest,
};
use crate::{
    auth::middleware::AuthUser, crypto::cmd::sign_command, error::AppError, state::AppState,
};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use uuid::Uuid;

// --------------------------------------------------------------------------
// GET /organizations
// --------------------------------------------------------------------------

pub async fn list_orgs(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let orgs = sqlx::query_as!(
        OrgWithMemberCount,
        r#"
        SELECT o.id, o.name, o.slug, o.owner_id, o.created_at,
               COUNT(m.user_id) AS "member_count!"
        FROM organizations o
        JOIN organization_members m ON m.organization_id = o.id
        WHERE m.user_id = $1
        GROUP BY o.id
        ORDER BY o.created_at ASC
        "#,
        user.user_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(orgs))
}

// --------------------------------------------------------------------------
// POST /organizations
// --------------------------------------------------------------------------

pub async fn create_org(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateOrgRequest>,
) -> Result<impl IntoResponse, AppError> {
    let slug = req.slug.to_lowercase();

    if !slug.chars().all(|c| c.is_alphanumeric() || c == '-') || slug.is_empty() {
        return Err(AppError::Validation(
            "slug: only lowercase letters, numbers, and hyphens".into(),
        ));
    }

    let org_id = Uuid::now_v7();

    let org = sqlx::query_as!(
        Organization,
        r#"
        WITH new_org AS (
            INSERT INTO organizations (id, name, slug, owner_id)
            VALUES ($1, $2, $3, $4)
            RETURNING *
        ),
        _ AS (
            INSERT INTO organization_members (organization_id, user_id, role)
            VALUES ($1, $4, 'owner')
        )
        SELECT id, name, slug, owner_id, created_at FROM new_org
        "#,
        org_id,
        req.name,
        slug,
        user.user_id,
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("organizations_slug_key") {
                return AppError::Conflict("slug already taken");
            }
        }
        AppError::Internal(e.into())
    })?;

    Ok((StatusCode::CREATED, Json(org)))
}

// --------------------------------------------------------------------------
// GET /organizations/:id
// --------------------------------------------------------------------------

pub async fn get_org(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let org = sqlx::query_as!(
        Organization,
        r#"
        SELECT o.id, o.name, o.slug, o.owner_id, o.created_at
        FROM organizations o
        JOIN organization_members m ON m.organization_id = o.id
        WHERE o.id = $1 AND m.user_id = $2
        "#,
        id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(org))
}

// --------------------------------------------------------------------------
// DELETE /organizations/:id
// --------------------------------------------------------------------------

pub async fn delete_org(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query!(
        "DELETE FROM organizations WHERE id = $1 AND owner_id = $2",
        id,
        user.user_id
    )
    .execute(&state.db)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// GET /organizations/:id/members — caller must be a member
// --------------------------------------------------------------------------

pub async fn list_members(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(org_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Verify caller is a member
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

    let members = sqlx::query_as!(
        OrgMember,
        r#"
        SELECT m.user_id, u.username, m.role, m.joined_at
        FROM organization_members m
        JOIN users u ON u.id = m.user_id
        WHERE m.organization_id = $1
        ORDER BY m.joined_at ASC
        "#,
        org_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(members))
}

// --------------------------------------------------------------------------
// POST /organizations/:id/members — only owner/admin can invite
// --------------------------------------------------------------------------

pub async fn invite_member(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(org_id): Path<Uuid>,
    Json(req): Json<InviteMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    let caller_role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if caller_role != "owner" && caller_role != "admin" {
        return Err(AppError::Forbidden);
    }

    let role = req.role.unwrap_or_else(|| "member".to_string());
    if !["owner", "admin", "member", "viewer"].contains(&role.as_str()) {
        return Err(AppError::Validation(
            "role: must be owner, admin, member, or viewer".into(),
        ));
    }

    let invitee_id = sqlx::query_scalar!(
        "SELECT id FROM users WHERE username = $1",
        req.username.to_lowercase()
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Validation("user not found".into()))?;

    sqlx::query!(
        r#"
        INSERT INTO organization_members (organization_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (organization_id, user_id) DO NOTHING
        "#,
        org_id,
        invitee_id,
        role
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// DELETE /organizations/:id/members/:user_id — owner/admin; owner can't be removed
// --------------------------------------------------------------------------

pub async fn remove_member(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((org_id, target_user_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let caller_role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if caller_role != "owner" && caller_role != "admin" {
        return Err(AppError::Forbidden);
    }

    // Prevent removing the org owner
    let target_role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        target_user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if target_role == "owner" {
        return Err(AppError::Validation(
            "cannot remove the organization owner".into(),
        ));
    }

    sqlx::query!(
        "DELETE FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        target_user_id
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// GET /organizations/:id/projects — list projects caller can see (member of org)
// --------------------------------------------------------------------------

pub async fn list_projects(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(org_id): Path<Uuid>,
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

    let projects = sqlx::query_as!(
        Project,
        "SELECT id, organization_id, agent_id, name, slug, created_at FROM projects WHERE organization_id = $1 ORDER BY created_at ASC",
        org_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(projects))
}

// --------------------------------------------------------------------------
// GET /organizations/:id/projects/:proj_id
// --------------------------------------------------------------------------

pub async fn get_project(
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

    let project = sqlx::query_as!(
        Project,
        "SELECT id, organization_id, agent_id, name, slug, created_at FROM projects WHERE id = $1 AND organization_id = $2",
        proj_id,
        org_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(project))
}

// --------------------------------------------------------------------------
// POST /organizations/:id/projects
// --------------------------------------------------------------------------

pub async fn create_project(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(org_id): Path<Uuid>,
    Json(req): Json<CreateProjectRequest>,
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

    let slug = req.slug.to_lowercase();
    if !slug.chars().all(|c| c.is_alphanumeric() || c == '-') || slug.is_empty() {
        return Err(AppError::Validation(
            "slug: only lowercase letters, numbers, and hyphens".into(),
        ));
    }

    // Verify agent exists
    let agent_exists = sqlx::query_scalar!("SELECT 1 FROM agents WHERE id = $1", req.agent_id)
        .fetch_optional(&state.db)
        .await?
        .is_some();

    if !agent_exists {
        return Err(AppError::Validation("agent not found".into()));
    }

    let project = sqlx::query_as!(
        Project,
        r#"
        INSERT INTO projects (id, organization_id, agent_id, name, slug)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, organization_id, agent_id, name, slug, created_at
        "#,
        Uuid::now_v7(),
        org_id,
        req.agent_id,
        req.name,
        slug,
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("projects_organization_id_slug_key") {
                return AppError::Conflict("slug already taken in this organization");
            }
        }
        AppError::Internal(e.into())
    })?;

    Ok((StatusCode::CREATED, Json(project)))
}

// --------------------------------------------------------------------------
// PUT /organizations/:id/projects/:proj_id/resources
// Signs and relays a container.update command to the target agent.
// --------------------------------------------------------------------------

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

    let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
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

// --------------------------------------------------------------------------
// Helper: resolve project's agent and relay signed command
// --------------------------------------------------------------------------

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

    let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
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

// --------------------------------------------------------------------------
// POST /organizations/:id/projects/:proj_id/containers — deploy
// --------------------------------------------------------------------------

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

// --------------------------------------------------------------------------
// GET /organizations/:id/projects/:proj_id/containers — list via agent
// --------------------------------------------------------------------------

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

// --------------------------------------------------------------------------
// POST /organizations/:id/projects/:proj_id/containers/:name/:action
// action: start | stop | restart | remove
// --------------------------------------------------------------------------

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

// --------------------------------------------------------------------------
// GET /organizations/:id/projects/:proj_id/scale/horizontal
// List data-plane tunnels for this project
// --------------------------------------------------------------------------

pub async fn list_horizontal_scale(
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

    // Verify project belongs to this org
    sqlx::query_scalar!(
        "SELECT 1 FROM projects WHERE id = $1 AND organization_id = $2",
        proj_id,
        org_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let tunnels = sqlx::query_as!(
        DataPlaneTunnel,
        r#"SELECT id, project_id, agent_a_id, agent_b_id, agent_a_wg_ip, agent_b_wg_ip,
                  wg_port, replica_count, status, created_at
           FROM data_plane_tunnels
           WHERE project_id = $1 AND status != 'torn_down'
           ORDER BY created_at ASC"#,
        proj_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(tunnels))
}

// --------------------------------------------------------------------------
// POST /organizations/:id/projects/:proj_id/scale/horizontal
// Establish data-plane WireGuard tunnel and deploy replicas on Agent-B.
// --------------------------------------------------------------------------

pub async fn horizontal_scale(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((org_id, proj_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<HorizontalScaleRequest>,
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

    // Get project's primary agent (Agent-A)
    let project = sqlx::query!(
        "SELECT agent_id FROM projects WHERE id = $1 AND organization_id = $2",
        proj_id,
        org_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let agent_a_id = project.agent_id;
    let agent_b_id = req.target_agent_id;

    if agent_a_id == agent_b_id {
        return Err(AppError::Validation(
            "target agent must differ from project's primary agent".into(),
        ));
    }

    // Verify both agents exist and are online
    let agent_a = sqlx::query!(
        "SELECT wg_ip, api_port, status FROM agents WHERE id = $1",
        agent_a_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let agent_b = sqlx::query!(
        "SELECT wg_ip, api_port, status FROM agents WHERE id = $1",
        agent_b_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if agent_a.status != "online" || agent_b.status != "online" {
        return Err(AppError::AgentUnavailable);
    }

    let wg_port = req.wg_port.unwrap_or(51821) as i32;

    // Allocate data-plane WireGuard IPs (simple scheme: 10.200.x.y per tunnel)
    let tunnel_count = sqlx::query_scalar!("SELECT COUNT(*) FROM data_plane_tunnels")
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);

    let subnet_idx = (tunnel_count % 254) + 1;
    let agent_a_dp_ip = format!("10.200.{}.1", subnet_idx);
    let agent_b_dp_ip = format!("10.200.{}.2", subnet_idx);

    // Generate keypairs and PSK via wg command
    let gen_key = |label: &str| -> Result<String, AppError> {
        let out = std::process::Command::new("wg")
            .arg("genkey")
            .output()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("wg genkey ({label}): {e}")))?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let pub_key = |priv_key: &str| -> Result<String, AppError> {
        use std::io::Write;
        let mut child = std::process::Command::new("wg")
            .arg("pubkey")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("wg pubkey: {e}")))?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(priv_key.as_bytes())
            .ok();
        let out = child
            .wait_with_output()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let gen_psk = || -> Result<String, AppError> {
        let out = std::process::Command::new("wg")
            .arg("genpsk")
            .output()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("wg genpsk: {e}")))?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };

    let privkey_a = gen_key("agent_a")?;
    let pubkey_a = pub_key(&privkey_a)?;
    let privkey_b = gen_key("agent_b")?;
    let pubkey_b = pub_key(&privkey_b)?;
    let psk = gen_psk()?;

    // Create tunnel record (pending)
    let tunnel_id = Uuid::now_v7();
    sqlx::query!(
        r#"INSERT INTO data_plane_tunnels
           (id, project_id, agent_a_id, agent_b_id,
            agent_a_pubkey, agent_b_pubkey, agent_a_wg_ip, agent_b_wg_ip,
            wg_port, replica_count, status)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'pending')"#,
        tunnel_id,
        proj_id,
        agent_a_id,
        agent_b_id,
        pubkey_a,
        pubkey_b,
        agent_a_dp_ip,
        agent_b_dp_ip,
        wg_port,
        req.replica_count as i32,
    )
    .execute(&state.db)
    .await?;

    let tok = &*state.config.internal_token;
    let http = reqwest::Client::new();

    // Command Agent-A: setup data-plane tunnel (as initiator)
    let setup_a = json!({
        "type": "wg.data_plane.setup",
        "tunnel_id": tunnel_id.to_string(),
        "role": "initiator",
        "private_key": privkey_a,
        "peer_pubkey": pubkey_b,
        "psk": psk,
        "local_ip": format!("{}/30", agent_a_dp_ip),
        "peer_endpoint": format!("{}:{}", agent_b.wg_ip, wg_port),
        "wg_port": wg_port,
    });
    let signed_a = sign_command(&state.config, agent_a_id, user.user_id, "write", &setup_a)?;
    let url_a = format!("http://{}:{}/cmd", agent_a.wg_ip, agent_a.api_port);
    http.post(&url_a)
        .header("Authorization", format!("Bearer {tok}"))
        .json(&signed_a)
        .send()
        .await
        .map_err(|_| AppError::BadGateway)?;

    // Command Agent-B: setup data-plane tunnel (as responder) + deploy replicas
    let setup_b = json!({
        "type": "wg.data_plane.setup",
        "tunnel_id": tunnel_id.to_string(),
        "role": "responder",
        "private_key": privkey_b,
        "peer_pubkey": pubkey_a,
        "psk": psk,
        "local_ip": format!("{}/30", agent_b_dp_ip),
        "peer_endpoint": format!("{}:{}", agent_a.wg_ip, wg_port),
        "wg_port": wg_port,
    });
    let signed_b = sign_command(&state.config, agent_b_id, user.user_id, "write", &setup_b)?;
    let url_b = format!("http://{}:{}/cmd", agent_b.wg_ip, agent_b.api_port);
    http.post(&url_b)
        .header("Authorization", format!("Bearer {tok}"))
        .json(&signed_b)
        .send()
        .await
        .map_err(|_| AppError::BadGateway)?;

    // Deploy replicas on Agent-B
    for i in 0..req.replica_count {
        let replica_cmd = json!({
            "type": "container.deploy",
            "tenant_id": org_id.to_string(),
            "name": format!("{}-replica-{}", proj_id.to_string().split('-').next().unwrap_or("r"), i),
            "image": req.image,
            "ports": [],
            "env": [],
        });
        let signed_replica = sign_command(
            &state.config,
            agent_b_id,
            user.user_id,
            "write",
            &replica_cmd,
        )?;
        http.post(&url_b)
            .header("Authorization", format!("Bearer {tok}"))
            .json(&signed_replica)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|_| AppError::BadGateway)?;
    }

    // Mark tunnel active
    sqlx::query!(
        "UPDATE data_plane_tunnels SET status='active', updated_at=NOW() WHERE id=$1",
        tunnel_id
    )
    .execute(&state.db)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "tunnel_id": tunnel_id,
            "agent_a_ip": agent_a_dp_ip,
            "agent_b_ip": agent_b_dp_ip,
            "replicas": req.replica_count,
        })),
    ))
}

// --------------------------------------------------------------------------
// DELETE /organizations/:id/projects/:proj_id/scale/horizontal/:tunnel_id
// Tear down a data-plane tunnel and remove replicas on Agent-B.
// --------------------------------------------------------------------------

pub async fn teardown_horizontal_scale(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((org_id, proj_id, tunnel_id)): Path<(Uuid, Uuid, Uuid)>,
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

    let tunnel = sqlx::query!(
        "SELECT agent_a_id, agent_b_id, replica_count FROM data_plane_tunnels WHERE id=$1 AND project_id=$2",
        tunnel_id,
        proj_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let tok = &*state.config.internal_token;
    let http = reqwest::Client::new();

    // Command each agent to tear down the data-plane interface
    for agent_id in [tunnel.agent_a_id, tunnel.agent_b_id] {
        let agent = sqlx::query!(
            "SELECT wg_ip, api_port, status FROM agents WHERE id=$1",
            agent_id
        )
        .fetch_optional(&state.db)
        .await?;

        if let Some(a) = agent {
            if a.status == "online" {
                let teardown = json!({
                    "type": "wg.data_plane.teardown",
                    "tunnel_id": tunnel_id.to_string(),
                });
                let signed =
                    sign_command(&state.config, agent_id, user.user_id, "write", &teardown)?;
                let _ = http
                    .post(&format!("http://{}:{}/cmd", a.wg_ip, a.api_port))
                    .header("Authorization", format!("Bearer {tok}"))
                    .json(&signed)
                    .timeout(std::time::Duration::from_secs(15))
                    .send()
                    .await;
            }
        }
    }

    sqlx::query!(
        "UPDATE data_plane_tunnels SET status='torn_down', updated_at=NOW() WHERE id=$1",
        tunnel_id
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
