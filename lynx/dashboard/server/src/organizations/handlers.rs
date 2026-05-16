use super::{CreateOrgRequest, InviteMemberRequest, OrgMember, Organization, OrgWithMemberCount};
use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

// --------------------------------------------------------------------------
// GET /organizations
// --------------------------------------------------------------------------

pub async fn list_orgs(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let orgs = sqlx::query_as!(
        OrgWithMemberCount,
        r#"
        SELECT o.id, o.name, o.slug, o.owner_id, o.created_at,
               COUNT(m.user_id) AS "member_count!"
        FROM organizations o
        JOIN organization_members m ON m.organization_id = o.id
        WHERE m.user_id = $1
        GROUP BY o.id
        ORDER BY o.created_at ASC
        "#,
        user.user_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(orgs))
}

// --------------------------------------------------------------------------
// POST /organizations
// --------------------------------------------------------------------------

pub async fn create_org(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateOrgRequest>,
) -> Result<impl IntoResponse, AppError> {
    let slug = req.slug.to_lowercase();

    if !slug.chars().all(|c| c.is_alphanumeric() || c == '-') || slug.is_empty() {
        return Err(AppError::Validation("slug: only lowercase letters, numbers, and hyphens".into()));
    }

    let org_id = Uuid::now_v7();

    let org = sqlx::query_as!(
        Organization,
        r#"
        WITH new_org AS (
            INSERT INTO organizations (id, name, slug, owner_id)
            VALUES ($1, $2, $3, $4)
            RETURNING *
        ),
        _ AS (
            INSERT INTO organization_members (organization_id, user_id, role)
            VALUES ($1, $4, 'owner')
        )
        SELECT id, name, slug, owner_id, created_at FROM new_org
        "#,
        org_id,
        req.name,
        slug,
        user.user_id,
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("organizations_slug_key") {
                return AppError::Conflict("slug already taken");
            }
        }
        AppError::Internal(e.into())
    })?;

    Ok((StatusCode::CREATED, Json(org)))
}

// --------------------------------------------------------------------------
// GET /organizations/:id
// --------------------------------------------------------------------------

pub async fn get_org(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let org = sqlx::query_as!(
        Organization,
        r#"
        SELECT o.id, o.name, o.slug, o.owner_id, o.created_at
        FROM organizations o
        JOIN organization_members m ON m.organization_id = o.id
        WHERE o.id = $1 AND m.user_id = $2
        "#,
        id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(org))
}

// --------------------------------------------------------------------------
// DELETE /organizations/:id
// --------------------------------------------------------------------------

pub async fn delete_org(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query!(
        "DELETE FROM organizations WHERE id = $1 AND owner_id = $2",
        id,
        user.user_id
    )
    .execute(&state.db)
    .await?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// GET /organizations/:id/members — caller must be a member
// --------------------------------------------------------------------------

pub async fn list_members(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(org_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    // Verify caller is a member
    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .is_some();

    if !is_member {
        return Err(AppError::NotFound);
    }

    let members = sqlx::query_as!(
        OrgMember,
        r#"
        SELECT m.user_id, u.username, m.role, m.joined_at
        FROM organization_members m
        JOIN users u ON u.id = m.user_id
        WHERE m.organization_id = $1
        ORDER BY m.joined_at ASC
        "#,
        org_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(members))
}

// --------------------------------------------------------------------------
// POST /organizations/:id/members — only owner/admin can invite
// --------------------------------------------------------------------------

pub async fn invite_member(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(org_id): Path<Uuid>,
    Json(req): Json<InviteMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    let caller_role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if caller_role != "owner" && caller_role != "admin" {
        return Err(AppError::Forbidden);
    }

    let role = req.role.unwrap_or_else(|| "member".to_string());
    if !["owner", "admin", "member", "viewer"].contains(&role.as_str()) {
        return Err(AppError::Validation("role: must be owner, admin, member, or viewer".into()));
    }

    let invitee_id = sqlx::query_scalar!(
        "SELECT id FROM users WHERE username = $1",
        req.username.to_lowercase()
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Validation("user not found".into()))?;

    sqlx::query!(
        r#"
        INSERT INTO organization_members (organization_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (organization_id, user_id) DO NOTHING
        "#,
        org_id,
        invitee_id,
        role
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------------------
// DELETE /organizations/:id/members/:user_id — owner/admin; owner can't be removed
// --------------------------------------------------------------------------

pub async fn remove_member(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((org_id, target_user_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    let caller_role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if caller_role != "owner" && caller_role != "admin" {
        return Err(AppError::Forbidden);
    }

    // Prevent removing the org owner
    let target_role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        target_user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if target_role == "owner" {
        return Err(AppError::Validation("cannot remove the organization owner".into()));
    }

    sqlx::query!(
        "DELETE FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        target_user_id
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}
