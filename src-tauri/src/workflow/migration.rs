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
use crate::data_transfer::{SqlFileTarget, TransferJob, TransferRunRequest};
use crate::store::MigrationProfileRef;
use crate::workflow::model::{
    StepExecutionResult, UnattendedDestructivePolicy, WorkflowMigrationOperation,
};
use crate::workflow::workflows::context::WorkflowContext;
use crate::workflow::WorkflowError;

struct RuntimeSession {
    db_session_id: String,
    database: String,
    schema: Option<String>,
}

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
        run_transfer(self, profile, token_variable, context, policy).await
    }
    async fn run_sync(
        &self,
        profile: datazen_data_sync::SyncProfile,
        policy: UnattendedDestructivePolicy,
    ) -> Result<MigrationOutcome, WorkflowError> {
        run_sync(self, profile, policy).await
    }
    async fn run_schema(
        &self,
        profile: datazen_schema_diff::SchemaDiffProfile,
        policy: UnattendedDestructivePolicy,
        reference: &MigrationProfileRef,
    ) -> Result<MigrationOutcome, WorkflowError> {
        run_schema(self, profile, policy, reference).await
    }
}

async fn open_session(
    state: &AppState,
    connection_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
    label: &str,
) -> Result<RuntimeSession, WorkflowError> {
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
    Ok(RuntimeSession {
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
    })
}

async fn release_session(state: &AppState, session: &RuntimeSession) {
    if let Err(error) =
        crate::services::migration_endpoint::close_dedicated_session(state, &session.db_session_id)
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

use datazen_feature_core::workflow::migration::{
    parse_sql_file_compression, parse_sql_file_encoding, transfer_migration_outcome,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_transfer::model::{
        TableExecutionOutcome, TableExecutionResult, TransferExecutionResult, TransferOptions,
    };
    use crate::data_transfer::{SqlFileCompression, SqlFileEncoding};
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
