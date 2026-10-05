//! Data Sync recordset bounds, tuple normalization, and SQL predicate construction.

use super::DataSyncError;
use crate::sql::quote_ident_sql;
use datazen_driver_api::{TableSchema, Value};
use serde::{Deserialize, Serialize};

/// A bounded row range used by Data Sync. Scalar fields retain the original
/// single-key profile shape; tuple_range selects a complete composite key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncRecordsetBound {
    pub value: serde_json::Value,
    #[serde(default = "default_true")]
    pub inclusive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncRecordsetTupleRange {
    /// Must match the complete effective primary key in declared order.
    pub columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<SyncRecordsetTupleBound>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<SyncRecordsetTupleBound>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncRecordsetTupleBound {
    pub values: Vec<serde_json::Value>,
    #[serde(default = "default_true")]
    pub inclusive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncRecordset {
    /// Omitted only when the source has one effective primary-key column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<SyncRecordsetBound>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<SyncRecordsetBound>,
    /// Complete composite primary-key range. Its absence preserves legacy
    /// scalar JSON exactly when old profiles are loaded and saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuple_range: Option<SyncRecordsetTupleRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
struct ResolvedSyncBound {
    values: Vec<Value>,
    inclusive: bool,
    key: Option<crate::recordset_bounds::BoundKey>,
}

#[derive(Debug, Clone)]
struct ResolvedSyncRecordset {
    order_by: Vec<String>,
    start: Option<ResolvedSyncBound>,
    end: Option<ResolvedSyncBound>,
    limit: Option<u64>,
}

fn resolve_recordset(
    recordset: &SyncRecordset,
    schema: &TableSchema,
) -> Result<ResolvedSyncRecordset, DataSyncError> {
    let primary_keys = schema.effective_primary_keys();
    if let Some(tuple_range) = recordset.tuple_range.as_ref() {
        if recordset.order_by.is_some() || recordset.start.is_some() || recordset.end.is_some() {
            return Err(DataSyncError::validation(
                "recordset cannot mix legacy scalar bounds with tupleRange",
            ));
        }
        if primary_keys.len() < 2 || tuple_range.columns != primary_keys {
            return Err(DataSyncError::validation(
                "tupleRange columns must exactly match the complete source primary key in declared order",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for key in &primary_keys {
            if !seen.insert(key) {
                return Err(DataSyncError::validation(
                    "source primary-key metadata contains duplicate columns",
                ));
            }
            let column = schema
                .columns
                .iter()
                .find(|column| column.name == *key)
                .ok_or_else(|| {
                    DataSyncError::validation(format!(
                        "source primary-key column '{key}' is not present in the table"
                    ))
                })?;
            if column.nullable {
                return Err(DataSyncError::validation(format!(
                    "source primary-key column '{key}' is nullable; tuple ranges require non-null keys"
                )));
            }
        }
        let start = resolve_tuple_bound(tuple_range.start.as_ref(), "start", &primary_keys)?;
        let end = resolve_tuple_bound(tuple_range.end.as_ref(), "end", &primary_keys)?;
        return finish_recordset(primary_keys, start, end, recordset.limit);
    }
    let order_by = match recordset.order_by.as_deref().map(str::trim) {
        Some(column) if !column.is_empty() => column.to_string(),
        _ => match primary_keys.as_slice() {
            [column] => column.clone(),
            [] => {
                return Err(DataSyncError::validation(
                    "recordset requires an explicit orderBy because the source table has no primary key",
                ))
            }
            _ => {
                return Err(DataSyncError::validation(
                    "recordset requires an explicit single orderBy column because the source primary key is composite",
                ))
            }
        },
    };
    if !schema.columns.iter().any(|column| column.name == order_by) {
        return Err(DataSyncError::validation(format!(
            "recordset orderBy column '{}' is not present in the source table",
            order_by
        )));
    }
    if !primary_keys.iter().any(|column| column == &order_by) {
        return Err(DataSyncError::validation(
            "recordset orderBy must be a source primary-key column for stable sync paging",
        ));
    }
    let order_type = schema
        .columns
        .iter()
        .find(|column| column.name == order_by)
        .map(|column| column.data_type.as_str())
        .ok_or_else(|| DataSyncError::validation("recordset orderBy column is missing"))?;
    resolve_recordset_parts(recordset, vec![order_by], order_type)
}

pub(super) fn validate_recordset(
    recordset: &SyncRecordset,
    schema: &TableSchema,
) -> Result<(), DataSyncError> {
    resolve_recordset(recordset, schema).map(|_| ())
}

pub(super) fn comparison_columns(
    recordset: &SyncRecordset,
    schema: &TableSchema,
) -> Result<Vec<String>, DataSyncError> {
    let resolved = resolve_recordset(recordset, schema)?;
    if resolved.start.is_some() || resolved.end.is_some() {
        Ok(resolved.order_by)
    } else {
        Ok(Vec::new())
    }
}

pub(super) fn recordset_limit(
    recordset: &SyncRecordset,
    schema: &TableSchema,
) -> Result<Option<u64>, DataSyncError> {
    resolve_recordset(recordset, schema).map(|resolved| resolved.limit)
}

fn resolve_recordset_without_schema<F>(
    recordset: &SyncRecordset,
    default_order: Option<&str>,
    column_type: F,
) -> Result<ResolvedSyncRecordset, DataSyncError>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(tuple_range) = recordset.tuple_range.as_ref() {
        for column in &tuple_range.columns {
            column_type(column).ok_or_else(|| {
                DataSyncError::validation(format!(
                    "recordset tuple key '{column}' is not present in the source table"
                ))
            })?;
        }
        let start = resolve_tuple_bound(tuple_range.start.as_ref(), "start", &tuple_range.columns)?;
        let end = resolve_tuple_bound(tuple_range.end.as_ref(), "end", &tuple_range.columns)?;
        return finish_recordset(tuple_range.columns.clone(), start, end, recordset.limit);
    }
    let order_by = recordset
        .order_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(default_order)
        .ok_or_else(|| {
            DataSyncError::validation(
                "recordset orderBy is required when building a predicate without source schema",
            )
        })?
        .to_string();
    let order_type = column_type(&order_by).ok_or_else(|| {
        DataSyncError::validation(format!(
            "recordset orderBy column '{}' is not present in the source table",
            order_by
        ))
    })?;
    resolve_recordset_parts(recordset, vec![order_by], &order_type)
}

fn resolve_recordset_parts(
    recordset: &SyncRecordset,
    order_by: Vec<String>,
    order_type: &str,
) -> Result<ResolvedSyncRecordset, DataSyncError> {
    let start = resolve_recordset_bound(recordset.start.as_ref(), "start", order_type)?;
    let end = resolve_recordset_bound(recordset.end.as_ref(), "end", order_type)?;
    finish_recordset(order_by, start, end, recordset.limit)
}

fn finish_recordset(
    order_by: Vec<String>,
    start: Option<ResolvedSyncBound>,
    end: Option<ResolvedSyncBound>,
    limit: Option<u64>,
) -> Result<ResolvedSyncRecordset, DataSyncError> {
    if let (Some(start), Some(end)) = (&start, &end) {
        if let (Some(start_key), Some(end_key)) = (&start.key, &end.key) {
            match crate::recordset_bounds::compare_bound_keys(start_key, end_key)
                .map_err(|error| DataSyncError::validation(error.to_string()))?
            {
                std::cmp::Ordering::Greater => {
                    return Err(DataSyncError::validation(
                        "recordset start bound must not be greater than end bound",
                    ));
                }
                std::cmp::Ordering::Equal if !(start.inclusive && end.inclusive) => {
                    return Err(DataSyncError::validation(
                        "recordset range is empty when equal bounds are exclusive",
                    ));
                }
                _ => {}
            }
        }
    }
    if limit == Some(0) {
        return Err(DataSyncError::validation(
            "recordset limit must be greater than 0",
        ));
    }
    Ok(ResolvedSyncRecordset {
        order_by,
        start,
        end,
        limit,
    })
}

fn resolve_recordset_bound(
    bound: Option<&SyncRecordsetBound>,
    name: &str,
    data_type: &str,
) -> Result<Option<ResolvedSyncBound>, DataSyncError> {
    let Some(bound) = bound else {
        return Ok(None);
    };
    let (value, key) = crate::recordset_bounds::canonical_bound_value(
        &bound.value,
        data_type,
        name,
    )
    .map_err(|error| DataSyncError::validation(error.to_string()))?;
    Ok(Some(ResolvedSyncBound {
        values: vec![value],
        inclusive: bound.inclusive,
        key: Some(key),
    }))
}

fn resolve_tuple_bound(
    bound: Option<&SyncRecordsetTupleBound>,
    name: &str,
    columns: &[String],
) -> Result<Option<ResolvedSyncBound>, DataSyncError> {
    let Some(bound) = bound else {
        return Ok(None);
    };
    if bound.values.len() != columns.len() {
        return Err(DataSyncError::validation(format!(
            "recordset {name} tuple has {} values, expected {} for the complete primary key",
            bound.values.len(),
            columns.len()
        )));
    }
    let values = bound
        .values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            if value.is_null() {
                return Err(DataSyncError::validation(format!(
                    "recordset {name} tuple component {} cannot be NULL",
                    index + 1
                )));
            }
            tuple_json_value(value).map_err(|error| {
                DataSyncError::validation(format!(
                    "recordset {name} tuple component {} is invalid: {error}",
                    index + 1
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ResolvedSyncBound {
        values,
        inclusive: bound.inclusive,
        key: None,
    }))
}

fn tuple_json_value(value: &serde_json::Value) -> Result<Value, DataSyncError> {
    Ok(match value {
        serde_json::Value::String(value) => Value::String(value.clone()),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(Value::Integer)
            .unwrap_or_else(|| Value::String(value.to_string())),
        serde_json::Value::Bool(value) => Value::Bool(*value),
        _ => {
            return Err(DataSyncError::validation(
                "tuple components must be scalar strings, numbers, or booleans",
            ))
        }
    })
}

/// Validate tuple endpoints using the driver's key normalizer before a
/// comparison plan is issued.
pub(super) fn validate_tuple_range_order<F>(
    recordset: &SyncRecordset,
    mut normalize: F,
) -> Result<(), DataSyncError>
where
    F: FnMut(&str, &Value) -> Result<datazen_driver_api::SyncKeyValue, String>,
{
    let Some(tuple_range) = recordset.tuple_range.as_ref() else {
        return Ok(());
    };
    let resolved = finish_recordset(
        tuple_range.columns.clone(),
        resolve_tuple_bound(tuple_range.start.as_ref(), "start", &tuple_range.columns)?,
        resolve_tuple_bound(tuple_range.end.as_ref(), "end", &tuple_range.columns)?,
        recordset.limit,
    )?;
    let mut normalize_bound = |bound: Option<&ResolvedSyncBound>, name: &str| {
        bound
            .map(|bound| {
                bound
                    .values
                    .iter()
                    .zip(&tuple_range.columns)
                    .map(|(value, column)| {
                        normalize(column, value).map_err(|reason| {
                            DataSyncError::validation(format!(
                                "recordset {name} bound for key '{column}' has no verified driver ordering: {reason}"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()
    };
    let start = normalize_bound(resolved.start.as_ref(), "start")?;
    let end = normalize_bound(resolved.end.as_ref(), "end")?;
    if let (Some(start), Some(end)) = (start, end) {
        match start.cmp(&end) {
            std::cmp::Ordering::Greater => {
                return Err(DataSyncError::validation(
                    "recordset start bound must not be greater than end bound",
                ));
            }
            std::cmp::Ordering::Equal
                if !(resolved.start.as_ref().is_some_and(|bound| bound.inclusive)
                    && resolved.end.as_ref().is_some_and(|bound| bound.inclusive)) =>
            {
                return Err(DataSyncError::validation(
                    "recordset range is empty when equal bounds are exclusive",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Construct recordset WHERE terms using the exact expressions and value
/// adapter shared with the live keyset scanner.
pub(super) fn build_predicate<P, F, N>(
    recordset: &SyncRecordset,
    quote: char,
    start_index: usize,
    default_order: Option<&str>,
    key_order: Option<(&[String], &[String])>,
    normalize_key_value: &N,
    column_type: &F,
    placeholder: &mut P,
) -> Result<(Vec<String>, Vec<Value>), DataSyncError>
where
    P: FnMut(usize, Option<&str>) -> Result<String, DataSyncError>,
    F: Fn(&str) -> Option<String>,
    N: Fn(&str, &Value) -> Result<Value, DataSyncError>,
{
    let resolved = resolve_recordset_without_schema(recordset, default_order, column_type)?;
    let expressions = if let Some((key_columns, key_expressions)) = key_order {
        if key_columns.len() != key_expressions.len() {
            return Err(DataSyncError::validation(
                "key ordering expression count does not match primary-key columns",
            ));
        }
        Some(
            resolved
                .order_by
                .iter()
                .map(|column| {
                    key_columns
                        .iter()
                        .position(|candidate| candidate == column)
                        .and_then(|index| key_expressions.get(index))
                        .cloned()
                        .ok_or_else(|| {
                            DataSyncError::validation(format!(
                                "recordset key '{column}' has no verified ordering expression"
                            ))
                        })
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else if recordset.tuple_range.is_some() {
        return Err(DataSyncError::validation(
            "tupleRange requires verified driver key ordering expressions",
        ));
    } else {
        None
    };

    let mut parts = Vec::with_capacity(2);
    let mut params = Vec::new();
    let mut next = start_index;
    for (bound_name, bound) in [
        ("start", resolved.start.as_ref()),
        ("end", resolved.end.as_ref()),
    ] {
        let Some(bound) = bound else {
            continue;
        };
        let operator = match (bound_name, bound.inclusive) {
            ("start", true) => ">=",
            ("start", false) => ">",
            ("end", true) => "<=",
            ("end", false) => "<",
            _ => unreachable!(),
        };
        let mut markers = Vec::with_capacity(resolved.order_by.len());
        for (column, value) in resolved.order_by.iter().zip(&bound.values) {
            let value = normalize_key_value(column, value)?;
            let marker = placeholder(next, column_type(column).as_deref())?;
            next += 1;
            markers.push(marker);
            params.push(value);
        }
        let quoted_columns = resolved
            .order_by
            .iter()
            .map(|column| quote_ident_sql(column, quote))
            .collect::<Vec<_>>();
        let left_columns = expressions.as_ref().unwrap_or(&quoted_columns);
        let left = if left_columns.len() == 1 {
            left_columns[0].clone()
        } else {
            format!("({})", left_columns.join(", "))
        };
        let right = if markers.len() == 1 {
            markers[0].clone()
        } else {
            format!("({})", markers.join(", "))
        };
        parts.push(format!("({left} {operator} {right})"));
    }
    Ok((parts, params))
}
