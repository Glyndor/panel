# Docker Compose Spec → lynx-compose Gap Analysis

**Date:** 2026-05-14 (updated after full P1/P2/P3 implementation and test pass)  
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
| `secrets` | ✅ | ✅ | ✅ | `file:`, `external:`, `content:`, `environment:` all wired |
| `configs` | ✅ | ✅ | ✅ | `file:`, `external:`, `content:`, `environment:` all wired |
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
| `build.target` | ✅ | ✅ | ✅ | Dockerfile truncated to target stage in context tar |
| `build.labels` | ✅ | ✅ | ✅ | |
| `build.network` | ✅ | ✅ | ✅ | → `networkmode` |
| `build.platforms` | ✅ | ✅ | 🔶 | Only first platform taken |
| `build.shm_size` | ✅ | ✅ | ✅ | Forwarded to `BuildImageOptions.shmsize` |
| `build.cache_from` | ✅ | ❌ | ⚠️ | Parsed; bollard 0.17 cachefrom not wired (BuildKit only) |
| `build.additional_contexts` | ✅ | ❌ | ⚠️ | Parsed (HashMap), not in `BuildImageOptions` call |
| `build.dockerfile_inline` | ✅ | ✅ | ✅ | Written to `.dockerfile-inline` in context tar |
| `build.cache_to` | ✅ | ❌ | ⚠️ | Parsed; BuildKit only — no bollard 0.17 equivalent |
| `build.extra_hosts` | ✅ | ✅ | ✅ | Forwarded to `BuildImageOptions.extrahosts` |
| `build.isolation` | ✅ | ❌ | ⚠️ | Parsed; Windows only — not applicable to Podman |
| `build.no_cache` | ✅ | ✅ | ✅ | Forwarded to `BuildImageOptions.nocache` |
| `build.pull` | ✅ | ✅ | ✅ | Forwarded to `BuildImageOptions.pull` |
| `build.ssh` | ✅ | ❌ | ⚠️ | Parsed; BuildKit SSH forwarding — no bollard 0.17 equivalent |
| `build.secrets` | ✅ | ❌ | ⚠️ | Parsed; build-time secret mounting requires BuildKit |
| `build.tags` | ✅ | ✅ | ✅ | Applied via `tag_image` after build |
| `build.ulimits` | ✅ | ❌ | ⚠️ | Parsed; no bollard 0.17 BuildImageOptions.ulimits |
| `build.privileged` | ✅ | ❌ | ⚠️ | Parsed; not in bollard 0.17 BuildImageOptions |
| `build.entitlements` | ✅ | ❌ | ⚠️ | Parsed; BuildKit attestations — no bollard 0.17 equivalent |
| `build.provenance` | ✅ | ❌ | ⚠️ | Parsed; BuildKit provenance — no bollard 0.17 equivalent |
| `build.sbom` | ✅ | ❌ | ⚠️ | Parsed; BuildKit SBOM — no bollard 0.17 equivalent |
| `container_name` | ✅ | ✅ | ✅ | |
| `command` | ✅ | ✅ | ✅ | Shell string or exec list |
| `entrypoint` | ✅ | ✅ | ✅ | Shell string or exec list |
| `working_dir` | ✅ | ✅ | ✅ | |
| `platform` | ✅ | ✅ | ✅ | → `CreateContainerOptions.platform` |
| `pull_policy` | ✅ | ✅ | ✅ | always/missing/never/build fully handled in engine |
| `runtime` | ✅ | ✅ | ✅ | → `HostConfig.runtime` |
| `scale` | ✅ | ✅ | ✅ | Replica loop in engine; indexed container names when scale > 1 |
| `attach` | ✅ | ❌ | ⚠️ | Parsed; log collection flag — no engine action for local stacks |

