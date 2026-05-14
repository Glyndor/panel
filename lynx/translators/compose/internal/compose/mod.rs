//! Compose file parsing, `extends:` resolution, `include:` merging, and
//! topological service ordering.

pub mod types;

mod extends;
mod include;

use std::collections::HashMap;
use std::path::Path;

use crate::error::{ComposeError, Result};
use crate::substitute;
use types::ComposeFile;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a compose file from disk, applying variable substitution and
/// resolving `extends:` / `include:` directives.
pub fn parse_file(path: &Path) -> Result<ComposeFile> {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let dir = abs.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut file = parse_file_inner(&abs, &dir)?;

    let includes = std::mem::take(&mut file.include);
    for inc in includes {
        let (extra_env_files, project_dir_override) = match &inc {
            types::IncludeConfig::Long { env_file, project_directory, .. } => (
                env_file.as_ref().map(|ef| ef.to_list()).unwrap_or_default(),
                project_directory.as_ref().map(|pd| dir.join(pd)),
            ),
            _ => (vec![], None),
        };
        for rel in inc.paths() {
            let inc_path = dir.join(&rel);
            let inc_dir = project_dir_override.clone().unwrap_or_else(|| {
                inc_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| dir.clone())
            });
            let included = parse_file_inner_with_env(&inc_path, &inc_dir, &extra_env_files)?;
            include::merge_compose_file(&mut file, included);
        }
    }

    extends::resolve_all_extends(&mut file, &dir)?;
    Ok(file)
}

/// Parse a compose YAML string (no file I/O).
///
/// Variable substitution is applied using only the process environment.
/// `extends: { file: ... }` and `include:` directives are not resolved —
/// use [`parse_file`] for that.
pub fn parse_str(content: &str) -> Result<ComposeFile> {
    let vars = substitute::build_vars(Path::new("."));
    let substituted = substitute::substitute(content, &vars)?;
    let mut file = deserialize_with_merge(&substituted)?;
    extends::resolve_extends_same_file(&mut file)?;
    Ok(file)
}

/// Parse raw (already-substituted) YAML into a `ComposeFile` without any
/// post-processing.
pub fn parse_str_raw(content: &str) -> Result<ComposeFile> {
    deserialize_with_merge(content)
}

/// Compute a topological start order for all services (Kahn's algorithm).
///
/// Returns service names dependencies-first.
/// Errors on cycles ([`ComposeError::CircularDependency`]) or missing required
/// dependencies ([`ComposeError::ServiceNotFound`]).
pub fn resolve_order(file: &ComposeFile) -> Result<Vec<String>> {
    let services: Vec<&str> = file.services.keys().map(|s| s.as_str()).collect();
    let mut in_degree: HashMap<&str, usize> = services.iter().map(|&s| (s, 0)).collect();
    let mut graph: HashMap<&str, Vec<&str>> = services.iter().map(|&s| (s, vec![])).collect();

    for (name, service) in &file.services {
        for dep in service.depends_on.service_names() {
            if !file.services.contains_key(&dep) {
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
// Internal helpers
// ---------------------------------------------------------------------------

pub(crate) fn parse_file_inner(path: &Path, dir: &Path) -> Result<ComposeFile> {
    parse_file_inner_with_env(path, dir, &[])
}

pub(crate) fn parse_file_inner_with_env(
    path: &Path,
    dir: &Path,
    extra_env_files: &[String],
) -> Result<ComposeFile> {
    let content = std::fs::read_to_string(path)
        .map_err(|_| ComposeError::FileNotFound(path.display().to_string()))?;
    let vars = if extra_env_files.is_empty() {
        substitute::build_vars(dir)
    } else {
        substitute::build_vars_with_env_files(dir, extra_env_files)
    };
    let substituted = substitute::substitute(&content, &vars)?;
    deserialize_with_merge(&substituted)
}

fn deserialize_with_merge(content: &str) -> Result<ComposeFile> {
    let mut value: serde_yaml::Value = serde_yaml::from_str(content)?;
    value.apply_merge().ok();
    let file: ComposeFile = serde_yaml::from_value(value)?;
    Ok(file)
}
