//! T524: `ResultTableView` port — status bar/headers/cells, selection, sorting,
//! staged edits and find actions, mutation/empty branches, and rect geometry at
//! both window sizes in both palettes.

use std::cell::RefCell;
use std::rc::Rc;
use rune_kit::SortDir;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::result_workspace::result_table_view::{
    build, can_edit, filter_rows, grid_read_only_message_key, is_mutation_execution,
    plan_replacements, read_only_reason, replace_text, view_rows, EditColumn, FindOperator, FindQuery, GridValue, PendingEdit,
    ResultColumn, ResultTableAction, ResultTableVm, ServerEditability,
};

fn vm() -> ResultTableVm {
    ResultTableVm {
        sql: "select id, name from users".to_owned(),
        columns: vec![
            ResultColumn { name: "id".to_owned(), data_type: "int4".to_owned() },
            ResultColumn { name: "name".to_owned(), data_type: "text".to_owned() },
        ],
        rows: vec![
            vec![GridValue::Number("1".to_owned()), GridValue::Text("alpha".to_owned())],
            vec![GridValue::Null, GridValue::Json("{\"a\":1}".to_owned())],
        ],
        execution_time_ms: 2,
        query_result_limit: 5000,
        ..Default::default()
    }
}

fn writable(vm: &mut ResultTableVm) {
    vm.source_writable_mapping_verified = true;
    vm.has_runtime_binding = true;
    vm.can_commit = true;
    vm.server_verdict = Some(ServerEditability {
        writable: true,
        identity_columns: vec![EditColumn { column: "id".to_owned(), source_column: "id".to_owned() }],
        editable_columns: vec![EditColumn { column: "name".to_owned(), source_column: "name".to_owned() }],
        ..Default::default()
    });
}

#[derive(Clone)]
struct Scene {
    vm: ResultTableVm,
    actions: Rc<RefCell<Vec<ResultTableAction>>>,
    dark: bool,
}

fn scene(vm: ResultTableVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match &action {
            ResultTableAction::ToggleFind => state.vm.find_open = !state.vm.find_open,
            ResultTableAction::CloseFind => {
                state.vm.find_open = false;
                state.vm.find_query = FindQuery::empty();
            }
            ResultTableAction::SelectRow(row) => state.vm.row_detail_index = Some(*row),
            ResultTableAction::ToggleSort(column) => {
                state.vm.sort = match state.vm.sort {
                    Some((current, SortDir::Ascending)) if current == *column => Some((*column, SortDir::Descending)),
                    _ => Some((*column, SortDir::Ascending)),
                };
            }
            ResultTableAction::StageEdit(edit) => {
                state.vm.pending_edits.retain(|item| item.row != edit.row || item.column != edit.column);
                state.vm.pending_edits.push(edit.clone());
            }
            ResultTableAction::RollbackEdits => state.vm.pending_edits.clear(),
            ResultTableAction::CommitEdits => {
                state.vm.pending_edits.clear();
                state.vm.commit_notice = Some(t("query.resultEditCommitted").to_owned());
            }
            ResultTableAction::OpenCell { row, column } => state.vm.editing_cell = Some((*row, column.clone())),
            _ => {}
        }
        actions.borrow_mut().push(action);
    });
    let mounted = build(&s.vm, Some(callback), s.dark);
    col().w_full().h_full().child(mounted)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn status_bar_and_grid_keys_and_cell_values() {
    let rt = runtime(scene(vm(), true), (1440.0, 816.0));
    for key in [
        "result-workspace-table", "result-status-bar", "result-find-toggle", "result-grid",
        "result-grid-head", "result-grid-col-0", "result-grid-body", "result-grid-row-0",
        "result-grid-cell-0-0-text", "result-grid-cell-1-0-text", "result-sort-strip", "result-sort-0",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(element(&rt, "result-grid-cell-0-0-text").text.as_deref(), Some("1"));
    assert_eq!(element(&rt, "result-grid-cell-1-0-text").text.as_deref(), Some("NULL"), "null cell");
    assert_eq!(element(&rt, "result-grid-cell-1-1-text").text.as_deref(), Some("{\"a\":1}"), "json cell");
    assert_eq!(element(&rt, "result-find-toggle").label.as_deref(), Some(t("resultFind.openLabel")));
    let texts: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    assert!(texts.iter().any(|x| x == &format!("2 {}", t("common.rows"))));
    assert!(texts.iter().any(|x| x == &format!("2 {}", t("common.columns"))));
}

#[test]
fn cell_display_covers_blob_and_booleans() {
    assert_eq!(GridValue::Null.display(), "NULL");
    assert_eq!(GridValue::Bool(true).display(), "true");
    assert_eq!(GridValue::Blob(vec![0xde, 0xad]).display(), "de ad");
    assert_eq!(GridValue::Number("42".to_owned()).display(), "42");
}

#[test]
fn row_selection_routes_the_original_index() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "result-grid-row-1");
    assert_eq!(rt.state.vm.row_detail_index, Some(1), "row detail gets the original index");
    let rt = runtime(scene(vm(), true), (1440.0, 816.0));
    assert_eq!(element(&rt, "result-grid").role, "grid");
}

