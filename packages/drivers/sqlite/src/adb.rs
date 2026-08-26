//! ADB (Android Debug Bridge) helper commands — SQLite-driver specific.
//!
//! Pulling `.db` files off an Android device is a capability of the SQLite
//! driver, exposed through the generic Driver Command API instead of dedicated
//! host IPCs (IPC refactor decision 2):
//!
//! * [`adb_command_definitions`] advertises three `requires_connection = false`
//!   commands executed by `driverType` (`"sqlite"`), no live session needed.
//! * The two list commands return the same JSON shapes as the former host IPCs
//!   (`{"package_name": …}` / `{"path", "name"}`) so frontend types stay put.
//! * `adb_pull_database` returns `{ fileName, dataBase64 }`; its metadata
//!   declares a [`DriverSaveDialogSpec`] so the host thin shell — generically,
//!   for any driver that opts in — pops the native save dialog, writes the
//!   bytes and hands back `{ savedPath }`. The driver never touches dialogs or
//!   the local filesystem.

use std::path::Path;

use datazen_driver_api::{
    CommandAccessLevel, CommandCategory, CommandResult, DriverCommandDefinition,
    DriverCommandMetadata, DriverError, DriverSaveDialogSpec,
};
use serde_json::{json, Value as JsonValue};
use tokio::process::Command;

const ADB_COMMANDS: &[&str] = &[
    "adb_list_packages",
    "adb_list_databases",
    "adb_pull_database",
];

const ADB_NOT_FOUND_MESSAGE: &str =
    "adb command not found. Please install Android SDK Platform Tools and ensure adb is in PATH.";

/// Driver Command definitions for the ADB helper surface.
pub fn adb_command_definitions() -> Vec<DriverCommandDefinition> {
    let save_spec = DriverSaveDialogSpec {
        file_name_field: "fileName".into(),
        data_base64_field: "dataBase64".into(),
        filter_name: "SQLite Database".into(),
        extensions: vec!["db".into(), "sqlite".into(), "sqlite3".into()],
        result_path_field: "savedPath".into(),
    };
    vec![
        DriverCommandDefinition {
            id: "adb_list_packages".into(),
            name: "List Android Packages".into(),
            description: Some(
                "List third-party packages installed on the connected Android device (via adb)"
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
            output_schema: Some(json!({
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "package_name": { "type": "string" } },
                    "required": ["package_name"]
                }
            })),
            permissions: vec!["driver.adb.list".into()],
            metadata: DriverCommandMetadata::new(CommandCategory::Io, CommandAccessLevel::Read)
                .unbound()
                .hide_from_workflow(),
        },
        DriverCommandDefinition {
            id: "adb_list_databases".into(),
            name: "List Android Databases".into(),
            description: Some(
                "List SQLite database files under a package's data directory (via adb)".into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "package": {
                        "type": "string",
                        "description": "Android package name, e.g. com.example.app"
                    }
                },
                "required": ["package"]
            }),
            output_schema: Some(json!({
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "name": { "type": "string" }
                    },
                    "required": ["path", "name"]
                }
            })),
            permissions: vec!["driver.adb.list".into()],
            metadata: DriverCommandMetadata::new(CommandCategory::Io, CommandAccessLevel::Read)
                .unbound()
                .hide_from_workflow(),
        },
        DriverCommandDefinition {
            id: "adb_pull_database".into(),
            name: "Pull Android Database".into(),
            description: Some(
                "Pull a SQLite database file from an Android device; the host thin shell saves \
                 it through a native save dialog and returns the picked path"
                    .into(),
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "package": {
                        "type": "string",
                        "description": "Android package name, e.g. com.example.app"
                    },
                    "dbPath": {
                        "type": "string",
                        "description": "Database file path inside the package's data directory"
                    }
                },
                "required": ["package", "dbPath"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "description": "Bytes envelope before the host save dialog; interactive \
                                execute_driver_command calls replace it with { savedPath }",
                "properties": {
                    "fileName": { "type": "string" },
                    "dataBase64": { "type": "string" }
                },
                "required": ["fileName", "dataBase64"]
            })),
            permissions: vec!["driver.adb.pull".into()],
            metadata: DriverCommandMetadata::new(CommandCategory::Io, CommandAccessLevel::Write)
                .unbound()
                .hide_from_workflow()
                .save_dialog(save_spec),
        },
    ]
}

pub fn is_adb_command(command: &str) -> bool {
    ADB_COMMANDS.contains(&command)
}

fn input_str(command: &str, input: &JsonValue, key: &str) -> Result<String, DriverError> {
    input
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            DriverError::InvalidConfig(format!("Command '{command}' requires string input '{key}'"))
        })
}

