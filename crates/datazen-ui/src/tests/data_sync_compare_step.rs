//! T024: `DataSyncCompareStep` — the three branches (comparing / compared /
//! prompt), the cancel journey, the page-key/cursor derivations, and the
//! empty child mounts (`data-sync-compare-summary`, `data-sync-table-list`,
//! `data-sync-diff-detail`) the child tasks fill.
//!
//! Probes the rendered element tree (keys, single `kit-root` mount,
//! geometry), drives the real cancel button through a pointer journey,
//! and checks the pure derivations against the React source. Every
//! assertion observes the runtime tree or the probe's recorded cancels,
//! not the component's own report.

use std::collections::HashMap;

use rune::{Cx, El, Env, Runtime};

use crate::i18n::{has, t};
use crate::views::{
    has_next_page, has_previous_page, has_table_selection, page_index_for, table_page_key,
    CompareStepBranch, CompareSummaryStats, DataSyncCompareStep, DataSyncCompareStepVm,
    DataSyncMappingStatus, DataSyncOperation, DataSyncPageCursor, DataSyncRowChange,
    DataSyncSelectionMode, DataSyncTableResult, DataSyncTableSelection, RowOperation,
    SyncConflictPolicy, SyncLargeValueMode, SyncMatchingStrategy, SyncOptions, SyncState,
    TableDiffFilter,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn table(source: &str, target: &str) -> DataSyncTableResult {
    DataSyncTableResult {
        source_table: source.to_owned(),
        target_table: target.to_owned(),
        ..DataSyncTableResult::default()
    }
}

fn selection(source: &str, target: &str) -> DataSyncTableSelection {
    DataSyncTableSelection {
        source_table: source.to_owned(),
        target_table: target.to_owned(),
        selection_mode: DataSyncSelectionMode::All,
        operations: vec![RowOperation::Insert, RowOperation::Update],
        excluded_rows: Vec::new(),
    }
}

fn cursor(next: Option<&str>, previous: &[&str]) -> DataSyncPageCursor {
    DataSyncPageCursor {
        current: None,
        next: next.map(str::to_owned),
        previous: previous.iter().map(|s| s.to_string()).collect(),
    }
}

fn row_change(operation: DataSyncOperation) -> DataSyncRowChange {
    DataSyncRowChange {
        operation,
        key: vec![serde_json::Value::from(1)],
        source_row: None,
        target_row: None,
        changed_columns: Vec::new(),
        selected: false,
    }
}

// ---------------------------------------------------------------------------
// Standalone probe
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Probe {
    vm: DataSyncCompareStepVm,
    dark: bool,
    cancelled: usize,
}

fn comparing_vm() -> DataSyncCompareStepVm {
    DataSyncCompareStepVm {
        sync_state: SyncState::Comparing,
        ..DataSyncCompareStepVm::default()
    }
}

fn compared_vm() -> DataSyncCompareStepVm {
    DataSyncCompareStepVm {
        sync_state: SyncState::Compared,
        compared: true,
        rows: vec![table("orders", "orders"), table("users", "users")],
        stats: CompareSummaryStats {
            inserts: 3,
            updates: 1,
            deletes: 0,
            unchanged_tables: 1,
            incompatible: 0,
        },
        filter: TableDiffFilter::All,
        selected_table: Some(table("orders", "orders")),
        ..DataSyncCompareStepVm::default()
    }
}

fn probe(vm: DataSyncCompareStepVm) -> Probe {
    Probe {
        vm,
        dark: true,
        cancelled: 0,
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    DataSyncCompareStep::<Probe>::new(s.vm.clone())
        .on_cancel(|st: &mut Probe| st.cancelled += 1)
        .build(s.dark)
}

fn unwired_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    DataSyncCompareStep::<Probe>::new(s.vm.clone()).build(s.dark)
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn runtime_unwired(probe: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(probe, unwired_view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key)
        .text
        .unwrap_or_else(|| panic!("{key} has text"))
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key)
        .label
        .unwrap_or_else(|| panic!("{key} has a label"))
}

fn kit_roots<S>(rt: &Runtime<S>) -> usize {
    rt.elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some("kit-root"))
        .count()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = element(rt, key).rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---------------------------------------------------------------------------
// Pure logic
// ---------------------------------------------------------------------------

#[test]
fn the_i18n_keys_the_step_asks_for_resolve() {
    for key in [
        "sync.comparing",
        "sync.comparePrompt",
        "sync.selectTableForDetail",
        "common.cancel",
    ] {
        assert!(has(key), "{key} must be in the zh-CN locale table");
        assert_ne!(t(key), key, "{key} must not echo the key");
    }
}

#[test]
fn sync_options_default_is_the_react_default() {
    // commands/sync.ts:88-96 DEFAULT_SYNC_OPTIONS.
    assert_eq!(
        SyncOptions::default(),
        SyncOptions {
            insert: true,
            update: true,
            delete: false,
            matching_strategy: Some(SyncMatchingStrategy::PrimaryKey),
            batch_size: Some(1000),
            large_value_mode: Some(SyncLargeValueMode::Full),
            conflict_policy: Some(SyncConflictPolicy::Abort),
        }
    );
}

#[test]
fn table_page_key_joins_with_nul() {
    // mappingView.ts:74-79.
    assert_eq!(table_page_key("orders", "orders"), "orders\u{0}orders");
    assert_eq!(table_page_key("", ""), "\u{0}");
    // The NUL separator is what keeps pairs apart: without it, "a"+"bc"
    // and "ab"+"c" would collide; with it they cannot.
    assert_ne!(table_page_key("a", "bc"), table_page_key("ab", "c"));
}

#[test]
fn has_table_selection_matches_the_pair() {
    let orders = table("orders", "orders");
    assert!(!has_table_selection(&[], &orders));
    assert!(has_table_selection(
        &[selection("orders", "orders")],
        &orders
    ));
    assert!(!has_table_selection(
        &[selection("orders", "orders_archive")],
        &orders
    ));
    assert!(!has_table_selection(
        &[selection("orders_archive", "orders")],
        &orders
    ));
    assert!(has_table_selection(
        &[selection("users", "users"), selection("orders", "orders")],
        &orders
    ));
}

#[test]
fn page_index_for_defaults_to_zero() {
    let orders = table("orders", "orders");
    let mut index = HashMap::new();
    assert_eq!(page_index_for(&index, &orders), 0);
    index.insert(table_page_key("orders", "orders"), 2);
    assert_eq!(page_index_for(&index, &orders), 2);
    // A cursor for another table does not leak into this one.
    assert_eq!(page_index_for(&index, &table("users", "users")), 0);
}

#[test]
fn has_previous_page_needs_a_non_empty_stack() {
    let orders = table("orders", "orders");
    let mut cursors = HashMap::new();
    assert!(!has_previous_page(&cursors, &orders));
    cursors.insert(table_page_key("orders", "orders"), cursor(None, &[]));
    assert!(!has_previous_page(&cursors, &orders));
    cursors.insert(
        table_page_key("orders", "orders"),
        cursor(None, &["page-1"]),
    );
    assert!(has_previous_page(&cursors, &orders));
}

#[test]
fn has_next_page_needs_a_non_empty_cursor() {
    let orders = table("orders", "orders");
    let mut cursors = HashMap::new();
    assert!(!has_next_page(&cursors, &orders));
    cursors.insert(table_page_key("orders", "orders"), cursor(None, &[]));
    assert!(!has_next_page(&cursors, &orders));
    // React's Boolean(next) is falsy for "" as well as for null.
    cursors.insert(table_page_key("orders", "orders"), cursor(Some(""), &[]));
    assert!(!has_next_page(&cursors, &orders));
    cursors.insert(
        table_page_key("orders", "orders"),
        cursor(Some("page-2"), &[]),
    );
    assert!(has_next_page(&cursors, &orders));
}

#[test]
fn branch_order_is_comparing_then_compared_then_prompt() {
    let mut vm = DataSyncCompareStepVm::default();
    assert_eq!(vm.branch(), CompareStepBranch::Prompt);
    vm.sync_state = SyncState::Inspecting;
    assert_eq!(vm.branch(), CompareStepBranch::Prompt);
    vm.sync_state = SyncState::Executing;
    assert_eq!(vm.branch(), CompareStepBranch::Prompt);
    // `compared` alone shows the result.
    vm.sync_state = SyncState::Compared;
    vm.compared = true;
    assert_eq!(vm.branch(), CompareStepBranch::Compared);
    // `comparing` wins over `compared` — React checks it first.
    vm.sync_state = SyncState::Comparing;
    assert_eq!(vm.branch(), CompareStepBranch::Comparing);
    // And `compared` without the flag is still the prompt.
    vm.sync_state = SyncState::Done;
    vm.compared = false;
    assert_eq!(vm.branch(), CompareStepBranch::Prompt);
}

#[test]
fn selected_table_page_derives_the_diff_detail_props() {
    let mut vm = compared_vm();
    let page = vm.selected_table_page().expect("a table is selected");
    assert!(!page.has_table_selection);
    assert_eq!(page.page_index, 0);
    assert!(!page.has_previous_page);
    assert!(!page.has_next_page);

    vm.table_selections = vec![selection("orders", "orders")];
    vm.page_index.insert(table_page_key("orders", "orders"), 3);
    vm.page_cursors.insert(
        table_page_key("orders", "orders"),
        cursor(Some("page-4"), &["page-2", "page-1"]),
    );
    let page = vm.selected_table_page().expect("a table is selected");
    assert!(page.has_table_selection);
    assert_eq!(page.page_index, 3);
    assert!(page.has_previous_page);
    assert!(page.has_next_page);

    vm.selected_table = None;
    assert!(vm.selected_table_page().is_none());
}

#[test]
fn table_result_defaults_carry_the_preview_face() {
    // The frontend TableResult spans the inspect face (rows) and the
    // preview face (counts/paging); the mirror keeps both optional.
    let mut result = table("orders", "orders");
    result.rows = vec![
        row_change(DataSyncOperation::Insert),
        row_change(DataSyncOperation::Unchanged),
    ];
    result.insert_count = Some(1);
    result.status = DataSyncMappingStatus::Matched;
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.insert_count, Some(1));
    assert_eq!(result.page_size, None);
    assert_eq!(result.first_cursor, None);
    assert_eq!(result.has_more, None);
}

