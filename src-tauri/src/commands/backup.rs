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

    let ext = filter_extension.trim_start_matches('.').to_lowercase();
    let allowed = ["sql", "gz", "dump"];
    if !allowed.contains(&ext.as_str()) {
        return Err(CommandError::Validation(format!(
            "File extension '.{ext}' not allowed"
        )));
    }

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
    state: &State<'_, AppState>,
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
    let opts: std::collections::HashSet<String> =
        options.unwrap_or_default().into_iter().collect();
    let schema_only = opts.contains("schema-only") || opts.contains("no-data");
    let data_only = opts.contains("data-only") || opts.contains("no-create-info");
    let add_drop = opts.contains("clean") || opts.contains("add-drop-table");
    let add_create_db = opts.contains("create");
    // Host-side CREATE DATABASE / \connect is dialect-mixed and unsafe across
    // engines. Refuse until backup moves into DatabaseDriver dump APIs.
    if add_create_db {
        return Err(CommandError::Validation(
            "Backup option 'create' is not supported; use a dump without create, or wait for driver-native backup".into(),
        ));
    }

    let tables = driver
        .get_tables(&handle, db_name)
        .await
        .cmd_err("backup_database")?;

    let qi = |name: &str| driver.quote_ident(name);

    let mut out = String::new();
    out.push_str(&format!("-- DataZen backup: {}\n", db_name));
    out.push_str(&format!("-- Date: {}\n", chrono::Utc::now().to_rfc3339()));
    if !opts.is_empty() {
        out.push_str(&format!(
            "-- Options: {}\n",
            opts.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    out.push('\n');

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
                        let vals: Vec<String> = row
                            .iter()
                            .map(|v| driver.format_sql_literal(v))
                            .collect();
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

async fn restore_database_from_path(
    state: &State<'_, AppState>,
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

    let statements: Vec<&str> = sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with("--"))
        .collect();

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
}
