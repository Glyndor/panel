use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{get, post},
    Router,
};

/// Auth-protected routes (require user JWT).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::get_migration_status))
        .route("/prepare", post(handlers::prepare_receive))
        .route("/start", post(handlers::start_migration))
        .route("/abort", post(handlers::abort_migration))
        .route("/confirm-shutdown", post(handlers::confirm_shutdown))
}

/// Unauthenticated routes — token-gated by migration token.
pub fn receive_router() -> Router<AppState> {
    Router::new()
        .route("/receive", post(handlers::receive_migration))
        .route("/agent-confirm", post(handlers::agent_confirm))
}
