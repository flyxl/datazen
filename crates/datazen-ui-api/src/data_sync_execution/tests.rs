use super::*;
use crate::{
    ComparisonRow, ComparisonTableRow, ComparisonTableStatus, DataSyncJobKind, DataSyncOperation,
    DataSyncSyncState, SyncOptions,
};
use serde_json::json;

fn t(key: &str) -> String {
    format!("T:{key}")
}

fn mapping_with_diff() -> Vec<ComparisonTableRow> {
    vec![ComparisonTableRow {
        source_table: "users".to_owned(),
        target_table: "users".to_owned(),
        status: ComparisonTableStatus::Matched,
        source_filter: Some("age > 18".to_owned()),
        rows: Some(vec![ComparisonRow {
            operation: DataSyncOperation::Update,
            key: vec![json!(1)],
            selected: true,
        }]),
    }]
}

fn input() -> ExecutionInput {
    ExecutionInput {
        source_id: "src".to_owned(),
        target_id: "tgt".to_owned(),
        target_read_only: false,
        write_outcome_uncertain: false,
        has_selected_deletes: false,
        mapping_results: mapping_with_diff(),
        selected_rows: vec![TableSelection {
            source_table: "users".to_owned(),
            target_table: "users".to_owned(),
        }],
        table_selections: Vec::new(),
        sync_options: SyncOptions {
            insert: true,
            update: true,
            delete: false,
        },
        source_database: "app".to_owned(),
        target_database: "app2".to_owned(),
        source_schema: None,
        target_schema: None,
        execute_job_id: "job-1".to_owned(),
        recompare_job_id: "job-2".to_owned(),
    }
}

fn responses() -> ExecutionResponses {
    ExecutionResponses {
        sessions: SessionRefresh::Ready {
            source_session_id: Some("s1".to_owned()),
            target_session_id: Some("s2".to_owned()),
        },
        execute: Some(ExecuteResult {
            outcome: None,
            rolled_back: false,
            error: String::new(),
        }),
        compare: Some(CompareResult::Rows(mapping_with_diff())),
        stale: false,
        cancel_requested_job: false,
    }
}

fn plan(input: &ExecutionInput, responses: &ExecutionResponses) -> Vec<ExecutionEffect> {
    super::plan_execution(input, responses, &t).effects
}

#[test]
fn surface_and_gap_are_pinned() {
    assert_eq!(DATA_SYNC_EXECUTION_SURFACE.len(), 9);
    assert_eq!(DATA_SYNC_EXECUTION_GAP.len(), 7);
}

#[test]
fn execute_guards_follow_the_documented_order() {
    let mut guard = ExecuteGuard {
        source_id: "src".to_owned(),
        target_id: "tgt".to_owned(),
        target_read_only: true,
        write_outcome_uncertain: true,
        has_selected_deletes: true,
    };
    assert_eq!(handle_execute(&guard), GuardOutcome::ReadOnlyError);
    guard.target_read_only = false;
    assert_eq!(handle_execute(&guard), GuardOutcome::OpenConfirm);
    guard.has_selected_deletes = false;
    assert_eq!(handle_execute(&guard), GuardOutcome::Proceed);

    assert_eq!(run_execute_guard(&guard), GuardOutcome::UncertainError);
    guard.write_outcome_uncertain = false;
    guard.target_read_only = true;
    assert_eq!(run_execute_guard(&guard), GuardOutcome::ReadOnlyError);
    guard.target_read_only = false;
    guard.target_id = String::new();
    assert_eq!(run_execute_guard(&guard), GuardOutcome::Noop);
}

#[test]
fn can_execute_needs_a_matched_table_with_diffs() {
    assert!(can_execute(false, &mapping_with_diff()));
    assert!(!can_execute(true, &mapping_with_diff()));
    let mut clean = mapping_with_diff();
    clean[0].rows = None;
    assert!(!can_execute(false, &clean));
    let mut disabled = mapping_with_diff();
    disabled[0].status = ComparisonTableStatus::Disabled;
    assert!(!can_execute(false, &disabled));
    assert!(!can_execute(false, &[]));
}

