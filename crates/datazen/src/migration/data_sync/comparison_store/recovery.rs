//! Cross-process leases and safe reclamation for comparison-store directories.

use fd_lock::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
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

/// A kernel-managed cross-process lease over the owner's lock file
/// (host-boundary follow-up F1: replaces the SQLite exclusive transaction).
///
/// The lock is `flock` on Unix / `LockFileEx` on Windows, held for the
/// lifetime of all stores created by this app process: the file stays open,
/// and if the process exits unexpectedly the kernel releases the lock when
/// the descriptor closes. A later process proves an owner is gone by opening
/// the same lock file and failing to acquire the lock (`WouldBlock` = owner
/// alive), which is what lets it reclaim that owner's stores safely. The
/// lock file's *content* is never read — only the kernel lock speaks.
pub(super) struct OwnerLease {
    pub(super) id: Uuid,
    _lock: Mutex<RwLock<File>>,
}

impl std::fmt::Debug for OwnerLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnerLease")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// 每个存储根一份 owner 租约。
///
/// 租约是**按根**缓存的：租约文件本身住在根目录下的 `.owners/`，用另一个根的
/// 租约去标记当前根的目录，会让陈旧目录永远无法被证明为可回收。
static PROCESS_OWNER_LEASES: OnceLock<Mutex<HashMap<PathBuf, Arc<OwnerLease>>>> = OnceLock::new();

/// 把一个已打开的锁文件变成「持有型」租约：`try_write` 成功后故意泄漏
/// guard（guard 的 Drop 会立即解锁），返回持有 `File` 的 `RwLock`。
/// 之后释放锁的唯一途径就是关闭文件——`RwLock` 被 drop 时 `File` 随之关闭，
/// 内核放锁。这与进程崩溃时内核自动放锁是同一条生命周期，也是租约语义
/// （「活到进程/句柄死为止」而不是「活到某个作用域为止」）的实现方式。
fn lock_owner_file(file: File) -> Result<Option<RwLock<File>>, String> {
    let mut lock = RwLock::new(file);
    // 借用注意：`try_write` 的返回值（含 guard）借用 `lock`，match 臂内不能
    // 移动它——先 break 出循环，借用结束后再构造返回值。
    let acquired = loop {
        match lock.try_write() {
            Ok(guard) => {
                std::mem::forget(guard);
                break true;
            }
            // 锁被别人持有：owner 还活着，证明失败。
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break false,
            // flock(LOCK_NB) 可能被信号打断（EINTR），重试即可。
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(format!(
                    "cannot lock Data Sync comparison owner lease: {error}"
                ));
            }
        }
    };
    Ok(if acquired { Some(lock) } else { None })
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
    let leases = PROCESS_OWNER_LEASES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut leases = leases
        .lock()
        .map_err(|_| "Data Sync comparison owner registry is unavailable".to_string())?;
    if let Some(owner) = leases.get(root) {
        return Ok(owner.clone());
    }
    let new_owner = acquire_owner_lease(root)?;
    leases.insert(root.to_path_buf(), new_owner.clone());
    Ok(new_owner)
}

pub(super) fn acquire_owner_lease(root: &Path) -> Result<Arc<OwnerLease>, String> {
    let owner_directory = root.join(OWNER_DIRECTORY);
    ensure_private_directory(&owner_directory)
        .map_err(|error| format!("cannot prepare Data Sync comparison owner directory: {error}"))?;
    let owner_id = Uuid::new_v4();
    let lock_path = owner_directory.join(format!("{owner_id}.lock"));
    let lock_file = create_new_file(&lock_path)
        .map_err(|error| format!("cannot create Data Sync comparison owner lease: {error}"))?;
    // A freshly created, uniquely named lock file cannot have a contender, so
    // `WouldBlock` is impossible by construction — any failure here is fatal.
    match lock_owner_file(lock_file) {
        Ok(Some(lock)) => Ok(Arc::new(OwnerLease {
            id: owner_id,
            _lock: Mutex::new(lock),
        })),
        Ok(None) => Err(
            "cannot lock Data Sync comparison owner lease: fresh lease is contended".to_string(),
        ),
        Err(error) => Err(error),
    }
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

/// 证明一个 owner 已经死亡：打开它的锁文件并尝试加锁。拿到锁 → owner
/// 已死（或从未活过），返回的凭据持有锁直到 drop；`WouldBlock` → owner
/// 还活着，返回 `None`。锁文件**内容**不参与证明——空的或乱写的锁文件
/// 只说明 owner 没有洁癖，不说明它还活着。
///
/// 锁文件缺失（或不是普通文件，比如软链）时同样返回 `None`：证明不了
/// 就不回收（fail closed）。
pub(super) struct OwnerLeaseProof {
    _lock: RwLock<File>,
}

pub(super) fn try_lock_existing_owner(
    root: &Path,
    owner_id: Uuid,
) -> Result<Option<OwnerLeaseProof>, String> {
    let path = root.join(OWNER_DIRECTORY).join(format!("{owner_id}.lock"));
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => {}
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "cannot inspect Data Sync comparison owner lease: {error}"
            ));
        }
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("cannot open Data Sync comparison owner lease: {error}"))?;
    match lock_owner_file(file)? {
        Some(lock) => Ok(Some(OwnerLeaseProof { _lock: lock })),
        None => Ok(None),
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
        let owner_proof = match try_lock_existing_owner(root, owner_id) {
            Ok(Some(proof)) => proof,
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
        drop(owner_proof);
    }
    reap_unreferenced_owner_leases(root);
}

/// 回收无引用的 owner 租约文件。
///
/// 只认 F1 之后的 `.lock` 文件；旧版本留下的 `.sqlite` 租约**不读也不删**
/// ——新代码无法证明其持有者死活，不动它是最安全的。其代价是升级前
/// 崩溃残留的 comparison 目录不再被新版本回收（一次性遗留，量级为
/// 崩溃当时的溢出结果），删除请手动清空存储根。
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
        let Some(owner_text) = name.strip_suffix(".lock") else {
            continue;
        };
        let Ok(owner_id) = Uuid::parse_str(owner_text) else {
            continue;
        };
        if format!("{owner_id}.lock") != name || referenced_owners.contains(&owner_id) {
            continue;
        }
        let proof = match try_lock_existing_owner(root, owner_id) {
            Ok(Some(proof)) => proof,
            Ok(None) | Err(_) => continue,
        };
        drop(proof);
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
