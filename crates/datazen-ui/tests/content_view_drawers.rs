//! T245: `ContentViewDrawers` port — drawer keys, split-pane source, automatic
//! chat context, result-readonly verdicts, KV slot and geometry at both window
//! sizes and both palettes. Pure derivations are pinned directly; rendered keys
//! and labels are read from the runtime element tree.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use serde_json::json;

use datazen_ui::views::content_view_drawers::{
    build, logic, DrawerAction, DrawerState, PanelKind,
};
use datazen_ui::i18n::t;

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

thread_local! {
    static FIXTURE: RefCell<(DrawerState, bool)> = RefCell::new((DrawerState::default(), true));
}

#[derive(Clone, Default)]
struct Scene {
    actions: Rc<RefCell<Vec<DrawerAction>>>,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    let (state, dark) = FIXTURE.with(|f| f.borrow().clone());
    let mut layer = Vec::new();
    let actions = scene.actions.clone();
    build::<Scene>(
        &state,
        Some(Rc::new(move |_s: &mut Scene, action: DrawerAction| {
            actions.borrow_mut().push(action);
        })),
        dark,
        &mut layer,
    );
    col().w_full().h_full().children(layer)
}

fn runtime(state: DrawerState, dark: bool, size: (f32, f32)) -> Runtime<Scene> {
    FIXTURE.with(|f| *f.borrow_mut() = (state, dark));
    let mut rt = Runtime::new(Scene::default(), view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).label.unwrap_or_default()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn assert_inside<S>(rt: &Runtime<S>, size: (f32, f32), label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > size.0 + 0.5 || r[1] + r[3] > size.1 + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(outside.is_empty(), "{label}: {} node(s) outside {size:?}: {outside:?}", outside.len());
}

fn query_state() -> DrawerState {
    DrawerState {
        panel_id: "panel-query-1".to_owned(),
        panel_kind: PanelKind::Query,
        has_db_session: true,
        detail_panel_applicable: true,
        ..DrawerState::default()
    }
}

fn result(sql: &str, id: i64) -> logic::StatementResult {
    logic::StatementResult {
        sql: sql.to_owned(),
        columns: vec![logic::ColumnMeta { name: "id".to_owned(), data_type: "INTEGER".to_owned() }],
        rows: vec![vec![json!(id)]],
        execution_time_ms: Some(1),
        source: Some(logic::ResultSource { writable_mapping_verified: false }),
        has_runtime_binding: true,
    }
}

// --- 1. drawer open/close keys ----------------------------------------------

#[test]
fn only_the_requested_drawers_mount_with_their_pinned_close_keys() {
    let mut state = DrawerState { detail_open: true, ..query_state() };
    let rt = runtime(state.clone(), true, DESIGN);
    assert!(has_key(&rt, "content-view-drawers"));
    assert!(has_key(&rt, "content-view-detail"));
    assert!(has_key(&rt, "content-view-detail-close"));
    assert!(!has_key(&rt, "content-view-ai"));

    state.ai_chat_open = true;
    let rt = runtime(state.clone(), true, DESIGN);
    assert!(has_key(&rt, "content-view-detail"));
    assert!(has_key(&rt, "content-view-ai"));
    assert!(has_key(&rt, "content-view-ai-close"));

    // detailOpen without an applicable panel mounts no drawer.
    let mut state = query_state();
    state.detail_open = true;
    state.detail_panel_applicable = false;
    let rt = runtime(state, true, DESIGN);
    assert!(!has_key(&rt, "content-view-detail"));

    // AI needs a db session id.
    let mut state = query_state();
    state.ai_chat_open = true;
    state.has_db_session = false;
    let rt = runtime(state, true, DESIGN);
    assert!(!has_key(&rt, "content-view-ai"));
}

#[test]
fn close_presses_emit_the_host_actions() {
    let mut state = DrawerState { detail_open: true, ai_chat_open: true, ..query_state() };
    state.table_columns.push(logic::ColumnMeta { name: "id".to_owned(), data_type: "INTEGER".to_owned() });
    state.table_rows.push(vec![json!(1)]);
    state.table_detail_row_index = Some(0);
    state.panel_kind = PanelKind::Table;
    let mut rt = runtime(state, true, DESIGN);

    click(&mut rt, "content-view-detail-close");
    click(&mut rt, "content-view-ai-close");
    let actions = rt.state.actions.borrow().clone();
    assert_eq!(actions, vec![DrawerAction::CloseDetail, DrawerAction::CloseAi]);
}

// --- 2. split-pane source ----------------------------------------------------

#[test]
fn the_detail_row_comes_from_the_focused_pane_not_the_default() {
    let mut state = query_state();
    state.detail_open = true;
    state.focused_pane = Some("b".to_owned());
    state.panes.insert("panel-query-1".to_owned(), logic::QueryExec {
        results: vec![result("SELECT id FROM default_pane", 1)],
        active_result_idx: 0,
        result_detail_row_index: Some(0),
        ..Default::default()
    });
    state.panes.insert(logic::pane_key("panel-query-1", "b"), logic::QueryExec {
        results: vec![result("SELECT id FROM focused_pane", 42)],
        active_result_idx: 0,
        result_detail_row_index: Some(0),
        ..Default::default()
    });

    let rt = runtime(state, true, DESIGN);
    let shown = text_of(&rt, "conn-detail-row-0-value");
    assert_eq!(shown, "42", "the drawer shows the focused pane's row");
    assert_ne!(shown, "1", "the default pane's row must not leak in");

    // The pure resolver agrees and falls back to the bare panel id.
    let mut panes: HashMap<String, logic::QueryExec> = HashMap::new();
    panes.insert("panel-query-1".to_owned(), logic::QueryExec::default());
    assert!(logic::resolve_active_exec("panel-query-1", None, &panes).is_some());
    assert_eq!(logic::resolve_focused_pane_id(None), logic::DEFAULT_PANE_ID);
    assert_eq!(logic::pane_key("p", "main"), "p");
    assert_eq!(logic::pane_key("p", "b"), "p::b");
}

// --- 3. auto-context ---------------------------------------------------------

#[test]
fn auto_context_caps_rows_columns_and_cells_and_trims_text() {
    let mut state = query_state();
    state.ai_chat_open = true;
    state.editor_selection = "  WHERE id = 1  ".to_owned();
    let mut exec = logic::QueryExec {
        sql: "SELECT id FROM orders".to_owned(),
        results: vec![logic::StatementResult {
            columns: (0..25)
                .map(|i| logic::ColumnMeta { name: format!("c{i}"), data_type: "TEXT".to_owned() })
                .collect(),
            rows: (0..7)
                .map(|r| (0..25).map(|c| json!(format!("v{r}-{c}"))).collect())
                .collect(),
            execution_time_ms: Some(9),
            ..Default::default()
        }],
        error: "syntax error near FROM".to_owned(),
        execution_time_ms: Some(9),
        ..Default::default()
    };
    let _ = &mut exec;
    state.panes.insert("panel-query-1".to_owned(), exec);
    let rt = runtime(state, true, DESIGN);

    assert_eq!(text_of(&rt, "conn-ai-auto-sql"), "SELECT id FROM orders");
    assert_eq!(text_of(&rt, "conn-ai-auto-selection"), "WHERE id = 1");
    assert_eq!(text_of(&rt, "conn-ai-auto-error"), "syntax error near FROM");
    assert_eq!(text_of(&rt, "conn-ai-auto-row-count"), "7", "rowCount is the full count");
    assert_eq!(text_of(&rt, "conn-ai-auto-elapsed"), "9");
    assert_eq!(text_of(&rt, "conn-ai-auto-row-0").split(" | ").count(), 20, "columns capped at 20");
    for i in 0..5 {
        assert!(has_key(&rt, &format!("conn-ai-auto-row-{i}")), "row {i} present");
    }
    assert!(!has_key(&rt, "conn-ai-auto-row-5"), "rows capped at 5");

    // Empty everything yields no auto-context at all.
    assert_eq!(
        logic::build_auto_context(PanelKind::Query, Some(&logic::QueryExec::default()), "  "),
        None
    );
}

#[test]
fn a_long_string_cell_is_cut_to_500_chars() {
    let long = "x".repeat(700);
    let cut = logic::preview_cell(&json!(long));
    assert_eq!(cut.as_str().map(str::len), Some(500));
    assert_eq!(logic::preview_cell(&json!(42)), json!(42));
    assert_eq!(logic::preview_cell(&json!(null)), json!(null));
}

// --- 4. result-readonly verdicts --------------------------------------------

#[test]
fn unjudged_result_is_read_only_with_unknown_origin() {
    let mut state = query_state();
    state.detail_open = true;
    state.panes.insert("panel-query-1".to_owned(), logic::QueryExec {
        results: vec![result("SELECT id FROM t", 1)],
        active_result_idx: 0,
        result_detail_row_index: Some(0),
        ..Default::default()
    });
    let rt = runtime(state, true, DESIGN);
    assert_eq!(label_of(&rt, "conn-detail-readonly-reason"), "unknownOrigin");
    assert_eq!(text_of(&rt, "conn-detail-readonly-reason-text"), t("query.gridReadOnly.unknownOrigin"));
    assert_eq!(label_of(&rt, "conn-detail-editable"), "false");
}

#[test]
fn a_refusing_verdict_keeps_its_reason_and_blocks_the_write() {
    let verdict = logic::ServerEditability {
        writable: false,
        reason: Some(logic::ReadOnlyReason::MultiTable),
        ..Default::default()
    };
    // A session-read-only refusal outranks even a verified mapping.
    let session_read_only = logic::ServerEditability {
        writable: false,
        reason: Some(logic::ReadOnlyReason::SessionReadOnly),
        ..Default::default()
    };
    assert!(!logic::resolve_result_write_back(true, &session_read_only));
    // With the mapping unverified (the default), the multi-table verdict is
    // what refuses the write-back.
    let refusing = result("SELECT id FROM t", 1);
    assert_eq!(
        logic::query_read_only_reason(
            PanelKind::Query,
            Some(&refusing),
            &verdict,
            Some(0),
        ),
        Some(logic::ReadOnlyReason::MultiTable)
    );
    assert!(!logic::query_result_editable(PanelKind::Query, Some(&refusing), &verdict));

    // A verified mapping on a writable verdict opens the write-back.
    let mut granted = logic::ServerEditability { writable: true, ..Default::default() };
    granted.identity_columns.push("id".to_owned());
    let mut verified = result("SELECT id FROM t", 1);
    verified.source = Some(logic::ResultSource { writable_mapping_verified: true });
    assert!(logic::query_result_editable(PanelKind::Query, Some(&verified), &granted));
    assert_eq!(
        logic::query_read_only_reason(PanelKind::Query, Some(&verified), &granted, Some(0)),
        None
    );
}

#[test]
fn a_null_key_or_duplicate_identity_blocks_an_otherwise_writable_row() {
    let columns = vec![logic::ColumnMeta { name: "id".to_owned(), data_type: "INTEGER".to_owned() }];
    let identities = vec!["id".to_owned()];
    let null_row = vec![json!(null)];
    assert_eq!(
        logic::resolve_row_addressability(&identities, &[null_row], &columns, 0),
        Some(logic::ReadOnlyReason::KeyValueNull)
    );
    let dupes = [vec![json!(1)], vec![json!(1)]];
    assert_eq!(
        logic::resolve_row_addressability(&identities, &dupes, &columns, 0),
        Some(logic::ReadOnlyReason::DuplicateIdentity)
    );
    assert_eq!(
        logic::resolve_row_addressability(&identities, &dupes, &columns, 5),
        Some(logic::ReadOnlyReason::RowNotFound)
    );
    // No identity columns: the narrow path re-verifies, so no refusal.
    assert_eq!(logic::resolve_row_addressability(&[], &dupes, &columns, 0), None);
}

// --- 5. KV key-props sidebar slot -------------------------------------------

#[test]
fn the_key_props_sidebar_replaces_the_row_detail_when_present() {
    let state = DrawerState {
        detail_open: true,
        key_props_slot: true,
        ..query_state()
    };
    let rt = runtime(state, true, DESIGN);
    assert!(has_key(&rt, "conn-kv-key-props-sidebar"));
    assert_eq!(label_of(&rt, "conn-kv-key-props-sidebar"), "kv-key-props-sidebar");
    assert!(!has_key(&rt, "conn-detail"), "the empty row grid is replaced");
    click_rt_close_sidebar(&rt);
}

fn click_rt_close_sidebar(rt: &Runtime<Scene>) {
    assert!(rt.find("conn-kv-key-props-sidebar").is_some(), "sidebar is pressable");
}

// --- 6. geometry -------------------------------------------------------------

#[test]
fn drawers_fit_both_window_sizes_and_both_palettes() {
    let mut state = query_state();
    state.detail_open = true;
    state.ai_chat_open = true;
    state.table_columns.push(logic::ColumnMeta { name: "id".to_owned(), data_type: "INTEGER".to_owned() });
    state.table_rows.push(vec![json!(1)]);
    state.table_detail_row_index = Some(0);
    state.panes.insert("panel-query-1".to_owned(), logic::QueryExec {
        results: vec![result("SELECT id FROM t", 1)],
        active_result_idx: 0,
        result_detail_row_index: Some(0),
        ..Default::default()
    });

    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(state.clone(), dark, size);
            assert!(has_key(&rt, "content-view-drawers"), "{size:?} dark={dark}: root mounted");
            assert_inside(&rt, size, &format!("{size:?} dark={dark}"));
        }
    }
}

