//! Pure helpers for exporting/importing the application data directory as ZIP.

use std::fs::{self, File};
use std::io::{copy, Read, Write};
use std::path::{Component, Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// True when any path component is the encryption key file `.key`.
fn has_key_component(rel_path: &Path) -> bool {
    rel_path
        .components()
        .any(|c| matches!(c, Component::Normal(name) if name.to_string_lossy() == ".key"))
}

/// Shared exclusions for logs, staging dirs, and temp files.
fn should_exclude_common(rel_path: &Path) -> bool {
    let mut first = true;
    for component in rel_path.components() {
        if let Component::Normal(name) = component {
            let s = name.to_string_lossy();
            if first && s == "logs" {
                return true;
            }
            if s == ".import_staging" || s == ".tmp" || s.ends_with(".tmp") {
                return true;
            }
        }
        first = false;
    }
    false
}

/// Options controlling app-data ZIP export.
#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    pub include_dashboard_runs: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_dashboard_runs: true,
        }
    }
}

fn is_dashboard_runs_path(rel_path: &Path) -> bool {
    rel_path
        .components()
        .next()
        .is_some_and(|c| matches!(c, Component::Normal(name) if name == "dashboard-runs"))
}

/// Returns true if `rel_path` (relative to the data dir root) should be skipped on export.
/// Like [`should_exclude_with_options`] with default export options.
/// Used by unit tests and as a convenience wrapper around the options API.
#[cfg_attr(not(test), allow(dead_code))]
pub fn should_exclude(rel_path: &Path) -> bool {
    should_exclude_with_options(rel_path, ExportOptions::default())
}

/// Like [`should_exclude`] but respects export options (e.g. skip `dashboard-runs/`).
pub fn should_exclude_with_options(rel_path: &Path, options: ExportOptions) -> bool {
    if should_exclude_common(rel_path) || has_key_component(rel_path) {
        return true;
    }
    if !options.include_dashboard_runs && is_dashboard_runs_path(rel_path) {
        return true;
    }
    false
}

/// Import filter: same as export except `.key` may be restored from legacy backups.
fn should_exclude_on_import(rel_path: &Path) -> bool {
    should_exclude_common(rel_path)
}

/// Recursively zip `data_dir` into `zip_path`, preserving relative paths.
/// Like [`export_app_data_with_options`] with default export options.
#[cfg_attr(not(test), allow(dead_code))]
pub fn export_app_data(data_dir: &Path, zip_path: &Path) -> std::io::Result<()> {
    export_app_data_with_options(data_dir, zip_path, ExportOptions::default())
}

/// Like [`export_app_data`] with explicit export options.
pub fn export_app_data_with_options(
    data_dir: &Path,
    zip_path: &Path,
    options: ExportOptions,
) -> std::io::Result<()> {
    let file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let zip_options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    add_dir_to_zip(&mut zip, data_dir, data_dir, zip_options, options)?;
    zip.finish()?;
    Ok(())
}

fn add_dir_to_zip<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    root: &Path,
    dir: &Path,
    options: SimpleFileOptions,
    export_options: ExportOptions,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        if should_exclude_with_options(rel, export_options) {
            continue;
        }
        if path.is_dir() {
            add_dir_to_zip(zip, root, &path, options, export_options)?;
        } else {
            let name = rel.to_string_lossy().replace('\\', "/");
            zip.start_file(name, options)?;
            let mut f = File::open(&path)?;
            copy(&mut f, zip)?;
        }
    }
    Ok(())
}

/// Validate a zip entry name; reject absolute paths and `..` components.
pub fn validate_zip_entry_path(name: &str) -> std::io::Result<PathBuf> {
    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') || normalized.starts_with("//") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "absolute path in zip entry",
        ));
    }
    // Windows absolute paths like C:/...
    if normalized.len() >= 2 {
        let bytes = normalized.as_bytes();
        if bytes[1] == b':' {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "absolute path in zip entry",
            ));
        }
    }

    let path = PathBuf::from(&normalized);
    for component in path.components() {
        match component {
            Component::ParentDir => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "path traversal in zip entry",
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "absolute path in zip entry",
                ));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(path)
}

