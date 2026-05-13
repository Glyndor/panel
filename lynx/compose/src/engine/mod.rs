//! Container orchestration engine.
//!
//! Translates a parsed `ComposeFile` into Podman API calls via bollard.

use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, LogOutput, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::{BuildImageOptions, CreateImageOptions};
use bollard::models::{
    EndpointSettings, HostConfig, HostConfigLogConfig, NetworkingConfig,
    RestartPolicy, RestartPolicyNameEnum, Ulimit,
};
use bollard::network::{ConnectNetworkOptions, CreateNetworkOptions};
use bollard::volume::CreateVolumeOptions;
use bytes::Bytes;
use flate2::write::GzEncoder;
use flate2::Compression;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

use crate::compose::types::{
    ComposeFile, RestartPolicy as ComposeRestart, SecretConfig, Service,
    ServiceCondition, VolumeMount,
};
use crate::error::{ComposeError, Result};
use crate::ports;

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// The orchestration engine that drives container lifecycle.
pub struct Engine {
    docker: Docker,
    project: String,
    /// Base directory for resolving relative `env_file:` paths.
    base_dir: std::path::PathBuf,
}

impl Engine {
    /// Create a new engine for the given project name.
    ///
    /// `base_dir` is the directory containing the compose file; used to
    /// resolve relative `env_file:` paths.
    pub fn new(docker: Docker, project: String) -> Self {
        Self {
            docker,
            project,
            base_dir: std::env::current_dir().unwrap_or_default(),
        }
    }

    /// Create an engine with an explicit base directory.
    pub fn with_base_dir(docker: Docker, project: String, base_dir: std::path::PathBuf) -> Self {
        Self { docker, project, base_dir }
    }

    // -----------------------------------------------------------------------
    // Public commands
    // -----------------------------------------------------------------------

    /// Start all services defined in `file` (in dependency order).
    pub async fn up(&self, file: &ComposeFile) -> Result<()> {
        self.up_with_options(file, false).await
    }

    /// Start all services, with an option to detach immediately.
    pub async fn up_with_options(&self, file: &ComposeFile, _detach: bool) -> Result<()> {
        let order = crate::compose::resolve_order(file)?;

        self.create_networks(file).await?;
        self.create_volumes(file).await?;

        for name in &order {
            let service = &file.services[name];

            // Wait for healthy dependencies if requested.
            for dep in service.depends_on.service_names() {
                let condition = service.depends_on.condition_for(&dep);
                if condition == ServiceCondition::ServiceHealthy {
                    let dep_service = &file.services[&dep];
                    let dep_container = self.container_name(&dep, dep_service);
                    self.wait_healthy(&dep_container, dep_service).await?;
                }
            }

            // Build image if needed.
            if service.build.is_some() {
                self.build_service(name, service).await?;
            } else {
                self.pull_image(service).await?;
            }

            let container_name = self.container_name(name, service);
            self.create_and_start(&container_name, name, service, file).await?;

            // Connect to additional networks.
            self.connect_extra_networks(&container_name, name, service, file).await?;

            info!("started {container_name}");
        }

        Ok(())
    }

    /// Stop and remove all containers for this project.
    pub async fn down(&self, file: &ComposeFile) -> Result<()> {
        self.down_with_options(file, false).await
    }

    /// Stop and remove all containers, optionally also removing volumes.
    pub async fn down_with_options(&self, file: &ComposeFile, remove_volumes: bool) -> Result<()> {
        let mut order = crate::compose::resolve_order(file)?;
        order.reverse();

        for name in &order {
            let service = &file.services[name];
            let container_name = self.container_name(name, service);

            let _ = self
                .docker
                .stop_container(&container_name, Some(StopContainerOptions { t: 10 }))
                .await;

            let _ = self
                .docker
                .remove_container(
                    &container_name,
                    Some(RemoveContainerOptions {
                        force: true,
                        v: remove_volumes,
                        ..Default::default()
                    }),
                )
                .await;

            info!("removed {container_name}");
        }

        Ok(())
    }