#[test]
fn sorting_emits_the_host_intent_and_applies_to_the_grid() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "result-sort-0");
    assert!(rt.state.actions.borrow().iter().any(|a| matches!(a, ResultTableAction::ToggleSort(0))));
    assert_eq!(rt.state.vm.sort, Some((0, SortDir::Ascending)));
    click(&mut rt, "result-sort-0");
    assert_eq!(rt.state.vm.sort, Some((0, SortDir::Descending)));
}

#[test]
fn find_toggle_filters_rows_and_reports_no_matches() {
    let mut rt = runtime(scene(vm(), true), (1440.0, 816.0));
    click(&mut rt, "result-find-toggle");
    assert!(rt.state.vm.find_open);
    assert!(has_key(&rt, "result-find-bar"));

    rt.state.vm.find_query = FindQuery { needle: "alpha".to_owned(), operator: FindOperator::Contains, ..Default::default() };
    rt.invalidate();
    rt.frame(0.0);
    let (rows, indexes) = view_rows(&rt.state.vm);
    assert_eq!(indexes, vec![0]);
    assert_eq!(rows.len(), 1);
    click(&mut rt, "result-find-close");
    assert!(!rt.state.vm.find_open);
}

#[test]
fn filter_rows_honors_column_and_case() {
    let rows = vec![vec![GridValue::Text("Alpha".to_owned())], vec![GridValue::Text("beta".to_owned())]];
    let columns = vec!["name".to_owned()];
    let query = FindQuery { needle: "Alpha".to_owned(), match_case: true, ..Default::default() };
    assert_eq!(filter_rows(&rows, &columns, &query), vec![0]);
    let wrong_case = FindQuery { needle: "alpha".to_owned(), match_case: true, ..Default::default() };
    assert_eq!(filter_rows(&rows, &columns, &wrong_case), Vec::<usize>::new());
    let insensitive = FindQuery { needle: "alpha".to_owned(), match_case: false, ..Default::default() };
    assert_eq!(filter_rows(&rows, &columns, &insensitive), vec![0]);
}

#[test]
fn staged_edits_need_an_explicit_commit() {
    let mut state = vm();
    writable(&mut state);
    let mut rt = runtime(scene(state, true), (1440.0, 816.0));
    assert!(can_edit(&rt.state.vm));
    rt.state.vm.pending_edits.push(PendingEdit {
        row: 0,
        column: "name".to_owned(),
        original: GridValue::Text("alpha".to_owned()),
        value: GridValue::Text("beta".to_owned()),
    });
    for _ in 0..4 { rt.invalidate(); rt.frame(1.0 / 60.0); }
    assert!(has_key(&rt, "pending-changes-bar"));
    click(&mut rt, "pending-commit");
    assert_eq!(rt.state.vm.commit_notice.as_deref(), Some(t("query.resultEditCommitted")));
    assert!(rt.state.vm.pending_edits.is_empty());
}

#[test]
fn mutation_execution_renders_the_summary_instead_of_the_grid() {
    let mut state = vm();
    state.columns.clear();
    state.rows.clear();
    state.sql = "INSERT INTO users (name) VALUES ('Alice')".to_owned();
    state.rows_affected = Some(1);
    assert!(is_mutation_execution(&state));
    let rt = runtime(scene(state, true), (1440.0, 816.0));
    assert!(has_key(&rt, "execution-summary-card"));
    assert!(!has_key(&rt, "result-grid"), "mutation has no tabular grid");
}

