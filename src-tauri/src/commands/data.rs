use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{DriverError, Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
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
    pub db_session_id: String,
    pub table: String,
    pub database: Option<String>,
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
        if change.row_identity.is_empty() {
            return Err(CommandError::Validation(
                "Row changes require primary-key identity".into(),
            ));
        }
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
        canonical.push(normalized);
    }
    canonical.sort_by_key(|change| {
        serde_json::to_string(&change.row_identity).expect("row identity is serializable")
    });
    Ok(canonical)
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
    if table.db_session_id.is_empty() || table.table.trim().is_empty() {
        return Err(CommandError::Validation(
            "Row change plan requires a database session and table".into(),
        ));
    }
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
        let statement = PlannedStatement {
            row_identity: change.row_identity.clone(),
            original_values: change.original_values.clone(),
            current_values: change.current_values.clone(),
            changed_columns: change.changed_columns.clone(),
            sql_template: if change.delete_marked {
                driver.build_delete_sql(&table.table, &pk_columns)
            } else {
                driver.build_update_sql(&table.table, &set_columns, &pk_columns)
            },
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
    Ok(rebuilt)
}

pub(crate) async fn preview_pending_changes_impl(
    state: &AppState,
    db_session_id: String,
    table: String,
    database: Option<String>,
    changes: Vec<PendingRowChange>,
) -> Result<RowChangePlan, CommandError> {
    tracing::info!(%db_session_id, %table, change_count = changes.len(), "preview_pending_changes");
    let (driver, _handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("preview_pending_changes")?;
    // Preview is deliberately pure with respect to the database: it obtains
    // the driver only to call the dialect SQL builders and opens no transaction
    // and executes no statement.
    build_row_change_plan(
        driver.as_ref(),
        RowChangeTableContext {
            db_session_id,
            table,
            database,
        },
        &changes,
    )
}

#[tauri::command]
pub async fn preview_pending_changes(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    database: Option<String>,
    changes: Vec<PendingRowChange>,
) -> Result<RowChangePlan, CommandError> {
    preview_pending_changes_impl(&state, db_session_id, table, database, changes).await
}

async fn execute_row_change_plan_impl(
    state: &AppState,
    db_session_id: &str,
    driver: &std::sync::Arc<dyn crate::db::DatabaseDriver>,
    handle: &crate::db::ConnectionHandle,
    plan: &RowChangePlan,
) -> Result<CommitPendingChangesResponse, CommandError> {
    if plan.updates.is_empty() && plan.deletes.is_empty() {
        return Ok(CommitPendingChangesResponse {
            plan_id: plan.plan_id.clone(),
            fingerprint: plan.fingerprint.clone(),
            statements: Vec::new(),
            affected_rows: 0,
        });
    }

    let session_open = state
        .session_transactions
        .lock()
        .await
        .contains_key(db_session_id);
    let trait_tx = if session_open {
        None
    } else {
        match driver.begin_transaction(handle).await {
            Ok(tx) => Some(tx),
            Err(DriverError::TransactionError(_)) => None,
            Err(e) => return Err(CommandError::Driver(e)),
        }
    };

    if !session_open && trait_tx.is_none() {
        driver
            .execute(handle, "BEGIN")
            .await
            .cmd_err("commit_pending_changes")?;
    }

    let result: Result<(Vec<RowCommitStatementResult>, u64), CommandError> = async {
        let mut statements = Vec::with_capacity(plan.updates.len() + plan.deletes.len());
        let mut affected_rows = 0;
        for planned in &plan.updates {
            let affected = driver
                .execute(handle, &planned.sql_template)
                .await
                .cmd_err("commit_pending_changes")?;
            affected_rows += affected;
            statements.push(RowCommitStatementResult {
                operation: "update".into(),
                row_identity: planned.row_identity.clone(),
                affected_rows: affected,
            });
        }
        for planned in &plan.deletes {
            let affected = driver
                .execute(handle, &planned.sql_template)
                .await
                .cmd_err("commit_pending_changes")?;
            affected_rows += affected;
            statements.push(RowCommitStatementResult {
                operation: "delete".into(),
                row_identity: planned.row_identity.clone(),
                affected_rows: affected,
            });
        }
        Ok((statements, affected_rows))
    }
    .await;

    match result {
        Ok((statements, affected_rows)) => {
            if session_open {
                return Ok(CommitPendingChangesResponse {
                    plan_id: plan.plan_id.clone(),
                    fingerprint: plan.fingerprint.clone(),
                    statements,
                    affected_rows,
                });
            }
            if let Some(tx) = trait_tx {
                driver.commit(tx).await.cmd_err("commit_pending_changes")?;
            } else {
                driver
                    .execute(handle, "COMMIT")
                    .await
                    .cmd_err("commit_pending_changes")?;
            }
            Ok(CommitPendingChangesResponse {
                plan_id: plan.plan_id.clone(),
                fingerprint: plan.fingerprint.clone(),
                statements,
                affected_rows,
            })
        }
        Err(error) => {
            if session_open {
                return Err(error);
            }
            if let Some(tx) = trait_tx {
                if let Err(rollback_error) = driver.rollback(tx).await {
                    tracing::warn!("rollback failed: {rollback_error}");
                }
            } else if let Err(rollback_error) = driver.execute(handle, "ROLLBACK").await {
                tracing::warn!("rollback failed: {rollback_error}");
            }
            Err(error)
        }
    }
}

pub(crate) async fn commit_pending_changes_impl(
    state: &AppState,
    request: CommitPendingChangesRequest,
) -> Result<CommitPendingChangesResponse, CommandError> {
    tracing::info!(
        db_session_id = %request.db_session_id,
        plan_id = %request.plan.plan_id,
        "commit_pending_changes"
    );
    super::query::ensure_session_database(
        state,
        &request.db_session_id,
        request.plan.table.database.as_deref(),
        "commit_pending_changes",
    )
    .await?;
    let (driver, handle) = state
        .connection_manager
        .get_session(&request.db_session_id)
        .await
        .cmd_err("commit_pending_changes")?;
    let read_only = state
        .connection_manager
        .get_session_config(&request.db_session_id)
        .await
        .map(|config| config.read_only)
        .unwrap_or(false);
    if read_only {
        return Err(CommandError::Validation(
            "Connection is read-only; row changes are not allowed".into(),
        ));
    }
    let plan = validate_immutable_plan(
        driver.as_ref(),
        &request.db_session_id,
        &request.plan,
        &request.fingerprint,
    )?;
    execute_row_change_plan_impl(&*state, &request.db_session_id, &driver, &handle, &plan).await
}

#[tauri::command]
pub async fn commit_pending_changes(
    state: State<'_, AppState>,
    db_session_id: String,
    plan: RowChangePlan,
    fingerprint: String,
) -> Result<CommitPendingChangesResponse, CommandError> {
    commit_pending_changes_impl(
        &state,
        CommitPendingChangesRequest {
            db_session_id,
            plan,
            fingerprint,
        },
    )
    .await
}

pub(crate) async fn commit_row_updates_impl(
    state: &AppState,
    db_session_id: String,
    table: String,
    updates: Vec<RowUpdateBatch>,
) -> Result<(), CommandError> {
    tracing::info!(%db_session_id, %table, batch_count = updates.len(), "commit_row_updates");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("commit_row_updates")?;
    let read_only = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .map(|c| c.read_only)
        .unwrap_or(false);
    if read_only {
        return Err(CommandError::Validation(
            "Connection is read-only; row edits are not allowed".into(),
        ));
    }
    for batch in &updates {
        if batch.set_columns.is_empty() {
            return Err(CommandError::Validation(
                "Row update requires changed columns".into(),
            ));
        }
        if batch.pk_columns.is_empty() {
            return Err(CommandError::Validation(
                "Row update requires primary-key columns".into(),
            ));
        }
    }

    let session_open = state
        .session_transactions
        .lock()
        .await
        .contains_key(&db_session_id);

    // Prefer DatabaseDriver transaction API; fall back to BEGIN/COMMIT strings
    // for drivers that still stub the trait. Skip wrapping when a session
    // transaction is already open so row edits join that transaction.
    let trait_tx = if session_open {
        None
    } else {
        match driver.begin_transaction(&handle).await {
            Ok(tx) => Some(tx),
            Err(DriverError::TransactionError(_)) => None,
            Err(e) => return Err(CommandError::Driver(e)),
        }
    };

    if !session_open && trait_tx.is_none() {
        driver
            .execute(&handle, "BEGIN")
            .await
            .cmd_err("commit_row_updates")?;
    }

    let result: Result<(), CommandError> = async {
        for batch in &updates {
            let set_columns: Vec<(&str, Option<Value>)> = batch
                .set_columns
                .iter()
                .map(|c| (c.column.as_str(), c.value.clone()))
                .collect();
            let pk_columns: Vec<(&str, Option<Value>)> = batch
                .pk_columns
                .iter()
                .map(|c| (c.column.as_str(), c.value.clone()))
                .collect();
            let sql = driver.build_update_sql(&table, &set_columns, &pk_columns);
            driver
                .execute(&handle, &sql)
                .await
                .cmd_err("commit_row_updates")?;
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            if session_open {
                tracing::info!(%db_session_id, %table, batch_count = updates.len(), "commit_row_updates OK (session tx)");
                return Ok(());
            }
            if let Some(tx) = trait_tx {
                driver.commit(tx).await.cmd_err("commit_row_updates")?;
            } else {
                driver
                    .execute(&handle, "COMMIT")
                    .await
                    .cmd_err("commit_row_updates")?;
            }
            tracing::info!(%db_session_id, %table, batch_count = updates.len(), "commit_row_updates OK");
            Ok(())
        }
        Err(e) => {
            if session_open {
                return Err(e);
            }
            if let Some(tx) = trait_tx {
                if let Err(rb_err) = driver.rollback(tx).await {
                    tracing::warn!("rollback failed: {rb_err}");
                }
            } else if let Err(rb_err) = driver.execute(&handle, "ROLLBACK").await {
                tracing::warn!("rollback failed: {rb_err}");
            }
            Err(e)
        }
    }
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
    state: &AppState,
    db_session_id: String,
    table: String,
    deletes: Vec<RowDeleteBatch>,
) -> Result<(), CommandError> {
    tracing::info!(%db_session_id, %table, batch_count = deletes.len(), "commit_row_deletes");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("commit_row_deletes")?;
    let read_only = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .map(|c| c.read_only)
        .unwrap_or(false);
    if read_only {
        return Err(CommandError::Validation(
            "Connection is read-only; row deletes are not allowed".into(),
        ));
    }
    if deletes.is_empty() {
        return Ok(());
    }
    for batch in &deletes {
        if batch.pk_columns.is_empty() {
            return Err(CommandError::Validation(
                "Row delete requires primary-key columns".into(),
            ));
        }
    }

    let session_open = state
        .session_transactions
        .lock()
        .await
        .contains_key(&db_session_id);

    let trait_tx = if session_open {
        None
    } else {
        match driver.begin_transaction(&handle).await {
            Ok(tx) => Some(tx),
            Err(DriverError::TransactionError(_)) => None,
            Err(e) => return Err(CommandError::Driver(e)),
        }
    };

    if !session_open && trait_tx.is_none() {
        driver
            .execute(&handle, "BEGIN")
            .await
            .cmd_err("commit_row_deletes")?;
    }

    let result: Result<(), CommandError> = async {
        for batch in &deletes {
            let pk_columns: Vec<(&str, Option<Value>)> = batch
                .pk_columns
                .iter()
                .map(|c| (c.column.as_str(), c.value.clone()))
                .collect();
            let sql = driver.build_delete_sql(&table, &pk_columns);
            driver
                .execute(&handle, &sql)
                .await
                .cmd_err("commit_row_deletes")?;
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            if session_open {
                tracing::info!(%db_session_id, %table, batch_count = deletes.len(), "commit_row_deletes OK (session tx)");
                return Ok(());
            }
            if let Some(tx) = trait_tx {
                driver.commit(tx).await.cmd_err("commit_row_deletes")?;
            } else {
                driver
                    .execute(&handle, "COMMIT")
                    .await
                    .cmd_err("commit_row_deletes")?;
            }
            tracing::info!(%db_session_id, %table, batch_count = deletes.len(), "commit_row_deletes OK");
            Ok(())
        }
        Err(e) => {
            if session_open {
                return Err(e);
            }
            if let Some(tx) = trait_tx {
                if let Err(rb_err) = driver.rollback(tx).await {
                    tracing::warn!("rollback failed: {rb_err}");
                }
            } else if let Err(rb_err) = driver.execute(&handle, "ROLLBACK").await {
                tracing::warn!("rollback failed: {rb_err}");
            }
            Err(e)
        }
    }
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

    #[tokio::test]
    async fn preview_builds_driver_sql_without_opening_a_transaction() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("preview-only").await;
        let plan = preview_pending_changes_impl(
            &test.state,
            conn_id.clone(),
            "users".into(),
            Some("app".into()),
            vec![update_change(1, "Alice", "Updated")],
        )
        .await
        .unwrap();

        assert_eq!(plan.table.db_session_id, conn_id);
        assert_eq!(plan.updates.len(), 1);
        assert!(plan.updates[0].sql_template.contains("UPDATE \"users\""));
        assert!(plan.updates[0].sql_template.contains("\"id\" = 1"));
        assert!(!crate::commands::session_transaction_status_impl(
            &test.state,
            plan.table.db_session_id.clone()
        )
        .await
        .unwrap());
    }

    #[tokio::test]
    async fn commit_reuses_plan_fingerprint_and_returns_plan_id() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("commit-plan").await;
        let plan = preview_pending_changes_impl(
            &test.state,
            conn_id.clone(),
            "users".into(),
            Some("app".into()),
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
        assert_eq!(response.affected_rows, 0);
    }

    #[tokio::test]
    async fn commit_rejects_stale_fingerprint_before_execution() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("stale-plan").await;
        let plan = preview_pending_changes_impl(
            &test.state,
            conn_id.clone(),
            "users".into(),
            None,
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
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("preview-no-pk").await;
        let error = preview_pending_changes_impl(
            &test.state,
            conn_id,
            "users".into(),
            None,
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

    #[tokio::test]
    async fn commit_row_updates_success() {
        let test = TestAppState::with_tables().await;
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
        let test = TestAppState::with_tables().await;
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
        let test = TestAppState::with_tables().await;
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
        let test = TestAppState::with_tables().await;
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
        let test = TestAppState::with_tables().await;
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
        let test = TestAppState::with_tables().await;
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
