//! Host-owned Workflow migration steps.
//!
//! A migration step is deliberately separate from Driver Commands. It resolves
//! a persisted profile, opens fresh sessions from connection ids, prepares the
//! same server-owned immutable plan used by the migration windows, and records
//! the run through the shared migration history store.

use std::collections::HashMap;

use crate::commands::AppState;
use crate::data_sync::SyncSourceFilter;
use crate::data_transfer::model::{
    Endpoint as TransferEndpoint, TransferRunOptions, TransferRunSelection,
};
use crate::data_transfer::{
    SqlFileCompression, SqlFileEncoding, SqlFileTarget, TransferJob, TransferRunRequest,
};
use crate::store::MigrationProfileRef;
use crate::workflow::model::{
    StepExecutionResult, StepStatus, UnattendedDestructivePolicy, WorkflowMigrationOperation,
};
use crate::workflow::workflows::context::WorkflowContext;
use crate::workflow::WorkflowError;

struct RuntimeSession {
    db_session_id: String,
    database: String,
    schema: Option<String>,
}

struct MigrationOutcome {
    result: serde_json::Value,
    committed: u64,
    failed: u64,
    conflicts: u64,
    cancelled: bool,
    success: bool,
    rollback_outcome: &'static str,
}

pub(crate) async fn execute_profile_step(
    state: &AppState,
    step_id: &str,
    operation: WorkflowMigrationOperation,
    profile_id: &str,
    expected_revision: Option<&str>,
    destructive_policy: UnattendedDestructivePolicy,
    sql_file_token_variable: Option<&str>,
    context: &WorkflowContext,
) -> Result<StepExecutionResult, WorkflowError> {
    match operation {
        WorkflowMigrationOperation::DataTransfer => {
            let (profile, reference) =
                match load_transfer_profile(state, profile_id, expected_revision).await {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(
                            record_preflight_failure(state, "dataTransfer", None, error).await
                        );
                    }
                };
            if let Err(error) = ensure_destructive_policy(
                destructive_policy,
                profile.write_mode.is_destructive(),
                "data transfer",
            ) {
                return Err(record_preflight_failure(
                    state,
                    "dataTransfer",
                    Some(&reference),
                    error,
                )
                .await);
            }
            let run =
                crate::commands::start_migration_run(state, "dataTransfer", Some(&reference)).await;
            let result = run_transfer(
                state,
                profile,
                sql_file_token_variable,
                context,
                destructive_policy,
            )
            .await;
            return finish_step_run(state, run, step_id, "dataTransfer", result).await;
        }
        WorkflowMigrationOperation::DataSync => {
            let (profile, reference) =
                match load_sync_profile(state, profile_id, expected_revision).await {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(record_preflight_failure(state, "dataSync", None, error).await);
                    }
                };
            if let Err(error) = ensure_destructive_policy(
                destructive_policy,
                profile.options.delete,
                "data synchronization delete",
            ) {
                return Err(
                    record_preflight_failure(state, "dataSync", Some(&reference), error).await,
                );
            }
            let run =
                crate::commands::start_migration_run(state, "dataSync", Some(&reference)).await;
            let result = run_sync(state, profile, destructive_policy).await;
            return finish_step_run(state, run, step_id, "dataSync", result).await;
        }
        WorkflowMigrationOperation::SchemaDiff => {
            let (profile, reference) =
                match load_schema_profile(state, profile_id, expected_revision).await {
                    Ok(value) => value,
                    Err(error) => {
                        return Err(
                            record_preflight_failure(state, "schemaDiff", None, error).await
                        );
                    }
                };
            if let Err(error) = ensure_destructive_policy(
                destructive_policy,
                profile.allow_destructive,
                "schema diff destructive changes",
            ) {
                return Err(
                    record_preflight_failure(state, "schemaDiff", Some(&reference), error).await,
                );
            }
            let result = run_schema(state, profile, destructive_policy, &reference).await;
            return match result {
                Ok(outcome) if outcome.success || outcome.cancelled => Ok(StepExecutionResult {
                    step_id: step_id.into(),
                    step_type: "migration".into(),
                    status: if outcome.cancelled {
                        StepStatus::Skipped
                    } else {
                        StepStatus::Success
                    },
                    result: Some(outcome.result),
                    execution_time_ms: 0,
                    error: None,
                    connection_name: None,
                    sql_executed: None,
                }),
                Ok(_) => Err(WorkflowError::Step(
                    "schemaDiff migration profile execution failed".into(),
                )),
                Err(error) => Err(error),
            };
        }
    }
}

