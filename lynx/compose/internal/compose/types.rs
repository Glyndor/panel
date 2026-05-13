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
    /// Compose file format version (legacy field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Project name (compose-spec, optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Files to merge in (compose-spec `include:`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<IncludeConfig>,

    /// Service definitions, keyed by service name.
    #[serde(default)]
    pub services: IndexMap<String, Service>,

    /// Top-level volumes section.
    #[serde(default)]
    pub volumes: IndexMap<String, Option<VolumeConfig>>,

    /// Top-level networks section.
    #[serde(default)]
    pub networks: IndexMap<String, Option<NetworkConfig>>,

    /// Top-level secrets section.
    #[serde(default)]
    pub secrets: IndexMap<String, SecretConfig>,

    /// Top-level configs section (compose-spec).
    #[serde(default)]
    pub configs: IndexMap<String, ConfigConfig>,
}

// ---------------------------------------------------------------------------
// Include
// ---------------------------------------------------------------------------

/// One entry in the top-level `include:` array (compose-spec).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum IncludeConfig {
    /// Short form: a path string.
    Path(String),
    /// Long form with explicit path / env_file / project_directory.
    Long {
        /// Compose file path(s) to include.
        path: StringOrList,
        /// Optional env_file(s) for the included project.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        env_file: Option<StringOrList>,
        /// Optional project directory for resolving relative paths.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project_directory: Option<String>,
    },
}

impl IncludeConfig {
    /// Return the include's path(s) as a flat list.
    pub fn paths(&self) -> Vec<String> {
        match self {
            IncludeConfig::Path(p) => vec![p.clone()],
            IncludeConfig::Long { path, .. } => path.to_list(),
        }
    }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/// A single service definition.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Service {
    // ---------------- core ----------------
    /// Container image reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,

    /// Build configuration (mutually compatible with `image`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildConfig>,

    /// Service-extension marker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<ExtendsConfig>,

    /// Container command (overrides image CMD).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Command>,

    /// Container entrypoint (overrides image ENTRYPOINT).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<Command>,

    // ---------------- ports / network ----------------
    /// Published ports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortMapping>,

    /// Exposed (but not published) ports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expose: Vec<String>,

    // ---------------- env / mounts ----------------
    /// Environment variables.
    #[serde(default)]
    pub environment: EnvVars,

    /// Path(s) to env files loaded into the container.
    #[serde(default)]
    pub env_file: StringOrList,

    /// Volume / bind / tmpfs mounts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<VolumeMount>,

    /// Tmpfs paths shorthand (`tmpfs: /run` or `tmpfs: [/run, /tmp]`).
    #[serde(default)]
    pub tmpfs: StringOrList,

    /// Re-use volumes from another container.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes_from: Vec<String>,

    /// Compose-spec `configs:` references attached to this service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub configs: Vec<ServiceConfigRef>,

    /// Compose-spec `secrets:` references attached to this service.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<ServiceSecretRef>,

    // ---------------- networking ----------------
    /// Network attachments (list or per-network map).
    #[serde(default)]
    pub networks: ServiceNetworks,

    /// Container hostname.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,

    /// Container DNS domain name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domainname: Option<String>,

    /// Container MAC address (default for all networks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,

    /// Compose-style aliases for legacy linking.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,

    /// External hosts to inject into `/etc/hosts`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_hosts: Vec<String>,

    /// DNS servers.
    #[serde(default)]
    pub dns: StringOrList,

    /// DNS search domains.
    #[serde(default)]
    pub dns_search: StringOrList,

    /// Mode override for the network namespace (e.g. `host`, `none`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_mode: Option<String>,

    // ---------------- ordering / health ----------------
    /// Service dependencies.
    #[serde(default)]
    pub depends_on: DependsOn,

    /// Healthcheck configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub healthcheck: Option<HealthCheck>,

    // ---------------- lifecycle / restart ----------------
    /// Restart policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<RestartPolicy>,

    /// Signal sent on stop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_signal: Option<String>,

    /// Time to wait for graceful stop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_grace_period: Option<String>,

    /// Profiles in which this service is active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<String>,

