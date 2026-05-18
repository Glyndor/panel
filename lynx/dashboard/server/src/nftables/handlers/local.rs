use crate::{
    agents::ws_hub,
    auth::middleware::AuthUser,
    crypto::cmd::sign_command,
    error::AppError,
    nftables::{rules_to_nft_chain, CreateRuleRequest, NftRule},
    state::AppState,
};
use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use uuid::Uuid;

pub async fn list_local_rules(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Verify agent exists.
    let exists = sqlx::query!("SELECT id FROM agents WHERE id = $1", agent_id)
        .fetch_optional(&state.db)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    let rules = sqlx::query_as!(
        NftRule,
        r#"
        SELECT id, scope, agent_id, kind, port, protocol,
               ip_list, ip_version, rate_per_min, description,
               priority, enabled, created_by, created_at, updated_at
        FROM nftables_rules
        WHERE scope = 'local' AND agent_id = $1
        ORDER BY priority ASC, created_at ASC
        "#,
        agent_id,
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rules))
}

pub async fn create_local_rule(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_rule_request(&req)?;

    let agent_exists = sqlx::query!("SELECT id FROM agents WHERE id = $1", agent_id)
        .fetch_optional(&state.db)
        .await?;
    if agent_exists.is_none() {
        return Err(AppError::NotFound);
    }

    let id = Uuid::now_v7();
    let ip_list = req.ip_list.unwrap_or_default();
    let ip_version = req.ip_version.unwrap_or_else(|| "both".into());
    let priority = req.priority.unwrap_or(0);

    let rule = sqlx::query_as!(
        NftRule,
        r#"
        INSERT INTO nftables_rules
            (id, scope, agent_id, kind, port, protocol, ip_list, ip_version,
             rate_per_min, description, priority, created_by)
        VALUES ($1, 'local', $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id, scope, agent_id, kind, port, protocol,
                  ip_list, ip_version, rate_per_min, description,
                  priority, enabled, created_by, created_at, updated_at
        "#,
        id,
        agent_id,
        req.kind,
        req.port,
        req.protocol,
        &ip_list,
        ip_version,
        req.rate_per_min,
        req.description,
        priority,
        user.user_id,
    )
    .fetch_one(&state.db)
    .await?;

    Ok((axum::http::StatusCode::CREATED, Json(rule)))
}

pub async fn delete_local_rule(
    State(state): State<AppState>,
    Path((agent_id, rule_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let deleted = sqlx::query!(
        "DELETE FROM nftables_rules WHERE id = $1 AND scope = 'local' AND agent_id = $2 RETURNING id",
        rule_id,
        agent_id,
    )
    .fetch_optional(&state.db)
    .await?;

    if deleted.is_none() {
        return Err(AppError::NotFound);
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Push local rules to the specific agent.
pub async fn push_local_rules(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(agent_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let agent = sqlx::query!(
        "SELECT id, wg_ip, api_port, status FROM agents WHERE id = $1",
        agent_id,
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if agent.status == "lockdown" || agent.status == "offline" {
        return Err(AppError::AgentUnavailable);
    }

    let rules = sqlx::query_as!(
        NftRule,
        r#"
        SELECT id, scope, agent_id, kind, port, protocol,
               ip_list, ip_version, rate_per_min, description,
               priority, enabled, created_by, created_at, updated_at
        FROM nftables_rules
        WHERE scope = 'local' AND agent_id = $1 AND enabled = true
        ORDER BY priority ASC
        "#,
        agent_id,
    )
    .fetch_all(&state.db)
    .await?;

    let chain_body = rules_to_nft_chain(&rules);

    let command = json!({
        "type": "nftables.apply",
        "chain": "lynx-local",
        "rules": chain_body,
    });

    let signed = sign_command(&state.config, agent.id, user.user_id, "admin", &command)?;
    let signed_val = serde_json::to_value(&signed).unwrap_or_default();

    let ok = if let Some(body) = ws_hub::push_command(&state, agent.id, signed_val).await {
        body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false)
    } else {
        let url = format!("http://{}:{}/cmd", agent.wg_ip, agent.api_port);
        let tok = &*state.config.internal_token;
        reqwest::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .json(&signed)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    };

    Ok(Json(json!({ "ok": ok })))
}

fn validate_rule_request(req: &CreateRuleRequest) -> Result<(), AppError> {
    let valid_kinds = ["allow_port", "block_port", "allow_ip", "block_ip", "rate_limit"];
    if !valid_kinds.contains(&req.kind.as_str()) {
        return Err(AppError::Validation("invalid rule kind".into()));
    }
    if matches!(req.kind.as_str(), "allow_port" | "block_port" | "rate_limit") && req.port.is_none() {
        return Err(AppError::Validation("port required for this rule kind".into()));
    }
    if req.kind == "rate_limit" && req.rate_per_min.is_none() {
        return Err(AppError::Validation("rate_per_min required for rate_limit".into()));
    }
    if let Some(proto) = &req.protocol {
        if !["tcp", "udp", "both"].contains(&proto.as_str()) {
            return Err(AppError::Validation("invalid protocol".into()));
        }
    }
    if let Some(ver) = &req.ip_version {
        if !["ipv4", "ipv6", "both"].contains(&ver.as_str()) {
            return Err(AppError::Validation("invalid ip_version".into()));
        }
    }
    Ok(())
}
