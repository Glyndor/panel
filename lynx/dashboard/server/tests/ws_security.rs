//! WebSocket security tests — Origin header validation (CSWSH defence).
//!
//! Spec: security.md §12.4, dashboard.md "WebSocket — Arquitectura completa".
//! Backend handler: src/agents/handlers/events_ws.rs::validate_ws_origin.
//!
//! Uses axum-test's `ws` feature with `http_transport` so the real WebSocket
//! upgrade path runs — required because `WebSocketUpgrade::from_request_parts`
//! needs hyper's `OnUpgrade` extension, which only exists with a real HTTP
//! transport (not the default in-memory mock).
//!
//! A mismatched `Origin` causes the handler to return `403 Forbidden` before
//! `ws.on_upgrade()` is called, so the test sees a plain HTTP response and
//! never completes the WS protocol negotiation.

mod helpers;

use lynx_dashboard_server::build_router;
use serde_json::json;

fn unique_ip() -> String {
    let id = uuid::Uuid::now_v7();
    let b = id.as_bytes();
    format!("{}.{}.{}.{}", b[8], b[9], b[10], b[11])
}

fn unique_username() -> String {
    let id = uuid::Uuid::now_v7().simple().to_string();
    format!("t{}", &id[..16])
}

/// Build a TestServer with HTTP transport so WS upgrades actually wire up.
async fn http_server() -> axum_test::TestServer {
    let state = helpers::test_state().await;
    let app = build_router(state);
    axum_test::TestServer::builder().http_transport().build(app)
}

/// Login as the seeded `testadmin` (which holds `*:*`).  Registers a throwaway
/// user first to keep prior test invariants; the actual session belongs to
/// `testadmin` to satisfy the `vps:read|*:*` ACL on the WS handler.
async fn login_as_admin(server: &axum_test::TestServer) -> (String, String) {
    let username = unique_username();
    let password = "ValidP@ss12!";
    let ip = unique_ip();

    server
        .post("/auth/register")
        .add_header("x-real-ip", &ip)
        .json(&json!({
            "username": username,
            "email": format!("{username}@example.com"),
            "password": password,
            "setup_token": "test-setup-token",
        }))
        .await;

    let login_ip = unique_ip();
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &login_ip)
        .json(&json!({ "username": "testadmin", "password": "AdminP@ss12!" }))
        .await;
    res.assert_status_ok();
    let token = res.json::<serde_json::Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    (token, login_ip)
}

// ---------------------------------------------------------------------------
// §12.4 — Cross-site WebSocket hijacking: foreign Origin must be rejected
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_events_upgrade_rejected_for_external_origin() {
    let server = http_server().await;
    let (token, ip) = login_as_admin(&server).await;

    let res = server
        .get_websocket("/agents/events/ws")
        .add_header("authorization", format!("Bearer {token}"))
        .add_header("x-real-ip", &ip)
        .add_header("origin", "https://evil.com")
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        403,
        "WS upgrade with foreign Origin must be 403, got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn ws_metrics_upgrade_rejected_for_external_origin() {
    let server = http_server().await;
    let (token, ip) = login_as_admin(&server).await;

    let fake_agent = uuid::Uuid::now_v7();
    let res = server
        .get_websocket(&format!("/agents/{fake_agent}/metrics/ws"))
        .add_header("authorization", format!("Bearer {token}"))
        .add_header("x-real-ip", &ip)
        .add_header("origin", "https://attacker.test")
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        403,
        "metrics WS upgrade with foreign Origin must be 403, got {}",
        res.status_code()
    );
}

// ---------------------------------------------------------------------------
// Wrong scheme on a matching host is still rejected — Origin must be https.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ws_events_upgrade_rejected_for_http_scheme_origin() {
    let server = http_server().await;
    let (token, ip) = login_as_admin(&server).await;

    // No domain configured in test state, so the validator compares against the
    // Host header (https://<host>).  Sending Origin with http:// must fail.
    let res = server
        .get_websocket("/agents/events/ws")
        .add_header("authorization", format!("Bearer {token}"))
        .add_header("x-real-ip", &ip)
        .add_header("origin", "http://localhost")
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        403,
        "WS upgrade with http:// Origin must be 403, got {}",
        res.status_code()
    );
}
