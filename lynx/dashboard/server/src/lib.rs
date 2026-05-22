pub mod admin;
pub mod agents;
pub mod alerts;
pub mod auth;
pub mod branding;
pub mod config;
pub mod crypto;
pub mod domain;
pub mod error;
pub mod migration;
pub mod nftables;
pub mod organizations;
pub mod peer_addr;
pub mod podman;
pub mod scheduler;
pub mod state;
pub mod update;

pub use config::Config;
pub use state::AppState;

use axum::{
    http::{header, HeaderValue},
    response::IntoResponse,
    routing::get,
    Router,
};
use tower_http::set_header::SetResponseHeaderLayer;

/// Build the full application router from an already-constructed `AppState`.
/// This does not start any background tasks or bind a port — callers do that.
pub fn build_router(state: AppState) -> Router {
    use axum::middleware;

    let auth_layer = middleware::from_fn_with_state(state.clone(), auth::middleware::require_auth);

    let agents_router = agents::router::router().route_layer(auth_layer.clone());
    let orgs_router = organizations::router::router().route_layer(auth_layer.clone());
    let admin_router = admin::router::router(state.clone()).route_layer(auth_layer.clone());
    let domain_router = domain::router::router().route_layer(auth_layer.clone());
    let migration_router = migration::router::router().route_layer(auth_layer.clone());
    let nftables_router = nftables::router::router().route_layer(auth_layer.clone());
    let auth_protected_router = auth::router::protected_router().route_layer(auth_layer);

    Router::new()
        .route("/health", get(health))
        .route("/branding", get(branding::handlers::get_branding))
        .nest("/auth", auth::router::router())
        .nest("/auth", auth_protected_router)
        .nest("/agents", agents_router)
        .nest("/agents", agents::router::agent_router())
        .nest("/organizations", orgs_router)
        .nest("/admin", admin_router)
        .nest("/domain", domain_router)
        .nest("/migration", migration_router)
        .nest("/migration", migration::router::receive_router())
        .nest("/nftables", nftables_router)
        .with_state(state)
        .layer(middleware::from_fn(peer_addr::inject_peer_addr))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static("max-age=63072000; includeSubDomains"),
        ))
}

async fn health() -> impl IntoResponse {
    axum::http::StatusCode::OK
}