async fn record_preflight_failure(
    state: &AppState,
    operation: &str,
    profile: Option<&MigrationProfileRef>,
    error: WorkflowError,
) -> WorkflowError {
    let run = crate::commands::start_migration_run(state, operation, profile).await;
    crate::commands::finish_migration_run(state, run, false, false, 0, 1, 0, "unknown").await;
    error
}

fn ensure_destructive_policy(
    policy: UnattendedDestructivePolicy,
    destructive: bool,
    operation: &str,
) -> Result<(), WorkflowError> {
    if destructive && policy != UnattendedDestructivePolicy::Allow {
        return Err(WorkflowError::Validation(format!(
            "unattended destructive policy rejects {operation}; set destructivePolicy to allow explicitly"
        )));
    }
    Ok(())
}

async fn finish_step_run(
    state: &AppState,
    run: crate::store::MigrationRunRecord,
    step_id: &str,
    operation: &str,
    result: Result<MigrationOutcome, WorkflowError>,
) -> Result<StepExecutionResult, WorkflowError> {
    match result {
        Ok(outcome) => {
            crate::commands::finish_migration_run(
                state,
                run,
                outcome.success,
                outcome.cancelled,
                outcome.committed,
                outcome.failed,
                outcome.conflicts,
                outcome.rollback_outcome,
            )
            .await;
            if !outcome.success && !outcome.cancelled {
                return Err(WorkflowError::Step(format!(
                    "{operation} migration profile execution failed"
                )));
            }
            Ok(StepExecutionResult {
                step_id: step_id.into(),
                step_type: "migration".into(),
                status: if outcome.cancelled {
                    StepStatus::Skipped
                } else {
                    StepStatus::Success
                },
                result: Some(outcome.result),
                execution_time_ms: 0,
                error: None,
                connection_name: None,
                sql_executed: None,
            })
        }
        Err(error) => {
            crate::commands::finish_migration_run(state, run, false, false, 0, 1, 0, "unknown")
                .await;
            Err(error)
        }
    }
}

async fn load_transfer_profile(
    state: &AppState,
    id: &str,
    expected_revision: Option<&str>,
) -> Result<(crate::data_transfer::TransferProfile, MigrationProfileRef), WorkflowError> {
    let profile = state
        .store
        .get_transfer_profiles()
        .await
        .into_iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| {
            WorkflowError::Validation(format!("transfer profile '{id}' was not found"))
        })?;
    profile.validate().map_err(WorkflowError::Validation)?;
    let reference = profile_reference(&profile.id, profile.updated_at.to_rfc3339());
    validate_profile_reference(state, "dataTransfer", &reference, expected_revision).await?;
    Ok((profile, reference))
}

async fn load_sync_profile(
    state: &AppState,
    id: &str,
    expected_revision: Option<&str>,
) -> Result<(crate::data_sync::SyncProfile, MigrationProfileRef), WorkflowError> {
    let profile = state
        .store
        .get_sync_profiles()
        .await
        .into_iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| WorkflowError::Validation(format!("sync profile '{id}' was not found")))?;
    profile.validate().map_err(WorkflowError::Validation)?;
    let reference = profile_reference(&profile.id, profile.updated_at.to_rfc3339());
    validate_profile_reference(state, "dataSync", &reference, expected_revision).await?;
    Ok((profile, reference))
}

