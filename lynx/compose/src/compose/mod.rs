//! Compose file parsing and service ordering.

pub mod types;
#[cfg(test)]
mod tests;

use std::path::Path;
use types::ComposeFile;
use crate::error::{ComposeError, Result};
use crate::substitute;

/// Parse a compose file from disk, applying variable substitution.
///
/// The directory containing `path` is used as the base for `.env` loading.
pub fn parse_file(path: &Path) -> Result<ComposeFile> {
    let content = std::fs::read_to_string(path).map_err(|_| {
        ComposeError::FileNotFound(path.display().to_string())
    })?;

    let dir = path.parent().unwrap_or(Path::new("."));
    let vars = substitute::build_vars(dir);
    let substituted = substitute::substitute(&content, &vars)?;

    parse_str_raw(&substituted)
}

/// Parse a compose YAML string that has already had variable substitution applied.
pub fn parse_str(content: &str) -> Result<ComposeFile> {
    // Apply substitution using only process environment (no `.env` file path available).
    let vars = crate::substitute::build_vars(Path::new("."));
    let substituted = substitute::substitute(content, &vars)?;
    parse_str_raw(&substituted)
}

/// Parse raw (already-substituted) YAML into a `ComposeFile`.
pub(crate) fn parse_str_raw(content: &str) -> Result<ComposeFile> {
    let file: ComposeFile = serde_yaml::from_str(content)?;
    Ok(file)
}

/// Compute a topological start order for all services.
///
/// Returns service names in the order they should be started (dependencies first).
/// Returns [`ComposeError::CircularDependency`] if a cycle is detected, or
/// [`ComposeError::ServiceNotFound`] if a dependency references an unknown service.
pub fn resolve_order(file: &ComposeFile) -> Result<Vec<String>> {
    let services: Vec<&str> = file.services.keys().map(|s| s.as_str()).collect();
    let mut in_degree: std::collections::HashMap<&str, usize> = services
        .iter()
        .map(|&s| (s, 0))
        .collect();
    let mut graph: std::collections::HashMap<&str, Vec<&str>> = services
        .iter()
        .map(|&s| (s, vec![]))
        .collect();

    for (name, service) in &file.services {
        for dep in service.depends_on.service_names() {
            if !file.services.contains_key(&dep) {
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
            .map_or(&[], |v| v.as_slice())
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
