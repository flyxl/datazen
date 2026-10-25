//! Install runtime wapps from a ZIP archive or a source directory.
//!
//! Flow (same shape as the theme installer): extract/copy into
//! `{wapps_dir}/.staging-{uuid}` → full validation → atomic rename onto
//! `{id}` (existing install is backed up as `{id}.old.bak` and removed once
//! the new package is in place).
//!
//! Every entry point takes `wapps_dir` as a parameter instead of reading an
//! ambient app-data root: the kernel is storage-root parameterized so a local
//! filesystem adapter and a remote adapter can drive it unchanged.

use std::fs::{self, File};
use std::io::{copy, Read};
use std::path::{Path, PathBuf};

use uuid::Uuid;
use zip::ZipArchive;

use crate::workspace_apps::manifest::{
    allowed_wapp_file_ext, parse_manifest, validate_manifest, validate_wapp_dir, WappManifest,
    MAX_WAPP_FILES, MAX_WAPP_UNCOMPRESSED,
};
use datazen_infra::path_safety::{validate_theme_zip_path, MAX_COMPRESSION_RATIO};

const STAGING_PREFIX: &str = ".staging-";
const BACKUP_SUFFIX: &str = ".old.bak";
const INSPECT_PREFIX: &str = ".datazen-inspect-";

/// Install a wapp ZIP into `{wapps_dir}/{manifest.id}/`.
pub fn install_from_zip(zip_path: &Path, wapps_dir: &Path) -> Result<WappManifest, String> {
    fs::create_dir_all(wapps_dir).map_err(|e| format!("create wapps dir: {e}"))?;

    let staging = staging_dir(wapps_dir);
    let result = (|| -> Result<WappManifest, String> {
        extract_wapp_zip(zip_path, &staging)?;
        finalize_staged_package(&staging, wapps_dir)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

/// Install a wapp from a plain directory into `{wapps_dir}/{manifest.id}/`.
/// Hidden files/dirs are skipped; everything else must pass the same rules a
/// ZIP would.
pub fn install_from_dir(src_dir: &Path, wapps_dir: &Path) -> Result<WappManifest, String> {
    if !src_dir.is_dir() {
        return Err(format!("source directory not found: {}", src_dir.display()));
    }
    fs::create_dir_all(wapps_dir).map_err(|e| format!("create wapps dir: {e}"))?;

    let staging = staging_dir(wapps_dir);
    let result = (|| -> Result<WappManifest, String> {
        copy_wapp_dir(src_dir, &staging)?;
        finalize_staged_package(&staging, wapps_dir)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

/// Validate a package fully **without installing it**: the ZIP/directory is
/// materialized into a throwaway temp directory, then the same rule set 1–7
/// as the real install runs against it. On success the manifest is returned
/// (name/version/permissions) so the UI can ask for confirmation; nothing is
/// ever written to `{wapps_dir}`.
pub fn inspect_wapp_package(package_path: &Path) -> Result<WappManifest, String> {
    if !package_path.exists() {
        return Err(format!(
            "wapp package not found: {}",
            package_path.display()
        ));
    }
    let is_zip = package_path.is_file()
        && package_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));

    let staging = std::env::temp_dir().join(format!("{INSPECT_PREFIX}{}", Uuid::new_v4()));
    let result = (|| -> Result<WappManifest, String> {
        if is_zip {
            extract_wapp_zip(package_path, &staging)?;
        } else {
            copy_wapp_dir(package_path, &staging)?;
        }

        let pack_root = resolve_pack_root(&staging)?;
        let content = fs::read_to_string(pack_root.join("manifest.json"))
            .map_err(|e| format!("read manifest.json: {e}"))?;
        let manifest = parse_manifest(&content)?;
        validate_manifest(&manifest, &pack_root)?;
        Ok(manifest)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

fn staging_dir(wapps_dir: &Path) -> PathBuf {
    wapps_dir.join(format!("{STAGING_PREFIX}{}", Uuid::new_v4()))
}

/// Resolve the package root inside staging, validate it fully, then swap it
/// into place under `{wapps_dir}/{id}`.
fn finalize_staged_package(staging_root: &Path, wapps_dir: &Path) -> Result<WappManifest, String> {
    let pack_root = resolve_pack_root(staging_root)?;

    // Full rule set (1–7) against the staged content.
    let manifest_path = pack_root.join("manifest.json");
    let content =
        fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest.json: {e}"))?;
    let manifest = crate::workspace_apps::manifest::parse_manifest(&content)?;
    crate::workspace_apps::manifest::validate_manifest(&manifest, &pack_root)?;

    let dest = wapps_dir.join(&manifest.id);
    atomic_replace_dir(&dest, &pack_root)?;

    // Belt and suspenders: the installed folder must revalidate cleanly.
    validate_wapp_dir(&dest)?;
    Ok(manifest)
}

#[derive(Clone, Copy, Debug)]
struct WappZipLimits {
    max_uncompressed_bytes: u64,
    max_compression_ratio: u64,
    max_entries: usize,
}

impl Default for WappZipLimits {
    fn default() -> Self {
        Self {
            max_uncompressed_bytes: MAX_WAPP_UNCOMPRESSED,
            max_compression_ratio: MAX_COMPRESSION_RATIO,
            max_entries: MAX_WAPP_FILES,
        }
    }
}

fn zip_bomb_err(message: impl Into<String>) -> String {
    message.into()
}

fn ratio_exceeds_limit(uncompressed: u64, compressed: u64, max_ratio: u64) -> bool {
    if compressed == 0 {
        return uncompressed > 0;
    }
    (uncompressed as u128) > (compressed as u128) * max_ratio as u128
}

fn check_compression_ratio(
    uncompressed: u64,
    compressed: u64,
    max_ratio: u64,
    context: &str,
) -> Result<(), String> {
    if ratio_exceeds_limit(uncompressed, compressed, max_ratio) {
        return Err(zip_bomb_err(format!(
            "zip bomb: compression ratio exceeded ({context})"
        )));
    }
    Ok(())
}

fn check_uncompressed_total(total: u64, max_bytes: u64) -> Result<(), String> {
    if total > max_bytes {
        return Err(zip_bomb_err(format!(
            "zip bomb: uncompressed size limit exceeded ({total} > {max_bytes})"
        )));
    }
    Ok(())
}

/// Reader that enforces per-entry and cumulative size caps while extracting.
struct LimitedZipReader<'a, R: Read> {
    inner: R,
    entry_remaining: u64,
    total_written: &'a mut u64,
    max_uncompressed_bytes: u64,
}

impl<R: Read> Read for LimitedZipReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n == 0 {
            return Ok(0);
        }

        let n = n as u64;
        *self.total_written = self.total_written.checked_add(n).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "zip bomb: overflow")
        })?;
        check_uncompressed_total(*self.total_written, self.max_uncompressed_bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        if n > self.entry_remaining {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "zip bomb: entry exceeded declared uncompressed size",
            ));
        }
        self.entry_remaining -= n;
        Ok(n as usize)
    }
}

