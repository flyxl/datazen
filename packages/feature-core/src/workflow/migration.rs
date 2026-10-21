//! Shared profile validation, unattended policy, history outcomes and variable sanitation.
use super::{
    context::WorkflowContext,
    error::WorkflowError,
    model::{
        StepExecutionResult, StepStatus, UnattendedDestructivePolicy, WorkflowMigrationOperation,
    },
};
use datazen_data_sync::model::TableMapping as SyncTableMapping;
use datazen_data_sync::{SyncOptions, SyncProfile, SyncSourceFilter};
use datazen_data_transfer::model::Endpoint as TransferEndpoint;
use datazen_data_transfer::{SqlFileTarget, TransferJob, TransferProfile};
use datazen_platform_api::dto::migration::{MigrationProfileRef, MigrationRunRecord};
use datazen_schema_diff::profile::SchemaDiffProfile;
use datazen_schema_diff::types::ColumnTypeOverride;
#[async_trait::async_trait]
pub trait MigrationHost: Send + Sync {
    async fn transfer_profiles(&self) -> Vec<TransferProfile>;
    async fn sync_profiles(&self) -> Vec<SyncProfile>;
    async fn schema_profiles(&self) -> Vec<SchemaDiffProfile>;
    async fn validate_reference(
        &self,
        operation: &str,
        reference: Option<&MigrationProfileRef>,
    ) -> Result<(), String>;
    async fn start_run(
        &self,
        operation: &str,
        reference: Option<&MigrationProfileRef>,
    ) -> MigrationRunRecord;
    async fn finish_run(
        &self,
        run: MigrationRunRecord,
        success: bool,
        cancelled: bool,
        committed: u64,
        failed: u64,
        conflicts: u64,
        rollback_outcome: &str,
    );
    async fn run_transfer(
        &self,
        profile: TransferProfile,
        token_variable: Option<&str>,
        context: &WorkflowContext,
        policy: UnattendedDestructivePolicy,
    ) -> Result<MigrationOutcome, WorkflowError>;
    async fn run_sync(
        &self,
        profile: SyncProfile,
        policy: UnattendedDestructivePolicy,
    ) -> Result<MigrationOutcome, WorkflowError>;
    async fn run_schema(
        &self,
        profile: SchemaDiffProfile,
        policy: UnattendedDestructivePolicy,
        reference: &MigrationProfileRef,
    ) -> Result<MigrationOutcome, WorkflowError>;
}
/// A live database session a migration step owns.
#[derive(Debug, Clone)]
pub struct MigrationSession {
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
}

/// The environment-specific operations a migration step needs.
///
/// The composition — which sessions to open, in which order, how the profile
/// becomes a plan, and when the sessions are released — belongs to this
/// crate, so a web host and the desktop host run the same step. The host
/// only provides the primitives: opening a session on its runtime and
/// running the plan its wizard owns.
#[async_trait::async_trait]
pub trait MigrationRunner: Send + Sync {
    /// Open a dedicated session for a persisted connection id.
    async fn open_session(
        &self,
        connection_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        label: &str,
    ) -> Result<MigrationSession, WorkflowError>;

    /// Release a session opened by [`Self::open_session`].
    async fn release_session(&self, session: &MigrationSession);

    /// Preview and apply a transfer plan built from the profile.
    async fn run_transfer(
        &self,
        job: TransferJob,
        destructive_policy: UnattendedDestructivePolicy,
    ) -> Result<MigrationOutcome, WorkflowError>;

    /// Compare and apply a data sync plan built from the profile.
    async fn run_sync(&self, request: SyncStepRequest) -> Result<MigrationOutcome, WorkflowError>;

    /// Plan and deploy a schema diff built from the profile.
    async fn run_schema(
        &self,
        request: SchemaStepRequest,
        reference: &MigrationProfileRef,
        destructive_policy: UnattendedDestructivePolicy,
    ) -> Result<MigrationOutcome, WorkflowError>;
}

/// Everything a data sync step needs, built from the profile and the opened
/// sessions.
#[derive(Debug, Clone)]
pub struct SyncStepRequest {
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    pub source_database: String,
    pub target_database: String,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
    pub tables: Vec<String>,
    pub mappings: Vec<SyncTableMapping>,
    pub filters: std::collections::HashMap<String, SyncSourceFilter>,
    pub options: SyncOptions,
}

/// Everything a schema diff step needs, built from the profile and the opened
/// sessions.
#[derive(Debug, Clone)]
pub struct SchemaStepRequest {
    pub source_db_session_id: String,
    pub target_db_session_id: String,
    pub tables: Vec<String>,
    pub target_only_tables: Vec<String>,
    pub allow_destructive: bool,
    pub require_rollback: bool,
    pub include_indexes: bool,
    pub type_overrides: Vec<ColumnTypeOverride>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
}

