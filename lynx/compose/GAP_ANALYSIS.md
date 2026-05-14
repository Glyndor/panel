# Docker Compose Spec → lynx-compose Gap Analysis

**Date:** 2026-05-13  
**Spec source:** Docker Compose Specification (current, 2025/2026)  
**Implementation:** `lynx/compose` (Rust, bollard/Podman API)  
**Goal:** 100% spec-compatible docker-compose → Podman translator

Legend:
- ✅ Parsed **and** executed (wired into API call)
- ⚠️  Parsed (struct field exists) but **not** applied to the API call
- ❌  Not parsed, not implemented
- 🔶 Partial — some sub-fields missing or logic incomplete

---

## 1. Top-Level Document Fields

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `version` | ✅ | n/a | ✅ | Accepted, ignored (spec-compliant) |
| `name` | ✅ | ✅ | ✅ | Used as project label prefix |
| `services` | ✅ | ✅ | ✅ | Full service map |
| `networks` | ✅ | ✅ | ✅ | Top-level network creation |
| `volumes` | ✅ | ✅ | ✅ | Top-level volume creation |
| `secrets` | ✅ | 🔶 | 🔶 | Parsed; only `file:` and `external:` paths wired; `content`, `environment`, `driver`, `driver_opts` not applied |
| `configs` | ✅ | 🔶 | 🔶 | Same as secrets — only `file:` and `external:` wired |
| `include` | ✅ | ✅ | ✅ | Paths merged; long-form `env_file` and `project_directory` parsed |
| `extends` (service-level) | ✅ | ✅ | ✅ | Same-file and cross-file resolution |

---

## 2. `services.*` Fields

### 2.1 Core / Image

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `image` | ✅ | ✅ | ✅ | |
| `build` (short — context string) | ✅ | ✅ | ✅ | |
| `build.context` | ✅ | ✅ | ✅ | |
| `build.dockerfile` | ✅ | ✅ | ✅ | |
| `build.args` | ✅ | ✅ | ✅ | |
| `build.target` | ✅ | ❌ | ⚠️ | Parsed; bollard `BuildImageOptions` target field not set (see comment in `engine/build.rs`) |
| `build.labels` | ✅ | ✅ | ✅ | |
| `build.network` | ✅ | ✅ | ✅ | → `networkmode` |
| `build.platforms` | ✅ | ✅ | 🔶 | Only first platform taken |
| `build.shm_size` | ✅ | ❌ | ⚠️ | Field in struct, not forwarded to `BuildImageOptions` |
| `build.cache_from` | ✅ | ❌ | ⚠️ | Parsed (Vec<String>), not in `BuildImageOptions` call |
| `build.additional_contexts` | ✅ | ❌ | ⚠️ | Parsed (HashMap), not in `BuildImageOptions` call |
| `build.dockerfile_inline` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.cache_to` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.extra_hosts` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.isolation` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.no_cache` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.pull` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.ssh` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.secrets` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.tags` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.ulimits` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.privileged` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.entitlements` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.provenance` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `build.sbom` | ❌ | ❌ | ❌ | Not in BuildConfig struct |
| `container_name` | ✅ | ✅ | ✅ | |
| `command` | ✅ | ✅ | ✅ | Shell string or exec list |
| `entrypoint` | ✅ | ✅ | ✅ | Shell string or exec list |
| `working_dir` | ✅ | ✅ | ✅ | |
| `platform` | ✅ | ✅ | ✅ | → `CreateContainerOptions.platform` |
| `pull_policy` | ❌ | ❌ | ❌ | Not parsed — controls always/never/missing image pull |
| `runtime` | ✅ | ✅ | ✅ | → `HostConfig.runtime` |
| `scale` | ✅ | ❌ | ⚠️ | Parsed; engine only starts one replica |
| `attach` | ❌ | ❌ | ❌ | Log collection flag |

### 2.2 Environment

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `environment` (map or list) | ✅ | ✅ | ✅ | |
| `env_file` (short — string/list) | ✅ | ✅ | ✅ | |
| `env_file` long-form `path` | ❌ | ❌ | ❌ | `env_file` stored as `StringOrList` only; long-form `{path, required, format}` not parsed |
| `env_file.required` | ❌ | ❌ | ❌ | Missing — missing file should be silently ignored when false |
| `env_file.format` | ❌ | ❌ | ❌ | Format hint (raw, etc.) |