async fn load_schema_profile(
    state: &AppState,
    id: &str,
    expected_revision: Option<&str>,
) -> Result<(crate::schema_diff::SchemaDiffProfile, MigrationProfileRef), WorkflowError> {
    let profile = state
        .store
        .get_schema_diff_profiles()
        .await
        .into_iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| {
            WorkflowError::Validation(format!("schema diff profile '{id}' was not found"))
        })?;
    profile.validate().map_err(WorkflowError::Validation)?;
    let reference = profile_reference(&profile.id, profile.updated_at.to_rfc3339());
    validate_profile_reference(state, "schemaDiff", &reference, expected_revision).await?;
    Ok((profile, reference))
}

fn profile_reference(id: &str, revision: String) -> MigrationProfileRef {
    MigrationProfileRef {
        id: id.into(),
        revision,
    }
}

async fn validate_profile_reference(
    state: &AppState,
    operation: &str,
    actual: &MigrationProfileRef,
    expected_revision: Option<&str>,
) -> Result<(), WorkflowError> {
    if let Some(revision) = expected_revision {
        let expected = MigrationProfileRef {
            id: actual.id.clone(),
            revision: revision.into(),
        };
        crate::commands::validate_migration_profile_ref(state, operation, Some(&expected))
            .await
            .map_err(|error| WorkflowError::Validation(error.to_string()))?;
    }
    crate::commands::validate_migration_profile_ref(state, operation, Some(actual))
        .await
        .map_err(|error| WorkflowError::Validation(error.to_string()))
}

async fn open_session(
    state: &AppState,
    connection_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
    label: &str,
) -> Result<RuntimeSession, WorkflowError> {
    let (db_session_id, database) = if let Some(database) =
        database.filter(|value| !value.trim().is_empty())
    {
        let session = state
            .connection_manager
            .connect_dedicated(connection_id, Some(database))
            .await
            .map_err(|error| WorkflowError::ConnectionFailed {
                connection_id: connection_id.into(),
                message: error.to_string(),
            })?;
        (session, database.to_string())
    } else {
        let (session, _, _) = state
            .connection_manager
            .resolve_session_for_connection(connection_id)
            .await
            .map_err(|error| WorkflowError::ConnectionFailed {
                connection_id: connection_id.into(),
                message: error.to_string(),
            })?;
        let config = state
            .connection_manager
            .get_session_config(&session)
            .await
            .map_err(|error| WorkflowError::Driver(error.to_string()))?;
        let database = match config.database {
            Some(database) => database,
            None => {
                if let Err(error) = state.connection_manager.release(&session).await {
                    tracing::warn!(
                        db_session_id = %session,
                        %error,
                        "failed to release workflow migration session after database resolution failed"
                    );
                }
                return Err(WorkflowError::Validation(format!(
                    "{label} connection has no selected database"
                )));
            }
        };
        (session, database)
    };
    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .map_err(|error| WorkflowError::Driver(error.to_string()))?;
    Ok(RuntimeSession {
        db_session_id,
        database,
        schema: schema
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or(config.schema),
    })
}

async fn release_session(state: &AppState, session: &RuntimeSession) {
    if let Err(error) = state
        .connection_manager
        .release(&session.db_session_id)
        .await
    {
        tracing::warn!(db_session_id = %session.db_session_id, %error, "failed to release workflow migration session");
    }
}

