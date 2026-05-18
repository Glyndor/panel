use super::super::{CreateOrgRequest, OrgWithMemberCount, Organization};
use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

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

pub async fn create_org(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateOrgRequest>,
) -> Result<impl IntoResponse, AppError> {
    let slug = req.slug.to_lowercase();

    if !slug.chars().all(|c| c.is_alphanumeric() || c == '-') || slug.is_empty() {
        return Err(AppError::Validation(
            "slug: only lowercase letters, numbers, and hyphens".into(),
        ));
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
