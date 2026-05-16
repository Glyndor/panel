mod admin;
mod agents;
mod auth;
mod config;
mod crypto;
mod error;
mod organizations;
mod state;

use anyhow::Context;
use axum::{
    extract::State,
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use state::AppState;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = config::Config::load()?;
    let db = sqlx::PgPool::connect(&config.database_url)
        .await
        .context("connect to PostgreSQL")?;

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .context("run migrations")?;

    let redis = redis::Client::open(config.redis_url.as_str())
        .context("open Redis client")?;
    let redis_manager = redis::aio::ConnectionManager::new(redis)
        .await
        .context("connect to Redis")?;

    let state = AppState {
        db,
        redis: redis_manager,
        config: Arc::new(config),
    };

    let auth_layer = middleware::from_fn_with_state(
        state.clone(),
        auth::middleware::require_auth,
    );

    let agents_router = agents::router::router()
        .route_layer(auth_layer.clone());

    let orgs_router = organizations::router::router()
        .route_layer(auth_layer.clone());

    let admin_router = admin::router::router()
        .route_layer(auth_layer);

    let app = Router::new()
        .route("/health", get(health))
        .nest("/auth", auth::router::router())
        .nest("/agents", agents_router)
        .nest("/agents", agents::router::agent_router())
        .nest("/organizations", orgs_router)
        .nest("/admin", admin_router)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    info!("listening on 0.0.0.0:8080");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    StatusCode::OK
}

pub async fn bearer_auth(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let expected = &*state.config.internal_token;
    let a = provided.as_bytes();
    let b = expected.as_bytes();

    let ok: bool = if a.len() == b.len() {
        a.ct_eq(b).into()
    } else {
        false
    };

    if ok {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