/// Maximum total uncompressed bytes allowed when extracting app-data ZIP archives.
pub const MAX_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

/// Reject when `uncompressed / compressed` exceeds this value (per entry or globally).
pub const MAX_COMPRESSION_RATIO: u64 = 100;

/// Maximum number of entries in an app-data ZIP archive.
pub const MAX_ZIP_ENTRIES: usize = 100_000;

#[derive(Clone, Copy, Debug)]
struct ZipExtractLimits {
    max_uncompressed_bytes: u64,
    max_compression_ratio: u64,
    max_entries: usize,
}

impl Default for ZipExtractLimits {
    fn default() -> Self {
        Self {
            max_uncompressed_bytes: MAX_UNCOMPRESSED_BYTES,
            max_compression_ratio: MAX_COMPRESSION_RATIO,
            max_entries: MAX_ZIP_ENTRIES,
        }
    }
}

fn zip_bomb_err(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
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
) -> std::io::Result<()> {
    if ratio_exceeds_limit(uncompressed, compressed, max_ratio) {
        return Err(zip_bomb_err(format!(
            "zip bomb: compression ratio exceeded ({context})"
        )));
    }
    Ok(())
}

fn check_uncompressed_total(total: u64, max_bytes: u64) -> std::io::Result<()> {
    if total > max_bytes {
        return Err(zip_bomb_err(format!(
            "zip bomb: uncompressed size limit exceeded ({total} > {max_bytes})"
        )));
    }
    Ok(())
}

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
        *self.total_written = self
            .total_written
            .checked_add(n)
            .ok_or_else(|| zip_bomb_err("zip bomb: uncompressed size limit exceeded (overflow)"))?;
        check_uncompressed_total(*self.total_written, self.max_uncompressed_bytes)?;

        if n > self.entry_remaining {
            return Err(zip_bomb_err(format!(
                "zip bomb: entry exceeded declared uncompressed size ({n} bytes over limit)"
            )));
        }
        self.entry_remaining -= n;
        Ok(n as usize)
    }
}

fn extract_zip_to_dir(zip_path: &Path, dest: &Path) -> std::io::Result<()> {
    extract_zip_to_dir_with_limits(zip_path, dest, ZipExtractLimits::default())
}

fn extract_zip_to_dir_with_limits(
    zip_path: &Path,
    dest: &Path,
    limits: ZipExtractLimits,
) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    let entry_count = archive.len();
    if entry_count > limits.max_entries {
        return Err(zip_bomb_err(format!(
            "zip bomb: too many entries ({entry_count} > {})",
            limits.max_entries
        )));
    }

    let mut total_uncompressed: u64 = 0;
    let mut total_compressed: u64 = 0;

    for i in 0..entry_count {
        let entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();
        let is_dir = entry_name.ends_with('/') || entry.is_dir();

        if !is_dir {
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
                .ok_or_else(|| {
                    zip_bomb_err("zip bomb: uncompressed size limit exceeded (overflow)")
                })?;
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

    let mut bytes_written: u64 = 0;

    for i in 0..entry_count {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();
        let rel = validate_zip_entry_path(&entry_name)?;
        let out_path = dest.join(&rel);

        if entry_name.ends_with('/') {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)?;
            let entry_size = entry.size();
            let mut limited = LimitedZipReader {
                inner: &mut entry,
                entry_remaining: entry_size,
                total_written: &mut bytes_written,
                max_uncompressed_bytes: limits.max_uncompressed_bytes,
            };
            copy(&mut limited, &mut out_file)?;
        }
    }

    Ok(())
}