### 2.3 Ports

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| Short form (`"8080:80"`, ranges, IPv4/IPv6) | ✅ | ✅ | ✅ | Full range expansion, IPv4/IPv6 |
| Long form `target` | ✅ | ✅ | ✅ | |
| Long form `published` | ✅ | ✅ | ✅ | String or number |
| Long form `protocol` | ✅ | ✅ | ✅ | tcp/udp/sctp |
| Long form `host_ip` | ✅ | ✅ | ✅ | |
| Long form `mode` | ✅ | ❌ | ⚠️ | Parsed; `host`/`ingress` not differentiated in HostConfig |
| Long form `app_protocol` | ✅ | ❌ | ⚠️ | Parsed; informational, no API equivalent in bollard |
| Long form `name` | ✅ | ❌ | ⚠️ | Parsed; informational label |
| `expose` | ✅ | ✅ | ✅ | → `ExposedPorts` without PortBinding |

### 2.4 Volumes (service-level)

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| Short form (`"./data:/app/data:ro"`) | ✅ | ✅ | ✅ | |
| Long `type: volume` | ✅ | ✅ | ✅ | |
| Long `type: bind` | ✅ | ✅ | ✅ | |
| Long `type: tmpfs` | ✅ | ✅ | ✅ | |
| Long `type: npipe` | ✅ | ❌ | ⚠️ | VolumeType::Npipe parsed; no special handling in build_binds |
| Long `type: cluster` | ✅ | ❌ | ⚠️ | VolumeType::Cluster parsed; no special handling |
| `source` | ✅ | ✅ | ✅ | |
| `target` | ✅ | ✅ | ✅ | |
| `read_only` | ✅ | ✅ | ✅ | → `ro`/`rw` option |
| `bind.propagation` | ✅ | ✅ | ✅ | Appended to bind string |
| `bind.create_host_path` | ✅ | ❌ | ⚠️ | Parsed; host dir not auto-created |
| `bind.selinux` | ✅ | ✅ | ✅ | Appended as selinux label option |
| `volume.nocopy` | ✅ | ✅ | ✅ | → `nocopy` mount option |
| `volume.labels` | ✅ | ❌ | ⚠️ | Parsed; not forwarded — volume labels only at create time |
| `volume.driver_config.name` | ✅ | ❌ | ⚠️ | Parsed in VolumeOptions.driver_config; not wired to API |
| `volume.driver_config.options` | ✅ | ❌ | ⚠️ | Same |
| `volume.subpath` | ✅ | ❌ | ⚠️ | Parsed; no Podman API equivalent yet |
| `tmpfs.size` | ✅ | ✅ | ✅ | → `size=N` mount option |
| `tmpfs.mode` | ✅ | ✅ | ✅ | → `mode=NNNN` mount option |
| `image.subpath` (volume type: image) | ❌ | ❌ | ❌ | New OCI-image volume type; not parsed |
| `consistency` | ✅ | ❌ | ⚠️ | Parsed; no-op on Linux/Podman |
| `volumes_from` | ✅ | ✅ | ✅ | → `HostConfig.volumes_from` |
| `tmpfs` (top-level service field) | ✅ | ✅ | ✅ | → `HostConfig.tmpfs` |

### 2.5 Networking

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `networks` (list of names) | ✅ | ✅ | ✅ | |
| `networks` (map with per-network config) | ✅ | ✅ | ✅ | |
| `networks.*.aliases` | ✅ | ✅ | ✅ | → `EndpointSettings.aliases` |
| `networks.*.ipv4_address` | ✅ | ✅ | ✅ | → `EndpointIpamConfig.ipv4_address` |
| `networks.*.ipv6_address` | ✅ | ✅ | ✅ | → `EndpointIpamConfig.ipv6_address` |
| `networks.*.link_local_ips` | ✅ | ✅ | ✅ | → `EndpointIpamConfig.link_local_ips` |
| `networks.*.mac_address` | ✅ | ✅ | ✅ | → `EndpointSettings.mac_address` |
| `networks.*.driver_opts` | ✅ | 🔶 | 🔶 | Parsed; `priority` forwarded to driver_opts; other opts ignored |
| `networks.*.gw_priority` | ✅ | ❌ | ⚠️ | Parsed; not forwarded to endpoint settings |
| `networks.*.priority` | ✅ | 🔶 | 🔶 | Stored in driver_opts as string; spec uses it to order network attachments |
| `networks.*.interface_name` | ❌ | ❌ | ❌ | New field (2024+); not parsed |
| `network_mode` | ✅ | ✅ | ✅ | → `HostConfig.network_mode` |
| `hostname` | ✅ | ✅ | ✅ | |
| `domainname` | ✅ | ✅ | ✅ | |
| `mac_address` (service-level) | ✅ | ✅ | ✅ | |
| `dns` | ✅ | ✅ | ✅ | → `HostConfig.dns` |
| `dns_opt` | ❌ | ❌ | ❌ | Not in Service struct → `HostConfig.dns_options` |
| `dns_search` | ✅ | ✅ | ✅ | → `HostConfig.dns_search` |
| `extra_hosts` | ✅ | ✅ | ✅ | → `HostConfig.extra_hosts` |
| `links` | ✅ | ✅ | ✅ | → `HostConfig.links` (legacy) |
| `external_links` | ❌ | ❌ | ❌ | Not in Service struct |