    /// List all containers belonging to this project.
    pub async fn ps(&self, _file: &ComposeFile) -> Result<()> {
        let label = format!("lynx.compose.project={}", self.project);
        let mut filters: HashMap<String, Vec<String>> = HashMap::new();
        filters.insert("label".to_string(), vec![label]);

        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await?;

        println!("{:<40} {:<30} {:<20}", "NAME", "IMAGE", "STATUS");
        for c in containers {
            let names = c
                .names
                .unwrap_or_default()
                .join(", ")
                .trim_start_matches('/')
                .to_string();
            let image = c.image.unwrap_or_default();
            let status = c.status.unwrap_or_default();
            let ports = c
                .ports
                .unwrap_or_default()
                .iter()
                .map(|p| {
                    format!(
                        "{}:{}->{}",
                        p.ip.as_deref().unwrap_or(""),
                        p.public_port.unwrap_or(0),
                        p.private_port
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            println!("{names:<40} {image:<30} {status:<20} {ports}");
        }

        Ok(())
    }

    /// Stream logs for a service (or all services if `service_name` is `None`).
    pub async fn logs(
        &self,
        file: &ComposeFile,
        service_name: Option<&str>,
        follow: bool,
    ) -> Result<()> {
        let targets: Vec<String> = if let Some(svc) = service_name {
            let service = file
                .services
                .get(svc)
                .ok_or_else(|| ComposeError::ServiceNotFound(svc.into()))?;
            vec![self.container_name(svc, service)]
        } else {
            file.services
                .iter()
                .map(|(n, s)| self.container_name(n, s))
                .collect()
        };

        for container_name in targets {
            let mut stream = self.docker.logs(
                &container_name,
                Some(LogsOptions::<String> {
                    stdout: true,
                    stderr: true,
                    follow,
                    ..Default::default()
                }),
            );

            while let Some(msg) = stream.next().await {
                match msg? {
                    LogOutput::StdOut { message } => {
                        print!("{}", String::from_utf8_lossy(&message));
                    }
                    LogOutput::StdErr { message } => {
                        eprint!("{}", String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Execute a command in a running service container.
    pub async fn exec(
        &self,
        file: &ComposeFile,
        service_name: &str,
        cmd: Vec<String>,
    ) -> Result<()> {
        let service = file
            .services
            .get(service_name)
            .ok_or_else(|| ComposeError::ServiceNotFound(service_name.into()))?;
        let container_name = self.container_name(service_name, service);

        let exec_id = self
            .docker
            .create_exec(
                &container_name,
                CreateExecOptions::<String> {
                    cmd: Some(cmd),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    attach_stdin: Some(true),
                    tty: Some(true),
                    ..Default::default()
                },
            )
            .await?
            .id;

        match self.docker.start_exec(&exec_id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                while let Some(msg) = output.next().await {
                    match msg? {
                        LogOutput::StdOut { message } => {
                            print!("{}", String::from_utf8_lossy(&message));
                        }
                        LogOutput::StdErr { message } => {
                            eprint!("{}", String::from_utf8_lossy(&message));
                        }
                        _ => {}
                    }
                }
            }
            StartExecResults::Detached => {}
        }

        Ok(())
    }

    /// Pull all images for all services in parallel.
    pub async fn pull(&self, file: &ComposeFile) -> Result<()> {
        let futs: Vec<_> = file
            .services
            .values()
            .filter(|s| s.image.is_some())
            .map(|s| self.pull_image(s))
            .collect();

        let results = futures::future::join_all(futs).await;
        for r in results {
            r?;
        }
        Ok(())
    }

    /// Restart one or all services.
    pub async fn restart(&self, file: &ComposeFile, service_name: Option<&str>) -> Result<()> {
        let names: Vec<String> = if let Some(svc) = service_name {
            if !file.services.contains_key(svc) {
                return Err(ComposeError::ServiceNotFound(svc.into()));
            }
            vec![svc.to_string()]
        } else {
            file.services.keys().cloned().collect()
        };

        for name in &names {
            let service = &file.services[name];
            let container_name = self.container_name(name, service);

            let _ = self
                .docker
                .stop_container(&container_name, Some(StopContainerOptions { t: 10 }))
                .await;

            self.docker
                .start_container(&container_name, None::<StartContainerOptions<String>>)
                .await?;

            info!("restarted {container_name}");
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    async fn create_networks(&self, file: &ComposeFile) -> Result<()> {
        for (name, config) in &file.networks {
            let network_name = config
                .as_ref()
                .and_then(|c| c.name.as_deref())
                .unwrap_or(name);

            let external = config.as_ref().and_then(|c| c.external).unwrap_or(false);
            if external {
                continue;
            }

            let driver = config
                .as_ref()
                .and_then(|c| c.driver.clone())
                .unwrap_or_else(|| "bridge".into());

            let options = CreateNetworkOptions {
                name: network_name,
                driver: driver.as_str(),
                ..Default::default()
            };

            match self.docker.create_network(options).await {
                Ok(_) => info!("created network {network_name}"),
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 409, ..
                }) => {
                    // Already exists — fine.
                }
                Err(e) => return Err(ComposeError::Podman(e)),
            }
        }
        Ok(())
    }

    async fn create_volumes(&self, file: &ComposeFile) -> Result<()> {
        for (name, config) in &file.volumes {
            let external = config.as_ref().and_then(|c| c.external).unwrap_or(false);
            if external {
                continue;
            }

            let volume_name = config
                .as_ref()
                .and_then(|c| c.name.as_deref())
                .unwrap_or(name);

            let options = CreateVolumeOptions {
                name: volume_name,
                ..Default::default()
            };

            match self.docker.create_volume(options).await {
                Ok(_) => info!("created volume {volume_name}"),
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 409, ..
                }) => {}
                Err(e) => return Err(ComposeError::Podman(e)),
            }
        }
        Ok(())
    }

    async fn pull_image(&self, service: &Service) -> Result<()> {
        let image = match &service.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        info!("pulling {image}");

        let mut stream = self.docker.create_image(
            Some(CreateImageOptions {
                from_image: image.as_str(),
                ..Default::default()
            }),
            None,
            None,
        );

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        debug!("{status}");
                    }
                }
                Err(e) => warn!("pull warning: {e}"),
            }
        }

        Ok(())
    }

    /// Build an image for a service that has a `build:` section.
    async fn build_service(&self, service_name: &str, service: &Service) -> Result<()> {
        let build = match &service.build {
            Some(b) => b,
            None => return Ok(()),
        };

        let context_path = self.base_dir.join(build.context());
        let dockerfile = build.dockerfile().unwrap_or("Dockerfile");
        let tag = service
            .image
            .clone()
            .unwrap_or_else(|| format!("{}:latest", service_name));

        info!("building {tag} from {}", context_path.display());

        let tar_bytes = build_context_tar(&context_path, dockerfile)?;

        let options = BuildImageOptions {
            dockerfile,
            t: tag.as_str(),
            rm: true,
            ..Default::default()
        };

        let body = Bytes::from(tar_bytes);
        let mut stream = self.docker.build_image(options, None, Some(body));

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(stream_msg) = info.stream {
                        print!("{stream_msg}");
                    }
                    if let Some(err) = info.error {
                        return Err(ComposeError::Build(err));
                    }
                }
                Err(e) => return Err(ComposeError::Podman(e)),
            }
        }

