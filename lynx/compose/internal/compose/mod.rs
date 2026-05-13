//! Compose file parsing, `extends:` resolution, `include:` merging, and
//! topological service ordering.

pub mod types;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::error::{ComposeError, Result};
use crate::substitute;
use types::{ComposeFile, DependsOn, EnvVars, Labels, Service, ServiceNetworks, StringOrList, Sysctls};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a compose file from disk, applying variable substitution and
/// resolving `extends:` / `include:` directives.
///
/// The directory containing `path` is used as the base for `.env` loading
/// and for resolving relative include / extends file paths.
pub fn parse_file(path: &Path) -> Result<ComposeFile> {
    let abs = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let dir = abs.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut file = parse_file_inner(&abs, &dir)?;

    // Process include directives first (they bring in additional services).
    let includes = std::mem::take(&mut file.include);
    for inc in includes {
        for rel in inc.paths() {
            let inc_path = dir.join(&rel);
            let inc_dir = inc_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| dir.clone());
            let included = parse_file_inner(&inc_path, &inc_dir)?;
            merge_compose_file(&mut file, included);
        }
    }

    // Resolve extends after includes — extends may reference services from
    // files we just merged in.
    resolve_all_extends(&mut file, &dir)?;

    Ok(file)
}

/// Parse a compose YAML string (no file I/O).
///
/// Variable substitution is applied using only the process environment.
/// `extends: { file: ... }` and `include:` directives that reference
/// external files are not resolved — use [`parse_file`] for that.
pub fn parse_str(content: &str) -> Result<ComposeFile> {
    let vars = substitute::build_vars(Path::new("."));
    let substituted = substitute::substitute(content, &vars)?;
    let mut file = deserialize_with_merge(&substituted)?;

    // Resolve extends within the same file (no file: lookups).
    resolve_extends_same_file(&mut file)?;
    Ok(file)
}

/// Parse raw (already-substituted) YAML into a `ComposeFile` without any
/// post-processing.  This is mainly used internally and by callers that
/// want to handle extends / includes themselves.
pub fn parse_str_raw(content: &str) -> Result<ComposeFile> {
    deserialize_with_merge(content)
}

/// Deserialize a compose YAML string while applying YAML merge keys
/// (`<<: *anchor`) — they are otherwise left literal by serde_yaml.
fn deserialize_with_merge(content: &str) -> Result<ComposeFile> {
    let mut value: serde_yaml::Value = serde_yaml::from_str(content)?;
    value.apply_merge().ok();
    let file: ComposeFile = serde_yaml::from_value(value)?;
    Ok(file)
}

/// Compute a topological start order for all services.
///
/// Returns service names in the order they should be started (dependencies first).
/// Returns [`ComposeError::CircularDependency`] if a cycle is detected, or
/// [`ComposeError::ServiceNotFound`] if a dependency references an unknown service.
pub fn resolve_order(file: &ComposeFile) -> Result<Vec<String>> {
    let services: Vec<&str> = file.services.keys().map(|s| s.as_str()).collect();
    let mut in_degree: HashMap<&str, usize> = services.iter().map(|&s| (s, 0)).collect();
    let mut graph: HashMap<&str, Vec<&str>> =
        services.iter().map(|&s| (s, vec![])).collect();

    for (name, service) in &file.services {
        for dep in service.depends_on.service_names() {
            if !file.services.contains_key(&dep) {
                // If marked as not required, just skip it.
                if !service.depends_on.required_for(&dep) {
                    continue;
                }
                return Err(ComposeError::ServiceNotFound(dep));
            }
            if let Some(neighbors) = graph.get_mut(dep.as_str()) {
                neighbors.push(name.as_str());
            }
            if let Some(deg) = in_degree.get_mut(name.as_str()) {
                *deg += 1;
            }
        }
    }

    let mut queue: std::collections::VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&s, _)| s)
        .collect();

    let mut order = Vec::new();
    while let Some(node) = queue.pop_front() {
        order.push(node.to_string());
        let neighbors: Vec<&str> = graph
            .get(node)
            .map_or(&[][..], |v| v.as_slice())
            .to_vec();
        for neighbor in neighbors {
            if let Some(deg) = in_degree.get_mut(neighbor) {
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(neighbor);
                }
            }
        }
    }

    if order.len() != services.len() {
        return Err(ComposeError::CircularDependency(
            "cycle detected in depends_on".into(),
        ));
    }

    Ok(order)
}

