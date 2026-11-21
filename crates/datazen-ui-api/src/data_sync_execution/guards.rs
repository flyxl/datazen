use crate::{ComparisonTableRow, ComparisonTableStatus, DataSyncOperation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecuteGuard {
    pub source_id: String,
    pub target_id: String,
    pub target_read_only: bool,
    pub write_outcome_uncertain: bool,
    pub has_selected_deletes: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardOutcome {
    Noop,
    UncertainError,
    ReadOnlyError,
    OpenConfirm,
    Proceed,
}

pub fn handle_execute(guard: &ExecuteGuard) -> GuardOutcome {
    if guard.target_read_only {
        return GuardOutcome::ReadOnlyError;
    }
    if guard.has_selected_deletes {
        return GuardOutcome::OpenConfirm;
    }
    GuardOutcome::Proceed
}

pub fn run_execute_guard(guard: &ExecuteGuard) -> GuardOutcome {
    if guard.source_id.is_empty() || guard.target_id.is_empty() {
        return GuardOutcome::Noop;
    }
    if guard.write_outcome_uncertain {
        return GuardOutcome::UncertainError;
    }
    if guard.target_read_only {
        return GuardOutcome::ReadOnlyError;
    }
    GuardOutcome::Proceed
}

pub fn can_execute(write_outcome_uncertain: bool, mappings: &[ComparisonTableRow]) -> bool {
    if write_outcome_uncertain {
        return false;
    }
    mappings.iter().any(|row| {
        row.status == ComparisonTableStatus::Matched
            && row
                .rows
                .as_ref()
                .map(|rows| {
                    rows.iter()
                        .any(|item| item.operation != DataSyncOperation::Unchanged)
                })
                .unwrap_or(false)
    })
}
