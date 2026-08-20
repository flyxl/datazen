use super::super::error::{CmdExt, CommandError};
use super::types::DATA_COMPARE_SAMPLE_LIMIT;
use crate::db::{TableSchema, Value};
use crate::schema_diff::types::{ChangedColumnDiff, ColumnSnapshot, TableColumnDiff};
use crate::sync::ir::{IRColumn, IRTable, IRType};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub(super) async fn maybe_use_database(
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    database: Option<&str>,
) -> Result<(), CommandError> {
    let Some(db) = database.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    driver
        .use_database(handle, db)
        .await
        .cmd_err("maybe_use_database")
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Run adapter-provided full-type SQL (if any) and map `(name, full_type)` rows.
pub(crate) async fn fetch_full_column_types(
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
        if let (
            Some(Some(crate::db::Value::String(name))),
            Some(Some(crate::db::Value::String(ft))),
        ) = (row.get(0), row.get(1))
        {
            map.insert(name.clone(), ft.clone());
        }
    }
    Ok(map)
}

/// Count rows in a table on a given connection.
pub(super) fn value_as_u64(value: &crate::db::Value) -> Option<u64> {
    match value {
        crate::db::Value::Integer(n) if *n >= 0 => Some(*n as u64),
        crate::db::Value::Float(f) if *f >= 0.0 && f.is_finite() => Some(*f as u64),
        crate::db::Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

pub(crate) async fn count_rows(
    driver: &dyn crate::db::DatabaseDriver,
    handle: &crate::db::ConnectionHandle,
    table: &str,
) -> Result<u64, CommandError> {
    let sql = format!("SELECT COUNT(*) FROM {}", driver.quote_ident(table));
    let res = driver.query(handle, &sql).await.cmd_err("count_rows")?;
    if let Some(row) = res.rows.first() {
        if let Some(Some(v)) = row.first() {
            if let Some(n) = value_as_u64(v) {
                return Ok(n);
            }
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
pub(crate) fn diff_table_schemas_ir(
    table: &str,
    src_ir: &IRTable,
    tgt_ir: &IRTable,
) -> TableColumnDiff {
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

pub(crate) async fn fetch_sample_rows(
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

    let result = driver
        .query(handle, &sql)
        .await
        .cmd_err("fetch_sample_rows")?;
    Ok(result.rows)
}

pub(crate) fn row_key(col_names: &[String], pk_cols: &[String], row: &[Option<Value>]) -> String {
    if pk_cols.is_empty() {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(row)
            .unwrap_or_default()
            .hash(&mut hasher);
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
        (Some(va), Some(vb)) => serde_json::to_string(va).ok() == serde_json::to_string(vb).ok(),
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
