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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_basic_env_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.env");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "# comment").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "DB_HOST=localhost").unwrap();
        writeln!(f, "PORT=5432").unwrap();
        writeln!(f, "NOVALUE").unwrap();

        let map = load_env_files(&["app.env".to_string()], dir.path()).unwrap();
        assert_eq!(map["DB_HOST"], "localhost");
        assert_eq!(map["PORT"], "5432");
        assert_eq!(map["NOVALUE"], "");
    }

    #[test]
    fn env_file_as_string() {
        use crate::compose::types::StringOrList;
        let sol = StringOrList::Single("file.env".to_string());
        let list = sol.to_list();
        assert_eq!(list, vec!["file.env"]);
    }

    #[test]
    fn env_file_as_list() {
        use crate::compose::types::StringOrList;
        let sol = StringOrList::List(vec!["a.env".to_string(), "b.env".to_string()]);
        let list = sol.to_list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn service_env_overrides_env_file() {
        let mut service_env = HashMap::new();
        service_env.insert("KEY".to_string(), Some("from-service".to_string()));

        let mut env_file_vars = HashMap::new();
        env_file_vars.insert("KEY".to_string(), "from-file".to_string());
        env_file_vars.insert("EXTRA".to_string(), "extra".to_string());

        let merged = merge_env(service_env, env_file_vars);
        let map: HashMap<_, _> = merged
            .iter()
            .filter_map(|s| {
                let mut it = s.splitn(2, '=');
                Some((it.next()?.to_string(), it.next()?.to_string()))
            })
            .collect();

        assert_eq!(map["KEY"], "from-service");
        assert_eq!(map["EXTRA"], "extra");
    }
}
