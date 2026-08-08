//! Install a theme pack ZIP into `{themes_root}/{id}/`.

use std::fs::{self, File};
use std::io::{copy, Read};
use std::path::{Path, PathBuf};

use zip::{ZipArchive, ZipWriter};
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

use super::validate::{
    allowed_theme_extension, validate_pack_contents, validate_pack_dir, validate_theme_zip_path,
    ThemeManifest, MAX_THEME_FILES, MAX_THEME_UNCOMPRESSED,
};
use crate::app_data_archive::{self, MAX_COMPRESSION_RATIO};

pub fn install_theme_zip(zip_path: &Path, themes_root: &Path) -> Result<ThemeManifest, String> {
    let staging = std::env::temp_dir().join(format!("datazen-theme-staging-{}", uuid::Uuid::new_v4()));

    let result = (|| -> Result<ThemeManifest, String> {
        extract_theme_zip(zip_path, &staging)?;
        let manifest = validate_pack_contents(&staging)?;
        let dest = themes_root.join(&manifest.id);
        atomic_replace_dir(&dest, &staging)?;
        validate_pack_dir(&dest)?;
        Ok(manifest)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

#[derive(Clone, Copy, Debug)]
struct ThemeZipLimits {
    max_uncompressed_bytes: u64,
    max_compression_ratio: u64,
    max_entries: usize,
}

impl Default for ThemeZipLimits {
    fn default() -> Self {
        Self {
            max_uncompressed_bytes: MAX_THEME_UNCOMPRESSED,
            max_compression_ratio: MAX_COMPRESSION_RATIO,
            max_entries: MAX_THEME_FILES,
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
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "zip bomb: overflow"))?;
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

fn extract_theme_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    extract_theme_zip_with_limits(zip_path, dest, ThemeZipLimits::default())
}

fn extract_theme_zip_with_limits(
    zip_path: &Path,
    dest: &Path,
    limits: ThemeZipLimits,
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

    for i in 0..entry_count {
        let entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let entry_name = entry.name().to_string();
        validate_theme_zip_path(&entry_name)?;
        if entry.is_symlink() {
            return Err(format!("symlink zip entry not allowed: {entry_name}"));
        }

        let is_dir = entry_name.ends_with('/') || entry.is_dir();
        if !is_dir {
            reject_forbidden_zip_extension(&entry_name)?;
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

    let mut bytes_written: u64 = 0;

    for i in 0..entry_count {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let entry_name = entry.name().to_string();
        validate_theme_zip_path(&entry_name)?;
        let rel = app_data_archive::validate_zip_entry_path(&entry_name)
            .map_err(|e| e.to_string())?;
        let out_path = dest.join(&rel);

        if entry_name.ends_with('/') {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            reject_forbidden_zip_extension(&entry_name)?;
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

fn reject_forbidden_zip_extension(entry_name: &str) -> Result<(), String> {
    let ext = Path::new(entry_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();

    if ext.is_empty() {
        return Err(format!("file without extension: {entry_name}"));
    }
    if !allowed_theme_extension(&ext) {
        return Err(format!("forbidden extension .{ext}: {entry_name}"));
    }
    Ok(())
}

/// Atomically replace `dest` with the fully prepared `staging` directory.
fn atomic_replace_dir(dest: &Path, staging: &Path) -> Result<(), String> {
    let parent = dest
        .parent()
        .ok_or_else(|| "destination has no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let name = dest
        .file_name()
        .ok_or_else(|| "destination has no directory name".to_string())?;
    let backup = parent.join(format!("{}.bak", name.to_string_lossy()));

    if backup.exists() {
        if backup.is_dir() {
            fs::remove_dir_all(&backup).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&backup).map_err(|e| e.to_string())?;
        }
    }

    if dest.exists() {
        fs::rename(dest, &backup).map_err(|e| e.to_string())?;
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
    use tempfile::TempDir;

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn install_theme_zip_from_fixture() {
        let zip_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/themes/community.fixture-dark.zip");
        assert!(
            zip_path.is_file(),
            "missing {}; run `node scripts/pack-theme-fixture.mjs`",
            zip_path.display()
        );

        let themes_root = TempDir::new().unwrap();
        let manifest = install_theme_zip(&zip_path, themes_root.path()).unwrap();
        assert_eq!(manifest.id, "community.fixture-dark");

        let installed = themes_root.path().join("community.fixture-dark");
        assert!(installed.join("manifest.json").is_file());
        assert!(installed.join("tokens.css").is_file());
        assert!(installed.join("icons/nav.settings.svg").is_file());
        validate_pack_dir(&installed).unwrap();
    }

    #[test]
    fn install_rejects_path_traversal_zip() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("evil.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("../evil.css", options).unwrap();
            zip.write_all(b"bad").unwrap();
            zip.finish().unwrap();
        }

        let themes_root = TempDir::new().unwrap();
        write_file(themes_root.path(), "keep.txt", "unchanged");
        assert!(install_theme_zip(&zip_path, themes_root.path()).is_err());
    }

    #[test]
    fn extract_rejects_forbidden_extension() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("evil.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("evil.js", options).unwrap();
            zip.write_all(b"alert(1)").unwrap();
            zip.finish().unwrap();
        }

        let dest = tmp.path().join("extract");
        let err = extract_theme_zip(&zip_path, &dest).unwrap_err();
        assert!(err.contains("forbidden extension .js"), "unexpected: {err}");
    }
}
