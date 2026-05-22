mod helpers;

use futures_util::future::join_all;
use serde_json::{json, Value};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unique_username() -> String {
    // Use first 16 hex chars of UUID v7 — unique enough, within 32-char max.
    let id = uuid::Uuid::now_v7().simple().to_string();
    format!("t{}", &id[..16])
}

/// Generate a unique fake IP so each test invocation has its own rate-limit bucket.
/// Uses random bytes from UUID v7 (bytes 8-11 are fully random, not timestamp).
fn unique_ip() -> String {
    let id = uuid::Uuid::now_v7();
    let b = id.as_bytes();
    // bytes[8..12] are the random node bits — avoid timestamp-prefix collisions.
    format!("{}.{}.{}.{}", b[8], b[9], b[10], b[11])
}

/// Register a fresh user and return (username, password).
async fn register_user(server: &axum_test::TestServer) -> (String, String) {
    let username = unique_username();
    let password = "ValidP@ss12!";
    let ip = unique_ip();

    let res = server
        .post("/auth/register")
        .add_header("x-real-ip", &ip)
        .json(&json!({
            "username": username,
            "email": format!("{}@example.com", username),
            "password": password,
            // Always included — ignored once an admin exists, required during bootstrap.
            "setup_token": "test-setup-token",
        }))
        .await;

    assert!(
        res.status_code().is_success(),
        "register helper failed: {} — {}",
        res.status_code(),
        res.text()
    );

    (username, password.to_string())
}

/// Login and return (access_token, refresh_token, ip).
async fn login_user(
    server: &axum_test::TestServer,
    username: &str,
    password: &str,
) -> (String, String, String) {
    let ip = unique_ip();
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &ip)
        .json(&json!({ "username": username, "password": password }))
        .await;
    res.assert_status_ok();
    let body: Value = res.json();
    (
        body["access_token"].as_str().unwrap().to_string(),
        body["refresh_token"].as_str().unwrap().to_string(),
        ip,
    )
}

// ---------------------------------------------------------------------------
// POST /auth/register
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_valid_creates_user() {
    let server = helpers::test_server().await;
    let (username, _) = register_user(&server).await;
    // If we get here without panic, registration succeeded.
    assert!(!username.is_empty());
}

#[tokio::test]
async fn register_duplicate_username_rejected() {
    let server = helpers::test_server().await;
    let username = unique_username();
    let email = format!("{}@example.com", username);

    server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": username,
            "email": email,
            "password": "ValidPass1!",
        }))
        .await;

    let res = server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": username,
            "email": format!("other{}@example.com", username),
            "password": "ValidPass1!",
        }))
        .await;

    assert!(
        res.status_code().is_client_error(),
        "duplicate username must fail; got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn register_weak_password_rejected() {
    let server = helpers::test_server().await;
    let username = unique_username();

    let res = server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": username,
            "email": format!("{}@example.com", username),
            "password": "weak",
        }))
        .await;

    assert!(
        res.status_code().is_client_error(),
        "weak password must be rejected; got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn register_short_username_rejected() {
    let server = helpers::test_server().await;

    let res = server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": "ab",
            "email": "ab@example.com",
            "password": "ValidPass1!",
        }))
        .await;

    assert!(
        res.status_code().is_client_error(),
        "short username must be rejected; got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn register_reserved_username_rejected() {
    let server = helpers::test_server().await;

    let res = server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": "admin",
            "email": "admin@example.com",
            "password": "ValidPass1!",
        }))
        .await;

    assert!(
        res.status_code().is_client_error(),
        "reserved username must be rejected; got {}",
        res.status_code()
    );
}

// ---------------------------------------------------------------------------
// POST /auth/login
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_correct_credentials_returns_tokens() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;

    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": username, "password": password }))
        .await;

    res.assert_status_ok();
    let body: Value = res.json();
    assert!(body["access_token"].is_string(), "must have access_token");
    assert!(body["refresh_token"].is_string(), "must have refresh_token");
    assert!(body["expires_in"].is_number(), "must have expires_in");
}

