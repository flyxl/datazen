use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::types::{DATA_COMPARE_MISMATCH_LIMIT, DATA_COMPARE_SAMPLE_LIMIT};
use crate::db::{DatabaseType, TableSchema, Value};
use crate::schema_diff::diff_table_schemas;
use crate::schema_diff::types::{ChangedColumnDiff, ColumnSnapshot, TableColumnDiff};
use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::sync::ddl::build_create_table_ddl;
use crate::sync::ir::{IRColumn, IRTable, IRType};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

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
    let mut source_ddl: Option<String> = None;
    let mut target_ddl: Option<String> = None;
    let mut ir_diff: Option<TableColumnDiff> = None;

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
            let src_full_types =
                fetch_full_column_types(src_adapter.as_ref(), src_driver.as_ref(), &src_handle, &table_name)
                    .await
                    .ok();
            let tgt_full_types = fetch_full_column_types(
                tgt_src_adapter.as_ref(),
                tgt_driver.as_ref(),
                &tgt_handle,
                &table_name,
            )
            .await
            .ok();

            let src_ir = src_adapter.table_to_ir(&src_schema, src_full_types.as_ref());
            let tgt_ir = tgt_src_adapter.table_to_ir(&tgt_schema, tgt_full_types.as_ref());
            ir_diff = Some(diff_table_schemas_ir(&table_name, &src_ir, &tgt_ir));
            source_ddl = Some(build_create_table_ddl(&src_ir, src_tgt_adapter.as_ref()));
            target_ddl = Some(build_create_table_ddl(&tgt_ir, tgt_adapter.as_ref()));
        }
    }

    let diff = ir_diff.unwrap_or_else(|| diff_table_schemas(&table_name, &src_schema, &tgt_schema));

    let mut result = serde_json::json!({
        "table": table_name,
        "missingOnTarget": diff.missing_on_target,
        "extraOnTarget": diff.extra_on_target,
        "added": diff.added,
        "removed": diff.removed,
        "changed": diff.changed,
    });
    if let Some(ddl) = source_ddl {
        result["sourceDdl"] = serde_json::Value::String(ddl);
    }
    if let Some(ddl) = target_ddl {
        result["targetDdl"] = serde_json::Value::String(ddl);
    }

    tracing::info!(%table_name, "compare_table_schemas OK");
    Ok(result)
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

// ── Helpers ─────────────────────────────────────────────────────────