// --- D2/D3/D4 regression cases ----------------------------------------------

#[test]
fn react_mounts_the_sidebar_wrapper_even_while_the_drawer_is_closed() {
    let state = DrawerState {
        detail_open: false,
        detail_panel_applicable: true,
        key_props_slot: true,
        ..query_state()
    };
    let rt = runtime(state, true, DESIGN);
    assert!(has_key(&rt, "conn-kv-key-props-sidebar"), "React mounts the sidebar wrapper even while the drawer is closed");
    assert!(!has_key(&rt, "conn-detail"), "the collapsed drawer shows no row-detail table");
}

#[test]
fn a_missing_exec_must_show_no_row() {
    let mut panes: HashMap<String, logic::QueryExec> = HashMap::new();
    // Only the default pane has an exec; the focused secondary pane does not.
    panes.insert("panel-query-1".to_owned(), logic::QueryExec {
        results: vec![result("SELECT id FROM default_pane", 1)],
        active_result_idx: 0,
        result_detail_row_index: Some(0),
        ..Default::default()
    });
    assert!(logic::resolve_active_exec("panel-query-1", Some("b"), &panes).is_none());
    let mut state = query_state();
    state.detail_open = true;
    state.focused_pane = Some("b".to_owned());
    state.panes = panes;
    let rt = runtime(state, true, DESIGN);
    assert!(!has_key(&rt, "conn-detail-row-0-value"), "a missing exec must show no row");
}

#[test]
fn a_missing_identity_column_is_row_not_found() {
    let columns = vec![logic::ColumnMeta { name: "other".to_owned(), data_type: "INTEGER".to_owned() }];
    let identities = vec!["id".to_owned()];
    let rows = [vec![json!(1)]];
    assert_eq!(
        logic::resolve_row_addressability(&identities, &rows, &columns, 0),
        Some(logic::ReadOnlyReason::RowNotFound)
    );
}
