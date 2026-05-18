use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Response types ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub force_password_change: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub roles: Vec<RoleRef>,
}

#[derive(Serialize)]
pub struct RoleRef {
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize)]
pub struct RoleRow {
    pub id: Uuid,
    pub name: String,
    pub permissions: Vec<PermRef>,
}

#[derive(Serialize)]
pub struct PermRef {
    pub id: Uuid,
    pub key: String,
}

#[derive(Deserialize)]
pub struct CreateRoleBody {
    pub name: String,
}

// ── Handlers ───────────────────────────────────────────────────────────────

/// GET /admin/users
pub async fn list_users(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let users = sqlx::query!(
        r#"SELECT id, username, force_password_change, created_at FROM users ORDER BY created_at"#
    )
    .fetch_all(&state.db)
    .await?;

    let mut result: Vec<UserRow> = Vec::with_capacity(users.len());

    for u in users {
        let roles = sqlx::query!(
            r#"SELECT r.id, r.name FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = $1 ORDER BY r.name"#,
            u.id
        )
        .fetch_all(&state.db)
        .await?;

        result.push(UserRow {
            id: u.id,
            username: u.username,
            force_password_change: u.force_password_change,
            created_at: u.created_at,
            roles: roles.into_iter().map(|r| RoleRef { id: r.id, name: r.name }).collect(),
        });
    }

    Ok(Json(result))
}

/// DELETE /admin/users/:id
pub async fn delete_user(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if user_id == caller.user_id {
        return Err(AppError::BadRequest("cannot delete your own account"));
    }

    // Guard: don't delete the last admin
    let is_admin: bool = sqlx::query_scalar!(
        r#"SELECT EXISTS(
            SELECT 1 FROM user_roles ur
            JOIN role_permissions rp ON rp.role_id = ur.role_id
            JOIN permissions p ON p.id = rp.permission_id
            WHERE ur.user_id = $1 AND p.key = '*:*'
        ) AS "exists!""#,
        user_id
    )
    .fetch_one(&state.db)
    .await?;

    if is_admin {
        let admin_count: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(DISTINCT ur.user_id) FROM user_roles ur
               JOIN role_permissions rp ON rp.role_id = ur.role_id
               JOIN permissions p ON p.id = rp.permission_id
               WHERE p.key = '*:*'"#
        )
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);

        if admin_count <= 1 {
            return Err(AppError::BadRequest("cannot delete the last admin account"));
        }
    }

    let rows = sqlx::query!("DELETE FROM users WHERE id = $1", user_id)
        .execute(&state.db)
        .await?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// GET /admin/permissions
pub async fn list_permissions(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let perms = sqlx::query!("SELECT id, key FROM permissions ORDER BY key")
        .fetch_all(&state.db)
        .await?;

    let result: Vec<PermRef> = perms.into_iter().map(|p| PermRef { id: p.id, key: p.key }).collect();
    Ok(Json(result))
}

/// GET /admin/roles
pub async fn list_roles(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let roles = sqlx::query!("SELECT id, name FROM roles ORDER BY name")
        .fetch_all(&state.db)
        .await?;

    let mut result: Vec<RoleRow> = Vec::with_capacity(roles.len());

    for r in roles {
        let perms = sqlx::query!(
            r#"SELECT p.id, p.key FROM role_permissions rp JOIN permissions p ON p.id = rp.permission_id WHERE rp.role_id = $1 ORDER BY p.key"#,
            r.id
        )
        .fetch_all(&state.db)
        .await?;

        result.push(RoleRow {
            id: r.id,
            name: r.name,
            permissions: perms.into_iter().map(|p| PermRef { id: p.id, key: p.key }).collect(),
        });
    }

    Ok(Json(result))
}

/// POST /admin/roles
pub async fn create_role(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthUser>,
    Json(body): Json<CreateRoleBody>,
) -> Result<impl IntoResponse, AppError> {
    let name = body.name.trim().to_string();
    if name.is_empty() || name.len() > 64 {
        return Err(AppError::Validation("role name must be 1–64 characters".into()));
    }

    let id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO roles (id, name, created_by) VALUES ($1, $2, $3)",
        id,
        name,
        caller.user_id,
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("unique") {
            AppError::Conflict("role name already exists")
        } else {
            AppError::from(e)
        }
    })?;

    Ok((axum::http::StatusCode::CREATED, Json(serde_json::json!({ "id": id, "name": name }))))
}

