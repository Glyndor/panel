use axum_test::TestServer;
use ed25519_dalek::SigningKey;
use lynx_dashboard_server::{build_router, crypto::pki, AppState};
use redis::aio::ConnectionManager;
use rustls;
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, RwLock};
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};
use zeroize::Zeroizing;

/// Build a deterministic test `AppState` backed by the CI/dev postgres and Redis.
pub async fn test_state() -> AppState {
    // Install the ring CryptoProvider exactly once per process.
    // Multiple parallel tokio::test tasks would race on this — install_default returns
    // Err if already set, which we safely ignore.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://lynx:lynx_dev@localhost:5433/lynx_dashboard".to_string());
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let db = PgPool::connect(&db_url).await.expect("connect to test DB");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migrate test DB");

    // Ensure bootstrap token window is open (idempotent — ON CONFLICT DO NOTHING).
    sqlx::query!(
        "INSERT INTO system_config (key, value) VALUES ('setup_token_issued_at', NOW()::text) ON CONFLICT (key) DO NOTHING"
    )
    .execute(&db)
    .await
    .expect("seed setup_token_issued_at");

    // Ensure a test admin exists so register tests don't race to bootstrap.
    // Uses ON CONFLICT to be idempotent across parallel test invocations.
    seed_test_admin(&db).await;

    let redis = redis::Client::open(redis_url.as_str()).expect("open redis");
    let redis_manager = ConnectionManager::new(redis).await.expect("connect redis");

    let sign_seed = [0x42u8; 32];
    let signing = SigningKey::from_bytes(&sign_seed);
    let sign_pub = signing.verifying_key().to_bytes();

    let enc_priv = [0x77u8; 32];
    let enc_pub = *X25519Public::from(&StaticSecret::from(enc_priv)).as_bytes();

    let ca_seed = [0x11u8; 32];
    let ca_signing = SigningKey::from_bytes(&ca_seed);
    let ca_pub = ca_signing.verifying_key().to_bytes();

    let (x509_ca_cert_der, x509_ca_key_der) =
        pki::generate_x509_ca().expect("generate test X.509 CA");
    let (x509_client_cert_der, x509_client_key_der) =
        pki::issue_x509_dashboard_client_cert(&x509_ca_cert_der, &x509_ca_key_der)
            .expect("issue test client cert");

    let config = Arc::new(lynx_dashboard_server::Config {
        database_url: db_url,
        redis_url,
        internal_token: Zeroizing::new("test-internal-token".to_string()),
        kek: Zeroizing::new([0xAAu8; 32]),
        pepper: Zeroizing::new("test-pepper".to_string()),
        setup_token: Some(Zeroizing::new("test-setup-token".to_string())),
        jwt_sign_private_seed: Zeroizing::new(sign_seed),
        jwt_sign_public_bytes: sign_pub,
        jwt_enc_private_bytes: Zeroizing::new(enc_priv),
        jwt_enc_public_bytes: enc_pub,
        ca_private_seed: Zeroizing::new(ca_seed),
        ca_public_bytes: ca_pub,
        x509_ca_cert_der,
        x509_ca_key_der,
        x509_client_cert_der,
        x509_client_key_der,
    });

    let (events_tx, _) = broadcast::channel(16);

    AppState {
        db,
        redis: redis_manager,
        config,
        latest_agent_version: Arc::new(RwLock::new(None)),
        wg_psks: Arc::new(RwLock::new(HashMap::new())),
        agent_ws_conns: Arc::new(RwLock::new(HashMap::new())),
        agent_metric_tx: Arc::new(RwLock::new(HashMap::new())),
        events_tx: Arc::new(events_tx),
    }
}

