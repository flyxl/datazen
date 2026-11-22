use super::*;
use crate::{ComparisonTableRow, ComparisonTableStatus};

fn input(step: WizardStep) -> NavigationInput {
    NavigationInput {
        step,
        sync_state: DataSyncSyncState::Idle,
        inspection_complete: true,
        mapping_results: vec![ComparisonTableRow {
            source_table: "users".to_owned(),
            target_table: "users".to_owned(),
            status: ComparisonTableStatus::Matched,
            source_filter: None,
            rows: None,
        }],
        source_id: "src".to_owned(),
        target_id: "tgt".to_owned(),
        source_database: "app".to_owned(),
        target_database: "app2".to_owned(),
        active_pairing_supported: true,
        source_session_error: None,
        target_session_error: None,
        scope_reconfirmation_required: false,
        compared: true,
    }
}

#[test]
fn step_order_and_literals_are_pinned() {
    for (step, literal) in [
        (WizardStep::Endpoints, "endpoints"),
        (WizardStep::Setup, "setup"),
        (WizardStep::Objects, "objects"),
        (WizardStep::Compare, "compare"),
        (WizardStep::Preview, "preview"),
        (WizardStep::Result, "result"),
    ] {
        assert_eq!(step.as_str(), literal);
    }
    assert_eq!(WIZARD_STEPS.len(), 6);
    assert_eq!(step_index(WizardStep::Compare), Some(3));
    assert_eq!(next_step(WizardStep::Setup), Some(WizardStep::Objects));
    assert_eq!(previous_step(WizardStep::Setup), Some(WizardStep::Endpoints));
    assert_eq!(previous_step(WizardStep::Endpoints), None);
    assert_eq!(next_step(WizardStep::Result), None);
}

#[test]
fn busy_and_compare_disabled_follow_their_rules() {
    assert!(is_busy(DataSyncSyncState::Inspecting));
    assert!(is_busy(DataSyncSyncState::Comparing));
    assert!(is_busy(DataSyncSyncState::Executing));
    assert!(!is_busy(DataSyncSyncState::Idle));
    assert!(!is_busy(DataSyncSyncState::Compared));
    assert!(!is_busy(DataSyncSyncState::Done));

    let mut state = input(WizardStep::Endpoints);
    assert!(!compare_disabled(&state));
    state.source_session_error = Some("offline".to_owned());
    assert!(compare_disabled(&state));
    state.source_session_error = Some(String::new());
    assert!(!compare_disabled(&state));
    state.target_session_error = Some("auth".to_owned());
    assert!(compare_disabled(&state));
}

#[test]
fn endpoints_next_needs_both_ids_databases_and_a_supported_pair() {
    assert!(can_go_next(&input(WizardStep::Endpoints)));
    let mut state = input(WizardStep::Endpoints);
    state.target_id = String::new();
    assert!(!can_go_next(&state));
    let mut state = input(WizardStep::Endpoints);
    state.source_database = String::new();
    assert!(!can_go_next(&state));
    let mut state = input(WizardStep::Endpoints);
    state.active_pairing_supported = false;
    assert!(!can_go_next(&state));
    let mut state = input(WizardStep::Endpoints);
    state.scope_reconfirmation_required = true;
    assert!(!can_go_next(&state));
    let mut state = input(WizardStep::Endpoints);
    state.target_session_error = Some("bad".to_owned());
    assert!(!can_go_next(&state));
}

#[test]
fn setup_objects_and_compare_next_rules_are_pinned() {
    assert!(can_go_next(&input(WizardStep::Setup)));
    let mut busy_setup = input(WizardStep::Setup);
    busy_setup.sync_state = DataSyncSyncState::Executing;
    assert!(!can_go_next(&busy_setup));

    assert!(can_go_next(&input(WizardStep::Objects)));
    let mut no_mapping = input(WizardStep::Objects);
    no_mapping.mapping_results.clear();
    assert!(!can_go_next(&no_mapping));
    let mut unfinished = input(WizardStep::Objects);
    unfinished.inspection_complete = false;
    assert!(!can_go_next(&unfinished));

    assert!(can_go_next(&input(WizardStep::Compare)));
    let mut not_compared = input(WizardStep::Compare);
    not_compared.compared = false;
    assert!(!can_go_next(&not_compared));
    assert!(!can_go_next(&input(WizardStep::Preview)));
    assert!(!can_go_next(&input(WizardStep::Result)));
}

#[test]
fn setup_and_objects_advance_then_fall_back_on_a_failed_phase() {
    assert_eq!(
        plan_go_next(&input(WizardStep::Setup), Some(true), None),
        vec![
            NavigationEffect::SetStep(WizardStep::Objects),
            NavigationEffect::Inspect,
        ]
    );
    assert_eq!(
        plan_go_next(&input(WizardStep::Setup), Some(false), None),
        vec![
            NavigationEffect::SetStep(WizardStep::Objects),
            NavigationEffect::Inspect,
            NavigationEffect::SetStep(WizardStep::Setup),
        ]
    );
    assert_eq!(
        plan_go_next(&input(WizardStep::Objects), None, Some(true)),
        vec![
            NavigationEffect::SetStep(WizardStep::Compare),
            NavigationEffect::Compare,
        ]
    );
    assert_eq!(
        plan_go_next(&input(WizardStep::Objects), None, Some(false)),
        vec![
            NavigationEffect::SetStep(WizardStep::Compare),
            NavigationEffect::Compare,
            NavigationEffect::SetStep(WizardStep::Objects),
        ]
    );
}

#[test]
fn plain_steps_just_set_the_next_step() {
    assert_eq!(
        plan_go_next(&input(WizardStep::Endpoints), None, None),
        vec![NavigationEffect::SetStep(WizardStep::Setup)]
    );
    assert_eq!(
        plan_go_next(&input(WizardStep::Compare), None, None),
        vec![NavigationEffect::SetStep(WizardStep::Preview)]
    );
    assert!(plan_go_next(&input(WizardStep::Result), None, None).is_empty());
}

#[test]
fn go_back_is_blocked_while_busy_and_stops_at_the_first_step() {
    assert_eq!(
        plan_go_back(&input(WizardStep::Compare)),
        vec![NavigationEffect::SetStep(WizardStep::Objects)]
    );
    let mut busy = input(WizardStep::Compare);
    busy.sync_state = DataSyncSyncState::Comparing;
    assert!(plan_go_back(&busy).is_empty());
    assert!(plan_go_back(&input(WizardStep::Endpoints)).is_empty());
}

#[test]
fn crate_root_reexports_the_navigation_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::WizardStep>();
    resolves::<crate::NavigationInput>();
    resolves::<crate::NavigationEffect>();
    assert_eq!(crate::WIZARD_STEPS.len(), 6);
    assert!(crate::can_go_next(&input(crate::WizardStep::Setup)));
    assert_eq!(crate::DATA_SYNC_WIZARD_NAVIGATION_GAP.len(), 3);
}
