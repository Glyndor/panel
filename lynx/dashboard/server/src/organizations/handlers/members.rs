use super::super::{InviteMemberRequest, OrgMember};
use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

pub async fn list_members(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(org_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
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
        return Err(AppError::Validation(
            "role: must be owner, admin, member, or viewer".into(),
        ));
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

    let target_role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        target_user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if target_role == "owner" {
        return Err(AppError::Validation(
            "cannot remove the organization owner".into(),
        ));
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
