//! Table ChangeSet: types, canonicalization, plan building and fingerprinting.
//!
//! Shared by every host: the IPC adapter only fetches the connection profile,
//! resolves a driver and hands over catalog/runtime handles. Keeping the plan,
//! the guards and the fingerprint payload here is what makes a future web host
//! produce byte-identical plans instead of a second implementation.

use crate::error::FeatureError;
use datazen_driver_api::{BoundSqlStatement, DatabaseDriver, Value};
use datazen_runtime::schema_cache::CachedColumns;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

use super::{insert_plan, row_identity};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowChangeTableContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub driver_type: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRowChange {
    pub row_identity: BTreeMap<String, Option<Value>>,
    pub original_values: BTreeMap<String, Option<Value>>,
    pub current_values: BTreeMap<String, Option<Value>>,
    pub changed_columns: Vec<String>,
    pub delete_marked: bool,
    /// Marks this entry as a **new row draft** rather than a change to an
    /// existing row. Such a row has no identity yet, so it skips the primary-key
    /// checks the other entries must pass: `changed_columns` is the exact
    /// INSERT column list and a column absent from it keeps the database
    /// default (auto-increment, generated, `DEFAULT now()`). `None` in
    /// `current_values` still means SQL `NULL`, so the three intents
    /// (value / NULL / default) survive the trip. It is never serialized as
    /// `false`, which keeps fingerprints of insert-free plans byte-identical
    /// to the previous payload.
    #[serde(default, skip_serializing_if = "is_false")]
    pub insert_marked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedStatement {
    pub row_identity: BTreeMap<String, Option<Value>>,
    pub original_values: BTreeMap<String, Option<Value>>,
    pub current_values: BTreeMap<String, Option<Value>>,
    pub changed_columns: Vec<String>,
    /// Driver-rendered statement preview; it is not a wire-level SQL audit.
    pub sql_template: String,
    /// Human-readable value summaries for the preview UI.
    pub parameter_summary: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeWarning {
    pub code: String,
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowChangePlan {
    pub plan_id: String,
    pub fingerprint: String,
    #[serde(default)]
    pub config_revision: Option<datazen_platform_api::id::Counter>,
    pub table: RowChangeTableContext,
    pub updates: Vec<PlannedStatement>,
    pub deletes: Vec<PlannedStatement>,
    /// New-row statements in **request order** — the user's visual draft order,
    /// which is also the execution order. Never re-sorted: insert order decides
    /// auto-increment allocation. `#[serde(default)]` only keeps clients that do
    /// not send the field deserializable; serialization always emits it.
    #[serde(default)]
    pub inserts: Vec<PlannedStatement>,
    pub warnings: Vec<ChangeWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitPendingChangesRequest {
    pub db_session_id: String,
    pub plan: RowChangePlan,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowCommitStatementResult {
    pub operation: String,
    pub row_identity: BTreeMap<String, Option<Value>>,
    pub affected_rows: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitPendingChangesResponse {
    pub plan_id: String,
    pub fingerprint: String,
    pub statements: Vec<RowCommitStatementResult>,
    pub affected_rows: u64,
}

pub fn canonicalize_changes(
    changes: &[PendingRowChange],
) -> Result<Vec<PendingRowChange>, FeatureError> {
    let mut canonical = Vec::with_capacity(changes.len());
    // New rows have no identity yet, so they cannot be sorted or keyed. They
    // keep request order (execution order) and land after the identity-sorted
    // changes; canonicalizing the rebuilt plan produces the same partition, so
    // fingerprints stay reconstructible.
    let mut inserts = Vec::new();
    for change in changes {
        if change.insert_marked {
            inserts.push(insert_plan::canonicalize_insert(change)?);
            continue;
        }
        let _ = identity_key(&change.row_identity)?;
        let mut normalized = change.clone();
        normalized.changed_columns.sort();
        normalized.changed_columns.dedup();
        if !normalized.delete_marked && normalized.changed_columns.is_empty() {
            continue;
        }
        if !normalized.delete_marked {
            for column in &normalized.changed_columns {
                if !normalized.current_values.contains_key(column) {
                    return Err(FeatureError::Validation(format!(
                        "Missing current value for changed column '{column}'"
                    )));
                }
            }
        }
        let _ = identity_key(&effective_identity(&normalized)?)?;
        canonical.push(normalized);
    }
    canonical.sort_by(|a, b| {
        match (identity_key(&a.row_identity), identity_key(&b.row_identity)) {
            (Ok(ka), Ok(kb)) => ka.cmp(&kb),
            _ => std::cmp::Ordering::Equal,
        }
    });

    let mut original_keys = HashSet::new();
    let mut current_keys = HashMap::new();
    for change in &canonical {
        let original_key = identity_key(&change.row_identity)?;
        if !original_keys.insert(original_key.clone()) {
            return Err(FeatureError::Validation(
                "Row changes contain duplicate primary-key identity".into(),
            ));
        }
        let current_key = identity_key(&effective_identity(change)?)?;
        if current_keys.insert(current_key, original_key).is_some() {
            return Err(FeatureError::Validation(
                "Row changes contain a primary-key identity collision".into(),
            ));
        }
    }
    canonical.extend(inserts);
    Ok(canonical)
}

pub fn identity_key(identity: &BTreeMap<String, Option<Value>>) -> Result<String, FeatureError> {
    if identity.is_empty() {
        return Err(FeatureError::Validation(
            "Row changes require primary-key identity".into(),
        ));
    }
    for (column, value) in identity {
        if column.trim().is_empty() || value.is_none() || matches!(value, Some(Value::Null)) {
            return Err(FeatureError::Validation(
                "Row identity must contain non-NULL primary-key values".into(),
            ));
        }
    }
    serde_json::to_string(identity).map_err(FeatureError::Json)
}

pub fn effective_identity(
    change: &PendingRowChange,
) -> Result<BTreeMap<String, Option<Value>>, FeatureError> {
    let mut identity = change.row_identity.clone();
    for column in change.row_identity.keys() {
        if let Some(value) = change.current_values.get(column) {
            identity.insert(column.clone(), value.clone());
        }
    }
    Ok(identity)
}

pub fn validate_table_context(table: &RowChangeTableContext) -> Result<(), FeatureError> {
    if table.connection_id.trim().is_empty()
        || table.db_session_id.trim().is_empty()
        || table.driver_type.trim().is_empty()
        || table
            .database
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        || table.table.trim().is_empty()
    {
        return Err(FeatureError::Validation(
            "Row change context is incomplete; connection, session, driver, database and table are required".into(),
        ));
    }
    Ok(())
}

pub fn changes_fingerprint(
    table: &RowChangeTableContext,
    changes: &[PendingRowChange],
) -> Result<String, FeatureError> {
    // Inserts are part of `changes` (marked by `insert_marked`), so their
    // written values, NULL-vs-unwritten distinction and relative order are all
    // covered. The marker itself only serializes when true (`is_false`), which
    // keeps insert-free fingerprints byte-identical to the pre-INSERT payload.
    #[derive(Serialize)]
    struct FingerprintPayload<'a> {
        table: &'a RowChangeTableContext,
        changes: &'a [PendingRowChange],
    }

    let payload = FingerprintPayload { table, changes };
    let bytes = serde_json::to_vec(&payload).map_err(FeatureError::Json)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// serde helper for [`PendingRowChange::insert_marked`].
fn is_false(value: &bool) -> bool {
    !*value
}

pub(crate) fn value_summary(value: &Option<Value>) -> String {
    let rendered = match value {
        None => "NULL".to_string(),
        Some(value) => serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".into()),
    };
    let mut chars = rendered.chars();
    let truncated: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

pub fn build_row_change_plan(
    driver: &dyn DatabaseDriver,
    table: RowChangeTableContext,
    changes: &[PendingRowChange],
    columns: &CachedColumns,
) -> Result<RowChangePlan, FeatureError> {
    validate_table_context(&table)?;
    row_identity::validate_identity_shapes(changes)?;
    for change in changes {
        let operation = if change.insert_marked {
            "INSERT"
        } else if change.delete_marked {
            "DELETE"
        } else {
            "UPDATE"
        };
        // A delete and a staged column write are contradictory intents: the
        // row is going away, so silently dropping the writes would tell the
        // user their edit was applied when it never ran.
        if change.delete_marked && !change.changed_columns.is_empty() {
            return Err(FeatureError::Validation(
                "grid.commit.conflictingIntents: a row marked for delete cannot also carry column writes; unmark the deletion or clear the staged updates".into(),
            ));
        }
        row_identity::validate_written_columns(operation, change, &table, columns)?;
        if !change.insert_marked {
            row_identity::validate_identity_against_primary_key(
                operation, change, &table, columns,
            )?;
        }
    }
    let canonical = canonicalize_changes(changes)?;
    let fingerprint = changes_fingerprint(&table, &canonical)?;
    let mut updates = Vec::new();
    let mut deletes = Vec::new();
    let mut inserts = Vec::new();
    let mut warnings = Vec::new();

    for change in canonical {
        if change.insert_marked {
            inserts.push(insert_plan::planned_insert_statement(
                driver,
                &table.table,
                &change,
            )?);
            continue;
        }
        let pk_columns: Vec<(&str, Option<Value>)> = change
            .row_identity
            .iter()
            .map(|(column, value)| (column.as_str(), value.clone()))
            .collect();
        let set_columns: Vec<(&str, Option<Value>)> = change
            .changed_columns
            .iter()
            .filter_map(|column| {
                change
                    .current_values
                    .get(column)
                    .cloned()
                    .map(|value| (column.as_str(), value))
            })
            .collect();
        let parameter_summary = change
            .changed_columns
            .iter()
            .filter_map(|column| {
                change
                    .current_values
                    .get(column)
                    .map(|value| format!("SET {column}={}", value_summary(value)))
            })
            .chain(
                change
                    .row_identity
                    .iter()
                    .map(|(column, value)| format!("PK {column}={}", value_summary(value))),
            )
            .collect();
        let sql_template = if change.delete_marked {
            driver
                .build_delete_statement(&table.table, &pk_columns)
                .map_err(FeatureError::Driver)?
                .sql
        } else {
            driver
                .build_update_statement(&table.table, &set_columns, &pk_columns)
                .map_err(FeatureError::Driver)?
                .sql
        };
        let statement = PlannedStatement {
            row_identity: change.row_identity.clone(),
            original_values: change.original_values.clone(),
            current_values: change.current_values.clone(),
            changed_columns: change.changed_columns.clone(),
            sql_template,
            parameter_summary,
        };

        if change.delete_marked {
            deletes.push(statement);
        } else {
            updates.push(statement);
        }
    }

    if !deletes.is_empty() {
        warnings.push(ChangeWarning {
            code: "delete-rows".into(),
            message: format!("{} row(s) will be deleted", deletes.len()),
            severity: "warning".into(),
        });
    }

    if !inserts.is_empty() {
        warnings.push(ChangeWarning {
            code: "insert-rows".into(),
            message: format!("{} new row(s) will be inserted", inserts.len()),
            severity: "info".into(),
        });
        // The host must not guess a new row's primary key. When the driver has
        // no returning seam the page is refreshed instead of highlighting the
        // new rows; say so up front instead of failing after the commit.
        let primary_keys: Vec<&str> = columns.primary_keys.iter().map(String::as_str).collect();
        if driver.insert_returning_clause(&primary_keys).is_none() {
            warnings.push(ChangeWarning {
                code: "grid.insert.returningUnavailable".into(),
                message: format!(
                    "{} new row(s) will be inserted, but this driver cannot return their primary keys; the page will be refreshed after commit instead of highlighting the new rows",
                    inserts.len()
                ),
                severity: "warning".into(),
            });
        }
    }

    Ok(RowChangePlan {
        plan_id: Uuid::new_v4().to_string(),
        fingerprint,
        config_revision: None,
        table,
        updates,
        deletes,
        inserts,
        warnings,
    })
}

pub fn plan_changes(plan: &RowChangePlan) -> Vec<PendingRowChange> {
    plan.updates
        .iter()
        .map(|statement| PendingRowChange {
            row_identity: statement.row_identity.clone(),
            original_values: statement.original_values.clone(),
            current_values: statement.current_values.clone(),
            changed_columns: statement.changed_columns.clone(),
            delete_marked: false,
            insert_marked: false,
        })
        .chain(plan.deletes.iter().map(|statement| PendingRowChange {
            row_identity: statement.row_identity.clone(),
            original_values: statement.original_values.clone(),
            current_values: statement.current_values.clone(),
            changed_columns: statement.changed_columns.clone(),
            delete_marked: true,
            insert_marked: false,
        }))
        .chain(plan.inserts.iter().map(|statement| PendingRowChange {
            // A new row's identity is only known after execution; the plan
            // never carries one, so the rebuild must not invent it either.
            row_identity: BTreeMap::new(),
            original_values: statement.original_values.clone(),
            current_values: statement.current_values.clone(),
            changed_columns: statement.changed_columns.clone(),
            delete_marked: false,
            insert_marked: true,
        }))
        .collect()
}

pub fn validate_immutable_plan(
    driver: &dyn DatabaseDriver,
    db_session_id: &str,
    plan: &RowChangePlan,
    fingerprint: &str,
    columns: &CachedColumns,
) -> Result<RowChangePlan, FeatureError> {
    if plan.plan_id.trim().is_empty() {
        return Err(FeatureError::Validation(
            "Row change plan id is required".into(),
        ));
    }
    if plan.table.db_session_id != db_session_id {
        return Err(FeatureError::Validation(
            "Row change plan belongs to a different database session".into(),
        ));
    }
    if plan.fingerprint != fingerprint {
        return Err(FeatureError::Validation(
            "grid.commit.stalePlan: Row change plan fingerprint does not match the confirmed plan"
                .into(),
        ));
    }
    // Rebuild all SQL from the immutable change metadata. The client-provided
    // SQL/summary fields are display-only and are never trusted for execution.
    let mut rebuilt =
        build_row_change_plan(driver, plan.table.clone(), &plan_changes(plan), columns)?;
    if rebuilt.fingerprint != fingerprint {
        return Err(FeatureError::Validation(
            "grid.commit.stalePlan: Row change plan fingerprint is stale or was modified".into(),
        ));
    }
    rebuilt.plan_id = plan.plan_id.clone();
    rebuilt.config_revision = plan.config_revision;
    Ok(rebuilt)
}

pub fn bound_statement_for_plan(
    driver: &dyn DatabaseDriver,
    table: &str,
    planned: &PlannedStatement,
    delete: bool,
) -> Result<BoundSqlStatement, FeatureError> {
    let pk_columns: Vec<(&str, Option<Value>)> = planned
        .row_identity
        .iter()
        .map(|(column, value)| (column.as_str(), value.clone()))
        .collect();
    if delete {
        return driver
            .build_delete_statement(table, &pk_columns)
            .map_err(FeatureError::Driver);
    }

    let set_columns: Vec<(&str, Option<Value>)> = planned
        .changed_columns
        .iter()
        .map(|column| {
            planned
                .current_values
                .get(column)
                .cloned()
                .map(|value| (column.as_str(), value))
                .ok_or_else(|| {
                    FeatureError::Validation(format!(
                        "Missing current value for changed column '{column}'"
                    ))
                })
        })
        .collect::<Result<_, _>>()?;
    driver
        .build_update_statement(table, &set_columns, &pk_columns)
        .map_err(FeatureError::Driver)
}
