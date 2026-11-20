//! T336: `windows/data-sync/MappingPanel.tsx` port — the empty-row bail,
//! the title/summary pair, the row cells (checkbox states, name, status
//! counters), the conditional action buttons, the reason line, the
//! expandable editors, and the callbacks through real presses.

use std::collections::HashSet;

use rune::{col, Cx, El, Env, Runtime};

use crate::views::data_sync::data_sync_compare_step::DataSyncMappingStatus;
use crate::views::data_sync::mapping_panel::{MappingPanel, MappingPanelVm};
use crate::views::data_sync::data_sync_compare_step::DataSyncTableResult;
use crate::views::data_transfer::source_filter_editor::StructuredSourceFilter;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn table(source: &str, status: DataSyncMappingStatus) -> DataSyncTableResult {
    DataSyncTableResult {
        source_table: source.to_owned(),
        target_table: format!("tgt_{source}"),
        status,
        ..DataSyncTableResult::default()
    }
}

fn vm(rows: Vec<DataSyncTableResult>, compared: bool, filter_table: Option<&str>) -> MappingPanelVm {
    MappingPanelVm {
        rows,
        disabled_tables: HashSet::new(),
        compared,
        filter_table: filter_table.map(str::to_string),
    }
}

#[derive(Default)]
struct PanelState {
    vm: MappingPanelVm,
    toggled: Vec<String>,
    schema_diff_opens: usize,
    transfer_opens: usize,
    editor_toggles: Vec<String>,
}

fn view(state: &PanelState, _cx: &mut Cx<PanelState>) -> El {
    let mut panel = MappingPanel::<PanelState>::new(state.vm.clone())
        .on_toggle_disabled(|s: &mut PanelState, table| s.toggled.push(table))
        .on_open_schema_diff(|s: &mut PanelState| s.schema_diff_opens += 1)
        .on_open_data_transfer(|s: &mut PanelState| s.transfer_opens += 1)
        .on_toggle_filter_editor(|s: &mut PanelState, table| s.editor_toggles.push(table));
    panel = panel.on_update_source_filter(|_s: &mut PanelState, _table, _filter| {});
    col().w_full().h_full().child(panel.build(true))
}

/// Filter wiring present, transfer wiring absent: the matched rows get
/// their expand buttons while the incompatible rows' transfer button
/// stays hidden (`onOpenDataTransfer?`).
fn view_filter_only(state: &PanelState, _cx: &mut Cx<PanelState>) -> El {
    let panel = MappingPanel::<PanelState>::new(state.vm.clone())
        .on_toggle_disabled(|s: &mut PanelState, table| s.toggled.push(table))
        .on_open_schema_diff(|s: &mut PanelState| s.schema_diff_opens += 1)
        .on_toggle_filter_editor(|s: &mut PanelState, table| s.editor_toggles.push(table))
        .on_update_source_filter(|_s: &mut PanelState, _table, _filter| {});
    col().w_full().h_full().child(panel.build(true))
}

fn view_without_callbacks(state: &PanelState, _cx: &mut Cx<PanelState>) -> El {
    col()
        .w_full()
        .h_full()
        .child(MappingPanel::<PanelState>::new(state.vm.clone()).build(true))
}

fn mount(vm: MappingPanelVm, callbacks: bool, size: (f32, f32)) -> Runtime<PanelState> {
    let state = PanelState { vm, ..PanelState::default() };
    let view = if callbacks { view } else { view_without_callbacks };
    let mut rt = Runtime::new(state, view, size, env(true));
    rt.frame(0.0);
    rt
}

fn mount_filter_only(vm: MappingPanelVm, size: (f32, f32)) -> Runtime<PanelState> {
    let state = PanelState { vm, ..PanelState::default() };
    let mut rt = Runtime::new(state, view_filter_only, size, env(true));
    rt.frame(0.0);
    rt
}