### 2.2 Environment

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `environment` (map or list) | ✅ | ✅ | ✅ | |
| `env_file` (short — string/list) | ✅ | ✅ | ✅ | |
| `env_file` long-form `path` | ✅ | ✅ | ✅ | Full `EnvFile`/`EnvFileEntry` enum handles long-form |
| `env_file.required` | ✅ | ✅ | ✅ | `required: false` silently skips missing files |
| `env_file.format` | ✅ | ❌ | ⚠️ | Parsed; only `dotenv` format supported |

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
| Long `type: npipe` | ✅ | ❌ | ⚠️ | Parsed; Windows named pipe — no Podman equivalent |
| Long `type: cluster` | ✅ | ❌ | ⚠️ | Parsed; cluster volume type — no local Podman equivalent |
| `source` | ✅ | ✅ | ✅ | |
| `target` | ✅ | ✅ | ✅ | |
| `read_only` | ✅ | ✅ | ✅ | → `ro`/`rw` option |
| `bind.propagation` | ✅ | ✅ | ✅ | Appended to bind string |
| `bind.create_host_path` | ✅ | ✅ | ✅ | `fs::create_dir_all` called before mounting |
| `bind.selinux` | ✅ | ✅ | ✅ | Appended as selinux label option |
| `volume.nocopy` | ✅ | ✅ | ✅ | → `nocopy` mount option |
| `volume.labels` | ✅ | ❌ | ⚠️ | Parsed; not forwarded — volume labels only at create time |
| `volume.driver_config.name` | ✅ | ❌ | ⚠️ | Parsed; no equivalent in bind-mount string |
| `volume.driver_config.options` | ✅ | ❌ | ⚠️ | Same |
| `volume.subpath` | ✅ | ❌ | ⚠️ | Parsed; no Podman API equivalent yet |
| `tmpfs.size` | ✅ | ✅ | ✅ | → `size=N` mount option |
| `tmpfs.mode` | ✅ | ✅ | ✅ | → `mode=NNNN` mount option |
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
| `networks.*.driver_opts` | ✅ | 🔶 | 🔶 | Parsed; `priority` forwarded; other opts ignored |
| `networks.*.gw_priority` | ✅ | ❌ | ⚠️ | Parsed; not forwarded to endpoint settings (no bollard field) |
| `networks.*.priority` | ✅ | 🔶 | 🔶 | Stored in driver_opts as string |
| `networks.*.interface_name` | ✅ | ❌ | ⚠️ | Parsed; no bollard 0.17 EndpointSettings field |
| `network_mode` | ✅ | ✅ | ✅ | → `HostConfig.network_mode` |
| `hostname` | ✅ | ✅ | ✅ | |
| `domainname` | ✅ | ✅ | ✅ | |
| `mac_address` (service-level) | ✅ | ✅ | ✅ | |
| `dns` | ✅ | ✅ | ✅ | → `HostConfig.dns` |
| `dns_opt` | ✅ | ✅ | ✅ | → `HostConfig.dns_options` |
| `dns_search` | ✅ | ✅ | ✅ | → `HostConfig.dns_search` |
| `extra_hosts` | ✅ | ✅ | ✅ | → `HostConfig.extra_hosts` |
| `links` | ✅ | ✅ | ✅ | → `HostConfig.links` (legacy) |
| `external_links` | ✅ | ✅ | ✅ | Merged into `HostConfig.links` alongside `links` |

### 2.6 Secrets & Configs (service-level references)

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `secrets` short form | ✅ | ✅ | ✅ | Mounts `/run/secrets/<name>` |
| `secrets` long `source` | ✅ | ✅ | ✅ | |
| `secrets` long `target` | ✅ | ✅ | ✅ | Custom mount path |
| `secrets` long `uid` | ✅ | ❌ | ⚠️ | Parsed; uid/gid not set on bind-mount (Podman limitation) |
| `secrets` long `gid` | ✅ | ❌ | ⚠️ | Same |
| `secrets` long `mode` | ✅ | ❌ | ⚠️ | Parsed; file permissions not enforced on bind-mount |
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
| `post_start` lifecycle hook | ✅ | ✅ | ✅ | Executed via exec after container start |
| `pre_stop` lifecycle hook | ✅ | ✅ | ✅ | Executed via exec before container stop |

### 2.9 Labels / Annotations / Metadata

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `labels` (map or list) | ✅ | ✅ | ✅ | → container labels; lynx.compose.* auto-added |
| `annotations` (map or list) | ✅ | ✅ | 🔶 | Merged into labels as `annotation.<key>=<val>` — not native OCI annotations |
| `label_file` | ✅ | ✅ | ✅ | Loads labels from file; lower priority than inline labels |
| `profiles` | ✅ | ✅ | ✅ | Services filtered by active profiles |
| `attach` | ✅ | ❌ | ⚠️ | Parsed; log collection flag — no engine action for local stacks |

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
| `credential_spec` | ❌ | ❌ | ❌ | Windows MSA credentials — not applicable to Podman |