#[tokio::test]
async fn login_wrong_password_rejected() {
    let server = helpers::test_server().await;
    let (username, _) = register_user(&server).await;

    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": username, "password": "WrongPass1!" }))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        401,
        "wrong password must return 401"
    );
}

#[tokio::test]
async fn login_nonexistent_user_same_error_as_wrong_password() {
    let server = helpers::test_server().await;

    let ip1 = unique_ip();
    let ip2 = unique_ip();

    let res_nonexistent = server
        .post("/auth/login")
        .add_header("x-real-ip", &ip1)
        .json(&json!({ "username": "doesnotexist_xyzxyz", "password": "ValidPass1!" }))
        .await;

    let res_wrong_pw = server
        .post("/auth/login")
        .add_header("x-real-ip", &ip2)
        .json(&json!({ "username": "doesnotexist_xyzxyz", "password": "AlsoWrong1!" }))
        .await;

    // Both must return the same status — anti-enumeration.
    assert_eq!(
        res_nonexistent.status_code(),
        res_wrong_pw.status_code(),
        "user-not-found and wrong-password must return same status code"
    );
}

// ---------------------------------------------------------------------------
// POST /auth/refresh
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refresh_rotates_tokens() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (_, refresh_token, _) = login_user(&server, &username, &password).await;

    let res = server
        .post("/auth/refresh")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "refresh_token": refresh_token }))
        .await;

    res.assert_status_ok();
    let body: Value = res.json();
    assert!(
        body["access_token"].is_string(),
        "refresh must return access_token"
    );
    assert!(
        body["refresh_token"].is_string(),
        "refresh must return new refresh_token"
    );
    assert_ne!(
        body["refresh_token"].as_str().unwrap(),
        refresh_token.as_str(),
        "refresh must rotate the token"
    );
}

#[tokio::test]
async fn refresh_old_token_rejected_after_rotation() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (_, original_refresh, _) = login_user(&server, &username, &password).await;

    let ip = unique_ip();

    // Consume the refresh token once.
    server
        .post("/auth/refresh")
        .add_header("x-real-ip", &ip)
        .json(&json!({ "refresh_token": original_refresh }))
        .await;

    // Replay the old refresh token — must be rejected.
    let res = server
        .post("/auth/refresh")
        .add_header("x-real-ip", &ip)
        .json(&json!({ "refresh_token": original_refresh }))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        401,
        "replayed refresh token must return 401"
    );
}

#[tokio::test]
async fn refresh_invalid_token_rejected() {
    let server = helpers::test_server().await;

    let res = server
        .post("/auth/refresh")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "refresh_token": "not.a.real.token" }))
        .await;

    assert!(
        res.status_code().is_client_error(),
        "invalid refresh token must be rejected; got {}",
        res.status_code()
    );
}

// ---------------------------------------------------------------------------
// POST /auth/logout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn logout_invalidates_access_token() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (access_token, _, _) = login_user(&server, &username, &password).await;

    // Logout.
    let logout_res = server
        .post("/auth/logout")
        .add_header("x-real-ip", &unique_ip())
        .add_header("authorization", format!("Bearer {}", access_token))
        .await;

    assert!(
        logout_res.status_code().is_success(),
        "logout must succeed; got {}",
        logout_res.status_code()
    );

    // The revoked access token must no longer work for /auth/me.
    let me_res = server
        .get("/auth/me")
        .add_header("authorization", format!("Bearer {}", access_token))
        .await;

    assert_eq!(
        me_res.status_code().as_u16(),
        401,
        "revoked access token must return 401 on /auth/me"
    );
}

// ---------------------------------------------------------------------------
// GET /auth/me
// ---------------------------------------------------------------------------

#[tokio::test]
async fn me_returns_user_info() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (access_token, _, login_ip) = login_user(&server, &username, &password).await;

    let res = server
        .get("/auth/me")
        .add_header("authorization", format!("Bearer {}", access_token))
        .add_header("x-real-ip", &login_ip)
        .await;

    res.assert_status_ok();
    let body: Value = res.json();
    assert_eq!(
        body["username"].as_str().unwrap(),
        username,
        "me must return username"
    );
    assert!(body["id"].is_string(), "me must return id");
}

