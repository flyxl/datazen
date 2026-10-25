//! Zip-slip guards for archive entry names.
//!
//! Why this capability is a cross-form primitive rather than a host-private
//! helper: the same rules encode the zip-slip boundary on every form that
//! accepts an archive — the desktop app-data backup/restore path and the
//! workspace-app / theme package installer — so the rules are *mechanism*
//! (path safety), not policy. The guards themselves are pure `std`: no IO, no
//! transport, no ambient state, nothing form-specific.
//!
//! Two byte-identical copies used to exist, one per consumer
//! (`src-tauri/src/app_data_archive.rs` and the workspace-app installer, whose
//! package has since been merged into `packages/feature-core/src/workspace_apps/`).
//! Neither imported the other and
//! the workspace-app copy only named the host copy in its header, so the host
//! side did not even know a second copy existed — a one-sided edit left one
//! side still enforcing the old rules, which on a security check is the worst
//! possible failure direction. Exactly one implementation lives here and every
//! form links it; callers keep their own entry points and error mapping.
//!
//! `MAX_COMPRESSION_RATIO` belongs here for the same reason: a zip-bomb
//! threshold is a property of the archive, not of whoever opened it, and two
//! independent constants drift apart silently.

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
        // The drive-letter guard also fires for a Windows-style backslash name.
        assert_eq!(reason("D:\\secret.txt"), "absolute path in zip entry");
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
    fn deep_traversal_is_rejected_at_the_first_parent_component() {
        assert!(validate_zip_entry_path("foo/../../../etc/passwd").is_err());
    }

    #[test]
    fn ratio_limit_is_the_documented_constant() {
        assert_eq!(MAX_COMPRESSION_RATIO, 100);
    }
}
