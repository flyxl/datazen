//! Validated, parameterized source filters for Data Transfer.
//!
//! Filters are represented as structured conditions.  The client never sends
//! a replacement SELECT or a raw WHERE fragment.  This module validates every
//! referenced column against the source schema and emits SQL plus bound
//! values for the driver's parameterized query API.

use crate::error::TransferError;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use datazen_driver_api::sql_identifiers::quote_ident_sql;
use datazen_driver_api::filters::{FilterCondition, FilterOperator};
use datazen_driver_api::{TableSchema, Value};
use serde::{Deserialize, Serialize};

const MAX_CONDITIONS: usize = 32;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum FilterLogic {
    #[default]
    And,
    Or,
}

/// JSON-backed to preserve the existing TransferJob equality contract while
/// keeping the public IPC shape `{ filters, logic }` easy for the UI to edit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct SourceFilter(pub serde_json::Value);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceFilterPayload {
    #[serde(default)]
    filters: Vec<SourceFilterCondition>,
    #[serde(default)]
    logic: FilterLogic,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceFilterCondition {
    column: String,
    operator: FilterOperator,
    #[serde(default)]
    value: serde_json::Value,
}

impl SourceFilter {
    pub fn new(filters: Vec<FilterCondition>, logic: FilterLogic) -> Result<Self, TransferError> {
        let filters = filters
            .into_iter()
            .map(|condition| {
                Ok(SourceFilterCondition {
                    column: condition.column,
                    operator: condition.operator,
                    value: value_to_json(&condition.value)?,
                })
            })
            .collect::<Result<Vec<_>, TransferError>>()?;
        serde_json::to_value(SourceFilterPayloadOwned { filters, logic })
            .map(Self)
            .map_err(|error| TransferError::validation(format!("invalid source filter: {error}")))
    }

    fn payload(&self) -> Result<SourceFilterPayload, TransferError> {
        serde_json::from_value(self.0.clone())
            .map_err(|error| TransferError::validation(format!("invalid source filter: {error}")))
    }

    pub fn validate(&self, schema: &TableSchema) -> Result<(), TransferError> {
        let payload = self.payload()?;
        if payload.filters.is_empty() {
            return Ok(());
        }
        if payload.filters.len() > MAX_CONDITIONS {
            return Err(TransferError::validation(format!(
                "source filter supports at most {MAX_CONDITIONS} conditions"
            )));
        }
        for condition in &payload.filters {
            if condition.column.trim().is_empty() {
                return Err(TransferError::validation(
                    "source filter column is required",
                ));
            }
            if !schema
                .columns
                .iter()
                .any(|column| column.name == condition.column)
            {
                return Err(TransferError::validation(format!(
                    "source filter column '{}' is not present in the source table",
                    condition.column
                )));
            }
            validate_condition(condition)?;
        }
        Ok(())
    }

    pub fn is_empty(&self) -> Result<bool, TransferError> {
        Ok(self.payload()?.filters.is_empty())
    }

    /// Build a `WHERE ...` fragment and the values bound to its placeholders.
    pub fn build_where<P>(
        &self,
        quote: char,
        start_index: usize,
        placeholder: P,
    ) -> Result<(Option<String>, Vec<Value>), TransferError>
    where
        P: FnMut(usize, Option<&str>) -> Result<String, TransferError>,
    {
        self.build_where_typed(quote, start_index, |_| None, placeholder)
    }

