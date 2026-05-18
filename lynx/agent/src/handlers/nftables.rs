use crate::{
    auth::PermissionLevel,
    error::AgentError,
    nftables,
    state::AppState,
};
use serde_json::{json, Value};

pub fn handle_nftables_apply(
    state: &AppState,
    cmd: &crate::auth::VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("nftables.apply requires write permission"));
    }

    let wg_port = cmd
        .command
        .get("wireguard_port")
        .and_then(|v| v.as_u64())
        .unwrap_or(51820) as u16;

    let ruleset = nftables::Ruleset {
        wireguard_port: wg_port,
        org_networks: vec![],
    };

    let rendered = nftables::apply(&ruleset).map_err(anyhow::Error::from)?;
    let checksum = nftables::checksum_of(&ruleset);
    state.set_nft_checksum(checksum);
    state.set_nft_last_ruleset(rendered);
    Ok(json!({ "ok": true }))
}

pub fn handle_nftables_restore(
    state: &AppState,
    cmd: &crate::auth::VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("nftables.restore requires write permission"));
    }

    let ruleset = state
        .nft_last_ruleset()
        .ok_or_else(|| AgentError::BadRequest("no ruleset has been applied yet"))?;

    nftables::apply_raw(&ruleset).map_err(anyhow::Error::from)?;

    let checksum = nftables::current_checksum().map_err(anyhow::Error::from)?;
    state.set_nft_checksum(checksum);

    Ok(json!({ "ok": true, "action": "restored" }))
}

pub fn handle_nftables_accept(
    state: &AppState,
    cmd: &crate::auth::VerifiedCommand,
) -> std::result::Result<Value, AgentError> {
    if cmd.permission == PermissionLevel::Read {
        return Err(AgentError::Forbidden("nftables.accept requires write permission"));
    }

    let current = nftables::current_checksum().map_err(anyhow::Error::from)?;
    state.set_nft_checksum(current.clone());
    state.set_nft_last_ruleset(String::new());

    Ok(json!({ "ok": true, "action": "accepted", "checksum": &current[..16] }))
}
