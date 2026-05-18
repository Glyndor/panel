use super::super::{CreateProjectRequest, Project};
use crate::{auth::middleware::AuthUser, error::AppError, state::AppState};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

pub async fn list_projects(
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

    let projects = sqlx::query_as!(
        Project,
        "SELECT id, organization_id, agent_id, name, slug, created_at FROM projects WHERE organization_id = $1 ORDER BY created_at ASC",
        org_id
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(projects))
}

pub async fn get_project(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((org_id, proj_id)): Path<(Uuid, Uuid)>,
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

    let project = sqlx::query_as!(
        Project,
        "SELECT id, organization_id, agent_id, name, slug, created_at FROM projects WHERE id = $1 AND organization_id = $2",
        proj_id,
        org_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(project))
}

pub async fn create_project(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(org_id): Path<Uuid>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, AppError> {
    let role = sqlx::query_scalar!(
        "SELECT role FROM organization_members WHERE organization_id = $1 AND user_id = $2",
        org_id,
        user.user_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if role == "viewer" {
        return Err(AppError::Forbidden);
    }

    let slug = req.slug.to_lowercase();
    if !slug.chars().all(|c| c.is_alphanumeric() || c == '-') || slug.is_empty() {
        return Err(AppError::Validation(
            "slug: only lowercase letters, numbers, and hyphens".into(),
        ));
    }

    let agent_exists = sqlx::query_scalar!("SELECT 1 FROM agents WHERE id = $1", req.agent_id)
        .fetch_optional(&state.db)
        .await?
        .is_some();

    if !agent_exists {
        return Err(AppError::Validation("agent not found".into()));
    }

    let project = sqlx::query_as!(
        Project,
        r#"
        INSERT INTO projects (id, organization_id, agent_id, name, slug)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, organization_id, agent_id, name, slug, created_at
        "#,
        Uuid::now_v7(),
        org_id,
        req.agent_id,
        req.name,
        slug,
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.constraint() == Some("projects_organization_id_slug_key") {
                return AppError::Conflict("slug already taken in this organization");
            }
        }
        AppError::Internal(e.into())
    })?;

    Ok((StatusCode::CREATED, Json(project)))
}