    // ---------------- identity / labels ----------------
    /// User-defined labels.
    #[serde(default)]
    pub labels: Labels,

    /// OCI annotations (Podman-specific, list or map).
    #[serde(default)]
    pub annotations: Labels,

    /// Explicit container name override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,

    /// User to run the container as.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// Working directory inside the container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Additional groups for the container user.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_add: Vec<String>,

    /// Platform string (e.g. `linux/amd64`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    // ---------------- security / capabilities ----------------
    /// Linux capabilities to add.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cap_add: Vec<String>,

    /// Linux capabilities to drop.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cap_drop: Vec<String>,

    /// Security options (e.g. `["no-new-privileges:true"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_opt: Vec<String>,

    /// Mount root filesystem read-only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,

    /// Run with elevated privileges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privileged: Option<bool>,

    /// Use an init process (PID 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init: Option<bool>,

    /// Allocate a TTY.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tty: Option<bool>,

    /// Keep STDIN open even when nothing is attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdin_open: Option<bool>,

    // ---------------- runtime / namespaces ----------------
    /// Container runtime (e.g. `nvidia`, `io.containerd.runc.v2`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,

    /// Shared memory size (e.g. `64m`, `1g`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shm_size: Option<String>,

    /// User namespace mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userns_mode: Option<String>,

    /// PID namespace mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<String>,

    /// IPC namespace mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipc: Option<String>,

    /// Cgroup parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroup_parent: Option<String>,

    /// Cgroup namespace mode (`host` or `private`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cgroup: Option<String>,

    // ---------------- devices / filesystem ----------------
    /// Host device mappings (`/dev/sda:/dev/xvda:rwm`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<String>,

    /// Storage driver options.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub storage_opt: HashMap<String, String>,

    // ---------------- resources / limits (top-level) ----------------
    /// Number of replicas (legacy alias for `deploy.replicas`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<u32>,

    /// CPU shares (relative weight).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_shares: Option<u64>,

    /// CFS quota (microseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_quota: Option<i64>,

    /// CFS period (microseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_period: Option<u64>,

    /// CPUs in cpuset format (e.g. `0-3`, `0,1`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpuset: Option<String>,

    /// Memory limit (e.g. `128m`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_limit: Option<String>,

    /// Total memory + swap limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memswap_limit: Option<String>,

    /// Soft memory reservation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_reservation: Option<String>,

    /// Disable the OOM killer for this container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oom_kill_disable: Option<bool>,

    /// OOM score adjustment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oom_score_adj: Option<i64>,

    // ---------------- logging / sysctl / ulimit ----------------
    /// Logging driver and options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingConfig>,

    /// Kernel sysctls inside the container.
    #[serde(default)]
    pub sysctls: Sysctls,

    /// Per-resource ulimit overrides.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub ulimits: IndexMap<String, UlimitConfig>,

    // ---------------- deploy ----------------
    /// Deploy section (Swarm / compose-spec).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deploy: Option<DeployConfig>,
}

// ---------------------------------------------------------------------------
// Extends
// ---------------------------------------------------------------------------

/// `extends:` configuration on a service.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ExtendsConfig {
    /// Short form — service name in the same file.
    Service(String),
    /// Long form with optional `file:`.
    Long {
        /// Service name to extend.
        service: String,
        /// File path containing the base service (defaults to current file).
        #[serde(skip_serializing_if = "Option::is_none")]
        file: Option<String>,
    },
}

impl ExtendsConfig {
    /// Return the base service name.
    pub fn service(&self) -> &str {
        match self {
            ExtendsConfig::Service(s) => s,
            ExtendsConfig::Long { service, .. } => service,
        }
    }

