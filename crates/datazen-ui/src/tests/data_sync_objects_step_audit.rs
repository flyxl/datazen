//! T177 audit (tester): independent checks of the objects step from
//! separately-built scenes: geometry at both window sizes and both themes,
//! the inspecting branch's precedence over the empty branch (rows empty
//! while inspecting), and the disabled set passing through to the panel's
//! row dimming.

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

fn audit_table(source: &str, status: DataSyncMappingStatus) -> DataSyncTableResult {
    DataSyncTableResult {
        source_table: source.to_owned(),
        target_table: format!("tgt_{source}"),
        status,
        ..DataSyncTableResult::default()
    }
}

#[derive(Default)]
struct AuditState {
    sync_state: SyncState,
    rows: Vec<DataSyncTableResult>,
    disabled: HashSet<String>,
}

fn audit_view(state: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    col().w_full().h_full().child(
        DataSyncObjectsStep::<AuditState>::new(
            state.sync_state,
            state.rows.clone(),
            state.disabled.clone(),
        )
        .on_toggle_disabled(|_s: &mut AuditState, _t| {})
        .on_open_schema_diff(|_s: &mut AuditState| {})
        .on_open_data_transfer(|_s: &mut AuditState| {})
        .on_update_source_filter(|_s: &mut AuditState, _t, _f| {})
        .on_toggle_filter_editor(|_s: &mut AuditState, _t| {})
        .build(true),
    )
}

fn audit_mount(state: AuditState, size: (f32, f32)) -> Runtime<AuditState> {
    let mut rt = Runtime::new(state, audit_view, size, env(true));
    rt.frame(0.0);
    rt
}

fn audit_has_key(rt: &Runtime<AuditState>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn audit_geometry_both_sizes_both_branches() {
    for rows in [
        vec![audit_table("users", DataSyncMappingStatus::Matched)],
        Vec::<DataSyncTableResult>::new(),
    ] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let state = AuditState {
                sync_state: SyncState::Idle,
                rows: rows.clone(),
                disabled: HashSet::new(),
            };
            let rt = audit_mount(state, (w, h));
            let mut outside = Vec::new();
            for e in rt.elements() {
                let Some(key) = e.key.as_deref() else { continue };
                if !key.starts_with("data-sync-objects-step") && !key.starts_with("data-sync-mapping") {
                    continue;
                }
                let [x, _y, ew, _eh] = e.rect;
                if x < -0.5 || x + ew > w + 0.5 {
                    outside.push(format!("{key} {:?}", e.rect));
                }
            }
            assert!(outside.is_empty(), "{w}x{h} rows={}: {}", rows.len(), outside.join("; "));
        }
    }
}

#[test]
fn audit_inspecting_precedes_the_empty_branch() {
    // Inspecting with NO rows: the spinner wins — the empty branch stays
    // hidden (React's conditional order).
    let state = AuditState {
        sync_state: SyncState::Inspecting,
        ..AuditState::default()
    };
    let rt = audit_mount(state, (960.0, 640.0));
    assert!(audit_has_key(&rt, "data-sync-objects-step-inspecting"));
    assert!(!audit_has_key(&rt, "data-sync-objects-step-empty"));
}

#[test]
fn audit_the_disabled_set_passes_through_to_the_row_dim() {
    let mut disabled = HashSet::new();
    disabled.insert("logs".to_owned());
    let state = AuditState {
        sync_state: SyncState::Idle,
        rows: vec![audit_table("logs", DataSyncMappingStatus::Matched)],
        disabled,
    };
    let rt = audit_mount(state, (960.0, 640.0));
    let row = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-mapping-row-logs->tgt_logs"))
        .expect("the row exists");
    assert_eq!(row.opacity, 0.6, "the disabled set reached the panel's dim");
}

#[test]
fn audit_the_inspecting_branch_renders_in_light_theme_too() {
    // The step's dark flag is host-chosen; light must not panic and must
    // paint the same branch.
    let state = AuditState {
        sync_state: SyncState::Inspecting,
        rows: vec![audit_table("users", DataSyncMappingStatus::Matched)],
        ..AuditState::default()
    };
    // Build through the dark(false) path.
    let view = |state: &AuditState, _cx: &mut Cx<AuditState>| -> El {
        col().w_full().h_full().child(
            DataSyncObjectsStep::<AuditState>::new(
                state.sync_state,
                state.rows.clone(),
                state.disabled.clone(),
            )
            .dark(false)
            .build(false),
        )
    };
    let mut rt = Runtime::new(state, view, (960.0, 640.0), env(false));
    rt.frame(0.0);
    assert!(audit_has_key(&rt, "data-sync-objects-step-inspecting"));
    assert!(audit_has_key(&rt, "data-sync-objects-step-spinner"));
}
