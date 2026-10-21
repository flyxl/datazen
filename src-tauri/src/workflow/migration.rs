//! Host-owned Workflow migration steps.
//!
//! A migration step is deliberately separate from Driver Commands. It resolves
//! a persisted profile, opens fresh sessions from connection ids, prepares the
//! same server-owned immutable plan used by the migration windows, and records
//! the run through the shared migration history store.

use crate::commands::AppState;
use crate::data_transfer::model::{TransferRunOptions, TransferRunSelection};
use crate::data_transfer::TransferRunRequest;
use crate::store::MigrationProfileRef;
use crate::workflow::WorkflowError;
use datazen_feature_core::workflow::context::WorkflowContext;
use datazen_feature_core::workflow::model::{
    StepExecutionResult, UnattendedDestructivePolicy, WorkflowMigrationOperation,
};

fn migration_command_error(error: crate::commands::CommandError) -> WorkflowError {
    WorkflowError::Driver(error.to_string())
}

pub use datazen_feature_core::workflow::migration::sanitize_variables;
use datazen_feature_core::workflow::migration::{MigrationHost, MigrationOutcome};
pub(crate) async fn execute_profile_step(
    state: &AppState,
    step_id: &str,
    operation: WorkflowMigrationOperation,
    profile_id: &str,
    expected_revision: Option<&str>,
    policy: UnattendedDestructivePolicy,
    token_variable: Option<&str>,
    context: &WorkflowContext,
) -> Result<StepExecutionResult, WorkflowError> {
    datazen_feature_core::workflow::migration::execute_profile_step(
        state,
        step_id,
        operation,
        profile_id,
        expected_revision,
        policy,
        token_variable,
        context,
    )
    .await
}
#[async_trait::async_trait]
impl MigrationHost for AppState {
    async fn transfer_profiles(&self) -> Vec<datazen_data_transfer::TransferProfile> {
        self.store.get_transfer_profiles().await
    }
    async fn sync_profiles(&self) -> Vec<datazen_data_sync::SyncProfile> {
        self.store.get_sync_profiles().await
    }
    async fn schema_profiles(&self) -> Vec<datazen_schema_diff::SchemaDiffProfile> {
        self.store.get_schema_diff_profiles().await
    }
    async fn validate_reference(
        &self,
        operation: &str,
        reference: Option<&MigrationProfileRef>,
    ) -> Result<(), String> {
        crate::commands::validate_migration_profile_ref(self, operation, reference)
            .await
            .map_err(|error| error.to_string())
    }
    async fn start_run(
        &self,
        operation: &str,
        reference: Option<&MigrationProfileRef>,
    ) -> crate::store::MigrationRunRecord {
        crate::commands::start_migration_run(self, operation, reference).await
    }
    async fn finish_run(
        &self,
        run: crate::store::MigrationRunRecord,
        success: bool,
        cancelled: bool,
        committed: u64,
        failed: u64,
        conflicts: u64,
        rollback_outcome: &str,
    ) {
        crate::commands::finish_migration_run(
            self,
            run,
            success,
            cancelled,
            committed,
            failed,
            conflicts,
            rollback_outcome,
        )
        .await;
    }
    async fn run_transfer(
        &self,
        profile: datazen_data_transfer::TransferProfile,
        token_variable: Option<&str>,
        context: &WorkflowContext,
        policy: UnattendedDestructivePolicy,
    ) -> Result<MigrationOutcome, WorkflowError> {
        datazen_feature_core::workflow::migration::run_transfer_step(
            self,
            profile,
            token_variable,
            context,
            policy,
        )
        .await
    }
    async fn run_sync(
        &self,
        profile: datazen_data_sync::SyncProfile,
        policy: UnattendedDestructivePolicy,
    ) -> Result<MigrationOutcome, WorkflowError> {
        let _ = policy;
        datazen_feature_core::workflow::migration::run_sync_step(self, profile).await
    }
    async fn run_schema(
        &self,
        profile: datazen_schema_diff::SchemaDiffProfile,
        policy: UnattendedDestructivePolicy,
        reference: &MigrationProfileRef,
    ) -> Result<MigrationOutcome, WorkflowError> {
        datazen_feature_core::workflow::migration::run_schema_step(self, profile, reference, policy)
            .await
    }
}

async fn open_session(
    state: &AppState,
    connection_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
    label: &str,
) -> Result<datazen_feature_core::workflow::migration::MigrationSession, WorkflowError> {
    let configured_database = database
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    let database = match configured_database {
        Some(database) => database,
        None => state
            .store
            .get_connection(connection_id)
            .await
            .and_then(|config| config.database)
            .ok_or_else(|| {
                WorkflowError::Validation(format!("{label} connection has no selected database"))
            })?,
    };
    let db_session_id = crate::services::migration_endpoint::open_dedicated_session(
        state,
        connection_id,
        &database,
        "workflow-migration",
    )
    .await
    .map_err(|error| WorkflowError::ConnectionFailed {
        connection_id: connection_id.into(),
        message: error.to_string(),
    })?;
    let context = crate::services::migration_endpoint::runtime_session_context(
        state,
        &db_session_id,
        datazen_runtime::application::LivePurpose::LongLivedHostWork,
    )
    .await
    .map_err(migration_command_error)?;
    let database = context
        .view
        .observed_context
        .namespace
        .database
        .clone()
        .or(context.config.database.clone())
        .unwrap_or(database);
    Ok(
        datazen_feature_core::workflow::migration::MigrationSession {
            db_session_id,
            database,
            schema: schema
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or(context
                    .view
                    .observed_context
                    .namespace
                    .schema
                    .clone()
                    .or(context.config.schema.clone())),
        },
    )
}

