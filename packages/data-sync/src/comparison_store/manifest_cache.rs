//! Reuse full validation only while every store file's identity is unchanged.
//! Page reads still reject edits, replacement, truncation and malformed rows;
//! they do not repeatedly deserialize all unmodified row payloads.

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use super::{disk::read_manifest, DiskManifest};

#[derive(Debug, PartialEq, Eq)]
struct FileStamp {
    path: PathBuf,
    length: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

fn stamps(directory: &Path) -> Result<Vec<FileStamp>, String> {
    let entries = fs::read_dir(directory)
        .map_err(|_| "cannot inspect Data Sync comparison store".to_string())?;
    let mut stamps = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| "cannot inspect Data Sync comparison file".to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| "cannot inspect Data Sync comparison file".to_string())?;
        if !metadata.is_file() {
            return Err("Data Sync comparison store contains a non-regular file".into());
        }
        stamps.push(FileStamp {
            path,
            length: metadata.len(),
            modified: metadata
                .modified()
                .map_err(|_| "cannot confirm Data Sync comparison file revision".to_string())?,
            #[cfg(unix)]
            identity: (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        });
    }
    stamps.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(stamps)
}

#[derive(Debug)]
struct ValidatedManifest {
    stamps: Vec<FileStamp>,
    manifest: DiskManifest,
}

#[derive(Debug, Default)]
pub(super) struct ManifestCache {
    validated: Mutex<Option<ValidatedManifest>>,
    #[cfg(test)]
    pub(super) validations: AtomicUsize,
}

impl ManifestCache {
    pub(super) fn read(&self, path: &Path, directory: &Path) -> Result<DiskManifest, String> {
        let mut cached = self
            .validated
            .lock()
            .map_err(|_| "Data Sync comparison validation is unavailable".to_string())?;
        let before = stamps(directory)?;
        if let Some(validated) = cached.as_ref() {
            if validated.stamps == before {
                return Ok(validated.manifest.clone());
            }
        }
        *cached = None;
        #[cfg(test)]
        self.validations.fetch_add(1, Ordering::SeqCst);
        let manifest = read_manifest(path, directory)?;
        let after = stamps(directory)?;
        if before != after {
            return Err(
                "Data Sync comparison store changed during validation; compare again".into(),
            );
        }
        *cached = Some(ValidatedManifest {
            stamps: after,
            manifest: manifest.clone(),
        });
        Ok(manifest)
    }
}
