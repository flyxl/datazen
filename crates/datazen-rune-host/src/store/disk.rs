//! The sidecar lock spans read, modify and atomic replacement, including password migration.
use super::{ConnectionRepositoryError, FileConnectionRepository, StoreFile};
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::path::Path;
use tempfile::NamedTempFile;

fn io_error(error: std::io::Error) -> ConnectionRepositoryError {
    ConnectionRepositoryError::BackendUnavailable(format!("connection store I/O: {}", error.kind()))
}

impl StoreFile {
    pub(super) fn read(path: &Path) -> Result<Self, ConnectionRepositoryError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(Self {
                    groups: Some(Vec::new()),
                    ..Self::default()
                })
            }
            Err(error) => return Err(io_error(error)),
        };
        serde_json::from_slice(&bytes).map_err(|error| {
            ConnectionRepositoryError::BackendUnavailable(format!(
                "parse the connection store at line {}, column {}",
                error.line(),
                error.column()
            ))
        })
    }
}

impl FileConnectionRepository {
    pub(super) fn transaction<T>(
        &self,
        change: impl FnOnce(&mut StoreFile) -> Result<(T, bool), ConnectionRepositoryError>,
    ) -> Result<T, ConnectionRepositoryError> {
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(io_error)?;
        // Lock a stable inode. The JSON file itself is replaced on every write.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.path.with_added_extension("lock"))
            .map_err(io_error)?;
        lock.lock().map_err(io_error)?;
        let mut store = StoreFile::read(&self.path)?;
        let (value, changed) = change(&mut store)?;
        if changed {
            let mut temp = NamedTempFile::new_in(parent).map_err(io_error)?;
            serde_json::to_writer_pretty(temp.as_file_mut(), &store).map_err(|_| {
                ConnectionRepositoryError::BackendUnavailable(
                    "could not write the connection store".into(),
                )
            })?;
            temp.as_file().sync_all().map_err(io_error)?;
            temp.persist(&self.path)
                .map_err(|error| io_error(error.error))?;
        }
        Ok(value)
    }
}