#[test]
fn early_exits_are_observable_and_do_not_call_the_bridge() {
    let mut early = input();
    early.source_id = String::new();
    assert!(plan(&early, &responses()).is_empty());

    early = input();
    early.write_outcome_uncertain = true;
    assert_eq!(
        plan(&early, &responses()),
        vec![
            ExecutionEffect::Error {
                message: "T:sync.executionUnknown".to_owned(),
                open: true,
            },
            ExecutionEffect::State(DataSyncSyncState::Unknown),
        ]
    );

    early = input();
    early.target_read_only = true;
    assert_eq!(
        plan(&early, &responses()),
        vec![ExecutionEffect::Error {
            message: "T:sync.targetReadOnly".to_owned(),
            open: true,
        }]
    );
}

#[test]
fn missing_sessions_and_failed_refresh_stop_before_the_write() {
    let mut missing = responses();
    missing.sessions = SessionRefresh::Ready {
        source_session_id: None,
        target_session_id: Some("s2".to_owned()),
    };
    assert_eq!(
        plan(&input(), &missing),
        vec![
            ExecutionEffect::State(DataSyncSyncState::Executing),
            ExecutionEffect::Progress("T:sync.executing".to_owned()),
            ExecutionEffect::State(DataSyncSyncState::Compared),
        ]
    );

    let mut failed = responses();
    failed.sessions = SessionRefresh::Failed("offline".to_owned());
    assert_eq!(
        plan(&input(), &failed),
        vec![
            ExecutionEffect::State(DataSyncSyncState::Executing),
            ExecutionEffect::Progress("T:sync.executing".to_owned()),
            ExecutionEffect::Error {
                message: "offline".to_owned(),
                open: true,
            },
            ExecutionEffect::State(DataSyncSyncState::Compared),
            ExecutionEffect::ClearProgress,
        ]
    );
}

#[test]
fn an_empty_selection_never_issues_the_write() {
    let mut empty = input();
    empty.selected_rows.clear();
    assert_eq!(
        plan(&empty, &responses()),
        vec![
            ExecutionEffect::State(DataSyncSyncState::Executing),
            ExecutionEffect::Progress("T:sync.executing".to_owned()),
            ExecutionEffect::State(DataSyncSyncState::Compared),
            ExecutionEffect::ClearProgress,
        ]
    );
}

#[test]
fn stale_and_cancelled_generations_stop_after_the_initial_progress() {
    let mut stale = responses();
    stale.stale = true;
    assert_eq!(
        plan(&input(), &stale),
        vec![
            ExecutionEffect::State(DataSyncSyncState::Executing),
            ExecutionEffect::Progress("T:sync.executing".to_owned()),
        ]
    );
    let mut cancelled = responses();
    cancelled.cancel_requested_job = true;
    assert_eq!(plan(&input(), &cancelled).len(), 2);
}

#[test]
fn not_started_and_unknown_outcomes_have_distinct_fences() {
    let mut not_started = responses();
    not_started.execute = Some(ExecuteResult {
        outcome: Some("not_started".to_owned()),
        rolled_back: false,
        error: String::new(),
    });
    let effects = plan(&input(), &not_started);
    assert!(effects.iter().any(|effect| matches!(effect, ExecutionEffect::Error { message, open } if message == "T:sync.executionNotStarted" && *open)));
    assert!(effects.iter().any(|effect| *effect == ExecutionEffect::State(DataSyncSyncState::Compared)));
    assert!(effects.iter().any(|effect| *effect == ExecutionEffect::Uncertain(false)));

    let mut unknown = responses();
    unknown.execute = Some(ExecuteResult {
        outcome: Some("unknown".to_owned()),
        rolled_back: false,
        error: "timeout".to_owned(),
    });
    let effects = plan(&input(), &unknown);
    assert!(effects.iter().any(|effect| matches!(effect, ExecutionEffect::Error { message, .. } if message == "T:sync.executionUnknown timeout")));
    assert!(effects.iter().any(|effect| *effect == ExecutionEffect::Uncertain(true)));
    assert!(effects.iter().any(|effect| *effect == ExecutionEffect::State(DataSyncSyncState::Unknown)));
    assert!(effects.contains(&ExecutionEffect::StepResult));
}