    /// Return the optional file path of the base service.
    pub fn file(&self) -> Option<&str> {
        match self {
            ExtendsConfig::Service(_) => None,
            ExtendsConfig::Long { file, .. } => file.as_deref(),
        }
    }
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
    /// Long form with optional dockerfile / args / target etc.
    Config {
        /// Build context directory (relative to the compose file).
        context: String,
        /// Dockerfile path (relative to context).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dockerfile: Option<String>,
        /// Build args (passed via `--build-arg`).
        #[serde(default)]
        args: EnvVars,
        /// Target stage for multi-stage builds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        /// Cache source images.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cache_from: Vec<String>,
        /// Image labels added to the built image.
        #[serde(default)]
        labels: Labels,
        /// Shared memory size for build steps.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shm_size: Option<String>,
        /// Network mode used during build.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        network: Option<String>,
        /// Target build platforms (`linux/amd64`, `linux/arm64`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        platforms: Vec<String>,
        /// Additional named build contexts.
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        additional_contexts: HashMap<String, String>,
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

    /// Return the build arguments.
    pub fn args(&self) -> EnvVars {
        match self {
            BuildConfig::Context(_) => EnvVars::Empty,
            BuildConfig::Config { args, .. } => args.clone(),
        }
    }

    /// Return the target stage, if any.
    pub fn target(&self) -> Option<&str> {
        match self {
            BuildConfig::Context(_) => None,
            BuildConfig::Config { target, .. } => target.as_deref(),
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
    /// Single string — invoked via `sh -c`.
    Shell(String),
    /// Exec form (already split into args).
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

    /// Convert to a flat argument list, preserving the shell vs. exec split.
    pub fn to_argv(&self) -> Vec<String> {
        match self {
            Command::Shell(s) => vec![s.clone()],
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
    /// No environment variables specified.
    #[default]
    Empty,
    /// List form: `["KEY=value", "BARE"]`.
    List(Vec<String>),
    /// Map form: `{KEY: value, BARE: ~}`.
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
                        serde_yaml::Value::Null => None,
                        _ => None,
                    });
                    (k.clone(), val)
                })
                .collect(),
        }
    }

    /// Returns true if no entries are present.
    pub fn is_empty(&self) -> bool {
        match self {
            EnvVars::Empty => true,
            EnvVars::List(v) => v.is_empty(),
            EnvVars::Map(m) => m.is_empty(),
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
    /// Field absent.
    #[default]
    Empty,
    /// Single value form.
    Single(String),
    /// List form.
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

    /// Returns true if no entries are present.
    pub fn is_empty(&self) -> bool {
        match self {
            StringOrList::Empty => true,
            StringOrList::Single(s) => s.is_empty(),
            StringOrList::List(v) => v.is_empty(),
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
    /// Short string form (`"8080:80"`, `"80/udp"`, ...).
    Short(String),
    /// Long object form.
    Long {
        /// Container-side port.
        target: u16,
        /// Optional published host port.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        published: Option<StringOrU16>,
        /// Protocol (`tcp`/`udp`/`sctp`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocol: Option<String>,
        /// Host IP to bind to.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host_ip: Option<String>,
        /// Bind mode (`host`, `ingress`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        /// Application protocol hint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        app_protocol: Option<String>,
        /// User-friendly name for this mapping.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

/// A value that may be either a string or a plain u16.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StringOrU16 {
    /// String form (allows ranges like `8000-8010`).
    String(String),
    /// Numeric port.
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
    /// Short form (`./data:/var/lib/data:ro`).
    Short(String),
    /// Long object form.
    Long {
        /// Mount type tag (`volume`, `bind`, `tmpfs`).
        #[serde(rename = "type")]
        volume_type: VolumeType,
        /// Source path or volume name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        /// Target inside the container.
        target: String,
        /// Read-only flag.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        read_only: Option<bool>,
        /// Bind-specific options.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bind: Option<BindOptions>,
        /// Volume-specific options.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volume: Option<VolumeOptions>,
        /// Tmpfs-specific options.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tmpfs: Option<TmpfsOptions>,
        /// Consistency level (Docker compat: `consistent`, `cached`, `delegated`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        consistency: Option<String>,
    },
}

impl VolumeMount {
    /// Return the container-side mount target path.
    pub fn target(&self) -> &str {
        match self {
            VolumeMount::Short(s) => {
                let parts: Vec<&str> = s.splitn(3, ':').collect();
                if parts.len() >= 2 {
                    parts[1]
                } else {
                    parts[0]
                }
            }
            VolumeMount::Long { target, .. } => target,
        }
    }
}

/// Volume type tag.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VolumeType {
    /// Named or anonymous volume.
    Volume,
    /// Bind mount of a host path.
    Bind,
    /// In-memory tmpfs.
    Tmpfs,
    /// NPipe (Windows-only, pass through unchanged).
    Npipe,
    /// Cluster-managed volume.
    Cluster,
}

/// Options for a bind mount (long form).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct BindOptions {
    /// Mount propagation mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub propagation: Option<String>,
    /// Auto-create the host path if missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_host_path: Option<bool>,
    /// SELinux relabel: `z` (shared) or `Z` (private).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selinux: Option<String>,
}

/// Options for a named volume mount (long form).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VolumeOptions {
    /// Disable copying contents from the image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nocopy: Option<bool>,
    /// Labels applied to the volume.
    #[serde(default)]
    pub labels: Labels,
    /// Optional driver configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_config: Option<DriverConfig>,
    /// Optional subpath inside the volume.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
}

/// Driver configuration for a volume mount.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DriverConfig {
    /// Driver name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Driver options.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,
}

