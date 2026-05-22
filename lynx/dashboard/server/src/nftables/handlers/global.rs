use crate::{
    agents::ws_hub,
    auth::middleware::AuthUser,
    crypto::cmd::sign_command,
    error::AppError,
    nftables::{rules_to_nft_chain, rules_to_nft_output_chain, CreateRuleRequest, NftRule},
    state::AppState,
};
use axum::{
    extract::{Extension, State},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use uuid::Uuid;

pub async fn list_global_rules(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let rules = sqlx::query_as!(
        NftRule,
        r#"
        SELECT id, scope, agent_id, kind, port, port_end, protocol,
               ip_list, ip_version, rate_per_min, description,
               priority, enabled, direction, created_by, created_at, updated_at
        FROM nftables_rules
        WHERE scope = 'global'
        ORDER BY priority ASC, created_at ASC
        "#
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(rules))
}

pub async fn create_global_rule(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateRuleRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_rule_request(&req)?;

    let id = Uuid::now_v7();
    let ip_list = req.ip_list.unwrap_or_default();
    let ip_version = req.ip_version.unwrap_or_else(|| "both".into());
    let priority = req.priority.unwrap_or(0);
    let direction = req.direction.unwrap_or_else(|| "input".into());

    let rule = sqlx::query_as!(
        NftRule,
        r#"
        INSERT INTO nftables_rules
            (id, scope, agent_id, kind, port, port_end, protocol, ip_list, ip_version,
             rate_per_min, description, priority, direction, created_by)
        VALUES ($1, 'global', NULL, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING id, scope, agent_id, kind, port, port_end, protocol,
                  ip_list, ip_version, rate_per_min, description,
                  priority, enabled, direction, created_by, created_at, updated_at
        "#,
        id,
        req.kind,
        req.port,
        req.port_end,
        req.protocol,
        &ip_list,
        ip_version,
        req.rate_per_min,
        req.description,
        priority,
        direction,
        user.user_id,
    )
    .fetch_one(&state.db)
    .await?;

    Ok((axum::http::StatusCode::CREATED, Json(rule)))
}

pub async fn delete_global_rule(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let deleted = sqlx::query!(
        "DELETE FROM nftables_rules WHERE id = $1 AND scope = 'global' RETURNING id",
        id
    )
    .fetch_optional(&state.db)
    .await?;

    if deleted.is_none() {
        return Err(AppError::NotFound);
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Push current global rules to all online agents.
/// Sends two commands per agent: input chain body + output chain body.
pub async fn push_global_rules(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let rules = sqlx::query_as!(
        NftRule,
        r#"
        SELECT id, scope, agent_id, kind, port, port_end, protocol,
               ip_list, ip_version, rate_per_min, description,
               priority, enabled, direction, created_by, created_at, updated_at
        FROM nftables_rules
        WHERE scope = 'global' AND enabled = true
        ORDER BY priority ASC
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let input_body = rules_to_nft_chain(&rules);
    let output_body = rules_to_nft_output_chain(&rules);

    let agents =
        sqlx::query!("SELECT id, wg_ip, api_port, status FROM agents WHERE status = 'online'")
            .fetch_all(&state.db)
            .await?;

    let mut pushed = 0u32;
    let mut failed = 0u32;

    for agent in &agents {
        let sent = push_both_chains(
            &state,
            agent.id,
            agent.wg_ip.as_str(),
            agent.api_port,
            user.user_id,
            &input_body,
            &output_body,
        )
        .await;

        if sent {
            for rule in &rules {
                let _ = sqlx::query!(
                    r#"
                    INSERT INTO global_rule_sync (rule_id, agent_id, synced_at)
                    VALUES ($1, $2, NOW())
                    ON CONFLICT (rule_id, agent_id) DO UPDATE SET synced_at = NOW()
                    "#,
                    rule.id,
                    agent.id,
                )
                .execute(&state.db)
                .await;
            }
            pushed += 1;
        } else {
            failed += 1;
        }
    }

    // Mark offline agents as pending_sync so they receive the rules on reconnect.
    let offline_agents = sqlx::query!("SELECT id FROM agents WHERE status != 'online'")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    for agent in &offline_agents {
        for rule in &rules {
            let _ = sqlx::query!(
                r#"
                INSERT INTO global_rule_sync (rule_id, agent_id)
                VALUES ($1, $2)
                ON CONFLICT (rule_id, agent_id) DO NOTHING
                "#,
                rule.id,
                agent.id,
            )
            .execute(&state.db)
            .await;
        }
    }

    Ok(Json(
        json!({ "pushed": pushed, "failed": failed, "pending": offline_agents.len() }),
    ))
}

/// Send the global input and output chain bodies to a single agent.
/// Returns true only if both commands succeed.
async fn push_both_chains(
    state: &AppState,
    agent_id: Uuid,
    wg_ip: &str,
    api_port: i32,
    user_id: Uuid,
    input_body: &str,
    output_body: &str,
) -> bool {
    let input_cmd = json!({
        "type": "nftables.apply",
        "chain": "lynx-global",
        "rules": input_body,
    });
    let output_cmd = json!({
        "type": "nftables.apply",
        "chain": "lynx-global-output",
        "rules": output_body,
    });

    let Ok(signed_in) = sign_command(
        state.config.as_ref(),
        agent_id,
        user_id,
        "admin",
        &input_cmd,
    ) else {
        return false;
    };
    let Ok(signed_out) = sign_command(
        state.config.as_ref(),
        agent_id,
        user_id,
        "admin",
        &output_cmd,
    ) else {
        return false;
    };

    let in_val = serde_json::to_value(&signed_in).unwrap_or_default();
    let sent_in = if ws_hub::push_command(state, agent_id, in_val)
        .await
        .is_some()
    {
        true
    } else {
        let url = format!("https://{wg_ip}:{api_port}/cmd");
        let tok = &*state.config.internal_token;
        reqwest::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .json(&signed_in)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    };

    if !sent_in {
        return false;
    }

    let out_val = serde_json::to_value(&signed_out).unwrap_or_default();
    if ws_hub::push_command(state, agent_id, out_val)
        .await
        .is_some()
    {
        true
    } else {
        let url = format!("https://{wg_ip}:{api_port}/cmd");
        let tok = &*state.config.internal_token;
        reqwest::Client::new()
            .post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .json(&signed_out)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}

fn validate_rule_request(req: &CreateRuleRequest) -> Result<(), AppError> {
    let valid_kinds = [
        "allow_port",
        "block_port",
        "allow_ip",
        "block_ip",
        "rate_limit",
        "drop_invalid_state",
        "tcp_flag_null",
        "tcp_flag_xmas",
        "tcp_flag_ack_new",
        "icmp_ping_limit",
        "allow_icmp_errors",
        "allow_ndp",
        "block_output_port",
    ];
    if !valid_kinds.contains(&req.kind.as_str()) {
        return Err(AppError::Validation("invalid rule kind".into()));
    }
    if matches!(
        req.kind.as_str(),
        "allow_port" | "block_port" | "rate_limit" | "block_output_port"
    ) && req.port.is_none()
    {
        return Err(AppError::Validation(
            "port required for this rule kind".into(),
        ));
    }
    if req.kind == "rate_limit" && req.rate_per_min.is_none() {
        return Err(AppError::Validation(
            "rate_per_min required for rate_limit".into(),
        ));
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
    if let Some(dir) = &req.direction {
        if !["input", "output"].contains(&dir.as_str()) {
            return Err(AppError::Validation("invalid direction".into()));
        }
    }
    // Validate port range: 1-65535, start ≤ end.
    if let Some(p) = req.port {
        if !(1..=65535).contains(&p) {
            return Err(AppError::Validation("port must be 1–65535".into()));
        }
    }
    if let Some(pe) = req.port_end {
        if !(1..=65535).contains(&pe) {
            return Err(AppError::Validation("port_end must be 1–65535".into()));
        }
        if let Some(p) = req.port {
            if p > pe {
                return Err(AppError::Validation("port must be ≤ port_end".into()));
            }
        }
    }
    // Validate IP list entries are valid IPs or CIDRs (IP/prefix_len).
    if let Some(ref ips) = req.ip_list {
        for ip_str in ips {
            if !is_valid_ip_or_cidr(ip_str) {
                return Err(AppError::Validation(format!(
                    "invalid IP or CIDR: {ip_str}"
                )));
            }
        }
    }
    Ok(())
}

fn is_valid_ip_or_cidr(s: &str) -> bool {
    if s.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    if let Some((ip_part, prefix_part)) = s.rsplit_once('/') {
        if let Ok(ip) = ip_part.parse::<std::net::IpAddr>() {
            if let Ok(prefix) = prefix_part.parse::<u8>() {
                let max = if ip.is_ipv4() { 32u8 } else { 128u8 };
                return prefix <= max;
            }
        }
    }
    false
}
