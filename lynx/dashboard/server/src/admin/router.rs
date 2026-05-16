use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rotate", post(handlers::rotate_keys))
        .route("/rotation-log", get(handlers::list_rotation_log))
        .route("/sessions", get(handlers::list_sessions))
        .route("/sessions/{id}", delete(handlers::revoke_session))
        .route("/update-check", get(handlers::update_check))
        .route("/trigger-update", post(handlers::trigger_update))
        .route("/update-log", get(handlers::list_update_log))
}
