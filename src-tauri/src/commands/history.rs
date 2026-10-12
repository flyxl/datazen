use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::data_sync::{DataSyncExecutionResponse, ExecutionOutcome};
use crate::store::MigrationProfileRef;
use crate::store::{HistoryScope, MigrationRunFilter, MigrationRunPage, MigrationRunRecord};
use chrono::Utc;
use tauri::State;
use uuid::Uuid;

pub(crate) async fn start_migration_run(
    state: &AppState,
    operation: &str,
    profile: Option<&MigrationProfileRef>,
) -> MigrationRunRecord {
    let run = MigrationRunRecord {
        id: Uuid::new_v4().to_string(),
        operation: operation.into(),
        status: "running".into(),
        outcome: "pending".into(),
        phase: "execute".into(),
        profile_id: profile.map(|value| value.id.clone()),
        profile_revision: profile.map(|value| value.revision.clone()),
        source_connection_id: None,
        target_connection_id: None,
        started_at: Utc::now().to_rfc3339(),
        finished_at: None,
        selected_count: 0,
        committed_count: 0,
        failed_count: 0,
        conflict_count: 0,
        cancelled: false,
        rollback_outcome: "notRequired".into(),
        error_summary: None,
    };
    if let Err(error) = state.store.save_migration_run(&run).await {
        tracing::warn!(%error, operation, "failed to persist migration run start");
    }
    run
}

pub(crate) async fn finish_migration_run(
    state: &AppState,
    mut run: MigrationRunRecord,
    success: bool,
    cancelled: bool,
    committed: u64,
    failed: u64,
    conflicts: u64,
    rollback_outcome: &str,
) {
    run.status = if cancelled {
        "cancelled"
    } else if success {
        "completed"
    } else {
        "failed"
    }
    .into();
    run.outcome = if success {
        "success"
    } else if cancelled {
        "cancelled"
    } else {
        "failed"
    }
    .into();
    run.phase = "finished".into();
    run.finished_at = Some(Utc::now().to_rfc3339());
    run.committed_count = committed;
    run.failed_count = failed;
    run.conflict_count = conflicts;
    run.cancelled = cancelled;
    run.rollback_outcome = rollback_outcome.into();
    if !success && !cancelled {
        run.error_summary =
            Some("Execution failed; details are available in the application log".into());
    }
    if let Err(error) = state.store.save_migration_run(&run).await {
        tracing::warn!(%error, "failed to persist migration run completion");
    }
}

/// Persist Data Sync's evidence-based outcome without changing the shared
/// Transfer / Schema Diff interpretation of `outcome` or `rollback_outcome`.
pub(crate) async fn finish_data_sync_migration_run(
    state: &AppState,
    mut run: MigrationRunRecord,
    response: &DataSyncExecutionResponse,
    cancelled: bool,
) {
    let outcome = response.outcome;
    run.status = if cancelled {
        "cancelled"
    } else if outcome == ExecutionOutcome::Committed {
        "completed"
    } else {
        "failed"
    }
    .into();
    run.outcome = match outcome {
        ExecutionOutcome::NotStarted => "not_started",
        ExecutionOutcome::Committed => "committed",
        ExecutionOutcome::PartiallyApplied => "partially_applied",
        ExecutionOutcome::RolledBack => "rolled_back",
        ExecutionOutcome::Unknown => "unknown",
    }
    .into();
    run.phase = "finished".into();
    run.finished_at = Some(Utc::now().to_rfc3339());
    run.committed_count = if matches!(
        outcome,
        ExecutionOutcome::Committed | ExecutionOutcome::PartiallyApplied
    ) {
        response.result.applied as u64
    } else {
        0
    };
    run.failed_count = u64::from(matches!(
        outcome,
        ExecutionOutcome::NotStarted
            | ExecutionOutcome::RolledBack
            | ExecutionOutcome::PartiallyApplied
    ));
    run.conflict_count = response.result.conflicts.len() as u64;
    run.cancelled = cancelled;
    run.rollback_outcome = match outcome {
        ExecutionOutcome::NotStarted | ExecutionOutcome::Committed => "notRequired",
        ExecutionOutcome::RolledBack => "completed",
        ExecutionOutcome::PartiallyApplied => "partial",
        ExecutionOutcome::Unknown => "unknown",
    }
    .into();
    run.error_summary = match outcome {
        ExecutionOutcome::Committed => None,
        ExecutionOutcome::PartiallyApplied => Some("Some batches committed before execution stopped. Compare current data before continuing.".into()),
        ExecutionOutcome::NotStarted => Some(
            "Execution did not start. Review the endpoint context and run a fresh comparison."
                .into(),
        ),
        ExecutionOutcome::RolledBack => {
            Some("Execution did not commit; rollback was confirmed.".into())
        }
        ExecutionOutcome::Unknown => Some(
            "Commit or rollback could not be confirmed. Compare current data before continuing."
                .into(),
        ),
    };
    if let Err(error) = state.store.save_migration_run(&run).await {
        tracing::warn!(%error, "failed to persist Data Sync migration run completion");
    }
}

