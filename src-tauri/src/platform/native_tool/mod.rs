//! Desktop POSIX version probes only. No native backup/restore/DB command is
//! enabled: their complete process/socket upper bounds are not established.
pub(crate) mod deployment;
#[cfg(unix)]
mod process;
use datazen::{jobs::domain::native_tool::*, shared::error::PortError};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

/// Trusted deployment configuration, never deserialized from IPC/Job payloads.
/// SHA-256 pins the audited executable AND its declared version/probe contract.
pub struct NativeProbeInstallation {
    pub tool_id: String,
    pub version: String,
    pub executable: PathBuf,
    pub sha256: [u8; 32],
    pub expected_version_line: String,
}
struct Installation {
    config: NativeProbeInstallation,
}
pub struct DesktopNativeTools {
    installations: HashMap<String, Arc<Installation>>,
    cleanup: Arc<Mutex<HashMap<String, NativeCleanup>>>,
}
fn denied() -> PortError {
    PortError::BackendUnavailable("native tool unavailable or unverified".into())
}
impl DesktopNativeTools {
    /// Empty/default registry is deny-all; no PATH search or client executable.
    pub fn new(configs: Vec<NativeProbeInstallation>) -> Result<Self, PortError> {
        let mut installations = HashMap::new();
        for config in configs {
            if !matches!(
                config.tool_id.as_str(),
                "postgres.pg-dump-version" | "mysql.mysqldump-version"
            ) || config.version.is_empty()
                || config.version.len() > 128
                || config.expected_version_line.len() > 512
                || config.expected_version_line.contains(['\n', '\r'])
                || !config.expected_version_line.contains(&config.version)
            {
                return Err(denied());
            }
            let prefix = if config.tool_id.starts_with("postgres.") {
                "pg_dump (PostgreSQL) "
            } else {
                "mysqldump "
            };
            if !config.expected_version_line.starts_with(prefix) {
                return Err(denied());
            }
            Self::verify(&config)?;
            let key = format!("{}@{}", config.tool_id, config.version);
            if installations
                .insert(key, Arc::new(Installation { config }))
                .is_some()
            {
                return Err(denied());
            }
        }
        Ok(Self {
            installations,
            cleanup: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    fn verify(config: &NativeProbeInstallation) -> Result<(), PortError> {
        if !config.executable.is_absolute() {
            return Err(denied());
        }
        let canonical = std::fs::canonicalize(&config.executable).map_err(|_| denied())?;
        if canonical != config.executable {
            return Err(denied());
        }
        let metadata = std::fs::symlink_metadata(&canonical).map_err(|_| denied())?;
        if !metadata.is_file() || metadata.len() > 256 * 1024 * 1024 {
            return Err(denied());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Deployment binaries may not be group/world-writable.
            if metadata.permissions().mode() & 0o022 != 0
                || metadata.permissions().mode() & 0o111 == 0
            {
                return Err(denied());
            }
        }
        let file = std::fs::read(&canonical).map_err(|_| denied())?;
        if file.starts_with(b"#!") && !cfg!(test) {
            return Err(denied());
        }
        if <[u8; 32]>::from(Sha256::digest(file)) != config.sha256 {
            return Err(denied());
        }
        Ok(())
    }
}
#[async_trait::async_trait]
impl NativeToolPort for DesktopNativeTools {
    fn resolve(&self, selection: &NativeToolSelection) -> Result<NativeToolResolution, PortError> {
        #[cfg(not(unix))]
        {
            let _ = selection;
            return Err(denied());
        }
        #[cfg(unix)]
        {
            let key = format!("{}@{}", selection.tool_id, selection.version);
            let installation = self.installations.get(&key).ok_or_else(denied)?;
            Self::verify(&installation.config)?;
            Ok(NativeToolResolution {
                key,
                selection: selection.clone(),
                max_processes: Some(1),
                max_database_sockets: Some(0),
                connection_id: None,
            })
        }
    }
    async fn run(
        &self,
        run_key: &str,
        resolution: NativeToolResolution,
        cancel: Arc<AtomicBool>,
        limits: NativeToolLimits,
    ) -> Result<NativeToolReport, PortError> {
        #[cfg(not(unix))]
        {
            let _ = (run_key, resolution, cancel, limits);
            Err(denied())
        }
        #[cfg(unix)]
        {
            // Resolve again before spawn; callers cannot forge key/cost metadata.
            if self.resolve(&resolution.selection)? != resolution {
                return Err(denied());
            }
            let installation = self
                .installations
                .get(&resolution.key)
                .cloned()
                .ok_or_else(denied)?;
            let key = run_key.to_owned();
            let cleanup = self.cleanup.clone();
            let task = tokio::spawn(async move {
                process::run(key, installation, cleanup, cancel, limits).await
            });
            // A detached provider worker keeps group/pipes/tempdir on caller drop.
            match task.await {
                Ok(value) => value,
                Err(_) => Ok(NativeToolReport {
                    termination: NativeTermination::Failed,
                    exit_code: None,
                    stdout_bytes: 0,
                    stderr_bytes: 0,
                    cleanup: NativeCleanup::Unconfirmed,
                    version_verified: false,
                }),
            }
        }
    }
    async fn cleanup_status(&self, run_key: &str) -> NativeCleanup {
        self.cleanup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(run_key)
            .copied()
            .unwrap_or(NativeCleanup::Unconfirmed)
    }
}
#[cfg(all(test, unix))]
mod tests;
