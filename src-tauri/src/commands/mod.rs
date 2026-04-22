//! Tauri IPC command surface.

use crate::cache::SchemaCache;
use crate::db::registry::DriverRegistry;
use crate::db::{
    ConnectionConfig, DatabaseType, ExplainResult, MultiQueryResult, QueryResult, ServerInfo,
    TableDataResult, TableInfo, TableSchema,
};
use crate::services::{ConnectionManager, FilterCondition, OrderBy, QueryExecutor, SortCondition};
use crate::store::{AppSettings, QueryHistoryEntry, Store, SyncTask};
use std::sync::Arc;
use tauri::{Emitter, State};
use uuid::Uuid;
use std::path::PathBuf;
use chrono::Utc;

/// Shared application state injected into every command handler.
pub struct AppState {
    #[allow(dead_code)]
    pub driver_registry: Arc<DriverRegistry>,
    pub connection_manager: Arc<ConnectionManager>,
    pub store: Arc<Store>,
    pub schema_cache: Arc<SchemaCache>,
}

fn log_err(cmd: &str, e: &dyn std::fmt::Display) -> String {
    let msg = e.to_string();
    tracing::error!(cmd, error = %msg);
    msg
}

#[tauri::command]
pub async fn get_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionConfig>, String> {
    let list = state.store.get_connections().await;
    tracing::debug!(count = list.len(), "get_connections");
    Ok(list)
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<(), String> {
    tracing::info!(id = %config.id, name = %config.name, "save_connection");
    state
        .store
        .save_connection(config)
        .await
        .map_err(|e| log_err("save_connection", &e))
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), String> {
    tracing::info!(%id, "delete_connection");
    state
        .store
        .delete_connection(&id)
        .await
        .map_err(|e| log_err("delete_connection", &e))
}

#[tauri::command]
pub async fn get_groups(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.store.get_groups().await)
}

#[tauri::command]
pub async fn save_groups(state: State<'_, AppState>, groups: Vec<String>) -> Result<(), String> {
    tracing::info!(count = groups.len(), "save_groups");
    state
        .store
        .save_groups(groups)
        .await
        .map_err(|e| log_err("save_groups", &e))
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<ServerInfo, String> {
    tracing::info!(
        name = %config.name,
        host = ?config.host,
        port = ?config.port,
        db_type = ?config.database_type,
        ssh = ?config.ssh_tunnel.as_ref().map(|s| format!("{}@{}:{}", s.username, s.host, s.port)),
        "test_connection"
    );
    let result = state
        .connection_manager
        .test_connection(&config)
        .await
        .map_err(|e| log_err("test_connection", &e))?;
    tracing::info!(version = %result.server_version, "test_connection OK");
    Ok(result)
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, config_id: String) -> Result<String, String> {
    tracing::info!(%config_id, "connect");
    let conn_id = state
        .connection_manager
        .connect(&config_id)
        .await
        .map_err(|e| log_err("connect", &e))?;

    if let Some(mut cfg) = state.store.get_connection(&config_id).await {
        cfg.last_connected_at = Some(chrono::Utc::now().to_rfc3339());
        let _ = state.store.save_connection(cfg).await;
    }

    tracing::info!(%config_id, %conn_id, "connect OK");
    Ok(conn_id)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, connection_id: String) -> Result<(), String> {
    tracing::info!(%connection_id, "disconnect");
    state
        .connection_manager
        .disconnect(&connection_id)
        .await
        .map_err(|e| log_err("disconnect", &e))?;
    state.schema_cache.clear_connection(&connection_id).await;
    tracing::info!(%connection_id, "disconnect OK");
    Ok(())
}

#[tauri::command]
pub async fn get_connection_info(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<serde_json::Value, String> {
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .map_err(|e| log_err("get_connection_info", &e))?;

    let db_type = match config.database_type {
        DatabaseType::PostgreSQL => "postgresql",
        DatabaseType::MySQL => "mysql",
        DatabaseType::MariaDB => "mariadb",
        DatabaseType::SQLite => "sqlite",
        DatabaseType::Redis => "redis",
    };

    Ok(serde_json::json!({
        "databaseType": db_type,
        "name": config.name,
        "host": config.host,
        "port": config.port,
        "database": config.database,
    }))
}

#[tauri::command]
pub async fn get_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, String> {
    tracing::debug!(%connection_id, "get_databases");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_databases", &e))?;

    let dbs = driver
        .get_databases(&handle)
        .await
        .map_err(|e| log_err("get_databases", &e))?;
    tracing::debug!(%connection_id, count = dbs.len(), "get_databases OK");
    Ok(dbs)
}

#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<Vec<TableInfo>, String> {
    tracing::debug!(%connection_id, %database, "get_tables");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_tables", &e))?;

    let tables = driver
        .get_tables(&handle, &database)
        .await
        .map_err(|e| log_err("get_tables", &e))?;
    tracing::debug!(%connection_id, %database, count = tables.len(), "get_tables OK");
    Ok(tables)
}

#[tauri::command]
pub async fn get_table_schema(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
) -> Result<TableSchema, String> {
    tracing::debug!(%connection_id, %table, "get_table_schema");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_table_schema", &e))?;

    let schema = driver
        .get_table_schema(&handle, &table)
        .await
        .map_err(|e| log_err("get_table_schema", &e))?;
    tracing::debug!(%connection_id, %table, cols = schema.columns.len(), "get_table_schema OK");
    Ok(schema)
}

#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    page: u32,
    page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    sorts: Option<Vec<SortCondition>>,
) -> Result<TableDataResult, String> {
    tracing::debug!(%connection_id, %table, page, page_size, "get_table_data");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_table_data", &e))?;

    let order = sorts
        .and_then(|list| list.into_iter().next())
        .map(|s| OrderBy {
            column: s.column,
            descending: s.descending,
        });

    let executor = QueryExecutor::new(state.schema_cache.clone());
    let result = executor
        .get_table_data(
            &driver,
            &handle,
            &connection_id,
            "",
            &table,
            page,
            page_size,
            filters,
            order,
        )
        .await
        .map_err(|e| log_err("get_table_data", &e))?;
    tracing::debug!(%connection_id, %table, rows = result.rows.len(), "get_table_data OK");
    Ok(result)
}