### 2.6 Secrets & Configs (service-level references)

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `secrets` short form | ✅ | ✅ | ✅ | Mounts `/run/secrets/<name>` |
| `secrets` long `source` | ✅ | ✅ | ✅ | |
| `secrets` long `target` | ✅ | ✅ | ✅ | Custom mount path |
| `secrets` long `uid` | ✅ | ❌ | ⚠️ | Parsed; uid/gid not set on bind-mount (Podman limitation without tmpfs) |
| `secrets` long `gid` | ✅ | ❌ | ⚠️ | Same |
| `secrets` long `mode` | ✅ | ❌ | ⚠️ | Same — file permissions not enforced |
| `configs` short form | ✅ | ✅ | ✅ | Mounts `/<name>` |
| `configs` long `source` | ✅ | ✅ | ✅ | |
| `configs` long `target` | ✅ | ✅ | ✅ | |
| `configs` long `uid` | ✅ | ❌ | ⚠️ | Same as secrets |
| `configs` long `gid` | ✅ | ❌ | ⚠️ | Same |
| `configs` long `mode` | ✅ | ❌ | ⚠️ | Same |

### 2.7 Health Check

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `healthcheck.test` | ✅ | ✅ | ✅ | Shell string → `CMD-SHELL`; exec list passed raw |
| `healthcheck.interval` | ✅ | ✅ | ✅ | Duration string → nanoseconds |
| `healthcheck.timeout` | ✅ | ✅ | ✅ | |
| `healthcheck.retries` | ✅ | ✅ | ✅ | |
| `healthcheck.start_period` | ✅ | ✅ | ✅ | |
| `healthcheck.start_interval` | ✅ | ✅ | ✅ | |
| `healthcheck.disable` | ✅ | ✅ | ✅ | → `["NONE"]` test |

### 2.8 Lifecycle / Restart

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `restart: no` | ✅ | ✅ | ✅ | |
| `restart: always` | ✅ | ✅ | ✅ | |
| `restart: on-failure[:N]` | ✅ | ✅ | ✅ | |
| `restart: unless-stopped` | ✅ | ✅ | ✅ | |
| `stop_signal` | ✅ | ✅ | ✅ | → `Config.stop_signal` |
| `stop_grace_period` | ✅ | ✅ | ✅ | Duration → `Config.stop_timeout` (seconds) |
| `depends_on` (list) | ✅ | ✅ | ✅ | |
| `depends_on` long `condition` | ✅ | ✅ | ✅ | service_started / service_healthy / service_completed_successfully |
| `depends_on.restart` | ✅ | ❌ | ⚠️ | Parsed; flag not acted upon (restart dep on change) |
| `depends_on.required` | ✅ | ✅ | ✅ | Optional deps skipped gracefully |
| `post_start` lifecycle hook | ❌ | ❌ | ❌ | Not parsed |
| `pre_stop` lifecycle hook | ❌ | ❌ | ❌ | Not parsed |

### 2.9 Labels / Annotations / Metadata

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `labels` (map or list) | ✅ | ✅ | ✅ | → container labels; lynx.compose.* auto-added |
| `annotations` (map or list) | ✅ | ✅ | 🔶 | Merged into labels as `annotation.<key>=<val>` — not native OCI annotations |
| `label_file` | ❌ | ❌ | ❌ | Not parsed |
| `profiles` | ✅ | ✅ | ✅ | Services filtered by active profiles |