/// Reject dot-prefixed path components so host-managed files (`.enabled`,
/// `.storage.json`, `.key`, …) can never arrive inside an installed package.
fn reject_hidden_components(entry_name: &str) -> Result<(), String> {
    for component in entry_name.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            continue;
        }
        if component.starts_with('.') {
            return Err(format!("hidden file not allowed in package: {entry_name}"));
        }
    }
    Ok(())
}

fn extract_wapp_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    extract_wapp_zip_with_limits(zip_path, dest, WappZipLimits::default())
}

fn extract_wapp_zip_with_limits(
    zip_path: &Path,
    dest: &Path,
    limits: WappZipLimits,
) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;

    let entry_count = archive.len();
    if entry_count > limits.max_entries {
        return Err(zip_bomb_err(format!(
            "zip bomb: too many entries ({entry_count} > {})",
            limits.max_entries
        )));
    }

    let mut total_uncompressed: u64 = 0;
    let mut total_compressed: u64 = 0;

    // First pass: name safety, quotas, declared-size sanity.
    for i in 0..entry_count {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let entry_name = entry.name().to_string();
        validate_theme_zip_path(&entry_name)?;
        reject_hidden_components(&entry_name)?;
        if entry.is_symlink() {
            return Err(format!("symlink zip entry not allowed: {entry_name}"));
        }

        let is_dir = entry_name.ends_with('/') || entry.is_dir();
        if !is_dir {
            reject_forbidden_extension(&entry_name)?;
            let uncompressed = entry.size();
            let compressed = entry.compressed_size();
            check_compression_ratio(
                uncompressed,
                compressed,
                limits.max_compression_ratio,
                &format!("entry `{entry_name}`"),
            )?;
            total_uncompressed = total_uncompressed
                .checked_add(uncompressed)
                .ok_or_else(|| zip_bomb_err("zip bomb: uncompressed size overflow"))?;
            total_compressed = total_compressed
                .checked_add(compressed)
                .ok_or_else(|| zip_bomb_err("zip bomb: compressed size overflow"))?;
            check_uncompressed_total(total_uncompressed, limits.max_uncompressed_bytes)?;
        }
    }

    if total_compressed > 0 {
        check_compression_ratio(
            total_uncompressed,
            total_compressed,
            limits.max_compression_ratio,
            "archive total",
        )?;
    }

    // Second pass: bounded extraction.
    let mut bytes_written: u64 = 0;
    for i in 0..entry_count {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let entry_name = entry.name().to_string();
        validate_theme_zip_path(&entry_name)?;
        let rel = datazen_infra::path_safety::validate_zip_entry_path(&entry_name)
            .map_err(|e| e.to_string())?;
        let out_path = dest.join(&rel);

        if entry_name.ends_with('/') {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            reject_forbidden_extension(&entry_name)?;
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out_file = File::create(&out_path).map_err(|e| e.to_string())?;
            let entry_size = entry.size();
            let mut limited = LimitedZipReader {
                inner: &mut entry,
                entry_remaining: entry_size,
                total_written: &mut bytes_written,
                max_uncompressed_bytes: limits.max_uncompressed_bytes,
            };
            copy(&mut limited, &mut out_file).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn reject_forbidden_extension(entry_name: &str) -> Result<(), String> {
    let ext = Path::new(entry_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    if ext.is_empty() {
        return Err(format!("file without extension: {entry_name}"));
    }
    if !allowed_wapp_file_ext(&ext) {
        return Err(format!("forbidden extension .{ext}: {entry_name}"));
    }
    Ok(())
}

/// Copy a source tree into staging with the same rules a ZIP must satisfy:
/// no symlinks, no hidden entries, whitelisted file extensions, size/count quotas.
fn copy_wapp_dir(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut stats = CopyStats::default();
    copy_dir_recursive(src, dest, src, &mut stats)
}

#[derive(Default)]
struct CopyStats {
    files: usize,
    total_bytes: u64,
}

fn copy_dir_recursive(
    src: &Path,
    dest_root: &Path,
    root: &Path,
    stats: &mut CopyStats,
) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| format!("read dir {}: {e}", src.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            return Err(format!("unreadable file name in {}", src.display()));
        };
        if name.starts_with('.') {
            continue;
        }

        let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            let rel = rel_lossy(&path, root);
            return Err(format!("symlink not allowed: {rel}"));
        }

        let rel = rel_lossy(&path, root);
        let target = dest_root.join(&rel);

        if path.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            copy_dir_recursive(&path, dest_root, root, stats)?;
            continue;
        }

        reject_forbidden_extension(&rel)?;

        stats.files += 1;
        if stats.files > MAX_WAPP_FILES {
            return Err(format!("too many files (max {MAX_WAPP_FILES})"));
        }

        stats.total_bytes = stats
            .total_bytes
            .checked_add(meta.len())
            .ok_or_else(|| "package size overflow".to_string())?;
        if stats.total_bytes > MAX_WAPP_UNCOMPRESSED {
            return Err(format!(
                "package size exceeds limit ({MAX_WAPP_UNCOMPRESSED} bytes)"
            ));
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&path, &target).map_err(|e| format!("copy {rel}: {e}"))?;
    }

    Ok(())
}

