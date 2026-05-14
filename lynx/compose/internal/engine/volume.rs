use std::collections::HashMap;

use bollard::volume::CreateVolumeOptions;
use tracing::info;

use crate::compose::types::{
    BindOptions, ComposeFile, ConfigConfig, SecretConfig, Service, ServiceConfigRef,
    ServiceSecretRef, VolumeMount, VolumeOptions, VolumeType,
};
use crate::error::{ComposeError, Result};

use super::Engine;

impl Engine {
    pub(super) async fn create_volumes(&self, file: &ComposeFile) -> Result<()> {
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
}

// ---------------------------------------------------------------------------
// Free helpers (pub(super) for container.rs)
// ---------------------------------------------------------------------------

pub(super) fn build_binds(service: &Service) -> Vec<String> {
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
                out.push(format!("{src}:{target}:{}", opts.join(",")));
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
        opts.push(s.clone());
    }
}

fn extend_volume_opts(opts: &mut Vec<String>, v: &VolumeOptions) {
    if v.nocopy.unwrap_or(false) {
        opts.push("nocopy".into());
    }
}

impl Engine {
    pub(super) fn build_secret_binds(
        &self,
        service: &Service,
        file: &ComposeFile,
    ) -> Result<Vec<String>> {
        let mut binds = Vec::new();
        for secret_ref in &service.secrets {
            let (name, target_override) = match secret_ref {
                ServiceSecretRef::Short(s) => (s.clone(), None),
                ServiceSecretRef::Long { source, target, .. } => (source.clone(), target.clone()),
            };
            if let Some(config) = file.secrets.get(&name) {
                let target = target_override.unwrap_or_else(|| format!("/run/secrets/{name}"));
                match config {
                    SecretConfig { file: Some(host_path), .. } => {
                        binds.push(format!("{host_path}:{target}:ro"));
                    }
                    SecretConfig { content: Some(content), .. } => {
                        let path = self.materialize_inline("secrets", &name, content.as_bytes())?;
                        binds.push(format!("{}:{target}:ro", path.display()));
                    }
                    SecretConfig { environment: Some(env_var), .. } => {
                        let value = std::env::var(env_var).unwrap_or_default();
                        let path = self.materialize_inline("secrets", &name, value.as_bytes())?;
                        binds.push(format!("{}:{target}:ro", path.display()));
                    }
                    SecretConfig { external: Some(true), .. } => {
                        tracing::debug!("external secret {name} — relying on runtime injection");
                    }
                    _ => {}
                }
            }
        }
        Ok(binds)
    }

    pub(super) fn build_config_binds(
        &self,
        service: &Service,
        file: &ComposeFile,
    ) -> Result<Vec<String>> {
        let mut binds = Vec::new();
        for config_ref in &service.configs {
            let (name, target_override) = match config_ref {
                ServiceConfigRef::Short(s) => (s.clone(), None),
                ServiceConfigRef::Long { source, target, .. } => (source.clone(), target.clone()),
            };
            if let Some(cfg) = file.configs.get(&name) {
                let target = target_override.unwrap_or_else(|| format!("/{name}"));
                match cfg {
                    ConfigConfig { file: Some(host_path), .. } => {
                        binds.push(format!("{host_path}:{target}:ro"));
                    }
                    ConfigConfig { content: Some(content), .. } => {
                        let path = self.materialize_inline("configs", &name, content.as_bytes())?;
                        binds.push(format!("{}:{target}:ro", path.display()));
                    }
                    ConfigConfig { environment: Some(env_var), .. } => {
                        let value = std::env::var(env_var).unwrap_or_default();
                        let path = self.materialize_inline("configs", &name, value.as_bytes())?;
                        binds.push(format!("{}:{target}:ro", path.display()));
                    }
                    ConfigConfig { external: Some(true), .. } => {
                        tracing::debug!("external config {name} — relying on runtime injection");
                    }
                    _ => {}
                }
            }
        }
        Ok(binds)
    }

    /// Write `content` to a per-project temp file and return its path.
    ///
    /// Files live at `$TMPDIR/lynx-compose-<project>/<kind>/<name>` and are
    /// cleaned up by `Engine::cleanup_temp_dir` when the stack goes down.
    fn materialize_inline(
        &self,
        kind: &str,
        name: &str,
        content: &[u8],
    ) -> Result<std::path::PathBuf> {
        let dir = std::env::temp_dir()
            .join(format!("lynx-compose-{}", self.project))
            .join(kind);
        std::fs::create_dir_all(&dir).map_err(ComposeError::Io)?;
        let path = dir.join(name);
        std::fs::write(&path, content).map_err(ComposeError::Io)?;
        Ok(path)
    }

    /// Remove the per-project temp directory created by `materialize_inline`.
    pub(super) fn cleanup_temp_dir(&self) {
        let dir = std::env::temp_dir().join(format!("lynx-compose-{}", self.project));
        let _ = std::fs::remove_dir_all(dir);
    }
}