### 2.10 Security / Capabilities

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `cap_add` | ✅ | ✅ | ✅ | |
| `cap_drop` | ✅ | ✅ | ✅ | |
| `privileged` | ✅ | ✅ | ✅ | |
| `read_only` | ✅ | ✅ | ✅ | → `readonly_rootfs` |
| `security_opt` | ✅ | ✅ | ✅ | → `HostConfig.security_opt` |
| `userns_mode` | ✅ | ✅ | ✅ | |
| `user` | ✅ | ✅ | ✅ | |
| `group_add` | ✅ | ✅ | ✅ | |
| `credential_spec` | ❌ | ❌ | ❌ | Windows MSA credentials; not parsed |

### 2.11 Namespaces / Runtime

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `ipc` | ✅ | ✅ | ✅ | → `HostConfig.ipc_mode` |
| `pid` | ✅ | ✅ | ✅ | → `HostConfig.pid_mode` |
| `uts` | ❌ | ❌ | ❌ | Not in Service struct → `HostConfig.uts_mode` |
| `cgroup` | ✅ | ❌ | ⚠️ | Parsed; not forwarded to `HostConfig.cgroup_parent` or cgroup_ns |
| `cgroup_parent` | ✅ | ✅ | ✅ | |
| `isolation` | ❌ | ❌ | ❌ | Windows isolation mode; not parsed |
| `init` | ✅ | ✅ | ✅ | → `HostConfig.init` |
| `tty` | ✅ | ✅ | ✅ | → `Config.tty` |
| `stdin_open` | ✅ | ✅ | ✅ | → `Config.open_stdin` |
| `shm_size` | ✅ | ✅ | ✅ | → `HostConfig.shm_size` (parsed with size module) |

### 2.12 Resource Limits (top-level, non-deploy)

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `cpu_shares` | ✅ | ✅ | ✅ | |
| `cpu_quota` | ✅ | ✅ | ✅ | |
| `cpu_period` | ✅ | ✅ | ✅ | |
| `cpuset` | ✅ | ✅ | ✅ | → `cpuset_cpus` |
| `cpus` | ❌ | ❌ | ❌ | Fractional CPU shorthand → `nano_cpus`; not in Service struct |
| `cpu_count` | ❌ | ❌ | ❌ | Not parsed |
| `cpu_percent` | ❌ | ❌ | ❌ | Not parsed |
| `cpu_rt_runtime` | ❌ | ❌ | ❌ | Real-time CPU; not parsed |
| `cpu_rt_period` | ❌ | ❌ | ❌ | Not parsed |
| `mem_limit` | ✅ | ✅ | ✅ | → `HostConfig.memory` |
| `memswap_limit` | ✅ | ✅ | ✅ | → `HostConfig.memory_swap` |
| `mem_reservation` | ✅ | ✅ | ✅ | → `HostConfig.memory_reservation` |
| `mem_swappiness` | ❌ | ❌ | ❌ | Not in Service struct |
| `oom_kill_disable` | ✅ | ✅ | ✅ | |
| `oom_score_adj` | ✅ | ✅ | ✅ | |
| `pids_limit` | ❌ | ❌ | ❌ | Not in Service struct → `HostConfig.pids_limit` |
| `blkio_config` | ❌ | ❌ | ❌ | Entire block I/O section missing |

### 2.13 Devices / Storage

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `devices` | ✅ | ✅ | ✅ | `host:container[:perm]` → `DeviceMapping` |
| `device_cgroup_rules` | ❌ | ❌ | ❌ | Not in Service struct → `HostConfig.device_cgroup_rules` |
| `storage_opt` | ✅ | ✅ | ✅ | → `HostConfig.storage_opt` |

### 2.14 Logging

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `logging.driver` | ✅ | ✅ | ✅ | → `HostConfigLogConfig.typ` |
| `logging.options` | ✅ | ✅ | ✅ | → `HostConfigLogConfig.config` |

### 2.15 Sysctls / Ulimits

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `sysctls` (map or list) | ✅ | ✅ | ✅ | → `HostConfig.sysctls` |
| `ulimits` (single int or soft/hard pair) | ✅ | ✅ | ✅ | → `ResourcesUlimits` list |