/// Options for a tmpfs mount (long form).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TmpfsOptions {
    /// Tmpfs size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// File mode (octal).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

// ---------------------------------------------------------------------------
// Service-level configs / secrets references
// ---------------------------------------------------------------------------

/// Reference from a service to a top-level `configs:` entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ServiceConfigRef {
    /// Short form — just the name.
    Short(String),
    /// Long form with mount options.
    Long {
        /// Source name (top-level configs key).
        source: String,
        /// Optional target path inside the container.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        /// File owner UID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uid: Option<String>,
        /// File owner GID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gid: Option<String>,
        /// File mode (octal as u32).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
    },
}

impl ServiceConfigRef {
    /// Return the source name.
    pub fn source(&self) -> &str {
        match self {
            ServiceConfigRef::Short(s) => s,
            ServiceConfigRef::Long { source, .. } => source,
        }
    }

    /// Return the explicit target path, if any.
    pub fn target(&self) -> Option<&str> {
        match self {
            ServiceConfigRef::Short(_) => None,
            ServiceConfigRef::Long { target, .. } => target.as_deref(),
        }
    }
}

/// Reference from a service to a top-level `secrets:` entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ServiceSecretRef {
    /// Short form — just the name.
    Short(String),
    /// Long form with mount options.
    Long {
        /// Source name (top-level secrets key).
        source: String,
        /// Optional target path inside the container.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
        /// File owner UID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        uid: Option<String>,
        /// File owner GID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gid: Option<String>,
        /// File mode (octal as u32).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<u32>,
    },
}

impl ServiceSecretRef {
    /// Return the source name.
    pub fn source(&self) -> &str {
        match self {
            ServiceSecretRef::Short(s) => s,
            ServiceSecretRef::Long { source, .. } => source,
        }
    }

    /// Return the explicit target path, if any.
    pub fn target(&self) -> Option<&str> {
        match self {
            ServiceSecretRef::Short(_) => None,
            ServiceSecretRef::Long { target, .. } => target.as_deref(),
        }
    }
}

// ---------------------------------------------------------------------------
// Networks
// ---------------------------------------------------------------------------

/// Service-level network attachment — list or map form.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum ServiceNetworks {
    /// No explicit attachment.
    #[default]
    Empty,
    /// List form.
    List(Vec<String>),
    /// Map form: per-network configuration.
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

    /// Return per-network configuration for a named network, if any.
    pub fn config_for(&self, name: &str) -> Option<&ServiceNetworkConfig> {
        match self {
            ServiceNetworks::Map(m) => m.get(name).and_then(|c| c.as_ref()),
            _ => None,
        }
    }
}

/// Per-network configuration for a service attachment.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ServiceNetworkConfig {
    /// DNS aliases on this network.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>,
    /// Static IPv4 address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_address: Option<String>,
    /// Static IPv6 address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6_address: Option<String>,
    /// Link-local IPs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_local_ips: Vec<String>,
    /// Endpoint priority (lower = preferred).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
    /// Per-network MAC address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    /// Driver-specific options.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub driver_opts: HashMap<String, String>,
    /// User-defined gateway priority.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gw_priority: Option<u32>,
}

