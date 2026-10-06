//! Dedicated Data Sync execute IPC (bypasses sql_guard / execute_query).

use super::super::error::{CmdExt, CommandError};
use super::super::AppState;
use super::apply::generate_data_sync_sql_impl;
use super::comparison_store::{ComparisonStore, ComparisonTableMetadata};
use super::host::state::StoredSelection;
use super::plans::{self, SelectionMatcher, StoredSyncPlan, SyncRunRequest, SyncRunSelection};
#[cfg(test)]
use crate::data_sync::execute_statement_batches_with_policy;
#[cfg(test)]
use crate::data_sync::execute_statements;
use crate::data_sync::job::ApplySpec;
use crate::data_sync::{
    DataSyncError, ExecutionResult, IdentityInsertTarget, StatementBatchSource, StatementExecutor,
    SyncOptions,
};
use crate::db::{ConnectionHandle, DatabaseDriver, TransactionHandle, Value};
use async_trait::async_trait;
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::JobState;
#[cfg(feature = "webdriver")]
use std::sync::atomic::AtomicU8;
#[cfg(feature = "webdriver")]
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[cfg(feature = "webdriver")]
static E2E_COMMIT_FAULT: AtomicU8 = AtomicU8::new(0);

#[cfg(feature = "webdriver")]
pub(crate) fn set_e2e_commit_fault(fault: &str) -> Result<(), String> {
    let value = match fault {
        "lost_after_commit" => 1,
        "lost_before_commit" => 2,
        "none" => 0,
        _ => return Err("unsupported Data Sync E2E commit fault".into()),
    };
    E2E_COMMIT_FAULT.store(value, Ordering::SeqCst);
    Ok(())
}

struct LiveExecutor {
    driver: Arc<dyn DatabaseDriver>,
    handle: ConnectionHandle,
    read_only: bool,
    tx: Option<TransactionHandle>,
}

#[async_trait]
impl StatementExecutor for LiveExecutor {
    fn is_read_only(&self) -> bool {
        self.read_only
    }

    async fn begin(&mut self) -> Result<(), crate::data_sync::DataSyncError> {
        let tx = self
            .driver
            .begin_transaction(&self.handle)
            .await
            .map_err(|e| crate::data_sync::DataSyncError::validation(e.to_string()))?;
        self.tx = Some(tx);
        Ok(())
    }

