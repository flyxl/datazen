//! T024 independent audit: `DataSyncCompareStep`.
//!
//! Written from the React source
//! (`packages/frontend/src/windows/data-sync/DataSyncCompareStep.tsx`,
//! `mappingView.ts`, `useDataSyncSelectionState.ts`,
//! `backend-client/src/types/dataSync.ts`, `commands/sync.ts`) without
//! reusing the coder's values or verdicts. It re-derives the branch
//! matrix over all seven `SyncState` variants, the NUL page key, the
//! empty-string cursor semantics, and the emptiness of the three child
//! mounts (`data-sync-compare-summary`, `data-sync-table-list`,
//! `data-sync-diff-detail`) — the compared branch must render *no* text
//! at all while its children are unported (T288/T308/T383).

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

#[derive(Clone)]
struct AuditProbe {
    vm: DataSyncCompareStepVm,
    dark: bool,
    cancelled: usize,
}

fn audit_view(s: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    DataSyncCompareStep::<AuditProbe>::new(s.vm.clone())
        .on_cancel(|st: &mut AuditProbe| st.cancelled += 1)
        .build(s.dark)
}

fn audit_runtime(vm: DataSyncCompareStepVm, dark: bool, size: (f32, f32)) -> Runtime<AuditProbe> {
    let probe = AuditProbe {
        vm,
        dark,
        cancelled: 0,
    };
    let mut rt = Runtime::new(
        probe,
        audit_view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn audit_has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn audit_element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn audit_texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.text.clone())
        .collect()
}

fn audit_click<S>(rt: &mut Runtime<S>, key: &str) {
    let r = audit_element(rt, key).rect;
    let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.frame(1.0 / 60.0);
}

fn audit_table(source: &str, target: &str) -> DataSyncTableResult {
    DataSyncTableResult {
        source_table: source.to_owned(),
        target_table: target.to_owned(),
        ..DataSyncTableResult::default()
    }
}

fn audit_cursor(next: Option<&str>, previous: &[&str]) -> DataSyncPageCursor {
    DataSyncPageCursor {
        current: None,
        next: next.map(str::to_owned),
        previous: previous.iter().map(|s| s.to_string()).collect(),
    }
}

fn audit_compared_vm() -> DataSyncCompareStepVm {
    DataSyncCompareStepVm {
        sync_state: SyncState::Compared,
        compared: true,
        rows: vec![
            audit_table("customers", "customers"),
            audit_table("orders", "orders"),
        ],
        stats: CompareSummaryStats {
            inserts: 5,
            updates: 2,
            deletes: 1,
            unchanged_tables: 0,
            incompatible: 1,
        },
        filter: TableDiffFilter::Insert,
        search: "cust".to_owned(),
        selected_table_key: Some(table_page_key("customers", "customers")),
        selected_table: Some(audit_table("customers", "customers")),
        ..DataSyncCompareStepVm::default()
    }
}

fn audit_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
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
// i18n + defaults
// ---------------------------------------------------------------------------

#[test]
fn audit_i18n_keys_resolve_and_do_not_echo() {
    for key in [
        "sync.comparing",
        "sync.comparePrompt",
        "sync.selectTableForDetail",
        "common.cancel",
    ] {
        assert!(has(key), "{key} must resolve in the locale table");
        assert_ne!(t(key), key, "{key} must translate, not echo");
    }
}

#[test]
fn audit_sync_options_default_matches_default_sync_options() {
    // commands/sync.ts:88-96 — re-asserted from the React literal, not
    // from the coder's test.
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
    // A non-default options value round-trips through the mirror.
    let custom = SyncOptions {
        insert: false,
        update: false,
        delete: true,
        matching_strategy: None,
        batch_size: None,
        large_value_mode: Some(SyncLargeValueMode::Hash),
        conflict_policy: Some(SyncConflictPolicy::Skip),
    };
    assert!(!custom.insert && custom.delete);
    assert_eq!(custom.large_value_mode, Some(SyncLargeValueMode::Hash));
    assert_eq!(custom.conflict_policy, Some(SyncConflictPolicy::Skip));
    assert_eq!(custom.matching_strategy, None);
    assert_eq!(TableDiffFilter::default(), TableDiffFilter::All);
}

