use super::error::{CmdExt, CommandError};
use super::AppState;
use std::path::PathBuf;
use tauri::State;

fn require_webdriver_path_ipc(disabled_msg: &'static str) -> Result<(), CommandError> {
    if !cfg!(feature = "webdriver") {
        return Err(CommandError::Validation(disabled_msg.into()));
    }
    Ok(())
}

/// Parsed mysqldump-style backup CLI flags from option strings.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct BackupDumpOptions {
    pub schema_only: bool,
    pub data_only: bool,
    pub add_drop: bool,
    pub add_create_db: bool,
}

pub(crate) fn parse_backup_options(options: &[String]) -> BackupDumpOptions {
    let opts: std::collections::HashSet<String> = options.iter().cloned().collect();
    BackupDumpOptions {
        schema_only: opts.contains("schema-only") || opts.contains("no-data"),
        data_only: opts.contains("data-only") || opts.contains("no-create-info"),
        add_drop: opts.contains("clean") || opts.contains("add-drop-table"),
        add_create_db: opts.contains("create"),
    }
}

pub(crate) fn validate_backup_filter_extension(filter_extension: &str) -> Result<String, CommandError> {
    let ext = filter_extension.trim_start_matches('.').to_lowercase();
    let allowed = ["sql", "gz", "dump"];
    if !allowed.contains(&ext.as_str()) {
        return Err(CommandError::Validation(format!(
            "File extension '.{ext}' not allowed"
        )));
    }
    Ok(ext)
}

pub(crate) fn backup_sql_header(db_name: &str, opts: &BackupDumpOptions) -> String {
    let mut out = String::new();
    out.push_str(&format!("-- DataZen backup: {db_name}\n"));
    out.push_str(&format!("-- Date: {}\n", chrono::Utc::now().to_rfc3339()));
    if opts.schema_only || opts.data_only || opts.add_drop || opts.add_create_db {
        let flags: Vec<&str> = [
            opts.schema_only.then_some("schema-only"),
            opts.data_only.then_some("data-only"),
            opts.add_drop.then_some("add-drop-table"),
            opts.add_create_db.then_some("create"),
        ]
        .into_iter()
        .flatten()
        .collect();
        out.push_str(&format!("-- Options: {}\n", flags.join(", ")));
    }
    out.push('\n');
    out
}

#[tauri::command]
pub async fn backup_database(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    output_path: String,
    options: Option<Vec<String>>,
    compress: Option<bool>,
) -> Result<(), CommandError> {
    require_webdriver_path_ipc("Direct path backup disabled; use backup_database_with_dialog")?;
    backup_database_to_path(
        &state,
        connection_id,
        database,
        PathBuf::from(output_path),
        options,
        compress,
    )
    .await
}

/// Native save dialog + database backup. Returns `true` if written.
#[tauri::command]
pub async fn backup_database_with_dialog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    default_file_name: String,
    filter_extension: String,
    options: Option<Vec<String>>,
    compress: Option<bool>,
) -> Result<bool, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let ext = validate_backup_filter_extension(&filter_extension)?;

    let picked = app
        .dialog()
        .file()
        .add_filter("Backup", &[ext.as_str()])
        .set_file_name(&default_file_name)
        .blocking_save_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let path = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;
    backup_database_to_path(&state, connection_id, database, path, options, compress).await?;
    Ok(true)
}