        Ok(())
    }

    async fn create_and_start(
        &self,
        container_name: &str,
        service_name: &str,
        service: &Service,
        file: &ComposeFile,
    ) -> Result<()> {
        let image = service
            .image
            .as_deref()
            .ok_or_else(|| ComposeError::NoImageOrBuild(service_name.into()))?;

        // --- environment ---
        let env = build_env(service, &self.base_dir);

        // --- binds ---
        let binds = build_binds(service);

        // --- secrets mounts ---
        let secret_binds = build_secret_binds(service, file);
        let all_binds: Vec<String> = binds.into_iter().chain(secret_binds).collect();

        // --- ports ---
        let parsed_ports = ports::parse_ports(&service.ports)?;
        let (port_bindings, exposed_ports) = ports::to_bollard(&parsed_ports);

        // --- restart policy ---
        let restart_policy = build_restart_policy(service);

        // --- logging ---
        let log_config = build_log_config(service);

        // --- network mode ---
        let (network_mode, first_network) = resolve_network_mode(service, file);

        // --- labels with project tracking ---
        let mut labels = service.labels.to_map();
        labels.insert(
            "lynx.compose.project".to_string(),
            self.project.clone(),
        );
        labels.insert(
            "lynx.compose.service".to_string(),
            service_name.to_string(),
        );

        // --- ulimits ---
        let ulimits: Vec<Ulimit> = service
            .ulimits
            .iter()
            .map(|(name, cfg)| Ulimit {
                name: Some(name.clone()),
                soft: Some(cfg.soft()),
                hard: Some(cfg.hard()),
            })
            .collect();

        // --- sysctls ---
        let sysctls: HashMap<String, String> = service.sysctls.to_map();

        // --- extra_hosts ---
        let extra_hosts: Vec<String> = service.extra_hosts.clone();

        // --- dns ---
        let dns = service.dns.to_list();
        let dns_search = service.dns_search.to_list();

        let host_config = HostConfig {
            binds: if all_binds.is_empty() {
                None
            } else {
                Some(all_binds)
            },
            network_mode: network_mode.clone(),
            restart_policy,
            port_bindings: if port_bindings.is_empty() {
                None
            } else {
                Some(port_bindings)
            },
            cap_add: if service.cap_add.is_empty() {
                None
            } else {
                Some(service.cap_add.clone())
            },
            cap_drop: if service.cap_drop.is_empty() {
                None
            } else {
                Some(service.cap_drop.clone())
            },
            sysctls: if sysctls.is_empty() {
                None
            } else {
                Some(sysctls)
            },
            ulimits: if ulimits.is_empty() {
                None
            } else {
                Some(ulimits)
            },
            extra_hosts: if extra_hosts.is_empty() {
                None
            } else {
                Some(extra_hosts)
            },
            dns: if dns.is_empty() { None } else { Some(dns) },
            dns_search: if dns_search.is_empty() {
                None
            } else {
                Some(dns_search)
            },
            init: service.init,
            privileged: service.privileged,
            log_config,
            pid_mode: service.pid.clone(),
            ipc_mode: service.ipc.clone(),
            ..Default::default()
        };

        let cmd = service.command.as_ref().map(|c| c.to_exec());

        let networking_config = first_network.map(|net| {
            let mut endpoints = HashMap::new();
            endpoints.insert(net, EndpointSettings::default());
            NetworkingConfig {
                endpoints_config: Some(endpoints),
            }
        });

        let config = Config::<String> {
            image: Some(image.to_string()),
            env: if env.is_empty() {
                None
            } else {
                Some(env)
            },
            cmd,
            host_config: Some(host_config),
            labels: Some(labels.into_iter().collect()),
            exposed_ports: if exposed_ports.is_empty() {
                None
            } else {
                Some(exposed_ports.into_iter().map(|(k, _)| (k, HashMap::new())).collect())
            },
            tty: service.tty,
            open_stdin: service.stdin_open,
            user: service.user.clone(),
            working_dir: service.working_dir.clone(),
            stop_signal: service.stop_signal.clone(),
            networking_config,
            ..Default::default()
        };

        // Remove any pre-existing container with the same name.
        let _ = self
            .docker
            .remove_container(
                container_name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await;

        self.docker
            .create_container(
                Some(CreateContainerOptions::<&str> {
                    name: container_name,
                    platform: service.platform.as_deref(),
                }),
                config,
            )
            .await?;

        self.docker
            .start_container(container_name, None::<StartContainerOptions<String>>)
            .await?;

        Ok(())
    }

    /// Connect a container to additional networks (beyond the first).
    async fn connect_extra_networks(
        &self,
        container_name: &str,
        _service_name: &str,
        service: &Service,
        file: &ComposeFile,
    ) -> Result<()> {
        // If network_mode is set, skip custom network attachment.
        if service.network_mode.is_some() {
            return Ok(());
        }

        let network_names = service.networks.names();
        // The first network was connected during container creation via
        // NetworkingConfig; connect the rest here.
        for network in network_names.iter().skip(1) {
            let full_name = resolve_network_name(network, file);
            self.docker
                .connect_network(
                    &full_name,
                    ConnectNetworkOptions {
                        container: container_name,
                        endpoint_config: EndpointSettings::default(),
                    },
                )
                .await?;
            debug!("connected {container_name} to network {full_name}");
        }

        Ok(())
    }

    /// Poll a container until its health status is `healthy` or timeout.
    async fn wait_healthy(&self, container_name: &str, service: &Service) -> Result<()> {
        use bollard::models::HealthStatusEnum;

        let retries = service
            .healthcheck
            .as_ref()
            .and_then(|h| h.retries)
            .unwrap_or(30);

        for _ in 0..retries {
            let info = self
                .docker
                .inspect_container(container_name, None)
                .await?;

            if let Some(state) = info.state {
                if let Some(health) = state.health {
                    if health.status == Some(HealthStatusEnum::HEALTHY) {
                        return Ok(());
                    }
                }
            }
            // Not healthy yet — sleep before next poll.

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        Err(ComposeError::HealthCheckTimeout(container_name.into()))
    }

    fn container_name(&self, service_name: &str, service: &Service) -> String {
        service
            .container_name
            .clone()
            .unwrap_or_else(|| format!("{}-{}", self.project, service_name))
    }
}

// ---------------------------------------------------------------------------
// Build helpers
// ---------------------------------------------------------------------------

/// Build the environment variable list for a container config.
///
/// Merges `env_file:` entries (lower priority) with `environment:` (higher priority).
fn build_env(service: &Service, base_dir: &Path) -> Vec<String> {
    // Load env_file vars (lower priority).
    let env_file_paths = service.env_file.to_list();
    let env_file_vars = if !env_file_paths.is_empty() {
        match crate::env_file::load_env_files(&env_file_paths, base_dir) {
            Ok(vars) => vars,
            Err(e) => {
                warn!("failed to load env_file: {e}");
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };

    crate::env_file::merge_env(service.environment.to_map(), env_file_vars)
}

/// Build the bind-mount list.
fn build_binds(service: &Service) -> Vec<String> {
    service
        .volumes
        .iter()
        .filter_map(|v| match v {
            VolumeMount::Short(s) => Some(s.clone()),
            VolumeMount::Long {
                source,
                target,
                read_only,
                ..
            } => {
                let src = source.as_deref().unwrap_or("");
                let ro = if read_only.unwrap_or(false) { ":ro" } else { "" };
                Some(format!("{src}:{target}{ro}"))
            }
        })
        .collect()
}

/// Build secret bind-mounts (`/run/secrets/<name>`).
fn build_secret_binds(service: &Service, file: &ComposeFile) -> Vec<String> {
    let mut binds = Vec::new();
    for secret_name in &service.secrets {
        if let Some(config) = file.secrets.get(secret_name) {
            match config {
                SecretConfig { file: Some(host_path), .. } => {
                    binds.push(format!(
                        "{host_path}:/run/secrets/{secret_name}:ro"
                    ));
                }
                SecretConfig { external: Some(true), .. } => {
                    // External secrets are handled by the runtime; add a label
                    // so Podman can inject them if supported.
                    debug!("external secret {secret_name} — relying on runtime injection");
                }
                _ => {}
            }
        }
    }
    binds
}

/// Convert compose restart policy to bollard's type.
fn build_restart_policy(service: &Service) -> Option<RestartPolicy> {
    service.restart.as_ref().map(|r| {
        let name = match r {
            ComposeRestart::No => RestartPolicyNameEnum::NO,
            ComposeRestart::Always => RestartPolicyNameEnum::ALWAYS,
            ComposeRestart::OnFailure => RestartPolicyNameEnum::ON_FAILURE,
            ComposeRestart::UnlessStopped => RestartPolicyNameEnum::UNLESS_STOPPED,
        };
        RestartPolicy {
            name: Some(name),
            maximum_retry_count: None,
        }
    })
}

/// Build log config from service `logging:`.
fn build_log_config(service: &Service) -> Option<HostConfigLogConfig> {
    service.logging.as_ref().map(|l| HostConfigLogConfig {
        typ: l.driver.clone(),
        config: if l.options.is_empty() {
            None
        } else {
            Some(l.options.clone())
        },
    })
}

/// Determine `network_mode` string and the first named network to attach via
/// `NetworkingConfig`.
///
/// Returns `(Option<network_mode>, Option<first_network_name>)`.
fn resolve_network_mode(
    service: &Service,
    file: &ComposeFile,
) -> (Option<String>, Option<String>) {
    if let Some(mode) = &service.network_mode {
        // Explicit network_mode — don't attach to any named networks.
        return (Some(mode.clone()), None);
    }

    let networks = service.networks.names();
    if networks.is_empty() {
        (None, None)
    } else {
        let first = resolve_network_name(&networks[0], file);
        (None, Some(first))
    }
}

/// Resolve a service-level network name to the actual network name (handling
/// `networks.<name>.name` overrides).
fn resolve_network_name(network: &str, file: &ComposeFile) -> String {
    file.networks
        .get(network)
        .and_then(|c| c.as_ref())
        .and_then(|c| c.name.as_deref())
        .unwrap_or(network)
        .to_string()
}

// ---------------------------------------------------------------------------
// Build context tar
// ---------------------------------------------------------------------------

/// Create a gzipped tar archive of the build context directory.
///
/// Respects `.dockerignore` if present.  Returns the archive bytes.
fn build_context_tar(context: &Path, _dockerfile: &str) -> Result<Vec<u8>> {
    let ignore_patterns = read_dockerignore(context);

    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut tar = tar::Builder::new(encoder);

    for entry in WalkDir::new(context).follow_links(false) {
        let entry = entry.map_err(|e| ComposeError::Io(e.into()))?;
        let abs = entry.path();
        let rel = abs
            .strip_prefix(context)
            .map_err(|_| ComposeError::Build("path strip error".into()))?;

        if rel.as_os_str().is_empty() {
            continue;
        }

        let rel_str = rel.to_string_lossy();

        if is_ignored(&rel_str, &ignore_patterns) {
            continue;
        }

        if abs.is_dir() {
            tar.append_dir(rel, abs)
                .map_err(|e| ComposeError::Build(e.to_string()))?;
        } else {
            tar.append_path_with_name(abs, rel)
                .map_err(|e| ComposeError::Build(e.to_string()))?;
        }
    }

    let gz = tar
        .into_inner()
        .map_err(|e| ComposeError::Build(e.to_string()))?;
    let bytes = gz
        .finish()
        .map_err(|e| ComposeError::Build(e.to_string()))?;

    Ok(bytes)
}

fn read_dockerignore(context: &Path) -> Vec<String> {
    let path = context.join(".dockerignore");
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

fn is_ignored(path: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        // Simple prefix / exact match (full glob support requires an extra crate).
        if pattern.ends_with('/') {
            if path.starts_with(pattern.as_str()) {
                return true;
            }
        } else if path == pattern || path.starts_with(&format!("{pattern}/")) {
            return true;
        }
    }
    false
}
