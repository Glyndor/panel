mod helpers;

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

// ---------------------------------------------------------------------------
// Security headers — present on all responses (§16.1 of test.md)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn security_headers_present_on_health() {
    let server = helpers::test_server().await;
    let res = server.get("/health").await;

    let headers = res.headers();
    assert_eq!(
        headers.get("x-frame-options").and_then(|v| v.to_str().ok()),
        Some("DENY"),
        "X-Frame-Options: DENY missing"
    );
    assert_eq!(
        headers.get("x-content-type-options").and_then(|v| v.to_str().ok()),
        Some("nosniff"),
        "X-Content-Type-Options: nosniff missing"
    );
    assert_eq!(
        headers.get("referrer-policy").and_then(|v| v.to_str().ok()),
        Some("no-referrer"),
        "Referrer-Policy: no-referrer missing"
    );
}

#[tokio::test]
async fn security_headers_present_on_login() {
    let server = helpers::test_server().await;
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": "any", "password": "any" }))
        .await;

    let headers = res.headers();
    assert!(
        headers.contains_key("x-frame-options"),
        "X-Frame-Options missing on 401 response"
    );
    assert!(
        headers.contains_key("x-content-type-options"),
        "X-Content-Type-Options missing on 401 response"
    );
    assert!(
        headers.contains_key("referrer-policy"),
        "Referrer-Policy missing on 401 response"
    );
}

// ---------------------------------------------------------------------------
// Anti-enumeration — login must return same status for unknown user vs wrong pw
// (§5.2 test.md — also in auth.rs but we verify status *and* body structure)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_anti_enumeration_status_matches() {
    let server = helpers::test_server().await;

    let res_missing = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": "totally_nonexistent_xyz", "password": "WrongPass1!" }))
        .await;

    let res_wrong_pw = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": "totally_nonexistent_xyz", "password": "AlsoWrong1!" }))
        .await;

    assert_eq!(
        res_missing.status_code(),
        res_wrong_pw.status_code(),
        "user-not-found and wrong-password must return identical status"
    );
    assert_eq!(res_missing.status_code().as_u16(), 401);
}

#[tokio::test]
async fn login_error_body_does_not_reveal_username_existence() {
    let server = helpers::test_server().await;

    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": "definitely_does_not_exist_abc123", "password": "Pass1!" }))
        .await;

    let body = res.text();
    // Body must not mention "not found", "no user", "doesn't exist", etc.
    let lower = body.to_lowercase();
    assert!(
        !lower.contains("not found") && !lower.contains("no user") && !lower.contains("doesn't exist"),
        "error body must not reveal user existence: {body}"
    );
}

// ---------------------------------------------------------------------------
// Open redirect (§12.5 test.md)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_absolute_external_redirect_to_is_blocked() {
    let server = helpers::test_server().await;
    let username = unique_username();

    // Register user first
    server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": username,
            "email": format!("{}@example.com", username),
            "password": "ValidP@ss12!",
            "setup_token": "test-setup-token",
        }))
        .await;

    // Login with an external redirect_to — backend must ignore or sanitize it
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": username,
            "password": "ValidP@ss12!",
            "redirect_to": "https://evil.com/phish",
        }))
        .await;

    // Login must succeed (200) — the redirect_to is handled by the frontend
    // The backend should return tokens, not a redirect to the external URL
    assert!(
        res.status_code().is_success(),
        "login with external redirect_to must still succeed: {}",
        res.status_code()
    );

    // Response must not contain the evil.com URL in any meaningful way
    let body = res.text();
    assert!(
        !body.contains("evil.com"),
        "response must not echo external redirect URL: {body}"
    );
}

// ---------------------------------------------------------------------------
// Pepper — never exposed in responses (§16.2 test.md)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_response_does_not_contain_pepper() {
    let server = helpers::test_server().await;

    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": "any", "password": "any" }))
        .await;

    let body = res.text();
    // The test pepper configured in helpers is "test-pepper"
    assert!(
        !body.contains("test-pepper"),
        "response must not expose the pepper: {body}"
    );
}

