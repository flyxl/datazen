//! Install runtime extensions from a ZIP archive or a source directory.
//!
//! Flow (mirrors [`crate::theme::install`]): extract/copy into
//! `{plugins_dir}/.staging-{uuid}` → full validation → atomic rename onto
//! `{id}` (existing install is backed up as `{id}.old.bak` and removed once
//! the new package is in place).

use std::fs::{self, File};
use std::io::{copy, Read};
use std::path::{Path, PathBuf};

use uuid::Uuid;
use zip::ZipArchive;

use super::manifest::{
    allowed_extension_file_ext, parse_manifest, validate_extension_dir, validate_manifest,
    ExtensionManifest, MAX_EXTENSION_FILES, MAX_EXTENSION_UNCOMPRESSED,
};
use crate::app_data_archive::MAX_COMPRESSION_RATIO;

const STAGING_PREFIX: &str = ".staging-";
const BACKUP_SUFFIX: &str = ".old.bak";
const INSPECT_PREFIX: &str = ".datazen-inspect-";

/// Install an extension ZIP into `{plugins_dir}/{manifest.id}/`.
pub fn install_from_zip(zip_path: &Path, plugins_dir: &Path) -> Result<ExtensionManifest, String> {
    fs::create_dir_all(plugins_dir).map_err(|e| format!("create plugins dir: {e}"))?;

    let staging = staging_dir(plugins_dir);
    let result = (|| -> Result<ExtensionManifest, String> {
        extract_plugin_zip(zip_path, &staging)?;
        finalize_staged_package(&staging, plugins_dir)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

/// Install an extension from a plain directory into `{plugins_dir}/{manifest.id}/`.
/// Hidden files/dirs are skipped; everything else must pass the same rules a
/// ZIP would.
pub fn install_from_dir(src_dir: &Path, plugins_dir: &Path) -> Result<ExtensionManifest, String> {
    if !src_dir.is_dir() {
        return Err(format!("source directory not found: {}", src_dir.display()));
    }
    fs::create_dir_all(plugins_dir).map_err(|e| format!("create plugins dir: {e}"))?;

    let staging = staging_dir(plugins_dir);
    let result = (|| -> Result<ExtensionManifest, String> {
        copy_extension_dir(src_dir, &staging)?;
        finalize_staged_package(&staging, plugins_dir)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

/// Validate a package fully **without installing it**: the ZIP/directory is
/// materialized into a throwaway temp directory, then the same rule set 1–7
/// as the real install runs against it. On success the manifest is returned
/// (name/version/permissions) so the UI can ask for confirmation; nothing is
/// ever written to `{plugins_dir}`.
pub fn inspect_extension_package(package_path: &Path) -> Result<ExtensionManifest, String> {
    if !package_path.exists() {
        return Err(format!(
            "extension package not found: {}",
            package_path.display()
        ));
    }
    let is_zip = package_path.is_file()
        && package_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"));

    let staging = std::env::temp_dir().join(format!("{INSPECT_PREFIX}{}", Uuid::new_v4()));
    let result = (|| -> Result<ExtensionManifest, String> {
        if is_zip {
            extract_plugin_zip(package_path, &staging)?;
        } else {
            copy_extension_dir(package_path, &staging)?;
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

fn staging_dir(plugins_dir: &Path) -> PathBuf {
    plugins_dir.join(format!("{STAGING_PREFIX}{}", Uuid::new_v4()))
}

/// Resolve the package root inside staging, validate it fully, then swap it
/// into place under `{plugins_dir}/{id}`.
fn finalize_staged_package(
    staging_root: &Path,
    plugins_dir: &Path,
) -> Result<ExtensionManifest, String> {
    let pack_root = resolve_pack_root(staging_root)?;

    // Full rule set (1–7) against the staged content.
    let manifest_path = pack_root.join("manifest.json");
    let content =
        fs::read_to_string(&manifest_path).map_err(|e| format!("read manifest.json: {e}"))?;
    let manifest = super::manifest::parse_manifest(&content)?;
    super::manifest::validate_manifest(&manifest, &pack_root)?;

    let dest = plugins_dir.join(&manifest.id);
    atomic_replace_dir(&dest, &pack_root)?;

    // Belt and suspenders: the installed folder must revalidate cleanly.
    validate_extension_dir(&dest)?;
    Ok(manifest)
}

#[derive(Clone, Copy, Debug)]
struct ExtensionZipLimits {
    max_uncompressed_bytes: u64,
    max_compression_ratio: u64,
    max_entries: usize,
}

impl Default for ExtensionZipLimits {
    fn default() -> Self {
        Self {
            max_uncompressed_bytes: MAX_EXTENSION_UNCOMPRESSED,
            max_compression_ratio: MAX_COMPRESSION_RATIO,
            max_entries: MAX_EXTENSION_FILES,
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

fn extract_plugin_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    extract_plugin_zip_with_limits(zip_path, dest, ExtensionZipLimits::default())
}

fn extract_plugin_zip_with_limits(
    zip_path: &Path,
    dest: &Path,
    limits: ExtensionZipLimits,
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
        crate::theme::validate_theme_zip_path(&entry_name)?;
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
        crate::theme::validate_theme_zip_path(&entry_name)?;
        let rel = crate::app_data_archive::validate_zip_entry_path(&entry_name)
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
    if !allowed_extension_file_ext(&ext) {
        return Err(format!("forbidden extension .{ext}: {entry_name}"));
    }
    Ok(())
}

/// Copy a source tree into staging with the same rules a ZIP must satisfy:
/// no symlinks, no hidden entries, whitelisted extensions, size/count quotas.
fn copy_extension_dir(src: &Path, dest: &Path) -> Result<(), String> {
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
        if stats.files > MAX_EXTENSION_FILES {
            return Err(format!("too many files (max {MAX_EXTENSION_FILES})"));
        }

        stats.total_bytes = stats
            .total_bytes
            .checked_add(meta.len())
            .ok_or_else(|| "package size overflow".to_string())?;
        if stats.total_bytes > MAX_EXTENSION_UNCOMPRESSED {
            return Err(format!(
                "package size exceeds limit ({MAX_EXTENSION_UNCOMPRESSED} bytes)"
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
        fs::rename(dest, &backup).map_err(|e| format!("backup existing plugin: {e}"))?;
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
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Mutex, MutexGuard};
    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    const DEMO_MANIFEST: &str = r#"{
      "id": "acme.demo",
      "name": "Demo",
      "version": "1.0.0",
      "apiVersion": 2,
      "entry": "index.html",
      "contributes": { "pages": [{ "id": "main", "title": "Main" }] },
      "permissions": ["storage:local"]
    }"#;

    fn add_file(zip: &mut ZipWriter<File>, name: &str, content: &str, options: SimpleFileOptions) {
        zip.start_file(name, options).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }

    fn write_demo_zip(zip_path: &Path, top_folder: Option<&str>) {
        let file = fs::File::create(zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        let prefix = top_folder.map(|f| format!("{f}/")).unwrap_or_default();
        add_file(
            &mut zip,
            &format!("{prefix}manifest.json"),
            DEMO_MANIFEST,
            options,
        );
        add_file(
            &mut zip,
            &format!("{prefix}index.html"),
            "<html>v1</html>",
            options,
        );
        zip.finish().unwrap();
    }

    #[test]
    fn install_from_zip_installs_valid_package() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path, None);

        let plugins_root = TempDir::new().unwrap();
        let manifest = install_from_zip(&zip_path, plugins_root.path()).unwrap();
        assert_eq!(manifest.id, "acme.demo");

        let installed = plugins_root.path().join("acme.demo");
        assert!(installed.join("manifest.json").is_file());
        assert!(installed.join("index.html").is_file());
        assert_eq!(
            fs::read_to_string(installed.join("index.html")).unwrap(),
            "<html>v1</html>"
        );
        validate_extension_dir(&installed).unwrap();
    }

    #[test]
    fn install_from_zip_accepts_single_top_level_folder() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path, Some("acme.demo"));

        let plugins_root = TempDir::new().unwrap();
        let manifest = install_from_zip(&zip_path, plugins_root.path()).unwrap();
        assert_eq!(manifest.id, "acme.demo");
        assert!(plugins_root.path().join("acme.demo/index.html").is_file());
    }

    #[test]
    fn install_from_zip_rejects_traversal_entries() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("evil.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            add_file(&mut zip, "../outside.html", "<html>evil</html>", options);
            zip.finish().unwrap();
        }

        let plugins_root = TempDir::new().unwrap();
        fs::write(plugins_root.path().join("keep.txt"), "unchanged").unwrap();

        let err = install_from_zip(&zip_path, plugins_root.path()).unwrap_err();
        assert!(
            err.contains("traversal") || err.contains("invalid zip entry"),
            "unexpected: {err}"
        );
        // Nothing escaped the plugins root and no staging dir remains.
        assert_eq!(
            fs::read_to_string(plugins_root.path().join("keep.txt")).unwrap(),
            "unchanged"
        );
        assert!(!plugins_root
            .path()
            .parent()
            .unwrap()
            .join("outside.html")
            .exists());
        assert!(read_staging_dirs(plugins_root.path()).is_empty());
    }

    fn read_staging_dirs(root: &Path) -> Vec<String> {
        fs::read_dir(root)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|n| n.starts_with(".staging-"))
            .collect()
    }

    #[test]
    fn install_from_zip_rejects_hidden_and_forbidden_entries() {
        for entry_name in [".storage.json", ".enabled", "assets/evil.sh", "run.exe"] {
            let tmp = TempDir::new().unwrap();
            let zip_path = tmp.path().join("bad.zip");
            {
                let file = fs::File::create(&zip_path).unwrap();
                let mut zip = ZipWriter::new(file);
                let options =
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
                add_file(&mut zip, entry_name, "payload", options);
                zip.finish().unwrap();
            }
            let plugins_root = TempDir::new().unwrap();
            let err = install_from_zip(&zip_path, plugins_root.path()).unwrap_err();
            assert!(
                err.contains("hidden file")
                    || err.contains("forbidden extension")
                    || err.contains("without extension"),
                "`{entry_name}`: unexpected error: {err}"
            );
        }
    }

    #[test]
    fn extract_enforces_configurable_size_limit() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("big.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            add_file(&mut zip, "blob.html", &"a".repeat(64 * 1024), options);
            zip.finish().unwrap();
        }

        let dest = tmp.path().join("out");
        let limits = ExtensionZipLimits {
            max_uncompressed_bytes: 32 * 1024,
            ..ExtensionZipLimits::default()
        };
        let err = extract_plugin_zip_with_limits(&zip_path, &dest, limits).unwrap_err();
        assert!(err.contains("uncompressed size limit"), "unexpected: {err}");
    }

    #[test]
    fn install_from_zip_rejects_ratio_bomb_with_default_limits() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("bomb.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            // 512 KiB of zeros compress far beyond MAX_COMPRESSION_RATIO.
            add_file(&mut zip, "bomb.html", &"0".repeat(512 * 1024), options);
            zip.finish().unwrap();
        }

        let plugins_root = TempDir::new().unwrap();
        let err = install_from_zip(&zip_path, plugins_root.path()).unwrap_err();
        assert!(err.contains("ratio"), "unexpected: {err}");
        assert!(read_staging_dirs(plugins_root.path()).is_empty());
    }

    #[test]
    fn reinstall_backs_up_existing_and_cleans_backup() {
        let tmp = TempDir::new().unwrap();
        let plugins_root = TempDir::new().unwrap();

        let v1 = tmp.path().join("v1.zip");
        write_demo_zip(&v1, None);
        install_from_zip(&v1, plugins_root.path()).unwrap();

        let v2 = tmp.path().join("v2.zip");
        {
            let file = fs::File::create(&v2).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let manifest =
                DEMO_MANIFEST.replace("\"version\": \"1.0.0\"", "\"version\": \"2.0.0\"");
            add_file(&mut zip, "manifest.json", &manifest, options);
            add_file(&mut zip, "index.html", "<html>v2</html>", options);
            add_file(&mut zip, "assets/icon.svg", "<svg/>", options);
            zip.finish().unwrap();
        }

        let manifest = install_from_zip(&v2, plugins_root.path()).unwrap();
        assert_eq!(manifest.version, "2.0.0");

        let installed = plugins_root.path().join("acme.demo");
        assert_eq!(
            fs::read_to_string(installed.join("index.html")).unwrap(),
            "<html>v2</html>"
        );
        assert!(installed.join("assets/icon.svg").is_file());
        // Backup removed after success; no staging leftovers.
        assert!(!plugins_root.path().join("acme.demo.old.bak").exists());
        assert!(read_staging_dirs(plugins_root.path()).is_empty());
    }

    #[test]
    fn failed_install_keeps_previous_package_and_cleans_staging() {
        let tmp = TempDir::new().unwrap();
        let plugins_root = TempDir::new().unwrap();

        let good = tmp.path().join("good.zip");
        write_demo_zip(&good, None);
        install_from_zip(&good, plugins_root.path()).unwrap();

        // Broken package: pages declared but entry file missing.
        let bad = tmp.path().join("bad.zip");
        {
            let file = fs::File::create(&bad).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let manifest =
                DEMO_MANIFEST.replace("\"version\": \"1.0.0\"", "\"version\": \"3.0.0\"");
            add_file(&mut zip, "manifest.json", &manifest, options);
            zip.finish().unwrap();
        }

        let err = install_from_zip(&bad, plugins_root.path()).unwrap_err();
        assert!(err.contains("entry"), "unexpected: {err}");

        let installed = plugins_root.path().join("acme.demo");
        assert_eq!(
            fs::read_to_string(installed.join("index.html")).unwrap(),
            "<html>v1</html>",
            "previous package must survive a failed upgrade"
        );
        assert!(read_staging_dirs(plugins_root.path()).is_empty());
    }

    #[test]
    fn install_from_dir_installs_package() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("manifest.json"), DEMO_MANIFEST).unwrap();
        fs::write(src.path().join("index.html"), "<html>dir-src</html>").unwrap();
        fs::create_dir_all(src.path().join("assets")).unwrap();
        fs::write(src.path().join("assets/icon.svg"), "<svg/>").unwrap();

        let plugins_root = TempDir::new().unwrap();
        let manifest = install_from_dir(src.path(), plugins_root.path()).unwrap();
        assert_eq!(manifest.id, "acme.demo");

        let installed = plugins_root.path().join("acme.demo");
        assert!(installed.join("assets/icon.svg").is_file());

        // Reinstall from an updated directory replaces the previous package.
        fs::write(src.path().join("index.html"), "<html>dir-v2</html>").unwrap();
        install_from_dir(src.path(), plugins_root.path()).unwrap();
        assert_eq!(
            fs::read_to_string(installed.join("index.html")).unwrap(),
            "<html>dir-v2</html>"
        );
        assert!(!plugins_root.path().join("acme.demo.old.bak").exists());
    }

    #[test]
    fn install_from_dir_skips_hidden_files() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("manifest.json"), DEMO_MANIFEST).unwrap();
        fs::write(src.path().join("index.html"), "<html></html>").unwrap();
        fs::write(src.path().join(".DS_Store"), "junk").unwrap();
        fs::create_dir_all(src.path().join(".git")).unwrap();
        fs::write(src.path().join(".git/config"), "[core]").unwrap();

        let plugins_root = TempDir::new().unwrap();
        install_from_dir(src.path(), plugins_root.path()).unwrap();

        let installed = plugins_root.path().join("acme.demo");
        assert!(!installed.join(".DS_Store").exists());
        assert!(!installed.join(".git").exists());
        validate_extension_dir(&installed).unwrap();
    }

    #[test]
    fn install_from_dir_rejects_symlinks_and_bad_extensions() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("manifest.json"), DEMO_MANIFEST).unwrap();
        fs::write(src.path().join("index.html"), "<html></html>").unwrap();
        let target = src.path().join("index.html");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, src.path().join("link.html")).unwrap();
            let plugins_root = TempDir::new().unwrap();
            let err = install_from_dir(src.path(), plugins_root.path()).unwrap_err();
            assert!(err.contains("symlink not allowed"), "unexpected: {err}");
            let _ = fs::remove_file(src.path().join("link.html"));
        }

        fs::write(src.path().join("script.py"), "print('hi')").unwrap();
        let plugins_root = TempDir::new().unwrap();
        let err = install_from_dir(src.path(), plugins_root.path()).unwrap_err();
        assert!(err.contains("forbidden extension .py"), "unexpected: {err}");
    }

    #[test]
    fn install_from_dir_missing_source_fails() {
        let plugins_root = TempDir::new().unwrap();
        let err = install_from_dir(Path::new("/nonexistent/src"), plugins_root.path()).unwrap_err();
        assert!(err.contains("not found"), "unexpected: {err}");
    }

    // Inspect staging dirs are created in the global temp dir by production
    // code, so parallel tests would observe each other's transient
    // `.datazen-inspect-*` entries and race on count_inspect_dirs(). Every
    // test below that calls inspect_extension_package holds this lock for its
    // whole body; no other test module creates that prefix.
    static INSPECT_TMP_LOCK: Mutex<()> = Mutex::new(());

    fn inspect_tmp_lock() -> MutexGuard<'static, ()> {
        INSPECT_TMP_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn count_inspect_dirs() -> usize {
        fs::read_dir(std::env::temp_dir())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with(INSPECT_PREFIX))
            })
            .count()
    }

    #[test]
    fn inspect_extension_package_returns_manifest_without_installing() {
        let _inspect_guard = inspect_tmp_lock();
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path, None);

        let before = count_inspect_dirs();
        let manifest = inspect_extension_package(&zip_path).unwrap();
        assert_eq!(manifest.id, "acme.demo");
        assert_eq!(manifest.name, "Demo");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(
            manifest
                .permissions
                .iter()
                .map(|p| p.as_str())
                .collect::<Vec<_>>(),
            vec!["storage:local"]
        );

        // The throwaway staging dir is cleaned up.
        assert_eq!(count_inspect_dirs(), before);
    }

    #[test]
    fn inspect_extension_package_accepts_top_level_folder_and_plain_dirs() {
        let _inspect_guard = inspect_tmp_lock();
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("demo.zip");
        write_demo_zip(&zip_path, Some("acme.demo"));
        let manifest = inspect_extension_package(&zip_path).unwrap();
        assert_eq!(manifest.id, "acme.demo");

        // Directory sources keep install semantics: the folder name does not
        // have to match the manifest id (staging gets renamed on real install).
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("manifest.json"), DEMO_MANIFEST).unwrap();
        fs::write(src.path().join("index.html"), "<html></html>").unwrap();
        let manifest = inspect_extension_package(src.path()).unwrap();
        assert_eq!(manifest.id, "acme.demo");
    }

    #[test]
    fn inspect_extension_package_rejects_invalid_packages() {
        let _inspect_guard = inspect_tmp_lock();
        // Missing path.
        let err =
            inspect_extension_package(Path::new("/nonexistent/datazen-inspect.zip")).unwrap_err();
        assert!(err.contains("not found"), "unexpected: {err}");

        // Manifest failing validation (apiVersion mismatch).
        let tmp = TempDir::new().unwrap();
        let bad = tmp.path().join("bad.zip");
        {
            let file = fs::File::create(&bad).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            let manifest = DEMO_MANIFEST.replace("\"apiVersion\": 2", "\"apiVersion\": 99");
            add_file(&mut zip, "manifest.json", &manifest, options);
            add_file(&mut zip, "index.html", "<html></html>", options);
            zip.finish().unwrap();
        }
        let err = inspect_extension_package(&bad).unwrap_err();
        assert!(err.contains("apiVersion"), "unexpected: {err}");

        // Malicious traversal entry is rejected by the shared extraction rules.
        let evil = tmp.path().join("evil.zip");
        {
            let file = fs::File::create(&evil).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            add_file(&mut zip, "../outside.html", "<html>evil</html>", options);
            zip.finish().unwrap();
        }
        let err = inspect_extension_package(&evil).unwrap_err();
        assert!(
            err.contains("traversal") || err.contains("invalid zip entry"),
            "unexpected: {err}"
        );
        assert_eq!(count_inspect_dirs(), 0);
    }
}
