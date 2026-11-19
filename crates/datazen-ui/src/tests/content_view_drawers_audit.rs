//! T245 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 10 tests do not pin: reason wire-word
//! parsing, sidebar width bounds, auto-context SQL fallback and non-query
//! panels, object preview cells, a rendered table-panel detail row, the
//! resolving banner text, and a non-vacuous `visible`-rect geometry check.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};
use serde_json::json;

use crate::i18n::t;
use crate::views::content_view_drawers::{
    build, logic, DrawerAction, DrawerState, PanelKind,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

#[derive(Clone, Default)]
struct AuditScene {
    state: DrawerState,
    actions: Rc<RefCell<Vec<DrawerAction>>>,
    dark: bool,
}

fn view(s: &AuditScene, _cx: &mut Cx<AuditScene>) -> El {
    let mut layer = Vec::new();
    let actions = s.actions.clone();
    build::<AuditScene>(
        &s.state,
        Some(Rc::new(move |_st: &mut AuditScene, action: DrawerAction| {
            actions.borrow_mut().push(action);
        })),
        s.dark,
        &mut layer,
    );
    col().w_full().h_full().children(layer)
}

fn runtime(state: DrawerState, dark: bool, size: (f32, f32)) -> Runtime<AuditScene> {
    let mut rt = Runtime::new(
        AuditScene { state, actions: Rc::new(RefCell::new(Vec::new())), dark },
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).label.unwrap_or_default()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    element(rt, key).text.unwrap_or_default()
}

fn query_state() -> DrawerState {
    DrawerState {
        panel_id: "p".to_owned(),
        panel_kind: PanelKind::Query,
        has_db_session: true,
        detail_panel_applicable: true,
        ..DrawerState::default()
    }
}

#[test]
fn parse_reason_accepts_both_wire_cases_and_falls_back_to_unknown() {
    use logic::ReadOnlyReason as R;
    for (raw, expected) in [
        ("unknownOrigin", R::UnknownOrigin),
        ("ambiguous_origin", R::AmbiguousOrigin),
        ("multiTable", R::MultiTable),
        ("no_primary_key", R::NoPrimaryKey),
        ("keyValueNull", R::KeyValueNull),
        ("row_not_found", R::RowNotFound),
        ("somethingNew", R::UnknownOrigin),
    ] {
        assert_eq!(logic::parse_reason(Some(raw)), Some(expected), "reason {raw}");
    }
    assert_eq!(logic::parse_reason(None), None);
    assert_eq!(R::MultiTable.message_key(), "query.gridReadOnly.multiTable");
}

#[test]
fn sidebar_width_is_clamped_to_the_react_bounds() {
    assert_eq!(logic::clamp_sidebar_width(100.0), 240.0);
    assert_eq!(logic::clamp_sidebar_width(800.0), 600.0);
    assert_eq!(logic::clamp_sidebar_width(320.0), 320.0);
}

#[test]
fn auto_context_falls_back_to_result_sql_and_ignores_non_query_panels() {
    let exec = logic::QueryExec {
        sql: "   ".to_owned(),
        results: vec![logic::StatementResult { sql: "SELECT 1".to_owned(), ..Default::default() }],
        ..Default::default()
    };
    let ctx = logic::build_auto_context(PanelKind::Query, Some(&exec), "  ")
        .expect("a result SQL fact makes the context non-empty");
    assert_eq!(ctx.editor_sql.as_deref(), Some("SELECT 1"));
    assert_eq!(
        logic::build_auto_context(PanelKind::Table, Some(&exec), "x"),
        None,
        "a table panel never gets query auto-context"
    );
    assert_eq!(logic::build_auto_context(PanelKind::Query, None, "x"), None);
}

#[test]
fn a_complex_preview_cell_is_stringified_and_capped() {
    let value = json!({"k": "x".repeat(700)});
    let cut = logic::preview_cell(&value);
    assert_eq!(cut.as_str().map(str::len), Some(500), "objects become a 500-char JSON string");
    assert!(cut.as_str().is_some_and(|s| s.starts_with('{')));
}

#[test]
fn a_table_panel_detail_row_renders_the_grid_row_values() {
    let mut state = query_state();
    state.panel_kind = PanelKind::Table;
    state.detail_open = true;
    state.table_columns.push(logic::ColumnMeta { name: "id".to_owned(), data_type: "INTEGER".to_owned() });
    state.table_rows.push(vec![json!(7)]);
    state.table_detail_row_index = Some(0);
    let rt = runtime(state, true, DESIGN);
    assert_eq!(text_of(&rt, "conn-detail-row-0-value"), "7");
    assert_eq!(label_of(&rt, "conn-detail-row-0"), "id");
    assert_eq!(label_of(&rt, "conn-detail-editable"), "true", "non-query panels stay editable");
}

#[test]
fn a_resolving_verdict_swaps_the_banner_text() {
    let mut state = query_state();
    state.detail_open = true;
    state.resolving_origin = true;
    state.verdict = Some(logic::ServerEditability {
        writable: false,
        reason: Some(logic::ReadOnlyReason::MultiTable),
        ..Default::default()
    });
    let rt = runtime(state, true, DESIGN);
    assert_eq!(label_of(&rt, "conn-detail-readonly-reason"), "multiTable");
    assert_eq!(text_of(&rt, "conn-detail-readonly-reason-text"), t("query.gridEdit.resolving"));
}

#[test]
fn long_drawer_values_stay_inside_both_windows_and_palettes_via_visible_rects() {
    let long = "x".repeat(300);
    let mut state = query_state();
    state.detail_open = true;
    state.ai_chat_open = true;
    state.editor_selection = long.clone();
    state.panes.insert(
        "p".to_owned(),
        logic::QueryExec {
            sql: long.clone(),
            error: long.clone(),
            results: vec![logic::StatementResult {
                sql: long.clone(),
                columns: vec![logic::ColumnMeta { name: long.clone(), data_type: "TEXT".to_owned() }],
                rows: vec![vec![json!(long.clone())]],
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let rt = runtime(state.clone(), dark, size);
            assert!(
                rt.elements().iter().any(|e| e.key.as_deref() == Some("content-view-drawers")),
                "{size:?} dark={dark}: root must be laid out"
            );
            for e in rt.elements() {
                let [x, y, w, h] = e.visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "{size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                    e.key.as_deref().unwrap_or(&e.path),
                    e.visible,
                    e.rect
                );
            }
        }
    }
}
