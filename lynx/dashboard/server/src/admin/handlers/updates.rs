use crate::{auth::middleware::AuthUser, crypto::cmd, error::AppError, state::AppState};
use axum::{
    extract::{Extension, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const GITHUB_REPO: &str = "Jaro-c/Lynx";

#[derive(Debug, Serialize)]
pub struct UpdateCheckResponse {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TriggerUpdateRequest {
    pub version: String,
    #[serde(default = "default_channel")]
    pub channel: String,
    pub agent_id: Option<Uuid>,
}

fn default_channel() -> String {
    "stable".to_string()
}

pub async fn update_check(
    State(_state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let current = env!("CARGO_PKG_VERSION").to_string();

    let client = reqwest::Client::builder()
        .user_agent(format!("lynx-dashboard/{current}"))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    let api_url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let res = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    if !res.status().is_success() {
        return Err(AppError::BadGateway);
    }

    let body: serde_json::Value = res
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    let latest = body["tag_name"]
        .as_str()
        .unwrap_or(&current)
        .trim_start_matches('v')
        .to_string();

    let release_url = body["html_url"].as_str().map(|s| s.to_string());
    let update_available = latest != current && !latest.is_empty();

    Ok(Json(UpdateCheckResponse {
        current_version: current,
        latest_version: latest,
        update_available,
        release_url,
    }))
}

pub async fn trigger_update(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<TriggerUpdateRequest>,
) -> Result<impl IntoResponse, AppError> {
    let valid_channels = ["stable", "edge"];
    if !valid_channels.contains(&req.channel.as_str()) {
        return Err(AppError::BadRequest("invalid channel"));
    }

    struct AgentTarget {
        id: Uuid,
        wg_ip: String,
        api_port: i32,
    }

    let agents: Vec<AgentTarget> = if let Some(id) = req.agent_id {
        sqlx::query!(
            "SELECT id, wg_ip, api_port FROM agents WHERE id = $1 AND status = 'online'",
            id
        )
        .fetch_all(&state.db)
        .await?
        .into_iter()
        .map(|r| AgentTarget {
            id: r.id,
            wg_ip: r.wg_ip,
            api_port: r.api_port,
        })
        .collect()
    } else {
        sqlx::query!("SELECT id, wg_ip, api_port FROM agents WHERE status = 'online'")
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .map(|r| AgentTarget {
                id: r.id,
                wg_ip: r.wg_ip,
                api_port: r.api_port,
            })
            .collect()
    };

    let mut sent = 0usize;
    let mut failed = 0usize;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(anyhow::Error::from(e)))?;

    for agent in &agents {
        let download_url = format!(
            "https://github.com/{GITHUB_REPO}/releases/download/{version}/lynx-agent-linux-x86_64",
            version = req.version
        );
        let sig_url = format!(
            "https://github.com/{GITHUB_REPO}/releases/download/{version}/lynx-agent-linux-x86_64.sig",
            version = req.version
        );

        let command = serde_json::json!({
            "type": "update.self",
            "version": req.version,
            "download_url": download_url,
            "sig_url": sig_url,
        });

        let signed = cmd::sign_command(&state.config, agent.id, user.user_id, "write", &command)
            .map_err(AppError::Internal)?;

        let url = format!("https://{}:{}/cmd", agent.wg_ip, agent.api_port);

        let log_id = Uuid::now_v7();
        let send_result = client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", &*state.config.internal_token),
            )
            .json(&signed)
            .send()
            .await;

        let status = match send_result {
            Ok(r) if r.status().is_success() => {
                sent += 1;
                "success"
            }
            _ => {
                failed += 1;
                "failed"
            }
        };

        sqlx::query!(
            r#"
            INSERT INTO update_log (id, triggered_by, version, channel, scope, agent_id, status)
            VALUES ($1, $2, $3, $4, 'agent', $5, $6)
            "#,
            log_id,
            user.user_id,
            req.version,
            req.channel,
            agent.id,
            status,
        )
        .execute(&state.db)
        .await?;
    }

    tracing::info!(
        user_id = %user.user_id,
        version = %req.version,
        sent,
        failed,
        "update triggered"
    );

    if let Err(e) = super::rotation::rotate_expiring_certs(&state, 14).await {
        tracing::warn!("cert expiry check during update failed: {e}");
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "version": req.version,
        "agents_sent": sent,
        "agents_failed": failed,
    })))
}