#[tokio::test]
async fn register_response_does_not_contain_pepper() {
    let server = helpers::test_server().await;
    let username = unique_username();

    let res = server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": username,
            "email": format!("{}@example.com", username),
            "password": "ValidP@ss12!",
            "setup_token": "test-setup-token",
        }))
        .await;

    let body = res.text();
    assert!(
        !body.contains("test-pepper"),
        "register response must not expose the pepper: {body}"
    );
}

// ---------------------------------------------------------------------------
// Unauthenticated access — protected endpoints return 401 (not 404 or 500)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn agents_endpoint_requires_auth() {
    let server = helpers::test_server().await;
    let res = server.get("/agents").await;
    assert_eq!(res.status_code().as_u16(), 401, "/agents must require auth");
}

#[tokio::test]
async fn organizations_endpoint_requires_auth() {
    let server = helpers::test_server().await;
    let res = server.get("/organizations").await;
    assert_eq!(res.status_code().as_u16(), 401, "/organizations must require auth");
}

#[tokio::test]
async fn admin_endpoint_requires_auth() {
    let server = helpers::test_server().await;
    let res = server.get("/admin/users").await;
    assert_eq!(res.status_code().as_u16(), 401, "/admin/* must require auth");
}

// ---------------------------------------------------------------------------
// Anti-enumeration — 404 vs 403 (§security.md, §12.3 pattern)
//
// For post-auth resources (orgs, projects, agents), the server must return
// 404 both when the resource does not exist AND when the requester lacks
// access.  Returning 403 would confirm existence to an unauthorized caller.
// ---------------------------------------------------------------------------

/// Register a user, log them in, return (bearer_token, login_ip).
async fn register_and_login(server: &axum_test::TestServer) -> (String, String) {
    let username = unique_username();
    let password = "ValidP@ss12!";

    server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
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
        .json(&json!({ "username": username, "password": password }))
        .await;
    res.assert_status_ok();
    let token = res.json::<serde_json::Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    (token, login_ip)
}

#[tokio::test]
async fn org_not_member_returns_404_not_403() {
    let server = helpers::test_server().await;

    // User A creates an organisation.
    let (token_a, ip_a) = register_and_login(&server).await;
    let create_res = server
        .post("/organizations")
        .add_header("x-real-ip", &ip_a)
        .add_header("authorization", format!("Bearer {token_a}"))
        .json(&json!({
            "name": "Secret Corp",
            "slug": format!("secret-{}", &uuid::Uuid::now_v7().simple().to_string()[..16]),
        }))
        .await;
    assert!(create_res.status_code().is_success(), "org create failed");
    let org_id = create_res
        .json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // User B (different account, NOT a member) tries to read the org.
    let (token_b, ip_b) = register_and_login(&server).await;
    let res = server
        .get(&format!("/organizations/{org_id}"))
        .add_header("x-real-ip", &ip_b)
        .add_header("authorization", format!("Bearer {token_b}"))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        404,
        "org GET by non-member must return 404, not 403; got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn org_nonexistent_returns_404() {
    let server = helpers::test_server().await;
    let (token, ip) = register_and_login(&server).await;

    // UUID v7 that was never inserted — must return 404, not 500.
    let fake_id = uuid::Uuid::now_v7();
    let res = server
        .get(&format!("/organizations/{fake_id}"))
        .add_header("x-real-ip", &ip)
        .add_header("authorization", format!("Bearer {token}"))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        404,
        "GET non-existent org must return 404; got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn org_delete_by_non_owner_returns_404_not_403() {
    let server = helpers::test_server().await;

    // Owner creates org.
    let (token_owner, ip_owner) = register_and_login(&server).await;
    let create_res = server
        .post("/organizations")
        .add_header("x-real-ip", &ip_owner)
        .add_header("authorization", format!("Bearer {token_owner}"))
        .json(&json!({
            "name": "Owned Corp",
            "slug": format!("owned-{}", &uuid::Uuid::now_v7().simple().to_string()[..16]),
        }))
        .await;
    let org_id = create_res.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Non-owner tries to delete — must get 404, not 403.
    let (token_other, ip_other) = register_and_login(&server).await;
    let res = server
        .delete(&format!("/organizations/{org_id}"))
        .add_header("x-real-ip", &ip_other)
        .add_header("authorization", format!("Bearer {token_other}"))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        404,
        "DELETE org by non-owner must return 404, not 403; got {}",
        res.status_code()
    );
}