// ---------------------------------------------------------------------------
// Internal: substitute + parse one file
// ---------------------------------------------------------------------------

fn parse_file_inner(path: &Path, dir: &Path) -> Result<ComposeFile> {
    let content = std::fs::read_to_string(path)
        .map_err(|_| ComposeError::FileNotFound(path.display().to_string()))?;

    let vars = substitute::build_vars(dir);
    let substituted = substitute::substitute(&content, &vars)?;

    deserialize_with_merge(&substituted)
}

// ---------------------------------------------------------------------------
// Include merging
// ---------------------------------------------------------------------------

/// Merge `other` into `target`.  Services / volumes / networks / secrets /
/// configs from `other` are added; existing entries in `target` win on
/// conflict (the parent file overrides included content).
fn merge_compose_file(target: &mut ComposeFile, other: ComposeFile) {
    for (k, v) in other.services {
        target.services.entry(k).or_insert(v);
    }
    for (k, v) in other.volumes {
        target.volumes.entry(k).or_insert(v);
    }
    for (k, v) in other.networks {
        target.networks.entry(k).or_insert(v);
    }
    for (k, v) in other.secrets {
        target.secrets.entry(k).or_insert(v);
    }
    for (k, v) in other.configs {
        target.configs.entry(k).or_insert(v);
    }
}

// ---------------------------------------------------------------------------
// Extends resolution
// ---------------------------------------------------------------------------

/// Resolve `extends:` only within the same file (no file: references followed).
/// Used by [`parse_str`] where we have no on-disk path.
fn resolve_extends_same_file(file: &mut ComposeFile) -> Result<()> {
    let names: Vec<String> = file.services.keys().cloned().collect();
    for name in names {
        let mut visited: HashSet<String> = HashSet::new();
        resolve_one_extends_in_memory(file, &name, &mut visited)?;
    }
    Ok(())
}

/// Resolve `extends:` for every service in `file`, including chains across
/// other compose files referenced by `extends.file`.
fn resolve_all_extends(file: &mut ComposeFile, base_dir: &Path) -> Result<()> {
    let names: Vec<String> = file.services.keys().cloned().collect();
    for name in names {
        let mut visited: HashSet<String> = HashSet::new();
        resolve_one_extends(file, &name, base_dir, &mut visited)?;
    }
    Ok(())
}

/// Resolve `extends:` for a single service in-memory only.
fn resolve_one_extends_in_memory(
    file: &mut ComposeFile,
    name: &str,
    visited: &mut HashSet<String>,
) -> Result<()> {
    if !visited.insert(name.to_string()) {
        return Err(ComposeError::Extends(format!("circular extends at {name}")));
    }

    let extends = match file
        .services
        .get(name)
        .and_then(|s| s.extends.clone())
    {
        Some(e) => e,
        None => return Ok(()),
    };

    if extends.file().is_some() {
        return Err(ComposeError::Extends(format!(
            "service '{name}' uses 'extends.file' but parser was given a string, not a path"
        )));
    }

    let base_name = extends.service().to_string();
    if base_name == name {
        return Err(ComposeError::Extends(format!(
            "service '{name}' extends itself"
        )));
    }

    // Recursively resolve the base service first.
    if file.services.get(&base_name).is_none() {
        return Err(ComposeError::Extends(format!(
            "service '{name}' extends unknown service '{base_name}'"
        )));
    }
    resolve_one_extends_in_memory(file, &base_name, visited)?;

    let base = file
        .services
        .get(&base_name)
        .cloned()
        .ok_or_else(|| ComposeError::Extends(base_name.clone()))?;

    if let Some(svc) = file.services.get_mut(name) {
        let merged = merge_service(base, svc.clone());
        *svc = merged;
        svc.extends = None;
    }

    Ok(())
}