async fn run_transfer(
    state: &AppState,
    profile: crate::data_transfer::TransferProfile,
    sql_file_token_variable: Option<&str>,
    context: &WorkflowContext,
    destructive_policy: UnattendedDestructivePolicy,
) -> Result<MigrationOutcome, WorkflowError> {
    let source_config = state
        .store
        .get_connection(&profile.source_connection_id)
        .await
        .ok_or_else(|| {
            WorkflowError::Validation("transfer source connection no longer exists".into())
        })?;
    let source_database = profile
        .source_database
        .clone()
        .or(source_config.database.clone());
    let source = open_session(
        state,
        &profile.source_connection_id,
        source_database.as_deref(),
        profile.source_schema.as_deref(),
        "transfer source",
    )
    .await?;
    let target_connection = profile.target_connection_id.as_deref();
    let target_database = profile.target_database.clone();
    let target = match (profile.destination_mode.as_str(), target_connection) {
        ("database", Some(connection_id)) => match open_session(
            state,
            connection_id,
            target_database.as_deref(),
            profile.target_schema.as_deref(),
            "transfer target",
        )
        .await
        {
            Ok(session) => Some(session),
            Err(error) => {
                release_session(state, &source).await;
                return Err(error);
            }
        },
        ("sqlFile", _) => None,
        ("database", None) => {
            release_session(state, &source).await;
            return Err(WorkflowError::Validation(
                "database transfer profile has no target connection".into(),
            ));
        }
        (other, _) => {
            release_session(state, &source).await;
            return Err(WorkflowError::Validation(format!(
                "unknown transfer destination mode '{other}'"
            )));
        }
    };

    let operation = async {
        let sql_file_target = if profile.destination_mode == "sqlFile" {
            let variable = sql_file_token_variable.ok_or_else(|| {
                WorkflowError::Validation(
                    "SQL-file workflow migrations require sqlFileTokenVariable".into(),
                )
            })?;
            let token = context.resolve_expression(variable);
            if token.trim().is_empty() {
                return Err(WorkflowError::Validation(format!(
                    "SQL-file workflow migration requires a fresh token in variable '{variable}'"
                )));
            }
            Some(SqlFileTarget {
                file_token: token,
                database_type: profile.sql_file_dialect.clone(),
                database: profile
                    .sql_file_database
                    .clone()
                    .or(profile.target_database.clone()),
                schema: profile
                    .sql_file_schema
                    .clone()
                    .or(profile.target_schema.clone()),
                encoding: profile
                    .sql_file_encoding
                    .as_deref()
                    .map(parse_sql_file_encoding)
                    .transpose()?,
                compression: profile
                    .sql_file_compression
                    .as_deref()
                    .map(parse_sql_file_compression)
                    .transpose()?,
            })
        } else {
            None
        };
        let job = TransferJob {
            source: TransferEndpoint {
                db_session_id: source.db_session_id.clone(),
                database: source.database.clone(),
                schema: profile.source_schema.clone().or(source.schema.clone()),
            },
            target: target.as_ref().map(|endpoint| TransferEndpoint {
                db_session_id: endpoint.db_session_id.clone(),
                database: endpoint.database.clone(),
                schema: profile.target_schema.clone().or(endpoint.schema.clone()),
            }),
            sql_file_target,
            mode: profile.mode,
            write_mode: profile.write_mode,
            tables: profile.tables.clone(),
            options: profile.options.clone(),
        };
        let preview = crate::commands::preview_data_transfer_impl(state, job)
            .await
            .map_err(|error| WorkflowError::Validation(error.to_string()))?;
        let request = TransferRunRequest {
            plan_id: preview.plan_id.clone(),
            selection: TransferRunSelection::default(),
            options: TransferRunOptions {
                confirmed_destructive: destructive_policy == UnattendedDestructivePolicy::Allow,
            },
            job_id: None,
            resume_token: None,
        };
        let result = crate::commands::execute_data_transfer_impl(state, request)
            .await
            .map_err(|error| WorkflowError::Step(error.to_string()))?;
        Ok(transfer_migration_outcome(preview.plan_id, result))
    }
    .await;
    release_session(state, &source).await;
    if let Some(target) = target.as_ref() {
        release_session(state, target).await;
    }
    operation
}

fn transfer_migration_outcome(
    plan_id: String,
    result: crate::data_transfer::model::TransferExecutionResult,
) -> MigrationOutcome {
    let failed = result.tables.iter().filter(|table| !table.success).count() as u64;
    MigrationOutcome {
        result: serde_json::json!({
            "operation": "dataTransfer",
            "planId": plan_id,
            "rowsInserted": result.rows_inserted,
            "tables": result.tables.len(),
            "partial": result.partial,
            "cancelled": result.cancelled,
            "resumeToken": result.resume_token,
        }),
        committed: result.rows_inserted,
        failed,
        conflicts: 0,
        cancelled: result.cancelled,
        success: !result.partial && !result.cancelled && failed == 0,
        rollback_outcome: crate::commands::transfer_rollback_history_outcome(&result),
    }
}

