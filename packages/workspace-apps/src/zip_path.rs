//! Zip entry-path guards shared by manifest, install and archive extraction.
//!
//! These checks used to live in two host modules — `app_data_archive`
//! (`validate_zip_entry_path`, `MAX_COMPRESSION_RATIO`) and a thin
//! `util::theme_zip` wrapper around it. The kernel cannot import host
//! modules, and the rules these encode (no absolute paths, no `..`
//! traversal, no symlink-looking names) are part of the package contract the
//! kernel enforces on every install, so the guards moved here.
//!
//! The host keeps `app_data_archive::validate_zip_entry_path` for
//! app-data backup/restore archives — a separate entry point with its own
//! tests, and pulling it in would drag host state into the kernel. The
//! `util::theme_zip` wrapper had no other production caller once the wapp
//! installer moved, so it was deleted rather than left as dead code; the two
//! guards it added (empty / NUL / `" -> "` names) are pinned here by
//! `empty_nul_and_symlink_names_are_rejected`.

use std::path::{Component, PathBuf};

/// Reject when `uncompressed / compressed` exceeds this value (per entry or globally).
pub const MAX_COMPRESSION_RATIO: u64 = 100;

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

/// Entry-name guard used for extension-style archives: empty/NUL/symlink-looking
/// names are rejected before the path rules above are applied.
pub fn validate_theme_zip_path(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('\0') {
        return Err("invalid zip entry name".into());
    }
    if name.contains(" -> ") {
        return Err("symlink entry name not allowed".into());
    }
    validate_zip_entry_path(name)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(name: &str) -> String {
        validate_theme_zip_path(name).expect_err("must be rejected")
    }

    #[test]
    fn ordinary_entries_pass() {
        for name in ["manifest.json", "assets/app.js", "a/b/c.css", "./x.png"] {
            assert!(validate_theme_zip_path(name).is_ok(), "{name} must pass");
        }
        assert_eq!(
            validate_zip_entry_path("assets\\app.js").expect("ok"),
            PathBuf::from("assets/app.js")
        );
    }

    #[test]
    fn absolute_and_traversal_entries_are_rejected() {
        assert_eq!(reason("/etc/passwd"), "absolute path in zip entry");
        assert_eq!(reason("//host/share"), "absolute path in zip entry");
        assert_eq!(reason("C:/Windows/system32"), "absolute path in zip entry");
        assert_eq!(reason("../outside.html"), "path traversal in zip entry");
        assert_eq!(
            reason("assets/../../outside.html"),
            "path traversal in zip entry"
        );
    }

    #[test]
    fn empty_nul_and_symlink_names_are_rejected() {
        assert_eq!(reason(""), "invalid zip entry name");
        assert_eq!(reason("bad\0name"), "invalid zip entry name");
        assert_eq!(
            reason("link -> /etc/passwd"),
            "symlink entry name not allowed"
        );
    }

    #[test]
    fn ratio_limit_is_the_documented_constant() {
        // The kernel pins 100:1; the host archive guard keeps the same value.
        assert_eq!(MAX_COMPRESSION_RATIO, 100);
    }
}
