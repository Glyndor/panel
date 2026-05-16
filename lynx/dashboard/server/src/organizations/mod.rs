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

#[derive(Debug, Deserialize)]
pub struct DeployContainerRequest {
    pub name: String,
    pub image: String,
    pub ports: Option<Vec<String>>,
    pub env: Option<Vec<String>>,
    pub cpus: Option<f64>,
    pub memory_mb: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ContainerActionPath {
    pub id: Uuid,
    pub proj_id: Uuid,
    pub name: String,
    pub action: String,
}

#[derive(Debug, Deserialize)]
pub struct HorizontalScaleRequest {
    /// Agent to deploy replicas on (Agent-B)
    pub target_agent_id: Uuid,
    /// Container image to run as replica
    pub image: String,
    /// Number of replicas to run on Agent-B
    pub replica_count: u32,
    /// Data-plane WireGuard port on both agents (default 51821)
    pub wg_port: Option<u16>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DataPlaneTunnel {
    pub id: Uuid,
    pub project_id: Uuid,
    pub agent_a_id: Uuid,
    pub agent_b_id: Uuid,
    pub agent_a_wg_ip: String,
    pub agent_b_wg_ip: String,
    pub wg_port: i32,
    pub replica_count: i32,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