async fn run_sync(
    state: &AppState,
    profile: crate::data_sync::SyncProfile,
    _destructive_policy: UnattendedDestructivePolicy,
) -> Result<MigrationOutcome, WorkflowError> {
    let source = open_session(
        state,
        &profile.source_connection_id,
        profile.source_database.as_deref(),
        profile.source_schema.as_deref(),
        "sync source",
    )
    .await?;
    let target = match open_session(
        state,
        &profile.target_connection_id,
        profile.target_database.as_deref(),
        profile.target_schema.as_deref(),
        "sync target",
    )
    .await
    {
        Ok(session) => session,
        Err(error) => {
            release_session(state, &source).await;
            return Err(error);
        }
    };
    let operation = async {
        let tables: Vec<String> = profile
            .tables
            .iter()
            .filter(|mapping| mapping.enabled)
            .map(|mapping| mapping.source_table.clone())
            .collect();
        let filters: HashMap<String, SyncSourceFilter> = profile
            .tables
            .iter()
            .filter_map(|mapping| {
                mapping
                    .source_filter
                    .clone()
                    .map(|filter| (mapping.source_table.clone(), filter))
            })
            .collect();
        let preview = crate::commands::compare_data_sync_impl(
            state,
            source.db_session_id.clone(),
            target.db_session_id.clone(),
            tables,
            None,
            Some(source.database.clone()),
            Some(target.database.clone()),
            profile.source_schema.clone().or(source.schema.clone()),
            profile.target_schema.clone().or(target.schema.clone()),
            profile.options.clone(),
            &profile.tables,
            &filters,
        )
        .await
        .map_err(|error| WorkflowError::Validation(error.to_string()))?;
        let result = crate::commands::execute_data_sync_profile_plan_impl(
            state,
            preview.plan_id,
            preview.selection_revision,
            profile.options.clone(),
        )
        .await
        .map_err(|error| WorkflowError::Step(error.to_string()))?;
        Ok(MigrationOutcome {
            result: serde_json::json!({
                "operation": "dataSync",
                "applied": result.applied,
                "affectedRows": result.affected_rows,
                "skipped": result.skipped,
                "conflicts": result.conflicts.len(),
                "rolledBack": result.rolled_back,
            }),
            committed: result.affected_rows,
            failed: u64::from(result.rolled_back),
            conflicts: result.conflicts.len() as u64,
            cancelled: false,
            success: !result.rolled_back,
            rollback_outcome: if result.rolled_back {
                "completed"
            } else {
                "notRequired"
            },
        })
    }
    .await;
    release_session(state, &source).await;
    release_session(state, &target).await;
    operation
}