    async fn execute(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, crate::data_sync::DataSyncError> {
        self.driver
            .execute_with_params(&self.handle, sql, params)
            .await
            .map_err(|e| crate::data_sync::DataSyncError::validation(e.to_string()))
    }

    async fn set_identity_insert(
        &mut self,
        target: &IdentityInsertTarget,
        enabled: bool,
    ) -> Result<(), crate::data_sync::DataSyncError> {
        self.driver
            .set_identity_insert(
                &self.handle,
                &target.database,
                target.schema.as_deref(),
                &target.table,
                enabled,
            )
            .await
            .map_err(|error| DataSyncError::validation(error.to_string()))
    }

    async fn discard_connection(&mut self) -> Result<(), crate::data_sync::DataSyncError> {
        self.driver
            .discard_connection(&self.handle)
            .await
            .map_err(|error| DataSyncError::validation(error.to_string()))
    }

    async fn commit(&mut self) -> Result<(), crate::data_sync::DataSyncError> {
        if let Some(tx) = self.tx.take() {
            #[cfg(feature = "webdriver")]
            match E2E_COMMIT_FAULT.swap(0, Ordering::SeqCst) {
                1 => {
                    self.driver
                        .commit(tx)
                        .await
                        .map_err(|e| crate::data_sync::DataSyncError::validation(e.to_string()))?;
                    return Err(crate::data_sync::DataSyncError::outcome_unknown(
                        "E2E injected commit response loss after commit",
                    ));
                }
                2 => {
                    // Drop the rollback acknowledgement too, modelling a lost
                    // response at the transaction boundary. This branch exists
                    // only in the webdriver build and consumes one armed fault.
                    let _ = self.driver.rollback(tx).await;
                    return Err(crate::data_sync::DataSyncError::outcome_unknown(
                        "E2E injected commit response loss before commit",
                    ));
                }
                _ => {}
            }
            self.driver
                .commit(tx)
                .await
                .map_err(|e| crate::data_sync::DataSyncError::validation(e.to_string()))?;
        }
        Ok(())
    }

    async fn rollback(&mut self) -> Result<(), crate::data_sync::DataSyncError> {
        if let Some(tx) = self.tx.take() {
            self.driver
                .rollback(tx)
                .await
                .map_err(|e| crate::data_sync::DataSyncError::validation(e.to_string()))?;
        }
        Ok(())
    }
}

/// The IPC preview still returns a single JSON array, so keep that response
/// explicitly bounded while the execution path consumes one generated page
/// at a time.
const SQL_PREVIEW_IPC_MAX_BYTES: usize = 16 * 1024 * 1024;

/// Refuse an apply whose target can no longer be trusted, before the Job is
/// submitted.
///
/// The target connection is opened and its schema fingerprint compared here so
/// a plan reviewed against a different schema cannot be applied. The opened
/// handle is not returned: the apply Job re-resolves its own session through the
/// handler, so holding it would only pin a connection for the sake of a value
/// nobody reads.
async fn validate_plan_context(
    state: &AppState,
    plan: &StoredSyncPlan,
) -> Result<(), CommandError> {
    let comparison = plan
        .comparison
        .summaries()
        .map_err(CommandError::Validation)?;
    if plan.target_read_only_at_preview {
        return Err(CommandError::Validation(
            "target connection was read-only during comparison; return to comparison".into(),
        ));
    }
    let source_config = state
        .connection_manager
        .get_session_config(&plan.source_db_session_id)
        .await
        .cmd_err("validate_data_sync_plan")?;
    let target_config = state
        .connection_manager
        .get_session_config(&plan.target_db_session_id)
        .await
        .cmd_err("validate_data_sync_plan")?;
    if target_config.read_only {
        return Err(CommandError::Validation(
            "target connection is now read-only; return to comparison".into(),
        ));
    }
    validate_active_database(
        source_config.database.as_deref(),
        &plan.source_database,
        "source",
    )?;
    validate_active_database(
        target_config.database.as_deref(),
        &plan.target_database,
        "target",
    )?;
    let (source_driver, source_handle) = state
        .connection_manager
        .get_session(&plan.source_db_session_id)
        .await
        .cmd_err("validate_data_sync_plan")?;
    let (target_driver, target_handle) = state
        .connection_manager
        .get_session(&plan.target_db_session_id)
        .await
        .cmd_err("validate_data_sync_plan")?;
    if source_driver.driver_type() != plan.source_driver_type
        || target_driver.driver_type() != plan.target_driver_type
        || plans::driver_protocol_version(source_driver.as_ref()) != plan.source_driver_protocol
        || plans::driver_protocol_version(target_driver.as_ref()) != plan.target_driver_protocol
    {
        return Err(CommandError::Validation(
            "driver contract changed since comparison; return to comparison".into(),
        ));
    }
    let source_fingerprint = current_schema_fingerprint(
        source_driver.as_ref(),
        &source_handle,
        &plan.source_db_session_id,
        &plan.source_database,
        plan.source_schema.as_deref(),
        &comparison,
        true,
    )
    .await?;
    let target_fingerprint = current_schema_fingerprint(
        target_driver.as_ref(),
        &target_handle,
        &plan.target_db_session_id,
        &plan.target_database,
        plan.target_schema.as_deref(),
        &comparison,
        false,
    )
    .await?;
    if source_fingerprint != plan.source_schema_fingerprint {
        tracing::warn!(
            expected = %plan.source_schema_fingerprint,
            actual = %source_fingerprint,
            "sync plan source schema fingerprint changed"
        );
        return Err(CommandError::Validation(
            "source schema/key changed since comparison; return to comparison".into(),
        ));
    }
    if target_fingerprint != plan.target_schema_fingerprint {
        tracing::warn!(
            expected = %plan.target_schema_fingerprint,
            actual = %target_fingerprint,
            "sync plan target schema fingerprint changed"
        );
        return Err(CommandError::Validation(
            "target schema/key changed since comparison; return to comparison".into(),
        ));
    }
    // The connection is dropped with this scope: the Job re-resolves it, and a
    // handle kept only to be discarded would pin the session for the whole
    // apply while adding nothing to the check above.
    drop(target_driver);
    drop(target_handle);
    Ok(())
}

fn validate_active_database(
    active_database: Option<&str>,
    planned_database: &str,
    side: &str,
) -> Result<(), CommandError> {
    let active = active_database
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let planned = planned_database.trim();
    if planned.is_empty() || active != Some(planned) {
        return Err(CommandError::Validation(format!(
            "{side} session active database changed or cannot be confirmed; return to comparison"
        )));
    }
    Ok(())
}

async fn current_schema_fingerprint(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    _session_id: &str,
    database: &str,
    schema: Option<&str>,
    comparison: &[ComparisonTableMetadata],
    source: bool,
) -> Result<String, CommandError> {
    let mut entries = Vec::new();
    for table in comparison
        .iter()
        .filter(|table| table.table.status == crate::data_sync::TableMappingStatus::Matched)
    {
        let relation = if source {
            &table.table.source_table
        } else {
            &table.table.target_table
        };
        let schema_snapshot = driver
            .get_table_schema(handle, relation, database, schema)
            .await
            .map_err(|_| {
                CommandError::Validation(format!(
                    "cannot confirm current {} schema metadata; return to comparison",
                    if source { "source" } else { "target" }
                ))
            })?;
        entries.push((
            relation.clone(),
            Some(schema_snapshot),
            table.table.source_filter.clone(),
        ));
    }
    plans::fingerprint_relations_with_filters(database, schema, entries)
        .map_err(CommandError::Validation)
}

struct StoreSqlPageSource<'a> {
    state: &'a AppState,
    target_db_session_id: &'a str,
    comparison: &'a ComparisonStore,
    matcher: &'a SelectionMatcher,
    options: &'a SyncOptions,
    target_database: Option<&'a str>,
    target_schema: Option<&'a str>,
    summaries: Vec<ComparisonTableMetadata>,
    table_index: usize,
    row_offset: usize,
}

