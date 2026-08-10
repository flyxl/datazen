use super::compare::{count_rows, fetch_full_column_types, resolve_adapters};
use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::types::{SyncProgressEvent, BATCH_SIZE};
use crate::db::{DatabaseType, TableSchema};
use crate::store::SyncTask;
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::sync::ddl::build_create_table_ddl;
use chrono::Utc;
use tauri::Emitter;

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

    let full_types = if src_adapter.full_column_types_query(table_name).is_some() {
        Some(
            fetch_full_column_types(src_adapter, src_driver.as_ref(), &src_handle, table_name)
                .await?,
        )
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

/// Sync multiple tables with progress events and checkpoint support.
pub(crate) async fn sync_tables_impl(
    state: &AppState,
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