/// Run adapter-provided full-type SQL (if any) and map `(name, full_type)` rows.
pub(super) async fn fetch_full_column_types(
    adapter: &dyn crate::sync::adapter::SyncSourceAdapter,
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    table: &str,
) -> Result<std::collections::HashMap<String, String>, CommandError> {
    let Some(sql) = adapter.full_column_types_query(table) else {
        return Ok(std::collections::HashMap::new());
    };
    let result = driver
        .query(handle, &sql)
        .await
        .cmd_err("fetch_full_column_types")?;
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

/// Fetch dialect-specific CREATE TABLE suffix (ENGINE / ORDER BY, etc.).
pub(super) async fn fetch_table_options(
    adapter: &dyn crate::sync::adapter::SyncSourceAdapter,
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    table: &str,
) -> Result<Option<String>, CommandError> {
    let Some(sql) = adapter.table_options_query(table) else {
        return Ok(None);
    };
    let result = driver
        .query(handle, &sql)
        .await
        .cmd_err("fetch_table_options")?;
    let Some(row) = result.rows.first() else {
        return Ok(None);
    };
    match row.first() {
        Some(Some(crate::db::Value::String(s))) if !s.trim().is_empty() => Ok(Some(s.clone())),
        _ => Ok(None),
    }
}

/// Resolve source and target sync adapters for a given pair of database types.
/// Registers only those two types (or one if they match) on first use.
pub(super) fn resolve_adapters(
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

/// Count rows in a table on a given connection.
pub(super) async fn count_rows(
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

/// Stable display string for IR types in schema-diff snapshots.
pub(crate) fn format_ir_type(t: &IRType) -> String {
    match t {
        IRType::Bool => "Bool".into(),
        IRType::Int8 => "Int8".into(),
        IRType::Int16 => "Int16".into(),
        IRType::Int32 => "Int32".into(),
        IRType::Int64 => "Int64".into(),
        IRType::Float32 => "Float32".into(),
        IRType::Float64 => "Float64".into(),
        IRType::Decimal { precision, scale } => format!("Decimal({precision},{scale})"),
        IRType::Char { length } => format!("Char({length})"),
        IRType::Varchar { length } => format!("Varchar({length:?})"),
        IRType::Text => "Text".into(),
        IRType::Binary { length } => format!("Binary({length:?})"),
        IRType::Blob => "Blob".into(),
        IRType::Date => "Date".into(),
        IRType::Time { with_timezone } => format!("Time(tz={with_timezone})"),
        IRType::Timestamp { with_timezone } => format!("Timestamp(tz={with_timezone})"),
        IRType::Json => "Json".into(),
        IRType::Uuid => "Uuid".into(),
        IRType::Bit { length } => format!("Bit({length})"),
        IRType::Other(s) => format!("Other({s})"),
    }
}

fn ir_column_snapshot(col: &IRColumn) -> ColumnSnapshot {
    ColumnSnapshot {
        name: col.name.clone(),
        data_type: format_ir_type(&col.ir_type),
        nullable: col.nullable,
        is_primary_key: col.is_primary_key,
    }
}

/// Compare schemas using IR types so dialect aliases (e.g. `character varying` vs `varchar`)
/// that map to the same [`IRType`] are not reported as dataType changes.
///
/// Same contract as [`crate::schema_diff::diff_table_schemas`]: source is the desired
/// state, so `missing_on_target`/`added` are columns to ADD to the target and
/// `extra_on_target`/`removed` are columns to DROP from the target.
pub(crate) fn diff_table_schemas_ir(table: &str, src_ir: &IRTable, tgt_ir: &IRTable) -> TableColumnDiff {
    let src_map: HashMap<&str, &IRColumn> = src_ir
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let tgt_map: HashMap<&str, &IRColumn> = tgt_ir
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();

    let mut missing_on_target = Vec::new();
    let mut extra_on_target = Vec::new();
    let mut changed = Vec::new();

    for col in &src_ir.columns {
        if !src_map.contains_key(col.name.as_str()) {
            missing_on_target.push(ir_column_snapshot(col));
        }
    }

    for col in &tgt_ir.columns {
        if !src_map.contains_key(col.name.as_str()) {
            extra_on_target.push(ir_column_snapshot(col));
        }
    }

    for col in &src_ir.columns {
        if let Some(tgt_col) = tgt_map.get(col.name.as_str()) {
            let mut changes = Vec::new();
            if col.ir_type != tgt_col.ir_type {
                changes.push("dataType".into());
            }
            if col.nullable != tgt_col.nullable {
                changes.push("nullable".into());
            }
            if col.is_primary_key != tgt_col.is_primary_key {
                changes.push("isPrimaryKey".into());
            }
            if !changes.is_empty() {
                changed.push(ChangedColumnDiff {
                    name: col.name.clone(),
                    source: ir_column_snapshot(col),
                    target: ir_column_snapshot(tgt_col),
                    changes,
                });
            }
        }
    }

    TableColumnDiff {
        table: table.to_string(),
        added: missing_on_target.clone(),
        removed: extra_on_target.clone(),
        missing_on_target,
        extra_on_target,
        changed,
    }
}

pub(crate) fn resolve_pk_columns(schema: &TableSchema) -> Vec<String> {
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

pub(crate) fn row_key(col_names: &[String], pk_cols: &[String], row: &[Option<Value>]) -> String {
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

pub(crate) fn value_key_part(value: &Option<Value>) -> String {
    match value {
        None => "\\N".into(),
        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "null".into()),
    }
}

pub(crate) fn rows_to_key_map(
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

pub(crate) fn row_to_json_map(col_names: &[String], row: &[Option<Value>]) -> serde_json::Value {
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

pub(crate) fn values_equal(a: Option<&Option<Value>>, b: Option<&Option<Value>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(va), Some(vb)) => serde_json::to_string(va).ok()
            == serde_json::to_string(vb).ok(),
        _ => false,
    }
}

pub(crate) fn rows_equal(
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
