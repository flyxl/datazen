//! Validated, parameterized sync filters for Data Synchronization.
//!
//! Filters are represented as structured conditions.  The client never sends
//! a replacement SELECT or a raw WHERE fragment.  This module validates every
//! referenced column against the source schema and emits SQL plus bound
//! values for the driver's parameterized query API.

use super::filter_values::{in_values, scalar_value, value_to_json};
use super::recordset::{
    build_predicate, comparison_columns as recordset_comparison_columns, recordset_limit,
    validate_recordset, validate_tuple_range_order,
};
pub use super::recordset::{SyncRecordset, SyncRecordsetBound};
use crate::data_sync::sql::quote_ident_sql;
use crate::data_sync::DataSyncError;
use crate::db::{TableSchema, Value};
use crate::services::query_executor::{FilterCondition, FilterOperator};
use serde::{Deserialize, Serialize};

const MAX_CONDITIONS: usize = 32;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SyncFilterLogic {
    #[default]
    And,
    Or,
}

/// JSON-backed to preserve the existing TransferJob equality contract while
/// keeping the public IPC shape `{ filters, logic }` easy for the UI to edit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct SyncSourceFilter(pub serde_json::Value);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SyncSyncSourceFilterPayload {
    #[serde(default)]
    filters: Vec<SyncSyncSourceFilterCondition>,
    #[serde(default)]
    logic: SyncFilterLogic,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recordset: Option<SyncRecordset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SyncSyncSourceFilterCondition {
    column: String,
    operator: FilterOperator,
    #[serde(default)]
    value: serde_json::Value,
}

impl SyncSourceFilter {
    pub fn new(
        filters: Vec<FilterCondition>,
        logic: SyncFilterLogic,
    ) -> Result<Self, DataSyncError> {
        let filters = filters
            .into_iter()
            .map(|condition| {
                Ok(SyncSyncSourceFilterCondition {
                    column: condition.column,
                    operator: condition.operator,
                    value: value_to_json(&condition.value)?,
                })
            })
            .collect::<Result<Vec<_>, DataSyncError>>()?;
        serde_json::to_value(SyncSyncSourceFilterPayloadOwned {
            filters,
            logic,
            recordset: None,
        })
        .map(Self)
        .map_err(|error| DataSyncError::validation(format!("invalid sync filter: {error}")))
    }

    fn payload(&self) -> Result<SyncSyncSourceFilterPayload, DataSyncError> {
        serde_json::from_value(self.0.clone())
            .map_err(|error| DataSyncError::validation(format!("invalid sync filter: {error}")))
    }

    pub fn validate(&self, schema: &TableSchema) -> Result<(), DataSyncError> {
        let payload = self.payload()?;
        if payload.filters.len() > MAX_CONDITIONS {
            return Err(DataSyncError::validation(format!(
                "sync filter supports at most {MAX_CONDITIONS} conditions"
            )));
        }
        for condition in &payload.filters {
            if condition.column.trim().is_empty() {
                return Err(DataSyncError::validation("sync filter column is required"));
            }
            if !schema
                .columns
                .iter()
                .any(|column| column.name == condition.column)
            {
                return Err(DataSyncError::validation(format!(
                    "sync filter column '{}' is not present in the source table",
                    condition.column
                )));
            }
            validate_condition(condition)?;
        }
        if let Some(recordset) = payload.recordset.as_ref() {
            validate_recordset(recordset, schema)?;
        }
        Ok(())
    }

    /// Validate tuple endpoints with the driver's own key normalizer before a
    /// comparison plan is issued.
    pub fn validate_tuple_range_order<F>(&self, normalize: F) -> Result<(), DataSyncError>
    where
        F: FnMut(&str, &Value) -> Result<datazen_driver_api::SyncKeyValue, String>,
    {
        let Some(recordset) = self.payload()?.recordset else {
            return Ok(());
        };
        validate_tuple_range_order(&recordset, normalize)
    }

    /// Return the validated recordset limit for the live keyset source.
    pub fn recordset_limit(&self, schema: &TableSchema) -> Result<Option<u64>, DataSyncError> {
        self.payload()?
            .recordset
            .as_ref()
            .map(|recordset| recordset_limit(recordset, schema))
            .transpose()
            .map(|limit| limit.flatten())
    }

    pub fn is_empty(&self) -> Result<bool, DataSyncError> {
        let payload = self.payload()?;
        Ok(payload.filters.is_empty() && payload.recordset.is_none())
    }

    pub fn has_tuple_range(&self) -> Result<bool, DataSyncError> {
        Ok(self
            .payload()?
            .recordset
            .is_some_and(|recordset| recordset.tuple_range.is_some()))
    }

    /// Columns whose filter predicates compare values or define a range.
    /// Null checks and limit-only recordsets do not depend on collation.
    pub fn comparison_columns(&self, schema: &TableSchema) -> Result<Vec<String>, DataSyncError> {
        let payload = self.payload()?;
        let mut columns = Vec::new();
        for condition in payload.filters {
            if !matches!(
                condition.operator,
                FilterOperator::IsNull | FilterOperator::IsNotNull
            ) {
                columns.push(condition.column);
            }
        }
        if let Some(recordset) = payload.recordset {
            columns.extend(recordset_comparison_columns(&recordset, schema)?);
        }
        let mut seen = std::collections::HashSet::new();
        columns.retain(|column| seen.insert(column.clone()));
        Ok(columns)
    }