#[test]
fn a_committed_write_recompares_merges_and_finishes() {
    let effects = plan(&input(), &responses());
    let call = effects
        .iter()
        .find_map(|effect| match effect {
            ExecutionEffect::ExecuteCall {
                source_session,
                target_session,
                job_id,
                ..
            } => Some((source_session.as_str(), target_session.as_str(), job_id.as_str())),
            _ => None,
        });
    assert_eq!(call, Some(("s1", "s2", "job-1")));
    let compare = effects
        .iter()
        .find_map(|effect| match effect {
            ExecutionEffect::CompareCall { job_id, tables, filters } => {
                Some((job_id.as_str(), tables.clone(), filters.clone()))
            }
            _ => None,
        });
    assert_eq!(
        compare,
        Some((
            "job-2",
            vec!["users".to_owned()],
            vec![("users".to_owned(), "age > 18".to_owned())]
        ))
    );
    assert_eq!(
        effects
            .iter()
            .filter(|effect| **effect == ExecutionEffect::RefreshJobs)
            .count(),
        2
    );
    assert!(effects.iter().any(|effect| matches!(effect, ExecutionEffect::LastResult(Some(value)) if value == "committed")));
    assert!(effects.iter().any(|effect| matches!(effect, ExecutionEffect::MergeMappings(rows) if rows[0].rows.as_ref().map(|r| r[0].selected).unwrap_or(false))));
    assert!(effects.iter().any(|effect| matches!(effect, ExecutionEffect::SelectedRows(rows) if rows.len() == 1)));
    assert_eq!(effects.last(), Some(&ExecutionEffect::Status(String::new())));
}

#[test]
fn a_failed_recompare_fences_the_write_as_done() {
    let mut failed = responses();
    failed.compare = Some(CompareResult::Failed("compare offline".to_owned()));
    let effects = plan(&input(), &failed);
    assert!(effects.iter().any(|effect| matches!(effect, ExecutionEffect::Error { message, .. } if message == "compare offline")));
    assert!(effects.iter().any(|effect| *effect == ExecutionEffect::Uncertain(false)));
    assert!(effects.iter().any(|effect| *effect == ExecutionEffect::State(DataSyncSyncState::Done)));
    assert!(effects.contains(&ExecutionEffect::StepResult));
    assert!(!effects.iter().any(|effect| matches!(effect, ExecutionEffect::MergeMappings(_))));
}

#[test]
fn a_paged_recompare_clears_the_selection_and_drops_rows() {
    let mut paged = responses();
    paged.compare = Some(CompareResult::Paged {
        tables: mapping_with_diff(),
        contract_version: "v2".to_owned(),
    });
    let effects = plan(&input(), &paged);
    assert!(effects.contains(&ExecutionEffect::ClearSelection));
    assert!(effects.iter().any(|effect| matches!(effect, ExecutionEffect::MergeMappings(rows) if rows[0].rows.is_none())));
    assert!(!effects.iter().any(|effect| matches!(effect, ExecutionEffect::SelectedRows(_))));
}

#[test]
fn finalize_clears_only_matching_refs() {
    let finalize = FinalizeInput {
        execute_job_id: "job-1".to_owned(),
        recompare_job_id: Some("job-2".to_owned()),
        ref_job_id: Some("job-1".to_owned()),
        ref_job_kind: Some(DataSyncJobKind::Execute),
        cancel_requested: Some("job-1".to_owned()),
        cancelling_status: Some("job-1".to_owned()),
        current_status: "T:sync.cancellingExecution".to_owned(),
        cancelling_message: "T:sync.cancellingExecution".to_owned(),
    };
    let plan = plan_finalize(&finalize);
    assert_eq!(
        plan,
        FinalizePlan {
            clear_write_in_flight: true,
            clear_status: true,
            clear_execute_refs: true,
            clear_compare_refs: false,
            clear_cancel_request: true,
        }
    );

    let no_match = FinalizeInput {
        ref_job_kind: Some(DataSyncJobKind::Compare),
        cancel_requested: Some("other".to_owned()),
        current_status: "something".to_owned(),
        ..finalize
    };
    let plan = plan_finalize(&no_match);
    assert!(!plan.clear_execute_refs);
    assert!(!plan.clear_status);
    assert!(!plan.clear_cancel_request);
}

#[test]
fn crate_root_reexports_the_execution_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::ExecutionInput>();
    resolves::<crate::ExecutionPlan>();
    resolves::<crate::ExecutionEffect>();
    resolves::<crate::ExecuteGuard>();
    assert_eq!(crate::DATA_SYNC_EXECUTION_SURFACE.len(), 9);
    assert!(crate::can_execute(false, &mapping_with_diff()));
}