### 2.11 Namespaces / Runtime

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `ipc` | ✅ | ✅ | ✅ | → `HostConfig.ipc_mode` |
| `pid` | ✅ | ✅ | ✅ | → `HostConfig.pid_mode` |
| `uts` | ✅ | ✅ | ✅ | → `HostConfig.uts_mode` |
| `cgroup` | ✅ | ❌ | ⚠️ | Parsed; bollard 0.17 has no `cgroupns_mode` field |
| `cgroup_parent` | ✅ | ✅ | ✅ | → `HostConfig.cgroup_parent` |
| `isolation` | ❌ | ❌ | ❌ | Windows isolation mode — not applicable to Podman |
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
| `cpus` | ✅ | ✅ | ✅ | → `nano_cpus` via `parse_cpus` |
| `cpu_count` | ✅ | ✅ | ✅ | → `HostConfig.cpu_count` |
| `cpu_percent` | ✅ | ✅ | ✅ | → `HostConfig.cpu_percent` |
| `cpu_rt_runtime` | ✅ | ✅ | ✅ | → `HostConfig.cpu_realtime_runtime` |
| `cpu_rt_period` | ✅ | ✅ | ✅ | → `HostConfig.cpu_realtime_period` |
| `mem_limit` | ✅ | ✅ | ✅ | → `HostConfig.memory` |
| `memswap_limit` | ✅ | ✅ | ✅ | → `HostConfig.memory_swap` |
| `mem_reservation` | ✅ | ✅ | ✅ | → `HostConfig.memory_reservation` |
| `mem_swappiness` | ✅ | ✅ | ✅ | → `HostConfig.memory_swappiness` |
| `oom_kill_disable` | ✅ | ✅ | ✅ | |
| `oom_score_adj` | ✅ | ✅ | ✅ | |
| `pids_limit` | ✅ | ✅ | ✅ | → `HostConfig.pids_limit` (merged with deploy.resources.limits.pids) |
| `blkio_config` | ✅ | ✅ | ✅ | Full struct + all 6 fields wired to `HostConfig` |

### 2.13 Devices / Storage

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `devices` (short form `host:container[:perm]`) | ✅ | ✅ | ✅ | → `DeviceMapping` |
| `device_cgroup_rules` | ✅ | ✅ | ✅ | → `HostConfig.device_cgroup_rules` |
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
| `deploy.mode` | ✅ | ❌ | ⚠️ | Parsed; Swarm-only — no local Podman equivalent |
| `deploy.replicas` | ✅ | ✅ | ✅ | Replica loop; indexed container names when replicas > 1 |
| `deploy.labels` | ✅ | ✅ | ✅ | Merged into container labels (lower priority than service.labels) |
| `deploy.endpoint_mode` | ✅ | ❌ | ⚠️ | Parsed; Swarm-only — no local Podman equivalent |
| `deploy.resources.limits.cpus` | ✅ | ✅ | ✅ | → `nano_cpus` via `resolve_resources` |
| `deploy.resources.limits.memory` | ✅ | ✅ | ✅ | → `HostConfig.memory` |
| `deploy.resources.limits.pids` | ✅ | ✅ | ✅ | → `HostConfig.pids_limit` |
| `deploy.resources.reservations.cpus` | ✅ | ❌ | ⚠️ | Parsed; no Podman CPU reservation API |
| `deploy.resources.reservations.memory` | ✅ | ✅ | ✅ | → `HostConfig.memory_reservation` |
| `deploy.resources.reservations.pids` | ✅ | ❌ | ⚠️ | Parsed; limits.pids takes precedence |
| `deploy.resources.reservations.devices` | ✅ | ✅ | ✅ | GPU reservations → `DeviceRequest` list |
| `deploy.restart_policy.*` | ✅ | ❌ | ⚠️ | Parsed; Swarm-only rolling restart policy |
| `deploy.update_config.*` | ✅ | ❌ | ⚠️ | Parsed; Swarm rolling update — no local equivalent |
| `deploy.rollback_config.*` | ✅ | ❌ | ⚠️ | Parsed; Swarm rollback — no local equivalent |
| `deploy.placement.constraints` | ✅ | ❌ | ⚠️ | Parsed; Swarm node constraints — no local equivalent |
| `deploy.placement.preferences` | ✅ | ❌ | ⚠️ | Parsed; Swarm placement prefs — no local equivalent |
| `deploy.placement.max_replicas_per_node` | ✅ | ❌ | ⚠️ | Parsed; Swarm-only |

