//! Cross-process leases and safe reclamation for comparison-store directories.

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

use super::create_new_file;

pub(super) const STORE_DIRECTORY_PREFIX: &str = "comparison-";
pub(super) const STORE_OWNER_FILE: &str = "store-owner.json";
pub(super) const STORE_OWNER_TEMP_FILE: &str = ".store-owner.tmp";
pub(super) const OWNER_DIRECTORY: &str = ".owners";
pub(super) const STORE_OWNER_FORMAT_VERSION: u32 = 1;
pub(super) const MAX_STORE_OWNER_BYTES: u64 = 256;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StoreOwnerMarker {
    pub(super) format_version: u32,
    pub(super) owner_id: Uuid,
    pub(super) store_id: Uuid,
}

/// A SQLite exclusive transaction is an OS-managed cross-process lease. The
/// connection stays open for the lifetime of all stores created by this app
/// process. If the process exits unexpectedly SQLite releases the lock, which
/// lets a later process prove the owner is gone before reclaiming its stores.
pub(super) struct OwnerLease {
    pub(super) id: Uuid,
    _connection: Mutex<Connection>,
}

impl std::fmt::Debug for OwnerLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnerLease")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

static PROCESS_OWNER_LEASE: OnceLock<Mutex<Option<Arc<OwnerLease>>>> = OnceLock::new();

pub(super) fn comparison_store_directory() -> PathBuf {
    std::env::temp_dir().join("datazen-sync-comparisons")
}

pub(super) fn ensure_private_store_root(path: &Path) -> io::Result<PathBuf> {
    ensure_private_directory(path)?;
    let canonical = fs::canonicalize(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "store root has no parent"))?;
    let canonical_parent = fs::canonicalize(parent)?;
    if canonical.parent() != Some(canonical_parent.as_path()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Data Sync comparison store root escaped its parent",
        ));
    }
    Ok(canonical)
}

pub(super) fn ensure_private_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Data Sync comparison storage path is not a real directory",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(false);
            #[cfg(unix)]
            builder.mode(0o700);
            match builder.create(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.file_type().is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Data Sync comparison storage path is not a real directory",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(super) fn current_process_owner(root: &Path) -> Result<Arc<OwnerLease>, String> {
    let lease = PROCESS_OWNER_LEASE.get_or_init(|| Mutex::new(None));
    let mut owner = lease
        .lock()
        .map_err(|_| "Data Sync comparison owner registry is unavailable".to_string())?;
    if let Some(owner) = owner.as_ref() {
        return Ok(owner.clone());
    }
    let new_owner = acquire_owner_lease(root)?;
    *owner = Some(new_owner.clone());
    Ok(new_owner)
}

pub(super) fn acquire_owner_lease(root: &Path) -> Result<Arc<OwnerLease>, String> {
    let owner_directory = root.join(OWNER_DIRECTORY);
    ensure_private_directory(&owner_directory)
        .map_err(|error| format!("cannot prepare Data Sync comparison owner directory: {error}"))?;
    let owner_id = Uuid::new_v4();
    let lock_path = owner_directory.join(format!("{owner_id}.sqlite"));
    let lock_file = create_new_file(&lock_path)
        .map_err(|error| format!("cannot create Data Sync comparison owner lease: {error}"))?;
    drop(lock_file);
    let connection = Connection::open_with_flags(
        &lock_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("cannot open Data Sync comparison owner lease: {error}"))?;
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(|error| format!("cannot configure Data Sync comparison owner lease: {error}"))?;
    connection
        .pragma_update(None, "journal_mode", "DELETE")
        .map_err(|error| format!("cannot configure Data Sync comparison owner lease: {error}"))?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS owner_lease (id INTEGER PRIMARY KEY CHECK (id = 1)); BEGIN EXCLUSIVE;",
        )
        .map_err(|error| format!("cannot lock Data Sync comparison owner lease: {error}"))?;
    #[cfg(unix)]
    fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot secure Data Sync comparison owner lease: {error}"))?;
    Ok(Arc::new(OwnerLease {
        id: owner_id,
        _connection: Mutex::new(connection),
    }))
}