/// Replace `data_dir` contents from `zip_path`. Existing `logs/` and `.key` (when absent
/// from the archive) are preserved when present locally.
///
/// Uses extract → validate in staging → build sibling `prepared` dir → atomic rename swap
/// so a failed import never leaves `data_dir` empty or partially written.
pub fn import_app_data(data_dir: &Path, zip_path: &Path) -> std::io::Result<()> {
    let parent = data_dir.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "data_dir has no parent directory",
        )
    })?;
    let dir_name = data_dir.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "data_dir has no directory name",
        )
    })?;

    let staging = std::env::temp_dir().join(format!("datazen-import-{}", uuid::Uuid::new_v4()));
    let prepared = parent.join(format!(
        ".{}-import-{}",
        dir_name.to_string_lossy(),
        uuid::Uuid::new_v4()
    ));

    let result = (|| -> std::io::Result<()> {
        extract_zip_to_dir(zip_path, &staging)?;

        let zip_has_key = staging.join(".key").is_file();
        let logs_path = data_dir.join("logs");
        let had_logs = logs_path.is_dir();
        let key_path = data_dir.join(".key");
        let had_key = key_path.is_file();

        fs::create_dir_all(&prepared)?;
        copy_dir_all_filtered(&staging, &prepared, &staging, |rel| {
            !should_exclude_on_import(rel)
        })?;

        if had_logs {
            copy_dir_all(&logs_path, &prepared.join("logs"))?;
        }

        if had_key && !zip_has_key {
            fs::copy(&key_path, &prepared.join(".key"))?;
        }

        atomic_replace_dir(data_dir, &prepared)
    })();

    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_dir_all(&prepared);
    result
}

/// Atomically replace `data_dir` with the fully prepared `prepared` directory (same parent).
fn atomic_replace_dir(data_dir: &Path, prepared: &Path) -> std::io::Result<()> {
    let parent = data_dir.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "data_dir has no parent directory",
        )
    })?;
    let name = data_dir.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "data_dir has no directory name",
        )
    })?;
    let backup = parent.join(format!("{}.bak", name.to_string_lossy()));

    if backup.exists() {
        if backup.is_dir() {
            fs::remove_dir_all(&backup)?;
        } else {
            fs::remove_file(&backup)?;
        }
    }

    if data_dir.exists() {
        fs::rename(data_dir, &backup)?;

        #[cfg(test)]
        if test_fail_before_swap() {
            let restore = fs::rename(&backup, data_dir);
            return restore.and_then(|_| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "injected swap failure",
                ))
            });
        }

        match fs::rename(prepared, data_dir) {
            Ok(()) => {
                let _ = fs::remove_dir_all(&backup);
                Ok(())
            }
            Err(e) => {
                let _ = fs::rename(&backup, data_dir);
                Err(e)
            }
        }
    } else {
        fs::create_dir_all(parent)?;
        fs::rename(prepared, data_dir)
    }
}

#[cfg(test)]
mod test_hooks {
    use std::cell::Cell;

    thread_local! {
        static FAIL_BEFORE_SWAP: Cell<bool> = const { Cell::new(false) };
    }

    pub fn fail_before_swap() -> bool {
        FAIL_BEFORE_SWAP.with(|f| f.get())
    }

    pub fn set_fail_before_swap(fail: bool) {
        FAIL_BEFORE_SWAP.with(|f| f.set(fail));
    }
}

#[cfg(test)]
fn test_fail_before_swap() -> bool {
    test_hooks::fail_before_swap()
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    copy_dir_all_filtered(src, dst, src, |_| true)
}