    /// Build a source predicate while passing the source column type to the
    /// driver's placeholder formatter. This lets PostgreSQL cast UI-entered
    /// text such as `2` to the inspected integer/numeric type without losing
    /// large-value precision in the browser.
    pub fn build_where_typed<P, F>(
        &self,
        quote: char,
        start_index: usize,
        column_type: F,
        mut placeholder: P,
    ) -> Result<(Option<String>, Vec<Value>), TransferError>
    where
        P: FnMut(usize, Option<&str>) -> Result<String, TransferError>,
        F: Fn(&str) -> Option<String>,
    {
        let payload = self.payload()?;
        if payload.filters.is_empty() {
            return Ok((None, Vec::new()));
        }
        let mut params = Vec::new();
        let mut parts = Vec::with_capacity(payload.filters.len());
        let mut next = start_index;
        for condition in &payload.filters {
            validate_condition(condition)?;
            let column = quote_ident_sql(&condition.column, quote);
            let data_type = column_type(&condition.column);
            let part = match condition.operator {
                FilterOperator::Eq
                | FilterOperator::Ne
                | FilterOperator::Gt
                | FilterOperator::Lt
                | FilterOperator::Gte
                | FilterOperator::Lte
                | FilterOperator::Like => {
                    let op = match condition.operator {
                        FilterOperator::Eq => "=",
                        FilterOperator::Ne => "!=",
                        FilterOperator::Gt => ">",
                        FilterOperator::Lt => "<",
                        FilterOperator::Gte => ">=",
                        FilterOperator::Lte => "<=",
                        FilterOperator::Like => "LIKE",
                        _ => unreachable!(),
                    };
                    let value = scalar_value(&condition.value)?;
                    let marker = placeholder(next, data_type.as_deref())?;
                    next += 1;
                    params.push(value);
                    format!("({column} {op} {marker})")
                }
                FilterOperator::In => {
                    let values = in_values(&condition.value)?;
                    let markers = values
                        .into_iter()
                        .map(|value| {
                            let marker = placeholder(next, data_type.as_deref())?;
                            next += 1;
                            params.push(value);
                            Ok(marker)
                        })
                        .collect::<Result<Vec<_>, TransferError>>()?;
                    format!("({column} IN ({}))", markers.join(", "))
                }
                FilterOperator::IsNull => format!("({column} IS NULL)"),
                FilterOperator::IsNotNull => format!("({column} IS NOT NULL)"),
            };
            parts.push(part);
        }
        let joiner = match payload.logic {
            FilterLogic::And => " AND ",
            FilterLogic::Or => " OR ",
        };
        Ok((Some(format!("WHERE {}", parts.join(joiner))), params))
    }

    /// Human-readable preview with anonymous placeholders. Values remain
    /// private to the server and are never interpolated into preview SQL.
    pub fn preview_where(&self, quote: char) -> Result<Option<String>, TransferError> {
        self.build_where(quote, 1, |_, _| Ok("?".into()))
            .map(|(sql, _)| sql)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceFilterPayloadOwned {
    filters: Vec<SourceFilterCondition>,
    logic: FilterLogic,
}

fn validate_condition(condition: &SourceFilterCondition) -> Result<(), TransferError> {
    match condition.operator {
        FilterOperator::IsNull | FilterOperator::IsNotNull => Ok(()),
        FilterOperator::In => {
            if in_values(&condition.value)?.is_empty() {
                Err(TransferError::validation(
                    "source filter IN requires at least one value",
                ))
            } else {
                Ok(())
            }
        }
        _ => {
            if condition.value.is_null()
                || matches!(&condition.value, serde_json::Value::String(value) if value.is_empty())
            {
                return Err(TransferError::validation(
                    "source filter condition requires a value",
                ));
            }
            Ok(())
        }
    }
}

fn scalar_value(value: &serde_json::Value) -> Result<Value, TransferError> {
    if value.is_array() {
        return Err(TransferError::validation(
            "source filter comparison requires one scalar value",
        ));
    }
    json_to_value(value)
}

fn in_values(value: &serde_json::Value) -> Result<Vec<Value>, TransferError> {
    match value {
        serde_json::Value::Array(values) => values
            .iter()
            .map(json_to_value)
            .collect::<Result<Vec<_>, _>>(),
        serde_json::Value::String(value) => Ok(value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| Value::String(value.to_string()))
            .collect()),
        _ => Err(TransferError::validation(
            "source filter IN requires an array or comma-separated string",
        )),
    }
}

fn value_to_json(value: &Value) -> Result<serde_json::Value, TransferError> {
    Ok(match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(value) => serde_json::Value::Bool(*value),
        Value::Integer(value) => serde_json::Value::Number((*value).into()),
        Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| TransferError::validation("source filter float is not finite"))?,
        Value::String(value) | Value::Timestamp(value) => serde_json::Value::String(value.clone()),
        Value::Bytes(value) => serde_json::json!({
            "$datazenType": "bytes",
            "encoding": "base64",
            "value": BASE64.encode(value),
        }),
        Value::Json(value) => value.clone(),
    })
}

