use super::{AppState, log_err};
use crate::db::{DatabaseType, TableSchema};
use crate::store::SyncTask;
use chrono::Utc;
use tauri::{Emitter, State};

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