// ---------------------------------------------------------------------------
// depends_on
// ---------------------------------------------------------------------------

/// `depends_on` — list or condition-map form.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum DependsOn {
    /// Empty.
    #[default]
    Empty,
    /// Simple list.
    List(Vec<String>),
    /// Map form with conditions.
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

    /// Return whether a dependency was declared as `restart: true` (re-create when dep restarts).
    pub fn restart_for(&self, service: &str) -> bool {
        match self {
            DependsOn::Map(m) => m.get(service).and_then(|c| c.restart).unwrap_or(false),
            _ => false,
        }
    }

    /// Return whether a dependency is required (defaults to true per spec).
    pub fn required_for(&self, service: &str) -> bool {
        match self {
            DependsOn::Map(m) => m.get(service).and_then(|c| c.required).unwrap_or(true),
            _ => true,
        }
    }
}

/// One entry in the map form of `depends_on`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DependsOnCondition {
    /// Readiness condition.
    pub condition: ServiceCondition,
    /// Whether to restart this service when the dependency restarts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<bool>,
    /// Whether the dependency is required (defaults to true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Readiness condition for a dependency.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCondition {
    /// Wait for container start.
    #[default]
    ServiceStarted,
    /// Wait for healthcheck `healthy`.
    ServiceHealthy,
    /// Wait for the container to exit successfully.
    ServiceCompletedSuccessfully,
}

// ---------------------------------------------------------------------------
// Healthcheck
// ---------------------------------------------------------------------------

/// Service healthcheck configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct HealthCheck {
    /// The healthcheck test command.  May be:
    /// - `["NONE"]` — disable the inherited check.
    /// - `["CMD", ...]` — exec form.
    /// - `["CMD-SHELL", "string"]` — shell form.
    /// - shell string — equivalent to `CMD-SHELL`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<Command>,
    /// Interval between checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<String>,
    /// Per-check timeout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Retries before becoming unhealthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,
    /// Initial grace period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_period: Option<String>,
    /// Interval used during the `start_period`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_interval: Option<String>,
    /// Disable healthcheck entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable: Option<bool>,
}

impl HealthCheck {
    /// Return true when this healthcheck is effectively disabled
    /// (either `disable: true` or `test: ["NONE"]`).
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
// Restart policy
// ---------------------------------------------------------------------------

/// Container restart policy.
///
/// Accepts `no`, `always`, `on-failure`, `on-failure:N`, `unless-stopped`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum RestartPolicy {
    /// Do not restart.
    No,
    /// Always restart.
    Always,
    /// Restart on non-zero exit, optionally with a max retry count.
    OnFailure {
        /// Maximum retry attempts.
        max_attempts: Option<u32>,
    },
    /// Restart unless explicitly stopped.
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

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

/// Labels — list or map form.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum Labels {
    /// No labels.
    #[default]
    Empty,
    /// List form (`["KEY=value"]`).
    List(Vec<String>),
    /// Map form.
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