#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
) -> Result<MultiQueryResult, String> {
    tracing::info!(%connection_id, sql_len = sql.len(), "execute_query");
    let settings = state.store.get_settings().await;
    let limit = if settings.limit_select_results && settings.query_result_limit > 0 {
        Some(settings.query_result_limit)
    } else {
        None
    };

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("execute_query", &e))?;

    match driver.query_multi(&handle, &sql, limit).await {
        Ok(result) => {
            tracing::info!(
                %connection_id,
                statements = result.results.len(),
                ms = result.total_time_ms,
                "execute_query OK"
            );
            let total_rows: u64 = result
                .results
                .iter()
                .filter_map(|r| r.rows_affected)
                .sum();
            let entry = QueryHistoryEntry {
                id: Uuid::new_v4().to_string(),
                connection_id: connection_id.clone(),
                database: String::new(),
                sql: sql.clone(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: result.total_time_ms,
                rows_affected: Some(total_rows),
                success: true,
                error_message: None,
            };
            let _ = state.store.add_query_history(entry).await;
            Ok(result)
        }
        Err(err) => {
            tracing::error!(%connection_id, error = %err, "execute_query failed");
            let entry = QueryHistoryEntry {
                id: Uuid::new_v4().to_string(),
                connection_id: connection_id.clone(),
                database: String::new(),
                sql: sql.clone(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: 0,
                rows_affected: None,
                success: false,
                error_message: Some(err.to_string()),
            };
            let _ = state.store.add_query_history(entry).await;
            Err(err.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_explain(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
) -> Result<ExplainResult, String> {
    tracing::debug!(%connection_id, "get_explain");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_explain", &e))?;

    driver
        .explain(&handle, &sql)
        .await
        .map_err(|e| log_err("get_explain", &e))
}

#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, connection_id: String) -> Result<(), String> {
    tracing::info!(%connection_id, "cancel_query");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("cancel_query", &e))?;

    driver
        .cancel_query(&handle)
        .await
        .map_err(|e| log_err("cancel_query", &e))
}

#[tauri::command]
pub async fn get_query_history(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<QueryHistoryEntry>, String> {
    Ok(state.store.get_query_history(limit).await)
}

#[tauri::command]
pub async fn clear_query_history(state: State<'_, AppState>) -> Result<(), String> {
    tracing::info!("clear_query_history");
    state
        .store
        .clear_query_history()
        .await
        .map_err(|e| log_err("clear_query_history", &e))
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.store.get_settings().await)
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    tracing::debug!(theme = %settings.theme, "save_settings");
    state
        .store
        .save_settings(settings)
        .await
        .map_err(|e| log_err("save_settings", &e))
}

#[tauri::command]
pub async fn write_file(path: String, contents: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    tokio::fs::write(&p, contents.as_bytes())
        .await
        .map_err(|e| log_err("write_file", &e))
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    tokio::fs::read_to_string(&p)
        .await
        .map_err(|e| log_err("read_file", &e))
}

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

/// Compare two databases for data sync.
#[tauri::command]
pub async fn compare_databases(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    tracing::info!(%source_connection_id, %target_connection_id, "compare_databases");

    let src_config = state.connection_manager
        .get_connection_config(&source_connection_id).await
        .map_err(|e| log_err("compare_databases", &e))?;
    let tgt_config = state.connection_manager
        .get_connection_config(&target_connection_id).await
        .map_err(|e| log_err("compare_databases", &e))?;

    let (src_driver, src_handle) = state.connection_manager
        .get_connection(&source_connection_id).await
        .map_err(|e| log_err("compare_databases", &e))?;
    let (tgt_driver, tgt_handle) = state.connection_manager
        .get_connection(&target_connection_id).await
        .map_err(|e| log_err("compare_databases", &e))?;

    let src_db = src_config.database.as_deref().unwrap_or("");
    let tgt_db = tgt_config.database.as_deref().unwrap_or("");

    let src_tables = src_driver.get_tables(&src_handle, src_db).await
        .map_err(|e| log_err("compare_databases", &e))?;
    let tgt_tables = tgt_driver.get_tables(&tgt_handle, tgt_db).await
        .map_err(|e| log_err("compare_databases", &e))?;

    let src_names: std::collections::HashSet<String> = src_tables.iter().map(|t| t.name.clone()).collect();
    let tgt_names: std::collections::HashSet<String> = tgt_tables.iter().map(|t| t.name.clone()).collect();

    let mut results = Vec::new();

    for t in &src_tables {
        let in_target = tgt_names.contains(&t.name);
        let mut status = if in_target { "identical" } else { "source_only" };

        if in_target {
            let src_schema = src_driver.get_table_schema(&src_handle, &t.name).await
                .map_err(|e| log_err("compare_databases", &e))?;
            let tgt_schema = tgt_driver.get_table_schema(&tgt_handle, &t.name).await
                .map_err(|e| log_err("compare_databases", &e))?;

            let src_cols: Vec<(&str, &str)> = src_schema.columns.iter()
                .map(|c| (c.name.as_str(), c.data_type.as_str())).collect();
            let tgt_cols: Vec<(&str, &str)> = tgt_schema.columns.iter()
                .map(|c| (c.name.as_str(), c.data_type.as_str())).collect();

            if src_cols != tgt_cols {
                status = "different";
            } else {
                let src_count = t.row_count.unwrap_or(-1);
                let tgt_count = tgt_tables.iter().find(|x| x.name == t.name)
                    .and_then(|x| x.row_count).unwrap_or(-1);
                if src_count != tgt_count { status = "different"; }
            }
        }

        results.push(serde_json::json!({
            "table": t.name,
            "status": status,
            "sourceRows": t.row_count,
            "targetRows": if in_target {
                tgt_tables.iter().find(|x| x.name == t.name).and_then(|x| x.row_count)
            } else { None },
        }));
    }

    for t in &tgt_tables {
        if !src_names.contains(&t.name) {
            results.push(serde_json::json!({
                "table": t.name,
                "status": "target_only",
                "sourceRows": null,
                "targetRows": t.row_count,
            }));
        }
    }

    tracing::info!(tables = results.len(), "compare_databases OK");
    Ok(results)
}

// ── Cross-database type mapping ─────────────────────────────────────

/// Map a PostgreSQL full type (from format_type()) to MySQL equivalent.
fn pg_type_to_mysql(pg_type: &str) -> String {
    let t = pg_type.trim();
    let lower = t.to_lowercase();

    // Array types → JSON
    if lower.ends_with("[]") || lower == "array" {
        return "JSON".to_string();
    }

    // character varying(N) → VARCHAR(N); bare → VARCHAR(255)
    if lower.starts_with("character varying") {
        return if let Some(rest) = lower.strip_prefix("character varying") {
            let rest = rest.trim();
            if rest.starts_with('(') { format!("VARCHAR{rest}") } else { "VARCHAR(255)".to_string() }
        } else { "VARCHAR(255)".to_string() };
    }
    // character(N) → CHAR(N)
    if lower.starts_with("character(") {
        return lower.replace("character", "CHAR");
    }
    if lower == "character" { return "CHAR(1)".to_string(); }

    // numeric(p,s) → DECIMAL(p,s); bare → DECIMAL(65,30)
    if lower.starts_with("numeric") {
        return if let Some(rest) = lower.strip_prefix("numeric") {
            let rest = rest.trim();
            if rest.starts_with('(') { format!("DECIMAL{rest}") } else { "DECIMAL(65,30)".to_string() }
        } else { "DECIMAL(65,30)".to_string() };
    }

    match lower.as_str() {
        "integer" | "int" | "int4" => "INT".into(),
        "bigint" | "int8" => "BIGINT".into(),
        "smallint" | "int2" => "SMALLINT".into(),
        "text" => "TEXT".into(),
        "boolean" | "bool" => "TINYINT(1)".into(),
        "real" | "float4" => "FLOAT".into(),
        "double precision" | "float8" => "DOUBLE".into(),
        "bytea" => "LONGBLOB".into(),
        "json" => "JSON".into(),
        "jsonb" => "JSON".into(),
        "uuid" => "CHAR(36)".into(),
        "date" => "DATE".into(),
        "time without time zone" | "time" => "TIME".into(),
        "time with time zone" | "timetz" => "TIME".into(),
        "timestamp without time zone" | "timestamp" => "DATETIME".into(),
        "timestamp with time zone" | "timestamptz" => "DATETIME".into(),
        "inet" => "VARCHAR(45)".into(),
        "cidr" => "VARCHAR(43)".into(),
        "macaddr" | "macaddr8" => "VARCHAR(17)".into(),
        "interval" => "VARCHAR(255)".into(),
        "money" => "DECIMAL(19,2)".into(),
        "oid" => "INT UNSIGNED".into(),
        "xml" => "TEXT".into(),
        "bit" => "BIT(1)".into(),
        _ => {
            if lower.starts_with("bit(") { return lower.replace("bit", "BIT"); }
            if lower.starts_with("bit varying") { return "BLOB".into(); }
            // Fallback: use source type as-is
            t.to_string()
        }
    }
}

/// Map a MySQL full type (COLUMN_TYPE) to PostgreSQL equivalent.
fn mysql_type_to_pg(mysql_type: &str) -> String {
    let t = mysql_type.trim();
    let lower = t.to_lowercase();
    let base = lower.replace(" unsigned", "").replace(" zerofill", "");

    // tinyint(1) is MySQL's boolean
    if base.starts_with("tinyint(1)") { return "boolean".into(); }

    if base.starts_with("varchar") { return base.replace("varchar", "character varying"); }
    if base.starts_with("char(") { return base.replace("char", "character"); }
    if base.starts_with("decimal") { return base.replace("decimal", "numeric"); }
    if base.starts_with("tinyint") { return "smallint".into(); }
    if base.starts_with("mediumint") { return "integer".into(); }
    if base.starts_with("int(") || base == "int" { return "integer".into(); }
    if base.starts_with("bigint") { return "bigint".into(); }
    if base.starts_with("smallint") { return "smallint".into(); }

    match base.as_str() {
        "float" => "real".into(),
        "double" => "double precision".into(),
        "datetime" | "timestamp" => "timestamp".into(),
        "date" => "date".into(),
        "time" => "time".into(),
        "year" => "smallint".into(),
        "text" | "longtext" | "mediumtext" | "tinytext" => "text".into(),
        "blob" | "longblob" | "mediumblob" | "tinyblob" | "binary" | "varbinary" => "bytea".into(),
        "json" => "jsonb".into(),
        "enum" => "text".into(),
        "set" => "text".into(),
        "bit" | "bit(1)" => "boolean".into(),
        _ => {
            if base.starts_with("enum(") || base.starts_with("set(") { return "text".into(); }
            if base.starts_with("varbinary") { return "bytea".into(); }
            if base.starts_with("binary") { return "bytea".into(); }
            t.to_string()
        }
    }
}

/// Translate a column default value for cross-database sync.
fn map_default_for_target(
    default: &str,
    src_type: &DatabaseType,
    tgt_type: &DatabaseType,
) -> Option<String> {
    let d = default.trim();

    match (src_type, tgt_type) {
        (DatabaseType::PostgreSQL, DatabaseType::MySQL | DatabaseType::MariaDB) => {
            // Strip PG sequence defaults — MySQL uses AUTO_INCREMENT instead
            if d.contains("nextval(") { return None; }
            // PG casts like 'val'::text or 'val'::character varying
            if d.contains("::") {
                let stripped = d.split("::").next().unwrap_or(d);
                return Some(stripped.to_string());
            }
            // now() → CURRENT_TIMESTAMP
            if d == "now()" || d == "CURRENT_TIMESTAMP" { return Some("CURRENT_TIMESTAMP".into()); }
            Some(d.to_string())
        }
        (DatabaseType::MySQL | DatabaseType::MariaDB, DatabaseType::PostgreSQL) => {
            if d == "CURRENT_TIMESTAMP" { return Some("now()".into()); }
            Some(d.to_string())
        }
        _ => Some(d.to_string()),
    }
}

/// Query full column types with precision from PostgreSQL using format_type().
async fn pg_full_column_types(
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    table: &str,
) -> Result<std::collections::HashMap<String, String>, String> {
    let sql = format!(
        r#"SELECT a.attname::text AS col_name,
                  format_type(a.atttypid, a.atttypmod) AS full_type
           FROM pg_attribute a
           WHERE a.attrelid = '{}'::regclass
             AND a.attnum > 0
             AND NOT a.attisdropped
           ORDER BY a.attnum"#,
        table.replace('\'', "''")
    );
    let result = driver.query(handle, &sql).await
        .map_err(|e| format!("pg_full_column_types: {e}"))?;
    let mut map = std::collections::HashMap::new();
    for row in &result.rows {
        if let (Some(Some(crate::db::Value::String(name))), Some(Some(crate::db::Value::String(ft)))) =
            (row.get(0), row.get(1))
        {
            map.insert(name.clone(), ft.clone());
        }
    }
    Ok(map)
}

/// Progress event emitted during sync.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncProgressEvent {
    task_id: String,
    phase: String,
    table_index: usize,
    total_tables: usize,
    current_table: String,
    source_row_count: u64,
    synced_rows: u64,
    completed_tables: Vec<String>,
    error: Option<String>,
}

const BATCH_SIZE: usize = 500;

/// Format a row value as a SQL literal for the target database.
fn format_value(v: &Option<crate::db::Value>, is_mysql_target: bool) -> String {
    match v {
        None | Some(crate::db::Value::Null) => "NULL".into(),
        Some(crate::db::Value::Bool(b)) => {
            if is_mysql_target {
                if *b { "1".into() } else { "0".into() }
            } else if *b { "TRUE".into() } else { "FALSE".into() }
        }
        Some(crate::db::Value::Integer(n)) => n.to_string(),
        Some(crate::db::Value::Float(f)) => f.to_string(),
        Some(crate::db::Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
        Some(crate::db::Value::Timestamp(s)) => format!("'{}'", s),
        Some(crate::db::Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
        Some(crate::db::Value::Bytes(b)) => {
            if is_mysql_target {
                format!("X'{}'", b.iter().map(|byte| format!("{:02x}", byte)).collect::<String>())
            } else {
                format!("'\\x{}'", b.iter().map(|byte| format!("{:02x}", byte)).collect::<String>())
            }
        }
    }
}

/// Build a CREATE TABLE DDL for the target database.
fn build_create_table_ddl(
    table_name: &str,
    src_schema: &TableSchema,
    src_type: &DatabaseType,
    tgt_type: &DatabaseType,
    cross_db: bool,
    pg_full_types: &Option<std::collections::HashMap<String, String>>,
    q: &dyn Fn(&str) -> String,
) -> String {
    let cols_sql: Vec<String> = src_schema.columns.iter().map(|c| {
        let target_type = if cross_db {
            let full_src_type = pg_full_types.as_ref()
                .and_then(|m| m.get(&c.name))
                .map(|s| s.as_str())
                .unwrap_or(&c.data_type);
            match (src_type, tgt_type) {
                (DatabaseType::PostgreSQL, DatabaseType::MySQL | DatabaseType::MariaDB) => pg_type_to_mysql(full_src_type),
                (DatabaseType::MySQL | DatabaseType::MariaDB, DatabaseType::PostgreSQL) => mysql_type_to_pg(&c.data_type),
                _ => c.data_type.clone(),
            }
        } else {
            c.data_type.clone()
        };
        let mut def = format!("  {} {}", q(&c.name), target_type);
        if !c.nullable { def.push_str(" NOT NULL"); }
        if let Some(ref dv) = c.default_value {
            if cross_db {
                if let Some(mapped) = map_default_for_target(dv, src_type, tgt_type) {
                    def.push_str(&format!(" DEFAULT {}", mapped));
                }
            } else {
                def.push_str(&format!(" DEFAULT {}", dv));
            }
        }
        def
    }).collect();

    let mut ddl = format!("CREATE TABLE {} (\n{}", q(table_name), cols_sql.join(",\n"));
    if !src_schema.primary_keys.is_empty() {
        let pk_cols: Vec<String> = src_schema.primary_keys.iter().map(|k| q(k)).collect();
        ddl.push_str(&format!(",\n  PRIMARY KEY ({})", pk_cols.join(", ")));
    }
    ddl.push_str("\n)");
    ddl
}

/// Sync a single table from source to target (drop+recreate+insert).
/// Kept for backward compatibility with existing E2E tests.
#[tauri::command]
pub async fn sync_table(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<u64, String> {
    tracing::info!(%source_connection_id, %target_connection_id, %table_name, "sync_table");

    let src_config = state.connection_manager
        .get_connection_config(&source_connection_id).await
        .map_err(|e| log_err("sync_table", &e))?;
    let tgt_config = state.connection_manager
        .get_connection_config(&target_connection_id).await
        .map_err(|e| log_err("sync_table", &e))?;

    let src_type = &src_config.database_type;
    let tgt_type = &tgt_config.database_type;
    let cross_db = src_type != tgt_type;

    let (src_driver, src_handle) = state.connection_manager
        .get_connection(&source_connection_id).await
        .map_err(|e| log_err("sync_table", &e))?;
    let (tgt_driver, tgt_handle) = state.connection_manager
        .get_connection(&target_connection_id).await
        .map_err(|e| log_err("sync_table", &e))?;

    let q = |name: &str| tgt_driver.quote_ident(name);
    let sq = |name: &str| src_driver.quote_ident(name);

    let src_schema = src_driver.get_table_schema(&src_handle, &table_name).await
        .map_err(|e| log_err("sync_table", &e))?;

    let pg_full_types = if cross_db && matches!(src_type, DatabaseType::PostgreSQL) {
        Some(pg_full_column_types(src_driver.as_ref(), &src_handle, &table_name).await?)
    } else { None };

    tgt_driver.execute(&tgt_handle, &format!("DROP TABLE IF EXISTS {}", q(&table_name))).await
        .map_err(|e| log_err("sync_table", &e))?;

    let create_ddl = build_create_table_ddl(&table_name, &src_schema, src_type, tgt_type, cross_db, &pg_full_types, &q);
    tgt_driver.execute(&tgt_handle, &create_ddl).await
        .map_err(|e| log_err("sync_table", &e))?;

    let src_col_names: Vec<String> = src_schema.columns.iter().map(|c| sq(&c.name)).collect();
    let tgt_col_names: Vec<String> = src_schema.columns.iter().map(|c| q(&c.name)).collect();
    let select_sql = format!("SELECT {} FROM {}", src_col_names.join(", "), sq(&table_name));
    let result = src_driver.query(&src_handle, &select_sql).await
        .map_err(|e| log_err("sync_table", &e))?;

    let is_mysql_target = matches!(tgt_type, DatabaseType::MySQL | DatabaseType::MariaDB);
    let cols_joined = tgt_col_names.join(", ");
    let mut total_rows: u64 = 0;

    for batch in result.rows.chunks(BATCH_SIZE) {
        let value_sets: Vec<String> = batch.iter().map(|row| {
            let vals: Vec<String> = row.iter().map(|v| format_value(v, is_mysql_target)).collect();
            format!("({})", vals.join(", "))
        }).collect();
        let insert = format!("INSERT INTO {} ({}) VALUES {}", q(&table_name), cols_joined, value_sets.join(", "));
        tgt_driver.execute(&tgt_handle, &insert).await
            .map_err(|e| log_err("sync_table", &e))?;
        total_rows += batch.len() as u64;
    }

    tracing::info!(%table_name, total_rows, "sync_table OK");
    Ok(total_rows)
}

/// Count rows in a table on a given connection.
async fn count_rows(
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    table: &str,
) -> Result<u64, String> {
    let sql = format!("SELECT COUNT(*) FROM {}", driver.quote_ident(table));
    let res = driver.query(handle, &sql).await.map_err(|e| e.to_string())?;
    if let Some(row) = res.rows.first() {
        if let Some(Some(crate::db::Value::Integer(n))) = row.first() {
            return Ok(*n as u64);
        }
    }
    Ok(0)
}

/// Sync multiple tables with progress events and checkpoint support.
#[tauri::command]
pub async fn sync_tables(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    task_id: String,
    source_connection_id: String,
    target_connection_id: String,
    source_config_id: String,
    target_config_id: String,
    tables: Vec<String>,
    skip_tables: Vec<String>,
    strategy: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(%task_id, table_count = tables.len(), %strategy, "sync_tables");

    let src_config = state.connection_manager
        .get_connection_config(&source_connection_id).await
        .map_err(|e| log_err("sync_tables", &e))?;
    let tgt_config = state.connection_manager
        .get_connection_config(&target_connection_id).await
        .map_err(|e| log_err("sync_tables", &e))?;

    let src_type = src_config.database_type.clone();
    let tgt_type = tgt_config.database_type.clone();
    let cross_db = src_type != tgt_type;

    let is_mysql_target = matches!(tgt_type, DatabaseType::MySQL | DatabaseType::MariaDB);

    let emit = |evt: SyncProgressEvent| { let _ = app_handle.emit("sync:progress", &evt); };

    let mut completed: Vec<String> = skip_tables.clone();
    let mut source_row_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let total_tables = tables.len();

    // Phase 1: count source rows for all tables
    emit(SyncProgressEvent {
        task_id: task_id.clone(), phase: "counting".into(),
        table_index: 0, total_tables, current_table: String::new(),
        source_row_count: 0, synced_rows: 0, completed_tables: completed.clone(),
        error: None,
    });

    {
        let (src_driver, src_handle) = state.connection_manager
            .get_connection(&source_connection_id).await
            .map_err(|e| log_err("sync_tables", &e))?;
        for t in &tables {
            let cnt = count_rows(src_driver.as_ref(), &src_handle, t).await?;
            source_row_counts.insert(t.clone(), cnt);
        }
    }

    // Save initial task state
    let mut task = SyncTask {
        id: task_id.clone(),
        source_connection_id: source_connection_id.clone(),
        target_connection_id: target_connection_id.clone(),
        source_config_id: source_config_id.clone(),
        target_config_id: target_config_id.clone(),
        tables: tables.clone(),
        completed_tables: completed.clone(),
        current_table: None,
        current_table_offset: 0,
        source_row_counts: source_row_counts.clone(),
        strategy: strategy.clone(),
        status: "running".into(),
        error_message: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    state.store.save_sync_task(task.clone()).await.map_err(|e| log_err("sync_tables", &e))?;

    // Phase 2: sync each table
    for (idx, table_name) in tables.iter().enumerate() {
        if completed.contains(table_name) {
            continue;
        }

        let src_rows = source_row_counts.get(table_name).copied().unwrap_or(0);

        emit(SyncProgressEvent {
            task_id: task_id.clone(), phase: "syncing".into(),
            table_index: idx, total_tables, current_table: table_name.clone(),
            source_row_count: src_rows, synced_rows: 0,
            completed_tables: completed.clone(), error: None,
        });

        // Update task checkpoint
        task.current_table = Some(table_name.clone());
        task.current_table_offset = 0;
        task.updated_at = Utc::now();
        state.store.save_sync_task(task.clone()).await.map_err(|e| log_err("sync_tables", &e))?;

        let sync_result: Result<u64, String> = async {
            let (src_driver, src_handle) = state.connection_manager
                .get_connection(&source_connection_id).await
                .map_err(|e| log_err("sync_tables", &e))?;
            let (tgt_driver, tgt_handle) = state.connection_manager
                .get_connection(&target_connection_id).await
                .map_err(|e| log_err("sync_tables", &e))?;

            let q = |name: &str| tgt_driver.quote_ident(name);
            let sq = |name: &str| src_driver.quote_ident(name);

            let src_schema = src_driver.get_table_schema(&src_handle, table_name).await
                .map_err(|e| log_err("sync_tables", &e))?;

            let pg_full_types = if cross_db && matches!(src_type, DatabaseType::PostgreSQL) {
                Some(pg_full_column_types(src_driver.as_ref(), &src_handle, table_name).await?)
            } else { None };

            // Drop + Create
            tgt_driver.execute(&tgt_handle, &format!("DROP TABLE IF EXISTS {}", q(table_name))).await
                .map_err(|e| log_err("sync_tables", &e))?;
            let create_ddl = build_create_table_ddl(table_name, &src_schema, &src_type, &tgt_type, cross_db, &pg_full_types, &q);
            tgt_driver.execute(&tgt_handle, &create_ddl).await
                .map_err(|e| log_err("sync_tables", &e))?;

            // SELECT all rows from source
            let src_col_names: Vec<String> = src_schema.columns.iter().map(|c| sq(&c.name)).collect();
            let tgt_col_names: Vec<String> = src_schema.columns.iter().map(|c| q(&c.name)).collect();
            let select_sql = format!("SELECT {} FROM {}", src_col_names.join(", "), sq(table_name));
            let result = src_driver.query(&src_handle, &select_sql).await
                .map_err(|e| log_err("sync_tables", &e))?;

            let cols_joined = tgt_col_names.join(", ");
            let mut synced: u64 = 0;

            // Batch insert
            for batch in result.rows.chunks(BATCH_SIZE) {
                let value_sets: Vec<String> = batch.iter().map(|row| {
                    let vals: Vec<String> = row.iter().map(|v| format_value(v, is_mysql_target)).collect();
                    format!("({})", vals.join(", "))
                }).collect();
                let insert = format!("INSERT INTO {} ({}) VALUES {}",
                    q(table_name), cols_joined, value_sets.join(", "));
                tgt_driver.execute(&tgt_handle, &insert).await
                    .map_err(|e| log_err("sync_tables", &e))?;

                synced += batch.len() as u64;

                // Emit row-level progress
                emit(SyncProgressEvent {
                    task_id: task_id.clone(), phase: "syncing".into(),
                    table_index: idx, total_tables, current_table: table_name.clone(),
                    source_row_count: src_rows, synced_rows: synced,
                    completed_tables: completed.clone(), error: None,
                });

                // Save checkpoint
                task.current_table_offset = synced;
                task.updated_at = Utc::now();
                state.store.save_sync_task(task.clone()).await.map_err(|e| log_err("sync_tables", &e))?;
            }

            Ok(synced)
        }.await;

        match sync_result {
            Ok(_rows) => {
                completed.push(table_name.clone());
                task.completed_tables = completed.clone();
                task.current_table = None;
                task.current_table_offset = 0;
                task.updated_at = Utc::now();
                state.store.save_sync_task(task.clone()).await.map_err(|e| log_err("sync_tables", &e))?;

                emit(SyncProgressEvent {
                    task_id: task_id.clone(), phase: "table_done".into(),
                    table_index: idx, total_tables, current_table: table_name.clone(),
                    source_row_count: src_rows, synced_rows: src_rows,
                    completed_tables: completed.clone(), error: None,
                });
            }
            Err(err) => {
                task.status = "failed".into();
                task.error_message = Some(err.clone());
                task.updated_at = Utc::now();
                state.store.save_sync_task(task.clone()).await.map_err(|e| log_err("sync_tables", &e))?;

                emit(SyncProgressEvent {
                    task_id: task_id.clone(), phase: "error".into(),
                    table_index: idx, total_tables, current_table: table_name.clone(),
                    source_row_count: src_rows, synced_rows: 0,
                    completed_tables: completed.clone(), error: Some(err.clone()),
                });

                return Err(err);
            }
        }
    }

    // Done
    task.status = "completed".into();
    task.current_table = None;
    task.updated_at = Utc::now();
    state.store.save_sync_task(task.clone()).await.map_err(|e| log_err("sync_tables", &e))?;

    emit(SyncProgressEvent {
        task_id: task_id.clone(), phase: "done".into(),
        table_index: total_tables, total_tables, current_table: String::new(),
        source_row_count: 0, synced_rows: 0,
        completed_tables: completed.clone(), error: None,
    });

    Ok(serde_json::json!({
        "taskId": task_id,
        "completedTables": completed,
        "totalTables": total_tables,
    }))
}

/// Get all saved sync tasks.
#[tauri::command]
pub async fn get_sync_tasks(state: State<'_, AppState>) -> Result<Vec<SyncTask>, String> {
    Ok(state.store.get_sync_tasks().await)
}

/// Save a sync task directly (used for resume/testing).
#[tauri::command]
pub async fn save_sync_task_direct(state: State<'_, AppState>, task: SyncTask) -> Result<(), String> {
    state.store.save_sync_task(task).await
        .map_err(|e| log_err("save_sync_task_direct", &e))
}

/// Delete a sync task.
#[tauri::command]
pub async fn delete_sync_task(state: State<'_, AppState>, task_id: String) -> Result<(), String> {
    state.store.delete_sync_task(&task_id).await
        .map_err(|e| log_err("delete_sync_task", &e))
}

/// Check if source data has changed since the task was created.
#[tauri::command]
pub async fn check_sync_conflicts(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<serde_json::Value, String> {
    let tasks = state.store.get_sync_tasks().await;
    let task = tasks.iter().find(|t| t.id == task_id)
        .ok_or_else(|| "Sync task not found".to_string())?;

    let (src_driver, src_handle) = state.connection_manager
        .get_connection(&task.source_connection_id).await
        .map_err(|e| log_err("check_sync_conflicts", &e))?;

    let mut conflicts = Vec::<serde_json::Value>::new();

    for table in &task.tables {
        if task.completed_tables.contains(table) { continue; }

        let original_count = task.source_row_counts.get(table).copied().unwrap_or(0);
        let current_count = count_rows(src_driver.as_ref(), &src_handle, table).await?;

        if current_count != original_count {
            conflicts.push(serde_json::json!({
                "table": table,
                "originalRows": original_count,
                "currentRows": current_count,
            }));
        }
    }

    Ok(serde_json::json!({
        "hasConflicts": !conflicts.is_empty(),
        "conflicts": conflicts,
    }))
}
