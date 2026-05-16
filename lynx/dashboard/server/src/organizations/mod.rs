pub mod handlers;
pub mod router;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Serialize)]
pub struct OrgWithMemberCount {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub member_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrgMember {
    pub user_id: Uuid,
    pub username: String,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct InviteMemberRequest {
    pub username: String,
    pub role: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub agent_id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: String,
    pub agent_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct UpdateResourcesRequest {
    pub container_name: String,
    pub cpus: Option<f64>,
    pub memory_mb: Option<i64>,
}