async fn run_schema(
    state: &AppState,
    profile: crate::schema_diff::SchemaDiffProfile,
    destructive_policy: UnattendedDestructivePolicy,
    reference: &MigrationProfileRef,
) -> Result<MigrationOutcome, WorkflowError> {
    let source = open_session(
        state,
        &profile.source_connection_id,
        Some(&profile.source_database),
        profile.source_schema.as_deref(),
        "schema diff source",
    )
    .await?;
    let target = match open_session(
        state,
        &profile.target_connection_id,
        Some(&profile.target_database),
        profile.target_schema.as_deref(),
        "schema diff target",
    )
    .await
    {
        Ok(session) => session,
        Err(error) => {
            release_session(state, &source).await;
            return Err(error);
        }
    };
    let operation = async {
        let plan = crate::commands::prepare_schema_diff_profile_plan_impl(
            state,
            source.db_session_id.clone(),
            target.db_session_id.clone(),
            profile.tables.clone(),
            profile.target_only_tables.clone(),
            profile.allow_destructive,
            Some(profile.include_indexes),
            Some(profile.type_overrides.clone()),
            profile.source_schema.clone(),
            profile.target_schema.clone(),
        )
        .await
        .map_err(|error| WorkflowError::Validation(error.to_string()))?;
        let result = crate::commands::execute_schema_diff_deploy_impl(
            state,
            target.db_session_id.clone(),
            plan.clone(),
            Some(true),
            Some(profile.require_rollback),
            (destructive_policy == UnattendedDestructivePolicy::Allow)
                .then_some(crate::schema_diff::deploy::DESTRUCTIVE_CONFIRM_TOKEN.to_string()),
            None,
            None,
            None,
            Some(reference.clone()),
        )
        .await
        .map_err(|error| WorkflowError::Step(error.to_string()))?;
        let success = matches!(result.status, crate::schema_diff::DeployStatus::Committed);
        let cancelled = matches!(result.status, crate::schema_diff::DeployStatus::Cancelled);
        Ok(MigrationOutcome {
            result: serde_json::json!({
                "operation": "schemaDiff",
                "status": result.status,
                "executed": result.executed_count,
                "statements": result.statement_count,
                "errors": result.errors.len(),
                "cancelled": cancelled,
            }),
            committed: result.executed_count as u64,
            failed: result.errors.len() as u64,
            conflicts: 0,
            cancelled,
            success,
            rollback_outcome: match result.status {
                crate::schema_diff::DeployStatus::RolledBack => "completed",
                crate::schema_diff::DeployStatus::Unknown
                | crate::schema_diff::DeployStatus::Mixed => "unknown",
                _ => "notRequired",
            },
        })
    }
    .await;
    release_session(state, &source).await;
    release_session(state, &target).await;
    operation
}

fn parse_sql_file_encoding(value: &str) -> Result<SqlFileEncoding, WorkflowError> {
    SqlFileEncoding::parse_profile(value)
        .map_err(|error| WorkflowError::Validation(error.to_string()))
}

fn parse_sql_file_compression(value: &str) -> Result<SqlFileCompression, WorkflowError> {
    SqlFileCompression::parse_profile(value)
        .map_err(|error| WorkflowError::Validation(error.to_string()))
}

/// Remove per-run SQL-file tokens before the generic workflow history writer
/// persists input variables. Profile records never contain these values, and
/// workflow history must not become a second token store.
pub(crate) fn sanitize_variables(
    workflow: &crate::workflow::model::WorkflowDefinition,
    variables: &serde_json::Value,
) -> serde_json::Value {
    let mut sanitized = variables.clone();
    let Some(values) = sanitized.as_object_mut() else {
        return sanitized;
    };
    for variable in token_variable_names(&workflow.steps) {
        if values.contains_key(&variable) {
            values.insert(variable, serde_json::Value::String("[redacted]".into()));
        }
    }
    sanitized
}

