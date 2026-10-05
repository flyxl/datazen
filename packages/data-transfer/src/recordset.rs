//! Validated, parameterized source recordset selection for Data Transfer.
//!
//! A recordset is a user-selected range over deterministic source key columns.
//! It is deliberately separate from checkpointing: the transfer still scans
//! one statement into the existing private spool and never saves an OFFSET for
//! restart.

use datazen_driver_api::TableSchema;

use datazen_data_sync::sql::quote_ident_sql;
use datazen_driver_api::Value;

use super::error::TransferError;
use super::filter::SourceFilter;
use super::model::{
    TransferRecordset, TransferRecordsetBound, TransferRecordsetTupleBound,
    TransferRecordsetTupleRange,
};
use datazen_data_sync::recordset_bounds::{canonical_bound_value, compare_bound_keys, BoundKey};

#[derive(Debug, Clone)]
pub struct ResolvedRecordset {
    pub order_by: Vec<String>,
    pub start: Option<ResolvedBound>,
    pub end: Option<ResolvedBound>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ResolvedBound {
    pub values: Vec<Value>,
    pub inclusive: bool,
    keys: Vec<BoundKey>,
}

#[derive(Debug, Clone)]
pub struct SourceScope {
    pub where_sql: Option<String>,
    pub recordset_sql: Option<String>,
    pub params: Vec<Value>,
    /// Parameters for the same predicate without the recordset LIMIT tail.
    pub count_params: Vec<Value>,
}

impl SourceScope {
    pub fn append_to(&self, sql: &mut String) {
        if let Some(where_sql) = &self.where_sql {
            sql.push(' ');
            sql.push_str(where_sql);
        }
        if let Some(recordset_sql) = &self.recordset_sql {
            sql.push(' ');
            sql.push_str(recordset_sql);
        }
    }
}

pub fn resolve_recordset(
    recordset: &TransferRecordset,
    schema: &TableSchema,
) -> Result<ResolvedRecordset, TransferError> {
    let (order_by, start, end) = if let Some(tuple_range) = &recordset.tuple_range {
        if recordset.order_by.is_some() || recordset.start.is_some() || recordset.end.is_some() {
            return Err(TransferError::validation(
                "recordset cannot mix legacy scalar bounds with tupleRange",
            ));
        }
        let primary_keys = schema.effective_primary_keys();
        if primary_keys.len() < 2 || tuple_range.columns != primary_keys {
            return Err(TransferError::validation(
                "tupleRange columns must exactly match the complete source primary key in declared order",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        let mut types = Vec::with_capacity(primary_keys.len());
        for key in &primary_keys {
            if !seen.insert(key) {
                return Err(TransferError::validation(
                    "source primary-key metadata contains duplicate columns",
                ));
            }
            let column = schema
                .columns
                .iter()
                .find(|column| column.name == *key)
                .ok_or_else(|| {
                    TransferError::validation(format!(
                        "source primary-key column '{key}' is not present in the table"
                    ))
                })?;
            if column.nullable {
                return Err(TransferError::validation(format!(
                    "source primary-key column '{key}' is nullable; tuple ranges require non-null keys"
                )));
            }
            datazen_data_sync::recordset_bounds::ensure_supported_bound_type(&column.data_type, key).map_err(TransferError::validation)?;
            types.push(column.data_type.as_str());
        }
        let start = resolve_tuple_bound(tuple_range.start.as_ref(), "start", &types)?;
        let end = resolve_tuple_bound(tuple_range.end.as_ref(), "end", &types)?;
        validate_range(start.as_ref(), end.as_ref())?;
        (primary_keys, start, end)
    } else {
        let order_by = match recordset.order_by.as_deref().map(str::trim) {
            Some(column) if !column.is_empty() => column.to_string(),
            _ => {
                let primary_keys = schema.effective_primary_keys();
                match primary_keys.as_slice() {
                    [column] => column.clone(),
                    [] => {
                        return Err(TransferError::validation(
                            "recordset requires an explicit orderBy because the source table has no primary key",
                        ));
                    }
                    _ => {
                        return Err(TransferError::validation(
                            "recordset requires tupleRange for the complete composite source primary key",
                        ));
                    }
                }
            }
        };
        if !schema.columns.iter().any(|column| column.name == order_by) {
            return Err(TransferError::validation(format!(
                "recordset orderBy column '{}' is not present in the source table",
                order_by
            )));
        }
        if recordset
            .order_by
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(TransferError::validation(
                "recordset orderBy must name one source column",
            ));
        }
        let order_type = schema
            .columns
            .iter()
            .find(|column| column.name == order_by)
            .map(|column| column.data_type.as_str())
            .ok_or_else(|| {
                TransferError::validation(format!(
                    "recordset orderBy column '{}' is not present in the source table",
                    order_by
                ))
            })?;
        let start = resolve_bound(recordset.start.as_ref(), "start", order_type)?;
        let end = resolve_bound(recordset.end.as_ref(), "end", order_type)?;
        validate_range(start.as_ref(), end.as_ref())?;
        (vec![order_by], start, end)
    };
    let limit = recordset
        .limit
        .map(|value| {
            if value == 0 {
                return Err(TransferError::validation(
                    "recordset limit must be greater than 0",
                ));
            }
            i64::try_from(value).map_err(|_| {
                TransferError::validation("recordset limit is too large for the source driver")
            })
        })
        .transpose()?;

    Ok(ResolvedRecordset {
        order_by,
        start,
        end,
        limit,
    })
}

fn resolve_bound(
    bound: Option<&TransferRecordsetBound>,
    name: &str,
    data_type: &str,
) -> Result<Option<ResolvedBound>, TransferError> {
    let Some(bound) = bound else {
        return Ok(None);
    };
    let (value, key) = canonical_bound_value(&bound.value, data_type, name).map_err(TransferError::validation)?;
    Ok(Some(ResolvedBound {
        values: vec![value],
        inclusive: bound.inclusive,
        keys: vec![key],
    }))
}

fn resolve_tuple_bound(
    bound: Option<&TransferRecordsetTupleBound>,
    name: &str,
    data_types: &[&str],
) -> Result<Option<ResolvedBound>, TransferError> {
    let Some(bound) = bound else {
        return Ok(None);
    };
    if bound.values.len() != data_types.len() {
        return Err(TransferError::validation(format!(
            "recordset {name} tuple has {} values, expected {} for the complete primary key",
            bound.values.len(),
            data_types.len()
        )));
    }
    let mut values = Vec::with_capacity(data_types.len());
    let mut keys = Vec::with_capacity(data_types.len());
    for (index, (raw, data_type)) in bound.values.iter().zip(data_types).enumerate() {
        let component = format!("{name}[{}]", index + 1);
        let (value, key) = canonical_bound_value(raw, data_type, &component).map_err(TransferError::validation)?;
        values.push(value);
        keys.push(key);
    }
    Ok(Some(ResolvedBound {
        values,
        inclusive: bound.inclusive,
        keys,
    }))
}

fn validate_range(
    start: Option<&ResolvedBound>,
    end: Option<&ResolvedBound>,
) -> Result<(), TransferError> {
    if let (Some(start), Some(end)) = (start, end) {
        match compare_bound_tuples(&start.keys, &end.keys)? {
            std::cmp::Ordering::Greater => {
                return Err(TransferError::validation(
                    "recordset start bound must not be greater than end bound",
                ));
            }
            std::cmp::Ordering::Equal if !(start.inclusive && end.inclusive) => {
                return Err(TransferError::validation(
                    "recordset range is empty when equal bounds are exclusive",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn compare_bound_tuples(
    left: &[BoundKey],
    right: &[BoundKey],
) -> Result<std::cmp::Ordering, TransferError> {
    if left.len() != right.len() {
        return Err(TransferError::validation(
            "recordset bounds do not match the same source key arity",
        ));
    }
    for (left, right) in left.iter().zip(right) {
        // The source collation is not available in TableSchema. Equal text
        // components can be skipped safely; distinct text bounds must fail
        // closed instead of assuming the host's lexical order matches SQL.
        let ordering = match (left, right) {
            (BoundKey::Text(left), BoundKey::Text(right)) if left == right => {
                std::cmp::Ordering::Equal
            }
            (BoundKey::Text(_), BoundKey::Text(_)) => {
                return Err(TransferError::validation(
                    "recordset text tuple bounds cannot be ordered without the source collation contract",
                ));
            }
            _ => compare_bound_keys(left, right).map_err(TransferError::validation)?,
        };
        if ordering != std::cmp::Ordering::Equal {
            return Ok(ordering);
        }
    }
    Ok(std::cmp::Ordering::Equal)
}

/// Build the exact source scope used by both preview and execution.
pub fn build_source_scope<P, F>(
    schema: &TableSchema,
    source_filter: Option<&SourceFilter>,
    recordset: Option<&TransferRecordset>,
    quote: char,
    source_driver_type: &str,
    mut placeholder: P,
    column_type: F,
) -> Result<SourceScope, TransferError>
where
    P: FnMut(usize, Option<&str>) -> Result<String, TransferError>,
    F: Fn(&str) -> Option<String>,
{
    let mut params = Vec::new();
    let mut where_parts = Vec::new();
    if let Some(source_filter) = source_filter {
        source_filter.validate(schema)?;
        let (where_sql, filter_params) =
            source_filter.build_where_typed(quote, 1, &column_type, |index, data_type| {
                placeholder(index, data_type)
            })?;
        if let Some(where_sql) = where_sql {
            where_parts.push(where_sql.trim_start_matches("WHERE ").to_string());
        }
        params.extend(filter_params);
    }

    let mut recordset_sql = None;
    let mut count_params = params.clone();
    if let Some(recordset) = recordset {
        let resolved = resolve_recordset(recordset, schema)?;
        if recordset.tuple_range.is_some() && !tuple_ordering_supported(source_driver_type) {
            return Err(TransferError::validation(format!(
                "source driver '{source_driver_type}' cannot guarantee matching composite tuple predicate and scan ordering"
            )));
        }
        let mut range_parts = Vec::new();
        for (bound_name, bound) in [("start", resolved.start), ("end", resolved.end)] {
            let Some(bound) = bound else {
                continue;
            };
            let operator = match (bound_name, bound.inclusive) {
                ("start", true) => ">=",
                ("start", false) => ">",
                ("end", true) => "<=",
                ("end", false) => "<",
                _ => {
                    return Err(TransferError::validation(
                        "recordset bound endpoint is invalid",
                    ));
                }
            };
            let mut markers = Vec::with_capacity(bound.values.len());
            for (column, value) in resolved.order_by.iter().zip(bound.values) {
                let index = params.len() + 1;
                markers.push(placeholder(index, column_type(column).as_deref())?);
                params.push(value);
            }
            let key_sql = resolved
                .order_by
                .iter()
                .map(|column| quote_ident_sql(column, quote))
                .collect::<Vec<_>>();
            let left = if key_sql.len() == 1 {
                key_sql[0].clone()
            } else {
                format!("({})", key_sql.join(", "))
            };
            let right = if markers.len() == 1 {
                markers[0].clone()
            } else {
                format!("({})", markers.join(", "))
            };
            range_parts.push(if resolved.order_by.len() == 1 {
                format!("({left} {operator} {right})")
            } else {
                format!("{left} {operator} {right}")
            });
        }
        where_parts.extend(range_parts);
        count_params = params.clone();
        let order_by = resolved
            .order_by
            .iter()
            .map(|column| format!("{} ASC", quote_ident_sql(column, quote)))
            .collect::<Vec<_>>()
            .join(", ");
        let mut tail = format!("ORDER BY {order_by}");
        if let Some(limit) = resolved.limit {
            let marker = placeholder(params.len() + 1, None)?;
            tail.push_str(" LIMIT ");
            tail.push_str(&marker);
            params.push(Value::Integer(limit));
        }
        recordset_sql = Some(tail);
    }

    Ok(SourceScope {
        where_sql: (!where_parts.is_empty())
            .then(|| format!("WHERE {}", where_parts.join(" AND "))),
        recordset_sql,
        params,
        count_params,
    })
}

fn tuple_ordering_supported(source_driver_type: &str) -> bool {
    matches!(
        source_driver_type.trim().to_ascii_lowercase().as_str(),
        "postgresql" | "mysql" | "mariadb"
    )
}

pub fn preview_summary(
    schema: &TableSchema,
    recordset: &TransferRecordset,
    quote: char,
) -> Result<String, TransferError> {
    let resolved = resolve_recordset(recordset, schema)?;
    let order_by = resolved
        .order_by
        .iter()
        .map(|column| format!("{} ASC", quote_ident_sql(column, quote)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut parts = Vec::new();
    if recordset.tuple_range.is_some() {
        let has_text_key = resolved.order_by.iter().any(|key| {
            schema
                .columns
                .iter()
                .find(|column| column.name == *key)
                .is_some_and(|column| {
                    datazen_data_sync::recordset_bounds::is_text_bound_type(&column.data_type)
                })
        });
        if has_text_key {
            parts.push(
                "text-key endpoints require identical text components unless source collation is known"
                    .to_string(),
            );
        }
        if let Some(start) = recordset
            .tuple_range
            .as_ref()
            .and_then(|range| range.start.as_ref())
        {
            parts.push(format!(
                "start {} {}",
                if start.inclusive { "≥" } else { ">" },
                serde_json::to_string(&start.values).map_err(|error| {
                    TransferError::validation(format!("cannot render recordset preview: {error}"))
                })?
            ));
        }
        if let Some(end) = recordset
            .tuple_range
            .as_ref()
            .and_then(|range| range.end.as_ref())
        {
            parts.push(format!(
                "end {} {}",
                if end.inclusive { "≤" } else { "<" },
                serde_json::to_string(&end.values).map_err(|error| {
                    TransferError::validation(format!("cannot render recordset preview: {error}"))
                })?
            ));
        }
    }
    let mut summary = if parts.is_empty() {
        String::new()
    } else {
        format!("RANGE {} · ", parts.join(" AND "))
    };
    summary.push_str(&format!("ORDER BY {order_by}"));
    if let Some(limit) = resolved.limit {
        summary.push_str(&format!(" LIMIT {limit}"));
    }
    Ok(summary)
}

#[cfg(test)]
mod tests;