### 2.16 Deploy (service.deploy)

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `deploy.mode` | ✅ | ❌ | ⚠️ | Parsed; global/replicated not differentiated |
| `deploy.replicas` | ✅ | ❌ | ⚠️ | Parsed; engine always starts 1 replica |
| `deploy.labels` | ✅ | ❌ | ⚠️ | Deploy-specific labels not merged into container labels |
| `deploy.endpoint_mode` | ✅ | ❌ | ⚠️ | Swarm field; no Podman equivalent |
| `deploy.resources.limits.cpus` | ✅ | ✅ | ✅ | → `nano_cpus` via `resolve_resources` |
| `deploy.resources.limits.memory` | ✅ | ✅ | ✅ | → `HostConfig.memory` |
| `deploy.resources.limits.pids` | ✅ | ❌ | ⚠️ | Parsed in ResourceSpec; not forwarded to `HostConfig.pids_limit` |
| `deploy.resources.limits.devices` | ❌ | ❌ | ❌ | GPU/device reservations not in ResourceSpec |
| `deploy.resources.reservations.cpus` | ✅ | ❌ | ⚠️ | Not forwarded (no Podman CPU reservation) |
| `deploy.resources.reservations.memory` | ✅ | ✅ | ✅ | → `HostConfig.memory_reservation` |
| `deploy.resources.reservations.pids` | ✅ | ❌ | ⚠️ | Same as limits.pids |
| `deploy.resources.reservations.devices` | ❌ | ❌ | ❌ | GPU reservations (capabilities, count, device_ids, options) |
| `deploy.restart_policy.condition` | ✅ | ❌ | ⚠️ | Swarm restart policy; not applied to container `RestartPolicy` |
| `deploy.restart_policy.delay` | ✅ | ❌ | ⚠️ | Same |
| `deploy.restart_policy.max_attempts` | ✅ | ❌ | ⚠️ | Same |
| `deploy.restart_policy.window` | ✅ | ❌ | ⚠️ | Same |
| `deploy.update_config.*` | ✅ | ❌ | ⚠️ | Swarm rolling update; no equivalent |
| `deploy.rollback_config.*` | ✅ | ❌ | ⚠️ | Same |
| `deploy.placement.constraints` | ✅ | ❌ | ⚠️ | Swarm node constraints; no local equivalent |
| `deploy.placement.preferences` | ✅ | ❌ | ⚠️ | Same |
| `deploy.placement.max_replicas_per_node` | ✅ | ❌ | ⚠️ | Same |

### 2.17 Advanced / Newer Fields

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `gpus` | ❌ | ❌ | ❌ | GPU device allocation (CDI / `--device nvidia.com/gpu=all`) |
| `models` | ❌ | ❌ | ❌ | AI model service integration (Docker AI feature) |
| `provider` | ❌ | ❌ | ❌ | External service management (Docker Cloud) |
| `develop` / `develop.watch` | ❌ | ❌ | ❌ | File-watch / live-reload; not parsed |
| `use_api_socket` | ❌ | ❌ | ❌ | Container engine socket access |
| `extends` (service-level) | ✅ | ✅ | ✅ | Cross-file and same-file |
| `external_links` | ❌ | ❌ | ❌ | Not in Service struct |
| `dns_opt` | ❌ | ❌ | ❌ | → `HostConfig.dns_options` |

---

## 3. Top-Level `networks.*` Fields

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `driver` | ✅ | ✅ | ✅ | Default: bridge |
| `driver_opts` | ✅ | ✅ | ✅ | → `CreateNetworkOptions.options` |
| `external` | ✅ | ✅ | ✅ | Skip creation if true |
| `name` | ✅ | ✅ | ✅ | Custom network name |
| `internal` | ✅ | ✅ | ✅ | → `CreateNetworkOptions.internal` |
| `attachable` | ✅ | ✅ | ✅ | |
| `enable_ipv6` | ✅ | ✅ | ✅ | |
| `enable_ipv4` | ❌ | ❌ | ❌ | Not in NetworkConfig (to disable IPv4) |
| `labels` | ✅ | ✅ | ✅ | lynx.compose.project auto-added |
| `ipam.driver` | ✅ | ❌ | ⚠️ | Parsed; not forwarded to `CreateNetworkOptions.ipam` |
| `ipam.config[].subnet` | ✅ | ❌ | ⚠️ | Parsed; IPAM config not wired to API call |
| `ipam.config[].gateway` | ✅ | ❌ | ⚠️ | Same |
| `ipam.config[].ip_range` | ✅ | ❌ | ⚠️ | Same |
| `ipam.config[].aux_addresses` | ✅ | ❌ | ⚠️ | Same |
| `ipam.options` | ✅ | ❌ | ⚠️ | Same |

---

## 4. Top-Level `volumes.*` Fields

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `driver` | ✅ | ✅ | ✅ | Default: local |
| `driver_opts` | ✅ | ✅ | ✅ | → `CreateVolumeOptions.driver_opts` |
| `external` | ✅ | ✅ | ✅ | Skip creation if true |
| `name` | ✅ | ✅ | ✅ | Custom volume name |
| `labels` | ✅ | ✅ | ✅ | lynx.compose.project auto-added |