pub(super) fn create_store_directory(root: &Path, owner: &OwnerLease) -> io::Result<PathBuf> {
    for _ in 0..8 {
        let store_id = Uuid::new_v4();
        let directory = root.join(format!("{STORE_DIRECTORY_PREFIX}{}-{store_id}", owner.id));
        let mut builder = fs::DirBuilder::new();
        builder.recursive(false);
        #[cfg(unix)]
        builder.mode(0o700);
        match builder.create(&directory) {
            Ok(()) => {
                let marker = StoreOwnerMarker {
                    format_version: STORE_OWNER_FORMAT_VERSION,
                    owner_id: owner.id,
                    store_id,
                };
                let marker_result = (|| {
                    let bytes = serde_json::to_vec(&marker).map_err(io::Error::other)?;
                    let temporary = directory.join(STORE_OWNER_TEMP_FILE);
                    let mut file = create_new_file(&temporary)?;
                    file.write_all(&bytes)?;
                    file.flush()?;
                    file.sync_all()?;
                    drop(file);
                    fs::rename(temporary, directory.join(STORE_OWNER_FILE))
                })();
                if let Err(error) = marker_result {
                    remove_owned_store_directory(root, &directory, owner.id);
                    return Err(error);
                }
                return Ok(directory);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique Data Sync comparison store directory",
    ))
}

pub(super) fn parse_store_directory_name(name: &str) -> Option<(Uuid, Uuid)> {
    let rest = name.strip_prefix(STORE_DIRECTORY_PREFIX)?;
    if rest.len() != 73 || rest.as_bytes().get(36) != Some(&b'-') {
        return None;
    }
    let owner_id = Uuid::parse_str(&rest[..36]).ok()?;
    let store_id = Uuid::parse_str(&rest[37..]).ok()?;
    if format!("{owner_id}-{store_id}") != rest {
        return None;
    }
    Some((owner_id, store_id))
}

pub(super) fn has_valid_owner_marker(directory: &Path, owner_id: Uuid, store_id: Uuid) -> bool {
    let path = directory.join(STORE_OWNER_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        _ => return false,
    };
    if metadata.len() > MAX_STORE_OWNER_BYTES {
        return false;
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    if file
        .take(MAX_STORE_OWNER_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_STORE_OWNER_BYTES
    {
        return false;
    }
    matches!(
        serde_json::from_slice::<StoreOwnerMarker>(&bytes),
        Ok(marker)
            if marker.format_version == STORE_OWNER_FORMAT_VERSION
                && marker.owner_id == owner_id
                && marker.store_id == store_id
    )
}

pub(super) fn try_lock_existing_owner(
    root: &Path,
    owner_id: Uuid,
) -> Result<Option<Connection>, String> {
    let path = root
        .join(OWNER_DIRECTORY)
        .join(format!("{owner_id}.sqlite"));
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "cannot inspect Data Sync comparison owner lease: {error}"
            ));
        }
    };
    if metadata.len() == 0 {
        return Ok(None);
    }
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("cannot open Data Sync comparison owner lease: {error}"))?;
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(|error| format!("cannot configure Data Sync comparison owner lease: {error}"))?;
    match connection.execute_batch("BEGIN EXCLUSIVE;") {
        Ok(()) => Ok(Some(connection)),
        Err(rusqlite::Error::SqliteFailure(error, _))
            if matches!(
                error.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(format!(
            "cannot prove Data Sync comparison owner is stale: {error}"
        )),
    }
}

pub(super) fn scavenge_stale_stores(root: &Path) {
    scavenge_stale_stores_with(root, remove_stale_store_directory);
}

