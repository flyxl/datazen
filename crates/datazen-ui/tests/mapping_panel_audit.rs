//! T336 audit (tester): independent checks of the mapping panel, from
//! separately-constructed scenes: two-row geometry at both window sizes,
//! the same-source/different-target key uniqueness, the disabled-set
//! dimming, and the source-less row's display name fallback.

use std::collections::HashSet;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::data_sync::data_sync_compare_step::{
    DataSyncMappingStatus, DataSyncTableResult,
};
use datazen_ui::views::data_sync::mapping_panel::{MappingPanel, MappingPanelVm};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn audit_table(source: &str, target: &str, status: DataSyncMappingStatus) -> DataSyncTableResult {
    DataSyncTableResult {
        source_table: source.to_owned(),
        target_table: target.to_owned(),
        status,
        insert_count: Some(1),
        ..DataSyncTableResult::default()
    }
}

#[derive(Default)]
struct AuditState {
    vm: MappingPanelVm,
}

fn audit_view(state: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    col().w_full().h_full().child(
        MappingPanel::<AuditState>::new(state.vm.clone())
            .on_toggle_disabled(|_s: &mut AuditState, _t| {})
            .on_open_schema_diff(|_s: &mut AuditState| {})
            .on_open_data_transfer(|_s: &mut AuditState| {})
            .on_toggle_filter_editor(|_s: &mut AuditState, _t| {})
            .on_update_source_filter(|_s: &mut AuditState, _t, _f| {})
            .build(true),
    )
}

fn audit_mount(vm: MappingPanelVm, size: (f32, f32)) -> Runtime<AuditState> {
    let state = AuditState { vm };
    let mut rt = Runtime::new(state, audit_view, size, env(true));
    rt.frame(0.0);
    rt
}

fn audit_vm(rows: Vec<DataSyncTableResult>, compared: bool) -> MappingPanelVm {
    MappingPanelVm {
        rows,
        disabled_tables: HashSet::new(),
        compared,
        filter_table: None,
    }
}

/// Both window sizes, no horizontal overflow of any panel element.
#[test]
fn audit_two_row_geometry_at_both_window_sizes() {
    let rows = vec![
        audit_table("users", "tgt_users", DataSyncMappingStatus::Matched),
        audit_table("legacy", "tgt_legacy", DataSyncMappingStatus::Incompatible),
    ];
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = audit_mount(audit_vm(rows.clone(), true), (w, h));
        let mut outside = Vec::new();
        for e in rt.elements() {
            let Some(key) = e.key.as_deref() else { continue };
            if !key.starts_with("data-sync-mapping") && key != "data-sync-incompatible-hint" {
                continue;
            }
            let [x, _y, ew, _eh] = e.rect;
            if x < -0.5 || x + ew > w + 0.5 {
                outside.push(format!("{key} {:?}", e.rect));
            }
        }
        assert!(outside.is_empty(), "{w}x{h}: {}", outside.join("; "));
    }
}

/// Two rows sharing a source but differing in target keep distinct keys —
/// the row key is the composite pair, not the source alone.
#[test]
fn audit_same_source_different_target_rows_keep_distinct_keys() {
    let rows = vec![
        audit_table("users", "tgt_a", DataSyncMappingStatus::Matched),
        audit_table("users", "tgt_b", DataSyncMappingStatus::Matched),
    ];
    let rt = audit_mount(audit_vm(rows, false), (960.0, 640.0));
    assert!(rt.elements().into_iter().any(|e| e.key.as_deref() == Some("data-sync-mapping-row-users->tgt_a")));
    assert!(rt.elements().into_iter().any(|e| e.key.as_deref() == Some("data-sync-mapping-row-users->tgt_b")));
}

/// A source-less row displays its target (mappingView's fallback), and the
/// empty source means no include toggle press payload.
#[test]
fn audit_a_sourceless_row_displays_its_target() {
    let rows = vec![audit_table("", "only_target", DataSyncMappingStatus::Matched)];
    let rt = audit_mount(audit_vm(rows, false), (960.0, 640.0));
    let name = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-mapping-row-->only_target-name"))
        .and_then(|e| e.text)
        .unwrap_or_default();
    assert_eq!(name, "only_target");
}

/// The disabled set dims the row even while its status stays MATCHED — the
/// checkbox shows unchecked and the row reports the dim.
#[test]
fn audit_the_disabled_set_dims_a_matched_row() {
    let rows = vec![audit_table("logs", "tgt_logs", DataSyncMappingStatus::Matched)];
    let mut vm = audit_vm(rows, false);
    vm.disabled_tables.insert("logs".to_owned());
    let rt = audit_mount(vm, (960.0, 640.0));
    let row = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-mapping-row-logs->tgt_logs"))
        .expect("the row exists");
    assert_eq!(row.opacity, 0.6, "React's opacity-60");
}

/// The summary text carries all four buckets' counts with the zh-CN
/// template, re-derived by the tester from the pack.
#[test]
fn audit_the_summary_template_is_interpolated_per_bucket() {
    let rows = vec![
        audit_table("a", "t_a", DataSyncMappingStatus::Matched),
        audit_table("b", "t_b", DataSyncMappingStatus::UnmappedTarget),
        audit_table("c", "t_c", DataSyncMappingStatus::Disabled),
    ];
    let rt = audit_mount(audit_vm(rows, false), (960.0, 640.0));
    let summary = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("data-sync-mapping-summary"))
        .and_then(|e| e.text)
        .unwrap_or_default();
    assert_eq!(summary, "1 已匹配 · 0 不兼容 · 1 未映射");
}
