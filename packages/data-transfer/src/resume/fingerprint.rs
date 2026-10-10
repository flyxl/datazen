use super::*;
use sha2::{Digest, Sha256};

/// Accept only exact declared primary-key order. A column flag fallback is
/// insufficient for a composite key because it loses constraint order.
pub fn resumable_primary_key(
    schema: &TableSchema,
    recordset: Option<&super::super::model::TransferRecordset>,
    driver_type: &str,
) -> Result<Vec<String>, TransferError> {
    primary_key_for_paging(schema, recordset, driver_type, true)
}

/// Offset paging is safe inside one proven read snapshot, even when a key
/// cannot be rebound losslessly or its collation is unsuitable for restart.
pub(crate) fn primary_key_for_snapshot(
    schema: &TableSchema,
    recordset: Option<&super::super::model::TransferRecordset>,
    driver_type: &str,
) -> Result<Vec<String>, TransferError> {
    primary_key_for_paging(schema, recordset, driver_type, false)
}

fn primary_key_for_paging(
    schema: &TableSchema,
    recordset: Option<&super::super::model::TransferRecordset>,
    driver_type: &str,
    resumable: bool,
) -> Result<Vec<String>, TransferError> {
    if !super::supports_chunk_driver(driver_type) {
        return Err(TransferError::unsupported(format!(
            "source driver '{driver_type}' has no verified keyset resume contract"
        )));
    }
    if schema.primary_keys.is_empty() {
        return Err(TransferError::unsupported(
            "in-table resume requires a declared primary key",
        ));
    }
    let keys = schema.primary_keys.clone();
    let mut seen = std::collections::HashSet::new();
    for key in &keys {
        if !seen.insert(key) {
            return Err(TransferError::unsupported(
                "in-table resume requires unique primary-key metadata",
            ));
        }
        let column = schema
            .columns
            .iter()
            .find(|column| column.name == *key)
            .ok_or_else(|| {
                TransferError::unsupported(format!(
                    "primary-key column '{key}' is missing from inspected source metadata"
                ))
            })?;
        if column.nullable || !column.is_primary_key {
            return Err(TransferError::unsupported(format!(
                "primary-key column '{key}' is not proven non-null by source metadata"
            )));
        }
        if resumable
            && driver_type.eq_ignore_ascii_case("mysql")
            && is_mysql_exact_numeric_type(&column.data_type)
            && !mysql_integer_cursor_is_lossless(&column.data_type)
        {
            return Err(TransferError::unsupported(format!(
                "MySQL primary-key column '{key}' uses exact numeric values with string cursor bindings; in-table resume is disabled for this key type"
            )));
        }
        if resumable {
            datazen_migration_common::recordset_bounds::ensure_supported_bound_type(
                &column.data_type,
                key,
            )
            .map_err(TransferError::validation)
            .map_err(|error| TransferError::unsupported(error.to_string()))?;
            if datazen_migration_common::recordset_bounds::is_text_bound_type(&column.data_type) {
                return Err(TransferError::unsupported(format!(
                "primary-key column '{key}' uses text ordering whose collation cannot be verified for resume"
            )));
            }
            if is_float_type(&column.data_type) {
                return Err(TransferError::unsupported(format!(
                "primary-key column '{key}' has floating-point ordering, which is not supported for resume"
            )));
            }
        }
    }
    let primary_indexes: Vec<_> = schema
        .indexes
        .iter()
        .filter(|index| index.is_primary)
        .collect();
    if primary_indexes.len() != 1 || primary_indexes[0].columns != keys {
        return Err(TransferError::unsupported(
            "complete declared primary-key index order could not be verified",
        ));
    }
    if let Some(recordset) = recordset {
        let resolved = resolve_recordset(recordset, schema)?;
        if resolved.order_by != keys {
            return Err(TransferError::unsupported(
                "in-table resume requires the recordset order to match the complete primary key",
            ));
        }
    }
    Ok(keys)
}

fn is_mysql_exact_numeric_type(data_type: &str) -> bool {
    let normalized = data_type.trim().to_ascii_lowercase();
    let base = normalized
        .split('(')
        .next()
        .unwrap_or(normalized.as_str())
        .split_whitespace()
        .next()
        .unwrap_or("");
    matches!(
        base,
        "tinyint"
            | "smallint"
            | "mediumint"
            | "int"
            | "integer"
            | "bigint"
            | "decimal"
            | "dec"
            | "numeric"
            | "fixed"
            | "serial"
    )
}

/// MySQL decodes TINYINT through INT into an i64/u32-backed Value::Integer.
/// BIGINT and DECIMAL are represented as strings, and binding those strings
/// through `?` loses the declared numeric type for exact keyset comparisons.
fn mysql_integer_cursor_is_lossless(data_type: &str) -> bool {
    let normalized = data_type.trim().to_ascii_lowercase();
    let base = normalized
        .split('(')
        .next()
        .unwrap_or(normalized.as_str())
        .split_whitespace()
        .next()
        .unwrap_or("");
    matches!(
        base,
        "tinyint" | "smallint" | "mediumint" | "int" | "integer"
    )
}