pub fn json_to_value(value: &serde_json::Value) -> Result<Value, TransferError> {
    Ok(match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(value) => Value::Bool(*value),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(Value::Integer)
            .or_else(|| value.as_f64().map(Value::Float))
            .ok_or_else(|| TransferError::validation("source filter number is out of range"))?,
        serde_json::Value::String(value) => Value::String(value.clone()),
        serde_json::Value::Object(value)
            if value.get("$datazenType") == Some(&serde_json::Value::String("bytes".into()))
                && value.get("encoding") == Some(&serde_json::Value::String("base64".into())) =>
        {
            let encoded = value
                .get("value")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    TransferError::validation("source filter bytes value is required")
                })?;
            Value::Bytes(BASE64.decode(encoded).map_err(|_| {
                TransferError::validation("source filter bytes value is invalid base64")
            })?)
        }
        _ => {
            return Err(TransferError::validation(
                "source filter IN values must be scalar",
            ))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::ColumnSchema;

    fn schema() -> TableSchema {
        TableSchema {
            table_name: "users".into(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                },
                ColumnSchema {
                    name: "status".into(),
                    data_type: "TEXT".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        }
    }

    fn condition(column: &str, operator: FilterOperator, value: Value) -> FilterCondition {
        FilterCondition {
            column: column.into(),
            operator,
            value,
        }
    }

    #[test]
    fn builds_parameterized_and_filter_without_interpolating_values() {
        let filter = SourceFilter::new(
            vec![
                condition(
                    "status",
                    FilterOperator::Eq,
                    Value::String("active'".into()),
                ),
                condition("id", FilterOperator::Gt, Value::Integer(2)),
            ],
            FilterLogic::And,
        )
        .unwrap();
        filter.validate(&schema()).unwrap();
        let (where_sql, params) = filter
            .build_where('"', 1, |i, _| Ok(format!("${i}")))
            .unwrap();
        assert_eq!(
            where_sql.as_deref(),
            Some("WHERE (\"status\" = $1) AND (\"id\" > $2)")
        );
        assert_eq!(params.len(), 2);
        assert!(matches!(params[0], Value::String(ref value) if value == "active'"));
    }

    #[test]
    fn rejects_unknown_columns_and_empty_values() {
        let unknown = SourceFilter::new(
            vec![condition("secret", FilterOperator::Eq, Value::Integer(1))],
            FilterLogic::And,
        )
        .unwrap();
        assert!(unknown.validate(&schema()).is_err());
        let empty = SourceFilter::new(
            vec![condition(
                "id",
                FilterOperator::Eq,
                Value::String(String::new()),
            )],
            FilterLogic::And,
        )
        .unwrap();
        assert!(empty.validate(&schema()).is_err());
    }

    #[test]
    fn in_values_are_bounded_and_parameterized() {
        let filter = SourceFilter::new(
            vec![condition(
                "id",
                FilterOperator::In,
                Value::Json(serde_json::json!([1, 2, 3])),
            )],
            FilterLogic::Or,
        )
        .unwrap();
        let (where_sql, params) = filter
            .build_where('`', 3, |i, _| Ok(format!("?{i}")))
            .unwrap();
        assert_eq!(where_sql.as_deref(), Some("WHERE (`id` IN (?3, ?4, ?5))"));
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn accepts_frontend_json_arrays_without_untagged_value_coercion() {
        let filter: SourceFilter = serde_json::from_value(serde_json::json!({
            "filters": [{"column": "id", "operator": "in", "value": [1, 2, 3]}],
            "logic": "and"
        }))
        .unwrap();
        let (_, params) = filter
            .build_where('"', 1, |i, _| Ok(format!("${i}")))
            .unwrap();
        assert!(matches!(
            params.as_slice(),
            [Value::Integer(1), Value::Integer(2), Value::Integer(3)]
        ));
    }

    #[test]
    fn preserves_binary_filter_values_with_an_explicit_marker() {
        let filter = SourceFilter::new(
            vec![condition(
                "status",
                FilterOperator::Eq,
                Value::Bytes(vec![0, 255]),
            )],
            FilterLogic::And,
        )
        .unwrap();
        let (_, params) = filter
            .build_where('"', 1, |i, _| Ok(format!("${i}")))
            .unwrap();
        assert!(matches!(params.as_slice(), [Value::Bytes(value)] if value == &[0, 255]));
    }

    #[test]
    fn passes_source_type_to_placeholder_formatter() {
        let filter = SourceFilter::new(
            vec![condition(
                "id",
                FilterOperator::Gt,
                Value::String("2".into()),
            )],
            FilterLogic::And,
        )
        .unwrap();
        let (sql, params) = filter
            .build_where_typed(
                '"',
                1,
                |column| (column == "id").then_some("integer".into()),
                |index, data_type| Ok(format!("${index}::{}", data_type.unwrap_or("none"))),
            )
            .unwrap();
        assert_eq!(sql.as_deref(), Some("WHERE (\"id\" > $1::integer)"));
        assert!(matches!(params.as_slice(), [Value::String(value)] if value == "2"));
    }
}