#[test]
fn audit_table_result_default_is_the_empty_inspect_face() {
    // backend-client dataSync.ts TableResult: every optional face is
    // absent by default; the mirror keeps them Option/empty.
    let result = DataSyncTableResult::default();
    assert_eq!(result.source_table, "");
    assert_eq!(result.target_table, "");
    assert_eq!(result.status, DataSyncMappingStatus::Matched);
    assert_eq!(result.incompatible_reason, None);
    assert!(result.warnings.is_empty());
    assert!(result.columns.is_empty());
    assert!(result.column_types.is_empty());
    assert!(result.primary_keys.is_empty());
    assert_eq!(result.unchanged_count, None);
    assert_eq!(result.insert_count, None);
    assert_eq!(result.update_count, None);
    assert_eq!(result.delete_count, None);
    assert_eq!(result.row_count, None);
    assert_eq!(result.page_size, None);
    assert_eq!(result.first_cursor, None);
    assert_eq!(result.has_more, None);
    assert!(result.rows.is_empty());
}

#[test]
fn audit_row_change_round_trips_through_the_mirror() {
    let change = DataSyncRowChange {
        operation: DataSyncOperation::Update,
        key: vec![serde_json::Value::from("cust-7")],
        source_row: Some(vec![serde_json::Value::from(1), serde_json::Value::Null]),
        target_row: Some(vec![serde_json::Value::from(2), serde_json::Value::Null]),
        changed_columns: vec!["balance".to_owned()],
        selected: true,
    };
    // A JSON null cell rides as Value::Null inside the row vector.
    assert_eq!(
        change.source_row.as_ref().unwrap()[1],
        serde_json::Value::Null
    );
    assert_eq!(change.changed_columns, vec!["balance".to_owned()]);
    assert!(change.selected);
    let mut result = audit_table("customers", "customers");
    result.rows = vec![change];
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].operation, DataSyncOperation::Update);
}

// ---------------------------------------------------------------------------
// Pure derivations — values chosen independently of the coder's tests
// ---------------------------------------------------------------------------

#[test]
fn audit_table_page_key_is_nul_separated() {
    // mappingView.ts:74-79: `${source}\u0000${target}`.
    let key = table_page_key("customers", "customers_archive");
    assert_eq!(key, "customers\u{0}customers_archive");
    assert_eq!(key.len(), "customers".len() + 1 + "customers_archive".len());
    assert_eq!(key.chars().filter(|c| *c == '\u{0}').count(), 1);
    // No separator collision: pairs that concat identically stay apart.
    assert_ne!(table_page_key("ab", "c"), table_page_key("a", "bc"));
    assert_ne!(table_page_key("orders", "a"), table_page_key("orders", "b"));
    // (A NUL *inside* a table name could still collide — out of contract:
    // database identifiers cannot contain NUL.)
    assert_eq!(table_page_key("", "only-target"), "\u{0}only-target");
}

#[test]
fn audit_branch_matrix_covers_all_sync_states() {
    // React: `syncState === 'comparing'` wins, then `compared`, else the
    // prompt — for every one of the seven SyncState variants.
    let states = [
        SyncState::Idle,
        SyncState::Inspecting,
        SyncState::Comparing,
        SyncState::Compared,
        SyncState::Executing,
        SyncState::Unknown,
        SyncState::Done,
    ];
    for state in states {
        for compared in [false, true] {
            let vm = DataSyncCompareStepVm {
                sync_state: state,
                compared,
                ..DataSyncCompareStepVm::default()
            };
            let expected = if state == SyncState::Comparing {
                CompareStepBranch::Comparing
            } else if compared {
                CompareStepBranch::Compared
            } else {
                CompareStepBranch::Prompt
            };
            assert_eq!(vm.branch(), expected, "state={state:?} compared={compared}");
        }
    }
}

#[test]
fn audit_has_table_selection_needs_the_exact_pair() {
    let customers = audit_table("customers", "customers");
    let scopes = vec![
        DataSyncTableSelection {
            source_table: "orders".to_owned(),
            target_table: "orders".to_owned(),
            selection_mode: DataSyncSelectionMode::Defaults,
            operations: vec![RowOperation::Delete],
            excluded_rows: Vec::new(),
        },
        DataSyncTableSelection {
            source_table: "customers".to_owned(),
            target_table: "customers_archive".to_owned(),
            selection_mode: DataSyncSelectionMode::All,
            operations: vec![RowOperation::Insert],
            excluded_rows: Vec::new(),
        },
    ];
    assert!(!has_table_selection(&scopes, &customers));
    let mut with_match = scopes.clone();
    with_match.push(DataSyncTableSelection {
        source_table: "customers".to_owned(),
        target_table: "customers".to_owned(),
        selection_mode: DataSyncSelectionMode::All,
        operations: vec![
            RowOperation::Insert,
            RowOperation::Update,
            RowOperation::Delete,
        ],
        excluded_rows: Vec::new(),
    });
    assert!(has_table_selection(&with_match, &customers));
}

