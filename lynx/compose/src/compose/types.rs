//! Docker Compose file type definitions.
//!
//! Covers the full compose-spec so that any valid `docker-compose.yml` can
//! be round-tripped through these types.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Top-level file
// ---------------------------------------------------------------------------

/// The root of a docker-compose file.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ComposeFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub services: IndexMap<String, Service>,
    #[serde(default)]
    pub volumes: IndexMap<String, Option<VolumeConfig>>,
    #[serde(default)]
    pub networks: IndexMap<String, Option<NetworkConfig>>,
    #[serde(default)]
    pub secrets: IndexMap<String, SecretConfig>,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// A single service definition.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Service {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    #[serde(default)]
    pub environment: EnvVars,
    #[serde(default)]
    pub env_file: StringOrList,
    #[serde(default)]
    pub volumes: Vec<VolumeMount>,
    #[serde(default)]
    pub networks: ServiceNetworks,
    #[serde(default)]
    pub depends_on: DependsOn,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<HealthCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<RestartPolicy>,
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(default)]
    pub labels: Labels,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub cap_add: Vec<String>,
    #[serde(default)]
    pub cap_drop: Vec<String>,

    // --- additional compose-spec fields ---

    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingConfig>,

    #[serde(default)]
    pub sysctls: Sysctls,

    #[serde(default)]
    pub ulimits: IndexMap<String, UlimitConfig>,

    #[serde(default)]
    pub extra_hosts: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdin_open: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tty: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub init: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub privileged: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipc: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_signal: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_grace_period: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    #[serde(default)]
    pub dns: StringOrList,

    #[serde(default)]
    pub dns_search: StringOrList,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_mode: Option<String>,

    #[serde(default)]
    pub profiles: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deploy: Option<DeployConfig>,
}

// ---------------------------------------------------------------------------
// Build config
// ---------------------------------------------------------------------------

/// Service build configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum BuildConfig {
    /// Short form: just the context path.
    Context(String),
    /// Long form with optional dockerfile / args / target.
    Config {
        context: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        dockerfile: Option<String>,
        #[serde(default)]
        args: EnvVars,
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
}

impl BuildConfig {
    /// Return the build context directory.
    pub fn context(&self) -> &str {
        match self {
            BuildConfig::Context(ctx) => ctx,
            BuildConfig::Config { context, .. } => context,
        }
    }

    /// Return the Dockerfile path relative to the context, if specified.
    pub fn dockerfile(&self) -> Option<&str> {
        match self {
            BuildConfig::Context(_) => None,
            BuildConfig::Config { dockerfile, .. } => dockerfile.as_deref(),
        }
    }
}

// ---------------------------------------------------------------------------
// Command
// ---------------------------------------------------------------------------

/// Container entrypoint / command — either a shell string or exec list.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Command {
    Shell(String),
    Exec(Vec<String>),
}

