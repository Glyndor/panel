//! `env_file:` loading for services.
//!
//! Reads KEY=VALUE pairs from files listed in a service's `env_file:` field.
//! Service-level `environment:` takes precedence over `env_file:` values.

use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;

/// Load all `env_file` paths relative to `base_dir`.
///
/// Returns a merged map.  If the same key appears in multiple files, the
/// first file wins (earlier entries in the list have higher priority).
/// `env_file:` never overrides service-level `environment:`.
pub fn load_env_files(paths: &[String], base_dir: &Path) -> Result<HashMap<String, String>> {
    let mut result: HashMap<String, String> = HashMap::new();

    for rel in paths {
        let abs = base_dir.join(rel);
        let content = match std::fs::read_to_string(&abs) {
            Ok(c) => c,
            Err(e) => {
                return Err(crate::error::ComposeError::Io(e));
            }
        };

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let (key, value) = if let Some(eq) = trimmed.find('=') {
                let k = trimmed[..eq].trim().to_string();
                let v = trimmed[eq + 1..].to_string();
                (k, v)
            } else {
                // KEY without value — treat as empty string.
                (trimmed.to_string(), String::new())
            };

            if key.is_empty() {
                continue;
            }

            // First file wins for duplicate keys.
            result.entry(key).or_insert(value);
        }
    }

    Ok(result)
}

/// Merge env_file values with service environment.
///
/// `service_env` takes precedence: only keys not already in `service_env` are added.
pub fn merge_env(
    service_env: HashMap<String, Option<String>>,
    env_file_vars: HashMap<String, String>,
) -> Vec<String> {
    let mut merged: HashMap<String, Option<String>> = env_file_vars
        .into_iter()
        .map(|(k, v)| (k, Some(v)))
        .collect();

    // Service env overrides env_file.
    for (k, v) in service_env {
        merged.insert(k, v);
    }

    merged
        .into_iter()
        .map(|(k, v)| match v {
            Some(val) => format!("{k}={val}"),
            None => k,
        })
        .collect()
}