    /// True when no entries are present.
    pub fn is_empty(&self) -> bool {
        match self {
            Labels::Empty => true,
            Labels::List(v) => v.is_empty(),
            Labels::Map(m) => m.is_empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

/// Logging driver configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LoggingConfig {
    /// Logging driver name (`json-file`, `journald`, ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    /// Driver-specific options.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Sysctls
// ---------------------------------------------------------------------------

/// `sysctls` accepts either a list (`["net.core.somaxconn=1024"]`) or a map.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(untagged)]
pub enum Sysctls {
    /// Empty.
    #[default]
    Empty,
    /// List form.
    List(Vec<String>),
    /// Map form.
    Map(IndexMap<String, serde_yaml::Value>),
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
// Ulimits
// ---------------------------------------------------------------------------

/// Ulimit value — either a single number (soft == hard) or an explicit soft/hard pair.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum UlimitConfig {
    /// Single value applied to both soft and hard limits.
    Single(i64),
    /// Explicit soft / hard pair.
    Pair {
        /// Soft limit.
        soft: i64,
        /// Hard limit.
        hard: i64,
    },
}

impl UlimitConfig {
    /// Soft limit.
    pub fn soft(&self) -> i64 {
        match self {
            UlimitConfig::Single(n) => *n,
            UlimitConfig::Pair { soft, .. } => *soft,
        }
    }

    /// Hard limit.
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
    /// Number of replicas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replicas: Option<u32>,
    /// Resource limits / reservations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesConfig>,
    /// Restart policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_policy: Option<DeployRestartPolicy>,
    /// Update policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_config: Option<DeployUpdateConfig>,
    /// Rollback policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_config: Option<DeployUpdateConfig>,
    /// Endpoint mode (`vip`, `dnsrr`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_mode: Option<String>,
    /// Deploy mode (`replicated`, `global`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Service labels.
    #[serde(default)]
    pub labels: Labels,
    /// Placement constraints / preferences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<DeployPlacement>,
}

/// Deploy update / rollback policy.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeployUpdateConfig {
    /// Number of containers to update at a time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallelism: Option<u32>,
    /// Delay between updates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<String>,
    /// Failure action (`continue`, `pause`, `rollback`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_action: Option<String>,
    /// Monitoring duration after each update.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
    /// Maximum failure ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_failure_ratio: Option<f64>,
    /// Order: `start-first` or `stop-first`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<String>,
}

/// Deploy placement constraints.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeployPlacement {
    /// Constraints (`node.role==manager`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<String>,
    /// Preferences (placement preferences).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferences: Vec<serde_yaml::Value>,
    /// Maximum replicas per node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_replicas_per_node: Option<u32>,
}

/// Resource limits and reservations.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ResourcesConfig {
    /// Hard limits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<ResourceSpec>,
    /// Soft reservations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reservations: Option<ResourceSpec>,
}

/// A single resource specification (CPUs + memory).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ResourceSpec {
    /// CPU cores (e.g. `"0.5"`, `"2"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<String>,
    /// Memory limit (e.g. `"128M"`, `"1G"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    /// PIDs limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids: Option<u64>,
}

/// Restart policy inside `deploy:`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeployRestartPolicy {
    /// `none`, `on-failure`, `any`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Delay before restart.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<String>,
    /// Maximum restart attempts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    /// Time window for evaluating attempts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<String>,
}

// ---------------------------------------------------------------------------
// Top-level resource configs
// ---------------------------------------------------------------------------

/// Named volume configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VolumeConfig {
    /// Volume driver.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    /// Driver options.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub driver_opts: HashMap<String, String>,
    /// Treat as external (don't create).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    /// Override volume name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Labels.
    #[serde(default)]
    pub labels: Labels,
}

/// Named network configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NetworkConfig {
    /// Network driver (`bridge`, `host`, `overlay`, `macvlan`, ...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    /// Driver options.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub driver_opts: HashMap<String, String>,
    /// Treat as external.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    /// Override network name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Internal-only network (no outbound).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal: Option<bool>,
    /// Enable IPv6.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_ipv6: Option<bool>,
    /// Attachable from CLI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachable: Option<bool>,
    /// IPAM configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipam: Option<IpamConfig>,
    /// Labels.
    #[serde(default)]
    pub labels: Labels,
}

/// IPAM configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct IpamConfig {
    /// IPAM driver name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    /// IPAM configurations (subnets/gateways).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<IpamPool>,
    /// Driver options.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub options: HashMap<String, String>,
}

/// IPAM subnet pool.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct IpamPool {
    /// Subnet CIDR.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subnet: Option<String>,
    /// Gateway address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    /// IP range CIDR.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_range: Option<String>,
    /// Auxiliary addresses.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub aux_addresses: HashMap<String, String>,
}

/// Named secret configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SecretConfig {
    /// Path to the secret file on the host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Treat as external.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    /// Override secret name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Inline content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Environment variable holding the secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// Driver name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    /// Driver options.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub driver_opts: HashMap<String, String>,
    /// Labels.
    #[serde(default)]
    pub labels: Labels,
}

/// Top-level config entry (compose-spec `configs:`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ConfigConfig {
    /// Path to a file on the host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Treat as external.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    /// Override config name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Inline config content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Environment variable holding the value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// Labels.
    #[serde(default)]
    pub labels: Labels,
}