impl Command {
    /// Convert to an exec-form argument list (wraps shell strings in `sh -c`).
    pub fn to_exec(&self) -> Vec<String> {
        match self {
            Command::Shell(s) => vec!["sh".into(), "-c".into(), s.clone()],
            Command::Exec(v) => v.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Environment variables
// ---------------------------------------------------------------------------

/// Environment variables as a list (`["KEY=VAL"]`) or map (`{KEY: VAL}`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum EnvVars {
    #[default]
    Empty,
    List(Vec<String>),
    Map(IndexMap<String, Option<serde_yaml::Value>>),
}

impl EnvVars {
    /// Convert to `HashMap<key, Option<value>>`.
    ///
    /// A `None` value means "inherit from environment" (key with no `=`).
    pub fn to_map(&self) -> HashMap<String, Option<String>> {
        match self {
            EnvVars::Empty => HashMap::new(),
            EnvVars::List(list) => list
                .iter()
                .filter_map(|s| {
                    let mut parts = s.splitn(2, '=');
                    let key = parts.next()?.to_string();
                    let val = parts.next().map(|v| v.to_string());
                    Some((key, val))
                })
                .collect(),
            EnvVars::Map(map) => map
                .iter()
                .map(|(k, v)| {
                    let val = v.as_ref().and_then(|v| match v {
                        serde_yaml::Value::String(s) => Some(s.clone()),
                        serde_yaml::Value::Number(n) => Some(n.to_string()),
                        serde_yaml::Value::Bool(b) => Some(b.to_string()),
                        _ => None,
                    });
                    (k.clone(), val)
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// String-or-list
// ---------------------------------------------------------------------------

/// A field that accepts either a single string or a list of strings.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum StringOrList {
    #[default]
    Empty,
    Single(String),
    List(Vec<String>),
}

impl StringOrList {
    /// Return the entries as a flat `Vec<String>`.
    pub fn to_list(&self) -> Vec<String> {
        match self {
            StringOrList::Empty => vec![],
            StringOrList::Single(s) => vec![s.clone()],
            StringOrList::List(v) => v.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Port mapping
// ---------------------------------------------------------------------------

/// Port mapping — short string form or long object form.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PortMapping {
    Short(String),
    Long {
        target: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        published: Option<StringOrU16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        host_ip: Option<String>,
    },
}

/// A value that may be either a string or a plain u16.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringOrU16 {
    String(String),
    Number(u16),
}

impl StringOrU16 {
    /// Return the value as a string.
    pub fn as_str_val(&self) -> String {
        match self {
            StringOrU16::String(s) => s.clone(),
            StringOrU16::Number(n) => n.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Volume mount
// ---------------------------------------------------------------------------

/// Volume mount — short string form or long object form.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum VolumeMount {
    Short(String),
    Long {
        #[serde(rename = "type")]
        volume_type: VolumeType,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        target: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        read_only: Option<bool>,
    },
}

impl VolumeMount {
    /// Return the container-side mount target path.
    pub fn target(&self) -> &str {
        match self {
            VolumeMount::Short(s) => {
                let parts: Vec<&str> = s.splitn(3, ':').collect();
                if parts.len() >= 2 { parts[1] } else { parts[0] }
            }
            VolumeMount::Long { target, .. } => target,
        }
    }
}

/// Volume type tag.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeType {
    Volume,
    Bind,
    Tmpfs,
}

// ---------------------------------------------------------------------------
// Networks
// ---------------------------------------------------------------------------

/// Service-level network attachment — list or map form.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum ServiceNetworks {
    #[default]
    Empty,
    List(Vec<String>),
    Map(IndexMap<String, Option<ServiceNetworkConfig>>),
}

impl ServiceNetworks {
    /// Return network names.
    pub fn names(&self) -> Vec<String> {
        match self {
            ServiceNetworks::Empty => vec![],
            ServiceNetworks::List(v) => v.clone(),
            ServiceNetworks::Map(m) => m.keys().cloned().collect(),
        }
    }
}

/// Per-network configuration for a service attachment.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceNetworkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_address: Option<String>,
}

// ---------------------------------------------------------------------------
// depends_on
// ---------------------------------------------------------------------------

/// `depends_on` — list or condition-map form.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum DependsOn {
    #[default]
    Empty,
    List(Vec<String>),
    Map(IndexMap<String, DependsOnCondition>),
}

impl DependsOn {
    /// Return the list of dependency service names.
    pub fn service_names(&self) -> Vec<String> {
        match self {
            DependsOn::Empty => vec![],
            DependsOn::List(v) => v.clone(),
            DependsOn::Map(m) => m.keys().cloned().collect(),
        }
    }

    /// Return the condition for a named dependency (defaults to `service_started`).
    pub fn condition_for(&self, service: &str) -> ServiceCondition {
        match self {
            DependsOn::Map(m) => m
                .get(service)
                .map(|c| c.condition.clone())
                .unwrap_or(ServiceCondition::ServiceStarted),
            _ => ServiceCondition::ServiceStarted,
        }
    }
}

/// One entry in the map form of `depends_on`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DependsOnCondition {
    pub condition: ServiceCondition,
}

/// Readiness condition for a dependency.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCondition {
    #[default]
    ServiceStarted,
    ServiceHealthy,
    ServiceCompletedSuccessfully,
}

// ---------------------------------------------------------------------------
// Healthcheck
// ---------------------------------------------------------------------------

/// Service healthcheck configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HealthCheck {
    pub test: Command,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_period: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
}

// ---------------------------------------------------------------------------
// Restart policy
// ---------------------------------------------------------------------------

/// Container restart policy.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    No,
    Always,
    OnFailure,
    UnlessStopped,
}

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

/// Labels — list or map form.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum Labels {
    #[default]
    Empty,
    List(Vec<String>),
    Map(IndexMap<String, String>),
}

impl Labels {
    /// Convert to a flat `HashMap`.
    pub fn to_map(&self) -> HashMap<String, String> {
        match self {
            Labels::Empty => HashMap::new(),
            Labels::List(list) => list
                .iter()
                .filter_map(|s| {
                    let mut parts = s.splitn(2, '=');
                    Some((
                        parts.next()?.to_string(),
                        parts.next().unwrap_or("").to_string(),
                    ))
                })
                .collect(),
            Labels::Map(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

/// Logging driver configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LoggingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(default)]
    pub options: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Sysctls
// ---------------------------------------------------------------------------

/// `sysctls` accepts either a list (`["net.core.somaxconn=1024"]`) or a map.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum Sysctls {
    #[default]
    Empty,
    List(Vec<String>),
    Map(IndexMap<String, String>),
}

impl Sysctls {
    /// Return as a flat map (`sysctl_name → value`).
    pub fn to_map(&self) -> HashMap<String, String> {
        match self {
            Sysctls::Empty => HashMap::new(),
            Sysctls::List(list) => list
                .iter()
                .filter_map(|s| {
                    let mut parts = s.splitn(2, '=');
                    let key = parts.next()?.to_string();
                    let val = parts.next().unwrap_or("").to_string();
                    Some((key, val))
                })
                .collect(),
            Sysctls::Map(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Ulimits
// ---------------------------------------------------------------------------

/// Ulimit value — either a single number (soft == hard) or an explicit soft/hard pair.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum UlimitConfig {
    Single(i64),
    Pair { soft: i64, hard: i64 },
}

impl UlimitConfig {
    pub fn soft(&self) -> i64 {
        match self {
            UlimitConfig::Single(n) => *n,
            UlimitConfig::Pair { soft, .. } => *soft,
        }
    }

    pub fn hard(&self) -> i64 {
        match self {
            UlimitConfig::Single(n) => *n,
            UlimitConfig::Pair { hard, .. } => *hard,
        }
    }
}

// ---------------------------------------------------------------------------
// Deploy
// ---------------------------------------------------------------------------

/// Basic `deploy:` section (Swarm / compose-spec).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeployConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_policy: Option<DeployRestartPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// Resource limits and reservations.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ResourcesConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<ResourceSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reservations: Option<ResourceSpec>,
}

/// A single resource specification (CPUs + memory).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ResourceSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
}

/// Restart policy inside `deploy:`.
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

// ---------------------------------------------------------------------------
// Top-level resource configs
// ---------------------------------------------------------------------------

/// Named volume configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VolumeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(default)]
    pub driver_opts: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Named network configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NetworkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal: Option<bool>,
}

/// Named secret configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecretConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