#[tokio::test]
async fn me_without_token_returns_401() {
    let server = helpers::test_server().await;

    let res = server.get("/auth/me").await;

    assert_eq!(
        res.status_code().as_u16(),
        401,
        "unauthenticated /me must return 401"
    );
}

// ---------------------------------------------------------------------------
// GET /health
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_returns_ok() {
    let server = helpers::test_server().await;
    let res = server.get("/health").await;
    res.assert_status_ok();
}

// ---------------------------------------------------------------------------
// force_password_change — backend enforcement (§5.6 test.md)
// ---------------------------------------------------------------------------

/// Log in as the pre-seeded testadmin. Returns (access_token, refresh_token, ip).
async fn admin_login(server: &axum_test::TestServer) -> (String, String, String) {
    login_user(server, "testadmin", "AdminP@ss12!").await
}

#[tokio::test]
async fn force_password_change_blocks_protected_routes() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (access_token, _, user_ip) = login_user(&server, &username, &password).await;

    // Get user id via /auth/me
    let me = server
        .get("/auth/me")
        .add_header("authorization", format!("Bearer {}", access_token))
        .add_header("x-real-ip", &user_ip)
        .await;
    me.assert_status_ok();
    let user_id = me.json::<Value>()["id"].as_str().unwrap().to_string();

    // Admin sets force_password_change = true on this user
    let (admin_token, _, admin_ip) = admin_login(&server).await;
    let force_res = server
        .post(&format!("/admin/users/{}/force-password-change", user_id))
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;
    assert!(
        force_res.status_code().is_success(),
        "admin force_password_change must succeed; got {}",
        force_res.status_code()
    );

    // Protected route must return 403 with force_password_change_required
    let res = server
        .get("/agents")
        .add_header("x-real-ip", &user_ip)
        .add_header("authorization", format!("Bearer {}", access_token))
        .await;
    assert_eq!(
        res.status_code().as_u16(),
        403,
        "force_password_change must block /agents with 403; got {}",
        res.status_code()
    );
    let body: Value = res.json();
    assert_eq!(
        body["error"].as_str(),
        Some("force_password_change_required"),
        "error code must be force_password_change_required"
    );
}

#[tokio::test]
async fn force_password_change_allows_change_password_endpoint() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (access_token, _, user_ip) = login_user(&server, &username, &password).await;

    let me = server
        .get("/auth/me")
        .add_header("authorization", format!("Bearer {}", access_token))
        .add_header("x-real-ip", &user_ip)
        .await;
    let user_id = me.json::<Value>()["id"].as_str().unwrap().to_string();

    let (admin_token, _, admin_ip) = admin_login(&server).await;
    server
        .post(&format!("/admin/users/{}/force-password-change", user_id))
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;

    // /auth/change-password must NOT be blocked — it's the only bypass
    let change_res = server
        .post("/auth/change-password")
        .add_header("authorization", format!("Bearer {}", access_token))
        .json(&json!({
            "current_password": password,
            "new_password": "NewValidP@ss12!",
        }))
        .await;
    assert_eq!(
        change_res.status_code().as_u16(),
        204,
        "change-password must succeed even with force_password_change; got {} — {}",
        change_res.status_code(),
        change_res.text()
    );
}

#[tokio::test]
async fn force_password_change_cleared_after_change() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (access_token, _, user_ip) = login_user(&server, &username, &password).await;

    let me = server
        .get("/auth/me")
        .add_header("authorization", format!("Bearer {}", access_token))
        .add_header("x-real-ip", &user_ip)
        .await;
    let user_id = me.json::<Value>()["id"].as_str().unwrap().to_string();

    let (admin_token, _, admin_ip) = admin_login(&server).await;
    server
        .post(&format!("/admin/users/{}/force-password-change", user_id))
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;

    // Change password (clears flag + invalidates all sessions)
    let new_password = "NewValidP@ss12!";
    server
        .post("/auth/change-password")
        .add_header("authorization", format!("Bearer {}", access_token))
        .json(&json!({
            "current_password": password,
            "new_password": new_password,
        }))
        .await;

    // Login again with new password
    let login_res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": username, "password": new_password }))
        .await;
    login_res.assert_status_ok();
    let body: Value = login_res.json();
    assert_eq!(
        body["force_password_change"].as_bool(),
        Some(false),
        "force_password_change must be false after password change"
    );
}

