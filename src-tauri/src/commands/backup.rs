use super::{AppState, log_err};
use tauri::State;

#[tauri::command]
pub async fn backup_database(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
    output_path: String,
    options: Option<Vec<String>>,
    compress: Option<bool>,
) -> Result<(), String> {
    tracing::info!(%connection_id, %output_path, "backup_database");
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .map_err(|e| log_err("backup_database", &e))?;

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("backup_database", &e))?;

    let db_name = database.as_deref().unwrap_or(config.database.as_deref().unwrap_or(""));
    let opts: std::collections::HashSet<String> = options.unwrap_or_default().into_iter().collect();
    let schema_only = opts.contains("schema-only") || opts.contains("no-data");
    let data_only = opts.contains("data-only") || opts.contains("no-create-info");
    let add_drop = opts.contains("clean") || opts.contains("add-drop-table");
    let add_create_db = opts.contains("create");

    let tables = driver
        .get_tables(&handle, db_name)
        .await
        .map_err(|e| log_err("backup_database", &e))?;

    let qi = |name: &str| driver.quote_ident(name);

    let mut out = String::new();
    out.push_str(&format!("-- DataZen backup: {}\n", db_name));
    out.push_str(&format!("-- Date: {}\n", chrono::Utc::now().to_rfc3339()));
    if !opts.is_empty() {
        out.push_str(&format!("-- Options: {}\n", opts.iter().cloned().collect::<Vec<_>>().join(", ")));
    }
    out.push('\n');

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
            .map_err(|e| log_err("backup_database", &e))?;

        out.push_str(&format!("-- Table: {}\n", tname));

        if add_drop {
            out.push_str(&format!("DROP TABLE IF EXISTS {};\n", qi(tname)));
        }

        if !data_only {
            let cols_sql: Vec<String> = schema.columns.iter().map(|c| {
                let mut def = format!("  {} {}", qi(&c.name), c.data_type);
                if !c.nullable { def.push_str(" NOT NULL"); }
                if let Some(ref dv) = c.default_value {
                    def.push_str(&format!(" DEFAULT {}", dv));
                }
                def
            }).collect();

            let mut create = format!("CREATE TABLE IF NOT EXISTS {} (\n{}", qi(tname), cols_sql.join(",\n"));
            if !schema.primary_keys.is_empty() {
                let pk_cols: Vec<String> = schema.primary_keys.iter().map(|k| qi(k)).collect();
                create.push_str(&format!(",\n  PRIMARY KEY ({})", pk_cols.join(", ")));
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
                        let vals: Vec<String> = row.iter().map(|v| match v {
                            None => "NULL".to_string(),
                            Some(crate::db::Value::Null) => "NULL".to_string(),
                            Some(crate::db::Value::Bool(b)) => if *b { "TRUE".to_string() } else { "FALSE".to_string() },
                            Some(crate::db::Value::Integer(n)) => n.to_string(),
                            Some(crate::db::Value::Float(f)) => f.to_string(),
                            Some(crate::db::Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
                            Some(crate::db::Value::Timestamp(s)) => format!("'{}'", s),
                            Some(crate::db::Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
                            Some(crate::db::Value::Bytes(b)) => format!("'\\x{}'", b.iter().map(|byte| format!("{:02x}", byte)).collect::<String>()),
                        }).collect();
                        out.push_str(&format!("INSERT INTO {} ({}) VALUES ({});\n",
                            qi(tname), col_names.join(", "), vals.join(", ")));
                    }
                    out.push('\n');
                }
                Err(e) => {
                    out.push_str(&format!("-- Error dumping data for {}: {}\n\n", tname, e));
                }
            }
        }
    }

    let data = out.as_bytes();
    if compress.unwrap_or(false) {
        use std::io::Write;
        let file = std::fs::File::create(&output_path)
            .map_err(|e| log_err("backup_database", &e))?;
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder.write_all(data).map_err(|e| log_err("backup_database", &e))?;
        encoder.finish().map_err(|e| log_err("backup_database", &e))?;
    } else {
        tokio::fs::write(&output_path, data)
            .await
            .map_err(|e| log_err("backup_database", &e))?;
    }
    tracing::info!(%output_path, "backup_database OK");
    Ok(())
}

#[tauri::command]
pub async fn restore_database(
    state: State<'_, AppState>,
    connection_id: String,
    input_path: String,
) -> Result<(), String> {
    tracing::info!(%connection_id, %input_path, "restore_database");
    let sql = tokio::fs::read_to_string(&input_path)
        .await
        .map_err(|e| log_err("restore_database", &e))?;

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("restore_database", &e))?;

    let statements: Vec<&str> = sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with("--"))
        .collect();

    let mut errors = Vec::new();
    for stmt in &statements {
        let full = format!("{};", stmt);
        if let Err(e) = driver.execute(&handle, &full).await {
            errors.push(format!("Error executing: {}... -> {}", &stmt[..stmt.len().min(80)], e));
        }
    }

    if errors.is_empty() {
        tracing::info!(%connection_id, statements = statements.len(), "restore_database OK");
        Ok(())
    } else {
        let msg = format!("部分语句执行失败 ({}/{}):\n{}", errors.len(), statements.len(), errors.join("\n"));
        Err(msg)
    }
}