impl<'a> StoreSqlPageSource<'a> {
    fn new(
        state: &'a AppState,
        target_db_session_id: &'a str,
        comparison: &'a ComparisonStore,
        matcher: &'a SelectionMatcher,
        options: &'a SyncOptions,
        target_database: Option<&'a str>,
        target_schema: Option<&'a str>,
    ) -> Result<Self, CommandError> {
        let summaries = comparison.summaries().map_err(CommandError::Validation)?;
        Ok(Self {
            state,
            target_db_session_id,
            comparison,
            matcher,
            options,
            target_database,
            target_schema,
            summaries,
            table_index: 0,
            row_offset: 0,
        })
    }

    async fn next_sql_batch(
        &mut self,
    ) -> Result<Option<Vec<crate::data_sync::SqlStatement>>, CommandError> {
        while self.table_index < self.summaries.len() {
            let table = &self.summaries[self.table_index];
            if table.table.status != crate::data_sync::TableMappingStatus::Matched
                || self.row_offset >= table.row_count
            {
                self.table_index += 1;
                self.row_offset = 0;
                continue;
            }

            let rows = self
                .comparison
                .load_table_page(
                    &table.table.source_table,
                    &table.table.target_table,
                    self.row_offset,
                    plans::SYNC_COMPARISON_STREAM_PAGE_SIZE,
                )
                .map_err(CommandError::Validation)?;
            if rows.is_empty() {
                return Err(CommandError::Validation(
                    "comparison page did not advance while generating SQL".into(),
                ));
            }
            self.row_offset = self.row_offset.saturating_add(rows.len());
            let Some(table_page) =
                plans::selected_table_page(table, rows, self.matcher, self.options)
                    .map_err(CommandError::Validation)?
            else {
                continue;
            };
            let statements = generate_data_sync_sql_impl(
                self.state,
                self.target_db_session_id.to_string(),
                vec![table_page],
                self.options.clone(),
                self.target_database.map(str::to_string),
                self.target_schema.map(str::to_string),
            )
            .await?;
            if !statements.is_empty() {
                return Ok(Some(statements));
            }
        }
        Ok(None)
    }
}