---

## 5. Top-Level `secrets.*` Fields

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `file` | ✅ | ✅ | ✅ | Bind-mounted read-only into container |
| `external` | ✅ | ✅ | ✅ | Skip — relies on runtime injection |
| `name` | ✅ | ❌ | ⚠️ | Parsed; not used to resolve bind path |
| `content` | ✅ | ❌ | ⚠️ | Parsed; inline content not written to tmpfs/file |
| `environment` | ✅ | ❌ | ⚠️ | Parsed; env-var-sourced secret not materialized |
| `driver` | ✅ | ❌ | ⚠️ | Parsed; external secret driver not called |
| `driver_opts` | ✅ | ❌ | ⚠️ | Same |
| `labels` | ✅ | ❌ | ⚠️ | Parsed; no equivalent in Podman secret API |
| `template_driver` | ❌ | ❌ | ❌ | Not in SecretConfig struct |

---

## 6. Top-Level `configs.*` Fields

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `file` | ✅ | ✅ | ✅ | Bind-mounted read-only |
| `external` | ✅ | ✅ | ✅ | |
| `name` | ✅ | ❌ | ⚠️ | Parsed; not used to resolve bind path |
| `content` | ✅ | ❌ | ⚠️ | Parsed; inline content not materialized |
| `environment` | ✅ | ❌ | ⚠️ | Parsed; env-var-sourced config not materialized |
| `labels` | ✅ | ❌ | ⚠️ | Parsed; no Podman equivalent |
| `template_driver` | ❌ | ❌ | ❌ | Not in ConfigConfig struct |
| `driver` | ❌ | ❌ | ❌ | Not in ConfigConfig struct (present in SecretConfig) |
| `driver_opts` | ❌ | ❌ | ❌ | Not in ConfigConfig struct |

---

## 7. Top-Level `include` Fields

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| Short form (string path) | ✅ | ✅ | ✅ | |
| `path` (string or list) | ✅ | ✅ | ✅ | |
| `env_file` | ✅ | ❌ | ⚠️ | Parsed in IncludeConfig; not used to override substitution env |
| `project_directory` | ✅ | ❌ | ⚠️ | Parsed; not used to adjust base_dir for included file |

---

## 8. `extends` (service-level)

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| Short form (service name string) | ✅ | ✅ | ✅ | |
| `service` | ✅ | ✅ | ✅ | |
| `file` | ✅ | ✅ | ✅ | Cross-file extension |

---

## 9. `develop.watch` Fields (per rule)

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `path` | ❌ | ❌ | ❌ | Entire develop section not implemented |
| `action` (sync/rebuild/restart/sync+restart/sync+exec) | ❌ | ❌ | ❌ | |
| `target` | ❌ | ❌ | ❌ | |
| `ignore` | ❌ | ❌ | ❌ | |
| `include` | ❌ | ❌ | ❌ | |
| `initial_sync` | ❌ | ❌ | ❌ | |
| `exec.command` | ❌ | ❌ | ❌ | |

---

## 10. `blkio_config` (service-level)

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `weight` | ❌ | ❌ | ❌ | → `HostConfig.blkio_weight` |
| `weight_device[].path` | ❌ | ❌ | ❌ | → `HostConfig.blkio_weight_device` |
| `weight_device[].weight` | ❌ | ❌ | ❌ | Same |
| `device_read_bps[].path` | ❌ | ❌ | ❌ | → `HostConfig.blkio_device_read_bps` |
| `device_read_bps[].rate` | ❌ | ❌ | ❌ | Same |
| `device_write_bps[].path` | ❌ | ❌ | ❌ | → `HostConfig.blkio_device_write_bps` |
| `device_write_bps[].rate` | ❌ | ❌ | ❌ | Same |
| `device_read_iops[].path` | ❌ | ❌ | ❌ | → `HostConfig.blkio_device_read_i_ops` |
| `device_read_iops[].rate` | ❌ | ❌ | ❌ | Same |
| `device_write_iops[].path` | ❌ | ❌ | ❌ | → `HostConfig.blkio_device_write_i_ops` |
| `device_write_iops[].rate` | ❌ | ❌ | ❌ | Same |

---

## 11. Prioritized Gap List

### P1 — Critical (common in real-world compose files)

#### P1-A: `secrets.content` and `secrets.environment` — inline/env secrets not materialized