fn rel_lossy(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// If `dir` has no manifest at root but exactly one subdirectory (and no root
/// files), use that subdirectory — supports zips packaged with a top folder.
fn resolve_pack_root(dir: &Path) -> Result<PathBuf, String> {
    if dir.join("manifest.json").is_file() {
        return Ok(dir.to_path_buf());
    }

    let mut subdirs = Vec::new();
    let mut root_files = 0usize;

    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let is_dot = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        if is_dot {
            continue;
        }
        if path.is_dir() {
            subdirs.push(path);
        } else if path.is_file() {
            root_files += 1;
        }
    }

    if subdirs.len() == 1 && root_files == 0 {
        return Ok(subdirs.into_iter().next().unwrap());
    }

    Err("missing manifest.json".into())
}

/// Atomically replace `dest` with `staging`. An existing install is moved to
/// `{dest}.old.bak` first; the backup is removed on success and restored on
/// failure.
fn atomic_replace_dir(dest: &Path, staging: &Path) -> Result<(), String> {
    let parent = dest
        .parent()
        .ok_or_else(|| "destination has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let name = dest
        .file_name()
        .ok_or_else(|| "destination has no directory name".to_string())?
        .to_string_lossy()
        .to_string();
    let backup = parent.join(format!("{name}{BACKUP_SUFFIX}"));

    if backup.exists() {
        if backup.is_dir() {
            fs::remove_dir_all(&backup).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
    }

    if dest.exists() {
        fs::rename(dest, &backup).map_err(|e| format!("backup existing wapp: {e}"))?;
        match fs::rename(staging, dest) {
            Ok(()) => {
                let _ = fs::remove_dir_all(&backup);
                Ok(())
            }
            Err(e) => {
                let _ = fs::rename(&backup, dest);
                Err(e.to_string())
            }
        }
    } else {
        fs::rename(staging, dest).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests;
