use serde::{Deserialize, Serialize};

use super::common::Labels;

// ---------------------------------------------------------------------------
// DeployConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeployConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_policy: Option<DeployRestartPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_config: Option<DeployUpdateConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_config: Option<DeployUpdateConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default)]
    pub labels: Labels,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<DeployPlacement>,
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ResourcesConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<ResourceSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reservations: Option<ResourceSpec>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ResourceSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids: Option<u64>,
}

// ---------------------------------------------------------------------------
// Deploy policies
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeployRestartPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeployUpdateConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelism: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_failure_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeployPlacement {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferences: Vec<serde_yaml::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_replicas_per_node: Option<u32>,
}
