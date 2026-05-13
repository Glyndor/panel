//! Container orchestration engine.
//!
//! Translates a parsed [`ComposeFile`] into Podman API calls via bollard.
//! All compose-spec runtime fields are mapped to the corresponding bollard
//! `Config` / `HostConfig` knobs; fields that have no direct equivalent
//! in the Docker API are silently ignored (with a `tracing::debug!` line).

use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, LogOutput, LogsOptions,
    RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::{BuildImageOptions, CreateImageOptions};
use bollard::models::{
    DeviceMapping, EndpointIpamConfig, EndpointSettings, HealthConfig, HostConfig,
    HostConfigLogConfig, NetworkingConfig, RestartPolicy as BollardRestart, RestartPolicyNameEnum,
    Ulimit,
};
use bollard::network::{ConnectNetworkOptions, CreateNetworkOptions};
use bollard::volume::CreateVolumeOptions;
use bollard::Docker;
use bytes::Bytes;
use flate2::write::GzEncoder;
use flate2::Compression;
use futures::StreamExt;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use walkdir::WalkDir;

use crate::compose::types::{
    BindOptions, BuildConfig, Command as ComposeCommand, ComposeFile, ConfigConfig, HealthCheck,
    LoggingConfig, RestartPolicy as ComposeRestart, SecretConfig, Service, ServiceConfigRef,
    ServiceCondition, ServiceNetworkConfig, ServiceSecretRef, VolumeMount, VolumeOptions,
    VolumeType,
};
use crate::error::{ComposeError, Result};
use crate::ports;
use crate::size;

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// The orchestration engine that drives container lifecycle for a single
/// compose project.
pub struct Engine {
    docker: Docker,
    project: String,
    /// Base directory for resolving relative `env_file:` paths.
    base_dir: PathBuf,
}

impl Engine {
    /// Create a new engine for the given project name.
    ///
    /// `base_dir` defaults to the current working directory.
    pub fn new(docker: Docker, project: String) -> Self {
        Self {
            docker,
            project,
            base_dir: std::env::current_dir().unwrap_or_default(),
        }
    }

    /// Create an engine with an explicit base directory.
    pub fn with_base_dir(docker: Docker, project: String, base_dir: PathBuf) -> Self {
        Self {
            docker,
            project,
            base_dir,
        }
    }

    // -----------------------------------------------------------------------
    // Public commands
    // -----------------------------------------------------------------------

    /// Start all services defined in `file` (in dependency order).
    pub async fn up(&self, file: &ComposeFile) -> Result<()> {
        self.up_with_options(file, false, &[]).await
    }