fn is_float_type(data_type: &str) -> bool {
    let normalized = data_type.trim().to_ascii_lowercase();
    let base = normalized.split('(').next().unwrap_or("").trim();
    matches!(base, "float" | "float4" | "float8" | "real" | "double") || base == "double precision"
}

pub(crate) fn source_projection(columns: &[&ColumnMapping], keys: &[String]) -> Vec<String> {
    let mut projection: Vec<String> = columns
        .iter()
        .map(|column| column.source_column.clone())
        .collect();
    for key in keys {
        if !projection.iter().any(|column| column == key) {
            projection.push(key.clone());
        }
    }
    projection
}

pub(crate) fn remaining_page_limit(
    chunk_size: u32,
    rows_seen: u64,
    total_limit: Option<u64>,
) -> Option<u32> {
    let Some(remaining) = total_limit.map(|limit| limit.saturating_sub(rows_seen)) else {
        return Some(chunk_size);
    };
    if remaining == 0 {
        return None;
    }
    Some(chunk_size.min(remaining.min(u32::MAX as u64) as u32))
}

pub(crate) fn effective_chunk_size(
    user_batch_size: u32,
    target_columns: usize,
    max_bound_parameters: usize,
) -> Result<u32, TransferError> {
    if target_columns == 0 {
        return Err(TransferError::validation(
            "in-table resume requires at least one target column",
        ));
    }
    let max_rows_by_parameters = max_bound_parameters / target_columns;
    if max_rows_by_parameters == 0 {
        return Err(TransferError::unsupported(
            "target mapping exceeds the driver's bound-parameter limit for a single row",
        ));
    }
    Ok(user_batch_size.min(max_rows_by_parameters as u32))
}

pub(crate) fn build_page_for_context(
    driver: &dyn DatabaseDriver,
    select_from: &str,
    scope: &SourceScope,
    keys: &[String],
    cursor: Option<&[Value]>,
    limit: u32,
    quote: char,
    schema: &TableSchema,
) -> Result<(String, Vec<Value>), TransferError> {
    build_keyset_page(
        select_from,
        scope.where_sql.as_deref(),
        &scope.count_params,
        keys,
        cursor,
        limit,
        quote,
        |index, data_type| {
            driver
                .parameter_placeholder(index, data_type)
                .map_err(|error| TransferError::unsupported(error.to_string()))
        },
        |key| {
            schema
                .columns
                .iter()
                .find(|column| column.name == key)
                .map(|column| column.data_type.clone())
        },
    )
}

pub(crate) fn build_snapshot_offset_page(
    driver: &dyn DatabaseDriver,
    select_from: &str,
    scope: &SourceScope,
    keys: &[String],
    offset: u64,
    limit: u32,
    quote: char,
) -> Result<(String, Vec<Value>), TransferError> {
    if !driver.supports_offset() || keys.is_empty() || limit == 0 {
        return Err(TransferError::unsupported(
            "snapshot offset paging is unavailable",
        ));
    }
    let mut sql = select_from.to_string();
    if let Some(filter) = scope.where_sql.as_deref() {
        let predicate = filter
            .trim()
            .strip_prefix("WHERE ")
            .unwrap_or(filter.trim());
        if !predicate.is_empty() {
            sql.push_str(" WHERE (");
            sql.push_str(predicate);
            sql.push(')');
        }
    }
    sql.push_str(" ORDER BY ");
    sql.push_str(
        &keys
            .iter()
            .map(|key| quote_ident_sql(key, quote))
            .collect::<Vec<_>>()
            .join(", "),
    );
    sql.push(' ');
    sql.push_str(&driver.pagination_syntax(u64::from(limit), offset).clause);
    Ok((sql, scope.count_params.clone()))
}