The spec allows defining a secret entirely inline or from a host env var. Currently these fields are parsed but silently ignored — the container gets no secret file at all.

**Fix:** Write the content (or env var value) to a temporary file on the host and bind-mount it read-only to the container's secret path.

```yaml
secrets:
  db_password:
    content: "s3cr3t"          # currently ⚠️ — parsed, not materialized
  api_key:
    environment: "MY_API_KEY"  # currently ⚠️ — parsed, not materialized
```

#### P1-B: `configs.content` and `configs.environment` — same issue as secrets

```yaml
configs:
  app_config:
    content: |
      key=value
      another=thing
```

#### P1-C: IPAM config not forwarded on network creation

`ipam.config[].subnet/gateway/ip_range` are parsed but not passed to `CreateNetworkOptions`. This breaks any compose file with static subnets.

```yaml
networks:
  backend:
    ipam:
      config:
        - subnet: 192.168.90.0/24
          gateway: 192.168.90.1
```

**Fix in `engine/network.rs`:** Build bollard `Ipam` struct from `IpamConfig` and assign it to `CreateNetworkOptions.ipam`.

#### P1-D: `build.target` not forwarded to `BuildImageOptions`

Multi-stage builds are very common. The field is parsed but the code has a `let _ = target_owned; // bollard field name varies` placeholder.

```yaml
services:
  app:
    build:
      context: .
      target: production   # currently ⚠️ — ignored
```

**Fix in `engine/build.rs`:** Set `BuildImageOptions { target: target_owned, .. }`.

#### P1-E: `env_file` long-form not parsed

`env_file` is stored as `StringOrList` so the long-form object with `path`, `required`, and `format` is silently dropped or causes a deserialization error.

```yaml
services:
  app:
    env_file:
      - path: .env.prod
        required: false        # currently ❌
      - path: .env.local
        required: true
```

**Fix:** Change `env_file` type to a proper enum/struct supporting both forms.

#### P1-F: `dns_opt` missing from Service struct

```yaml
services:
  app:
    dns_opt:
      - ndots:5
      - use-vc
```

**Fix:** Add `dns_opt: StringOrList` to `Service`; forward to `HostConfig.dns_options`.

#### P1-G: `scale` and `deploy.replicas` — only 1 replica started

Multiple replicas are ignored. For local Podman use (no Swarm) this is P1 for `scale`; `deploy.replicas` may be P2.

```yaml
services:
  worker:
    scale: 3         # currently ⚠️ — only 1 container started
```

#### P1-H: `pull_policy` not implemented

Controls whether an image is pulled before starting. Missing means the engine always attempts a pull for services without `build`, which can be wrong for local-only images.

```yaml
services:
  app:
    image: myapp:local
    pull_policy: never    # currently ❌ — always attempts pull
```

---

### P2 — Important (needed for broad compatibility)

#### P2-A: `deploy.resources.limits.pids` not forwarded

```yaml
deploy:
  resources:
    limits:
      pids: 100    # ⚠️ parsed, not sent to HostConfig.pids_limit
```

#### P2-B: `deploy.resources.(limits|reservations).devices` — GPU support missing

```yaml
deploy:
  resources:
    reservations:
      devices:
        - capabilities: [gpu]
          count: 1            # ❌ not parsed
```

#### P2-C: `blkio_config` entirely missing

Relatively common for I/O-sensitive workloads. All seven sub-fields need a new struct and wiring to `HostConfig`.

#### P2-D: `devices` long-form missing

The spec allows a long-form device syntax with `source`, `target`, `permissions`. Currently only the short string form is parsed.

```yaml
services:
  app:
    devices:
      - source: /dev/ttyUSB0
        target: /dev/ttyUSB0
        permissions: rwm
```

#### P2-E: `device_cgroup_rules` missing

```yaml
services:
  app:
    device_cgroup_rules:
      - 'c 1:3 mr'
      - 'b 7:* rmw'
```

#### P2-F: `pids_limit` (top-level service field) missing

```yaml
services:
  app:
    pids_limit: 256    # ❌ not in Service struct
```

#### P2-G: `uts` namespace mode missing

```yaml
services:
  app:
    uts: host    # ❌ not in Service struct
```

#### P2-H: `networks.*.gw_priority` and `interface_name` not forwarded

```yaml
services:
  app:
    networks:
      frontend:
        gw_priority: 100      # ⚠️ parsed, not applied
        interface_name: eth0  # ❌ not parsed
```

#### P2-I: `post_start` / `pre_stop` lifecycle hooks