// ---------------------------------------------------------------------------
// POST /auth/change-password
// ---------------------------------------------------------------------------

#[tokio::test]
async fn change_password_wrong_current_rejected() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (access_token, _, _) = login_user(&server, &username, &password).await;

    let res = server
        .post("/auth/change-password")
        .add_header("authorization", format!("Bearer {}", access_token))
        .json(&json!({
            "current_password": "WrongCurrent1!",
            "new_password": "NewValidP@ss12!",
        }))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        401,
        "wrong current password must be rejected with 401; got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn change_password_weak_new_password_rejected() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (access_token, _, _) = login_user(&server, &username, &password).await;

    let res = server
        .post("/auth/change-password")
        .add_header("authorization", format!("Bearer {}", access_token))
        .json(&json!({
            "current_password": password,
            "new_password": "weak",
        }))
        .await;

    assert!(
        res.status_code().is_client_error(),
        "weak new password must be rejected; got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn change_password_invalidates_old_sessions() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (access_token, _, _) = login_user(&server, &username, &password).await;

    // Change password
    server
        .post("/auth/change-password")
        .add_header("authorization", format!("Bearer {}", access_token))
        .json(&json!({
            "current_password": password,
            "new_password": "NewValidP@ss12!",
        }))
        .await;

    // Old access token must now be invalid
    let me_res = server
        .get("/auth/me")
        .add_header("authorization", format!("Bearer {}", access_token))
        .await;
    assert_eq!(
        me_res.status_code().as_u16(),
        401,
        "old access token must be revoked after password change"
    );
}

// ---------------------------------------------------------------------------
// Single-session mode (§5.3 test.md)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn single_session_second_login_invalidates_first() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;
    let (token_a, _, ip_a) = login_user(&server, &username, &password).await;

    // Enable single_session mode — must use same IP as login for require_auth to pass
    server
        .post("/auth/me/single-session")
        .add_header("x-real-ip", &ip_a)
        .add_header("authorization", format!("Bearer {}", token_a))
        .json(&json!({ "enabled": true }))
        .await;

    // Second login — should invalidate token_a
    let (token_b, _, ip_b) = login_user(&server, &username, &password).await;

    // token_a must now be rejected. We replay the IP it was bound to so the
    // /auth/me handler reaches the session-revocation check rather than
    // short-circuiting on `intercepted` for an IP mismatch.
    let res_a = server
        .get("/auth/me")
        .add_header("x-real-ip", &ip_a)
        .add_header("authorization", format!("Bearer {}", token_a))
        .await;
    assert_eq!(
        res_a.status_code().as_u16(),
        401,
        "first session token must be revoked after second login with single_session=true"
    );

    // token_b must still work — must use the IP it was bound to.
    let res_b = server
        .get("/auth/me")
        .add_header("x-real-ip", &ip_b)
        .add_header("authorization", format!("Bearer {}", token_b))
        .await;
    assert_eq!(
        res_b.status_code().as_u16(),
        200,
        "second login token must still be valid"
    );
}

// ---------------------------------------------------------------------------
// Session intercepted — IP mismatch (§5.5 test.md)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ip_mismatch_invalidates_session() {
    let server = helpers::test_server().await;
    let (username, password) = register_user(&server).await;

    // Login from IP A
    let ip_a = unique_ip();
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &ip_a)
        .json(&json!({ "username": username, "password": password }))
        .await;
    res.assert_status_ok();
    let body: Value = res.json();
    let access_token = body["access_token"].as_str().unwrap().to_string();

    // Use token from a DIFFERENT IP — middleware must reject and fire intercepted
    let ip_b = unique_ip();
    let res = server
        .get("/auth/me/preferences")
        .add_header("x-real-ip", &ip_b)
        .add_header("authorization", format!("Bearer {}", access_token))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        401,
        "token used from different IP must return 401 (intercepted); got {}",
        res.status_code()
    );

    // Subsequent use of the same token from the original IP must also fail (session deleted)
    let res2 = server
        .get("/auth/me/preferences")
        .add_header("x-real-ip", &ip_a)
        .add_header("authorization", format!("Bearer {}", access_token))
        .await;
    assert_eq!(
        res2.status_code().as_u16(),
        401,
        "token must remain invalid after intercepted event"
    );
}