/// DELETE /admin/roles/:id
pub async fn delete_role(
    State(state): State<AppState>,
    Path(role_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Guard: don't delete role if it's the only source of *:* for any user
    let admin_users_via_this_role: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(DISTINCT ur.user_id) FROM user_roles ur
           JOIN role_permissions rp ON rp.role_id = ur.role_id
           JOIN permissions p ON p.id = rp.permission_id
           WHERE ur.role_id = $1 AND p.key = '*:*'"#,
        role_id
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0);

    if admin_users_via_this_role > 0 {
        // Check if those users have another role granting *:*
        let users_without_other_admin: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(DISTINCT ur.user_id) FROM user_roles ur
               JOIN role_permissions rp ON rp.role_id = ur.role_id
               JOIN permissions p ON p.id = rp.permission_id
               WHERE ur.role_id = $1 AND p.key = '*:*'
               AND NOT EXISTS (
                   SELECT 1 FROM user_roles ur2
                   JOIN role_permissions rp2 ON rp2.role_id = ur2.role_id
                   JOIN permissions p2 ON p2.id = rp2.permission_id
                   WHERE ur2.user_id = ur.user_id AND ur2.role_id != $1 AND p2.key = '*:*'
               )"#,
            role_id
        )
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);

        if users_without_other_admin > 0 {
            return Err(AppError::BadRequest(
                "deleting this role would leave users without admin access",
            ));
        }
    }

    let rows = sqlx::query!("DELETE FROM roles WHERE id = $1", role_id)
        .execute(&state.db)
        .await?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /admin/roles/:id/permissions/:perm_id
pub async fn add_role_permission(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthUser>,
    Path((role_id, perm_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO role_permissions (id, role_id, permission_id, created_by) VALUES ($1, $2, $3, $4)
         ON CONFLICT (role_id, permission_id) DO NOTHING",
        id,
        role_id,
        perm_id,
        caller.user_id,
    )
    .execute(&state.db)
    .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// DELETE /admin/roles/:id/permissions/:perm_id
pub async fn remove_role_permission(
    State(state): State<AppState>,
    Path((role_id, perm_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    // Guard: don't remove *:* if it would leave someone without an admin role
    let perm_key: Option<String> =
        sqlx::query_scalar!("SELECT key FROM permissions WHERE id = $1", perm_id)
            .fetch_optional(&state.db)
            .await?;

    if perm_key.as_deref() == Some("*:*") {
        let users_losing_admin: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(DISTINCT ur.user_id) FROM user_roles ur WHERE ur.role_id = $1
               AND NOT EXISTS (
                   SELECT 1 FROM user_roles ur2
                   JOIN role_permissions rp2 ON rp2.role_id = ur2.role_id
                   JOIN permissions p2 ON p2.id = rp2.permission_id
                   WHERE ur2.user_id = ur.user_id AND ur2.role_id != $1 AND p2.key = '*:*'
               )"#,
            role_id
        )
        .fetch_one(&state.db)
        .await?
        .unwrap_or(0);

        if users_losing_admin > 0 {
            return Err(AppError::BadRequest(
                "removing this permission would leave users without admin access",
            ));
        }
    }

    sqlx::query!(
        "DELETE FROM role_permissions WHERE role_id = $1 AND permission_id = $2",
        role_id,
        perm_id,
    )
    .execute(&state.db)
    .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /admin/users/:id/roles/:role_id
pub async fn add_user_role(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthUser>,
    Path((user_id, role_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    // Verify user and role exist
    let user_exists: bool = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM users WHERE id = $1) AS "exists!""#,
        user_id
    )
    .fetch_one(&state.db)
    .await?;

    if !user_exists {
        return Err(AppError::NotFound);
    }

    let id = Uuid::now_v7();
    sqlx::query!(
        "INSERT INTO user_roles (id, user_id, role_id, created_by) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, role_id) DO NOTHING",
        id,
        user_id,
        role_id,
        caller.user_id,
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("foreign key") {
            AppError::NotFound
        } else {
            AppError::from(e)
        }
    })?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// DELETE /admin/users/:id/roles/:role_id
pub async fn remove_user_role(
    State(state): State<AppState>,
    Extension(caller): Extension<AuthUser>,
    Path((user_id, role_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    // Guard: can't remove own last admin role
    if user_id == caller.user_id {
        let is_last_admin_role: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM role_permissions rp
                JOIN permissions p ON p.id = rp.permission_id
                WHERE rp.role_id = $1 AND p.key = '*:*'
            ) AS "exists!""#,
            role_id
        )
        .fetch_one(&state.db)
        .await?;

        if is_last_admin_role {
            let other_admin_roles: i64 = sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM user_roles ur
                   JOIN role_permissions rp ON rp.role_id = ur.role_id
                   JOIN permissions p ON p.id = rp.permission_id
                   WHERE ur.user_id = $1 AND ur.role_id != $2 AND p.key = '*:*'"#,
                user_id,
                role_id
            )
            .fetch_one(&state.db)
            .await?
            .unwrap_or(0);

            if other_admin_roles == 0 {
                return Err(AppError::BadRequest("cannot remove your own last admin role"));
            }
        }
    }

    sqlx::query!(
        "DELETE FROM user_roles WHERE user_id = $1 AND role_id = $2",
        user_id,
        role_id,
    )
    .execute(&state.db)
    .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
