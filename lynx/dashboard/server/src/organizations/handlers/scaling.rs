use super::super::{DataPlaneTunnel, HorizontalScaleRequest};
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

    let tunnel_count = sqlx::query_scalar!("SELECT COUNT(*) FROM data_plane_tunnels")
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);

    let subnet_idx = (tunnel_count % 254) + 1;
    let agent_a_dp_ip = format!("10.200.{}.1", subnet_idx);
    let agent_b_dp_ip = format!("10.200.{}.2", subnet_idx);

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
                    .post(format!("http://{}:{}/cmd", a.wg_ip, a.api_port))
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
