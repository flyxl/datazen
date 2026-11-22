//! T554: `TableView` decisions — grid columns, editors, insert drafts, dirty
//! index, filter operators, navigation guard, Quick Look and view geometry.

use std::collections::BTreeMap;
use rune::{col, Cx, El, Env, Runtime};

use crate::views::table_view::{
    build, can_start_edit, dirty_counts, dirty_row, displayed_columns, edit_refusal,
    filter_operators, has_pending_work, is_quick_lookable, navigation_allowed,
    normalize_selection_range, quick_look_target, read_only_message_key, reduce_notice,
    resolve_default_sort, tip_expired, CellValue, ColumnMeta, DirtyCell, DirtyRow, DraftRow,
    NoticeEvent, ReadOnlyReason, SHARED_OPERATORS, TableViewAction, TableViewVm,
};

fn columns() -> Vec<ColumnMeta> {
    vec![
        ColumnMeta { name: "id".to_owned(), data_type: "integer".to_owned(), nullable: false },
        ColumnMeta { name: "name".to_owned(), data_type: "text".to_owned(), nullable: true },
    ]
}

fn vm() -> TableViewVm {
    TableViewVm {
        columns: columns(),
        rows: vec![
            vec![CellValue::Number("1".to_owned()), CellValue::Text("alpha".to_owned())],
            vec![CellValue::Number("2".to_owned()), CellValue::Text("beta".to_owned())],
        ],
        has_primary_key: true,
        ..Default::default()
    }
}

