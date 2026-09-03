use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{DriverError, Value};
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

fn validate_table_context(table: &RowChangeTableContext) -> Result<(), CommandError> {
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
    table: RowChangeTableContext,
    changes: Vec<PendingRowChange>,
) -> Result<RowChangePlan, CommandError> {
    tracing::info!(db_session_id = %table.db_session_id, table = %table.table, change_count = changes.len(), "preview_pending_changes");
    let (driver, _handle) = state
        .connection_manager
        .get_session(&table.db_session_id)
        .await
        .cmd_err("preview_pending_changes")?;
    // Preview is deliberately pure with respect to the database: it obtains
    // the driver only to call the dialect SQL builders and opens no transaction
    // and executes no statement.
    build_row_change_plan(driver.as_ref(), table, &changes)
}

#[tauri::command]
pub async fn preview_pending_changes(
    state: State<'_, AppState>,
    context: RowChangeTableContext,
    changes: Vec<PendingRowChange>,
) -> Result<RowChangePlan, CommandError> {
    preview_pending_changes_impl(&state, context, changes).await
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
            if affected != 1 {
                return Err(CommandError::Validation(format!(
                    "UPDATE for one row identity affected {affected} rows; refusing ambiguous write"
                )));
            }
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
            if affected != 1 {
                return Err(CommandError::Validation(format!(
                    "DELETE for one row identity affected {affected} rows; refusing ambiguous write"
                )));
            }
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
    let (driver, handle) = state
        .connection_manager
        .get_session(&request.db_session_id)
        .await
        .cmd_err("commit_pending_changes")?;
    let config = state
        .connection_manager
        .get_session_config(&request.db_session_id)
        .await
        .cmd_err("commit_pending_changes")?;
    let owner = state
        .connection_manager
        .owner_connection_id(&request.db_session_id)
        .await;
    validate_table_context(&request.plan.table)?;
    if owner.as_deref() != Some(request.plan.table.connection_id.as_str()) {
        return Err(CommandError::Validation(
            "Row change plan belongs to a different connection".into(),
        ));
    }
    if driver.driver_type() != request.plan.table.driver_type
        || config.database_type != request.plan.table.driver_type
    {
        return Err(CommandError::Validation(
            "Row change plan belongs to a different driver context".into(),
        ));
    }
    if config.database != request.plan.table.database {
        return Err(CommandError::Validation(
            "Database context changed; reload and preview pending changes again".into(),
        ));
    }
    // A connection-level schema is optional. When it is omitted, the table
    // context may still contain the concrete schema returned by the
    // navigator (for example PostgreSQL's `public`). Only an explicitly
    // configured schema is a binding context that must match the preview.
    if let Some(config_schema) = config.schema.as_deref() {
        if request.plan.table.schema.as_deref() != Some(config_schema) {
            return Err(CommandError::Validation(
                "Schema context changed; reload and preview pending changes again".into(),
            ));
        }
    }
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
        validate_legacy_pk_columns(&batch.pk_columns, "Row update")?;
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
        validate_legacy_pk_columns(&batch.pk_columns, "Row delete")?;
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
        let test = TestAppState::with_tables().await;
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
        let mut options = crate::testing::app_state::rich_mock_options();
        options.execute_rows_affected = 1;
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
        let test = TestAppState::with_tables().await;
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
        let test = TestAppState::with_tables().await;
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
