use serde_json::Value;

use crate::DataSyncOperation;

use super::{
    ComparisonTableRow, ComparisonTableStatus, DataSyncSyncState, SYNC_CANCELLING_EXECUTION_KEY,
    SYNC_COMPARE_CANCELLED_KEY,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataSyncJobKind {
    Compare,
    Execute,
}

impl DataSyncJobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Compare => "compare",
            Self::Execute => "execute",
        }
    }
}

pub fn next_generation(current: &mut u64) -> u64 {
    *current = current.wrapping_add(1);
    *current
}

pub fn is_current_generation(generation: u64, current: u64) -> bool {
    generation == current
}

pub fn is_compared_sync_state(state: DataSyncSyncState) -> bool {
    matches!(
        state,
        DataSyncSyncState::Compared
            | DataSyncSyncState::Executing
            | DataSyncSyncState::Unknown
            | DataSyncSyncState::Done
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClearJobRefsInput {
    pub job_id: Option<String>,
    pub job_kind: Option<DataSyncJobKind>,
    pub ref_job_id: Option<String>,
    pub ref_job_kind: Option<DataSyncJobKind>,
}

pub fn clear_job_refs(input: &ClearJobRefsInput) -> bool {
    input.job_id.is_some()
        && input.job_kind.is_some()
        && input.ref_job_id == input.job_id
        && input.ref_job_kind == input.job_kind
}

pub fn is_paged_comparison(contract_version: Option<&str>) -> bool {
    contract_version.is_some()
}

pub type SelectedComparisonRow = (String, String, DataSyncOperation, Vec<Value>);

pub fn selected_rows_from_merged(merged: &[ComparisonTableRow]) -> Vec<SelectedComparisonRow> {
    let mut rows = Vec::new();
    for table in merged {
        if let Some(items) = &table.rows {
            for item in items {
                if item.selected && item.operation != DataSyncOperation::Unchanged {
                    rows.push((
                        table.source_table.clone(),
                        table.target_table.clone(),
                        item.operation,
                        item.key.clone(),
                    ));
                }
            }
        }
    }
    rows
}

pub fn first_diff_key(merged: &[ComparisonTableRow]) -> Option<String> {
    merged
        .iter()
        .find(|table| {
            table.status == ComparisonTableStatus::Matched
                && table.rows.as_ref().is_some_and(|rows| {
                    rows.iter()
                        .any(|row| row.operation != DataSyncOperation::Unchanged)
                })
        })
        .map(|table| table.source_table.clone())
}

#[derive(Clone, Debug, PartialEq)]
pub struct CancelInput {
    pub job_id: Option<String>,
    pub job_kind: Option<DataSyncJobKind>,
    pub write_in_flight: bool,
    pub write_outcome_uncertain: bool,
    pub has_mappings: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CancelActions {
    pub reassert_uncertain: bool,
    pub cancelling_status_job: bool,
    pub status_key: &'static str,
    pub sync_state: DataSyncSyncState,
    pub clear_execute_progress: bool,
    pub should_cancel_ipc: bool,
}

pub fn cancel_actions(input: &CancelInput) -> CancelActions {
    let executing_write = input.job_kind == Some(DataSyncJobKind::Execute) && input.write_in_flight;
    let sync_state = if input.write_outcome_uncertain {
        DataSyncSyncState::Unknown
    } else if input.has_mappings {
        DataSyncSyncState::Compared
    } else {
        DataSyncSyncState::Idle
    };
    CancelActions {
        reassert_uncertain: input.write_outcome_uncertain,
        cancelling_status_job: executing_write,
        status_key: if executing_write {
            SYNC_CANCELLING_EXECUTION_KEY
        } else {
            SYNC_COMPARE_CANCELLED_KEY
        },
        sync_state: if executing_write {
            DataSyncSyncState::Executing
        } else {
            sync_state
        },
        clear_execute_progress: !executing_write,
        should_cancel_ipc: input.job_id.is_some(),
    }
}

pub fn should_keep_cancelled_result(
    generation: u64,
    current_generation: u64,
    ref_job_id: &Option<String>,
    job_id: &Option<String>,
    ref_job_kind: &Option<DataSyncJobKind>,
    job_kind: &Option<DataSyncJobKind>,
) -> bool {
    !is_current_generation(generation, current_generation)
        || ref_job_id != job_id
        || ref_job_kind != job_kind
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn table(name: &str, rows: Vec<(DataSyncOperation, bool)>) -> ComparisonTableRow {
        ComparisonTableRow {
            source_table: name.to_owned(),
            target_table: name.to_owned(),
            status: ComparisonTableStatus::Matched,
            source_filter: None,
            rows: Some(
                rows.into_iter()
                    .map(
                        |(operation, selected)| crate::data_sync_comparison::ComparisonRow {
                            operation,
                            key: vec![json!(1)],
                            selected,
                        },
                    )
                    .collect(),
            ),
        }
    }

    #[test]
    fn generation_fence_only_accepts_the_newest_run() {
        let mut generation = 3;
        assert_eq!(next_generation(&mut generation), 4);
        assert!(is_current_generation(4, generation));
        assert!(!is_current_generation(3, generation));
    }

    #[test]
    fn compared_states_include_executing_unknown_and_done() {
        assert!(is_compared_sync_state(DataSyncSyncState::Compared));
        assert!(is_compared_sync_state(DataSyncSyncState::Executing));
        assert!(is_compared_sync_state(DataSyncSyncState::Unknown));
        assert!(is_compared_sync_state(DataSyncSyncState::Done));
        assert!(!is_compared_sync_state(DataSyncSyncState::Idle));
        assert!(!is_compared_sync_state(DataSyncSyncState::Inspecting));
    }

    #[test]
    fn job_refs_are_cleared_only_for_the_matching_job() {
        let input = ClearJobRefsInput {
            job_id: Some("j1".to_owned()),
            job_kind: Some(DataSyncJobKind::Compare),
            ref_job_id: Some("j1".to_owned()),
            ref_job_kind: Some(DataSyncJobKind::Compare),
        };
        assert!(clear_job_refs(&input));
        assert!(!clear_job_refs(&ClearJobRefsInput {
            ref_job_id: Some("j2".to_owned()),
            ..input
        }));
        assert!(is_paged_comparison(Some("v1")));
        assert!(!is_paged_comparison(None));
    }

    #[test]
    fn selected_rows_come_only_from_selected_non_unchanged_rows() {
        let merged = [
            table(
                "a",
                vec![
                    (DataSyncOperation::Insert, true),
                    (DataSyncOperation::Unchanged, true),
                ],
            ),
            table("b", vec![(DataSyncOperation::Delete, false)]),
        ];
        let selected = selected_rows_from_merged(&merged);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "a");
        assert_eq!(selected[0].2, DataSyncOperation::Insert);
    }

    #[test]
    fn first_diff_picks_the_first_matched_table_with_a_change() {
        let merged = [
            table("clean", vec![(DataSyncOperation::Unchanged, false)]),
            table("dirty", vec![(DataSyncOperation::Update, false)]),
        ];
        assert_eq!(first_diff_key(&merged).as_deref(), Some("dirty"));
        assert_eq!(first_diff_key(&[]), None);
    }

    #[test]
    fn cancel_branches_for_execution_and_comparison() {
        let execute = cancel_actions(&CancelInput {
            job_id: Some("j1".to_owned()),
            job_kind: Some(DataSyncJobKind::Execute),
            write_in_flight: true,
            write_outcome_uncertain: false,
            has_mappings: true,
        });
        assert!(execute.cancelling_status_job);
        assert_eq!(execute.status_key, "sync.cancellingExecution");
        assert!(!execute.clear_execute_progress);
        assert!(execute.should_cancel_ipc);

        let compare = cancel_actions(&CancelInput {
            job_id: None,
            job_kind: Some(DataSyncJobKind::Compare),
            write_in_flight: false,
            write_outcome_uncertain: true,
            has_mappings: true,
        });
        assert!(!compare.cancelling_status_job);
        assert_eq!(compare.status_key, "sync.compareCancelled");
        assert_eq!(compare.sync_state, DataSyncSyncState::Unknown);
        assert!(compare.clear_execute_progress);
        assert!(!compare.should_cancel_ipc);
    }

    #[test]
    fn stale_cancel_completions_are_ignored() {
        assert!(should_keep_cancelled_result(
            1,
            2,
            &Some("j".to_owned()),
            &Some("j".to_owned()),
            &Some(DataSyncJobKind::Compare),
            &Some(DataSyncJobKind::Compare)
        ));
        assert!(!should_keep_cancelled_result(
            2,
            2,
            &Some("j".to_owned()),
            &Some("j".to_owned()),
            &Some(DataSyncJobKind::Compare),
            &Some(DataSyncJobKind::Compare)
        ));
    }
}
