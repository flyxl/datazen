use super::error::CommandError;
use super::AppState;
use crate::db::{BoundSqlStatement, DriverError, Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use tauri::State;
use uuid::Uuid;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellUpdate {
    pub column: String,
    pub value: Option<Value>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowUpdateBatch {
    pub set_columns: Vec<CellUpdate>,
    pub pk_columns: Vec<CellUpdate>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowDeleteBatch {
    pub pk_columns: Vec<CellUpdate>,
}

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

fn canonicalize_changes(
    changes: &[PendingRowChange],
) -> Result<Vec<PendingRowChange>, CommandError> {
    let mut canonical = Vec::with_capacity(changes.len());
    for change in changes {
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
                    return Err(CommandError::Validation(format!(
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
            return Err(CommandError::Validation(
                "Row changes contain duplicate primary-key identity".into(),
            ));
        }
        let current_key = identity_key(&effective_identity(change)?)?;
        if current_keys.insert(current_key, original_key).is_some() {
            return Err(CommandError::Validation(
                "Row changes contain a primary-key identity collision".into(),
            ));
        }
    }
    Ok(canonical)
}

fn identity_key(identity: &BTreeMap<String, Option<Value>>) -> Result<String, CommandError> {
    if identity.is_empty() {
        return Err(CommandError::Validation(
            "Row changes require primary-key identity".into(),
        ));
    }
    for (column, value) in identity {
        if column.trim().is_empty() || value.is_none() || matches!(value, Some(Value::Null)) {
            return Err(CommandError::Validation(
                "Row identity must contain non-NULL primary-key values".into(),
            ));
        }
    }
    serde_json::to_string(identity).map_err(CommandError::Json)
}

fn effective_identity(
    change: &PendingRowChange,
) -> Result<BTreeMap<String, Option<Value>>, CommandError> {
    let mut identity = change.row_identity.clone();
    for column in change.row_identity.keys() {
        if let Some(value) = change.current_values.get(column) {
            identity.insert(column.clone(), value.clone());
        }
    }
    Ok(identity)
}

pub(super) fn validate_table_context(table: &RowChangeTableContext) -> Result<(), CommandError> {
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
        return Err(CommandError::Validation(
            "Row change context is incomplete; connection, session, driver, database and table are required".into(),
        ));
    }
    Ok(())
}

fn validate_legacy_pk_columns(columns: &[CellUpdate], operation: &str) -> Result<(), CommandError> {
    if columns.is_empty() {
        return Err(CommandError::Validation(format!(
            "{operation} requires primary-key columns"
        )));
    }
    let mut names = HashSet::new();
    for column in columns {
        if column.column.trim().is_empty()
            || column.value.is_none()
            || matches!(column.value, Some(Value::Null))
            || !names.insert(column.column.as_str())
        {
            return Err(CommandError::Validation(format!(
                "{operation} requires unique non-NULL primary-key values"
            )));
        }
    }
    Ok(())
}

fn changes_fingerprint(
    table: &RowChangeTableContext,
    changes: &[PendingRowChange],
) -> Result<String, CommandError> {
    #[derive(Serialize)]
    struct FingerprintPayload<'a> {
        table: &'a RowChangeTableContext,
        changes: &'a [PendingRowChange],
    }

    let payload = FingerprintPayload { table, changes };
    let bytes = serde_json::to_vec(&payload).map_err(CommandError::Json)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn value_summary(value: &Option<Value>) -> String {
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

fn build_row_change_plan(
    driver: &dyn crate::db::DatabaseDriver,
    table: RowChangeTableContext,
    changes: &[PendingRowChange],
) -> Result<RowChangePlan, CommandError> {
    validate_table_context(&table)?;
    let canonical = canonicalize_changes(changes)?;
    let fingerprint = changes_fingerprint(&table, &canonical)?;
    let mut updates = Vec::new();
    let mut deletes = Vec::new();
    let mut warnings = Vec::new();

    for change in canonical {
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
                .map_err(CommandError::Driver)?
                .sql
        } else {
            driver
                .build_update_statement(&table.table, &set_columns, &pk_columns)
                .map_err(CommandError::Driver)?
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
            if !change.changed_columns.is_empty() {
                warnings.push(ChangeWarning {
                    code: "delete-suppresses-update".into(),
                    message: "A deleted row's staged updates will not be executed".into(),
                    severity: "warning".into(),
                });
            }
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

    Ok(RowChangePlan {
        plan_id: Uuid::new_v4().to_string(),
        fingerprint,
        config_revision: None,
        table,
        updates,
        deletes,
        warnings,
    })
}

fn plan_changes(plan: &RowChangePlan) -> Vec<PendingRowChange> {
    plan.updates
        .iter()
        .map(|statement| PendingRowChange {
            row_identity: statement.row_identity.clone(),
            original_values: statement.original_values.clone(),
            current_values: statement.current_values.clone(),
            changed_columns: statement.changed_columns.clone(),
            delete_marked: false,
        })
        .chain(plan.deletes.iter().map(|statement| PendingRowChange {
            row_identity: statement.row_identity.clone(),
            original_values: statement.original_values.clone(),
            current_values: statement.current_values.clone(),
            changed_columns: statement.changed_columns.clone(),
            delete_marked: true,
        }))
        .collect()
}

fn validate_immutable_plan(
    driver: &dyn crate::db::DatabaseDriver,
    db_session_id: &str,
    plan: &RowChangePlan,
    fingerprint: &str,
) -> Result<RowChangePlan, CommandError> {
    if plan.plan_id.trim().is_empty() {
        return Err(CommandError::Validation(
            "Row change plan id is required".into(),
        ));
    }
    if plan.table.db_session_id != db_session_id {
        return Err(CommandError::Validation(
            "Row change plan belongs to a different database session".into(),
        ));
    }
    if plan.fingerprint != fingerprint {
        return Err(CommandError::Validation(
            "Row change plan fingerprint does not match the confirmed plan".into(),
        ));
    }
    // Rebuild all SQL from the immutable change metadata. The client-provided
    // SQL/summary fields are display-only and are never trusted for execution.
    let mut rebuilt = build_row_change_plan(driver, plan.table.clone(), &plan_changes(plan))?;
    if rebuilt.fingerprint != fingerprint {
        return Err(CommandError::Validation(
            "Row change plan fingerprint is stale or was modified".into(),
        ));
    }
    rebuilt.plan_id = plan.plan_id.clone();
    rebuilt.config_revision = plan.config_revision;
    Ok(rebuilt)
}

pub(super) fn bound_statement_for_plan(
    driver: &dyn crate::db::DatabaseDriver,
    table: &str,
    planned: &PlannedStatement,
    delete: bool,
) -> Result<BoundSqlStatement, CommandError> {
    let pk_columns: Vec<(&str, Option<Value>)> = planned
        .row_identity
        .iter()
        .map(|(column, value)| (column.as_str(), value.clone()))
        .collect();
    if delete {
        return driver
            .build_delete_statement(table, &pk_columns)
            .map_err(CommandError::Driver);
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
                    CommandError::Validation(format!(
                        "Missing current value for changed column '{column}'"
                    ))
                })
        })
        .collect::<Result<_, _>>()?;
    driver
        .build_update_statement(table, &set_columns, &pk_columns)
        .map_err(CommandError::Driver)
}

pub(crate) async fn preview_pending_changes_impl(
    state: &AppState,
    table: RowChangeTableContext,
    changes: Vec<PendingRowChange>,
) -> Result<RowChangePlan, CommandError> {
    tracing::info!(db_session_id = %table.db_session_id, table = %table.table, change_count = changes.len(), "preview_pending_changes");
    let (config, metadata) = state
        .store
        .platform_profile(&table.connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection profile unavailable".into()))?;
    if !metadata.enabled || config.database_type != table.driver_type {
        return Err(CommandError::Validation(
            "Table connection context changed".into(),
        ));
    }
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| CommandError::NotConfigured("Driver unavailable".into()))?;
    let mut plan = build_row_change_plan(driver.as_ref(), table, &changes)?;
    plan.config_revision = Some(datazen_platform_api::id::Counter::new(
        metadata.config_revision,
    ));
    Ok(plan)
}

#[tauri::command]
pub async fn preview_pending_changes(
    state: State<'_, AppState>,
    context: RowChangeTableContext,
    changes: Vec<PendingRowChange>,
) -> Result<RowChangePlan, CommandError> {
    preview_pending_changes_impl(&state, context, changes).await
}

pub(crate) async fn commit_pending_changes_impl(
    state: &AppState,
    request: CommitPendingChangesRequest,
) -> Result<CommitPendingChangesResponse, CommandError> {
    commit_pending_changes_with_handle(state, request, None).await
}

async fn commit_pending_changes_with_handle(
    state: &AppState,
    request: CommitPendingChangesRequest,
    manual_handle: Option<datazen_platform_api::dto::session::SessionHandle>,
) -> Result<CommitPendingChangesResponse, CommandError> {
    tracing::info!(
        db_session_id = %request.db_session_id,
        plan_id = %request.plan.plan_id,
        "commit_pending_changes"
    );
    validate_table_context(&request.plan.table)?;
    let (config, metadata) = state
        .store
        .platform_profile(&request.plan.table.connection_id)
        .await
        .ok_or_else(|| CommandError::NotFound("Connection profile unavailable".into()))?;
    if !metadata.enabled || config.database_type != request.plan.table.driver_type {
        return Err(CommandError::Validation(
            "Table connection context changed".into(),
        ));
    }
    if request.plan.config_revision
        != Some(datazen_platform_api::id::Counter::new(
            metadata.config_revision,
        ))
    {
        return Err(CommandError::Validation(
            "Connection configuration changed; preview the changes again".into(),
        ));
    }
    if config.read_only {
        return Err(CommandError::Validation(
            "Connection is read-only; row changes are not allowed".into(),
        ));
    }
    let driver = state
        .driver_registry
        .get_sql_driver_by_name(config.database_type.as_str())
        .await
        .ok_or_else(|| CommandError::NotConfigured("Driver unavailable".into()))?;
    let plan = validate_immutable_plan(
        driver.as_ref(),
        &request.db_session_id,
        &request.plan,
        &request.fingerprint,
    )?;
    super::data_session::execute_plan(state, driver.as_ref(), &plan, manual_handle).await
}

#[tauri::command]
pub async fn commit_pending_changes(
    state: State<'_, AppState>,
    db_session_id: String,
    manual_handle: Option<datazen_platform_api::dto::session::SessionHandle>,
    plan: RowChangePlan,
    fingerprint: String,
) -> Result<CommitPendingChangesResponse, CommandError> {
    commit_pending_changes_with_handle(
        &state,
        CommitPendingChangesRequest {
            db_session_id,
            plan,
            fingerprint,
        },
        manual_handle,
    )
    .await
}

pub(crate) async fn commit_row_updates_impl(
    _state: &AppState,
    _db_session_id: String,
    _table: String,
    _updates: Vec<RowUpdateBatch>,
) -> Result<(), CommandError> {
    // Legacy writes contain no provenance, config revision or runtime binding.
    // Reject rather than silently joining a connection-tree transaction.
    Err(CommandError::Validation(
        "Legacy row writes are disabled; preview and commit a target-bound ChangeSet".into(),
    ))
}

#[tauri::command]
pub async fn commit_row_updates(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    updates: Vec<RowUpdateBatch>,
) -> Result<(), CommandError> {
    commit_row_updates_impl(&state, db_session_id, table, updates).await
}

pub(crate) async fn commit_row_deletes_impl(
    _state: &AppState,
    _db_session_id: String,
    _table: String,
    _deletes: Vec<RowDeleteBatch>,
) -> Result<(), CommandError> {
    // Legacy writes contain no provenance, config revision or runtime binding.
    // Reject rather than silently joining a connection-tree transaction.
    Err(CommandError::Validation(
        "Legacy row writes are disabled; preview and commit a target-bound ChangeSet".into(),
    ))
}

#[tauri::command]
pub async fn commit_row_deletes(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    deletes: Vec<RowDeleteBatch>,
) -> Result<(), CommandError> {
    commit_row_deletes_impl(&state, db_session_id, table, deletes).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state::TestAppState;

    async fn with_parameterized_driver() -> TestAppState {
        let mut options = crate::testing::app_state::rich_mock_options();
        options.parameterized_writes = true;
        TestAppState::with_options(options).await
    }

    fn table_context(connection_id: &str, db_session_id: &str) -> RowChangeTableContext {
        RowChangeTableContext {
            connection_id: connection_id.into(),
            db_session_id: db_session_id.into(),
            driver_type: "postgres".into(),
            database: Some("app".into()),
            schema: None,
            table: "users".into(),
        }
    }

    fn update_change(id: i64, original: &str, current: &str) -> PendingRowChange {
        PendingRowChange {
            row_identity: BTreeMap::from([("id".into(), Some(Value::Integer(id)))]),
            original_values: BTreeMap::from([(
                "name".into(),
                Some(Value::String(original.into())),
            )]),
            current_values: BTreeMap::from([("name".into(), Some(Value::String(current.into())))]),
            changed_columns: vec!["name".into()],
            delete_marked: false,
        }
    }

    fn delete_change(id: i64) -> PendingRowChange {
        PendingRowChange {
            row_identity: BTreeMap::from([("id".into(), Some(Value::Integer(id)))]),
            original_values: BTreeMap::new(),
            current_values: BTreeMap::new(),
            changed_columns: Vec::new(),
            delete_marked: true,
        }
    }

    fn composite_update_change(tenant_id: i64, id: i64, next_id: Option<i64>) -> PendingRowChange {
        let mut current_values =
            BTreeMap::from([("name".into(), Some(Value::String("Updated".into())))]);
        let mut changed_columns = vec!["name".into()];
        if let Some(next_id) = next_id {
            current_values.insert("id".into(), Some(Value::Integer(next_id)));
            changed_columns.push("id".into());
        }
        PendingRowChange {
            row_identity: BTreeMap::from([
                ("tenant_id".into(), Some(Value::Integer(tenant_id))),
                ("id".into(), Some(Value::Integer(id))),
            ]),
            original_values: BTreeMap::new(),
            current_values,
            changed_columns,
            delete_marked: false,
        }
    }

    #[tokio::test]
    async fn preview_builds_driver_sql_without_opening_a_transaction() {
        let test = with_parameterized_driver().await;
        let (_, conn_id) = test.save_and_connect("preview-only").await;
        let plan = preview_pending_changes_impl(
            &test.state,
            table_context("preview-only", &conn_id),
            vec![update_change(1, "Alice", "Updated")],
        )
        .await
        .unwrap();

        assert_eq!(plan.table.db_session_id, conn_id);
        assert_eq!(plan.updates.len(), 1);
        assert!(plan.updates[0].sql_template.contains("UPDATE \"users\""));
        assert!(plan.updates[0].sql_template.contains("\"id\" = ?2"));
        assert!(!plan.updates[0].sql_template.contains("Updated"));
        assert!(!crate::commands::session_transaction_status_impl(
            &test.state,
            plan.table.db_session_id.clone()
        )
        .await
        .unwrap());
    }

    #[tokio::test]
    async fn commit_reuses_plan_fingerprint_and_returns_plan_id() {
        let mut options = crate::testing::app_state::rich_mock_options();
        options.execute_rows_affected = 1;
        options.parameterized_writes = true;
        let test = TestAppState::with_options(options).await;
        let (_, conn_id) = test.save_and_connect("commit-plan").await;
        let plan = preview_pending_changes_impl(
            &test.state,
            table_context("commit-plan", &conn_id),
            vec![update_change(1, "Alice", "Updated"), delete_change(2)],
        )
        .await
        .unwrap();
        let response = commit_pending_changes_impl(
            &test.state,
            CommitPendingChangesRequest {
                db_session_id: conn_id,
                fingerprint: plan.fingerprint.clone(),
                plan: plan.clone(),
            },
        )
        .await
        .unwrap();

        assert_eq!(response.plan_id, plan.plan_id);
        assert_eq!(response.fingerprint, plan.fingerprint);
        assert_eq!(response.statements.len(), 2);
        assert_eq!(response.affected_rows, 2);
    }

    #[tokio::test]
    async fn commit_rejects_stale_fingerprint_before_execution() {
        let test = with_parameterized_driver().await;
        let (_, conn_id) = test.save_and_connect("stale-plan").await;
        let plan = preview_pending_changes_impl(
            &test.state,
            table_context("stale-plan", &conn_id),
            vec![update_change(1, "Alice", "Updated")],
        )
        .await
        .unwrap();
        let error = commit_pending_changes_impl(
            &test.state,
            CommitPendingChangesRequest {
                db_session_id: conn_id.clone(),
                fingerprint: "stale".into(),
                plan,
            },
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("fingerprint"));
        assert!(
            !crate::commands::session_transaction_status_impl(&test.state, conn_id)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn preview_rejects_changes_without_primary_key_identity() {
        let test = with_parameterized_driver().await;
        let (_, conn_id) = test.save_and_connect("preview-no-pk").await;
        let error = preview_pending_changes_impl(
            &test.state,
            table_context("preview-no-pk", &conn_id),
            vec![PendingRowChange {
                row_identity: BTreeMap::new(),
                original_values: BTreeMap::new(),
                current_values: BTreeMap::from([(
                    "name".into(),
                    Some(Value::String("Updated".into())),
                )]),
                changed_columns: vec!["name".into()],
                delete_marked: false,
            }],
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("primary-key"));
    }

    #[test]
    fn canonicalize_rejects_null_and_duplicate_composite_identities() {
        let null_identity = PendingRowChange {
            row_identity: BTreeMap::from([
                ("tenant_id".into(), Some(Value::Integer(1))),
                ("id".into(), None),
            ]),
            original_values: BTreeMap::new(),
            current_values: BTreeMap::from([(
                "name".into(),
                Some(Value::String("Updated".into())),
            )]),
            changed_columns: vec!["name".into()],
            delete_marked: false,
        };
        let error = canonicalize_changes(&[null_identity]).unwrap_err();
        assert!(error.to_string().contains("non-NULL"));

        let duplicate = composite_update_change(1, 9, None);
        let error = canonicalize_changes(&[duplicate.clone(), duplicate]).unwrap_err();
        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn canonicalize_rejects_composite_current_identity_collision() {
        let first = composite_update_change(1, 9, Some(11));
        let second = composite_update_change(1, 10, Some(11));
        let error = canonicalize_changes(&[first, second]).unwrap_err();
        assert!(error.to_string().contains("collision"));
    }

    /// [tester] Regression: sort-by-identity uses validated keys without panicking.
    #[test]
    fn test_tester_canonicalize_sorts_by_identity_key() {
        let changes = vec![
            update_change(3, "c", "c"),
            update_change(1, "a", "a"),
            update_change(2, "b", "b"),
        ];
        let canonical = canonicalize_changes(&changes).unwrap();
        let ids: Vec<i64> = canonical
            .iter()
            .map(|change| match change.row_identity.get("id") {
                Some(Some(Value::Integer(id))) => *id,
                other => panic!("expected integer id, got {other:?}"),
            })
            .collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn commit_rejects_database_context_change_without_switching_session() {
        let mut options = crate::testing::app_state::rich_mock_options();
        options.execute_rows_affected = 1;
        options.parameterized_writes = true;
        let test = TestAppState::with_options(options).await;
        let (_, conn_id) = test.save_and_connect("context-database").await;
        let plan = preview_pending_changes_impl(
            &test.state,
            RowChangeTableContext {
                database: Some("other_db".into()),
                ..table_context("context-database", &conn_id)
            },
            vec![update_change(1, "Alice", "Updated")],
        )
        .await
        .unwrap();

        let error = commit_pending_changes_impl(
            &test.state,
            CommitPendingChangesRequest {
                db_session_id: conn_id,
                fingerprint: plan.fingerprint.clone(),
                plan,
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("Database context changed"));
        assert!(test.mock.use_database_calls().is_empty());
    }

    #[tokio::test]
    async fn commit_allows_concrete_table_schema_when_connection_schema_is_unspecified() {
        let mut options = crate::testing::app_state::rich_mock_options();
        options.execute_rows_affected = 1;
        options.parameterized_writes = true;
        let test = TestAppState::with_options(options).await;
        let (_, conn_id) = test.save_and_connect("context-schema-default").await;
        let plan = preview_pending_changes_impl(
            &test.state,
            RowChangeTableContext {
                schema: Some("public".into()),
                ..table_context("context-schema-default", &conn_id)
            },
            vec![update_change(1, "Alice", "Updated")],
        )
        .await
        .unwrap();

        commit_pending_changes_impl(
            &test.state,
            CommitPendingChangesRequest {
                db_session_id: conn_id,
                fingerprint: plan.fingerprint.clone(),
                plan,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn commit_rejects_explicit_schema_context_change() {
        let mut options = crate::testing::app_state::rich_mock_options();
        options.execute_rows_affected = 1;
        options.parameterized_writes = true;
        let test = TestAppState::with_options(options).await;
        let mut config =
            crate::testing::app_state::sample_postgres_config("context-schema-explicit");
        config.schema = Some("private".into());
        test.store.save_connection(config).await.unwrap();
        let conn_id = test.connect_config("context-schema-explicit").await;
        let plan = preview_pending_changes_impl(
            &test.state,
            RowChangeTableContext {
                schema: Some("public".into()),
                ..table_context("context-schema-explicit", &conn_id)
            },
            vec![update_change(1, "Alice", "Updated")],
        )
        .await
        .unwrap();

        let error = commit_pending_changes_impl(
            &test.state,
            CommitPendingChangesRequest {
                db_session_id: conn_id,
                fingerprint: plan.fingerprint.clone(),
                plan,
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("Schema context changed"));
    }

    #[tokio::test]
    async fn commit_row_updates_success() {
        let test = with_parameterized_driver().await;
        let (_, conn_id) = test.save_and_connect("data-cfg").await;
        commit_row_updates_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowUpdateBatch {
                set_columns: vec![CellUpdate {
                    column: "name".into(),
                    value: Some(Value::String("Alice".into())),
                }],
                pk_columns: vec![CellUpdate {
                    column: "id".into(),
                    value: Some(Value::Integer(1)),
                }],
            }],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn commit_row_updates_not_connected_errors() {
        let test = TestAppState::new().await;
        assert!(
            commit_row_updates_impl(&test.state, "missing".into(), "users".into(), vec![],)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn commit_row_updates_rejects_read_only_connection() {
        let test = with_parameterized_driver().await;
        let mut config = crate::testing::app_state::sample_postgres_config("ro-edit");
        config.read_only = true;
        test.store.save_connection(config).await.unwrap();
        let conn_id = test.connect_config("ro-edit").await;
        let err = commit_row_updates_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowUpdateBatch {
                set_columns: vec![CellUpdate {
                    column: "name".into(),
                    value: Some(Value::String("x".into())),
                }],
                pk_columns: vec![CellUpdate {
                    column: "id".into(),
                    value: Some(Value::Integer(1)),
                }],
            }],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn commit_row_updates_joins_open_session_transaction() {
        let test = with_parameterized_driver().await;
        let (_, conn_id) = test.save_and_connect("tx-edit").await;
        crate::commands::begin_session_transaction_impl(&test.state, conn_id.clone())
            .await
            .unwrap();
        commit_row_updates_impl(
            &test.state,
            conn_id.clone(),
            "users".into(),
            vec![RowUpdateBatch {
                set_columns: vec![CellUpdate {
                    column: "name".into(),
                    value: Some(Value::String("Bob".into())),
                }],
                pk_columns: vec![CellUpdate {
                    column: "id".into(),
                    value: Some(Value::Integer(1)),
                }],
            }],
        )
        .await
        .unwrap();
        assert!(
            crate::commands::session_transaction_status_impl(&test.state, conn_id.clone())
                .await
                .unwrap()
        );
        crate::commands::commit_session_transaction_impl(&test.state, conn_id)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn commit_row_deletes_success() {
        let test = with_parameterized_driver().await;
        let (_, conn_id) = test.save_and_connect("data-del").await;
        commit_row_deletes_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowDeleteBatch {
                pk_columns: vec![CellUpdate {
                    column: "id".into(),
                    value: Some(Value::Integer(1)),
                }],
            }],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn commit_row_deletes_rejects_read_only_connection() {
        let test = with_parameterized_driver().await;
        let mut config = crate::testing::app_state::sample_postgres_config("ro-del");
        config.read_only = true;
        test.store.save_connection(config).await.unwrap();
        let conn_id = test.connect_config("ro-del").await;
        let err = commit_row_deletes_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowDeleteBatch {
                pk_columns: vec![CellUpdate {
                    column: "id".into(),
                    value: Some(Value::Integer(1)),
                }],
            }],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn commit_row_deletes_rejects_empty_pk() {
        let test = with_parameterized_driver().await;
        let (_, conn_id) = test.save_and_connect("data-del-empty-pk").await;
        let err = commit_row_deletes_impl(
            &test.state,
            conn_id,
            "users".into(),
            vec![RowDeleteBatch { pk_columns: vec![] }],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("primary-key"));
    }
}

#[path = "table_target.rs"]
mod table_target;

#[tauri::command]
pub async fn get_table_data_at_target(
    state: State<'_, AppState>,
    request: table_target::TableReadRequest,
) -> Result<crate::db::TableDataResult, CommandError> {
    table_target::read(&state, request).await
}

#[path = "result_mapping.rs"]
mod result_mapping;
pub(crate) use result_mapping::{
    result_relation_candidate, verify_result_mapping, ResultRelationCandidate,
};
