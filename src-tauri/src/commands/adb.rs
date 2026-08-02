use super::error::{CmdExt, CommandError};
use serde::Serialize;
use std::path::Path;
use tokio::process::Command;

#[derive(Debug, Serialize)]
pub struct AdbPackage {
    pub package_name: String,
}

#[derive(Debug, Serialize)]
pub struct AdbDatabaseFile {
    pub path: String,
    pub name: String,
}

async fn run_adb(args: &[&str]) -> Result<String, CommandError> {
    let output = Command::new("adb")
        .args(args)
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CommandError::NotConfigured(
                    "adb command not found. Please install Android SDK Platform Tools and ensure adb is in PATH.".into(),
                )
            } else {
                CommandError::Internal(format!("Failed to run adb: {e}"))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CommandError::Internal(format!("adb error: {stderr}")));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn validate_package_name(pkg: &str) -> Result<(), CommandError> {
    if pkg.is_empty()
        || !pkg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(CommandError::Validation(format!(
            "Invalid package name: {pkg}"
        )));
    }
    Ok(())
}

fn validate_local_path(path: &str) -> Result<(), CommandError> {
    if path.is_empty() {
        return Err(CommandError::Validation(
            "Local path cannot be empty".into(),
        ));
    }
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(CommandError::Validation(format!(
                "Parent directory does not exist: {}",
                parent.display()
            )));
        }
    }
    Ok(())
}

/// List third-party packages installed on the connected Android device.
#[tauri::command]
pub async fn adb_list_packages() -> Result<Vec<AdbPackage>, CommandError> {
    tracing::info!("[adb_list_packages] listing packages");

    let output = run_adb(&["shell", "pm", "list", "packages", "-3"])
        .await
        .cmd_err("adb_list_packages")?;

    let packages: Vec<AdbPackage> = output
        .lines()
        .filter_map(|line| {
            line.strip_prefix("package:")
                .map(|name| AdbPackage {
                    package_name: name.trim().to_string(),
                })
        })
        .collect();

    tracing::info!("[adb_list_packages] found {} packages", packages.len());
    Ok(packages)
}

/// List database files under a package's data directory.
#[tauri::command]
pub async fn adb_list_databases(package: String) -> Result<Vec<AdbDatabaseFile>, CommandError> {
    validate_package_name(&package)?;
    tracing::info!("[adb_list_databases] package={package}");

    let output = run_adb(&[
        "shell",
        &format!(
            "run-as {package} find ./databases -name '*.db' -o -name '*.sqlite' -o -name '*.sqlite3' 2>/dev/null || echo ''"
        ),
    ])
    .await
    .cmd_err("adb_list_databases")?;

    let files: Vec<AdbDatabaseFile> = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let path = line.trim().to_string();
            let name = Path::new(&path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            AdbDatabaseFile { path, name }
        })
        .collect();

    tracing::info!("[adb_list_databases] found {} db files", files.len());
    Ok(files)
}

/// Pull a database file from the Android device to a local path.
#[tauri::command]
pub async fn adb_pull_database(
    package: String,
    db_path: String,
    local_path: String,
) -> Result<String, CommandError> {
    validate_package_name(&package)?;
    validate_local_path(&local_path)?;

    if db_path.contains("..") || db_path.contains('\0') {
        return Err(CommandError::Validation(format!(
            "Invalid database path: {db_path}"
        )));
    }

    tracing::info!("[adb_pull_database] package={package}, db={db_path}, local={local_path}");

    let output = Command::new("adb")
        .args(["exec-out", &format!("run-as {package} cat {db_path}")])
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CommandError::NotConfigured(
                    "adb command not found. Please install Android SDK Platform Tools and ensure adb is in PATH.".into(),
                )
            } else {
                CommandError::Internal(format!("Failed to run adb: {e}"))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CommandError::Internal(format!(
            "adb pull failed: {stderr}"
        )));
    }

    if output.stdout.is_empty() {
        return Err(CommandError::Internal(
            "adb returned empty data. The file may not exist or the app may not be debuggable."
                .into(),
        ));
    }

    tokio::fs::write(&local_path, &output.stdout)
        .await
        .cmd_err("adb_pull_database")?;

    let size_kb = output.stdout.len() / 1024;
    tracing::info!(
        "[adb_pull_database] saved {} bytes ({size_kb} KB) to {local_path}",
        output.stdout.len()
    );

    Ok(local_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_package_name_valid() {
        assert!(validate_package_name("com.example.app").is_ok());
        assert!(validate_package_name("com.android.chrome").is_ok());
        assert!(validate_package_name("my_app_123").is_ok());
    }

    #[test]
    fn test_validate_package_name_invalid() {
        assert!(validate_package_name("").is_err());
        assert!(validate_package_name("com/evil/path").is_err());
        assert!(validate_package_name("pkg;rm -rf /").is_err());
        assert!(validate_package_name("pkg && echo hi").is_err());
    }

    #[test]
    fn test_validate_local_path_empty() {
        assert!(validate_local_path("").is_err());
    }

    #[test]
    fn test_validate_local_path_valid() {
        assert!(validate_local_path("/tmp/test.db").is_ok());
    }

    #[test]
    fn test_db_path_traversal_rejected() {
        let result = tokio::runtime::Runtime::new().unwrap().block_on(
            adb_pull_database(
                "com.example.app".into(),
                "../../../etc/passwd".into(),
                "/tmp/test.db".into(),
            ),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid database path"));
    }
}