async fn backup_database_to_path(
    state: &AppState,
    connection_id: String,
    database: Option<String>,
    output_path: PathBuf,
    options: Option<Vec<String>>,
    compress: Option<bool>,
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, path = %output_path.display(), "backup_database");
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .cmd_err("backup_database")?;

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("backup_database")?;

    let db_name = database
        .as_deref()
        .unwrap_or(config.database.as_deref().unwrap_or(""));
    let opts = parse_backup_options(&options.unwrap_or_default());
    let schema_only = opts.schema_only;
    let data_only = opts.data_only;
    let add_drop = opts.add_drop;
    let add_create_db = opts.add_create_db;

    let tables = driver
        .get_tables(&handle, db_name)
        .await
        .cmd_err("backup_database")?;

    let qi = |name: &str| driver.quote_ident(name);

    let mut out = backup_sql_header(db_name, &opts);

    if add_create_db {
        let q_db = qi(db_name);
        out.push_str(&format!("CREATE DATABASE IF NOT EXISTS {};\n", q_db));
        out.push_str(&format!("\\connect {};\n\n", q_db));
    }

    for table in &tables {
        let tname = &table.name;

        let schema = driver
            .get_table_schema(&handle, tname)
            .await
            .cmd_err("backup_database")?;

        out.push_str(&format!("-- Table: {}\n", tname));

        if add_drop {
            out.push_str(&format!("DROP TABLE IF EXISTS {};\n", qi(tname)));
        }

        if !data_only {
            let cols_sql: Vec<String> = schema
                .columns
                .iter()
                .map(|c| {
                    let mut def = format!("  {} {}", qi(&c.name), c.data_type);
                    if !c.nullable {
                        def.push_str(" NOT NULL");
                    }
                    if let Some(ref dv) = c.default_value {
                        def.push_str(&format!(" DEFAULT {}", dv));
                    }
                    def
                })
                .collect();

            let mut create = format!(
                "CREATE TABLE IF NOT EXISTS {} (\n{}",
                qi(tname),
                cols_sql.join(",\n")
            );
            if !schema.primary_keys.is_empty() {
                let pks: Vec<String> = schema.primary_keys.iter().map(|k| qi(k)).collect();
                create.push_str(&format!(",\n  PRIMARY KEY ({})", pks.join(", ")));
            }
            create.push_str("\n);\n\n");
            out.push_str(&create);
        }

        if !schema_only {
            let col_names: Vec<String> = schema.columns.iter().map(|c| qi(&c.name)).collect();
            let select_sql = format!("SELECT {} FROM {}", col_names.join(", "), qi(tname));

            match driver.query(&handle, &select_sql).await {
                Ok(result) => {
                    for row in &result.rows {
                        let vals: Vec<String> = row.iter().map(format_backup_value).collect();
                        out.push_str(&format!(
                            "INSERT INTO {} ({}) VALUES ({});\n",
                            qi(tname),
                            col_names.join(", "),
                            vals.join(", ")
                        ));
                    }
                    out.push('\n');
                }
                Err(e) => {
                    out.push_str(&format!("-- Error dumping data for {tname}: {e}\n\n"));
                }
            }
        }
    }

    let data = out.as_bytes();
    if compress.unwrap_or(false) {
        use std::io::Write;
        let file = std::fs::File::create(&output_path).map_err(|e| {
            tracing::error!(cmd = "backup_database", error = %e);
            CommandError::Io(e)
        })?;
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder.write_all(data).map_err(|e| {
            tracing::error!(cmd = "backup_database", error = %e);
            CommandError::Io(e)
        })?;
        encoder.finish().map_err(|e| {
            tracing::error!(cmd = "backup_database", error = %e);
            CommandError::Io(e)
        })?;
    } else {
        tokio::fs::write(&output_path, data)
            .await
            .cmd_err("backup_database")?;
    }
    tracing::info!(path = %output_path.display(), "backup_database OK");
    Ok(())
}

#[tauri::command]
pub async fn restore_database(
    state: State<'_, AppState>,
    connection_id: String,
    input_path: String,
) -> Result<(), CommandError> {
    require_webdriver_path_ipc("Direct path restore disabled; use restore_database_with_dialog")?;
    restore_database_from_path(&state, connection_id, PathBuf::from(input_path)).await
}

/// Native open dialog + restore. Returns `true` if restored.
#[tauri::command]
pub async fn restore_database_with_dialog(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<bool, CommandError> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .add_filter("SQL", &["sql"])
        .blocking_pick_file();
    let Some(fp) = picked else {
        return Ok(false);
    };
    let path = fp
        .into_path()
        .map_err(|e| CommandError::Validation(format!("Invalid dialog path: {e}")))?;
    restore_database_from_path(&state, connection_id, path).await?;
    Ok(true)
}

fn split_restore_statements(sql: &str) -> Vec<&str> {
    sql.split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with("--"))
        .collect()
}