#[test]
fn empty_result_and_read_only_reasons_are_explicit() {
    let mut state = vm();
    state.rows.clear();
    let rt = runtime(scene(state, true), (1440.0, 816.0));
    assert!(has_key(&rt, "result-empty-status"));
    assert!(!has_key(&rt, "result-find-no-matches"));

    let mut state = vm();
    state.source_writable_mapping_verified = true;
    state.has_runtime_binding = false;
    assert_eq!(read_only_reason(&state), Some("unknownOrigin".to_owned()));
    assert_eq!(grid_read_only_message_key("multiTable"), "query.gridReadOnly.multiTable");
    let rt = runtime(scene(state, true), (1440.0, 816.0));
    assert!(has_key(&rt, "result-data-dt-grid-readonly-reason-unknownOrigin"));
}

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut state = vm();
            writable(&mut state);
            state.find_open = true;
            state.pending_edits.push(PendingEdit {
                row: 0, column: "name".to_owned(),
                original: GridValue::Text("alpha".to_owned()), value: GridValue::Text("beta".to_owned()),
            });
            let rt = runtime(scene(state, dark), size);
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

// --- regression pins for the React-parity find/replace fixes -----------------

#[test]
fn any_column_not_contains_keeps_only_rows_with_no_matching_cell() {
    let rows = vec![
        vec![GridValue::Text("acme corp".to_owned()), GridValue::Text("other".to_owned())],
        vec![GridValue::Text("beta".to_owned()), GridValue::Text("gamma".to_owned())],
        vec![GridValue::Text("delta".to_owned()), GridValue::Null],
    ];
    let columns = vec!["name".to_owned(), "note".to_owned()];
    let query = FindQuery {
        needle: "acme".to_owned(),
        operator: FindOperator::NotContains,
        ..Default::default()
    };
    assert_eq!(filter_rows(&rows, &columns, &query), vec![1, 2], "the row must hold no matching cell");
}

#[test]
fn replace_text_splices_the_needle_in_place() {
    let case = FindQuery { needle: "acme".to_owned(), match_case: true, ..Default::default() };
    assert_eq!(replace_text("acme corp", &case, "acorp"), "acorp corp");
    let insensitive = FindQuery { needle: "acme".to_owned(), match_case: false, ..Default::default() };
    assert_eq!(replace_text("Acme acme", &insensitive, "X"), "X X", "every occurrence, folded for search");
    let columns = vec!["name".to_owned(), "email".to_owned()];
    let rows = vec![vec![GridValue::Text("acme corp".to_owned()), GridValue::Text("a@acme.io".to_owned())]];
    let query = FindQuery { needle: "acme".to_owned(), ..Default::default() };
    assert_eq!(
        plan_replacements(&rows, &columns, &query, "acorp"),
        vec![(0, "name".to_owned(), "acorp corp".to_owned()), (0, "email".to_owned(), "a@acorp.io".to_owned())]
    );
}

#[test]
fn prefix_only_plans_genuinely_matching_cells() {
    let columns = vec!["a".to_owned(), "b".to_owned()];
    let rows = vec![vec![GridValue::Text("zabc".to_owned()), GridValue::Text("abcdef".to_owned())]];
    let query = FindQuery { needle: "abc".to_owned(), operator: FindOperator::Prefix, ..Default::default() };
    assert_eq!(
        plan_replacements(&rows, &columns, &query, "Z"),
        vec![(0, "b".to_owned(), "Zdef".to_owned())],
        "only a true prefix match is planned and spliced"
    );
    let suffix = FindQuery { needle: "abc".to_owned(), operator: FindOperator::Suffix, ..Default::default() };
    assert_eq!(plan_replacements(&rows, &columns, &suffix, "Z"), vec![(0, "a".to_owned(), "zZ".to_owned())]);
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Dropping the null cell marker made `status_bar_and_grid_keys_and_cell_values`
//    fail on `result-grid-cell-1-0-text`.
// 2. Making the mutation branch always emit the grid made `mutation_execution_renders_the_summary_instead_of_the_grid`
//    fail on `mutation has no tabular grid`.