#[async_trait]
impl StatementBatchSource for StoreSqlPageSource<'_> {
    async fn next_batch(
        &mut self,
    ) -> Result<Option<Vec<crate::data_sync::SqlStatement>>, DataSyncError> {
        self.next_sql_batch()
            .await
            .map_err(|error| DataSyncError::validation(error.to_string()))
    }
}

/// SQL preview is still one IPC array, so reject it as soon as its serialized
/// response would exceed the documented cap. Execution uses the same page
/// source directly and never accumulates this preview vector.
async fn generate_bounded_data_sync_sql_preview(
    mut source: StoreSqlPageSource<'_>,
) -> Result<Vec<crate::data_sync::SqlStatement>, CommandError> {
    let mut statements = Vec::new();
    let mut response_bytes = 2usize; // JSON array brackets
    while let Some(batch) = source.next_sql_batch().await? {
        for statement in batch {
            append_bounded_preview_statement(&mut statements, &mut response_bytes, statement)?;
        }
    }
    Ok(statements)
}

fn append_bounded_preview_statement(
    statements: &mut Vec<crate::data_sync::SqlStatement>,
    response_bytes: &mut usize,
    statement: crate::data_sync::SqlStatement,
) -> Result<(), CommandError> {
    let statement_bytes = serde_json::to_vec(&statement)
        .map_err(|error| CommandError::Validation(error.to_string()))?
        .len();
    let separator_bytes = usize::from(!statements.is_empty());
    let next_size = response_bytes
        .saturating_add(separator_bytes)
        .saturating_add(statement_bytes);
    if next_size > SQL_PREVIEW_IPC_MAX_BYTES {
        return Err(CommandError::Validation(format!(
            "Data Sync SQL preview exceeds the {} MiB IPC limit; select fewer rows or operations",
            SQL_PREVIEW_IPC_MAX_BYTES / (1024 * 1024)
        )));
    }
    *response_bytes = next_size;
    statements.push(statement);
    Ok(())
}