/// Resolve `extends:` for a single service, possibly loading other compose
/// files for `extends.file` references.
fn resolve_one_extends(
    file: &mut ComposeFile,
    name: &str,
    base_dir: &Path,
    visited: &mut HashSet<String>,
) -> Result<()> {
    if !visited.insert(name.to_string()) {
        return Err(ComposeError::Extends(format!("circular extends at {name}")));
    }

    let extends = match file
        .services
        .get(name)
        .and_then(|s| s.extends.clone())
    {
        Some(e) => e,
        None => return Ok(()),
    };

    let base_name = extends.service().to_string();

    let base_service = if let Some(file_path) = extends.file() {
        let abs = base_dir.join(file_path);
        let abs = abs.canonicalize().unwrap_or(abs);
        let dir = abs
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| base_dir.to_path_buf());
        let mut other = parse_file_inner(&abs, &dir)?;
        // Recursively resolve extends in the loaded file.
        let mut nested_visited: HashSet<String> = HashSet::new();
        resolve_one_extends(&mut other, &base_name, &dir, &mut nested_visited)?;
        other
            .services
            .swap_remove(&base_name)
            .ok_or_else(|| {
                ComposeError::Extends(format!(
                    "service '{base_name}' not found in {}",
                    abs.display()
                ))
            })?
    } else {
        if base_name == name {
            return Err(ComposeError::Extends(format!(
                "service '{name}' extends itself"
            )));
        }
        if !file.services.contains_key(&base_name) {
            return Err(ComposeError::Extends(format!(
                "service '{name}' extends unknown service '{base_name}'"
            )));
        }
        // Resolve the base in-place first.
        resolve_one_extends(file, &base_name, base_dir, visited)?;
        file.services
            .get(&base_name)
            .cloned()
            .ok_or_else(|| ComposeError::Extends(base_name.clone()))?
    };

    if let Some(svc) = file.services.get_mut(name) {
        let merged = merge_service(base_service, svc.clone());
        *svc = merged;
        svc.extends = None;
    }

    Ok(())
}