```yaml
services:
  app:
    post_start:
      - command: ["/scripts/init.sh"]
    pre_stop:
      - command: ["/scripts/cleanup.sh"]
```

#### P2-J: `build.secrets` (build-time secret mounting)

```yaml
services:
  app:
    build:
      context: .
      secrets:
        - server-certificate    # ❌ not parsed
```

#### P2-K: `build.ssh` (SSH agent forwarding at build time)

```yaml
services:
  app:
    build:
      ssh:
        - default              # ❌ not parsed
```

#### P2-L: `deploy.labels` not merged into container labels

```yaml
deploy:
  labels:
    - "com.example.description=API service"    # ⚠️ parsed, not applied
```

---

### P3 — Nice-to-Have / Swarm/Advanced

#### P3-A: `develop.watch` — file watching / live reload

The entire `develop:` section and `compose watch` command are not implemented. This is primarily a DX feature.

#### P3-B: `annotations` — should use OCI annotations, not labels

Currently annotations are merged into container labels as `annotation.<key>`. Correct behaviour is to pass them as OCI annotations via the Podman-specific annotation API.

#### P3-C: `build.cache_from` / `build.cache_to` — BuildKit cache

```yaml
build:
  cache_from:
    - type=registry,ref=myregistry/myapp:cache
  cache_to:
    - type=registry,ref=myregistry/myapp:cache,mode=max
```

#### P3-D: `build.dockerfile_inline` — inline Dockerfile

```yaml
services:
  app:
    build:
      dockerfile_inline: |
        FROM alpine
        RUN echo hello
```

#### P3-E: `build.extra_hosts` at build time

#### P3-F: `build.no_cache`, `build.pull` flags

#### P3-G: `build.ulimits` at build time

#### P3-H: `build.tags` — additional image tags after build

#### P3-I: `include.env_file` and `include.project_directory` — long-form include not fully honoured

#### P3-J: `mem_swappiness`, `cpu_count`, `cpu_percent`, `cpu_rt_*` (obscure resource fields)

#### P3-K: `network.enable_ipv4: false` — disable IPv4

#### P3-L: `credential_spec` (Windows; not applicable to Podman)

#### P3-M: `external_links` — links to containers outside this compose project

#### P3-N: `label_file` — load labels from file (like env_file but for labels)

#### P3-O: `isolation` (Windows container isolation)

#### P3-P: `gpus` / `models` / `provider` (Docker AI / Cloud extensions)

#### P3-Q: `use_api_socket` — mount container engine socket

#### P3-R: `build.entitlements` / `build.provenance` / `build.sbom` (BuildKit attestations)

#### P3-S: Deploy Swarm-only fields

`deploy.mode`, `deploy.endpoint_mode`, `deploy.update_config`, `deploy.rollback_config`, `deploy.placement.*`, `deploy.restart_policy.*` — these are Swarm-only and have no Podman local equivalent. Document as intentionally skipped.

---

## 12. Summary Counts

| Status | Count |
|---|---|
| ✅ Fully implemented | ~65 |
| 🔶 Partial | ~8 |
| ⚠️ Parsed but not executed | ~35 |
| ❌ Not parsed / not implemented | ~55 |

**Total spec fields analysed:** ~163

---

## 13. Recommended Implementation Order

1. **P1-D** `build.target` — one-liner fix in `engine/build.rs`
2. **P1-F** `dns_opt` — add field to struct + wire to `HostConfig.dns_options`
3. **P1-E** `env_file` long-form — new enum type, update `env_file.rs` loader
4. **P1-C** IPAM network config — build `Ipam` struct in `engine/network.rs`
5. **P1-A/B** `secrets.content` / `secrets.environment` — tempfile materialisation
6. **P1-G** `scale` replicas — loop in `engine/mod.rs` up_with_options
7. **P1-H** `pull_policy` — add to Service struct, gate pull in `engine/build.rs`
8. **P2-A** `deploy.resources.limits.pids` → `HostConfig.pids_limit`
9. **P2-F** `pids_limit` top-level field
10. **P2-G** `uts` namespace
11. **P2-C** `blkio_config` new struct + full wiring
12. **P2-E** `device_cgroup_rules`
13. **P2-I** `post_start` / `pre_stop` lifecycle hooks (exec after start)
14. **P2-B** GPU device reservations
15. **P2-J/K** `build.secrets` / `build.ssh`
16. **P2-L** `deploy.labels` merge
17. **P3-A** `develop.watch` (separate command)