// ---------------------------------------------------------------------------
// Concurrent refresh — §5.4 test.md
//
// When 10 requests simultaneously submit the same refresh token, exactly one
// must succeed and the remaining nine must receive 401.  The atomic UPDATE
// WHERE refresh_token_hash = old_hash guarantees only the first writer wins.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_refresh_only_one_wins() {
    let server = Arc::new(helpers::test_server().await);
    let (username, password) = register_user(&server).await;
    let (_, refresh_token, _) = login_user(&server, &username, &password).await;

    // 10 concurrent refresh requests, each from a distinct IP to avoid
    // hitting the per-IP rate limit (10 / 5min window).
    let futures: Vec<_> = (0..10u8)
        .map(|i| {
            let server = Arc::clone(&server);
            let token = refresh_token.clone();
            async move {
                let ip = format!("172.31.{i}.1");
                server
                    .post("/auth/refresh")
                    .add_header("x-real-ip", &ip)
                    .json(&json!({ "refresh_token": token }))
                    .await
                    .status_code()
                    .as_u16()
            }
        })
        .collect();

    let statuses: Vec<u16> = join_all(futures).await;

    let successes = statuses.iter().filter(|&&s| s == 200).count();
    let unauthorized = statuses.iter().filter(|&&s| s == 401).count();

    assert_eq!(
        successes, 1,
        "exactly 1 concurrent refresh must succeed; got statuses: {statuses:?}"
    );
    assert_eq!(
        unauthorized, 9,
        "exactly 9 concurrent refreshes must return 401; got statuses: {statuses:?}"
    );
}

#[tokio::test]
async fn concurrent_refresh_winner_returns_valid_tokens() {
    let server = Arc::new(helpers::test_server().await);
    let (username, password) = register_user(&server).await;
    let (_, refresh_token, _) = login_user(&server, &username, &password).await;

    let futures: Vec<_> = (0..10u8)
        .map(|i| {
            let server = Arc::clone(&server);
            let token = refresh_token.clone();
            async move {
                let ip = format!("192.0.2.{i}");
                let res = server
                    .post("/auth/refresh")
                    .add_header("x-real-ip", &ip)
                    .json(&json!({ "refresh_token": token }))
                    .await;
                let status = res.status_code().as_u16();
                if status == 200 {
                    let body: Value = res.json();
                    Some((
                        body["access_token"].as_str().unwrap_or("").to_string(),
                        body["refresh_token"].as_str().unwrap_or("").to_string(),
                        ip,
                    ))
                } else {
                    None
                }
            }
        })
        .collect();

    let results: Vec<Option<(String, String, String)>> = join_all(futures).await;

    let winners: Vec<_> = results.into_iter().flatten().collect();
    assert_eq!(
        winners.len(),
        1,
        "exactly one response must contain tokens; got {} winners",
        winners.len()
    );

    let (new_access, new_refresh, winner_ip) = &winners[0];
    assert!(
        !new_access.is_empty(),
        "winner access_token must not be empty"
    );
    assert!(
        !new_refresh.is_empty(),
        "winner refresh_token must not be empty"
    );

    // The new access token must work — replay the IP that the winning refresh
    // was bound to, otherwise /auth/me rejects on ip_hash mismatch.
    let me = server
        .get("/auth/me")
        .add_header("x-real-ip", winner_ip)
        .add_header("authorization", format!("Bearer {new_access}"))
        .await;
    assert!(
        me.status_code().is_success(),
        "access token from winning refresh must be valid; got {}",
        me.status_code()
    );
}
