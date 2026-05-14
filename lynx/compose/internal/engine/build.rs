use std::path::Path;

use bollard::image::{BuildImageOptions, CreateImageOptions};
use bytes::Bytes;
use flate2::write::GzEncoder;
use flate2::Compression;
use futures::StreamExt;
use tracing::{debug, info, warn};
use walkdir::WalkDir;

use crate::compose::types::{BuildConfig, Service};
use crate::error::{ComposeError, Result};

use super::Engine;

impl Engine {
    pub(super) async fn pull_image(&self, service: &Service) -> Result<()> {
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

    pub(super) async fn build_service(&self, service_name: &str, service: &Service) -> Result<()> {
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

        let arg_map = build.args().to_map();
        let env: std::collections::HashMap<String, String> = std::env::vars().collect();
        let mut build_args: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (k, v) in arg_map {
            let value = match v {
                Some(val) => val,
                None => env.get(&k).cloned().unwrap_or_default(),
            };
            build_args.insert(k, value);
        }

        let mut labels: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
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
            networkmode: network_owned,
            platform: platform_owned,
            target: target_owned,
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
}

// ---------------------------------------------------------------------------
// Build context tar
// ---------------------------------------------------------------------------

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