    /// Build a `WHERE ...` fragment and the values bound to its placeholders.
    pub fn build_where<P>(
        &self,
        quote: char,
        start_index: usize,
        placeholder: P,
    ) -> Result<(Option<String>, Vec<Value>), DataSyncError>
    where
        P: FnMut(usize, Option<&str>) -> Result<String, DataSyncError>,
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
        placeholder: P,
    ) -> Result<(Option<String>, Vec<Value>), DataSyncError>
    where
        P: FnMut(usize, Option<&str>) -> Result<String, DataSyncError>,
        F: Fn(&str) -> Option<String>,
    {
        self.build_where_typed_with_default_order(
            quote,
            start_index,
            None,
            column_type,
            placeholder,
        )
    }

    /// Build the same predicate while allowing the live keyset source to
    /// supply the one effective primary-key column used by an omitted
    /// `recordset.orderBy`.
    pub fn build_where_typed_with_default_order<P, F>(
        &self,
        quote: char,
        start_index: usize,
        default_order: Option<&str>,
        column_type: F,
        placeholder: P,
    ) -> Result<(Option<String>, Vec<Value>), DataSyncError>
    where
        P: FnMut(usize, Option<&str>) -> Result<String, DataSyncError>,
        F: Fn(&str) -> Option<String>,
    {
        self.build_where_typed_with_key_order(
            quote,
            start_index,
            default_order,
            None,
            |_, value| Ok(value.clone()),
            column_type,
            placeholder,
        )
    }

    /// Build the recordset predicate using the driver's exact key ordering
    /// expressions. The supplied expression list must follow `key_columns`;
    /// key values are transformed with the same adapter used by keyset seek.
    pub fn build_where_typed_with_key_order<P, F, N>(
        &self,
        quote: char,
        start_index: usize,
        default_order: Option<&str>,
        key_order: Option<(&[String], &[String])>,
        normalize_key_value: N,
        column_type: F,
        mut placeholder: P,
    ) -> Result<(Option<String>, Vec<Value>), DataSyncError>
    where
        P: FnMut(usize, Option<&str>) -> Result<String, DataSyncError>,
        F: Fn(&str) -> Option<String>,
        N: Fn(&str, &Value) -> Result<Value, DataSyncError>,
    {
        let payload = self.payload()?;
        if payload.filters.is_empty() && payload.recordset.is_none() {
            return Ok((None, Vec::new()));
        }
        let mut params = Vec::new();
        let mut parts = Vec::with_capacity(payload.filters.len() + 2);
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
                        .collect::<Result<Vec<_>, DataSyncError>>()?;
                    format!("({column} IN ({}))", markers.join(", "))
                }
                FilterOperator::IsNull => format!("({column} IS NULL)"),
                FilterOperator::IsNotNull => format!("({column} IS NOT NULL)"),
            };
            parts.push(part);
        }

        if let Some(recordset) = payload.recordset.as_ref() {
            let (recordset_parts, recordset_params) = build_predicate(
                recordset,
                quote,
                next,
                default_order,
                key_order,
                &normalize_key_value,
                &column_type,
                &mut placeholder,
            )?;
            parts.extend(recordset_parts);
            params.extend(recordset_params);
        }

        if parts.is_empty() {
            return Ok((None, params));
        }

        if payload.recordset.is_some() && !payload.filters.is_empty() {
            let filter_count = payload.filters.len();
            let filter_joiner = match payload.logic {
                SyncFilterLogic::And => " AND ",
                SyncFilterLogic::Or => " OR ",
            };
            let filter_sql = parts[..filter_count].join(filter_joiner);
            let recordset_sql = parts[filter_count..].join(" AND ");
            return Ok((
                Some(format!("WHERE ({filter_sql}) AND {recordset_sql}")),
                params,
            ));
        }
        let joiner = match payload.logic {
            SyncFilterLogic::And => " AND ",
            SyncFilterLogic::Or => " OR ",
        };
        Ok((Some(format!("WHERE {}", parts.join(joiner))), params))
    }

    /// Human-readable preview with anonymous placeholders. Values remain
    /// private to the server and are never interpolated into preview SQL.
    pub fn preview_where(&self, quote: char) -> Result<Option<String>, DataSyncError> {
        self.build_where(quote, 1, |_, _| Ok("?".into()))
            .map(|(sql, _)| sql)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncSyncSourceFilterPayloadOwned {
    filters: Vec<SyncSyncSourceFilterCondition>,
    logic: SyncFilterLogic,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recordset: Option<SyncRecordset>,
}

fn validate_condition(condition: &SyncSyncSourceFilterCondition) -> Result<(), DataSyncError> {
    match condition.operator {
        FilterOperator::IsNull | FilterOperator::IsNotNull => Ok(()),
        FilterOperator::In => {
            if in_values(&condition.value)?.is_empty() {
                Err(DataSyncError::validation(
                    "sync filter IN requires at least one value",
                ))
            } else {
                Ok(())
            }
        }
        _ => {
            if condition.value.is_null()
                || matches!(&condition.value, serde_json::Value::String(value) if value.is_empty())
            {
                return Err(DataSyncError::validation(
                    "sync filter condition requires a value",
                ));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
