mod helpers;

use axum_test::TestServer;
use lynx_dashboard_server::build_router;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Redis fail-closed — §5.2 test.md
//
// When Redis is unavailable, auth endpoints that depend on it (rate-limit
// checks, JTI storage/validation) must return 503 Service Unavailable.
// The server must never emit tokens while Redis is down.
// ---------------------------------------------------------------------------

fn unique_ip() -> String {
    let id = uuid::Uuid::now_v7();
    let b = id.as_bytes();
    format!("{}.{}.{}.{}", b[8], b[9], b[10], b[11])
}

/// Build a `TestServer` whose Redis connection is backed by a fake TCP socket
/// that accepts but immediately drops — simulating Redis being down.
async fn server_with_redis_down() -> TestServer {
    let state = helpers::test_state_redis_down().await;
    TestServer::new(build_router(state))
}

// ---------------------------------------------------------------------------
// Login → 503 when Redis is down
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_returns_503_when_redis_down() {
    let server = server_with_redis_down().await;

    // testadmin is pre-seeded by helpers — credentials are valid in the DB.
    // Redis rate-limit check runs before password verification, so even a valid
    // login returns 503 when Redis is unavailable.
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": "testadmin", "password": "AdminP@ss12!" }))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        503,
        "login must return 503 when Redis is down; got {} — {}",
        res.status_code(),
        res.text()
    );

    let body: Value = res.json();
    assert_eq!(
        body["error"].as_str(),
        Some("service_unavailable"),
        "error field must be 'service_unavailable'; got {body}"
    );
}

#[tokio::test]
async fn login_emits_no_token_when_redis_down() {
    let server = server_with_redis_down().await;

    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": "testadmin", "password": "AdminP@ss12!" }))
        .await;

    // 503 — no token must be present in the body.
    let body = res.text();
    assert!(
        !body.contains("access_token"),
        "no access_token must be emitted when Redis is down; body: {body}"
    );
    assert!(
        !body.contains("refresh_token"),
        "no refresh_token must be emitted when Redis is down; body: {body}"
    );
}

// ---------------------------------------------------------------------------
// Register → 503 when Redis is down
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_returns_503_when_redis_down() {
    let server = server_with_redis_down().await;

    let username = format!("rdwn{}", &uuid::Uuid::now_v7().simple().to_string()[..12]);

    let res = server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": username,
            "email": format!("{username}@example.com"),
            "password": "ValidP@ss12!",
        }))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        503,
        "register must return 503 when Redis is down; got {} — {}",
        res.status_code(),
        res.text()
    );
}

// ---------------------------------------------------------------------------
// Refresh → 503 when Redis is down
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refresh_returns_503_when_redis_down() {
    let server = server_with_redis_down().await;

    // The refresh endpoint checks the rate-limit via Redis before anything
    // else — so any request returns 503 when Redis is down, regardless of
    // whether the refresh token itself is valid.
    let res = server
        .post("/auth/refresh")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            // A syntactically valid base64url-encoded token (32 random bytes
            // base64url-encoded). The rate-limit check fires before token
            // decoding, so 503 is expected.
            "refresh_token": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        }))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        503,
        "refresh must return 503 when Redis is down; got {} — {}",
        res.status_code(),
        res.text()
    );

    let body: Value = res.json();
    assert_eq!(
        body["error"].as_str(),
        Some("service_unavailable"),
        "error must be 'service_unavailable'; got {body}"
    );
}

// ---------------------------------------------------------------------------
// Normal flow resumes when Redis is back (healthy server still works)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_succeeds_when_redis_is_healthy() {
    // Confirm the normal server (with real Redis) still works — guards against
    // test infrastructure issues bleeding into the redis-down tests.
    let server = helpers::test_server().await;

    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": "testadmin", "password": "AdminP@ss12!" }))
        .await;

    assert!(
        res.status_code().is_success(),
        "login must succeed when Redis is healthy; got {} — {}",
        res.status_code(),
        res.text()
    );

    let body: Value = res.json();
    assert!(
        body["access_token"].is_string(),
        "access_token must be present in healthy response; got {body}"
    );
}