/// Run a transfer migration step: open the sessions the profile names, build
/// the transfer job and hand it to the runner.
pub async fn run_transfer_step(
    runner: &dyn MigrationRunner,
    profile: TransferProfile,
    sql_file_token_variable: Option<&str>,
    context: &WorkflowContext,
    destructive_policy: UnattendedDestructivePolicy,
) -> Result<MigrationOutcome, WorkflowError> {
    let source = runner
        .open_session(
            &profile.source_connection_id,
            profile.source_database.as_deref(),
            profile.source_schema.as_deref(),
            "transfer source",
        )
        .await?;
    let target = match (
        profile.destination_mode.as_str(),
        profile.target_connection_id.as_deref(),
    ) {
        ("database", Some(connection_id)) => {
            match runner
                .open_session(
                    connection_id,
                    profile.target_database.as_deref(),
                    profile.target_schema.as_deref(),
                    "transfer target",
                )
                .await
            {
                Ok(session) => Some(session),
                Err(error) => {
                    runner.release_session(&source).await;
                    return Err(error);
                }
            }
        }
        ("sqlFile", _) => None,
        ("database", None) => {
            runner.release_session(&source).await;
            return Err(WorkflowError::Validation(
                "database transfer profile has no target connection".into(),
            ));
        }
        (other, _) => {
            runner.release_session(&source).await;
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
        runner.run_transfer(job, destructive_policy).await
    }
    .await;
    runner.release_session(&source).await;
    if let Some(target) = target.as_ref() {
        runner.release_session(target).await;
    }
    operation
}

/// Run a data sync migration step: open both sessions and hand the comparison
/// request to the runner.
pub async fn run_sync_step(
    runner: &dyn MigrationRunner,
    profile: SyncProfile,
) -> Result<MigrationOutcome, WorkflowError> {
    let source = runner
        .open_session(
            &profile.source_connection_id,
            profile.source_database.as_deref(),
            profile.source_schema.as_deref(),
            "sync source",
        )
        .await?;
    let target = match runner
        .open_session(
            &profile.target_connection_id,
            profile.target_database.as_deref(),
            profile.target_schema.as_deref(),
            "sync target",
        )
        .await
    {
        Ok(session) => session,
        Err(error) => {
            runner.release_session(&source).await;
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
        let filters: std::collections::HashMap<String, SyncSourceFilter> = profile
            .tables
            .iter()
            .filter_map(|mapping| {
                mapping
                    .source_filter
                    .clone()
                    .map(|filter| (mapping.source_table.clone(), filter))
            })
            .collect();
        runner
            .run_sync(SyncStepRequest {
                source_db_session_id: source.db_session_id.clone(),
                target_db_session_id: target.db_session_id.clone(),
                source_database: source.database.clone(),
                target_database: target.database.clone(),
                source_schema: profile.source_schema.clone().or(source.schema.clone()),
                target_schema: profile.target_schema.clone().or(target.schema.clone()),
                tables,
                mappings: profile.tables.clone(),
                filters,
                options: profile.options.clone(),
            })
            .await
    }
    .await;
    runner.release_session(&source).await;
    runner.release_session(&target).await;
    operation
}

/// Run a schema diff migration step: open both sessions and hand the plan
/// request to the runner.
pub async fn run_schema_step(
    runner: &dyn MigrationRunner,
    profile: SchemaDiffProfile,
    reference: &MigrationProfileRef,
    destructive_policy: UnattendedDestructivePolicy,
) -> Result<MigrationOutcome, WorkflowError> {
    let source = runner
        .open_session(
            &profile.source_connection_id,
            Some(&profile.source_database),
            profile.source_schema.as_deref(),
            "schema diff source",
        )
        .await?;
    let target = match runner
        .open_session(
            &profile.target_connection_id,
            Some(&profile.target_database),
            profile.target_schema.as_deref(),
            "schema diff target",
        )
        .await
    {
        Ok(session) => session,
        Err(error) => {
            runner.release_session(&source).await;
            return Err(error);
        }
    };

    let operation = async {
        runner
            .run_schema(
                SchemaStepRequest {
                    source_db_session_id: source.db_session_id.clone(),
                    target_db_session_id: target.db_session_id.clone(),
                    tables: profile.tables.clone(),
                    target_only_tables: profile.target_only_tables.clone(),
                    allow_destructive: profile.allow_destructive,
                    require_rollback: profile.require_rollback,
                    include_indexes: profile.include_indexes,
                    type_overrides: profile.type_overrides.clone(),
                    source_schema: profile.source_schema.clone(),
                    target_schema: profile.target_schema.clone(),
                },
                reference,
                destructive_policy,
            )
            .await
    }
    .await;
    runner.release_session(&source).await;
    runner.release_session(&target).await;
    operation
}

pub struct MigrationOutcome {
    pub result: serde_json::Value,
    pub committed: u64,
    pub failed: u64,
    pub conflicts: u64,
    pub cancelled: bool,
    pub success: bool,
    pub rollback_outcome: &'static str,
}

pub async fn execute_profile_step(
    state: &dyn MigrationHost,
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
            let run = state.start_run("dataTransfer", Some(&reference)).await;
            let result = state
                .run_transfer(
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
            let run = state.start_run("dataSync", Some(&reference)).await;
            let result = state.run_sync(profile, destructive_policy).await;
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
            let result = state
                .run_schema(profile, destructive_policy, &reference)
                .await;
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
    state: &dyn MigrationHost,
    operation: &str,
    profile: Option<&MigrationProfileRef>,
    error: WorkflowError,
) -> WorkflowError {
    let run = state.start_run(operation, profile).await;
    state
        .finish_run(run, false, false, 0, 1, 0, "unknown")
        .await;
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
    state: &dyn MigrationHost,
    run: MigrationRunRecord,
    step_id: &str,
    operation: &str,
    result: Result<MigrationOutcome, WorkflowError>,
) -> Result<StepExecutionResult, WorkflowError> {
    match result {
        Ok(outcome) => {
            state
                .finish_run(
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
            state
                .finish_run(run, false, false, 0, 1, 0, "unknown")
                .await;
            Err(error)
        }
    }
}

async fn load_transfer_profile(
    state: &dyn MigrationHost,
    id: &str,
    expected_revision: Option<&str>,
) -> Result<(TransferProfile, MigrationProfileRef), WorkflowError> {
    let profile = state
        .transfer_profiles()
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
    state: &dyn MigrationHost,
    id: &str,
    expected_revision: Option<&str>,
) -> Result<(SyncProfile, MigrationProfileRef), WorkflowError> {
    let profile = state
        .sync_profiles()
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
    state: &dyn MigrationHost,
    id: &str,
    expected_revision: Option<&str>,
) -> Result<(SchemaDiffProfile, MigrationProfileRef), WorkflowError> {
    let profile = state
        .schema_profiles()
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
    state: &dyn MigrationHost,
    operation: &str,
    actual: &MigrationProfileRef,
    expected_revision: Option<&str>,
) -> Result<(), WorkflowError> {
    if let Some(revision) = expected_revision {
        let expected = MigrationProfileRef {
            id: actual.id.clone(),
            revision: revision.into(),
        };
        state
            .validate_reference(operation, Some(&expected))
            .await
            .map_err(|error| WorkflowError::Validation(error.to_string()))?;
    }
    state
        .validate_reference(operation, Some(actual))
        .await
        .map_err(|error| WorkflowError::Validation(error.to_string()))
}

/// Remove per-run SQL-file tokens before the generic workflow history writer
/// persists input variables. Profile records never contain these values, and
/// workflow history must not become a second token store.
pub fn sanitize_variables(
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

use datazen_data_transfer::model::{TableExecutionOutcome, TransferExecutionResult};
use datazen_data_transfer::{SqlFileCompression, SqlFileEncoding};
pub fn transfer_rollback_history_outcome(result: &TransferExecutionResult) -> &'static str {
    // SQL-file output does not use target database transactions or typed
    // per-table outcomes. Preserve its existing run-history contract here.
    if result.tables.iter().all(|table| table.outcome.is_none()) {
        return if result.partial {
            "unknown"
        } else {
            "notRequired"
        };
    }

    if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::Unknown))
    {
        "unknown"
    } else if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::PartiallyApplied))
    {
        "partiallyApplied"
    } else if result
        .tables
        .iter()
        .any(|table| table.outcome == Some(TableExecutionOutcome::RolledBack))
    {
        "rolledBack"
    } else if result.partial
        && result
            .tables
            .iter()
            .all(|table| table.outcome == Some(TableExecutionOutcome::NotStarted))
    {
        "notStarted"
    } else {
        "notRequired"
    }
}

pub fn transfer_error_history_outcome(write_started: bool) -> &'static str {
    if write_started {
        "unknown"
    } else {
        "notStarted"
    }
}

pub fn transfer_migration_outcome(
    plan_id: String,
    result: datazen_data_transfer::model::TransferExecutionResult,
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
        rollback_outcome: transfer_rollback_history_outcome(&result),
    }
}

pub fn parse_sql_file_encoding(value: &str) -> Result<SqlFileEncoding, WorkflowError> {
    SqlFileEncoding::parse_profile(value)
        .map_err(|error| WorkflowError::Validation(error.to_string()))
}

pub fn parse_sql_file_compression(value: &str) -> Result<SqlFileCompression, WorkflowError> {
    SqlFileCompression::parse_profile(value)
        .map_err(|error| WorkflowError::Validation(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_file_formats_accept_the_extended_set() {
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
