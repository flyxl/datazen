use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{DatabaseType, TableSchema, Value};
use crate::schema_diff::diff_table_schemas;
use crate::store::SyncTask;
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::sync::ddl::build_create_table_ddl;
use chrono::Utc;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tauri::{Emitter, State};

const DATA_COMPARE_SAMPLE_LIMIT: usize = 1000;
const DATA_COMPARE_MISMATCH_LIMIT: usize = 50;

/// Compare two databases for data sync.
pub(crate) async fn compare_databases_impl(
    state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
) -> Result<Vec<serde_json::Value>, CommandError> {
    tracing::info!(%source_connection_id, %target_connection_id, "compare_databases");

    let src_config = state.connection_manager
        .get_connection_config(&source_connection_id).await
        .cmd_err("compare_databases")?;
    let tgt_config = state.connection_manager
        .get_connection_config(&target_connection_id).await
        .cmd_err("compare_databases")?;

    let (src_driver, src_handle) = state.connection_manager
        .get_connection(&source_connection_id).await
        .cmd_err("compare_databases")?;
    let (tgt_driver, tgt_handle) = state.connection_manager
        .get_connection(&target_connection_id).await
        .cmd_err("compare_databases")?;

    let src_db = src_config.database.as_deref().unwrap_or("");
    let tgt_db = tgt_config.database.as_deref().unwrap_or("");

    let src_tables = src_driver.get_tables(&src_handle, src_db).await
        .cmd_err("compare_databases")?;
    let tgt_tables = tgt_driver.get_tables(&tgt_handle, tgt_db).await
        .cmd_err("compare_databases")?;

    let src_names: std::collections::HashSet<String> = src_tables.iter().map(|t| t.name.clone()).collect();
    let tgt_names: std::collections::HashSet<String> = tgt_tables.iter().map(|t| t.name.clone()).collect();

    let mut results = Vec::new();

    for t in &src_tables {
        let in_target = tgt_names.contains(&t.name);
        let mut status = if in_target { "identical" } else { "source_only" };

        let mut source_rows: Option<u64> = None;
        let mut target_rows: Option<u64> = None;

        if in_target {
            let src_schema = src_driver.get_table_schema(&src_handle, &t.name).await
                .cmd_err("compare_databases")?;
            let tgt_schema = tgt_driver.get_table_schema(&tgt_handle, &t.name).await
                .cmd_err("compare_databases")?;

            let src_cols: Vec<(&str, &str)> = src_schema.columns.iter()
                .map(|c| (c.name.as_str(), c.data_type.as_str())).collect();
            let tgt_cols: Vec<(&str, &str)> = tgt_schema.columns.iter()
                .map(|c| (c.name.as_str(), c.data_type.as_str())).collect();

            if src_cols != tgt_cols {
                status = "different";
            } else {
                let src_count = count_rows(src_driver.as_ref(), &src_handle, &t.name).await?;
                let tgt_count = count_rows(tgt_driver.as_ref(), &tgt_handle, &t.name).await?;
                source_rows = Some(src_count);
                target_rows = Some(tgt_count);
                if src_count != tgt_count {
                    status = "different";
                }
            }
        }

        results.push(serde_json::json!({
            "table": t.name,
            "status": status,
            "sourceRows": source_rows.or_else(|| t.row_count.map(|n| n as u64)),
            "targetRows": target_rows.or_else(|| {
                tgt_tables.iter().find(|x| x.name == t.name)
                    .and_then(|x| x.row_count.map(|n| n as u64))
            }),
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

#[tauri::command]
pub async fn compare_databases(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
) -> Result<Vec<serde_json::Value>, CommandError> {
    compare_databases_impl(&state, source_connection_id, target_connection_id).await
}

/// Compare column-level schema differences for a single table.
pub(crate) async fn compare_table_schemas_impl(
    state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    tracing::info!(%source_connection_id, %target_connection_id, %table_name, "compare_table_schemas");

    let src_config = state.connection_manager
        .get_connection_config(&source_connection_id).await
        .cmd_err("compare_table_schemas")?;
    let tgt_config = state.connection_manager
        .get_connection_config(&target_connection_id).await
        .cmd_err("compare_table_schemas")?;

    let (src_driver, src_handle) = state.connection_manager
        .get_connection(&source_connection_id).await
        .cmd_err("compare_table_schemas")?;
    let (tgt_driver, tgt_handle) = state.connection_manager
        .get_connection(&target_connection_id).await
        .cmd_err("compare_table_schemas")?;

    let src_schema = src_driver.get_table_schema(&src_handle, &table_name).await
        .cmd_err("compare_table_schemas")?;
    let tgt_schema = tgt_driver.get_table_schema(&tgt_handle, &table_name).await
        .cmd_err("compare_table_schemas")?;

    // Source = desired: missingOnTarget → ADD, extraOnTarget → DROP.
    // `added`/`removed` kept as aliases for one release.
    let diff = diff_table_schemas(&table_name, &src_schema, &tgt_schema);

    let mut result = serde_json::json!({
        "table": table_name,
        "missingOnTarget": diff.missing_on_target,
        "extraOnTarget": diff.extra_on_target,
        "added": diff.added,
        "removed": diff.removed,
        "changed": diff.changed,
    });

    if state.sync_adapters
        .ensure_pair(&src_config.database_type, &tgt_config.database_type)
        .is_ok()
    {
        let src_source = state.sync_adapters.get_source(&src_config.database_type);
        let tgt_source = state.sync_adapters.get_source(&tgt_config.database_type);
        let src_target = state.sync_adapters.get_target(&src_config.database_type);
        let tgt_target = state.sync_adapters.get_target(&tgt_config.database_type);

        if let (Some(src_adapter), Some(tgt_src_adapter), Some(src_tgt_adapter), Some(tgt_adapter)) =
            (src_source, tgt_source, src_target, tgt_target)
        {
            let src_full_types = if src_config.database_type == "postgresql" {
                pg_full_column_types(src_driver.as_ref(), &src_handle, &table_name).await.ok()
            } else {
                None
            };
            let tgt_full_types = if tgt_config.database_type == "postgresql" {
                pg_full_column_types(tgt_driver.as_ref(), &tgt_handle, &table_name).await.ok()
            } else {
                None
            };

            let src_ir = src_adapter.table_to_ir(&src_schema, src_full_types.as_ref());
            let tgt_ir = tgt_src_adapter.table_to_ir(&tgt_schema, tgt_full_types.as_ref());
            result["sourceDdl"] =
                serde_json::Value::String(build_create_table_ddl(&src_ir, src_tgt_adapter.as_ref()));
            result["targetDdl"] =
                serde_json::Value::String(build_create_table_ddl(&tgt_ir, tgt_adapter.as_ref()));
        }
    }

    tracing::info!(%table_name, "compare_table_schemas OK");
    Ok(result)
}

#[tauri::command]
pub async fn compare_table_schemas(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    compare_table_schemas_impl(&state, source_connection_id, target_connection_id, table_name).await
}

/// Sample row-level data differences for a single table.
pub(crate) async fn compare_table_data_impl(
    state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    tracing::info!(%source_connection_id, %target_connection_id, %table_name, "compare_table_data");

    let (src_driver, src_handle) = state.connection_manager
        .get_connection(&source_connection_id).await
        .cmd_err("compare_table_data")?;
    let (tgt_driver, tgt_handle) = state.connection_manager
        .get_connection(&target_connection_id).await
        .cmd_err("compare_table_data")?;

    let src_schema = src_driver.get_table_schema(&src_handle, &table_name).await
        .cmd_err("compare_table_data")?;
    let tgt_schema = tgt_driver.get_table_schema(&tgt_handle, &table_name).await
        .cmd_err("compare_table_data")?;

    let source_row_count = count_rows(src_driver.as_ref(), &src_handle, &table_name).await?;
    let target_row_count = count_rows(tgt_driver.as_ref(), &tgt_handle, &table_name).await?;

    let col_names: Vec<String> = src_schema.columns.iter().map(|c| c.name.clone()).collect();
    let pk_cols = resolve_pk_columns(&src_schema);

    let src_rows = fetch_sample_rows(
        src_driver.as_ref(),
        &src_handle,
        &table_name,
        &col_names,
        &pk_cols,
    ).await?;
    let tgt_rows = fetch_sample_rows(
        tgt_driver.as_ref(),
        &tgt_handle,
        &table_name,
        &tgt_schema.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        &resolve_pk_columns(&tgt_schema),
    ).await?;

    let src_map = rows_to_key_map(&col_names, &pk_cols, &src_rows);
    let tgt_col_names: Vec<String> = tgt_schema.columns.iter().map(|c| c.name.clone()).collect();
    let tgt_map = rows_to_key_map(&tgt_col_names, &resolve_pk_columns(&tgt_schema), &tgt_rows);

    let mut mismatches = Vec::new();
    let mut truncated = false;

    for (key, src_row) in &src_map {
        match tgt_map.get(key) {
            None => {
                if mismatches.len() >= DATA_COMPARE_MISMATCH_LIMIT {
                    truncated = true;
                    break;
                }
                mismatches.push(serde_json::json!({
                    "key": key,
                    "kind": "source_only",
                    "source": row_to_json_map(&col_names, src_row),
                }));
            }
            Some(tgt_row) if !rows_equal(&col_names, src_row, &tgt_col_names, tgt_row) => {
                if mismatches.len() >= DATA_COMPARE_MISMATCH_LIMIT {
                    truncated = true;
                    break;
                }
                mismatches.push(serde_json::json!({
                    "key": key,
                    "kind": "different",
                    "source": row_to_json_map(&col_names, src_row),
                    "target": row_to_json_map(&tgt_col_names, tgt_row),
                }));
            }
            _ => {}
        }
    }

    if !truncated {
        for (key, tgt_row) in &tgt_map {
            if !src_map.contains_key(key) {
                if mismatches.len() >= DATA_COMPARE_MISMATCH_LIMIT {
                    truncated = true;
                    break;
                }
                mismatches.push(serde_json::json!({
                    "key": key,
                    "kind": "target_only",
                    "target": row_to_json_map(&tgt_col_names, tgt_row),
                }));
            }
        }
    }

    let sampled_rows = src_rows.len().max(tgt_rows.len()) as u64;

    tracing::info!(%table_name, mismatches = mismatches.len(), "compare_table_data OK");
    Ok(serde_json::json!({
        "table": table_name,
        "sourceRowCount": source_row_count,
        "targetRowCount": target_row_count,
        "sampledRows": sampled_rows,
        "mismatches": mismatches,
        "truncated": truncated,
    }))
}

#[tauri::command]
pub async fn compare_table_data(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<serde_json::Value, CommandError> {
    compare_table_data_impl(&state, source_connection_id, target_connection_id, table_name).await
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Query full column types with precision from PostgreSQL using format_type().
async fn pg_full_column_types(
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    table: &str,
) -> Result<std::collections::HashMap<String, String>, CommandError> {
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
        .cmd_err("pg_full_column_types")?;
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

/// Resolve source and target sync adapters for a given pair of database types.
/// Registers only those two types (or one if they match) on first use.
fn resolve_adapters(
    state: &AppState,
    src_type: &DatabaseType,
    tgt_type: &DatabaseType,
) -> Result<(Arc<dyn SyncSourceAdapter>, Arc<dyn SyncTargetAdapter>), CommandError> {
    state
        .sync_adapters
        .ensure_pair(src_type, tgt_type)
        .map_err(CommandError::NotFound)?;
    let src_adapter = state.sync_adapters.get_source(src_type).ok_or_else(|| {
        CommandError::NotFound(format!("No sync source adapter for {:?}", src_type))
    })?;
    let tgt_adapter = state.sync_adapters.get_target(tgt_type).ok_or_else(|| {
        CommandError::NotFound(format!("No sync target adapter for {:?}", tgt_type))
    })?;
    Ok((src_adapter, tgt_adapter))
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

/// Count rows in a table on a given connection.
async fn count_rows(
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    table: &str,
) -> Result<u64, CommandError> {
    let sql = format!("SELECT COUNT(*) FROM {}", driver.quote_ident(table));
    let res = driver.query(handle, &sql).await
        .cmd_err("count_rows")?;
    if let Some(row) = res.rows.first() {
        if let Some(Some(crate::db::Value::Integer(n))) = row.first() {
            return Ok(*n as u64);
        }
    }
    Ok(0)
}

fn resolve_pk_columns(schema: &TableSchema) -> Vec<String> {
    if !schema.primary_keys.is_empty() {
        return schema.primary_keys.clone();
    }
    schema
        .columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect()
}

async fn fetch_sample_rows(
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    table: &str,
    col_names: &[String],
    pk_cols: &[String],
) -> Result<Vec<Vec<Option<Value>>>, CommandError> {
    if col_names.is_empty() {
        return Ok(Vec::new());
    }

    let sq = |name: &str| driver.quote_ident(name);
    let select_cols: Vec<String> = col_names.iter().map(|c| sq(c)).collect();
    let order_cols: Vec<String> = if pk_cols.is_empty() {
        select_cols.clone()
    } else {
        pk_cols.iter().map(|c| sq(c)).collect()
    };

    let sql = format!(
        "SELECT {} FROM {} ORDER BY {} LIMIT {}",
        select_cols.join(", "),
        sq(table),
        order_cols.join(", "),
        DATA_COMPARE_SAMPLE_LIMIT,
    );

    let result = driver.query(handle, &sql).await.cmd_err("fetch_sample_rows")?;
    Ok(result.rows)
}

fn row_key(col_names: &[String], pk_cols: &[String], row: &[Option<Value>]) -> String {
    if pk_cols.is_empty() {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(row).unwrap_or_default().hash(&mut hasher);
        format!("h:{:016x}", hasher.finish())
    } else {
        pk_cols
            .iter()
            .map(|pk| {
                let idx = col_names.iter().position(|n| n == pk).unwrap_or(0);
                value_key_part(row.get(idx).unwrap_or(&None))
            })
            .collect::<Vec<_>>()
            .join("\x00")
    }
}

fn value_key_part(value: &Option<Value>) -> String {
    match value {
        None => "\\N".into(),
        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "null".into()),
    }
}

fn rows_to_key_map(
    col_names: &[String],
    pk_cols: &[String],
    rows: &[Vec<Option<Value>>],
) -> HashMap<String, Vec<Option<Value>>> {
    let mut map = HashMap::new();
    for row in rows {
        let key = row_key(col_names, pk_cols, row);
        map.insert(key, row.clone());
    }
    map
}

fn row_to_json_map(col_names: &[String], row: &[Option<Value>]) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for (i, name) in col_names.iter().enumerate() {
        let val = row.get(i).cloned().flatten();
        obj.insert(
            name.clone(),
            serde_json::to_value(val).unwrap_or(serde_json::Value::Null),
        );
    }
    serde_json::Value::Object(obj)
}

fn values_equal(a: Option<&Option<Value>>, b: Option<&Option<Value>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(va), Some(vb)) => serde_json::to_string(va).ok()
            == serde_json::to_string(vb).ok(),
        _ => false,
    }
}

fn rows_equal(
    src_cols: &[String],
    src_row: &[Option<Value>],
    tgt_cols: &[String],
    tgt_row: &[Option<Value>],
) -> bool {
    for col in src_cols {
        let src_idx = src_cols.iter().position(|n| n == col);
        let tgt_idx = tgt_cols.iter().position(|n| n == col);
        let src_val = src_idx.and_then(|i| src_row.get(i));
        let tgt_val = tgt_idx.and_then(|i| tgt_row.get(i));
        if !values_equal(src_val, tgt_val) {
            return false;
        }
    }
    true
}

/// Core sync logic for a single table, using the IR adapter pipeline.
async fn sync_one_table<F>(
    state: &AppState,
    source_connection_id: &str,
    target_connection_id: &str,
    table_name: &str,
    src_type: &DatabaseType,
    tgt_type: &DatabaseType,
    src_adapter: &dyn SyncSourceAdapter,
    tgt_adapter: &dyn SyncTargetAdapter,
    on_progress: F,
) -> Result<u64, CommandError>
where
    F: Fn(u64) + Send + Sync,
{
    let _cross_db = src_type != tgt_type;

    let (src_driver, src_handle) = state.connection_manager
        .get_connection(source_connection_id).await
        .cmd_err("sync_one_table")?;
    let (tgt_driver, tgt_handle) = state.connection_manager
        .get_connection(target_connection_id).await
        .cmd_err("sync_one_table")?;

    let sq = |name: &str| src_driver.quote_ident(name);

    let src_schema: TableSchema = src_driver.get_table_schema(&src_handle, table_name).await
        .cmd_err("sync_one_table")?;

    let full_types = if src_type == "postgresql" {
        Some(pg_full_column_types(src_driver.as_ref(), &src_handle, table_name).await?)
    } else {
        None
    };

    let ir_table = src_adapter.table_to_ir(&src_schema, full_types.as_ref());

    tgt_driver.execute(
        &tgt_handle,
        &format!("DROP TABLE IF EXISTS {}", tgt_adapter.quote_ident(table_name)),
    ).await.cmd_err("sync_one_table")?;

    let create_ddl = build_create_table_ddl(&ir_table, tgt_adapter);
    tgt_driver.execute(&tgt_handle, &create_ddl).await
        .cmd_err("sync_one_table")?;

    let src_col_names: Vec<String> = src_schema.columns.iter().map(|c| sq(&c.name)).collect();
    let tgt_col_names: Vec<String> = ir_table.columns.iter()
        .map(|c| tgt_adapter.quote_ident(&c.name)).collect();
    let select_sql = format!("SELECT {} FROM {}", src_col_names.join(", "), sq(table_name));
    let result = src_driver.query(&src_handle, &select_sql).await
        .cmd_err("sync_one_table")?;

    let cols_joined = tgt_col_names.join(", ");
    let mut synced: u64 = 0;

    for batch in result.rows.chunks(BATCH_SIZE) {
        let value_sets: Vec<String> = batch.iter().map(|row| {
            let vals: Vec<String> = row.iter().enumerate().map(|(i, v)| {
                tgt_adapter.format_literal(v, &ir_table.columns[i].ir_type)
            }).collect();
            format!("({})", vals.join(", "))
        }).collect();

        let insert = format!(
            "INSERT INTO {} ({}) VALUES {}",
            tgt_adapter.quote_ident(table_name),
            cols_joined,
            value_sets.join(", ")
        );
        tgt_driver.execute(&tgt_handle, &insert).await
            .cmd_err("sync_one_table")?;

        synced += batch.len() as u64;
        on_progress(synced);
    }

    Ok(synced)
}

// ── Tauri Commands ──────────────────────────────────────────────────

/// Sync a single table from source to target (drop+recreate+insert).
pub(crate) async fn sync_table_impl(
    state: &AppState,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<u64, CommandError> {
    tracing::info!(%source_connection_id, %target_connection_id, %table_name, "sync_table");

    let src_config = state.connection_manager
        .get_connection_config(&source_connection_id).await
        .cmd_err("sync_table")?;
    let tgt_config = state.connection_manager
        .get_connection_config(&target_connection_id).await
        .cmd_err("sync_table")?;

    let src_type = &src_config.database_type;
    let tgt_type = &tgt_config.database_type;

    let (src_adapter, tgt_adapter) = resolve_adapters(&state, src_type, tgt_type)?;

    let total = sync_one_table(
        &state,
        &source_connection_id,
        &target_connection_id,
        &table_name,
        src_type,
        tgt_type,
        src_adapter.as_ref(),
        tgt_adapter.as_ref(),
        &|_| {},
    ).await?;

    tracing::info!(%table_name, total, "sync_table OK");
    Ok(total)
}

#[tauri::command]
pub async fn sync_table(
    state: State<'_, AppState>,
    source_connection_id: String,
    target_connection_id: String,
    table_name: String,
) -> Result<u64, CommandError> {
    sync_table_impl(&state, source_connection_id, target_connection_id, table_name).await
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
) -> Result<serde_json::Value, CommandError> {
    tracing::info!(%task_id, table_count = tables.len(), %strategy, "sync_tables");

    let src_config = state.connection_manager
        .get_connection_config(&source_connection_id).await
        .cmd_err("sync_tables")?;
    let tgt_config = state.connection_manager
        .get_connection_config(&target_connection_id).await
        .cmd_err("sync_tables")?;

    let src_type = src_config.database_type.clone();
    let tgt_type = tgt_config.database_type.clone();

    let (src_adapter, tgt_adapter) = resolve_adapters(&state, &src_type, &tgt_type)?;

    let emit = |evt: SyncProgressEvent| { let _ = app_handle.emit("sync:progress", &evt); };

    let mut completed: Vec<String> = skip_tables.clone();
    let mut source_row_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let total_tables = tables.len();

    emit(SyncProgressEvent {
        task_id: task_id.clone(), phase: "counting".into(),
        table_index: 0, total_tables, current_table: String::new(),
        source_row_count: 0, synced_rows: 0, completed_tables: completed.clone(),
        error: None,
    });

    {
        let (src_driver, src_handle) = state.connection_manager
            .get_connection(&source_connection_id).await
            .cmd_err("sync_tables")?;
        for t in &tables {
            let cnt = count_rows(src_driver.as_ref(), &src_handle, t).await?;
            source_row_counts.insert(t.clone(), cnt);
        }
    }

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
    state.store.save_sync_task(task.clone()).await.cmd_err("sync_tables")?;

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

        task.current_table = Some(table_name.clone());
        task.current_table_offset = 0;
        task.updated_at = Utc::now();
        state.store.save_sync_task(task.clone()).await.cmd_err("sync_tables")?;

        let task_id_clone = task_id.clone();
        let table_name_clone = table_name.clone();
        let completed_clone = completed.clone();
        let emit_ref = &emit;

        let sync_result = sync_one_table(
            &state,
            &source_connection_id,
            &target_connection_id,
            table_name,
            &src_type,
            &tgt_type,
            src_adapter.as_ref(),
            tgt_adapter.as_ref(),
            &|synced| {
                emit_ref(SyncProgressEvent {
                    task_id: task_id_clone.clone(), phase: "syncing".into(),
                    table_index: idx, total_tables, current_table: table_name_clone.clone(),
                    source_row_count: src_rows, synced_rows: synced,
                    completed_tables: completed_clone.clone(), error: None,
                });
            },
        ).await;

        match sync_result {
            Ok(_rows) => {
                completed.push(table_name.clone());
                task.completed_tables = completed.clone();
                task.current_table = None;
                task.current_table_offset = 0;
                task.updated_at = Utc::now();
                state.store.save_sync_task(task.clone()).await.cmd_err("sync_tables")?;

                emit(SyncProgressEvent {
                    task_id: task_id.clone(), phase: "table_done".into(),
                    table_index: idx, total_tables, current_table: table_name.clone(),
                    source_row_count: src_rows, synced_rows: src_rows,
                    completed_tables: completed.clone(), error: None,
                });
            }
            Err(err) => {
                let err_msg = err.to_string();
                task.status = "failed".into();
                task.error_message = Some(err_msg.clone());
                task.updated_at = Utc::now();
                state.store.save_sync_task(task.clone()).await.cmd_err("sync_tables")?;

                emit(SyncProgressEvent {
                    task_id: task_id.clone(), phase: "error".into(),
                    table_index: idx, total_tables, current_table: table_name.clone(),
                    source_row_count: src_rows, synced_rows: 0,
                    completed_tables: completed.clone(), error: Some(err_msg),
                });

                return Err(err);
            }
        }
    }

    task.status = "completed".into();
    task.current_table = None;
    task.updated_at = Utc::now();
    state.store.save_sync_task(task.clone()).await.cmd_err("sync_tables")?;

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
pub(crate) async fn get_sync_tasks_impl(state: &AppState) -> Result<Vec<SyncTask>, CommandError> {
    Ok(state.store.get_sync_tasks().await)
}

pub(crate) async fn save_sync_task_direct_impl(
    state: &AppState,
    task: SyncTask,
) -> Result<(), CommandError> {
    state
        .store
        .save_sync_task(task)
        .await
        .cmd_err("save_sync_task_direct")
}

pub(crate) async fn delete_sync_task_impl(
    state: &AppState,
    task_id: String,
) -> Result<(), CommandError> {
    state
        .store
        .delete_sync_task(&task_id)
        .await
        .cmd_err("delete_sync_task")
}

pub(crate) async fn check_sync_conflicts_impl(
    state: &AppState,
    task_id: String,
) -> Result<serde_json::Value, CommandError> {
    let tasks = state.store.get_sync_tasks().await;
    let task = tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| CommandError::NotFound("Sync task not found".into()))?;

    let (src_driver, src_handle) = state
        .connection_manager
        .get_connection(&task.source_connection_id)
        .await
        .cmd_err("check_sync_conflicts")?;

    let mut conflicts = Vec::<serde_json::Value>::new();

    for table in &task.tables {
        if task.completed_tables.contains(table) {
            continue;
        }

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

#[tauri::command]
pub async fn get_sync_tasks(state: State<'_, AppState>) -> Result<Vec<SyncTask>, CommandError> {
    get_sync_tasks_impl(&state).await
}

#[tauri::command]
pub async fn save_sync_task_direct(state: State<'_, AppState>, task: SyncTask) -> Result<(), CommandError> {
    save_sync_task_direct_impl(&state, task).await
}

#[tauri::command]
pub async fn delete_sync_task(state: State<'_, AppState>, task_id: String) -> Result<(), CommandError> {
    delete_sync_task_impl(&state, task_id).await
}

#[tauri::command]
pub async fn check_sync_conflicts(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<serde_json::Value, CommandError> {
    check_sync_conflicts_impl(&state, task_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnSchema, TableSchema, Value};

    fn col(name: &str, data_type: &str, nullable: bool, pk: bool) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            default_value: None,
            comment: None,
            is_primary_key: pk,
            is_auto_increment: false,
        }
    }

    fn table(name: &str, columns: Vec<ColumnSchema>, primary_keys: Vec<String>) -> TableSchema {
        TableSchema {
            table_name: name.into(),
            columns,
            primary_keys,
            indexes: vec![],
            foreign_keys: vec![],
        }
    }

    #[test]
    fn diff_table_schemas_detects_added_removed_changed() {
        let src = table(
            "users",
            vec![
                col("id", "integer", false, true),
                col("name", "text", true, false),
                col("legacy", "text", true, false),
            ],
            vec!["id".into()],
        );
        let tgt = table(
            "users",
            vec![
                col("id", "integer", false, true),
                col("name", "varchar", true, false),
                col("email", "text", false, false),
            ],
            vec!["id".into()],
        );

        // Source = desired state: columns on src missing from tgt are "added" (to target).
        let diff = diff_table_schemas("users", &src, &tgt);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].name, "legacy");
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].name, "email");
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.changed[0].name, "name");
        assert!(diff.changed[0].changes.contains(&"dataType".into()));
    }

    #[test]
    fn resolve_pk_columns_prefers_primary_keys_list() {
        let schema = table(
            "t",
            vec![col("a", "int", false, true), col("b", "int", false, false)],
            vec!["b".into()],
        );
        assert_eq!(resolve_pk_columns(&schema), vec!["b".to_string()]);
    }

    #[test]
    fn resolve_pk_columns_falls_back_to_column_flags() {
        let schema = table(
            "t",
            vec![col("a", "int", false, true), col("b", "int", false, false)],
            vec![],
        );
        assert_eq!(resolve_pk_columns(&schema), vec!["a".to_string()]);
    }

    #[test]
    fn row_key_uses_pk_values() {
        let cols = vec!["id".into(), "name".into()];
        let row = vec![
            Some(Value::Integer(42)),
            Some(Value::String("alice".into())),
        ];
        let key = row_key(&cols, &["id".into()], &row);
        assert!(key.contains("42"));
        assert!(!key.starts_with("h:"));
    }

    #[test]
    fn row_key_hashes_when_no_pk() {
        let cols = vec!["name".into()];
        let row = vec![Some(Value::String("bob".into()))];
        let key = row_key(&cols, &[], &row);
        assert!(key.starts_with("h:"));
        assert_eq!(key, row_key(&cols, &[], &row));
    }

    #[test]
    fn value_key_part_serializes_null_and_values() {
        assert_eq!(value_key_part(&None), "\\N");
        assert_eq!(
            value_key_part(&Some(Value::Integer(1))),
            serde_json::to_string(&Value::Integer(1)).unwrap()
        );
    }

    #[test]
    fn rows_to_key_map_last_row_wins_duplicate_keys() {
        let cols = vec!["id".into()];
        let rows = vec![
            vec![Some(Value::Integer(1))],
            vec![Some(Value::Integer(1))],
        ];
        let map = rows_to_key_map(&cols, &["id".into()], &rows);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn row_to_json_map_aligns_columns() {
        let cols = vec!["id".into(), "name".into()];
        let row = vec![
            Some(Value::Integer(7)),
            Some(Value::String("x".into())),
        ];
        let json = row_to_json_map(&cols, &row);
        assert_eq!(json["id"], 7);
        assert_eq!(json["name"], "x");
    }

    #[test]
    fn rows_equal_requires_matching_values_for_all_source_columns() {
        let src_cols = vec!["id".into(), "name".into()];
        let tgt_cols = vec!["id".into(), "extra".into()];
        let src_row = vec![Some(Value::Integer(1)), Some(Value::String("a".into()))];
        let tgt_row = vec![Some(Value::Integer(1)), Some(Value::String("ignored".into()))];
        assert!(!rows_equal(&src_cols, &src_row, &tgt_cols, &tgt_row));
    }

    #[test]
    fn rows_equal_true_when_columns_align() {
        let cols = vec!["id".into(), "name".into()];
        let a = vec![Some(Value::Integer(1)), Some(Value::String("a".into()))];
        let b = vec![Some(Value::Integer(1)), Some(Value::String("a".into()))];
        assert!(rows_equal(&cols, &a, &cols, &b));
    }

    #[test]
    fn rows_equal_detects_mismatch() {
        let cols = vec!["id".into()];
        let a = vec![Some(Value::Integer(1))];
        let b = vec![Some(Value::Integer(2))];
        assert!(!rows_equal(&cols, &a, &cols, &b));
    }

    #[test]
    fn values_equal_treats_missing_and_null_as_distinct() {
        assert!(values_equal(Some(&None), Some(&None)));
        assert!(!values_equal(None, Some(&Some(Value::Null))));
        assert!(values_equal(
            Some(&Some(Value::String("a".into()))),
            Some(&Some(Value::String("a".into())))
        ));
    }

    #[tokio::test]
    async fn sync_task_crud_and_compare() {
        use crate::testing::app_state::{sample_postgres_config, TestAppState};
        use chrono::Utc;

        let test = TestAppState::with_tables().await;
        test.store
            .save_connection(sample_postgres_config("src-cfg"))
            .await
            .unwrap();
        test.store
            .save_connection({
                let mut c = sample_postgres_config("tgt-cfg");
                c.name = "Target".into();
                c
            })
            .await
            .unwrap();

        let src_conn = test.connect_config("src-cfg").await;
        let tgt_conn = test.connect_config("tgt-cfg").await;

        let results = compare_databases_impl(&test.state, src_conn.clone(), tgt_conn.clone())
            .await
            .unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0]["status"], "identical");

        let task = SyncTask {
            id: "task-1".into(),
            source_connection_id: src_conn.clone(),
            target_connection_id: tgt_conn,
            source_config_id: "src-cfg".into(),
            target_config_id: "tgt-cfg".into(),
            tables: vec!["users".into()],
            completed_tables: vec![],
            current_table: None,
            current_table_offset: 0,
            source_row_counts: [("users".to_string(), 2u64)]
                .into_iter()
                .collect(),
            strategy: "full".into(),
            status: "running".into(),
            error_message: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        save_sync_task_direct_impl(&test.state, task).await.unwrap();
        assert_eq!(get_sync_tasks_impl(&test.state).await.unwrap().len(), 1);

        let conflicts = check_sync_conflicts_impl(&test.state, "task-1".into())
            .await
            .unwrap();
        assert_eq!(conflicts["hasConflicts"], false);

        delete_sync_task_impl(&test.state, "task-1".into())
            .await
            .unwrap();
        assert!(get_sync_tasks_impl(&test.state).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn compare_table_schemas_and_data_impl() {
        use crate::db::Value;
        use crate::testing::app_state::{sample_postgres_config, TestAppState};
        use crate::testing::mock_driver::MockDriverOptions;

        let opts = MockDriverOptions {
            count_total: 1,
            query_rows: vec![vec![
                Some(Value::Integer(1)),
                Some(Value::String("alice".into())),
            ]],
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        test.store
            .save_connection(sample_postgres_config("src"))
            .await
            .unwrap();
        test.store
            .save_connection(sample_postgres_config("tgt"))
            .await
            .unwrap();
        let src = test.connect_config("src").await;
        let tgt = test.connect_config("tgt").await;

        let schema_diff = compare_table_schemas_impl(
            &test.state,
            src.clone(),
            tgt.clone(),
            "users".into(),
        )
        .await
        .unwrap();
        assert_eq!(schema_diff["table"], "users");

        let data_diff = compare_table_data_impl(
            &test.state,
            src,
            tgt,
            "users".into(),
        )
        .await
        .unwrap();
        assert_eq!(data_diff["table"], "users");
    }

    #[tokio::test]
    async fn sync_table_impl_copies_rows() {
        use crate::testing::app_state::{sample_postgres_config, TestAppState};

        let test = TestAppState::with_tables().await;
        test.registry
            .register_test_driver("postgresql", test.mock.clone())
            .await;

        let mut src_cfg = sample_postgres_config("src-sync");
        src_cfg.database_type = "postgresql".into();
        let mut tgt_cfg = sample_postgres_config("tgt-sync");
        tgt_cfg.database_type = "postgresql".into();

        test.store.save_connection(src_cfg).await.unwrap();
        test.store.save_connection(tgt_cfg).await.unwrap();
        let src = test.connect_config("src-sync").await;
        let tgt = test.connect_config("tgt-sync").await;

        let rows = sync_table_impl(&test.state, src, tgt, "users".into())
            .await
            .unwrap();
        assert!(rows >= 1);
    }

    #[tokio::test]
    async fn check_sync_conflicts_missing_task_errors() {
        use crate::testing::app_state::TestAppState;

        let test = TestAppState::new().await;
        assert!(check_sync_conflicts_impl(&test.state, "missing".into())
            .await
            .is_err());
    }
}