#[test]
fn audit_page_index_for_reads_only_its_own_pair() {
    let customers = audit_table("customers", "customers");
    let orders = audit_table("orders", "orders");
    let mut index = HashMap::new();
    index.insert(table_page_key("orders", "orders"), 4);
    index.insert(table_page_key("customers", "customers"), 1);
    assert_eq!(page_index_for(&index, &customers), 1);
    assert_eq!(page_index_for(&index, &orders), 4);
    assert_eq!(
        page_index_for(&index, &audit_table("products", "products")),
        0
    );
}

#[test]
fn audit_cursor_semantics_combine_previous_and_next() {
    let customers = audit_table("customers", "customers");
    let mut cursors = HashMap::new();
    // Previous stack present but next empty: previous yes, next no —
    // React's Boolean(next) is falsy for "".
    cursors.insert(
        table_page_key("customers", "customers"),
        audit_cursor(Some(""), &["page-0"]),
    );
    assert!(has_previous_page(&cursors, &customers));
    assert!(!has_next_page(&cursors, &customers));
    // Next present but no previous stack: the reverse split.
    cursors.insert(
        table_page_key("customers", "customers"),
        audit_cursor(Some("page-2"), &[]),
    );
    assert!(!has_previous_page(&cursors, &customers));
    assert!(has_next_page(&cursors, &customers));
    // A cursor recorded under another table leaks into neither.
    let mut other = HashMap::new();
    other.insert(
        table_page_key("orders", "orders"),
        audit_cursor(Some("page-9"), &["page-8"]),
    );
    assert!(!has_previous_page(&other, &customers));
    assert!(!has_next_page(&other, &customers));
}

#[test]
fn audit_selected_table_page_derives_each_field_independently() {
    let mut vm = audit_compared_vm();
    // No selections / cursors yet: everything is the zero face.
    let page = vm.selected_table_page().expect("a table is selected");
    assert!(!page.has_table_selection);
    assert_eq!(page.page_index, 0);
    assert!(!page.has_previous_page);
    assert!(!page.has_next_page);

    // Cursors for a *different* table must not move this table's page.
    vm.page_index.insert(table_page_key("orders", "orders"), 7);
    vm.page_cursors.insert(
        table_page_key("orders", "orders"),
        audit_cursor(Some("page-9"), &["page-8"]),
    );
    let page = vm.selected_table_page().expect("a table is selected");
    assert_eq!(page.page_index, 0);
    assert!(!page.has_previous_page);
    assert!(!page.has_next_page);

    // Now the selected table's own entries: all four fields flip.
    vm.table_selections = vec![DataSyncTableSelection {
        source_table: "customers".to_owned(),
        target_table: "customers".to_owned(),
        selection_mode: DataSyncSelectionMode::All,
        operations: vec![RowOperation::Insert],
        excluded_rows: Vec::new(),
    }];
    vm.page_index
        .insert(table_page_key("customers", "customers"), 2);
    vm.page_cursors.insert(
        table_page_key("customers", "customers"),
        audit_cursor(Some("page-3"), &["page-1"]),
    );
    let page = vm.selected_table_page().expect("a table is selected");
    assert!(page.has_table_selection);
    assert_eq!(page.page_index, 2);
    assert!(page.has_previous_page);
    assert!(page.has_next_page);

    vm.selected_table = None;
    assert!(vm.selected_table_page().is_none());
}

// ---------------------------------------------------------------------------
// Rendered tree — empty child mounts, cancel journey, light theme
// ---------------------------------------------------------------------------

#[test]
fn audit_comparing_branch_renders_spinner_text_and_cancel() {
    let vm = DataSyncCompareStepVm {
        sync_state: SyncState::Comparing,
        compared: true,
        ..DataSyncCompareStepVm::default()
    };
    // `comparing` wins over `compared` at render time too, not just in
    // branch().
    let rt = audit_runtime(vm, true, DESIGN);
    assert!(audit_has_key(&rt, "data-sync-compare-step"));
    assert!(audit_has_key(&rt, "data-sync-comparing"));
    assert!(audit_has_key(&rt, "data-sync-comparing-spinner"));
    assert_eq!(
        audit_element(&rt, "data-sync-comparing-text")
            .text
            .as_deref(),
        Some(t("sync.comparing"))
    );
    assert_eq!(
        audit_element(&rt, "data-sync-cancel").label.as_deref(),
        Some(t("common.cancel"))
    );
    assert!(!audit_has_key(&rt, "data-sync-compared"));
    assert!(!audit_has_key(&rt, "data-sync-compare-prompt"));
    assert_eq!(
        rt.elements()
            .iter()
            .filter(|e| e.key.as_deref() == Some("kit-root"))
            .count(),
        1
    );
}

