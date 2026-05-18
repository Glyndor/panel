mod helpers;

use serde_json::{json, Value};

fn unique_username() -> String {
    let id = uuid::Uuid::now_v7().simple().to_string();
    format!("t{}", &id[..16])
}

fn unique_ip() -> String {
    let id = uuid::Uuid::now_v7();
    let b = id.as_bytes();
    format!("{}.{}.{}.{}", b[8], b[9], b[10], b[11])
}

/// Login as the pre-seeded testadmin. Returns (access_token, refresh_token, ip).
async fn admin_login(server: &axum_test::TestServer) -> (String, String, String) {
    let ip = unique_ip();
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &ip)
        .json(&json!({ "username": "testadmin", "password": "AdminP@ss12!" }))
        .await;
    res.assert_status_ok();
    let body: Value = res.json();
    (
        body["access_token"].as_str().unwrap().to_string(),
        body["refresh_token"].as_str().unwrap().to_string(),
        ip,
    )
}

/// Register a new user and return (username, password, user_id).
async fn register_and_get_id(server: &axum_test::TestServer) -> (String, String, String) {
    let username = unique_username();
    let password = "ValidP@ss12!";

    server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": username,
            "email": format!("{}@example.com", username),
            "password": password,
            "setup_token": "test-setup-token",
        }))
        .await;

    // Login to get access token and then /auth/me for user_id
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({ "username": username, "password": password }))
        .await;
    res.assert_status_ok();
    let token = res.json::<Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    // /auth/me is on the public router — no IP check, no require_auth
    let me = server
        .get("/auth/me")
        .add_header("authorization", format!("Bearer {}", token))
        .await;
    me.assert_status_ok();
    let user_id = me.json::<Value>()["id"].as_str().unwrap().to_string();

    (username, password.to_string(), user_id)
}

// ---------------------------------------------------------------------------
// GET /admin/users — list users
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_list_users_returns_array() {
    let server = helpers::test_server().await;
    let (admin_token, _, admin_ip) = admin_login(&server).await;

    let res = server
        .get("/admin/users")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;

    res.assert_status_ok();
    let body: Value = res.json();
    assert!(
        body.is_array(),
        "list users must return an array; got {body}"
    );
}

#[tokio::test]
async fn admin_list_users_requires_admin() {
    let server = helpers::test_server().await;

    // Regular user login — capture IP for subsequent require_auth calls
    let username = unique_username();
    server
        .post("/auth/register")
        .add_header("x-real-ip", &unique_ip())
        .json(&json!({
            "username": username,
            "email": format!("{}@example.com", username),
            "password": "ValidP@ss12!",
        }))
        .await;
    let user_ip = unique_ip();
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &user_ip)
        .json(&json!({ "username": username, "password": "ValidP@ss12!" }))
        .await;
    let token = res.json::<Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    let res = server
        .get("/admin/users")
        .add_header("x-real-ip", &user_ip)
        .add_header("authorization", format!("Bearer {}", token))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        403,
        "/admin/users must require admin; got {}",
        res.status_code()
    );
}

// ---------------------------------------------------------------------------
// GET /admin/permissions and GET /admin/roles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_list_permissions_includes_star_star() {
    let server = helpers::test_server().await;
    let (admin_token, _, admin_ip) = admin_login(&server).await;

    let res = server
        .get("/admin/permissions")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;

    res.assert_status_ok();
    let body: Value = res.json();
    assert!(body.is_array(), "permissions must be an array");
    let perms: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p["key"].as_str())
        .collect();
    assert!(
        perms.contains(&"*:*"),
        "permissions must include *:*; got {perms:?}"
    );
}

#[tokio::test]
async fn admin_list_roles_returns_array() {
    let server = helpers::test_server().await;
    let (admin_token, _, admin_ip) = admin_login(&server).await;

    let res = server
        .get("/admin/roles")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;

    res.assert_status_ok();
    let body: Value = res.json();
    assert!(body.is_array(), "roles must be an array");
}

// ---------------------------------------------------------------------------
// POST /admin/roles — create role, assign permission, assign to user
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_create_role_and_assign_to_user() {
    let server = helpers::test_server().await;
    let (admin_token, _, admin_ip) = admin_login(&server).await;
    let (_, _, user_id) = register_and_get_id(&server).await;

    // Create a new role
    let role_name = format!("testrole-{}", &uuid::Uuid::now_v7().simple().to_string()[..16]);
    let create_res = server
        .post("/admin/roles")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .json(&json!({ "name": role_name }))
        .await;
    assert!(
        create_res.status_code().is_success(),
        "create role must succeed; got {} — {}",
        create_res.status_code(),
        create_res.text()
    );

    // Fetch role id from roles list
    let roles: Value = server
        .get("/admin/roles")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await
        .json();
    let role_id = roles
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"].as_str() == Some(&role_name))
        .and_then(|r| r["id"].as_str())
        .expect("newly created role must appear in list");

    // Fetch vps:read permission id
    let perms: Value = server
        .get("/admin/permissions")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await
        .json();
    let perm_id = perms
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["key"].as_str() == Some("vps:read"))
        .and_then(|p| p["id"].as_str())
        .expect("vps:read permission must exist");

    // Assign permission to role
    let assign_perm_res = server
        .post(&format!("/admin/roles/{}/permissions/{}", role_id, perm_id))
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;
    assert!(
        assign_perm_res.status_code().is_success(),
        "assign permission to role must succeed; got {}",
        assign_perm_res.status_code()
    );

    // Assign role to user
    let assign_role_res = server
        .post(&format!("/admin/users/{}/roles/{}", user_id, role_id))
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;
    assert!(
        assign_role_res.status_code().is_success(),
        "assign role to user must succeed; got {}",
        assign_role_res.status_code()
    );
}