fn validate_requested_options(
    plan: &StoredSyncPlan,
    options: &SyncOptions,
) -> Result<(), CommandError> {
    options.validate().map_err(CommandError::from)?;
    if options.matching_strategy != plan.options.matching_strategy
        || options.large_value_mode != plan.options.large_value_mode
        || options.conflict_policy != plan.options.conflict_policy
        || plans::fingerprint_conflict_policy(options.conflict_policy)
            != plan.conflict_policy_fingerprint
    {
        return Err(CommandError::Validation(
            "comparison options changed; return to comparison".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn generate_data_sync_sql_for_plan_impl(
    state: &AppState,
    plan_id: String,
    selection: SyncRunSelection,
    options: SyncOptions,
) -> Result<Vec<crate::data_sync::SqlStatement>, CommandError> {
    let plan = plans::peek_plan(&plan_id).map_err(CommandError::Validation)?;
    if selection.revision != plan.selection_revision {
        return Err(CommandError::Validation(
            "selection revision is stale; return to comparison".into(),
        ));
    }
    validate_requested_options(&plan, &options)?;
    let matcher = plans::validate_selection_streaming(&plan.comparison, &selection, &options)
        .map_err(CommandError::Validation)?;
    let _context = validate_plan_context(state, &plan).await?;
    generate_bounded_data_sync_sql_preview(StoreSqlPageSource::new(
        state,
        &plan.target_db_session_id,
        &plan.comparison,
        &matcher,
        &options,
        Some(&plan.target_database),
        plan.target_schema.as_deref(),
    )?)
    .await
}

pub(crate) async fn execute_data_sync_plan_impl(
    state: &AppState,
    request: SyncRunRequest,
) -> Result<ExecutionResult, CommandError> {
    if request.plan_id.trim().is_empty() {
        return Err(CommandError::Validation(
            "execute_data_sync requires a comparison planId".into(),
        ));
    }
    let plan = plans::peek_plan(&request.plan_id).map_err(CommandError::Validation)?;
    if request.selection.revision != plan.selection_revision {
        return Err(CommandError::Validation(
            "selection revision is stale; return to comparison".into(),
        ));
    }
    validate_requested_options(&plan, &request.options)?;
    let _matcher =
        plans::validate_selection_streaming(&plan.comparison, &request.selection, &request.options)
            .map_err(CommandError::Validation)?;
    // The endpoint context is still proved here so a stale session fails before
    // any Job exists: the apply Job reports its own effect outcome, and a
    // preflight refusal must never be reported as "unknown" (§7).
    validate_plan_context(state, &plan).await?;
    // The reviewed selection is handed to the Job as the only mutable input it
    // may act on, and the Job consumes it exactly once.
    super::host::state::store_selection(
        &request.plan_id,
        StoredSelection {
            selection: request.selection,
            options: request.options.clone(),
        },
    );
    let outcome = super::jobs::submit_apply(
        state,
        ApplySpec {
            plan_id: request.plan_id,
            selection_revision: plan.selection_revision,
            options: request.options,
        },
        request.job_id.clone(),
    )
    .await;
    if let Some(id) = request.job_id.as_deref() {
        super::jobs::remove_job(id).await;
    }
    applied_outcome(outcome)
}

/// Map the Job's terminal projection onto the IPC contract.
///
/// §7 decides every branch: a Job whose recorded effect is `NotStarted` or
/// `RolledBack` is a refusal, `Unknown` is a fence that must never be reported
/// as "did not start", and `PartiallyApplied` is a success with a warning
/// because rows already committed cannot be unwound by the caller.
fn applied_outcome(
    outcome: Result<super::jobs::SyncJobOutcome, CommandError>,
) -> Result<ExecutionResult, CommandError> {
    let applied = outcome?;
    let committed = applied.committed as usize;
    let message = applied.message.clone();
    match applied.state {
        JobState::Succeeded => Ok(ExecutionResult {
            applied: committed,
            affected_rows: applied.committed,
            rolled_back: false,
            rollback_reason: None,
            skipped: 0,
            conflicts: Vec::new(),
        }),
        JobState::Cancelled => Ok(ExecutionResult {
            applied: committed,
            affected_rows: applied.committed,
            rolled_back: applied.effect == EffectOutcome::RolledBack,
            rollback_reason: message,
            skipped: 0,
            conflicts: Vec::new(),
        }),
        JobState::Failed => Err(match applied.effect {
            EffectOutcome::Unknown => CommandError::DataSyncOutcomeUnknown(
                message.unwrap_or_else(|| {
                    "commit or rollback could not be confirmed; compare current data before continuing"
                        .to_string()
                }),
            ),
            EffectOutcome::PartiallyApplied => CommandError::Validation(
                message.unwrap_or_else(|| {
                    "some rows committed before the failure; compare current data before continuing"
                        .to_string()
                }),
            ),
            _ => CommandError::DataSyncNotStarted(message.unwrap_or_else(|| {
                "the apply job did not start; compare again".to_string()
            })),
        }),
        JobState::Queued | JobState::Running => Err(CommandError::Internal(message.unwrap_or_else(
            || "the apply job was still running when the request returned".to_string(),
        ))),
    }
}

// Kept only for the existing Rust command-path unit tests.  This helper is
// cfg(test) and is not registered as an IPC command; production execution can
// only enter through `execute_data_sync_plan_impl`.
#[cfg(test)]
pub(crate) async fn execute_data_sync_impl(
    state: &AppState,
    target_db_session_id: String,
    statements: Vec<crate::data_sync::SqlStatement>,
    job_id: Option<String>,
    _target_database: Option<String>,
) -> Result<ExecutionResult, CommandError> {
    let config = state
        .connection_manager
        .get_session_config(&target_db_session_id)
        .await
        .cmd_err("execute_data_sync")?;
    let (driver, handle) = state
        .connection_manager
        .get_session(&target_db_session_id)
        .await
        .cmd_err("execute_data_sync")?;
    let mut executor = LiveExecutor {
        driver,
        handle,
        read_only: config.read_only,
        tx: None,
    };
    let cancelled = match job_id.as_deref() {
        Some(id) => Some(super::jobs::ensure_job(id).await),
        None => None,
    };
    let result = execute_statements(&statements, &mut executor, cancelled)
        .await
        .map_err(CommandError::from);
    if let Some(id) = job_id.as_deref() {
        super::jobs::remove_job(id).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::jobs_contract;
    use super::*;
    use crate::commands::sync::plans::{SyncRunSelection, SyncSelectionMode, SyncTableSelection};
    use crate::data_sync::execute::RecordingExecutor;
    use crate::data_sync::{
        ChangeOperation, ComparisonResult, ConflictPolicy, RowChange, TableResult,
    };
    use crate::testing::app_state::{rich_mock_options, TestAppState};
    use crate::testing::mock_driver::MockDriverOptions;

    #[tokio::test]
    async fn execution_streams_a_plan_larger_than_64_mib_in_order_without_full_load() {
        let test = TestAppState::with_options(MockDriverOptions {
            parameterized_writes: true,
            ..rich_mock_options()
        })
        .await;
        test.save_and_connect("sync-stream-target").await;
        let target_db_session_id = test.connect_config("sync-stream-target").await;

        let mut options = SyncOptions::default();
        options.conflict_policy = ConflictPolicy::Force;
        let mut rows = Vec::with_capacity(plans::SYNC_COMPARISON_STREAM_PAGE_SIZE + 1);
        rows.push(RowChange::insert(
            vec![Value::Integer(0)],
            vec![
                Some(Value::Integer(0)),
                Some(Value::String("x".repeat(
                    super::super::comparison_store::COMPARISON_FULL_LOAD_LIMIT as usize + 1,
                ))),
            ],
            &options,
        ));
        rows.extend((1..plans::SYNC_COMPARISON_STREAM_PAGE_SIZE).map(|key| {
            RowChange::insert(
                vec![Value::Integer(key as i64)],
                vec![
                    Some(Value::Integer(key as i64)),
                    Some(Value::String(format!("name-{key}"))),
                ],
                &options,
            )
        }));
        rows.push(RowChange::update(
            vec![Value::Integer(
                plans::SYNC_COMPARISON_STREAM_PAGE_SIZE as i64,
            )],
            vec![
                Some(Value::Integer(
                    plans::SYNC_COMPARISON_STREAM_PAGE_SIZE as i64,
                )),
                Some(Value::String("new-name".into())),
            ],
            vec![
                Some(Value::Integer(
                    plans::SYNC_COMPARISON_STREAM_PAGE_SIZE as i64,
                )),
                Some(Value::String("old-name".into())),
            ],
            vec!["name".into()],
            &options,
        ));
        let mut table = TableResult::matched("users", "users", rows);
        table.columns = vec!["id".into(), "name".into()];
        table.column_types = vec!["integer".into(), "text".into()];
        table.primary_keys = vec!["id".into()];
        let comparison = ComparisonStore::from_comparison(ComparisonResult::new(vec![table]))
            .expect("comparison should persist");
        assert!(comparison.is_spilled());

        let selection = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![SyncTableSelection {
                source_table: "users".into(),
                target_table: "users".into(),
                selection_mode: SyncSelectionMode::All,
                operations: vec![ChangeOperation::Insert, ChangeOperation::Update],
                excluded_rows: Vec::new(),
            }],
        };
        let matcher = plans::validate_selection_streaming(&comparison, &selection, &options)
            .expect("selection should match the persisted comparison");
        let mut source = StoreSqlPageSource::new(
            &test.state,
            &target_db_session_id,
            &comparison,
            &matcher,
            &options,
            Some("app"),
            None,
        )
        .expect("stream source should initialize");
        let first_batch = source
            .next_sql_batch()
            .await
            .expect("first bounded page should generate")
            .expect("first page should have statements");

        assert_eq!(first_batch.len(), plans::SYNC_COMPARISON_STREAM_PAGE_SIZE);
        assert!(matches!(
            first_batch
                .first()
                .map(|statement| statement.row_key.as_slice()),
            Some([Value::Integer(0)])
        ));
        assert_eq!(
            first_batch
                .last()
                .map(|statement| statement.parameters.len()),
            Some(2),
            "force policy must omit optimistic target-row predicates"
        );

        let mut executor = RecordingExecutor::default();
        let result = execute_statement_batches_with_policy(
            first_batch,
            &mut source,
            &mut executor,
            None,
            ConflictPolicy::Force,
        )
        .await
        .expect("streamed statements should execute");

        assert_eq!(result.applied, plans::SYNC_COMPARISON_STREAM_PAGE_SIZE + 1);
        assert!(!result.rolled_back);
        assert_eq!(
            executor.calls.len(),
            plans::SYNC_COMPARISON_STREAM_PAGE_SIZE + 3
        );
        assert!(executor.calls[1].contains("INSERT"));
        assert!(executor.calls[plans::SYNC_COMPARISON_STREAM_PAGE_SIZE].contains("INSERT"));
        assert!(executor.calls[plans::SYNC_COMPARISON_STREAM_PAGE_SIZE + 1].contains("UPDATE"));
        assert_eq!(executor.calls.last().map(String::as_str), Some("commit"));
        assert_eq!(comparison.full_load_calls(), 0);
        assert_eq!(
            test.mock.get_schema_calls(),
            2,
            "501 rows must be generated as two independently bounded pages"
        );
    }

    #[test]
    fn sql_preview_rejects_a_statement_that_exceeds_the_ipc_bound() {
        let statement = crate::data_sync::SqlStatement {
            table: "users".into(),
            operation: ChangeOperation::Insert,
            sql: "INSERT INTO users VALUES (?)".into(),
            preview_sql: "INSERT INTO users VALUES (?)".into(),
            parameters: vec![Value::String("x".repeat(SQL_PREVIEW_IPC_MAX_BYTES + 1))],
            row_key: vec![Value::Integer(1)],
            identity_insert: None,
        };
        let mut statements = Vec::new();
        let mut response_bytes = 2;
        let error =
            append_bounded_preview_statement(&mut statements, &mut response_bytes, statement)
                .unwrap_err();

        assert!(error.to_string().contains("16 MiB IPC limit"));
        assert!(error.to_string().contains("select fewer rows"));
        assert!(statements.is_empty());
    }

    #[test]
    fn test_tester_sql_preview_counts_each_statement_and_json_separator() {
        let first = crate::data_sync::SqlStatement {
            table: "users".into(),
            operation: ChangeOperation::Insert,
            sql: "INSERT INTO users VALUES (?)".into(),
            preview_sql: "INSERT INTO users VALUES (?)".into(),
            parameters: vec![Value::String("first".into())],
            row_key: vec![Value::Integer(1)],
            identity_insert: None,
        };
        let second = crate::data_sync::SqlStatement {
            parameters: vec![Value::String("second".into())],
            row_key: vec![Value::Integer(2)],
            ..first.clone()
        };
        let expected_bytes = 2
            + serde_json::to_vec(&first).unwrap().len()
            + 1
            + serde_json::to_vec(&second).unwrap().len();
        let mut statements = Vec::new();
        let mut response_bytes = 2;

        append_bounded_preview_statement(&mut statements, &mut response_bytes, first).unwrap();
        append_bounded_preview_statement(&mut statements, &mut response_bytes, second).unwrap();

        assert_eq!(statements.len(), 2);
        assert_eq!(response_bytes, expected_bytes);
    }

    #[tokio::test]
    async fn test_tester_bounded_sql_preview_returns_selected_rows_in_order() {
        let test = TestAppState::with_options(MockDriverOptions {
            parameterized_writes: true,
            ..rich_mock_options()
        })
        .await;
        test.save_and_connect("sync-preview-target").await;
        let target_db_session_id = test.connect_config("sync-preview-target").await;
        let options = SyncOptions::default();
        let rows = (1..=2)
            .map(|key| {
                RowChange::insert(
                    vec![Value::Integer(key)],
                    vec![
                        Some(Value::Integer(key)),
                        Some(Value::String(format!("name-{key}"))),
                    ],
                    &options,
                )
            })
            .collect();
        let mut table = TableResult::matched("users", "users", rows);
        table.columns = vec!["id".into(), "name".into()];
        table.column_types = vec!["integer".into(), "text".into()];
        table.primary_keys = vec!["id".into()];
        let comparison =
            ComparisonStore::from_comparison(ComparisonResult::new(vec![table])).unwrap();
        let selection = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![SyncTableSelection {
                source_table: "users".into(),
                target_table: "users".into(),
                selection_mode: SyncSelectionMode::All,
                operations: vec![ChangeOperation::Insert],
                excluded_rows: Vec::new(),
            }],
        };
        let matcher =
            plans::validate_selection_streaming(&comparison, &selection, &options).unwrap();
        let source = StoreSqlPageSource::new(
            &test.state,
            &target_db_session_id,
            &comparison,
            &matcher,
            &options,
            Some("app"),
            None,
        )
        .unwrap();

        let statements = generate_bounded_data_sync_sql_preview(source)
            .await
            .unwrap();

        assert_eq!(statements.len(), 2);
        let row_keys = statements
            .iter()
            .map(|statement| statement.row_key.as_slice())
            .collect::<Vec<_>>();
        assert_eq!(row_keys.len(), 2);
        assert!(matches!(row_keys[0], [Value::Integer(1)]));
        assert!(matches!(row_keys[1], [Value::Integer(2)]));
    }

    #[tokio::test]
    async fn test_tester_plan_preview_and_execution_use_the_paged_path() {
        // A real pair of endpoints: the preview text and the execution must both
        // come from the ChangeSet the prepare Job froze, not from a plan minted
        // by hand (which carries no Artifact and is refused at submit time).
        let pair = jobs_contract::diff_pair("paged-plan").await;
        let preview = jobs_contract::compare(&pair, None, None).await.unwrap();
        let options = SyncOptions::default();
        let selection = jobs_contract::insert_selection(preview.selection_revision);

        let sql = generate_data_sync_sql_for_plan_impl(
            pair.state(),
            preview.plan_id.clone(),
            selection.clone(),
            options.clone(),
        )
        .await
        .unwrap();
        assert_eq!(sql.len(), 1, "one pending row ⇒ one preview statement");

        let result = execute_data_sync_plan_impl(
            pair.state(),
            SyncRunRequest {
                plan_id: preview.plan_id,
                selection,
                options,
                job_id: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(result.applied, 1);
        assert!(!result.rolled_back);
        assert_eq!(pair.test.mock.open_transaction_count(), 0);
        assert_eq!(pair.target.open_transaction_count(), 0);
    }

    #[tokio::test]
    async fn test_tester_empty_streamed_plan_is_rejected_without_consuming_the_plan() {
        // Both endpoints already agree ⇒ the change set is empty. The refusal
        // happens before `accept`, so the planId stays unconsumed and the user
        // can re-compare instead of being locked out by a spent plan.
        let pair = jobs_contract::identical_pair("empty-plan").await;
        let preview = jobs_contract::compare(&pair, None, None).await.unwrap();
        let request = || SyncRunRequest {
            plan_id: preview.plan_id.clone(),
            selection: jobs_contract::insert_selection(preview.selection_revision),
            options: SyncOptions::default(),
            job_id: None,
        };

        let first = execute_data_sync_plan_impl(pair.state(), request())
            .await
            .unwrap_err();
        assert!(first.to_string().contains("change set is empty"));

        // Same planId, same refusal: a spent planId would answer "already
        // submitted" here instead.
        let second = execute_data_sync_plan_impl(pair.state(), request())
            .await
            .unwrap_err();
        assert!(second.to_string().contains("change set is empty"));

        assert!(plans::peek_plan(&preview.plan_id).is_ok());
        assert_eq!(pair.test.mock.open_transaction_count(), 0);
        assert_eq!(pair.target.open_transaction_count(), 0);
    }
}
