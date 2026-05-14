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

pub(crate) fn build_binds(service: &Service, base_dir: &std::path::Path) -> Vec<String> {
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

                // Auto-create host directory for bind mounts when requested.
                if matches!(volume_type, VolumeType::Bind) {
                    if let Some(b) = bind {
                        if b.create_host_path.unwrap_or(false) && !src.is_empty() {
                            let abs = if std::path::Path::new(src).is_absolute() {
                                std::path::PathBuf::from(src)
                            } else {
                                base_dir.join(src)
                            };
                            if let Err(e) = std::fs::create_dir_all(&abs) {
                                tracing::warn!(
                                    "create_host_path: failed to create {}: {e}",
                                    abs.display()
                                );
                            }
                        }
                    }
                }

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

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::build_binds;
    use crate::compose::types::{BindOptions, Service, VolumeMount, VolumeOptions, VolumeType};
    use std::path::Path;

    fn svc_with_volumes(vols: Vec<VolumeMount>) -> Service {
        Service { volumes: vols, ..Default::default() }
    }

    #[test]
    fn short_form_passthrough() {
        let svc = svc_with_volumes(vec![VolumeMount::Short("./data:/app/data".into())]);
        let binds = build_binds(&svc, Path::new("/base"));
        assert_eq!(binds, vec!["./data:/app/data"]);
    }

    #[test]
    fn long_form_bind_read_only() {
        let svc = svc_with_volumes(vec![VolumeMount::Long {
            volume_type: VolumeType::Bind,
            source: Some("/host/path".into()),
            target: "/container/path".into(),
            read_only: Some(true),
            bind: None,
            volume: None,
            tmpfs: None,
            consistency: None,
        }]);
        let binds = build_binds(&svc, Path::new("/base"));
        assert_eq!(binds.len(), 1);
        assert!(binds[0].contains("ro"));
        assert!(binds[0].contains("/host/path:/container/path"));
    }

    #[test]
    fn long_form_bind_with_propagation() {
        let svc = svc_with_volumes(vec![VolumeMount::Long {
            volume_type: VolumeType::Bind,
            source: Some("/host".into()),
            target: "/cont".into(),
            read_only: Some(false),
            bind: Some(BindOptions {
                propagation: Some("rshared".into()),
                create_host_path: None,
                selinux: None,
            }),
            volume: None,
            tmpfs: None,
            consistency: None,
        }]);
        let binds = build_binds(&svc, Path::new("/base"));
        assert!(binds[0].contains("rshared"));
    }

    #[test]
    fn long_form_volume_nocopy() {
        let svc = svc_with_volumes(vec![VolumeMount::Long {
            volume_type: VolumeType::Volume,
            source: Some("myvolume".into()),
            target: "/data".into(),
            read_only: None,
            bind: None,
            volume: Some(VolumeOptions {
                nocopy: Some(true),
                ..Default::default()
            }),
            tmpfs: None,
            consistency: None,
        }]);
        let binds = build_binds(&svc, Path::new("/base"));
        assert!(binds[0].contains("nocopy"));
    }

    #[test]
    fn tmpfs_type_excluded_from_binds() {
        let svc = svc_with_volumes(vec![VolumeMount::Long {
            volume_type: VolumeType::Tmpfs,
            source: None,
            target: "/run".into(),
            read_only: None,
            bind: None,
            volume: None,
            tmpfs: None,
            consistency: None,
        }]);
        let binds = build_binds(&svc, Path::new("/base"));
        assert!(binds.is_empty());
    }
}