pub(crate) async fn purge_history_impl(
    state: &AppState,
    scope: String,
    retain_days: Option<u32>,
) -> Result<u64, CommandError> {
    let scope = HistoryScope::parse(&scope)
        .ok_or_else(|| CommandError::Validation(format!("Invalid history scope: {scope}")))?;
    tracing::info!(?scope, ?retain_days, "purge_history");
    state
        .store
        .purge_history(scope, retain_days)
        .await
        .cmd_err("purge_history")
}

#[tauri::command]
pub async fn purge_history(
    state: State<'_, AppState>,
    scope: String,
    retain_days: Option<u32>,
) -> Result<u64, CommandError> {
    purge_history_impl(&state, scope, retain_days).await
}

#[tauri::command]
pub async fn list_migration_runs(
    state: State<'_, AppState>,
    filter: Option<MigrationRunFilter>,
    offset: Option<u64>,
    limit: Option<u64>,
) -> Result<MigrationRunPage, CommandError> {
    state
        .store
        .list_migration_runs(
            &filter.unwrap_or_default(),
            offset.unwrap_or(0),
            limit.unwrap_or(25),
        )
        .await
        .cmd_err("list_migration_runs")
}

#[tauri::command]
pub async fn get_migration_run(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<MigrationRunRecord, CommandError> {
    state
        .store
        .get_migration_run(&run_id)
        .await
        .cmd_err("get_migration_run")?
        .ok_or_else(|| CommandError::Validation("Migration run was not found".into()))
}

pub(crate) async fn validate_migration_profile_ref(
    state: &AppState,
    operation: &str,
    reference: Option<&MigrationProfileRef>,
) -> Result<(), CommandError> {
    let Some(reference) = reference else {
        return Ok(());
    };
    let actual = match operation {
        "dataSync" => state
            .store
            .get_sync_profiles()
            .await
            .into_iter()
            .find(|profile| profile.id == reference.id)
            .map(|profile| profile.updated_at.to_rfc3339()),
        "dataTransfer" => state
            .store
            .get_transfer_profiles()
            .await
            .into_iter()
            .find(|profile| profile.id == reference.id)
            .map(|profile| profile.updated_at.to_rfc3339()),
        "schemaDiff" => state
            .store
            .get_schema_diff_profiles()
            .await
            .into_iter()
            .find(|profile| profile.id == reference.id)
            .map(|profile| profile.updated_at.to_rfc3339()),
        _ => None,
    };
    let revisions_match = |actual: &str, expected: &str| {
        chrono::DateTime::parse_from_rfc3339(actual).ok()
            == chrono::DateTime::parse_from_rfc3339(expected).ok()
    };
    match actual {
        Some(actual) if revisions_match(&actual, &reference.revision) => Ok(()),
        Some(_) => Err(CommandError::Validation(
            "Migration profile changed; review and prepare the plan again".into(),
        )),
        None => Err(CommandError::Validation(
            "Migration profile is missing; execution was blocked".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::QueryHistoryEntry;
    use crate::testing::app_state::TestAppState;
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_entry(sql: &str) -> QueryHistoryEntry {
        QueryHistoryEntry {
            id: Uuid::new_v4().to_string(),
            connection_id: "cfg1".into(),
            database: "app".into(),
            schema: None,
            sql: sql.into(),
            executed_at: Utc::now(),
            execution_time_ms: 1,
            rows_affected: None,
            success: true,
            error_message: None,
        }
    }

    #[tokio::test]
    async fn purge_history_clear_all_query_scope() {
        let test = TestAppState::new().await;
        test.store
            .add_query_history(sample_entry("SELECT 1"))
            .await
            .unwrap();

        let deleted = purge_history_impl(&test.state, "query".into(), None)
            .await
            .unwrap();
        assert_eq!(deleted, 1);
        assert!(test
            .store
            .get_query_history(10, None, None, None)
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn purge_history_rejects_invalid_scope() {
        let test = TestAppState::new().await;
        let err = purge_history_impl(&test.state, "invalid".into(), Some(7))
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Validation(_)));
    }

    #[tokio::test]
    async fn profile_reference_fails_closed_when_profile_is_missing() {
        let test = TestAppState::new().await;
        let reference = MigrationProfileRef {
            id: "removed-profile".into(),
            revision: Utc::now().to_rfc3339(),
        };
        let error = validate_migration_profile_ref(&test.state, "dataSync", Some(&reference))
            .await
            .unwrap_err();
        assert!(matches!(error, CommandError::Validation(_)));
    }

    #[tokio::test]
    async fn data_sync_unknown_history_keeps_only_stable_endpoint_and_profile_refs() {
        let test = TestAppState::new().await;
        let profile = MigrationProfileRef {
            id: "sync-profile-a".into(),
            revision: "2026-09-23T00:00:00Z".into(),
        };
        let mut run = start_migration_run(&test.state, "dataSync", Some(&profile)).await;
        let run_id = run.id.clone();
        run.source_connection_id = Some("source-connection".into());
        run.target_connection_id = Some("target-connection".into());
        run.selected_count = 3;
        let response = DataSyncExecutionResponse::unknown(
            "commit lost; do not persist this detailed error or any row literal",
        );

        finish_data_sync_migration_run(&test.state, run, &response, false).await;

        let stored = test
            .store
            .get_migration_run(&run_id)
            .await
            .unwrap()
            .expect("run is persisted");
        assert_eq!(stored.outcome, "unknown");
        assert_eq!(stored.rollback_outcome, "unknown");
        assert_eq!(stored.profile_id.as_deref(), Some("sync-profile-a"));
        let page = test
            .store
            .list_migration_runs(
                &MigrationRunFilter {
                    operation: Some("dataSync".into()),
                    ..Default::default()
                },
                0,
                10,
            )
            .await
            .unwrap();
        let stored = page
            .items
            .iter()
            .find(|item| item.id == run_id)
            .expect("run is listed");
        assert_eq!(stored.outcome, "unknown");
        assert_eq!(stored.rollback_outcome, "unknown");
        assert_eq!(stored.profile_id.as_deref(), Some("sync-profile-a"));
        assert_eq!(
            stored.profile_revision.as_deref(),
            Some("2026-09-23T00:00:00Z")
        );
        assert_eq!(
            stored.source_connection_id.as_deref(),
            Some("source-connection")
        );
        assert_eq!(
            stored.target_connection_id.as_deref(),
            Some("target-connection")
        );
        assert!(!stored
            .error_summary
            .as_deref()
            .unwrap_or_default()
            .contains("row literal"));
        let json = serde_json::to_value(stored).unwrap();
        for forbidden in ["sql", "rows", "parameters", "credentials", "sourceFilter"] {
            assert!(json.get(forbidden).is_none());
        }
    }

    #[tokio::test]
    async fn data_sync_history_persists_not_started_commit_rollback_and_unknown_separately() {
        use crate::data_sync::ExecutionResult;

        let test = TestAppState::new().await;
        let scenarios = [
            (
                "not-started",
                DataSyncExecutionResponse::failed_before_start("stale plan"),
                false,
                "not_started",
                "notRequired",
                0,
            ),
            (
                "committed",
                DataSyncExecutionResponse::from_result(ExecutionResult {
                    applied: 2,
                    rolled_back: false,
                    rollback_reason: None,
                    affected_rows: 2,
                    skipped: 0,
                    conflicts: Vec::new(),
                }),
                false,
                "committed",
                "notRequired",
                2,
            ),
            (
                "rolled-back",
                DataSyncExecutionResponse::from_result(ExecutionResult {
                    applied: 2,
                    rolled_back: true,
                    rollback_reason: Some("statement rejected".into()),
                    affected_rows: 2,
                    skipped: 0,
                    conflicts: Vec::new(),
                }),
                false,
                "rolled_back",
                "completed",
                0,
            ),
            (
                "unknown",
                DataSyncExecutionResponse::unknown("commit response lost"),
                false,
                "unknown",
                "unknown",
                0,
            ),
            (
                "cancelled-before-start",
                DataSyncExecutionResponse::failed_before_start(
                    "execute cancelled before any changes were applied",
                ),
                true,
                "not_started",
                "notRequired",
                0,
            ),
        ];

        for (id_suffix, response, cancelled, expected, rollback, committed_count) in scenarios {
            let mut run = start_migration_run(&test.state, "dataSync", None).await;
            let run_id = run.id.clone();
            run.source_connection_id = Some("source-stable-id".into());
            run.target_connection_id = Some("target-stable-id".into());
            finish_data_sync_migration_run(&test.state, run, &response, cancelled).await;
            let saved = test
                .store
                .get_migration_run(&run_id)
                .await
                .unwrap()
                .expect("history run was saved");
            assert_eq!(saved.outcome, expected, "{id_suffix}");
            assert_eq!(saved.rollback_outcome, rollback);
            assert_eq!(saved.committed_count, committed_count);
            assert_eq!(
                saved.source_connection_id.as_deref(),
                Some("source-stable-id")
            );
            assert_eq!(
                saved.target_connection_id.as_deref(),
                Some("target-stable-id")
            );
            if cancelled {
                assert_eq!(saved.status, "cancelled");
                assert!(saved.cancelled);
            } else if expected == "committed" {
                assert_eq!(saved.status, "completed");
            } else {
                assert_eq!(saved.status, "failed");
            }
            if expected == "rolled_back" {
                assert_eq!(saved.failed_count, 1);
            }
        }
    }
}