#[tokio::test]
async fn admin_create_duplicate_role_rejected() {
    let server = helpers::test_server().await;
    let (admin_token, _, admin_ip) = admin_login(&server).await;

    let role_name = format!("dup-{}", &uuid::Uuid::now_v7().simple().to_string()[..16]);

    server
        .post("/admin/roles")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .json(&json!({ "name": role_name }))
        .await;

    let dup_res = server
        .post("/admin/roles")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .json(&json!({ "name": role_name }))
        .await;

    assert!(
        dup_res.status_code().is_client_error(),
        "duplicate role name must be rejected; got {}",
        dup_res.status_code()
    );
}

// ---------------------------------------------------------------------------
// POST /admin/users/:id/force-password-change — block protected routes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_force_password_change_blocks_protected_routes() {
    let server = helpers::test_server().await;
    let (admin_token, _, admin_ip) = admin_login(&server).await;
    let (username, password, user_id) = register_and_get_id(&server).await;

    // Get the user's access token — capture IP for subsequent protected calls
    let user_ip = unique_ip();
    let (user_token, _) = {
        let res = server
            .post("/auth/login")
            .add_header("x-real-ip", &user_ip)
            .json(&json!({ "username": username, "password": password }))
            .await;
        res.assert_status_ok();
        let body: Value = res.json();
        (
            body["access_token"].as_str().unwrap().to_string(),
            body["refresh_token"].as_str().unwrap().to_string(),
        )
    };

    // Admin forces password change
    let res = server
        .post(&format!(
            "/admin/users/{}/force-password-change",
            user_id
        ))
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;
    assert!(
        res.status_code().is_success(),
        "admin force_password_change must succeed; got {}",
        res.status_code()
    );

    // User's protected requests must now return 403 force_password_change_required
    let blocked = server
        .get("/agents")
        .add_header("x-real-ip", &user_ip)
        .add_header("authorization", format!("Bearer {}", user_token))
        .await;
    assert_eq!(blocked.status_code().as_u16(), 403);
    assert_eq!(
        blocked.json::<Value>()["error"].as_str(),
        Some("force_password_change_required")
    );
}

// ---------------------------------------------------------------------------
// DELETE /admin/users/:id/sessions — revoke one user's sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_revoke_user_sessions_invalidates_tokens() {
    let server = helpers::test_server().await;
    let (admin_token, _, admin_ip) = admin_login(&server).await;
    let (username, password, user_id) = register_and_get_id(&server).await;

    let user_ip = unique_ip();
    let user_res = server
        .post("/auth/login")
        .add_header("x-real-ip", &user_ip)
        .json(&json!({ "username": username, "password": password }))
        .await;
    let user_token = user_res.json::<Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    // Admin revokes all sessions for the user
    let revoke_res = server
        .delete(&format!("/admin/users/{}/sessions", user_id))
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;
    assert!(
        revoke_res.status_code().is_success(),
        "admin revoke sessions must succeed; got {}",
        revoke_res.status_code()
    );

    // User token must now be invalid — JTI was removed from Redis
    // (IP doesn't matter here since JTI check fires first in require_auth)
    let me_res = server
        .get("/auth/me/preferences")
        .add_header("x-real-ip", &user_ip)
        .add_header("authorization", format!("Bearer {}", user_token))
        .await;
    assert_eq!(
        me_res.status_code().as_u16(),
        401,
        "revoked token must return 401; got {}",
        me_res.status_code()
    );
}

// ---------------------------------------------------------------------------
// POST /admin/users/force-password-change-all — blocks all users
// ---------------------------------------------------------------------------

#[tokio::test]
async fn force_password_change_all_blocks_existing_sessions() {
    let server = helpers::test_server().await;
    let (admin_token, _, admin_ip) = admin_login(&server).await;
    let (username, password, _) = register_and_get_id(&server).await;

    let user_ip = unique_ip();
    let user_res = server
        .post("/auth/login")
        .add_header("x-real-ip", &user_ip)
        .json(&json!({ "username": username, "password": password }))
        .await;
    let user_token = user_res.json::<Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    // Flag all users (including this one)
    server
        .post("/admin/users/force-password-change-all")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;

    // User's protected route must return 403 force_password_change_required
    let res = server
        .get("/agents")
        .add_header("x-real-ip", &user_ip)
        .add_header("authorization", format!("Bearer {}", user_token))
        .await;
    assert_eq!(
        res.status_code().as_u16(),
        403,
        "force_password_change_all must block user's protected routes; got {}",
        res.status_code()
    );
}

// ---------------------------------------------------------------------------
// GET /admin/sessions — list own sessions, DELETE /admin/sessions/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_list_own_sessions_returns_current() {
    let server = helpers::test_server().await;
    let (admin_token, _, admin_ip) = admin_login(&server).await;

    let res = server
        .get("/admin/sessions")
        .add_header("x-real-ip", &admin_ip)
        .add_header("authorization", format!("Bearer {}", admin_token))
        .await;

    res.assert_status_ok();
    let body: Value = res.json();
    assert!(body.is_array(), "sessions must be an array");
    assert!(
        !body.as_array().unwrap().is_empty(),
        "must have at least one active session"
    );
}
