use super::handlers;
use crate::{auth::middleware::require_admin, state::AppState};
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};

pub fn router(state: AppState) -> Router<AppState> {
    let admin_layer = middleware::from_fn_with_state(state, require_admin);

    // Admin-only routes — require *:* permission on top of require_auth
    let admin_only = Router::new()
        .route("/rotate", post(handlers::rotate_keys))
        .route("/rotation-log", get(handlers::list_rotation_log))
        .route("/sessions/all", delete(handlers::revoke_all_sessions))
        .route("/users", get(handlers::list_users))
        .route("/users/{id}", delete(handlers::delete_user))
        .route("/users/{user_id}/sessions", delete(handlers::revoke_user_sessions))
        .route(
            "/users/{user_id}/sessions/{session_id}",
            delete(handlers::admin_revoke_session),
        )
        .route(
            "/users/{id}/force-password-change",
            post(handlers::force_password_change),
        )
        .route(
            "/users/force-password-change-all",
            post(handlers::force_password_change_all),
        )
        .route("/users/{id}/roles/{role_id}", post(handlers::add_user_role))
        .route(
            "/users/{id}/roles/{role_id}",
            delete(handlers::remove_user_role),
        )
        .route("/roles", get(handlers::list_roles))
        .route("/roles", post(handlers::create_role))
        .route("/roles/{id}", delete(handlers::delete_role))
        .route(
            "/roles/{id}/permissions/{perm_id}",
            post(handlers::add_role_permission),
        )
        .route(
            "/roles/{id}/permissions/{perm_id}",
            delete(handlers::remove_role_permission),
        )
        .route("/permissions", get(handlers::list_permissions))
        .route("/trigger-update", post(handlers::trigger_update))
        .route("/branding", put(crate::branding::handlers::update_branding))
        .route_layer(admin_layer);

    // Authenticated-user routes — any logged-in user (require_auth already applied by main.rs)
    let auth_only = Router::new()
        .route("/sessions", get(handlers::list_sessions))
        .route("/sessions/{id}", delete(handlers::revoke_session))
        .route("/update-check", get(handlers::update_check))
        .route("/update-log", get(handlers::list_update_log))
        .route("/alerts", get(handlers::list_alerts))
        .route("/alerts/{id}/acknowledge", post(handlers::acknowledge_alert));

    Router::new().merge(admin_only).merge(auth_only)
}