/// Merge `override_svc` over `base`.  `override_svc` wins for any field
/// it explicitly sets; `Vec` / `Map` collections are completely replaced
/// when the override is non-empty (matches docker-compose's semantics for
/// most fields; `command`, `entrypoint`, `ports`, `volumes` are replaced).
fn merge_service(base: Service, override_svc: Service) -> Service {
    fn opt<T>(o: Option<T>, b: Option<T>) -> Option<T> {
        o.or(b)
    }

    fn merge_envvars(base: EnvVars, over: EnvVars) -> EnvVars {
        if matches!(over, EnvVars::Empty) && !matches!(base, EnvVars::Empty) {
            return base;
        }
        if matches!(base, EnvVars::Empty) {
            return over;
        }
        // Merge into a map: base entries first, override wins per-key.
        let mut merged: indexmap::IndexMap<String, Option<serde_yaml::Value>> =
            indexmap::IndexMap::new();
        for (k, v) in base.to_map() {
            merged.insert(k, v.map(serde_yaml::Value::String));
        }
        for (k, v) in over.to_map() {
            merged.insert(k, v.map(serde_yaml::Value::String));
        }
        EnvVars::Map(merged)
    }

    fn merge_labels(base: Labels, over: Labels) -> Labels {
        if base.is_empty() && over.is_empty() {
            return Labels::Empty;
        }
        let mut map: indexmap::IndexMap<String, String> = indexmap::IndexMap::new();
        for (k, v) in base.to_map() {
            map.insert(k, v);
        }
        for (k, v) in over.to_map() {
            map.insert(k, v);
        }
        Labels::Map(map)
    }

    fn merge_vec<T: Clone>(base: Vec<T>, over: Vec<T>) -> Vec<T> {
        if over.is_empty() { base } else { over }
    }

    fn merge_string_or_list(base: StringOrList, over: StringOrList) -> StringOrList {
        if over.is_empty() { base } else { over }
    }

    Service {
        image: opt(override_svc.image, base.image),
        build: override_svc.build.or(base.build),
        extends: override_svc.extends.or(base.extends),
        command: override_svc.command.or(base.command),
        entrypoint: override_svc.entrypoint.or(base.entrypoint),
        ports: merge_vec(base.ports, override_svc.ports),
        expose: merge_vec(base.expose, override_svc.expose),
        environment: merge_envvars(base.environment, override_svc.environment),
        env_file: merge_string_or_list(base.env_file, override_svc.env_file),
        volumes: merge_vec(base.volumes, override_svc.volumes),
        tmpfs: merge_string_or_list(base.tmpfs, override_svc.tmpfs),
        volumes_from: merge_vec(base.volumes_from, override_svc.volumes_from),
        configs: merge_vec(base.configs, override_svc.configs),
        secrets: merge_vec(base.secrets, override_svc.secrets),
        networks: if matches!(override_svc.networks, ServiceNetworks::Empty) {
            base.networks
        } else {
            override_svc.networks
        },
        hostname: override_svc.hostname.or(base.hostname),
        domainname: override_svc.domainname.or(base.domainname),
        mac_address: override_svc.mac_address.or(base.mac_address),
        links: merge_vec(base.links, override_svc.links),
        extra_hosts: merge_vec(base.extra_hosts, override_svc.extra_hosts),
        dns: merge_string_or_list(base.dns, override_svc.dns),
        dns_search: merge_string_or_list(base.dns_search, override_svc.dns_search),
        network_mode: override_svc.network_mode.or(base.network_mode),
        depends_on: if matches!(override_svc.depends_on, DependsOn::Empty) {
            base.depends_on
        } else {
            override_svc.depends_on
        },
        healthcheck: override_svc.healthcheck.or(base.healthcheck),
        restart: override_svc.restart.or(base.restart),
        stop_signal: override_svc.stop_signal.or(base.stop_signal),
        stop_grace_period: override_svc.stop_grace_period.or(base.stop_grace_period),
        profiles: merge_vec(base.profiles, override_svc.profiles),
        labels: merge_labels(base.labels, override_svc.labels),
        annotations: merge_labels(base.annotations, override_svc.annotations),
        container_name: override_svc.container_name.or(base.container_name),
        user: override_svc.user.or(base.user),
        working_dir: override_svc.working_dir.or(base.working_dir),
        group_add: merge_vec(base.group_add, override_svc.group_add),
        platform: override_svc.platform.or(base.platform),
        cap_add: merge_vec(base.cap_add, override_svc.cap_add),
        cap_drop: merge_vec(base.cap_drop, override_svc.cap_drop),
        security_opt: merge_vec(base.security_opt, override_svc.security_opt),
        read_only: override_svc.read_only.or(base.read_only),
        privileged: override_svc.privileged.or(base.privileged),
        init: override_svc.init.or(base.init),
        tty: override_svc.tty.or(base.tty),
        stdin_open: override_svc.stdin_open.or(base.stdin_open),
        runtime: override_svc.runtime.or(base.runtime),
        shm_size: override_svc.shm_size.or(base.shm_size),
        userns_mode: override_svc.userns_mode.or(base.userns_mode),
        pid: override_svc.pid.or(base.pid),
        ipc: override_svc.ipc.or(base.ipc),
        cgroup_parent: override_svc.cgroup_parent.or(base.cgroup_parent),
        cgroup: override_svc.cgroup.or(base.cgroup),
        devices: merge_vec(base.devices, override_svc.devices),
        storage_opt: {
            let mut m = base.storage_opt;
            for (k, v) in override_svc.storage_opt {
                m.insert(k, v);
            }
            m
        },
        scale: override_svc.scale.or(base.scale),
        cpu_shares: override_svc.cpu_shares.or(base.cpu_shares),
        cpu_quota: override_svc.cpu_quota.or(base.cpu_quota),
        cpu_period: override_svc.cpu_period.or(base.cpu_period),
        cpuset: override_svc.cpuset.or(base.cpuset),
        mem_limit: override_svc.mem_limit.or(base.mem_limit),
        memswap_limit: override_svc.memswap_limit.or(base.memswap_limit),
        mem_reservation: override_svc.mem_reservation.or(base.mem_reservation),
        oom_kill_disable: override_svc.oom_kill_disable.or(base.oom_kill_disable),
        oom_score_adj: override_svc.oom_score_adj.or(base.oom_score_adj),
        logging: override_svc.logging.or(base.logging),
        sysctls: if matches!(override_svc.sysctls, Sysctls::Empty) {
            base.sysctls
        } else {
            override_svc.sysctls
        },
        ulimits: {
            let mut m = base.ulimits;
            for (k, v) in override_svc.ulimits {
                m.insert(k, v);
            }
            m
        },
        deploy: override_svc.deploy.or(base.deploy),
    }
}

