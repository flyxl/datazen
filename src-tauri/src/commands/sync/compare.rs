use crate::data_sync::sql::qualify_relation_sql;
use crate::schema_diff::types::{ChangedColumnDiff, ColumnSnapshot, TableColumnDiff};
use crate::transfer::ir::{IRColumn, IRTable, IRType};
use std::collections::HashMap;

// ── Helpers ─────────────────────────────────────────────────────────

/// Run adapter-provided full-type SQL (if any) and map `(name, full_type)` rows.
pub(crate) async fn fetch_full_column_types(
    adapter: &dyn crate::transfer::adapter::SyncSourceAdapter,
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
    family: &str,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> Result<u64, CommandError> {
    let quote = if family == "mysql" { '`' } else { '"' };
    let qualified = qualify_relation_sql(family, database, schema, table, quote);
    let sql = format!("SELECT COUNT(*) FROM {qualified}");
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