/// Seed a static test admin user idempotently.
/// Uses a PostgreSQL advisory lock so parallel test processes don't race.
async fn seed_test_admin(db: &PgPool) {
    use lynx_dashboard_server::crypto::{kek, password};
    use uuid::Uuid;

    // Advisory lock key — arbitrary constant, unique to this seeding operation.
    const LOCK_KEY: i64 = 0x4c796e78_5f746573i64; // "Lynx_tes"

    // Acquire session-level advisory lock — blocks until no other test holds it.
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(LOCK_KEY)
        .execute(db)
        .await
        .expect("acquire advisory lock");

    // Check specifically that testadmin exists AND has *:* — not just any admin.
    // Other test binaries may have bootstrapped their own admin user (satisfying
    // "any admin with *:*") causing this function to return early and skip
    // inserting testadmin, which would make rotation/admin tests fail.
    let testadmin_ready: bool = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM users u
            JOIN user_roles ur ON ur.user_id = u.id
            JOIN role_permissions rp ON rp.role_id = ur.role_id
            JOIN permissions p ON p.id = rp.permission_id
            WHERE u.username = 'testadmin' AND p.key = '*:*'
        ) AS "exists!""#
    )
    .fetch_one(db)
    .await
    .unwrap_or(false);

    if testadmin_ready {
        // Reset force_password_change in case a previous test set it on all users.
        let _ = sqlx::query!(
            "UPDATE users SET force_password_change = false WHERE username = 'testadmin'"
        )
        .execute(db)
        .await;

        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(LOCK_KEY)
            .execute(db)
            .await
            .ok();
        return;
    }

    let test_kek = [0xAAu8; 32];
    let test_pepper = "test-pepper";
    let user_id = Uuid::now_v7();

    let pwd_hash = password::hash("AdminP@ss12!").expect("hash admin password");
    let dek = kek::gen_dek();
    let dek_encrypted = kek::encrypt_dek(&dek, &test_kek).expect("encrypt dek");
    let email_lower = "testadmin@example.com";
    let email_encrypted =
        kek::encrypt_with_dek(email_lower.as_bytes(), &dek).expect("encrypt email");
    let email_hash = lynx_dashboard_server::crypto::hash::email_hash(email_lower, test_pepper);

    // Insert user — skip if already exists (another parallel test may have inserted it).
    let _ = sqlx::query!(
        r#"INSERT INTO users (id, username, email_hash, email_encrypted, password_hash, dek_encrypted)
           VALUES ($1, 'testadmin', $2, $3, $4, $5)
           ON CONFLICT (username) DO NOTHING"#,
        user_id,
        email_hash,
        email_encrypted,
        pwd_hash,
        dek_encrypted,
    )
    .execute(db)
    .await;

    // Fetch the actual user_id in case it was already inserted.
    let actual_id: Uuid = sqlx::query_scalar!("SELECT id FROM users WHERE username = 'testadmin'")
        .fetch_one(db)
        .await
        .expect("fetch testadmin id");

    // Create Admin role and assign *:* — use DO NOTHING on conflicts.
    let star_perm_id: Uuid = sqlx::query_scalar!("SELECT id FROM permissions WHERE key = '*:*'")
        .fetch_one(db)
        .await
        .expect("fetch *:* permission");

    let role_id = Uuid::now_v7();
    let _ = sqlx::query!(
        "INSERT INTO roles (id, name, created_by) VALUES ($1, 'Admin', $2) ON CONFLICT (name) DO NOTHING",
        role_id,
        actual_id,
    )
    .execute(db)
    .await;

    let actual_role_id: Uuid = sqlx::query_scalar!("SELECT id FROM roles WHERE name = 'Admin'")
        .fetch_one(db)
        .await
        .expect("fetch Admin role id");

    let _ = sqlx::query!(
        "INSERT INTO role_permissions (id, role_id, permission_id, created_by) VALUES ($1, $2, $3, $4) ON CONFLICT (role_id, permission_id) DO NOTHING",
        Uuid::now_v7(),
        actual_role_id,
        star_perm_id,
        actual_id,
    )
    .execute(db)
    .await;

    let _ = sqlx::query!(
        "INSERT INTO user_roles (id, user_id, role_id, created_by) VALUES ($1, $2, $3, $4) ON CONFLICT (user_id, role_id) DO NOTHING",
        Uuid::now_v7(),
        actual_id,
        actual_role_id,
        actual_id,
    )
    .execute(db)
    .await;

    // Reset force_password_change in case a previous test set it on all users.
    let _ =
        sqlx::query!("UPDATE users SET force_password_change = false WHERE username = 'testadmin'")
            .execute(db)
            .await;

    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(LOCK_KEY)
        .execute(db)
        .await
        .ok();
}

pub async fn test_server() -> TestServer {
    let state = test_state().await;
    let app = build_router(state);
    TestServer::new(app)
}

