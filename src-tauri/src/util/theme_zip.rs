//! Zip entry path validation for theme and extension archives.

/// Validate a zip entry path; reject absolute paths, `..`, and symlink-like names.
pub fn validate_theme_zip_path(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('\0') {
        return Err("invalid zip entry name".into());
    }
    if name.contains(" -> ") {
        return Err("symlink entry name not allowed".into());
    }

    crate::app_data_archive::validate_zip_entry_path(name)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_entry() {
        assert!(validate_theme_zip_path("../evil.css").is_err());
        assert!(validate_theme_zip_path("icons/../../x.css").is_err());
        assert!(validate_theme_zip_path("tokens.css").is_ok());
    }

    #[test]
    fn validate_theme_zip_path_rejects_empty_and_null() {
        assert!(validate_theme_zip_path("").is_err());
        assert!(validate_theme_zip_path("icons\0evil.css").is_err());
        assert!(validate_theme_zip_path("a -> b").is_err());
    }
}
