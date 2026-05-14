use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub fn to_exec(&self) -> Vec<String> {
        match self {
            Command::Shell(s) => vec!["sh".into(), "-c".into(), s.clone()],
            Command::Exec(v) => v.clone(),
        }
    }

    pub fn to_argv(&self) -> Vec<String> {
        match self {
            Command::Shell(s) => vec![s.clone()],
            Command::Exec(v) => v.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// EnvVars
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
                        serde_yaml::Value::Null => None,
                        _ => None,
                    });
                    (k.clone(), val)
                })
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            EnvVars::Empty => true,
            EnvVars::List(v) => v.is_empty(),
            EnvVars::Map(m) => m.is_empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// StringOrList
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
    pub fn to_list(&self) -> Vec<String> {
        match self {
            StringOrList::Empty => vec![],
            StringOrList::Single(s) => vec![s.clone()],
            StringOrList::List(v) => v.clone(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            StringOrList::Empty => true,
            StringOrList::Single(s) => s.is_empty(),
            StringOrList::List(v) => v.is_empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// EnvFile — supports both short form and long-form {path, required, format}
// ---------------------------------------------------------------------------

/// One entry in an `env_file:` list — either a bare path or a long-form object.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EnvFileEntry {
    Path(String),
    Config {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        required: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<String>,
    },
}

impl EnvFileEntry {
    pub fn path(&self) -> &str {
        match self {
            EnvFileEntry::Path(p) => p,
            EnvFileEntry::Config { path, .. } => path,
        }
    }

    /// `true` by default — missing file is an error unless `required: false`.
    pub fn required(&self) -> bool {
        match self {
            EnvFileEntry::Path(_) => true,
            EnvFileEntry::Config { required, .. } => required.unwrap_or(true),
        }
    }
}

/// `env_file:` field — single path, list of paths, or list of long-form objects.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum EnvFile {
    #[default]
    Empty,
    Single(EnvFileEntry),
    List(Vec<EnvFileEntry>),
}

impl EnvFile {
    pub fn to_entries(&self) -> Vec<EnvFileEntry> {
        match self {
            EnvFile::Empty => vec![],
            EnvFile::Single(e) => vec![e.clone()],
            EnvFile::List(v) => v.clone(),
        }
    }

    /// Return just the paths (strips `required` / `format` info).
    /// Kept for test compatibility; prefer `to_entries()` in engine code.
    pub fn to_list(&self) -> Vec<String> {
        self.to_entries().into_iter().map(|e| e.path().to_string()).collect()
    }

    pub fn is_empty(&self) -> bool {
        match self {
            EnvFile::Empty => true,
            EnvFile::Single(_) => false,
            EnvFile::List(v) => v.is_empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// PortMapping
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PortMapping {
    Short(String),
    Long {
        target: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        published: Option<StringOrU16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocol: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host_ip: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app_protocol: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringOrU16 {
    String(String),
    Number(u16),
}

impl StringOrU16 {
    pub fn as_str_val(&self) -> String {
        match self {
            StringOrU16::String(s) => s.clone(),
            StringOrU16::Number(n) => n.to_string(),
        }
    }
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

    pub fn is_empty(&self) -> bool {
        match self {
            Labels::Empty => true,
            Labels::List(v) => v.is_empty(),
            Labels::Map(m) => m.is_empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// LoggingConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LoggingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Sysctls
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum Sysctls {
    #[default]
    Empty,
    List(Vec<String>),
    Map(IndexMap<String, serde_yaml::Value>),
}

impl Sysctls {
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
            Sysctls::Map(m) => m
                .iter()
                .map(|(k, v)| {
                    let s = match v {
                        serde_yaml::Value::String(s) => s.clone(),
                        serde_yaml::Value::Number(n) => n.to_string(),
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        _ => String::new(),
                    };
                    (k.clone(), s)
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// UlimitConfig
// ---------------------------------------------------------------------------

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
// DependsOn
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum DependsOn {
    #[default]
    Empty,
    List(Vec<String>),
    Map(IndexMap<String, DependsOnCondition>),
}

impl DependsOn {
    pub fn service_names(&self) -> Vec<String> {
        match self {
            DependsOn::Empty => vec![],
            DependsOn::List(v) => v.clone(),
            DependsOn::Map(m) => m.keys().cloned().collect(),
        }
    }

    pub fn condition_for(&self, service: &str) -> ServiceCondition {
        match self {
            DependsOn::Map(m) => m
                .get(service)
                .map(|c| c.condition.clone())
                .unwrap_or(ServiceCondition::ServiceStarted),
            _ => ServiceCondition::ServiceStarted,
        }
    }

    pub fn restart_for(&self, service: &str) -> bool {
        match self {
            DependsOn::Map(m) => m.get(service).and_then(|c| c.restart).unwrap_or(false),
            _ => false,
        }
    }

    pub fn required_for(&self, service: &str) -> bool {
        match self {
            DependsOn::Map(m) => m.get(service).and_then(|c| c.required).unwrap_or(true),
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DependsOnCondition {
    pub condition: ServiceCondition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCondition {
    #[default]
    ServiceStarted,
    ServiceHealthy,
    ServiceCompletedSuccessfully,
}

// ---------------------------------------------------------------------------
// HealthCheck
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct HealthCheck {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<Command>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_period: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_interval: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
}

impl HealthCheck {
    pub fn is_disabled(&self) -> bool {
        if self.disable.unwrap_or(false) {
            return true;
        }
        match &self.test {
            Some(Command::Exec(v)) if v.len() == 1 && v[0].eq_ignore_ascii_case("NONE") => true,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// BlkioConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct BlkioConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub weight_device: Vec<BlkioWeightDevice>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_read_bps: Vec<BlkioRateDevice>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_write_bps: Vec<BlkioRateDevice>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_read_iops: Vec<BlkioRateDevice>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_write_iops: Vec<BlkioRateDevice>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlkioWeightDevice {
    pub path: String,
    pub weight: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlkioRateDevice {
    pub path: String,
    pub rate: serde_yaml::Value,
}

impl BlkioRateDevice {
    /// Return rate as bytes/second (or IOPS as a plain integer).
    pub fn rate_value(&self) -> i64 {
        match &self.rate {
            serde_yaml::Value::Number(n) => n.as_i64().unwrap_or(0),
            serde_yaml::Value::String(s) => crate::size::parse_memory(s).unwrap_or(0),
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// LifecycleHook
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LifecycleHook {
    pub command: Command,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privileged: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub environment: EnvVars,
}

// ---------------------------------------------------------------------------
// RestartPolicy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum RestartPolicy {
    No,
    Always,
    OnFailure { max_attempts: Option<u32> },
    UnlessStopped,
}

impl<'de> Deserialize<'de> for RestartPolicy {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "no" => Ok(RestartPolicy::No),
            "always" => Ok(RestartPolicy::Always),
            "unless-stopped" => Ok(RestartPolicy::UnlessStopped),
            s if s == "on-failure" => Ok(RestartPolicy::OnFailure { max_attempts: None }),
            s if s.starts_with("on-failure:") => {
                let n = s["on-failure:".len()..]
                    .parse::<u32>()
                    .map_err(serde::de::Error::custom)?;
                Ok(RestartPolicy::OnFailure {
                    max_attempts: Some(n),
                })
            }
            other => Err(serde::de::Error::custom(format!(
                "invalid restart policy: {other}"
            ))),
        }
    }
}