async fn release_session(
    state: &AppState,
    session: &datazen_feature_core::workflow::migration::MigrationSession,
) {
    if let Err(error) =
        crate::services::migration_endpoint::close_dedicated_session(state, &session.db_session_id)
            .await
    {
        tracing::warn!(db_session_id = %session.db_session_id, %error, "failed to release workflow migration session");
    }
}

async fn run_transfer(
    state: &AppState,
    job: crate::data_transfer::TransferJob,
    destructive_policy: UnattendedDestructivePolicy,
) -> Result<MigrationOutcome, WorkflowError> {
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

async fn run_sync(
    state: &AppState,
    request: datazen_feature_core::workflow::migration::SyncStepRequest,
) -> Result<MigrationOutcome, WorkflowError> {
    let preview = crate::commands::compare_data_sync_impl(
        state,
        request.source_db_session_id,
        request.target_db_session_id,
        request.tables,
        None,
        Some(request.source_database),
        Some(request.target_database),
        request.source_schema,
        request.target_schema,
        request.options.clone(),
        &request.mappings,
        &request.filters,
    )
    .await
    .map_err(|error| WorkflowError::Validation(error.to_string()))?;
    let result = crate::commands::execute_data_sync_profile_plan_impl(
        state,
        preview.plan_id,
        preview.selection_revision,
        request.options,
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

async fn run_schema(
    state: &AppState,
    request: datazen_feature_core::workflow::migration::SchemaStepRequest,
    reference: &MigrationProfileRef,
    destructive_policy: UnattendedDestructivePolicy,
) -> Result<MigrationOutcome, WorkflowError> {
    let plan = crate::commands::prepare_schema_diff_profile_plan_impl(
        state,
        request.source_db_session_id,
        request.target_db_session_id.clone(),
        request.tables,
        request.target_only_tables,
        request.allow_destructive,
        Some(request.include_indexes),
        Some(request.type_overrides),
        request.source_schema,
        request.target_schema,
    )
    .await
    .map_err(|error| WorkflowError::Validation(error.to_string()))?;
    let result = crate::commands::execute_schema_diff_deploy_impl(
        state,
        request.target_db_session_id.clone(),
        plan.clone(),
        Some(true),
        Some(request.require_rollback),
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
            crate::schema_diff::DeployStatus::Unknown | crate::schema_diff::DeployStatus::Mixed => {
                "unknown"
            }
            _ => "notRequired",
        },
    })
}

use datazen_feature_core::workflow::migration::transfer_migration_outcome;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_transfer::model::{
        TableExecutionOutcome, TableExecutionResult, TransferExecutionResult, TransferOptions,
    };
    use crate::data_transfer::{TransferMode, TransferProfile, WriteMode};
    use crate::store::MigrationRunFilter;
    use crate::testing::app_state::TestAppState;
    use chrono::Utc;
    use datazen_feature_core::workflow::model::{
        WorkflowDefinition, WorkflowMigrationOperation, WorkflowStep, WorkflowVisibility,
    };

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
}

/// The desktop implementation of the shared migration runner: it owns the
/// runtime session and the wizard commands, while the composition (which
/// sessions to open, how the profile becomes a plan, when they are released)
/// lives in `datazen_feature_core::workflow::migration`.
#[async_trait::async_trait]
impl datazen_feature_core::workflow::migration::MigrationRunner for AppState {
    async fn open_session(
        &self,
        connection_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        label: &str,
    ) -> Result<datazen_feature_core::workflow::migration::MigrationSession, WorkflowError> {
        open_session(self, connection_id, database, schema, label).await
    }

    async fn release_session(
        &self,
        session: &datazen_feature_core::workflow::migration::MigrationSession,
    ) {
        release_session(self, session).await
    }

    async fn run_transfer(
        &self,
        job: crate::data_transfer::TransferJob,
        destructive_policy: UnattendedDestructivePolicy,
    ) -> Result<MigrationOutcome, WorkflowError> {
        run_transfer(self, job, destructive_policy).await
    }

    async fn run_sync(
        &self,
        request: datazen_feature_core::workflow::migration::SyncStepRequest,
    ) -> Result<MigrationOutcome, WorkflowError> {
        run_sync(self, request).await
    }

    async fn run_schema(
        &self,
        request: datazen_feature_core::workflow::migration::SchemaStepRequest,
        reference: &MigrationProfileRef,
        destructive_policy: UnattendedDestructivePolicy,
    ) -> Result<MigrationOutcome, WorkflowError> {
        run_schema(self, request, reference, destructive_policy).await
    }
}
