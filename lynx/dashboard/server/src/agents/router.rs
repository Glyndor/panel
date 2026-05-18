use super::{handlers, ws_hub};
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

/// Routes that require user JWT auth (applied via route_layer in main.rs)
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::list_agents).post(handlers::register_agent),
        )
        .route("/events", get(handlers::list_agent_events))
        .route(
            "/{id}",
            get(handlers::get_agent).delete(handlers::remove_agent),
        )
        .route("/{id}/heartbeat", post(handlers::relay_heartbeat))
        .route("/{id}/cmd", post(handlers::send_command))
        .route("/{id}/nftables-status", get(handlers::nftables_status))
        .route("/{id}/nftables-resolve", post(handlers::nftables_resolve))
        .route("/{id}/audit-log", get(handlers::list_audit_log))
}

/// Routes that agents call directly (own sync token, not user JWT)
pub fn agent_router() -> Router<AppState> {
    Router::new()
        .route("/{id}/audit-sync", post(handlers::receive_audit_sync))
        .route("/{id}/events", post(handlers::receive_event))
        .route("/{id}/ws", get(ws_hub::agent_ws_handler))
}