pub(crate) async fn fingerprint_source_rows(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    select_from: &str,
    source_scope: &SourceScope,
    keys: &[String],
    cursor_indexes: &[usize],
    projection: &[String],
    schema: &TableSchema,
    quote: char,
    chunk_size: u32,
    total_limit: Option<u64>,
    checkpoint: &mut dyn TransferResumeCheckpoint,
    cancelled: Option<&AtomicBool>,
    offset_paging: bool,
) -> Result<String, TransferError> {
    let mut hasher = Sha256::new();
    hash_bytes(&mut hasher, b"datazen-transfer-source-v1");
    for name in projection {
        hash_bytes(&mut hasher, name.as_bytes());
        if let Some(column) = schema.columns.iter().find(|column| column.name == *name) {
            hash_bytes(&mut hasher, column.data_type.as_bytes());
        }
    }
    for key in keys {
        hash_bytes(&mut hasher, key.as_bytes());
    }
    if let Some(where_sql) = source_scope.where_sql.as_deref() {
        hash_bytes(&mut hasher, where_sql.as_bytes());
    }
    let mut cursor: Option<Vec<Value>> = None;
    let mut rows_seen = 0u64;
    loop {
        checkpoint.renew()?;
        if cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err(TransferError::cancelled(
                "source fingerprint was cancelled before target writes",
            ));
        }
        let Some(limit) = remaining_page_limit(chunk_size, rows_seen, total_limit) else {
            break;
        };
        let (sql, params) = if offset_paging {
            build_snapshot_offset_page(
                driver,
                select_from,
                source_scope,
                keys,
                rows_seen,
                limit,
                quote,
            )?
        } else {
            build_page_for_context(
                driver,
                select_from,
                source_scope,
                keys,
                cursor.as_deref(),
                limit,
                quote,
                schema,
            )?
        };
        let page = driver
            .query_with_params(handle, &sql, &params)
            .await
            .map_err(|error| {
                TransferError::validation(format!("source fingerprint page failed: {error}"))
            })?;
        if cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err(TransferError::cancelled(
                "source fingerprint was cancelled before target writes",
            ));
        }
        validate_page(&page, projection, limit as usize)?;
        if page.rows.is_empty() {
            break;
        }
        for row in &page.rows {
            for value in row {
                hash_value(&mut hasher, value.as_ref());
            }
        }
        if !offset_paging {
            cursor = Some(last_cursor(&page.rows, cursor_indexes)?);
        }
        rows_seen = rows_seen.saturating_add(page.rows.len() as u64);
    }
    hasher.update(rows_seen.to_be_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn validate_page(
    page: &datazen_driver_api::QueryResult,
    expected_columns: &[String],
    max_rows: usize,
) -> Result<(), TransferError> {
    let actual: Vec<_> = page
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect();
    // Some drivers derive result-column metadata from the first returned row.
    // An empty bounded page therefore has no `columns` even though the SELECT
    // projection is unchanged. The query itself was constructed from the
    // inspected schema, so accept absent metadata only when there are no rows.
    if actual != expected_columns && !(page.rows.is_empty() && actual.is_empty()) {
        return Err(TransferError::validation(
            format!(
                "source page projection changed while transfer was running (expected {expected_columns:?}, got {actual:?})"
            ),
        ));
    }
    if page.rows.len() > max_rows {
        return Err(TransferError::validation(
            "source driver returned more rows than the bounded page limit",
        ));
    }
    if page
        .rows
        .iter()
        .any(|row| row.len() != expected_columns.len())
    {
        return Err(TransferError::validation(
            "source page row width differs from its inspected projection",
        ));
    }
    Ok(())
}

pub(crate) fn last_cursor(
    rows: &[Vec<Option<Value>>],
    indexes: &[usize],
) -> Result<Vec<Value>, TransferError> {
    let row = rows
        .last()
        .ok_or_else(|| TransferError::validation("cannot checkpoint an empty source page"))?;
    indexes
        .iter()
        .map(|index| {
            let value = row
                .get(*index)
                .and_then(Option::as_ref)
                .ok_or_else(|| TransferError::validation("source primary-key cursor is NULL"))?;
            if !cursor_value_supported(value) {
                return Err(TransferError::unsupported(
                    "source primary-key cursor has an unsupported decoded value type",
                ));
            }
            Ok(value.clone())
        })
        .collect()
}

pub(crate) fn cursor_value_supported(value: &Value) -> bool {
    matches!(value, Value::Integer(_) | Value::String(_) | Value::Bool(_))
}

pub(crate) fn hash_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

pub(crate) fn hash_value(hasher: &mut Sha256, value: Option<&Value>) {
    let Some(value) = value else {
        hasher.update([0]);
        return;
    };
    match value {
        Value::Null => hasher.update([1]),
        Value::Bool(value) => {
            hasher.update([2]);
            hasher.update([u8::from(*value)]);
        }
        Value::Integer(value) => {
            hasher.update([3]);
            hasher.update(value.to_be_bytes());
        }
        Value::Float(value) => {
            hasher.update([4]);
            hasher.update(value.to_bits().to_be_bytes());
        }
        Value::String(value) => {
            hasher.update([5]);
            hash_bytes(hasher, value.as_bytes());
        }
        Value::Bytes(value) => {
            hasher.update([6]);
            hash_bytes(hasher, value);
        }
        Value::Timestamp(value) => {
            hasher.update([7]);
            hash_bytes(hasher, value.as_bytes());
        }
        Value::Json(value) => {
            hasher.update([8]);
            match serde_json::to_vec(value) {
                Ok(bytes) => hash_bytes(hasher, &bytes),
                Err(_) => hash_bytes(hasher, b"invalid-json-value"),
            }
        }
    }
}