### 2.17 Advanced / Newer Fields

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `gpus` | ✅ | ✅ | ✅ | → `DeviceRequest` with `gpu` capability; `all` maps to count=-1 |
| `models` | ❌ | ❌ | ❌ | Docker AI model service integration — not in Podman |
| `provider` | ❌ | ❌ | ❌ | Docker Cloud external service management — not applicable |
| `develop` / `develop.watch` | ✅ | ❌ | ⚠️ | Parsed; file-watch engine (`watch.rs`) present but not wired to `up` |
| `use_api_socket` | ❌ | ❌ | ❌ | Container engine socket access — not parsed |
| `extends` (service-level) | ✅ | ✅ | ✅ | Cross-file and same-file |
| `external_links` | ✅ | ✅ | ✅ | Merged into `HostConfig.links` alongside `links` |

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
| `ipam.driver` | ✅ | ✅ | ✅ | → `CreateNetworkOptions.ipam.driver` |
| `ipam.config[].subnet` | ✅ | ✅ | ✅ | → `CreateNetworkOptions.ipam.config[].subnet` |
| `ipam.config[].gateway` | ✅ | ✅ | ✅ | → `CreateNetworkOptions.ipam.config[].gateway` |
| `ipam.config[].ip_range` | ✅ | ✅ | ✅ | → `CreateNetworkOptions.ipam.config[].ip_range` |
| `ipam.config[].aux_addresses` | ✅ | ✅ | ✅ | → `CreateNetworkOptions.ipam.config[].auxiliary_addresses` |
| `ipam.options` | ✅ | ✅ | ✅ | → `CreateNetworkOptions.ipam.options` |

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
| `content` | ✅ | ✅ | ✅ | Written to tempfile; bind-mounted read-only |
| `environment` | ✅ | ✅ | ✅ | Env var value written to tempfile; bind-mounted read-only |
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
| `content` | ✅ | ✅ | ✅ | Written to tempfile; bind-mounted read-only |
| `environment` | ✅ | ✅ | ✅ | Env var value written to tempfile; bind-mounted read-only |
| `labels` | ✅ | ❌ | ⚠️ | Parsed; no Podman equivalent |
| `template_driver` | ❌ | ❌ | ❌ | Not in ConfigConfig struct |
| `driver` | ❌ | ❌ | ❌ | Not in ConfigConfig struct |
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
| `path` | ✅ | ❌ | ⚠️ | Parsed; `watch.rs` engine exists but not wired to `up` command |
| `action` (sync/rebuild/restart/sync+restart/sync+exec) | ✅ | ❌ | ⚠️ | Same |
| `target` | ✅ | ❌ | ⚠️ | Same |
| `ignore` | ✅ | ❌ | ⚠️ | Same |
| `include` | ✅ | ❌ | ⚠️ | Same |
| `initial_sync` | ✅ | ❌ | ⚠️ | Same |
| `exec.command` | ✅ | ❌ | ⚠️ | Same |

---

## 10. `blkio_config` (service-level)

| Field | Parsed | Executed | Status | Notes |
|---|---|---|---|---|
| `weight` | ✅ | ✅ | ✅ | → `HostConfig.blkio_weight` |
| `weight_device[].path` | ✅ | ✅ | ✅ | → `HostConfig.blkio_weight_device` |
| `weight_device[].weight` | ✅ | ✅ | ✅ | Same |
| `device_read_bps[].path` | ✅ | ✅ | ✅ | → `HostConfig.blkio_device_read_bps` |
| `device_read_bps[].rate` | ✅ | ✅ | ✅ | Size string or integer → bytes/s |
| `device_write_bps[].path` | ✅ | ✅ | ✅ | → `HostConfig.blkio_device_write_bps` |
| `device_write_bps[].rate` | ✅ | ✅ | ✅ | Same |
| `device_read_iops[].path` | ✅ | ✅ | ✅ | → `HostConfig.blkio_device_read_i_ops` |
| `device_read_iops[].rate` | ✅ | ✅ | ✅ | Integer IOPS |
| `device_write_iops[].path` | ✅ | ✅ | ✅ | → `HostConfig.blkio_device_write_i_ops` |
| `device_write_iops[].rate` | ✅ | ✅ | ✅ | Same |

---

## 11. Intentionally Not Implemented (Swarm / Windows / Docker-AI)

These fields are parsed (where sensible) but have no Podman local equivalent
and are deliberately not wired to the engine:

| Category | Fields |
|---|---|
| **Swarm-only** | `deploy.mode`, `deploy.endpoint_mode`, `deploy.restart_policy.*`, `deploy.update_config.*`, `deploy.rollback_config.*`, `deploy.placement.*` |
| **Windows-only** | `credential_spec`, `isolation` (service), `build.isolation`, `type: npipe` |
| **BuildKit / Docker-only** | `build.cache_from`, `build.cache_to`, `build.ssh`, `build.secrets`, `build.ulimits`, `build.privileged`, `build.entitlements`, `build.provenance`, `build.sbom` |
| **Docker AI / Cloud** | `models`, `provider`, `use_api_socket` |
| **No bollard 0.17 field** | `cgroup` (cgroupns_mode), `networks.*.gw_priority`, `networks.*.interface_name` |

---

## 12. Summary Counts

| Status | Count |
|---|---|
| ✅ Fully implemented (parse + wire) | ~105 |
| 🔶 Partial | ~4 |
| ⚠️ Parsed but not executed | ~30 |
| ❌ Not parsed / not implemented | ~8 |

**Total spec fields analysed:** ~147

### Test coverage

| Test suite | Tests |
|---|---|
| parse (unit: basic, fields, coverage, anchors, extends, include, order) | 153 |
| env_file loading and merge | 9 |
| ports conversion and formats | 23 |
| substitute modifiers and dotenv | 37 |
| engine unit (build.rs, container.rs, volume.rs — internal `#[cfg(test)]`) | 16 |
| **Total** | **238** |
