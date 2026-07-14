use super::{AppState, log_err};
use crate::db::{DatabaseType, TableSchema};
use crate::store::SyncTask;
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::sync::ddl::build_create_table_ddl;
use chrono::Utc;
use std::sync::Arc;
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

// ── Helpers ─────────────────────────────────────────────────────────

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

/// Resolve source and target sync adapters for a given pair of database types.
fn resolve_adapters(
    state: &AppState,
    src_type: &DatabaseType,
    tgt_type: &DatabaseType,
) -> Result<(Arc<dyn SyncSourceAdapter>, Arc<dyn SyncTargetAdapter>), String> {
    let src_adapter = state.sync_adapters.get_source(src_type)
        .ok_or_else(|| format!("No sync source adapter for {:?}", src_type))?;
    let tgt_adapter = state.sync_adapters.get_target(tgt_type)
        .ok_or_else(|| format!("No sync target adapter for {:?}", tgt_type))?;
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
) -> Result<u64, String>
where
    F: Fn(u64) + Send + Sync,
{
    let cross_db = src_type != tgt_type;

    let (src_driver, src_handle) = state.connection_manager
        .get_connection(source_connection_id).await
        .map_err(|e| log_err("sync_one_table", &e))?;
    let (tgt_driver, tgt_handle) = state.connection_manager
        .get_connection(target_connection_id).await
        .map_err(|e| log_err("sync_one_table", &e))?;

    let sq = |name: &str| src_driver.quote_ident(name);

    let src_schema: TableSchema = src_driver.get_table_schema(&src_handle, table_name).await
        .map_err(|e| log_err("sync_one_table", &e))?;

    // For PG sources, always fetch full types with precision (e.g. varchar(100)
    // instead of just "character varying") to preserve type fidelity in IR.
    let full_types = if matches!(src_type, DatabaseType::PostgreSQL) {
        Some(pg_full_column_types(src_driver.as_ref(), &src_handle, table_name).await?)
    } else {
        None
    };

    // Source → IR
    let ir_table = src_adapter.table_to_ir(&src_schema, full_types.as_ref());

    // Drop existing target table
    tgt_driver.execute(
        &tgt_handle,
        &format!("DROP TABLE IF EXISTS {}", tgt_adapter.quote_ident(table_name)),
    ).await.map_err(|e| log_err("sync_one_table", &e))?;

    // IR → target DDL
    let create_ddl = build_create_table_ddl(&ir_table, tgt_adapter);
    tgt_driver.execute(&tgt_handle, &create_ddl).await
        .map_err(|e| log_err("sync_one_table", &e))?;

    // SELECT all rows from source
    let src_col_names: Vec<String> = src_schema.columns.iter().map(|c| sq(&c.name)).collect();
    let tgt_col_names: Vec<String> = ir_table.columns.iter()
        .map(|c| tgt_adapter.quote_ident(&c.name)).collect();
    let select_sql = format!("SELECT {} FROM {}", src_col_names.join(", "), sq(table_name));
    let result = src_driver.query(&src_handle, &select_sql).await
        .map_err(|e| log_err("sync_one_table", &e))?;

    let cols_joined = tgt_col_names.join(", ");
    let mut synced: u64 = 0;

    // Batch insert using IR-driven literal formatting
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
            .map_err(|e| log_err("sync_one_table", &e))?;

        synced += batch.len() as u64;
        on_progress(synced);
    }

    Ok(synced)
}

// ── Tauri Commands ──────────────────────────────────────────────────

/// Sync a single table from source to target (drop+recreate+insert).
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

    let (src_adapter, tgt_adapter) = resolve_adapters(&state, &src_type, &tgt_type)?;

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

        task.current_table = Some(table_name.clone());
        task.current_table_offset = 0;
        task.updated_at = Utc::now();
        state.store.save_sync_task(task.clone()).await.map_err(|e| log_err("sync_tables", &e))?;

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