// ---------------------------------------------------------------------------
// Comparing branch
// ---------------------------------------------------------------------------

#[test]
fn comparing_branch_shows_spinner_text_and_cancel() {
    let rt = runtime(probe(comparing_vm()), DESIGN);
    assert!(has_key(&rt, "data-sync-compare-step"));
    assert!(has_key(&rt, "data-sync-comparing"));
    assert!(has_key(&rt, "data-sync-comparing-spinner"));
    assert_eq!(
        text_of(&rt, "data-sync-comparing-text"),
        t("sync.comparing")
    );
    assert!(has_key(&rt, "data-sync-cancel"));
    // The kit Button announces its label on the button root; the inner
    // text child is a11y-hidden by the kit.
    assert_eq!(label_of(&rt, "data-sync-cancel"), t("common.cancel"));
    assert!(!has_key(&rt, "data-sync-compared"));
    assert!(!has_key(&rt, "data-sync-compare-prompt"));
    assert_eq!(kit_roots(&rt), 1);
}

#[test]
fn cancel_button_fires_on_cancel() {
    let mut rt = runtime(probe(comparing_vm()), DESIGN);
    assert_eq!(rt.state.cancelled, 0);
    click(&mut rt, "data-sync-cancel");
    assert_eq!(rt.state.cancelled, 1, "one press calls onCancel once");
    click(&mut rt, "data-sync-cancel");
    assert_eq!(rt.state.cancelled, 2, "a second press calls it again");
}

