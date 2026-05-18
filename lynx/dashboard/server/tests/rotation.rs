mod helpers;

use serde_json::{json, Value};

fn unique_ip() -> String {
    let id = uuid::Uuid::now_v7();
    let b = id.as_bytes();
    format!("{}.{}.{}.{}", b[8], b[9], b[10], b[11])
}

async fn admin_login(server: &axum_test::TestServer) -> (String, String) {
    let ip = unique_ip();
    let res = server
        .post("/auth/login")
        .add_header("x-real-ip", &ip)
        .json(&serde_json::json!({ "username": "testadmin", "password": "AdminP@ss12!" }))
        .await;
    res.assert_status_ok();
    let token = res.json::<Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();
    (token, ip)
}

// ---------------------------------------------------------------------------
// §9.4 — Update + scheduled rotation in same cycle (test.md)
//
// Unit tests for the scheduler guard logic (needs_scheduled_rotation) live
// in src/scheduler.rs as inline #[tokio::test] tests.
//
// These integration tests verify the HTTP-level rotation_log behavior:
//   - Entries carry correct reasons and scopes
//   - Distinct rotation events produce distinct IDs (no duplicates)
//   - Invalid reasons are rejected
//
// NOTE: Tests here use scope='certificates' to avoid flushing Redis / deleting
// sessions (which would interfere with concurrent tests sharing the same DB).
// Session-invalidation-by-JWT-rotation is already covered in tests/auth.rs.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rotation_log_stores_update_reason_correctly() {
    let server = helpers::test_server().await;
    let (token, ip) = admin_login(&server).await;

    // 'certificates' scope: issues agent certs — no JWT flush, no Redis writes.
    // Safe to run in parallel with other tests.
    let res = server
        .post("/admin/rotate")
        .add_header("x-real-ip", &ip)
        .add_header("authorization", format!("Bearer {token}"))
        .json(&json!({ "scope": "certificates", "reason": "update" }))
        .await;

    assert!(
        res.status_code().is_success(),
        "POST /admin/rotate with reason='update' must succeed; got {} — {}",
        res.status_code(),
        res.text()
    );

    let body: Value = res.json();
    let rotation_id = body["rotation_id"]
        .as_str()
        .expect("response must include rotation_id")
        .to_string();

    // Token still valid (no JWT rotation happened)
    let log: Value = server
        .get("/admin/rotation-log")
        .add_header("x-real-ip", &ip)
        .add_header("authorization", format!("Bearer {token}"))
        .await
        .json();

    let entry = log
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"].as_str() == Some(&rotation_id))
        .expect("rotation_log must contain the entry we just created");

    assert_eq!(
        entry["reason"].as_str(),
        Some("update"),
        "rotation_log entry must have reason='update'"
    );
    assert_eq!(
        entry["scope"].as_str(),
        Some("certificates"),
        "rotation_log entry must have scope='certificates'"
    );
}

#[tokio::test]
async fn rotation_log_update_and_scheduled_produce_distinct_entries() {
    let server = helpers::test_server().await;
    let (token, ip) = admin_login(&server).await;

    // Trigger 'update' rotation (simulates post-update cert rotation)
    let res_update = server
        .post("/admin/rotate")
        .add_header("x-real-ip", &ip)
        .add_header("authorization", format!("Bearer {token}"))
        .json(&json!({ "scope": "certificates", "reason": "update" }))
        .await;
    assert!(
        res_update.status_code().is_success(),
        "update rotation must succeed; got {} — {}",
        res_update.status_code(),
        res_update.text()
    );
    let update_id = res_update.json::<Value>()["rotation_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Token still valid — trigger 'scheduled' rotation (simulates 90-day scheduler)
    let res_sched = server
        .post("/admin/rotate")
        .add_header("x-real-ip", &ip)
        .add_header("authorization", format!("Bearer {token}"))
        .json(&json!({ "scope": "certificates", "reason": "scheduled" }))
        .await;
    assert!(
        res_sched.status_code().is_success(),
        "scheduled rotation must succeed; got {} — {}",
        res_sched.status_code(),
        res_sched.text()
    );
    let sched_id = res_sched.json::<Value>()["rotation_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Must produce two distinct rotation_log entries
    assert_ne!(
        update_id, sched_id,
        "update and scheduled rotations must create distinct rotation_log entries"
    );

    let log: Value = server
        .get("/admin/rotation-log")
        .add_header("x-real-ip", &ip)
        .add_header("authorization", format!("Bearer {token}"))
        .await
        .json();

    let entries = log.as_array().unwrap();

    let found_update = entries
        .iter()
        .any(|e| e["id"].as_str() == Some(&update_id) && e["reason"].as_str() == Some("update"));
    let found_sched = entries.iter().any(|e| {
        e["id"].as_str() == Some(&sched_id) && e["reason"].as_str() == Some("scheduled")
    });

    assert!(found_update, "rotation_log must contain the 'update' entry");
    assert!(found_sched, "rotation_log must contain the 'scheduled' entry");

    // No duplicate IDs in the log
    let ids: Vec<&str> = entries.iter().filter_map(|e| e["id"].as_str()).collect();
    let unique_count = ids.iter().collect::<std::collections::HashSet<_>>().len();
    assert_eq!(
        ids.len(),
        unique_count,
        "rotation_log must not contain duplicate entries"
    );
}

#[tokio::test]
async fn rotation_log_invalid_reason_rejected() {
    let server = helpers::test_server().await;
    let (token, ip) = admin_login(&server).await;

    let res = server
        .post("/admin/rotate")
        .add_header("x-real-ip", &ip)
        .add_header("authorization", format!("Bearer {token}"))
        .json(&json!({ "scope": "certificates", "reason": "bogus_reason" }))
        .await;

    assert!(
        res.status_code().is_client_error(),
        "invalid reason must be rejected with 4xx; got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn rotation_log_invalid_scope_rejected() {
    let server = helpers::test_server().await;
    let (token, ip) = admin_login(&server).await;

    let res = server
        .post("/admin/rotate")
        .add_header("x-real-ip", &ip)
        .add_header("authorization", format!("Bearer {token}"))
        .json(&json!({ "scope": "unknown_scope", "reason": "manual" }))
        .await;

    assert!(
        res.status_code().is_client_error(),
        "invalid scope must be rejected with 4xx; got {}",
        res.status_code()
    );
}

#[tokio::test]
async fn rotation_log_non_admin_cannot_rotate() {
    let server = helpers::test_server().await;
    let username = format!(
        "t{}",
        &uuid::Uuid::now_v7().simple().to_string()[..16]
    );

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
    let user_res = server
        .post("/auth/login")
        .add_header("x-real-ip", &user_ip)
        .json(&json!({ "username": username, "password": "ValidP@ss12!" }))
        .await;
    let user_token = user_res.json::<Value>()["access_token"]
        .as_str()
        .unwrap()
        .to_string();

    let res = server
        .post("/admin/rotate")
        .add_header("x-real-ip", &user_ip)
        .add_header("authorization", format!("Bearer {user_token}"))
        .json(&json!({ "scope": "certificates", "reason": "manual" }))
        .await;

    assert_eq!(
        res.status_code().as_u16(),
        403,
        "non-admin must not be able to trigger rotation; got {}",
        res.status_code()
    );
}
