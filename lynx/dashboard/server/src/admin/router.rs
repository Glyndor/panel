use super::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rotate", post(handlers::rotate_keys))
        .route("/rotation-log", get(handlers::list_rotation_log))
        // Current user's own sessions
        .route("/sessions", get(handlers::list_sessions))
        .route("/sessions/{id}", delete(handlers::revoke_session))
        // Admin session management (any user)
        .route("/sessions/all", delete(handlers::revoke_all_sessions))
        .route("/users/{user_id}/sessions", delete(handlers::revoke_user_sessions))
        .route(
            "/users/{user_id}/sessions/{session_id}",
            delete(handlers::admin_revoke_session),
        )
        // Admin force password change
        .route(
            "/users/{id}/force-password-change",
            post(handlers::force_password_change),
        )
        .route(
            "/users/force-password-change-all",
            post(handlers::force_password_change_all),
        )
        .route("/update-check", get(handlers::update_check))
        .route("/trigger-update", post(handlers::trigger_update))
        .route("/update-log", get(handlers::list_update_log))
        .route("/branding", put(crate::branding::handlers::update_branding))
        .route("/alerts", get(handlers::list_alerts))
        .route("/alerts/{id}/acknowledge", post(handlers::acknowledge_alert))
}