/// Build an `AppState` identical to `test_state` except the Redis `ConnectionManager`
/// is backed by a TCP socket that accepts the initial connection then immediately
/// closes — making all subsequent Redis commands fail with a connection error.
///
/// Used to verify that auth endpoints return 503 (fail-closed) when Redis is down.
pub async fn test_state_redis_down() -> AppState {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://lynx:lynx_dev@localhost:5433/lynx_dashboard".to_string());

    let db = PgPool::connect(&db_url).await.expect("connect to test DB");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migrate test DB");

    sqlx::query!(
        "INSERT INTO system_config (key, value) VALUES ('setup_token_issued_at', NOW()::text) ON CONFLICT (key) DO NOTHING"
    )
    .execute(&db)
    .await
    .expect("seed setup_token_issued_at");

    seed_test_admin(&db).await;

    // Minimal fake Redis server.
    //
    // redis-rs 0.27 sends a CLIENT SETINFO pipeline on every new connection.
    // ConnectionManager then keeps the connection alive and retries (with
    // exponential backoff) if it closes.  To avoid the backoff delay we keep
    // each accepted connection open and respond to every incoming RESP2 command
    // with a RESP2 error, causing the rate-limit check (first Redis command in
    // every auth handler) to fail → 503.
    //
    // Protocol sketch:
    //   client → *N\r\n   (array of N bulk strings = one command per frame)
    //   server → +OK\r\n  for CLIENT SETINFO frames during setup
    //            -ERR redis_down\r\n for everything else
    let fake_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake redis listener");
    let fake_port = fake_listener.local_addr().unwrap().port();
    let redis_url = format!("redis://127.0.0.1:{fake_port}");

    tokio::spawn(async move {
        while let Ok((mut stream, _)) = fake_listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let mut setup_done = false;

                loop {
                    let n = match tokio::time::timeout(
                        std::time::Duration::from_secs(30),
                        stream.read(&mut buf),
                    )
                    .await
                    {
                        Ok(Ok(0)) | Err(_) => break, // EOF or idle timeout
                        Ok(Ok(n)) => n,
                        Ok(Err(_)) => break,
                    };

                    // Count RESP arrays received — each *N is one command.
                    let cmd_count = buf[..n].iter().filter(|&&b| b == b'*').count().max(1);

                    let response = if !setup_done {
                        // First batch: CLIENT SETINFO × 2 — respond OK for setup.
                        setup_done = true;
                        "+OK\r\n".repeat(cmd_count)
                    } else {
                        // All subsequent commands → error so handlers return 503.
                        "-ERR redis_down\r\n".repeat(cmd_count)
                    };

                    if stream.write_all(response.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    let redis_client = redis::Client::open(redis_url.as_str()).expect("open fake redis client");

    let redis_manager = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        ConnectionManager::new(redis_client),
    )
    .await
    .expect("ConnectionManager::new timed out")
    .expect("ConnectionManager::new failed");

    let sign_seed = [0x42u8; 32];
    let signing = SigningKey::from_bytes(&sign_seed);
    let sign_pub = signing.verifying_key().to_bytes();

    let enc_priv = [0x77u8; 32];
    let enc_pub = *X25519Public::from(&StaticSecret::from(enc_priv)).as_bytes();

    let ca_seed = [0x11u8; 32];
    let ca_signing = SigningKey::from_bytes(&ca_seed);
    let ca_pub = ca_signing.verifying_key().to_bytes();

    let (x509_ca_cert_der, x509_ca_key_der) =
        pki::generate_x509_ca().expect("generate test X.509 CA");
    let (x509_client_cert_der, x509_client_key_der) =
        pki::issue_x509_dashboard_client_cert(&x509_ca_cert_der, &x509_ca_key_der)
            .expect("issue test client cert");

    let config = Arc::new(lynx_dashboard_server::Config {
        database_url: db_url,
        redis_url,
        internal_token: Zeroizing::new("test-internal-token".to_string()),
        kek: Zeroizing::new([0xAAu8; 32]),
        pepper: Zeroizing::new("test-pepper".to_string()),
        setup_token: Some(Zeroizing::new("test-setup-token".to_string())),
        jwt_sign_private_seed: Zeroizing::new(sign_seed),
        jwt_sign_public_bytes: sign_pub,
        jwt_enc_private_bytes: Zeroizing::new(enc_priv),
        jwt_enc_public_bytes: enc_pub,
        ca_private_seed: Zeroizing::new(ca_seed),
        ca_public_bytes: ca_pub,
        x509_ca_cert_der,
        x509_ca_key_der,
        x509_client_cert_der,
        x509_client_key_der,
    });

    let (events_tx, _) = broadcast::channel(16);

    AppState {
        db,
        redis: redis_manager,
        config,
        latest_agent_version: Arc::new(RwLock::new(None)),
        wg_psks: Arc::new(RwLock::new(HashMap::new())),
        agent_ws_conns: Arc::new(RwLock::new(HashMap::new())),
        agent_metric_tx: Arc::new(RwLock::new(HashMap::new())),
        events_tx: Arc::new(events_tx),
    }
}