fn format_backup_value(v: &Option<crate::db::Value>) -> String {
    match v {
        None => "NULL".to_string(),
        Some(crate::db::Value::Null) => "NULL".to_string(),
        Some(crate::db::Value::Bool(b)) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Some(crate::db::Value::Integer(n)) => n.to_string(),
        Some(crate::db::Value::Float(f)) => f.to_string(),
        Some(crate::db::Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
        Some(crate::db::Value::Timestamp(s)) => format!("'{s}'"),
        Some(crate::db::Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
        Some(crate::db::Value::Bytes(b)) => format!(
            "'\\x{}'",
            b.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        ),
    }
}

async fn restore_database_from_path(
    state: &AppState,
    connection_id: String,
    input_path: PathBuf,
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, path = %input_path.display(), "restore_database");
    let sql = tokio::fs::read_to_string(&input_path)
        .await
        .cmd_err("restore_database")?;

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("restore_database")?;

    let statements = split_restore_statements(&sql);

    let mut errors = Vec::new();
    for stmt in &statements {
        let full = format!("{};", stmt);
        if let Err(e) = driver.execute(&handle, &full).await {
            let max = 80;
            let end = if stmt.len() <= max {
                stmt.len()
            } else {
                let mut e = max;
                while e > 0 && !stmt.is_char_boundary(e) {
                    e -= 1;
                }
                e
            };
            errors.push(format!("Error executing: {}... -> {e}", &stmt[..end]));
        }
    }

    if errors.is_empty() {
        tracing::info!(
            %connection_id,
            statements = statements.len(),
            "restore_database OK"
        );
        Ok(())
    } else {
        let msg = format!(
            "Partial restore failure ({}/{} statements failed):\n{}",
            errors.len(),
            statements.len(),
            errors.join("\n")
        );
        Err(CommandError::Internal(msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_backup_options_recognizes_aliases() {
        let opts = parse_backup_options(&[
            "schema-only".into(),
            "clean".into(),
            "create".into(),
        ]);
        assert_eq!(
            opts,
            BackupDumpOptions {
                schema_only: true,
                data_only: false,
                add_drop: true,
                add_create_db: true,
            }
        );
        let opts = parse_backup_options(&["no-data".into(), "no-create-info".into()]);
        assert!(opts.schema_only);
        assert!(opts.data_only);
    }

    #[test]
    fn validate_backup_filter_extension_accepts_sql_gz_dump() {
        assert_eq!(validate_backup_filter_extension("sql").unwrap(), "sql");
        assert_eq!(validate_backup_filter_extension(".GZ").unwrap(), "gz");
        assert!(validate_backup_filter_extension("exe").is_err());
    }

    #[test]
    fn backup_sql_header_includes_flags() {
        let header = backup_sql_header(
            "app",
            &BackupDumpOptions {
                schema_only: true,
                add_drop: true,
                ..Default::default()
            },
        );
        assert!(header.contains("-- DataZen backup: app"));
        assert!(header.contains("schema-only"));
        assert!(header.contains("add-drop-table"));
    }

    #[test]
    fn format_backup_value_covers_remaining_types() {
        use crate::db::Value;

        assert_eq!(format_backup_value(&Some(Value::Bool(false))), "FALSE");
        assert_eq!(
            format_backup_value(&Some(Value::Timestamp("2024-01-01".into()))),
            "'2024-01-01'"
        );
        assert_eq!(
            format_backup_value(&Some(Value::Json(serde_json::json!({"a":1})))),
            "'{\"a\":1}'"
        );
    }

    #[test]
    fn require_webdriver_path_ipc_gates_without_feature() {
        let result =
            require_webdriver_path_ipc("Direct path backup disabled; use backup_database_with_dialog");
        if cfg!(feature = "webdriver") {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("disabled"));
        }
    }

    #[test]
    fn split_restore_statements_skips_comment_only_segments() {
        let sql = "-- comment only;\nCREATE TABLE t (id INT);\nINSERT INTO t VALUES (1);";
        let stmts = split_restore_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "CREATE TABLE t (id INT)");
        assert_eq!(stmts[1], "INSERT INTO t VALUES (1)");
    }

    #[test]
    fn format_backup_value_covers_scalar_types() {
        use crate::db::Value;

        assert_eq!(format_backup_value(&None), "NULL");
        assert_eq!(format_backup_value(&Some(Value::Null)), "NULL");
        assert_eq!(format_backup_value(&Some(Value::Bool(true))), "TRUE");
        assert_eq!(format_backup_value(&Some(Value::Integer(42))), "42");
        assert_eq!(format_backup_value(&Some(Value::Float(1.5))), "1.5");
        assert_eq!(
            format_backup_value(&Some(Value::String("O'Brien".into()))),
            "'O''Brien'"
        );
        assert_eq!(
            format_backup_value(&Some(Value::Bytes(vec![0xde, 0xad]))),
            "'\\xdead'"
        );
    }

    #[tokio::test]
    async fn backup_and_restore_roundtrip() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("backup-cfg").await;
        let backup_path = test._temp.path().join("backup.sql");

        backup_database_to_path(
            &test.state,
            conn_id.clone(),
            Some("app".into()),
            backup_path.clone(),
            None,
            None,
        )
        .await
        .unwrap();

        let sql = std::fs::read_to_string(&backup_path).unwrap();
        assert!(sql.contains("INSERT INTO"));

        restore_database_from_path(&test.state, conn_id, backup_path)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn backup_database_errors_when_not_connected() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::with_tables().await;
        let path = test._temp.path().join("fail.sql");
        assert!(
            backup_database_to_path(
                &test.state,
                "missing".into(),
                None,
                path,
                None,
                None,
            )
            .await
            .is_err()
        );
    }
}