pub(super) fn scavenge_stale_stores_with<F>(root: &Path, mut remove: F)
where
    F: FnMut(&Path, &Path, Uuid, Uuid) -> io::Result<()>,
{
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(path = %root.display(), error = %error, "cannot scan Data Sync comparison stores");
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(path = %root.display(), error = %error, "cannot inspect Data Sync comparison store entry");
                continue;
            }
        };
        match entry.file_type() {
            Ok(entry_type) if entry_type.is_dir() => {}
            Ok(_) => continue,
            Err(error) => {
                tracing::warn!(path = %entry.path().display(), error = %error, "cannot inspect Data Sync comparison store entry type");
                continue;
            }
        }
        let name = match entry.file_name().to_str() {
            Some(name) => name.to_string(),
            None => continue,
        };
        let Some((owner_id, store_id)) = parse_store_directory_name(&name) else {
            continue;
        };
        let directory = entry.path();
        if !has_valid_owner_marker(&directory, owner_id, store_id) {
            continue;
        }
        let owner_connection = match try_lock_existing_owner(root, owner_id) {
            Ok(Some(connection)) => connection,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(path = %directory.display(), error = %error, "cannot prove Data Sync comparison owner is stale");
                continue;
            }
        };
        if let Err(error) = remove(root, &directory, owner_id, store_id) {
            if error.kind() != io::ErrorKind::NotFound {
                tracing::warn!(path = %directory.display(), error = %error, "failed to reclaim stale Data Sync comparison store");
            }
        }
        drop(owner_connection);
    }
    reap_unreferenced_owner_leases(root);
}

pub(super) fn reap_unreferenced_owner_leases(root: &Path) {
    let mut referenced_owners = HashSet::new();
    let stores = match fs::read_dir(root) {
        Ok(stores) => stores,
        Err(_) => return,
    };
    for entry in stores.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if let Some((owner, _)) = parse_store_directory_name(name) {
                referenced_owners.insert(owner);
            }
        }
    }

    let owner_directory = root.join(OWNER_DIRECTORY);
    let owners = match fs::read_dir(&owner_directory) {
        Ok(owners) => owners,
        Err(_) => return,
    };
    for entry in owners.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(owner_text) = name.strip_suffix(".sqlite") else {
            continue;
        };
        let Ok(owner_id) = Uuid::parse_str(owner_text) else {
            continue;
        };
        if format!("{owner_id}.sqlite") != name || referenced_owners.contains(&owner_id) {
            continue;
        }
        let connection = match try_lock_existing_owner(root, owner_id) {
            Ok(Some(connection)) => connection,
            Ok(None) | Err(_) => continue,
        };
        drop(connection);
        if let Err(error) = fs::remove_file(&path) {
            if error.kind() != io::ErrorKind::NotFound {
                tracing::debug!(path = %path.display(), error = %error, "could not remove stale Data Sync comparison owner lease");
            }
        }
    }
}

pub(super) fn remove_stale_store_directory(
    root: &Path,
    directory: &Path,
    owner_id: Uuid,
    store_id: Uuid,
) -> io::Result<()> {
    let actual_owner = validate_store_path(root, directory)?;
    if actual_owner != owner_id {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Data Sync comparison owner does not match the store path",
        ));
    }
    if !has_valid_owner_marker(directory, owner_id, store_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Data Sync comparison store owner marker is invalid",
        ));
    }
    fs::remove_dir_all(directory)
}

pub(super) fn remove_owned_store_directory(root: &Path, directory: &Path, owner_id: Uuid) {
    match validate_store_path(root, directory) {
        Ok(actual_owner) if actual_owner == owner_id => {
            if let Err(error) = fs::remove_dir_all(directory) {
                if error.kind() != io::ErrorKind::NotFound {
                    tracing::warn!(path = %directory.display(), error = %error, "failed to remove Data Sync comparison store");
                }
            }
        }
        Ok(_) => {
            tracing::warn!(path = %directory.display(), "refusing to remove Data Sync comparison store owned by another process")
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(path = %directory.display(), error = %error, "refusing to remove Data Sync comparison store outside its root")
        }
    }
}

pub(super) fn validate_store_path(root: &Path, directory: &Path) -> io::Result<Uuid> {
    let root_metadata = fs::symlink_metadata(root)?;
    if !root_metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Data Sync comparison store root is not a real directory",
        ));
    }
    let root = fs::canonicalize(root)?;
    let directory_metadata = fs::symlink_metadata(directory)?;
    if !directory_metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Data Sync comparison store is not a real directory",
        ));
    }
    let parent = directory
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "store has no parent"))?;
    if fs::canonicalize(parent)? != root {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Data Sync comparison store escaped its root",
        ));
    }
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid store name"))?;
    parse_store_directory_name(name)
        .map(|(owner_id, _)| owner_id)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unrecognized store name"))
}