fn copy_dir_all_filtered<F>(
    src: &Path,
    dst_root: &Path,
    root: &Path,
    include: F,
) -> std::io::Result<()>
where
    F: Copy + Fn(&Path) -> bool,
{
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        if !include(rel) {
            continue;
        }
        let target = dst_root.join(rel);
        if path.is_dir() {
            fs::create_dir_all(&target)?;
            copy_dir_all_filtered(&path, dst_root, root, include)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn should_exclude_logs_and_temp_patterns() {
        assert!(should_exclude(Path::new("logs")));
        assert!(should_exclude(Path::new("logs/app.log")));
        assert!(should_exclude(Path::new("foo.tmp")));
        assert!(should_exclude(Path::new("dir/.tmp")));
        assert!(should_exclude(Path::new(".import_staging/x")));
        assert!(!should_exclude(Path::new("connections.json")));
        assert!(!should_exclude(Path::new("settings.json")));
        assert!(!should_exclude(Path::new("nested/settings.json")));
    }

    #[test]
    fn should_exclude_key_file_at_root_and_nested() {
        assert!(should_exclude(Path::new(".key")));
        assert!(should_exclude(Path::new("subdir/.key")));
        assert!(should_exclude(Path::new("a/b/.key")));
        assert!(!should_exclude(Path::new("not-key.txt")));
        assert!(!should_exclude(Path::new("my.keyfile")));
    }

    #[test]
    fn export_zip_omits_key_even_when_present_in_source() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "connections.json", r#"{"connections":[]}"#);
        write_file(source.path(), ".key", "secret-aes-key-material");
        write_file(source.path(), "nested/.key", "nested-key");

        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("backup.zip");
        export_app_data(source.path(), &zip_path).unwrap();

        let file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "connections.json"));
        assert!(!names.iter().any(|n| n == ".key" || n.ends_with("/.key")));
    }

    #[test]
    fn import_without_key_in_zip_preserves_existing_target_key() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "settings.json", r#"{"theme":"dark"}"#);
        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("backup.zip");
        export_app_data(source.path(), &zip_path).unwrap();

        let target = TempDir::new().unwrap();
        write_file(target.path(), ".key", "local-encryption-key");
        write_file(target.path(), "old.json", "removed");

        import_app_data(target.path(), &zip_path).unwrap();

        assert_eq!(
            fs::read_to_string(target.path().join(".key")).unwrap(),
            "local-encryption-key"
        );
        assert!(target.path().join("settings.json").exists());
        assert!(!target.path().join("old.json").exists());
    }

    #[test]
    fn import_with_legacy_key_in_zip_restores_key_from_archive() {
        let dir = TempDir::new().unwrap();
        let zip_path = dir.path().join("legacy.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("settings.json", options).unwrap();
            zip.write_all(b"{}").unwrap();
            zip.start_file(".key", options).unwrap();
            zip.write_all(b"legacy-key-from-backup").unwrap();
            zip.finish().unwrap();
        }

        let target = TempDir::new().unwrap();
        write_file(target.path(), ".key", "old-local-key");

        import_app_data(target.path(), &zip_path).unwrap();

        assert_eq!(
            fs::read_to_string(target.path().join(".key")).unwrap(),
            "legacy-key-from-backup"
        );
    }

    #[test]
    fn rejects_path_traversal_in_zip_entries() {
        assert!(validate_zip_entry_path("../etc/passwd").is_err());
        assert!(validate_zip_entry_path("/etc/passwd").is_err());
        assert!(validate_zip_entry_path("C:/Windows/system.ini").is_err());
        assert!(validate_zip_entry_path("foo/../../bar").is_err());
        assert!(validate_zip_entry_path("settings.json").is_ok());
    }

    #[test]
    fn round_trip_excludes_logs_and_preserves_existing_logs() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "connections.json", r#"{"connections":[]}"#);
        write_file(source.path(), "settings.json", r#"{"theme":"dark"}"#);
        write_file(source.path(), "logs/app.log", "log line");
        write_file(source.path(), "scratch.tmp", "temp");
        write_file(source.path(), ".import_staging/partial", "staging");

        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("backup.zip");
        export_app_data(source.path(), &zip_path).unwrap();

        let file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "connections.json"));
        assert!(names.iter().any(|n| n == "settings.json"));
        assert!(!names.iter().any(|n| n.starts_with("logs/")));
        assert!(!names.iter().any(|n| n.contains(".tmp")));
        assert!(!names.iter().any(|n| n.contains(".import_staging")));

        let target = TempDir::new().unwrap();
        write_file(target.path(), "logs/existing.log", "keep me");
        write_file(target.path(), "old.json", "removed");

        import_app_data(target.path(), &zip_path).unwrap();

        assert!(target.path().join("connections.json").exists());
        assert!(target.path().join("settings.json").exists());
        assert!(!target.path().join("old.json").exists());
        assert!(!target.path().join("scratch.tmp").exists());
        assert_eq!(
            fs::read_to_string(target.path().join("logs/existing.log")).unwrap(),
            "keep me"
        );
        assert!(!target.path().join("logs/app.log").exists());
    }

    #[test]
    fn rejects_malicious_zip_on_import() {
        let dir = TempDir::new().unwrap();
        let zip_path = dir.path().join("evil.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("../outside.txt", options).unwrap();
            zip.write_all(b"evil").unwrap();
            zip.finish().unwrap();
        }

        let target = TempDir::new().unwrap();
        write_file(target.path(), "keep.json", "unchanged");
        let err = import_app_data(target.path(), &zip_path).unwrap_err();
        assert!(
            err.to_string().contains("traversal") || err.kind() == std::io::ErrorKind::InvalidInput
        );
        assert_eq!(
            fs::read_to_string(target.path().join("keep.json")).unwrap(),
            "unchanged"
        );
    }

    #[test]
    fn import_swap_failure_restores_original_data_dir() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "new.json", r#"{"imported":true}"#);
        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("backup.zip");
        export_app_data(source.path(), &zip_path).unwrap();

        let target = TempDir::new().unwrap();
        write_file(target.path(), "keep.json", "must-stay");

        test_hooks::set_fail_before_swap(true);
        let result = import_app_data(target.path(), &zip_path);
        test_hooks::set_fail_before_swap(false);

        let err = result.unwrap_err();
        assert!(err.to_string().contains("injected swap failure"));

        assert_eq!(
            fs::read_to_string(target.path().join("keep.json")).unwrap(),
            "must-stay"
        );
        assert!(!target.path().join("new.json").exists());
    }

    #[test]
    fn should_exclude_nested_tmp_and_keep_normal_files() {
        assert!(should_exclude(Path::new("workflows/draft.tmp")));
        assert!(should_exclude(Path::new(".tmp")));
        assert!(!should_exclude(Path::new("workflows/daily.yaml")));
        assert!(!should_exclude(Path::new("contexts/rules.md")));
        assert!(!should_exclude(Path::new("not-logs/app.log")));
    }

    #[test]
    fn validate_zip_entry_accepts_nested_and_backslash() {
        let p = validate_zip_entry_path("contexts\\rules.md").unwrap();
        assert_eq!(p, PathBuf::from("contexts/rules.md"));
        assert!(validate_zip_entry_path("a/b/c.json").is_ok());
        assert!(validate_zip_entry_path("./settings.json").is_ok());
    }

    #[test]
    fn validate_zip_entry_rejects_unc_and_prefix() {
        assert!(validate_zip_entry_path("//server/share").is_err());
        assert!(validate_zip_entry_path("D:\\secret.txt").is_err());
        assert!(validate_zip_entry_path("foo/../../../etc/passwd").is_err());
    }

    #[test]
    fn export_empty_data_dir_creates_valid_zip() {
        let source = TempDir::new().unwrap();
        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("empty.zip");
        export_app_data(source.path(), &zip_path).unwrap();
        assert!(zip_path.is_file());
        let file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let file_entries = (0..archive.len())
            .filter(|&i| !archive.by_index(i).unwrap().name().ends_with('/'))
            .count();
        assert_eq!(file_entries, 0);
    }

    #[test]
    fn export_includes_nested_directories() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "workflows/a.yaml", "id: a\n");
        write_file(source.path(), "contexts/docs/readme.md", "# hi");
        write_file(source.path(), "history/queries.json", "[]");

        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("nested.zip");
        export_app_data(source.path(), &zip_path).unwrap();

        let target = TempDir::new().unwrap();
        import_app_data(target.path(), &zip_path).unwrap();
        assert_eq!(
            fs::read_to_string(target.path().join("workflows/a.yaml")).unwrap(),
            "id: a\n"
        );
        assert_eq!(
            fs::read_to_string(target.path().join("contexts/docs/readme.md")).unwrap(),
            "# hi"
        );
    }

    #[test]
    fn import_without_existing_logs_leaves_no_logs_dir() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "settings.json", r#"{"theme":"light"}"#);
        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("backup.zip");
        export_app_data(source.path(), &zip_path).unwrap();

        let target = TempDir::new().unwrap();
        write_file(target.path(), "stale.txt", "gone");
        import_app_data(target.path(), &zip_path).unwrap();

        assert!(target.path().join("settings.json").exists());
        assert!(!target.path().join("stale.txt").exists());
        assert!(!target.path().join("logs").exists());
    }

    #[test]
    fn import_into_missing_data_dir_creates_it() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "connections.json", "[]");
        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("backup.zip");
        export_app_data(source.path(), &zip_path).unwrap();

        let parent = TempDir::new().unwrap();
        let target = parent.path().join("brand-new-data");
        assert!(!target.exists());
        import_app_data(&target, &zip_path).unwrap();
        assert!(target.join("connections.json").exists());
    }

    #[test]
    fn import_rejects_absolute_unix_path_entry() {
        let dir = TempDir::new().unwrap();
        let zip_path = dir.path().join("abs.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("/etc/passwd", options).unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        let target = TempDir::new().unwrap();
        assert!(import_app_data(target.path(), &zip_path).is_err());
    }

    #[test]
    fn import_extracts_nested_file_creating_parents() {
        let dir = TempDir::new().unwrap();
        let zip_path = dir.path().join("nested.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("workflows/x.yaml", options).unwrap();
            zip.write_all(b"ok").unwrap();
            zip.finish().unwrap();
        }
        let target = TempDir::new().unwrap();
        import_app_data(target.path(), &zip_path).unwrap();
        assert!(target.path().join("workflows").is_dir());
        assert_eq!(
            fs::read_to_string(target.path().join("workflows/x.yaml")).unwrap(),
            "ok"
        );
    }

    #[test]
    fn import_skips_excluded_names_from_staging_copy() {
        // Zip built manually with a .tmp file; import filter should drop it.
        let dir = TempDir::new().unwrap();
        let zip_path = dir.path().join("with-tmp.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("keep.json", options).unwrap();
            zip.write_all(b"{}").unwrap();
            zip.start_file("drop.tmp", options).unwrap();
            zip.write_all(b"tmp").unwrap();
            zip.finish().unwrap();
        }
        let target = TempDir::new().unwrap();
        import_app_data(target.path(), &zip_path).unwrap();
        assert!(target.path().join("keep.json").exists());
        assert!(!target.path().join("drop.tmp").exists());
    }

    fn write_zero_payload_zip(zip_path: &Path, entry_name: &str, zero_bytes: usize) {
        let file = File::create(zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file(entry_name, options).unwrap();
        zip.write_all(&vec![0u8; zero_bytes]).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn rejects_zip_bomb_high_compression_ratio() {
        let dir = TempDir::new().unwrap();
        let zip_path = dir.path().join("ratio-bomb.zip");
        // Highly compressible payload; ratio far above MAX_COMPRESSION_RATIO (100).
        write_zero_payload_zip(&zip_path, "bomb.bin", 512 * 1024);

        let dest = TempDir::new().unwrap();
        let err = extract_zip_to_dir(&zip_path, dest.path()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("zip bomb") && msg.contains("ratio"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn rejects_zip_bomb_uncompressed_size_limit() {
        let dir = TempDir::new().unwrap();
        let zip_path = dir.path().join("size-bomb.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file("big.bin", options).unwrap();
            zip.write_all(&vec![b'a'; 64 * 1024]).unwrap();
            zip.finish().unwrap();
        }

        let dest = TempDir::new().unwrap();
        let limits = ZipExtractLimits {
            max_uncompressed_bytes: 32 * 1024,
            max_compression_ratio: MAX_COMPRESSION_RATIO,
            max_entries: MAX_ZIP_ENTRIES,
        };
        let err = extract_zip_to_dir_with_limits(&zip_path, dest.path(), limits).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("zip bomb") && msg.contains("uncompressed size limit"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn rejects_zip_bomb_too_many_entries() {
        let dir = TempDir::new().unwrap();
        let zip_path = dir.path().join("many-entries.zip");
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            for i in 0..5 {
                zip.start_file(format!("file{i}.txt"), options).unwrap();
                zip.write_all(b"x").unwrap();
            }
            zip.finish().unwrap();
        }

        let dest = TempDir::new().unwrap();
        let limits = ZipExtractLimits {
            max_uncompressed_bytes: MAX_UNCOMPRESSED_BYTES,
            max_compression_ratio: MAX_COMPRESSION_RATIO,
            max_entries: 4,
        };
        let err = extract_zip_to_dir_with_limits(&zip_path, dest.path(), limits).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("zip bomb") && msg.contains("too many entries"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn export_skips_dashboard_runs_when_disabled() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "dashboards.json", "[]");
        write_file(
            source.path(),
            "dashboard-runs/d1/w1/run.json",
            r#"{"id":"run-1"}"#,
        );

        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("backup.zip");
        export_app_data_with_options(
            source.path(),
            &zip_path,
            ExportOptions {
                include_dashboard_runs: false,
            },
        )
        .unwrap();

        let file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n == "dashboards.json"));
        assert!(!names.iter().any(|n| n.starts_with("dashboard-runs/")));
    }

    #[test]
    fn export_includes_dashboard_runs_by_default() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "dashboards.json", "[]");
        write_file(
            source.path(),
            "dashboard-runs/d1/w1/run.json",
            r#"{"id":"run-1"}"#,
        );

        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("backup.zip");
        export_app_data(source.path(), &zip_path).unwrap();

        let file = File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.iter().any(|n| n.starts_with("dashboard-runs/")));
    }

    #[test]
    fn normal_small_backup_imports_after_zip_bomb_checks() {
        let source = TempDir::new().unwrap();
        write_file(source.path(), "connections.json", r#"{"connections":[]}"#);
        write_file(source.path(), "settings.json", r#"{"theme":"dark"}"#);
        write_file(source.path(), "workflows/daily.yaml", "id: daily\n");

        let out = TempDir::new().unwrap();
        let zip_path = out.path().join("backup.zip");
        export_app_data(source.path(), &zip_path).unwrap();

        let target = TempDir::new().unwrap();
        import_app_data(target.path(), &zip_path).unwrap();

        assert!(target.path().join("connections.json").exists());
        assert!(target.path().join("settings.json").exists());
        assert_eq!(
            fs::read_to_string(target.path().join("workflows/daily.yaml")).unwrap(),
            "id: daily\n"
        );
    }

    #[test]
    fn import_rejects_zip_bomb_via_import_app_data() {
        let dir = TempDir::new().unwrap();
        let zip_path = dir.path().join("ratio-bomb.zip");
        write_zero_payload_zip(&zip_path, "bomb.bin", 512 * 1024);

        let target = TempDir::new().unwrap();
        write_file(target.path(), "keep.json", "unchanged");

        let err = import_app_data(target.path(), &zip_path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("zip bomb") && msg.contains("ratio"),
            "unexpected error: {msg}"
        );
        assert_eq!(
            fs::read_to_string(target.path().join("keep.json")).unwrap(),
            "unchanged"
        );
    }
}