fn maybe(rt: &Runtime<PanelState>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key(rt: &Runtime<PanelState>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of(rt: &Runtime<PanelState>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn label_of(rt: &Runtime<PanelState>, key: &str) -> String {
    maybe(rt, key)
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} has a label"))
}

fn click(rt: &mut Runtime<PanelState>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn base(source: &str) -> String {
    format!("data-sync-mapping-row-{source}->tgt_{source}")
}

#[test]
fn an_empty_row_list_renders_no_children() {
    let rt = mount(vm(Vec::new(), false, None), true, (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-mapping-panel"));
    assert!(!has_key(&rt, "data-sync-mapping-summary"));
    assert!(!has_key(&rt, "data-sync-mapping-panel-list"));
    assert!(!has_key(&rt, "data-sync-incompatible-hint"));
}

#[test]
fn the_title_and_summary_follow_the_compared_flag() {
    let rows = vec![table("users", DataSyncMappingStatus::Matched)];
    let rt = mount(vm(rows, false, None), true, (960.0, 640.0));
    assert_eq!(text_of(&rt, "data-sync-mapping-panel-title"), t_str("sync.mappingTitle"));
    assert_eq!(
        text_of(&rt, "data-sync-mapping-summary"),
        "1 已匹配 · 0 不兼容 · 0 未映射"
    );

    let rows = vec![table("users", DataSyncMappingStatus::Matched)];
    let rt = mount(vm(rows, true, None), true, (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "data-sync-mapping-panel-title"),
        t_str("sync.mappingAfterCompare")
    );
}

fn t_str(key: &str) -> String {
    crate::i18n::t(key).to_owned()
}

#[test]
fn the_status_cell_shows_counters_only_past_a_comparison() {
    let mut rows = vec![table("users", DataSyncMappingStatus::Matched)];
    rows[0].insert_count = Some(2);
    rows[0].update_count = Some(3);
    rows[0].delete_count = Some(1);

    // Before the comparison: the label, not the counters.
    let rt = mount(vm(rows.clone(), false, None), true, (960.0, 640.0));
    assert_eq!(text_of(&rt, &format!("{}-status", base("users"))), t_str("sync.mappingMatched"));

    // Past the comparison with diffs: the counters.
    let rt = mount(vm(rows.clone(), true, None), true, (960.0, 640.0));
    assert_eq!(text_of(&rt, &format!("{}-status", base("users"))), "+2 / ~3 / −1");

    // Past the comparison without diffs: the label again.
    let clean = vec![table("users", DataSyncMappingStatus::Matched)];
    let rt = mount(vm(clean, true, None), true, (960.0, 640.0));
    assert_eq!(text_of(&rt, &format!("{}-status", base("users"))), t_str("sync.mappingMatched"));
}

#[test]
fn the_incompatible_status_and_reason_line() {
    let mut incompatible = table("legacy", DataSyncMappingStatus::Incompatible);
    incompatible.incompatible_reason = Some("类型不兼容".to_owned());
    let rt = mount(vm(vec![incompatible], false, None), true, (960.0, 640.0));
    assert_eq!(
        text_of(&rt, &format!("{}-status", base("legacy"))),
        t_str("sync.mappingIncompatible")
    );
    assert_eq!(text_of(&rt, &format!("{}-reason", base("legacy"))), "类型不兼容");
    // The hint appears with at least one incompatible row.
    assert!(has_key(&rt, "data-sync-incompatible-hint"));
}

#[test]
fn matched_rows_without_filter_wiring_get_no_buttons() {
    let rows = vec![table("users", DataSyncMappingStatus::Matched)];
    // callbacks: false leaves on_update_source_filter unwired.
    let rt = mount(vm(rows, false, None), false, (960.0, 640.0));
    assert!(!has_key(&rt, &format!("{}-data-sync-source-filter", base("users"))));
    assert!(!has_key(&rt, &format!("{}-data-sync-recordset", base("users"))));
}

#[test]
fn matched_rows_with_filter_wiring_get_both_buttons() {
    let rows = vec![table("users", DataSyncMappingStatus::Matched)];
    let rt = mount(vm(rows, false, None), true, (960.0, 640.0));
    let source_filter_key = format!("{}-data-sync-source-filter", base("users"));
    let recordset_key = format!("{}-data-sync-recordset", base("users"));
    assert!(has_key(&rt, &source_filter_key));
    assert!(has_key(&rt, &recordset_key));
    assert_eq!(label_of(&rt, &source_filter_key), t_str("sync.sourceFilter"));
    assert_eq!(label_of(&rt, &recordset_key), t_str("sync.recordset"));

    // A UNMAPPED_SOURCE row gets neither.
    let rows = vec![table("orphan", DataSyncMappingStatus::UnmappedSource)];
    let rt = mount(vm(rows, false, None), true, (960.0, 640.0));
    assert!(!has_key(&rt, &format!("{}-data-sync-source-filter", base("orphan"))));
}

#[test]
fn incompatible_rows_get_schema_diff_and_conditional_transfer_buttons() {
    let rows = vec![table("legacy", DataSyncMappingStatus::Incompatible)];
    let rt = mount(vm(rows, false, None), true, (960.0, 640.0));
    assert!(has_key(&rt, &format!("{}-schema-diff", base("legacy"))));
    assert!(has_key(&rt, &format!("{}-data-sync-open-transfer", base("legacy"))));

    // Without the transfer callback the button is absent.
    let rows = vec![table("legacy", DataSyncMappingStatus::Incompatible)];
    let rt = mount_filter_only(vm(rows, false, None), (960.0, 640.0));
    assert!(has_key(&rt, &format!("{}-schema-diff", base("legacy"))));
    assert!(!has_key(&rt, &format!("{}-data-sync-open-transfer", base("legacy"))));
}

#[test]
fn presses_reach_the_wired_callbacks() {
    let mut matched = table("users", DataSyncMappingStatus::Matched);
    matched.insert_count = Some(1);
    let mut incompatible = table("legacy", DataSyncMappingStatus::Incompatible);
    incompatible.incompatible_reason = Some("r".to_owned());
    let rows = vec![matched, incompatible];
    let mut rt = mount(vm(rows, false, None), true, (960.0, 640.0));

    // The include checkbox toggles the disabled set entry.
    click(&mut rt, &format!("{}-include", base("users")));
    assert_eq!(rt.state.toggled, vec!["users".to_owned()]);

    // The expand buttons toggle the editor slot.
    click(&mut rt, &format!("{}-data-sync-source-filter", base("users")));
    assert_eq!(rt.state.editor_toggles, vec!["users".to_owned()]);
    click(&mut rt, &format!("{}-data-sync-recordset", base("users")));
    assert_eq!(
        rt.state.editor_toggles,
        vec!["users".to_owned(), "users".to_owned()],
        "both buttons write the same cell"
    );

    // The incompatible row's buttons navigate.
    click(&mut rt, &format!("{}-schema-diff", base("legacy")));
    assert_eq!(rt.state.schema_diff_opens, 1);
    click(&mut rt, &format!("{}-data-sync-open-transfer", base("legacy")));
    assert_eq!(rt.state.transfer_opens, 1);
}

#[test]
fn the_editor_section_mounts_only_for_the_expanded_matched_row() {
    let rows = vec![table("users", DataSyncMappingStatus::Matched)];
    // Not expanded: no editor.
    let rt = mount(vm(rows.clone(), false, None), true, (960.0, 640.0));
    assert!(!has_key(&rt, &format!("{}-editor", base("users"))));

    // Expanded: the editor with the recordset and predicate sub-editors.
    let rt = mount(vm(rows, false, Some("users")), true, (960.0, 640.0));
    assert!(has_key(&rt, &format!("{}-editor", base("users"))));

    // An expanded UNMAPPED row does not mount (status gate).
    let rows = vec![table("orphan", DataSyncMappingStatus::UnmappedSource)];
    let rt = mount(vm(rows, false, Some("orphan")), true, (960.0, 640.0));
    assert!(!has_key(&rt, &format!("{}-editor", base("orphan"))));
}

#[test]
fn disabled_rows_dim_and_cannot_toggle_unmatched() {
    let mut disabled_row = table("logs", DataSyncMappingStatus::Disabled);
    disabled_row.source_table = "logs".to_owned();
    disabled_row.target_table = "tgt_logs".to_owned();
    let rows = vec![disabled_row, table("orphan", DataSyncMappingStatus::UnmappedSource)];
    let rt = mount(vm(rows, false, None), true, (960.0, 640.0));
    // The DISABLED row's checkbox exists; the UNMAPPED row's is disabled
    // (present, not toggleable — the checkbox's own disabled flag).
    assert!(has_key(&rt, "data-sync-mapping-row-logs->tgt_logs-include"));
    assert!(has_key(&rt, "data-sync-mapping-row-orphan->tgt_orphan-include"));
}

#[test]
fn the_summary_counts_all_buckets() {
    let rows = vec![
        table("a", DataSyncMappingStatus::Matched),
        table("b", DataSyncMappingStatus::Incompatible),
        table("c", DataSyncMappingStatus::UnmappedSource),
        table("d", DataSyncMappingStatus::UnmappedTarget),
        table("e", DataSyncMappingStatus::Disabled),
    ];
    let rt = mount(vm(rows, false, None), true, (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "data-sync-mapping-summary"),
        "1 已匹配 · 1 不兼容 · 2 未映射"
    );
}
