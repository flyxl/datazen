use serde_json::Value;

pub const DATA_SYNC_RUN_STATE_GAP: [&str; 3] = [
    "React useState / Dispatch runtime",
    "the hook subscription that re-renders a consumer on a set",
    "SetStateAction updater closures evaluated by React",
];

pub const DATA_SYNC_RUN_STATE_SURFACE: [&str; 6] = [
    "DataSyncRunState",
    "writeOutcomeUncertain",
    "scopeReconfirmationRequired",
    "lastExecutionResult",
    "useDataSyncRunState",
    "DATA_SYNC_RUN_STATE_GAP",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataSyncExecutionOutcome {
    NotStarted,
    Committed,
    PartiallyApplied,
    RolledBack,
    Unknown,
}

impl DataSyncExecutionOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Committed => "committed",
            Self::PartiallyApplied => "partially_applied",
            Self::RolledBack => "rolled_back",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataSyncConflict {
    pub table: String,
    pub operation: String,
    pub row_key: Vec<Value>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataSyncExecutionResult {
    pub applied: u64,
    pub rolled_back: bool,
    pub rollback_reason: Option<String>,
    pub affected_rows: Option<u64>,
    pub skipped: Option<u64>,
    pub conflicts: Vec<DataSyncConflict>,
    pub outcome: DataSyncExecutionOutcome,
    pub error: Option<String>,
}

pub enum SetAction<T> {
    Value(T),
    Update(Box<dyn FnOnce(T) -> T>),
}

impl<T> SetAction<T> {
    pub fn apply(self, current: T) -> T {
        match self {
            Self::Value(value) => value,
            Self::Update(update) => update(current),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataSyncRunState {
    pub write_outcome_uncertain: bool,
    pub scope_reconfirmation_required: bool,
    pub last_execution_result: Option<DataSyncExecutionResult>,
}

impl DataSyncRunState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_write_outcome_uncertain(&mut self, action: SetAction<bool>) {
        self.write_outcome_uncertain = action.apply(self.write_outcome_uncertain);
    }

    pub fn set_scope_reconfirmation_required(&mut self, action: SetAction<bool>) {
        self.scope_reconfirmation_required = action.apply(self.scope_reconfirmation_required);
    }

    pub fn set_last_execution_result(
        &mut self,
        action: SetAction<Option<DataSyncExecutionResult>>,
    ) {
        self.last_execution_result = action.apply(self.last_execution_result.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(outcome: DataSyncExecutionOutcome) -> DataSyncExecutionResult {
        DataSyncExecutionResult {
            applied: 3,
            rolled_back: false,
            rollback_reason: None,
            affected_rows: Some(5),
            skipped: None,
            conflicts: Vec::new(),
            outcome,
            error: None,
        }
    }

    #[test]
    fn a_fresh_run_starts_with_both_safety_fences_cleared() {
        let state = DataSyncRunState::new();
        assert!(!state.write_outcome_uncertain);
        assert!(!state.scope_reconfirmation_required);
        assert_eq!(state.last_execution_result, None);
    }

    #[test]
    fn value_setters_write_directly() {
        let mut state = DataSyncRunState::new();
        state.set_write_outcome_uncertain(SetAction::Value(true));
        state.set_scope_reconfirmation_required(SetAction::Value(true));
        assert!(state.write_outcome_uncertain);
        assert!(state.scope_reconfirmation_required);
        state.set_write_outcome_uncertain(SetAction::Value(false));
        assert!(!state.write_outcome_uncertain, "the fence is only cleared by an explicit set");
    }

    #[test]
    fn updater_setters_see_the_previous_value() {
        let mut state = DataSyncRunState::new();
        state.set_write_outcome_uncertain(SetAction::Update(Box::new(|current| !current)));
        assert!(state.write_outcome_uncertain);
        state.set_write_outcome_uncertain(SetAction::Update(Box::new(|current| !current)));
        assert!(!state.write_outcome_uncertain);
        state.set_scope_reconfirmation_required(SetAction::Update(Box::new(|current| current || true)));
        assert!(state.scope_reconfirmation_required);
    }

    #[test]
    fn the_execution_result_can_be_set_and_updated_without_losing_fields() {
        let mut state = DataSyncRunState::new();
        state.set_last_execution_result(SetAction::Value(Some(result(DataSyncExecutionOutcome::Committed))));
        let committed = state.last_execution_result.clone().expect("a committed result");
        assert_eq!(committed.applied, 3);
        assert_eq!(committed.outcome, DataSyncExecutionOutcome::Committed);

        state.set_last_execution_result(SetAction::Update(Box::new(|previous| {
            let mut next = previous.expect("a previous result");
            next.outcome = DataSyncExecutionOutcome::Unknown;
            next.error = Some("unknown outcome".to_owned());
            Some(next)
        })));
        let unknown = state.last_execution_result.clone().expect("an updated result");
        assert_eq!(unknown.outcome, DataSyncExecutionOutcome::Unknown);
        assert_eq!(unknown.error.as_deref(), Some("unknown outcome"));
        assert_eq!(unknown.applied, 3, "unmentioned fields survive the update");

        state.set_last_execution_result(SetAction::Value(None));
        assert_eq!(state.last_execution_result, None);
    }

    #[test]
    fn outcome_wire_spellings_and_surface_are_pinned() {
        assert_eq!(DataSyncExecutionOutcome::NotStarted.as_str(), "not_started");
        assert_eq!(DataSyncExecutionOutcome::PartiallyApplied.as_str(), "partially_applied");
        assert_eq!(DataSyncExecutionOutcome::RolledBack.as_str(), "rolled_back");
        assert_eq!(DataSyncExecutionOutcome::Unknown.as_str(), "unknown");
        assert_eq!(DATA_SYNC_RUN_STATE_GAP.len(), 3);
        assert_eq!(DATA_SYNC_RUN_STATE_SURFACE.len(), 6);
        assert_eq!(crate::DATA_SYNC_RUN_STATE_GAP.len(), 3);
        assert_eq!(crate::DATA_SYNC_RUN_STATE_SURFACE.len(), 6);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::DataSyncRunState::new;
        let _: Option<crate::SetAction<bool>> = None;
        let _: Option<crate::DataSyncExecutionResult> = None;
        let _: Option<crate::DataSyncConflict> = None;
        let _: Option<crate::DataSyncExecutionOutcome> = None;
    }
}
