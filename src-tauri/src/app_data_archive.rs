//! Pure helpers for exporting/importing the application data directory as ZIP.

use std::fs::{self, File};
use std::io::{copy, Write};
use std::path::{Component, Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Returns true if `rel_path` (relative to the data dir root) should be skipped.
pub fn should_exclude(rel_path: &Path) -> bool {
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

/// Recursively zip `data_dir` into `zip_path`, preserving relative paths.
pub fn export_app_data(data_dir: &Path, zip_path: &Path) -> std::io::Result<()> {
    let file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    add_dir_to_zip(&mut zip, data_dir, data_dir, options)?;
    zip.finish()?;
    Ok(())
}

fn add_dir_to_zip<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    root: &Path,
    dir: &Path,
    options: SimpleFileOptions,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(root).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
        })?;
        if should_exclude(rel) {
            continue;
        }
        if path.is_dir() {
            add_dir_to_zip(zip, root, &path, options)?;
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

fn extract_zip_to_dir(zip_path: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
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
            copy(&mut entry, &mut out_file)?;
        }
    }
    Ok(())
}

/// Replace `data_dir` contents from `zip_path`. Existing `logs/` is preserved when present.
pub fn import_app_data(data_dir: &Path, zip_path: &Path) -> std::io::Result<()> {
    let staging = std::env::temp_dir().join(format!(
        "datazen-import-{}",
        uuid::Uuid::new_v4()
    ));
    let logs_backup = std::env::temp_dir().join(format!(
        "datazen-logs-{}",
        uuid::Uuid::new_v4()
    ));

    let result = (|| -> std::io::Result<()> {
        extract_zip_to_dir(zip_path, &staging)?;

        let logs_path = data_dir.join("logs");
        let had_logs = logs_path.is_dir();
        if had_logs {
            copy_dir_all(&logs_path, &logs_backup)?;
        }

        clear_dir_contents(data_dir)?;
        copy_dir_all_filtered(&staging, data_dir, &staging, |rel| !should_exclude(rel))?;

        if had_logs {
            copy_dir_all(&logs_backup, &logs_path)?;
        }

        Ok(())
    })();

    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_dir_all(&logs_backup);
    result
}

fn clear_dir_contents(dir: &Path) -> std::io::Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    copy_dir_all_filtered(src, dst, src, |_| true)
}

fn copy_dir_all_filtered<F>(
    src: &Path,
    dst: &Path,
    root: &Path,
    include: F,
) -> std::io::Result<()>
where
    F: Copy + Fn(&Path) -> bool,
{
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(root).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
        })?;
        if !include(rel) {
            continue;
        }
        let target = dst.join(rel);
        if path.is_dir() {
            copy_dir_all_filtered(&path, &target, root, include)?;
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

        let zip_path = source.path().join("backup.zip");
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
        let err = import_app_data(target.path(), &zip_path).unwrap_err();
        assert!(
            err.to_string().contains("traversal") || err.kind() == std::io::ErrorKind::InvalidInput
        );
    }
}