fn validate_package_name(pkg: &str) -> Result<(), DriverError> {
    if pkg.is_empty()
        || !pkg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(DriverError::InvalidConfig(format!(
            "Invalid package name: {pkg}"
        )));
    }
    Ok(())
}

fn validate_db_path(db_path: &str) -> Result<(), DriverError> {
    if db_path.contains("..") || db_path.contains('\0') {
        return Err(DriverError::InvalidConfig(format!(
            "Invalid database path: {db_path}"
        )));
    }
    Ok(())
}

fn default_pull_file_name(db_path: &str) -> String {
    Path::new(db_path)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pulled.db".into())
}

async fn adb_output(program: &str, args: &[&str]) -> Result<String, DriverError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DriverError::InvalidConfig(ADB_NOT_FOUND_MESSAGE.into())
            } else {
                DriverError::QueryFailed(format!("Failed to run adb: {e}"))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DriverError::QueryFailed(format!("adb error: {stderr}")));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn run_adb(args: &[&str]) -> Result<String, DriverError> {
    adb_output("adb", args).await
}

fn parse_adb_package_list(output: &str) -> Vec<JsonValue> {
    output
        .lines()
        .filter_map(|line| {
            line.strip_prefix("package:")
                .map(|name| json!({ "package_name": name.trim() }))
        })
        .collect()
}

fn parse_adb_database_lines(output: &str) -> Vec<JsonValue> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let path = line.trim();
            let name = Path::new(path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string());
            json!({ "path": path, "name": name })
        })
        .collect()
}

/// Pull raw database bytes via `adb exec-out run-as <pkg> cat <db_path>`.
async fn pull_database_bytes(package: &str, db_path: &str) -> Result<Vec<u8>, DriverError> {
    validate_package_name(package)?;
    validate_db_path(db_path)?;

    let output = tokio::process::Command::new("adb")
        .args(["exec-out", &format!("run-as {package} cat {db_path}")])
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DriverError::InvalidConfig(ADB_NOT_FOUND_MESSAGE.into())
            } else {
                DriverError::QueryFailed(format!("Failed to run adb: {e}"))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DriverError::QueryFailed(format!(
            "adb pull failed: {stderr}"
        )));
    }

    if output.stdout.is_empty() {
        return Err(DriverError::QueryFailed(
            "adb returned empty data. The file may not exist or the app may not be debuggable."
                .into(),
        ));
    }

    Ok(output.stdout)
}