#[derive(Clone)]
struct Scene {
    vm: TableViewVm,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(build(&s.vm, None::<std::rc::Rc<dyn Fn(&mut Scene, TableViewAction) + 'static>>, s.dark))
}

fn runtime(vm: TableViewVm, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let mut rt = Runtime::new(Scene { vm, dark }, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

// ---- Grid / column selection (TableView.test.tsx) ---------------------------

#[test]
fn displayed_columns_honor_visibility_and_explicit_order() {
    assert_eq!(displayed_columns(&columns(), None, None), vec!["id", "name"]);
    let visible = vec!["name".to_owned()];
    assert_eq!(displayed_columns(&columns(), Some(&visible), None), vec!["name"]);
    let order = vec!["name".to_owned(), "id".to_owned()];
    assert_eq!(displayed_columns(&columns(), None, Some(&order)), vec!["name", "id"]);
    let empty: Vec<String> = Vec::new();
    assert!(displayed_columns(&columns(), Some(&empty), None).is_empty(), "all hidden");
}

#[test]
fn grid_mounts_rows_cells_and_hidden_markers() {
    let rt = runtime(vm(), (960.0, 640.0), true);
    for key in ["table-view", "table-view-grid", "table-view-grid-head", "table-view-grid-row-0", "table-view-grid-cell-0-0-text"] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
}

// ---- Editors (TableView.editors.test.tsx) -----------------------------------

#[test]
fn editor_refusals_name_the_real_reason() {
    let mut state = vm();
    state.read_only = true;
    assert_eq!(edit_refusal(&state, 0), Some(ReadOnlyReason::ConnectionReadOnly));
    state.read_only = false;
    state.read_only_reason = Some(ReadOnlyReason::View);
    assert_eq!(edit_refusal(&state, 0), Some(ReadOnlyReason::View), "a view reports the view reason");
    state.read_only_reason = None;
    state.has_primary_key = false;
    assert_eq!(edit_refusal(&state, 0), Some(ReadOnlyReason::NoPrimaryKey));
    state.has_primary_key = true;
    assert_eq!(edit_refusal(&state, 0), None);
    assert_eq!(read_only_message_key(ReadOnlyReason::NoPrimaryKey), "cellEditor.readOnly.noPrimaryKey");
}

#[test]
fn deleted_rows_cannot_start_an_edit_but_staged_cells_can() {
    let mut state = vm();
    state.dirty.insert(0, DirtyRow { cells: Vec::new(), deleted: true });
    assert!(!can_start_edit(&state, 0), "delete-marked row is not editable");
    state.dirty.insert(0, DirtyRow {
        cells: vec![DirtyCell {
            column: "name".to_owned(),
            original: CellValue::Text("alpha".to_owned()),
            next: CellValue::Text("alice".to_owned()),
        }],
        deleted: false,
    });
    assert!(can_start_edit(&state, 0), "a staged cell edit stays editable");
}

// ---- Insert drafts & notice (TableView.insert*.test.tsx) --------------------

#[test]
fn add_row_entry_is_gated_by_read_only_and_drafts_are_host_owned() {
    let mut state = vm();
    state.drafts.push(DraftRow { id: "d1".to_owned(), values: BTreeMap::new() });
    assert!(has_pending_work(&state));
    let rt = runtime(state.clone(), (960.0, 640.0), true);
    assert!(has_key(&rt, "table-view-insert-draft"));
    state.read_only = true;
    state.drafts.clear();
    assert!(!has_pending_work(&state));
    let rt = runtime(state, (960.0, 640.0), true);
    assert!(!has_key(&rt, "table-view-insert-draft"));
}

#[test]
fn insert_notice_expires_on_page_state_changes_and_later_success() {
    let (notice, _) = reduce_notice(None, NoticeEvent::CommitNotLocated);
    assert_eq!(notice.as_deref(), Some("the new row could not be located"));
    for event in [
        NoticeEvent::PageChanged,
        NoticeEvent::LoadRetried,
        NoticeEvent::Reloaded,
        NoticeEvent::ContextChanged,
    ] {
        let label = format!("{event:?}");
        let (notice, _) = reduce_notice(notice.as_deref(), event);
        assert!(notice.is_none(), "notice invalidated by {label}");
    }
    let (notice, row) = reduce_notice(None, NoticeEvent::CommitLocated(7));
    assert!(notice.is_none());
    assert_eq!(row, Some(7), "a located insert marks its row");
}

// ---- Dirty index (TableView.dirty.test.tsx) ---------------------------------

#[test]
fn dirty_counts_read_the_single_index() {
    let mut state = vm();
    state.dirty.insert(0, DirtyRow {
        cells: vec![
            DirtyCell { column: "id".to_owned(), original: CellValue::Number("1".to_owned()), next: CellValue::Number("9".to_owned()) },
            DirtyCell { column: "name".to_owned(), original: CellValue::Text("a".to_owned()), next: CellValue::Text("b".to_owned()) },
        ],
        deleted: false,
    });
    state.dirty.insert(1, DirtyRow { cells: Vec::new(), deleted: true });
    assert_eq!(dirty_counts(&state), (2, 2), "two changed cells across two rows");
    assert!(dirty_row(&state, 1).is_some_and(|row| row.deleted), "delete-marked row is in the index");
}

// ---- Filter operators (TableView.filterOperators.test.tsx) ------------------

#[test]
fn no_driver_declaration_gets_the_shared_operator_set() {
    let shared = filter_operators(&[]);
    assert_eq!(shared.len(), SHARED_OPERATORS.len());
    assert!(shared.iter().any(|op| op == "isNotNull"));
    assert!(!shared.iter().any(|op| op == "regex"), "dialect operators are opt-in");
}

#[test]
fn a_declared_subset_is_used_verbatim() {
    let declared = vec!["eq".to_owned(), "contains".to_owned()];
    assert_eq!(filter_operators(&declared), declared);
}

// ---- Navigation guard (TableView.navigationGuard.test.tsx) ------------------

#[test]
fn invalid_editor_draft_blocks_navigation_reads() {
    let mut state = vm();
    assert!(navigation_allowed(&state));
    state.invalid_draft = true;
    assert!(!navigation_allowed(&state), "exact-count re-read, retry and page change are refused");
    let rt = runtime(state, (960.0, 640.0), true);
    assert!(has_key(&rt, "table-view-navigation-blocked"));
}

// ---- Quick Look (TableView.quickLook.test.tsx) ------------------------------

#[test]
fn quick_lookable_values_are_length_json_or_blob() {
    assert!(!is_quick_lookable(&CellValue::Null, "text"));
    assert!(!is_quick_lookable(&CellValue::Text("short".to_owned()), "text"));
    assert!(is_quick_lookable(&CellValue::Text("x".repeat(121)), "text"));
    assert!(is_quick_lookable(&CellValue::Json("{}".to_owned()), "text"));
    assert!(is_quick_lookable(&CellValue::Blob(vec![1]), "bytea"));
}

#[test]
fn quick_look_target_resolves_the_focused_cell() {
    let mut state = vm();
    state.rows[0][1] = CellValue::Text("x".repeat(130));
    assert_eq!(quick_look_target(&state, 0, "name"), Some((0, "name".to_owned())));
    assert_eq!(quick_look_target(&state, 0, "id"), None, "a short number is not quick-lookable");
    assert_eq!(quick_look_target(&state, 0, "missing"), None);
}

// ---- Misc pure contracts -----------------------------------------------------

#[test]
fn default_sort_and_selection_range_are_normalized() {
    assert_eq!(resolve_default_sort("inherit", "primaryKeyAsc"), "primaryKeyAsc");
    assert_eq!(resolve_default_sort("nameDesc", "primaryKeyAsc"), "nameDesc");
    assert_eq!(normalize_selection_range(5, 2), (2, 5));
    assert_eq!(normalize_selection_range(2, 5), (2, 5));
}

#[test]
fn read_only_tip_deadline_is_recorded_not_owned() {
    let mut state = vm();
    state.show_read_only_tip = Some(ReadOnlyReason::ConnectionReadOnly);
    state.read_only_tip_until = Some(2.5);
    assert!(!tip_expired(&state, 2.49));
    assert!(tip_expired(&state, 2.5));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = vm();
            state.sort = Some((0, rune_kit::SortDir::Ascending));
            state.highlighted_row = Some(1);
            state.drafts.push(DraftRow::default());
            state.quick_look = Some((0, "name".to_owned()));
            state.inserted_row = Some(0);
            let rt = runtime(state, size, dark);
            let outside: Vec<String> = rt.elements().iter()
                .filter(|e| {
                    let r = e.rect;
                    r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
                })
                .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
                .collect();
            assert!(outside.is_empty(), "{size:?} dark={dark}: outside {outside:?}");
        }
    }
}

#[test]
fn cell_values_display_every_variant() {
    assert_eq!(CellValue::Null.display(), "NULL");
    assert_eq!(CellValue::Text("hi".to_owned()).display(), "hi");
    assert_eq!(CellValue::Number("42".to_owned()).display(), "42");
    assert_eq!(CellValue::Bool(true).display(), "true");
    assert_eq!(CellValue::Json("{}".to_owned()).display(), "{}");
    // A blob previews at most eight bytes as lowercase hex.
    assert_eq!(
        CellValue::Blob(vec![0x00, 0x01, 0x0f, 0x10, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]).display(),
        "00 01 0f 10 aa bb cc dd"
    );
}

#[test]
fn displayed_columns_ignores_unknown_and_duplicate_order_names() {
    let order = vec!["ghost".to_owned(), "name".to_owned(), "name".to_owned(), "id".to_owned()];
    assert_eq!(
        displayed_columns(&columns(), None, Some(&order)),
        vec!["name", "id"],
        "unknown/duplicate explicit names are ignored"
    );
}

#[test]
fn tip_without_a_deadline_never_expires() {
    let state = vm();
    assert!(!tip_expired(&state, 10_000.0), "no recorded deadline: never expires");
}

#[test]
fn quick_look_target_out_of_range_is_none() {
    let state = vm();
    assert_eq!(quick_look_target(&state, 7, "name"), None, "a missing row has no target");
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Mapping the driver subset back to shared operators made `a_declared_subset_is_used_verbatim`
//    fail on `left: ["eq","ne",…] right: ["eq","contains"]`.
// 2. Ignoring the guard made `invalid_editor_draft_blocks_navigation_reads`
//    fail on `table-view-navigation-blocked`.