#[test]
fn unwired_cancel_button_renders_without_a_handler() {
    let mut rt = runtime_unwired(probe(comparing_vm()), DESIGN);
    assert!(has_key(&rt, "data-sync-cancel"));
    assert_eq!(label_of(&rt, "data-sync-cancel"), t("common.cancel"));
    // Pressing it runs the no-op fallback, not a handler: nothing
    // happens and nothing panics.
    click(&mut rt, "data-sync-cancel");
    assert_eq!(rt.state.cancelled, 0);
}

// ---------------------------------------------------------------------------
// Compared branch
// ---------------------------------------------------------------------------

#[test]
fn compared_branch_mounts_the_child_slots_and_the_detail_prompt() {
    let mut vm = compared_vm();
    vm.selected_table = None;
    let rt = runtime(probe(vm), DESIGN);
    assert!(has_key(&rt, "data-sync-compared"));
    assert!(has_key(&rt, "data-sync-compare-summary"));
    assert!(has_key(&rt, "data-sync-compare-body"));
    assert!(has_key(&rt, "data-sync-table-list"));
    assert!(has_key(&rt, "data-sync-compare-detail"));
    assert!(has_key(&rt, "data-sync-select-table-prompt"));
    assert_eq!(
        text_of(&rt, "data-sync-select-table-prompt-text"),
        t("sync.selectTableForDetail")
    );
    assert!(!has_key(&rt, "data-sync-diff-detail"));
    assert!(!has_key(&rt, "data-sync-comparing"));
    assert!(!has_key(&rt, "data-sync-compare-prompt"));
    assert_eq!(kit_roots(&rt), 1);
}

#[test]
fn compared_branch_with_a_selection_mounts_the_diff_detail_slot() {
    let rt = runtime(probe(compared_vm()), DESIGN);
    assert!(has_key(&rt, "data-sync-diff-detail"));
    assert!(!has_key(&rt, "data-sync-select-table-prompt"));
    assert_eq!(kit_roots(&rt), 1);
}

// ---------------------------------------------------------------------------
// Prompt branch
// ---------------------------------------------------------------------------

#[test]
fn prompt_branch_shows_the_compare_prompt() {
    let rt = runtime(probe(DataSyncCompareStepVm::default()), DESIGN);
    assert!(has_key(&rt, "data-sync-compare-prompt"));
    assert_eq!(
        text_of(&rt, "data-sync-compare-prompt-text"),
        t("sync.comparePrompt")
    );
    assert!(!has_key(&rt, "data-sync-comparing"));
    assert!(!has_key(&rt, "data-sync-compared"));
    assert_eq!(kit_roots(&rt), 1);
}

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

#[test]
fn all_branches_stay_inside_both_viewports() {
    for (label, vm) in [
        ("comparing", comparing_vm()),
        ("compared-no-selection", {
            let mut vm = compared_vm();
            vm.selected_table = None;
            vm
        }),
        ("compared-with-selection", compared_vm()),
        ("prompt", DataSyncCompareStepVm::default()),
    ] {
        for (w, h) in [MIN, DESIGN] {
            let rt = runtime(probe(vm.clone()), (w, h));
            assert_in_window(&rt, w, h, &format!("{label} {w}x{h}"));
        }
    }
}