/// Execute one of the ADB driver commands (no connection session involved).
pub async fn execute_adb_command(
    command: &str,
    input: &JsonValue,
) -> Result<CommandResult, DriverError> {
    match command {
        "adb_list_packages" => {
            tracing::info!("[adb_list_packages] listing packages");
            let output = run_adb(&["shell", "pm", "list", "packages", "-3"]).await?;
            let packages = parse_adb_package_list(&output);
            tracing::info!("[adb_list_packages] found {} packages", packages.len());
            Ok(CommandResult::new(JsonValue::Array(packages)))
        }
        "adb_list_databases" => {
            let package = input_str(command, input, "package")?;
            validate_package_name(&package)?;
            tracing::info!("[adb_list_databases] package={package}");
            let output = run_adb(&[
                "shell",
                &format!(
                    "run-as {package} find ./databases -name '*.db' -o -name '*.sqlite' -o -name '*.sqlite3' 2>/dev/null || echo ''"
                ),
            ])
            .await?;
            let files = parse_adb_database_lines(&output);
            tracing::info!("[adb_list_databases] found {} db files", files.len());
            Ok(CommandResult::new(JsonValue::Array(files)))
        }
        "adb_pull_database" => {
            let package = input_str(command, input, "package")?;
            let db_path = input_str(command, input, "dbPath")?;
            let bytes = pull_database_bytes(&package, &db_path).await?;
            use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
            // Byte payload only — the host thin shell owns the native save
            // dialog (metadata.saveDialog) and returns { savedPath } to JS.
            Ok(CommandResult::new(json!({
                "fileName": default_pull_file_name(&db_path),
                "dataBase64": BASE64.encode(bytes),
            })))
        }
        _ => Err(DriverError::Unsupported(format!(
            "unsupported driver command: {command}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_adb_package_list() {
        let output = "package:com.example.app\npackage:com.android.chrome\nignored\n";
        let packages = parse_adb_package_list(output);
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0]["package_name"], "com.example.app");
        assert_eq!(packages[1]["package_name"], "com.android.chrome");
    }

    #[test]
    fn test_parse_adb_database_lines() {
        let output = "./databases/app.db\n\n./databases/cache.sqlite3\n";
        let files = parse_adb_database_lines(output);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0]["name"], "app.db");
        assert_eq!(files[0]["path"], "./databases/app.db");
        assert_eq!(files[1]["name"], "cache.sqlite3");
    }

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
    fn test_db_path_traversal_rejected() {
        assert!(validate_db_path("../../../etc/passwd").is_err());
        assert!(validate_db_path("./databases/app.db").is_ok());
    }

    #[test]
    fn test_default_pull_file_name() {
        assert_eq!(default_pull_file_name("./databases/app.db"), "app.db");
        assert_eq!(default_pull_file_name(""), "pulled.db");
    }

    #[test]
    fn definitions_cover_exactly_the_three_adb_commands() {
        let ids: Vec<_> = adb_command_definitions()
            .iter()
            .map(|d| d.id.clone())
            .collect();
        assert_eq!(ids, ADB_COMMANDS.to_vec());
        assert!(ADB_COMMANDS.iter().all(|c| is_adb_command(c)));
        assert!(!is_adb_command("query"));
    }

    #[test]
    fn adb_commands_run_without_connection_and_stay_out_of_workflows() {
        for definition in adb_command_definitions() {
            assert!(
                !definition.metadata.requires_connection,
                "{} must be executable by driverType without a session",
                definition.id
            );
            assert!(
                !definition.metadata.workflow,
                "{} must stay out of the Workflow UI",
                definition.id
            );
            assert!(definition.input_schema["type"] == "object");
        }

        let pull = adb_command_definitions()
            .into_iter()
            .find(|d| d.id == "adb_pull_database")
            .unwrap();
        let spec = pull
            .metadata
            .save_dialog
            .expect("pull declares save dialog");
        assert_eq!(spec.file_name_field, "fileName");
        assert_eq!(spec.data_base64_field, "dataBase64");
        assert_eq!(spec.filter_name, "SQLite Database");
        assert_eq!(spec.extensions, vec!["db", "sqlite", "sqlite3"]);
        assert_eq!(spec.result_path_field, "savedPath");

        for id in ["adb_list_packages", "adb_list_databases"] {
            let definition = adb_command_definitions()
                .into_iter()
                .find(|d| d.id == id)
                .unwrap();
            assert!(definition.metadata.save_dialog.is_none());
        }
    }

    #[test]
    fn pull_input_schema_requires_package_and_db_path() {
        let pull = adb_command_definitions()
            .into_iter()
            .find(|d| d.id == "adb_pull_database")
            .unwrap();
        assert_eq!(
            pull.input_schema["required"],
            serde_json::json!(["package", "dbPath"])
        );
        assert_eq!(pull.input_schema["properties"]["dbPath"]["type"], "string");
    }

    #[tokio::test]
    async fn unknown_command_is_unsupported() {
        let err = execute_adb_command("adb_drop_everything", &json!({}))
            .await
            .unwrap_err();
        assert!(
            matches!(err, DriverError::Unsupported(msg) if msg.contains("adb_drop_everything"))
        );
    }

    #[tokio::test]
    async fn list_databases_requires_string_package_input() {
        let err = execute_adb_command("adb_list_databases", &json!({}))
            .await
            .unwrap_err();
        assert!(
            matches!(err, DriverError::InvalidConfig(ref msg) if msg.contains("'package'")),
            "unexpected error: {err:?}"
        );
    }

    #[tokio::test]
    async fn pull_validates_paths_before_spawning_adb() {
        let input = json!({ "package": "com.example.app", "dbPath": "../../../etc/passwd" });
        let err = execute_adb_command("adb_pull_database", &input)
            .await
            .unwrap_err();
        assert!(
            matches!(err, DriverError::InvalidConfig(ref msg) if msg.contains("Invalid database path")),
            "unexpected error: {err:?}"
        );

        let input = json!({ "package": "bad;pkg", "dbPath": "./databases/app.db" });
        let err = execute_adb_command("adb_pull_database", &input)
            .await
            .unwrap_err();
        assert!(
            matches!(err, DriverError::InvalidConfig(ref msg) if msg.contains("Invalid package name"))
        );
    }

    #[tokio::test]
    async fn missing_adb_binary_reports_install_guidance() {
        // Same mapping as the former host IPC: a NotFound spawn error becomes a
        // descriptive "tool not configured" error, not a raw IO failure.
        let err = adb_output("datazen-missing-adb-tool-for-tests", &["version"])
            .await
            .unwrap_err();
        assert!(
            matches!(err, DriverError::InvalidConfig(ref msg)
                if msg.contains("not found") && msg.contains("Android SDK Platform Tools")),
            "unexpected error: {err:?}"
        );
    }
}
