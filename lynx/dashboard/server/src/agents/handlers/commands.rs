use crate::{crypto::cmd::sign_command, error::AppError, state::AppState};
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use uuid::Uuid;

pub async fn relay_heartbeat(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = sqlx::query!(
        "SELECT wg_ip::text AS wg_ip, api_port FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let url = format!("http://{}:{}/heartbeat", agent.wg_ip, agent.api_port);

    let token = &*state.config.internal_token;
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            sqlx::query!(
                "UPDATE agents SET status='online', last_heartbeat=NOW() WHERE id=$1",
                id
            )
            .execute(&state.db)
            .await?;
            Ok(axum::http::StatusCode::NO_CONTENT)
        }
        Ok(r) => {
            let status_code = r.status().as_u16();
            let is_lockdown = status_code == 423;
            let new_status = if is_lockdown { "lockdown" } else { "offline" };
            sqlx::query!(
                "UPDATE agents SET status=$1, last_heartbeat=NOW() WHERE id=$2",
                new_status,
                id
            )
            .execute(&state.db)
            .await?;
            Err(AppError::BadGateway)
        }
        Err(_) => {
            sqlx::query!("UPDATE agents SET status='offline' WHERE id=$1", id)
                .execute(&state.db)
                .await?;
            Err(AppError::BadGateway)
        }
    }
}

pub async fn send_command(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse, AppError> {
    let agent = sqlx::query!(
        "SELECT wg_ip, api_port, status FROM agents WHERE id = $1",
        id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if agent.status == "lockdown" || agent.status == "offline" {
        return Err(AppError::AgentUnavailable);
    }

    let cmd_user_id = payload
        .get("user_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .ok_or(AppError::BadRequest("user_id required in command"))?;

    let permission = payload
        .get("permission")
        .and_then(|v| v.as_str())
        .unwrap_or("read")
        .to_string();

    let signed = sign_command(&state.config, id, cmd_user_id, &permission, &payload)?;

    let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);

    let token = &*state.config.internal_token;
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .json(&signed)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| AppError::BadGateway)?;

    let status = resp.status();
    let body: Value = resp.json().await.unwrap_or(json!({}));

    Ok((
        axum::http::StatusCode::from_u16(status.as_u16())
            .unwrap_or(axum::http::StatusCode::BAD_GATEWAY),
        Json(body),
    ))
}
