use crate::{auth::PermissionLevel, error::AgentError, nftables, state::AppState};

use serde_json::{json, Value};

pub async fn handle_nftables_apply(
    state: &AppState,
    cmd: &crate::auth::VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "nftables.apply requires write permission",
        ));
    }

    // Chain-specific update
    if let Some(chain) = cmd.command.get("chain").and_then(|v| v.as_str()) {
        let rules = cmd
            .command
            .get("rules")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        match chain {
            "lynx-global" => state.set_nft_global_body(rules.clone()),
            "lynx-local" => state.set_nft_local_body(rules.clone()),
            "lynx-global-output" => state.set_nft_global_output_body(rules.clone()),
            "lynx-local-output" => state.set_nft_local_output_body(rules.clone()),
            _ => {
                return Err(AgentError::BadRequest(
                    "unknown chain: must be lynx-global, lynx-local, lynx-global-output, or lynx-local-output",
                ))
            }
        }

        let result = apply_current_ruleset(state)?;
        let wg = state.nft_wg_port() as i32;
        let _ = sqlx::query!(
            "UPDATE nftables_state SET body = $1, wg_port = $2, updated_at = NOW() WHERE chain = $3",
            rules, wg, chain
        )
        .execute(&state.db)
        .await;
        return Ok(result);
    }

    // Full apply: { wireguard_port: 51820 }
    let wg_port = cmd
        .command
        .get("wireguard_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(51820) as u16;

    state.set_nft_wg_port(wg_port);

    let result = apply_current_ruleset(state)?;
    let wg = wg_port as i32;
    let _ = sqlx::query!(
        "UPDATE nftables_state SET wg_port = $1, updated_at = NOW()",
        wg
    )
    .execute(&state.db)
    .await;
    Ok(result)
}

fn apply_current_ruleset(state: &AppState) -> std::result::Result<Value, AgentError> {
    let ruleset = nftables::Ruleset {
        wireguard_port: state.nft_wg_port(),
        dashboard_port: state.config.dashboard_port,
        dashboard_wg_ip: crate::nftables::extract_url_host(
            state.config.dashboard_url.as_deref().unwrap_or(""),
        ),
        org_networks: vec![],
        global_body: state.nft_global_body(),
        local_body: state.nft_local_body(),
        global_output_body: state.nft_global_output_body(),
        local_output_body: state.nft_local_output_body(),
    };

    let rendered = nftables::apply(&ruleset)?;
    let checksum = nftables::current_checksum()?;
    state.set_nft_checksum(checksum);
    state.set_nft_chain_checksums(
        nftables::chain_checksum("lynx-base").ok(),
        nftables::chain_checksum("lynx-global").ok(),
        nftables::chain_checksum("lynx-local").ok(),
    );
    state.set_nft_last_ruleset(rendered);

    Ok(json!({ "ok": true }))
}

pub fn handle_nftables_restore(
    state: &AppState,
    cmd: &crate::auth::VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "nftables.restore requires write permission",
        ));
    }

    let ruleset = state
        .nft_last_ruleset()
        .ok_or_else(|| AgentError::BadRequest("no ruleset has been applied yet"))?;

    nftables::apply_raw(&ruleset)?;

    let checksum = nftables::current_checksum()?;
    state.set_nft_checksum(checksum);
    state.set_nft_chain_checksums(
        nftables::chain_checksum("lynx-base").ok(),
        nftables::chain_checksum("lynx-global").ok(),
        nftables::chain_checksum("lynx-local").ok(),
    );

    Ok(json!({ "ok": true, "action": "restored" }))
}

pub fn handle_nftables_accept(
    state: &AppState,
    cmd: &crate::auth::VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden(
            "nftables.accept requires write permission",
        ));
    }

    let current = nftables::current_checksum()?;
    state.set_nft_checksum(current.clone());
    state.set_nft_last_ruleset(String::new());

    Ok(json!({ "ok": true, "action": "accepted", "checksum": &current[..16] }))
}