#[test]
fn audit_cancel_journey_fires_once_per_press() {
    let vm = DataSyncCompareStepVm {
        sync_state: SyncState::Comparing,
        ..DataSyncCompareStepVm::default()
    };
    let mut rt = audit_runtime(vm, true, DESIGN);
    assert_eq!(rt.state.cancelled, 0);
    audit_click(&mut rt, "data-sync-cancel");
    assert_eq!(rt.state.cancelled, 1);
    audit_click(&mut rt, "data-sync-cancel");
    audit_click(&mut rt, "data-sync-cancel");
    assert_eq!(rt.state.cancelled, 3);
}

#[test]
fn audit_compared_branch_with_selection_renders_no_text_at_all() {
    // The three child mounts are empty until T288/T308/T383 fill them,
    // so the compared branch with a selection must contain zero text
    // nodes — any stray label would be a leak from an unported child.
    let rt = audit_runtime(audit_compared_vm(), true, DESIGN);
    assert!(audit_has_key(&rt, "data-sync-compared"));
    assert!(audit_has_key(&rt, "data-sync-compare-summary"));
    assert!(audit_has_key(&rt, "data-sync-compare-body"));
    assert!(audit_has_key(&rt, "data-sync-table-list"));
    assert!(audit_has_key(&rt, "data-sync-compare-detail"));
    assert!(audit_has_key(&rt, "data-sync-diff-detail"));
    assert!(!audit_has_key(&rt, "data-sync-select-table-prompt"));
    assert!(audit_texts(&rt).is_empty(), "texts: {:?}", audit_texts(&rt));
}

#[test]
fn audit_compared_branch_without_selection_shows_only_the_prompt_text() {
    let mut vm = audit_compared_vm();
    vm.selected_table = None;
    vm.selected_table_key = None;
    let rt = audit_runtime(vm, true, DESIGN);
    assert!(audit_has_key(&rt, "data-sync-compared"));
    assert!(audit_has_key(&rt, "data-sync-compare-summary"));
    assert!(audit_has_key(&rt, "data-sync-table-list"));
    assert!(audit_has_key(&rt, "data-sync-select-table-prompt"));
    assert!(!audit_has_key(&rt, "data-sync-diff-detail"));
    assert_eq!(
        audit_texts(&rt),
        vec![t("sync.selectTableForDetail").to_owned()]
    );
}

#[test]
fn audit_prompt_branch_shows_only_the_compare_prompt_text() {
    let rt = audit_runtime(DataSyncCompareStepVm::default(), true, DESIGN);
    assert!(audit_has_key(&rt, "data-sync-compare-prompt"));
    assert!(!audit_has_key(&rt, "data-sync-comparing"));
    assert!(!audit_has_key(&rt, "data-sync-compared"));
    assert_eq!(audit_texts(&rt), vec![t("sync.comparePrompt").to_owned()]);
}

#[test]
fn audit_light_theme_renders_all_three_branches() {
    let comparing = DataSyncCompareStepVm {
        sync_state: SyncState::Comparing,
        ..DataSyncCompareStepVm::default()
    };
    let rt = audit_runtime(comparing, false, DESIGN);
    assert!(audit_has_key(&rt, "data-sync-comparing"));
    assert!(audit_has_key(&rt, "data-sync-cancel"));

    let rt = audit_runtime(audit_compared_vm(), false, DESIGN);
    assert!(audit_has_key(&rt, "data-sync-compared"));
    assert!(audit_has_key(&rt, "data-sync-diff-detail"));

    let rt = audit_runtime(DataSyncCompareStepVm::default(), false, DESIGN);
    assert!(audit_has_key(&rt, "data-sync-compare-prompt"));
}

// ---------------------------------------------------------------------------
// Geometry — own window check, both viewports, every branch
// ---------------------------------------------------------------------------

#[test]
fn audit_all_branches_stay_inside_both_viewports() {
    let cases: [(&str, DataSyncCompareStepVm); 4] = [
        (
            "comparing",
            DataSyncCompareStepVm {
                sync_state: SyncState::Comparing,
                ..DataSyncCompareStepVm::default()
            },
        ),
        ("compared-no-selection", {
            let mut vm = audit_compared_vm();
            vm.selected_table = None;
            vm
        }),
        ("compared-with-selection", audit_compared_vm()),
        ("prompt", DataSyncCompareStepVm::default()),
    ];
    for (label, vm) in cases {
        for (w, h) in [MIN, DESIGN] {
            let rt = audit_runtime(vm.clone(), true, (w, h));
            audit_in_window(&rt, w, h, &format!("{label} {w}x{h}"));
        }
    }
}
