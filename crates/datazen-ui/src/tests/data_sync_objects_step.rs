//! T177: the Data Sync objects step — the three branches (inspecting,
//! mapping, empty), the pass-through callbacks, and `compared=false`.

use std::collections::HashSet;

use rune::{col, Cx, El, Env, Runtime};

use crate::views::data_sync::data_sync_compare_step::{
    DataSyncMappingStatus, DataSyncTableResult, SyncState,
};
use crate::views::data_sync::data_sync_objects_step::DataSyncObjectsStep;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn table(source: &str) -> DataSyncTableResult {
    DataSyncTableResult {
        source_table: source.to_owned(),
        target_table: format!("tgt_{source}"),
        status: DataSyncMappingStatus::Matched,
        ..DataSyncTableResult::default()
    }
}

#[derive(Default)]
struct StepState {
    sync_state: SyncState,
    rows: Vec<DataSyncTableResult>,
    disabled: HashSet<String>,
    toggled: Vec<String>,
    schema_diff_opens: usize,
    transfer_opens: usize,
}

fn view(state: &StepState, _cx: &mut Cx<StepState>) -> El {
    col().w_full().h_full().child(
        DataSyncObjectsStep::<StepState>::new(
            state.sync_state,
            state.rows.clone(),
            state.disabled.clone(),
        )
        .on_toggle_disabled(|s: &mut StepState, t| s.toggled.push(t))
        .on_open_schema_diff(|s: &mut StepState| s.schema_diff_opens += 1)
        .on_open_data_transfer(|s: &mut StepState| s.transfer_opens += 1)
        .on_update_source_filter(|_s: &mut StepState, _t, _f| {})
        .on_toggle_filter_editor(|_s: &mut StepState, _t| {})
        .build(true),
    )
}

fn mount(state: StepState) -> Runtime<StepState> {
    let mut rt = Runtime::new(state, view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    rt
}

fn has_key(rt: &Runtime<StepState>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of(rt: &Runtime<StepState>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn click(rt: &mut Runtime<StepState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_inspecting_branch_replaces_the_mapping() {
    let state = StepState {
        sync_state: SyncState::Inspecting,
        rows: vec![table("users")],
        ..StepState::default()
    };
    let rt = mount(state);
    assert!(has_key(&rt, "data-sync-objects-step"));
    assert!(has_key(&rt, "data-sync-objects-step-inspecting"));
    assert!(has_key(&rt, "data-sync-objects-step-spinner"));
    assert_eq!(
        text_of(&rt, "data-sync-objects-step-text"),
        crate::i18n::t("sync.inspecting").to_owned()
    );
    // The mapping branch does not mount while inspecting — not even with
    // rows present.
    assert!(!has_key(&rt, "data-sync-mapping-panel"));
}

#[test]
fn the_mapping_branch_mounts_with_rows() {
    let state = StepState {
        sync_state: SyncState::Idle,
        rows: vec![table("users"), table("logs")],
        ..StepState::default()
    };
    let rt = mount(state);
    assert!(has_key(&rt, "data-sync-mapping-panel"));
    assert!(has_key(&rt, "data-sync-mapping-summary"));
    // The pre-comparison title, not the after-compare one.
    assert_eq!(
        text_of(&rt, "data-sync-mapping-panel-title"),
        crate::i18n::t("sync.mappingTitle").to_owned()
    );
    assert!(!has_key(&rt, "data-sync-objects-step-empty"));
    assert!(!has_key(&rt, "data-sync-objects-step-inspecting"));
}

#[test]
fn the_empty_branch_mounts_without_rows() {
    let state = StepState {
        sync_state: SyncState::Idle,
        ..StepState::default()
    };
    let rt = mount(state);
    assert!(has_key(&rt, "data-sync-objects-step-empty"));
    assert_eq!(
        text_of(&rt, "data-sync-objects-step-empty-text"),
        crate::i18n::t("sync.noTablesFound").to_owned()
    );
    assert!(!has_key(&rt, "data-sync-mapping-panel"));
}

#[test]
fn a_compared_state_still_maps_this_step() {
    // Only 'inspecting' diverts to the spinner — every other state (even
    // post-comparison ones) shows the mapping.
    for state in [SyncState::Compared, SyncState::Executing, SyncState::Done] {
        let scene = StepState {
            sync_state: state,
            rows: vec![table("users")],
            ..StepState::default()
        };
        let rt = mount(scene);
        assert!(has_key(&rt, "data-sync-mapping-panel"), "{state:?} maps");
        assert!(!has_key(&rt, "data-sync-objects-step-inspecting"), "{state:?} does not spin");
    }
}

#[test]
fn pass_through_callbacks_reach_the_step() {
    let state = StepState {
        sync_state: SyncState::Idle,
        rows: vec![table("users"), table("legacy_incompatible")],
        ..StepState::default()
    };
    // One matched row and one incompatible row so both button kinds exist.
    let mut state = state;
    state.rows[1].status = DataSyncMappingStatus::Incompatible;
    state.rows[1].incompatible_reason = Some("r".to_owned());
    let mut rt = mount(state);

    // The include checkbox reaches the step's callback through the panel.
    click(&mut rt, "data-sync-mapping-row-users->tgt_users-include");
    assert_eq!(rt.state.toggled, vec!["users".to_owned()]);
    // The incompatible row's buttons reach the step's callbacks.
    click(&mut rt, "data-sync-mapping-row-legacy_incompatible->tgt_legacy_incompatible-schema-diff");
    assert_eq!(rt.state.schema_diff_opens, 1);
    click(
        &mut rt,
        "data-sync-mapping-row-legacy_incompatible->tgt_legacy_incompatible-data-sync-open-transfer",
    );
    assert_eq!(rt.state.transfer_opens, 1);
}
