#[cfg(test)]
mod tests;

use crate::{tables_for_compare, ComparisonTableRow, DataSyncSyncState};

pub const WIZARD_STEPS: [WizardStep; 6] = [
    WizardStep::Endpoints,
    WizardStep::Setup,
    WizardStep::Objects,
    WizardStep::Compare,
    WizardStep::Preview,
    WizardStep::Result,
];

pub const DATA_SYNC_WIZARD_NAVIGATION_GAP: [&str; 3] = [
    "React useCallback/useMemo state wiring",
    "async handleInspect / handleCompare IPC timing",
    "wizard setStep closure identity across renders",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WizardStep {
    Endpoints,
    Setup,
    Objects,
    Compare,
    Preview,
    Result,
}

impl WizardStep {
    pub fn as_str(self) -> &'static str {
        match self {
            WizardStep::Endpoints => "endpoints",
            WizardStep::Setup => "setup",
            WizardStep::Objects => "objects",
            WizardStep::Compare => "compare",
            WizardStep::Preview => "preview",
            WizardStep::Result => "result",
        }
    }
}

pub fn step_index(step: WizardStep) -> Option<usize> {
    WIZARD_STEPS.iter().position(|candidate| *candidate == step)
}

pub fn next_step(step: WizardStep) -> Option<WizardStep> {
    step_index(step).and_then(|index| WIZARD_STEPS.get(index + 1).copied())
}

pub fn previous_step(step: WizardStep) -> Option<WizardStep> {
    let index = step_index(step)?;
    if index == 0 {
        return None;
    }
    WIZARD_STEPS.get(index - 1).copied()
}

pub struct NavigationInput {
    pub step: WizardStep,
    pub sync_state: DataSyncSyncState,
    pub inspection_complete: bool,
    pub mapping_results: Vec<ComparisonTableRow>,
    pub source_id: String,
    pub target_id: String,
    pub source_database: String,
    pub target_database: String,
    pub active_pairing_supported: bool,
    pub source_session_error: Option<String>,
    pub target_session_error: Option<String>,
    pub scope_reconfirmation_required: bool,
    pub compared: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationEffect {
    SetStep(WizardStep),
    Inspect,
    Compare,
}

pub fn is_busy(state: DataSyncSyncState) -> bool {
    matches!(
        state,
        DataSyncSyncState::Inspecting
            | DataSyncSyncState::Comparing
            | DataSyncSyncState::Executing
    )
}

pub fn compare_disabled(input: &NavigationInput) -> bool {
    let has_error = |value: Option<&String>| value.is_some_and(|text| !text.is_empty());
    has_error(input.source_session_error.as_ref()) || has_error(input.target_session_error.as_ref())
}

pub fn can_go_next(input: &NavigationInput) -> bool {
    let busy = is_busy(input.sync_state);
    match input.step {
        WizardStep::Endpoints => {
            !input.source_id.is_empty()
                && !input.target_id.is_empty()
                && !input.source_database.is_empty()
                && !input.target_database.is_empty()
                && input.active_pairing_supported
                && !input.scope_reconfirmation_required
                && !compare_disabled(input)
        }
        WizardStep::Setup => !busy,
        WizardStep::Objects => {
            input.inspection_complete
                && !tables_for_compare(&input.mapping_results).is_empty()
                && !busy
        }
        WizardStep::Compare => input.compared && !busy,
        WizardStep::Preview | WizardStep::Result => false,
    }
}

pub fn plan_go_next(
    input: &NavigationInput,
    inspect_ok: Option<bool>,
    compare_ok: Option<bool>,
) -> Vec<NavigationEffect> {
    let next = match next_step(input.step) {
        Some(next) => next,
        None => return Vec::new(),
    };
    if input.step == WizardStep::Setup && next == WizardStep::Objects {
        let mut effects = vec![NavigationEffect::SetStep(next), NavigationEffect::Inspect];
        if inspect_ok == Some(false) {
            effects.push(NavigationEffect::SetStep(input.step));
        }
        return effects;
    }
    if input.step == WizardStep::Objects && next == WizardStep::Compare {
        let mut effects = vec![NavigationEffect::SetStep(next), NavigationEffect::Compare];
        if compare_ok == Some(false) {
            effects.push(NavigationEffect::SetStep(input.step));
        }
        return effects;
    }
    vec![NavigationEffect::SetStep(next)]
}

pub fn plan_go_back(input: &NavigationInput) -> Vec<NavigationEffect> {
    if is_busy(input.sync_state) {
        return Vec::new();
    }
    match previous_step(input.step) {
        Some(previous) => vec![NavigationEffect::SetStep(previous)],
        None => Vec::new(),
    }
}