fn token_variable_names(steps: &[crate::workflow::model::WorkflowStep]) -> Vec<String> {
    let mut names = Vec::new();
    for step in steps {
        match step {
            crate::workflow::model::WorkflowStep::Migration {
                sql_file_token_variable: Some(variable),
                ..
            } => names.push(variable.clone()),
            crate::workflow::model::WorkflowStep::Condition {
                then_steps,
                else_steps,
                ..
            } => {
                names.extend(token_variable_names(then_steps));
                if let Some(else_steps) = else_steps {
                    names.extend(token_variable_names(else_steps));
                }
            }
            crate::workflow::model::WorkflowStep::ForEach { steps, .. } => {
                names.extend(token_variable_names(steps));
            }
            _ => {}
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_transfer::model::{
        TableExecutionOutcome, TableExecutionResult, TransferExecutionResult, TransferOptions,
    };
    use crate::data_transfer::{TransferMode, TransferProfile, WriteMode};
    use crate::store::MigrationRunFilter;
    use crate::testing::app_state::TestAppState;
    use crate::workflow::model::{
        WorkflowDefinition, WorkflowMigrationOperation, WorkflowStep, WorkflowVisibility,
    };
    use chrono::Utc;

    #[test]
    fn workflow_transfer_history_preserves_partial_and_unknown_outcomes() {
        for (table_outcome, expected_history_outcome) in [
            (TableExecutionOutcome::PartiallyApplied, "partiallyApplied"),
            (TableExecutionOutcome::Unknown, "unknown"),
        ] {
            let migration = transfer_migration_outcome(
                "plan-1".into(),
                TransferExecutionResult {
                    tables: vec![TableExecutionResult::database(
                        "users",
                        "users",
                        None,
                        table_outcome,
                        Some("injected outcome".into()),
                    )],
                    partial: true,
                    ..Default::default()
                },
            );
            assert_eq!(migration.rollback_outcome, expected_history_outcome);
            assert!(!migration.success);
            assert_eq!(migration.result["planId"], "plan-1");
        }
    }

    #[tokio::test]
    async fn unattended_destructive_transfer_is_rejected_before_connection_resolution() {
        let test = TestAppState::new().await;
        test.store
            .save_transfer_profile(TransferProfile {
                version: TransferProfile::CURRENT_VERSION,
                id: "destructive-transfer".into(),
                name: "destructive".into(),
                source_connection_id: "missing-source".into(),
                target_connection_id: Some("missing-target".into()),
                source_database: Some("source".into()),
                target_database: Some("target".into()),
                source_schema: None,
                target_schema: None,
                destination_mode: "database".into(),
                sql_file_dialect: None,
                sql_file_encoding: None,
                sql_file_compression: None,
                sql_file_database: None,
                sql_file_schema: None,
                mode: TransferMode::Data,
                write_mode: WriteMode::DropCreateInsert,
                tables: vec![],
                options: TransferOptions::default(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .await
            .unwrap();

        let error = execute_profile_step(
            &test.state,
            "migration",
            WorkflowMigrationOperation::DataTransfer,
            "destructive-transfer",
            None,
            UnattendedDestructivePolicy::Reject,
            None,
            &WorkflowContext::new(&serde_json::json!({})),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("unattended destructive policy"));
        let history = test
            .store
            .list_migration_runs(&MigrationRunFilter::default(), 0, 10)
            .await
            .unwrap();
        assert_eq!(history.total, 1);
        assert_eq!(history.items[0].status, "failed");
        assert_eq!(
            history.items[0].profile_id.as_deref(),
            Some("destructive-transfer")
        );
    }

    #[test]
    fn workflow_history_redacts_sql_file_token_variables() {
        let workflow = WorkflowDefinition {
            id: "export-workflow".into(),
            name: "Export".into(),
            description: String::new(),
            version: None,
            blocks: Vec::new(),
            author: None,
            variables: vec![],
            connection: None,
            database: None,
            steps: vec![WorkflowStep::Migration {
                id: "export".into(),
                operation: WorkflowMigrationOperation::DataTransfer,
                profile_id: "profile".into(),
                profile_revision: None,
                destructive_policy: UnattendedDestructivePolicy::Reject,
                sql_file_token_variable: Some("destinationToken".into()),
                timeout_secs: None,
                on_error: None,
            }],
            output: None,
            timeout_secs: None,
            error_handling: None,
            schedule: None,
            visibility: WorkflowVisibility::User,
        };
        let sanitized = sanitize_variables(
            &workflow,
            &serde_json::json!({"destinationToken": "opaque-token", "keep": "value"}),
        );
        assert_eq!(sanitized["destinationToken"], "[redacted]");
        assert_eq!(sanitized["keep"], "value");
    }

    #[test]
    fn test_tester_workflow_accepts_extended_sql_file_formats() {
        assert_eq!(
            parse_sql_file_encoding(" utf16Le ").unwrap(),
            SqlFileEncoding::Utf16Le
        );
        assert_eq!(
            parse_sql_file_encoding("UTF-16BE").unwrap(),
            SqlFileEncoding::Utf16Be
        );
        assert_eq!(
            parse_sql_file_compression("gzip").unwrap(),
            SqlFileCompression::Gzip
        );
        assert!(parse_sql_file_encoding("cp936").is_err());
        assert!(parse_sql_file_compression("brotli").is_err());
    }
}