    /// Start all services, with options for detached mode and active profiles.
    ///
    /// Services without a `profiles:` list always start.  Services with at
    /// least one profile only start if one of their profiles appears in
    /// `active_profiles` (or in the `COMPOSE_PROFILES` env var).
    pub async fn up_with_options(
        &self,
        file: &ComposeFile,
        _detach: bool,
        active_profiles: &[String],
    ) -> Result<()> {
        let order = crate::compose::resolve_order(file)?;
        let active = active_profiles_set(active_profiles);

        self.create_networks(file).await?;
        self.create_volumes(file).await?;

        for name in &order {
            let service = &file.services[name];

            if !service_in_profiles(service, &active) {
                debug!("skipping {name}: no active profile match");
                continue;
            }

            // Wait for dependencies as required.
            for dep in service.depends_on.service_names() {
                let condition = service.depends_on.condition_for(&dep);
                let dep_service = match file.services.get(&dep) {
                    Some(s) => s,
                    None => continue,
                };
                if !service_in_profiles(dep_service, &active) {
                    continue;
                }
                let dep_container = self.container_name(&dep, dep_service);

                match condition {
                    ServiceCondition::ServiceStarted => { /* implicit */ }
                    ServiceCondition::ServiceHealthy => {
                        if dep_service.healthcheck.as_ref().map(|h| !h.is_disabled()).unwrap_or(false) {
                            self.wait_healthy(&dep_container, dep_service).await?;
                        } else {
                            debug!(
                                "{dep} requested service_healthy but has no healthcheck — skipping wait"
                            );
                        }
                    }
                    ServiceCondition::ServiceCompletedSuccessfully => {
                        self.wait_completed(&dep_container).await?;
                    }
                }
            }

            // Build / pull image.
            if service.build.is_some() {
                self.build_service(name, service).await?;
            } else {
                self.pull_image(service).await?;
            }

            let container_name = self.container_name(name, service);
            self.create_and_start(&container_name, name, service, file).await?;

            // Connect to additional networks (the first was attached at create-time).
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
            .list_containers(Some(ListContainersOptions::<String> {
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

            let mut labels: HashMap<String, String> = config
                .as_ref()
                .map(|c| c.labels.to_map())
                .unwrap_or_default();
            labels.insert("lynx.compose.project".to_string(), self.project.clone());

            let driver_opts: HashMap<String, String> = config
                .as_ref()
                .map(|c| c.driver_opts.clone())
                .unwrap_or_default();

            let options = CreateNetworkOptions::<String> {
                name: network_name.to_string(),
                driver: driver.clone(),
                internal: config.as_ref().and_then(|c| c.internal).unwrap_or(false),
                attachable: config.as_ref().and_then(|c| c.attachable).unwrap_or(false),
                enable_ipv6: config.as_ref().and_then(|c| c.enable_ipv6).unwrap_or(false),
                options: driver_opts,
                labels,
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

            let mut labels: HashMap<String, String> = config
                .as_ref()
                .map(|c| c.labels.to_map())
                .unwrap_or_default();
            labels.insert("lynx.compose.project".to_string(), self.project.clone());

            let driver = config
                .as_ref()
                .and_then(|c| c.driver.clone())
                .unwrap_or_else(|| "local".into());

            let driver_opts: HashMap<String, String> = config
                .as_ref()
                .map(|c| c.driver_opts.clone())
                .unwrap_or_default();

            let options = CreateVolumeOptions::<String> {
                name: volume_name.to_string(),
                driver: driver.clone(),
                driver_opts,
                labels,
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
                platform: service.platform.as_deref().unwrap_or(""),
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

        // Build args — empty values inherit from the host env.
        let arg_map = build.args().to_map();
        let env: HashMap<String, String> = std::env::vars().collect();
        let mut build_args: HashMap<String, String> = HashMap::new();
        for (k, v) in arg_map {
            let value = match v {
                Some(val) => val,
                None => env.get(&k).cloned().unwrap_or_default(),
            };
            build_args.insert(k, value);
        }

        let mut labels: HashMap<String, String> = HashMap::new();
        if let BuildConfig::Config { labels: l, .. } = build {
            labels.extend(l.to_map());
        }

        let target_owned = build.target().map(|s| s.to_string()).unwrap_or_default();
        let network_owned = if let BuildConfig::Config { network: Some(n), .. } = build {
            n.clone()
        } else {
            String::new()
        };
        let platform_owned = if let BuildConfig::Config { platforms, .. } = build {
            platforms.first().cloned().unwrap_or_default()
        } else {
            String::new()
        };

        let options = BuildImageOptions::<String> {
            dockerfile: dockerfile.to_string(),
            t: tag.clone(),
            rm: true,
            buildargs: build_args,
            labels,
            target: target_owned,
            networkmode: network_owned,
            platform: platform_owned,
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

        // --- mounts ---
        let binds = build_binds(service);
        let secret_binds = build_secret_binds(service, file);
        let config_binds = build_config_binds(service, file);
        let all_binds: Vec<String> = binds
            .into_iter()
            .chain(secret_binds)
            .chain(config_binds)
            .collect();

        // --- ports ---
        let parsed_ports = ports::parse_ports(&service.ports)?;
        let (port_bindings, mut exposed_ports) = ports::to_bollard(&parsed_ports);

        // Add bare "expose:" entries (no host bindings).
        for raw in &service.expose {
            let key = if raw.contains('/') {
                raw.clone()
            } else {
                format!("{raw}/tcp")
            };
            exposed_ports.entry(key).or_default();
        }

        // --- restart policy ---
        let restart_policy = build_restart_policy(service);

        // --- logging ---
        let log_config = build_log_config(service.logging.as_ref());

        // --- network mode ---
        let (network_mode, first_network) = resolve_network_mode(service, file);

        // --- labels with project tracking ---
        let mut labels = service.labels.to_map();
        // Annotations (Podman OCI) — fold into labels under `annotation.*`.
        for (k, v) in service.annotations.to_map() {
            labels.insert(format!("annotation.{k}"), v);
        }
        labels.insert("lynx.compose.project".to_string(), self.project.clone());
        labels.insert("lynx.compose.service".to_string(), service_name.to_string());

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

        // --- extra_hosts / dns ---
        let extra_hosts: Vec<String> = service.extra_hosts.clone();
        let dns = service.dns.to_list();
        let dns_search = service.dns_search.to_list();

        // --- devices ---
        let devices: Vec<DeviceMapping> = service
            .devices
            .iter()
            .map(|s| parse_device(s.as_str()))
            .collect();

        // --- tmpfs (shorthand list) ---
        let tmpfs_list = service.tmpfs.to_list();
        let mut tmpfs_map: HashMap<String, String> = tmpfs_list
            .into_iter()
            .map(|p| (p, String::new()))
            .collect();
        // Tmpfs declared via long-form volume mounts.
        for v in &service.volumes {
            if let VolumeMount::Long {
                volume_type: VolumeType::Tmpfs,
                target,
                tmpfs,
                ..
            } = v
            {
                let opts = tmpfs_options_to_string(tmpfs.as_ref());
                tmpfs_map.insert(target.clone(), opts);
            }
        }

        // --- cpu / memory ---
        let (mem_limit, mem_reservation, memswap, nano_cpus, cpu_quota_eff, cpu_period_eff) =
            resolve_resources(service);

        let host_config = HostConfig {
            binds: opt_vec(all_binds),
            network_mode: network_mode.clone(),
            restart_policy,
            port_bindings: opt_map(port_bindings),
            cap_add: opt_vec(service.cap_add.clone()),
            cap_drop: opt_vec(service.cap_drop.clone()),
            sysctls: opt_map(sysctls),
            ulimits: opt_vec(ulimits),
            extra_hosts: opt_vec(extra_hosts),
            dns: opt_vec(dns),
            dns_search: opt_vec(dns_search),
            init: service.init,
            privileged: service.privileged,
            log_config,
            pid_mode: service.pid.clone(),
            ipc_mode: service.ipc.clone(),
            cgroup_parent: service.cgroup_parent.clone(),
            shm_size: service.shm_size.as_deref().and_then(size::parse_memory),
            userns_mode: service.userns_mode.clone(),
            security_opt: opt_vec(service.security_opt.clone()),
            readonly_rootfs: service.read_only,
            devices: opt_vec(devices),
            tmpfs: opt_map(tmpfs_map),
            volumes_from: opt_vec(service.volumes_from.clone()),
            links: opt_vec(service.links.clone()),
            runtime: service.runtime.clone(),
            memory: mem_limit,
            memory_reservation: mem_reservation,
            memory_swap: memswap,
            nano_cpus,
            cpu_shares: service.cpu_shares.map(|s| s as i64),
            cpu_quota: cpu_quota_eff,
            cpu_period: cpu_period_eff,
            cpuset_cpus: service.cpuset.clone(),
            oom_kill_disable: service.oom_kill_disable,
            oom_score_adj: service.oom_score_adj,
            storage_opt: opt_map(service.storage_opt.clone()),
            group_add: opt_vec(service.group_add.clone()),
            ..Default::default()
        };

        let cmd = service.command.as_ref().map(|c| c.to_exec());
        let entrypoint = service.entrypoint.as_ref().map(|c| c.to_exec());

        let networking_config = first_network.as_ref().map(|net| {
            let mut endpoints = HashMap::new();
            let svc_net_cfg = service.networks.config_for(net);
            endpoints.insert(net.clone(), build_endpoint_settings(svc_net_cfg, file));
            NetworkingConfig {
                endpoints_config: endpoints,
            }
        });

        let healthcheck = service.healthcheck.as_ref().map(build_healthcheck);

        let exposed_ports_json: HashMap<String, HashMap<(), ()>> = exposed_ports;

        let config = Config::<String> {
            image: Some(image.to_string()),
            env: opt_vec(env),
            cmd,
            entrypoint,
            host_config: Some(host_config),
            labels: opt_map(labels),
            exposed_ports: opt_map(exposed_ports_json),
            tty: service.tty,
            open_stdin: service.stdin_open,
            user: service.user.clone(),
            working_dir: service.working_dir.clone(),
            stop_signal: service.stop_signal.clone(),
            stop_timeout: service
                .stop_grace_period
                .as_deref()
                .and_then(size::parse_duration_secs)
                .map(|s| s as i64),
            hostname: service.hostname.clone(),
            domainname: service.domainname.clone(),
            mac_address: service.mac_address.clone(),
            networking_config,
            healthcheck,
            ..Default::default()
        };
        // Note: `mac_address` on Config is deprecated in newer Docker versions
        // but still accepted by Podman; the per-network MAC takes precedence.

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
        for network in network_names.iter().skip(1) {
            let full_name = resolve_network_name(network, file);
            let endpoint_config =
                build_endpoint_settings(service.networks.config_for(network), file);
            self.docker
                .connect_network(
                    &full_name,
                    ConnectNetworkOptions {
                        container: container_name,
                        endpoint_config,
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
            let info = self.docker.inspect_container(container_name, None).await?;
            if let Some(state) = info.state {
                if let Some(health) = state.health {
                    if health.status == Some(HealthStatusEnum::HEALTHY) {
                        return Ok(());
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }

        Err(ComposeError::HealthCheckTimeout(container_name.into()))
    }

    /// Poll a container until it has exited with status 0.
    async fn wait_completed(&self, container_name: &str) -> Result<()> {
        for _ in 0..600 {
            let info = self.docker.inspect_container(container_name, None).await?;
            if let Some(state) = info.state {
                let status = state.status.map(|s| format!("{s:?}").to_lowercase());
                if status.as_deref() == Some("exited") {
                    if state.exit_code.unwrap_or(-1) == 0 {
                        return Ok(());
                    }
                    return Err(ComposeError::HealthCheckTimeout(format!(
                        "{container_name} exited with non-zero status"
                    )));
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
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
// Profile filtering
// ---------------------------------------------------------------------------

/// Build the active-profile set, falling back to the `COMPOSE_PROFILES`
/// environment variable when no explicit list is supplied.
fn active_profiles_set(active: &[String]) -> HashSet<String> {
    if !active.is_empty() {
        return active.iter().cloned().collect();
    }
    std::env::var("COMPOSE_PROFILES")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// True if the service should be started given the active profile set.
fn service_in_profiles(service: &Service, active: &HashSet<String>) -> bool {
    if service.profiles.is_empty() {
        return true;
    }
    service.profiles.iter().any(|p| active.contains(p))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn opt_vec<T>(v: Vec<T>) -> Option<Vec<T>> {
    if v.is_empty() { None } else { Some(v) }
}

fn opt_map<K, V>(m: HashMap<K, V>) -> Option<HashMap<K, V>> {
    if m.is_empty() { None } else { Some(m) }
}

// `cgroupns_mode` is intentionally not forwarded — bollard 0.17 exposes it
// as an enum that varies between minor releases.  Podman accepts the
// `cgroup` compose field when set via the API too; rely on that path.
#[allow(dead_code)]
fn map_cgroupns_mode(s: &str) -> Option<String> {
    match s {
        "host" | "private" => Some(s.to_string()),
        _ => None,
    }
}

/// Resolve effective resource limits, preferring the top-level fields and
/// falling back to `deploy.resources.*`.
///
/// Returns `(memory, memory_reservation, memory_swap, nano_cpus, cpu_quota, cpu_period)`.
fn resolve_resources(
    service: &Service,
) -> (
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
) {
    let mut memory = service.mem_limit.as_deref().and_then(size::parse_memory);
    let mut mem_reservation = service
        .mem_reservation
        .as_deref()
        .and_then(size::parse_memory);
    let memswap = service.memswap_limit.as_deref().and_then(size::parse_memory);

    let mut nano_cpus = None;
    let cpu_quota = service.cpu_quota;
    let cpu_period = service.cpu_period.map(|p| p as i64);

    if let Some(deploy) = &service.deploy {
        if let Some(res) = &deploy.resources {
            if let Some(limits) = &res.limits {
                if memory.is_none() {
                    memory = limits.memory.as_deref().and_then(size::parse_memory);
                }
                if nano_cpus.is_none() {
                    nano_cpus = limits.cpus.as_deref().and_then(size::parse_cpus);
                }
            }
            if let Some(reserv) = &res.reservations {
                if mem_reservation.is_none() {
                    mem_reservation = reserv.memory.as_deref().and_then(size::parse_memory);
                }
            }
        }
    }

    (memory, mem_reservation, memswap, nano_cpus, cpu_quota, cpu_period)
}

fn tmpfs_options_to_string(opts: Option<&crate::compose::types::TmpfsOptions>) -> String {
    let opts = match opts {
        Some(o) => o,
        None => return String::new(),
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(size) = opts.size {
        parts.push(format!("size={size}"));
    }
    if let Some(mode) = opts.mode {
        parts.push(format!("mode={mode:o}"));
    }
    parts.join(",")
}

/// Build the environment variable list for a container config.
///
/// Merges `env_file:` entries (lower priority) with `environment:` (higher priority).
fn build_env(service: &Service, base_dir: &Path) -> Vec<String> {
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
///
/// Translates compose volume entries into Docker's `binds` strings.  Long
/// forms map to `source:target[:options]`; the type, propagation, SELinux
/// flag, and `ro/rw` bits are encoded into the options string.
fn build_binds(service: &Service) -> Vec<String> {
    let mut out = Vec::new();
    for v in &service.volumes {
        match v {
            VolumeMount::Short(s) => out.push(s.clone()),
            VolumeMount::Long {
                volume_type,
                source,
                target,
                read_only,
                bind,
                volume,
                ..
            } => {
                if matches!(volume_type, VolumeType::Tmpfs) {
                    // Handled via HostConfig.tmpfs, not binds.
                    continue;
                }
                let src = source.as_deref().unwrap_or("");
                let mut opts: Vec<String> = Vec::new();
                if read_only.unwrap_or(false) {
                    opts.push("ro".into());
                } else {
                    opts.push("rw".into());
                }
                if let Some(b) = bind {
                    extend_bind_opts(&mut opts, b);
                }
                if let Some(vol) = volume {
                    extend_volume_opts(&mut opts, vol);
                }
                let opt_str = opts.join(",");
                out.push(format!("{src}:{target}:{opt_str}"));
            }
        }
    }
    out
}

fn extend_bind_opts(opts: &mut Vec<String>, b: &BindOptions) {
    if let Some(p) = &b.propagation {
        opts.push(p.clone());
    }
    if let Some(s) = &b.selinux {
        // Compose uses "z" or "Z" — pass directly, it's a Docker bind option.
        opts.push(s.clone());
    }
}

fn extend_volume_opts(opts: &mut Vec<String>, v: &VolumeOptions) {
    if v.nocopy.unwrap_or(false) {
        opts.push("nocopy".into());
    }
}

/// Build secret bind-mounts (`/run/secrets/<name>`).
fn build_secret_binds(service: &Service, file: &ComposeFile) -> Vec<String> {
    let mut binds = Vec::new();
    for secret_ref in &service.secrets {
        let (name, target_override) = match secret_ref {
            ServiceSecretRef::Short(s) => (s.clone(), None),
            ServiceSecretRef::Long { source, target, .. } => {
                (source.clone(), target.clone())
            }
        };
        if let Some(config) = file.secrets.get(&name) {
            let target = target_override.unwrap_or_else(|| format!("/run/secrets/{name}"));
            match config {
                SecretConfig {
                    file: Some(host_path),
                    ..
                } => {
                    binds.push(format!("{host_path}:{target}:ro"));
                }
                SecretConfig {
                    external: Some(true),
                    ..
                } => {
                    debug!("external secret {name} — relying on runtime injection");
                }
                _ => {}
            }
        }
    }
    binds
}

/// Build config bind-mounts (compose-spec `configs:`).
///
/// Configs are mounted into `/<name>` by default (or the `target:` override),
/// using the host file path declared in the top-level `configs:` section.
fn build_config_binds(service: &Service, file: &ComposeFile) -> Vec<String> {
    let mut binds = Vec::new();
    for config_ref in &service.configs {
        let (name, target_override) = match config_ref {
            ServiceConfigRef::Short(s) => (s.clone(), None),
            ServiceConfigRef::Long { source, target, .. } => {
                (source.clone(), target.clone())
            }
        };
        if let Some(cfg) = file.configs.get(&name) {
            let target = target_override.unwrap_or_else(|| format!("/{name}"));
            match cfg {
                ConfigConfig {
                    file: Some(host_path),
                    ..
                } => {
                    binds.push(format!("{host_path}:{target}:ro"));
                }
                ConfigConfig {
                    external: Some(true),
                    ..
                } => {
                    debug!("external config {name} — relying on runtime injection");
                }
                _ => {}
            }
        }
    }
    binds
}

/// Convert compose restart policy to bollard's type.
fn build_restart_policy(service: &Service) -> Option<BollardRestart> {
    service.restart.as_ref().map(|r| match r {
        ComposeRestart::No => BollardRestart {
            name: Some(RestartPolicyNameEnum::NO),
            maximum_retry_count: None,
        },
        ComposeRestart::Always => BollardRestart {
            name: Some(RestartPolicyNameEnum::ALWAYS),
            maximum_retry_count: None,
        },
        ComposeRestart::OnFailure { max_attempts } => BollardRestart {
            name: Some(RestartPolicyNameEnum::ON_FAILURE),
            maximum_retry_count: max_attempts.map(|n| n as i64),
        },
        ComposeRestart::UnlessStopped => BollardRestart {
            name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
            maximum_retry_count: None,
        },
    })
}

/// Build log config from a service `logging:` section.
fn build_log_config(logging: Option<&LoggingConfig>) -> Option<HostConfigLogConfig> {
    logging.map(|l| HostConfigLogConfig {
        typ: l.driver.clone(),
        config: if l.options.is_empty() {
            None
        } else {
            Some(l.options.clone())
        },
    })
}

/// Translate a compose [`HealthCheck`] into a bollard [`HealthConfig`].
fn build_healthcheck(hc: &HealthCheck) -> HealthConfig {
    if hc.is_disabled() {
        return HealthConfig {
            test: Some(vec!["NONE".to_string()]),
            ..Default::default()
        };
    }
    let test = hc.test.as_ref().map(|cmd| match cmd {
        ComposeCommand::Shell(s) => vec!["CMD-SHELL".to_string(), s.clone()],
        ComposeCommand::Exec(v) => v.clone(),
    });
    HealthConfig {
        test,
        interval: hc.interval.as_deref().and_then(size::parse_duration_nanos),
        timeout: hc.timeout.as_deref().and_then(size::parse_duration_nanos),
        retries: hc.retries.map(|r| r as i64),
        start_period: hc
            .start_period
            .as_deref()
            .and_then(size::parse_duration_nanos),
        start_interval: hc
            .start_interval
            .as_deref()
            .and_then(size::parse_duration_nanos),
    }
}

/// Build endpoint settings for a network attachment, translating per-network
/// aliases / IPs into bollard's `EndpointSettings`.
fn build_endpoint_settings(
    cfg: Option<&ServiceNetworkConfig>,
    _file: &ComposeFile,
) -> EndpointSettings {
    let mut settings = EndpointSettings::default();
    if let Some(c) = cfg {
        if let Some(aliases) = &c.aliases {
            settings.aliases = Some(aliases.clone());
        }
        if c.ipv4_address.is_some() || c.ipv6_address.is_some() || !c.link_local_ips.is_empty() {
            settings.ipam_config = Some(EndpointIpamConfig {
                ipv4_address: c.ipv4_address.clone(),
                ipv6_address: c.ipv6_address.clone(),
                link_local_ips: if c.link_local_ips.is_empty() {
                    None
                } else {
                    Some(c.link_local_ips.clone())
                },
            });
        }
        if c.mac_address.is_some() {
            settings.mac_address = c.mac_address.clone();
        }
        if let Some(prio) = c.priority {
            let mut m = HashMap::new();
            m.insert("priority".to_string(), prio.to_string());
            settings.driver_opts = Some(m);
        }
    }
    settings
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

/// Parse a device mapping string `host[:container[:permissions]]`.
fn parse_device(s: &str) -> DeviceMapping {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    let host = parts.first().copied().unwrap_or("").to_string();
    let cont = parts.get(1).copied().map(|c| c.to_string()).unwrap_or_else(|| host.clone());
    let perm = parts.get(2).copied().unwrap_or("rwm").to_string();
    DeviceMapping {
        path_on_host: Some(host),
        path_in_container: Some(cont),
        cgroup_permissions: Some(perm),
    }
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
