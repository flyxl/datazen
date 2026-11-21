//! T506: `QueryEditorSection` port — toolbar keys/actions, editor host and idle
//! hint, the Query Builder context rebind from `QueryEditorSection.qbContext.test.tsx`,
//! execution-strategy selection, and rect-based geometry at both sizes in both
//! themes.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::query::query_editor_section::{
    build, qb_context_key, qb_rebind_action, ExecutionStrategy, QueryEditorAction,
    QueryEditorSectionVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn vm() -> QueryEditorSectionVm {
    QueryEditorSectionVm {
        panel_id: "query-panel-1".to_owned(),
        db_session_id: "session-1".to_owned(),
        connection_id: "connection-1".to_owned(),
        sql: "SELECT 1".to_owned(),
        sql_edit: rune::TextEdit::new("SELECT 1".to_owned()),
        has_context_selectors: true,
        query_builder_available: true,
        selected_database: Some("app".to_owned()),
        selected_schema: Some("public".to_owned()),
        ..Default::default()
    }
}

#[derive(Clone)]
struct Scene {
    vm: QueryEditorSectionVm,
    actions: Rc<RefCell<Vec<QueryEditorAction>>>,
    dark: bool,
}

fn scene(vm: QueryEditorSectionVm, dark: bool) -> Scene {
    Scene { vm, actions: Rc::new(RefCell::new(Vec::new())), dark }
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let actions = s.actions.clone();
    let callback = Rc::new(move |state: &mut Scene, action| {
        match &action {
            QueryEditorAction::OpenVisualBuilder { context_key, .. } => {
                state.vm.qb_open_panel = Some(state.vm.panel_id.clone());
                state.vm.qb_bound_context_key = Some(context_key.clone());
            }
            QueryEditorAction::CloseVisualBuilder { .. } => state.vm.qb_open_panel = None,
            QueryEditorAction::SetExecutionStrategy(strategy) => state.vm.execution_strategy = *strategy,
            QueryEditorAction::ToggleExecutionStrategy => {
                state.vm.execution_strategy_open = !state.vm.execution_strategy_open
            }
            QueryEditorAction::ToggleNl2sql => state.vm.nl2sql_visible = !state.vm.nl2sql_visible,
            QueryEditorAction::ToggleHistory => state.vm.history_visible = !state.vm.history_visible,
            QueryEditorAction::ToggleFavorites => state.vm.favorites_visible = !state.vm.favorites_visible,
            _ => {}
        }
        actions.borrow_mut().push(action);
    });
    let edit: Rc<dyn for<'b> Fn(&'b mut Scene) -> &'b mut rune::TextEdit> =
        Rc::new(|state| &mut state.vm.sql_edit);
    let mounted = build(&s.vm, Some(edit), Some(callback), s.dark);
    col().w_full().h_full().child(mounted)
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

/// The host's per-rebuild hook: React's `useLayoutEffect` rebinds the builder.
fn pump_rebind(rt: &mut Runtime<Scene>) {
    if let Some(action) = qb_rebind_action(&rt.state.vm) {
        if let QueryEditorAction::OpenVisualBuilder { context_key, .. } = &action {
            rt.state.vm.qb_bound_context_key = Some(context_key.clone());
        }
        rt.state.actions.borrow_mut().push(action);
    }
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

// --- pure qb context ----------------------------------------------------------

#[test]
fn qb_context_key_is_the_json_tuple_and_matches_the_react_test() {
    assert_eq!(
        qb_context_key("connection-1", "session-1", Some("app"), Some("public")),
        r#"["connection-1","session-1","app","public"]"#
    );
    assert_eq!(
        qb_context_key("connection-1", "session-1", Some("analytics"), Some("private")),
        r#"["connection-1","session-1","analytics","private"]"#
    );
    assert_eq!(qb_context_key("", "s", None, None), r#"["","s","",""]"#);
}

// --- keys and toolbar actions -------------------------------------------------

#[test]
fn editor_shell_and_toolbar_keys_are_pinned() {
    let mut state = vm();
    state.show_idle_hint = true;
    state.supports_explain = true;
    let rt = runtime(scene(state, true), DESIGN);
    for key in [
        "query-editor-section",
        "query-editor-toolbar",
        "query-context-selectors",
        "editor-execute-button",
        "editor-execution-strategy-button",
        "editor-save-button",
        "editor-nl2sql-button",
        "query-toolbar-divider-main",
        "editor-format-button",
        "editor-explain-button",
        "editor-snippets-button",
        "editor-refresh-completion-button",
        "editor-visual-builder-button",
        "editor-begin-tx-button",
        "query-toolbar-spacer",
        "query-toolbar-shortcut",
        "editor-history-toggle",
        "editor-favorites-toggle",
        "query-editor-host",
        "query-editor-input",
        "query-idle-hint",
        "query-editor-resize-handle",
    ] {
        assert!(has_key(&rt, key), "{key} is mounted");
    }
    assert_eq!(element(&rt, "query-editor-toolbar").rect[3], 36.0, "toolbar keeps h-9");
    assert_eq!(element(&rt, "query-idle-hint").label.as_deref(), None);
    assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some(t("query.shortcutHint"))), "idle hint copy");
}

#[test]
fn toolbar_emits_execute_toggles_and_disabled_format_on_empty_sql() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    click(&mut rt, "editor-execute-button");
    assert_eq!(rt.state.actions.borrow().first(), Some(&QueryEditorAction::Execute));

    click(&mut rt, "editor-nl2sql-button");
    assert!(rt.state.actions.borrow().contains(&QueryEditorAction::ToggleNl2sql));
    click(&mut rt, "editor-history-toggle");
    assert!(rt.state.actions.borrow().contains(&QueryEditorAction::ToggleHistory));
    click(&mut rt, "editor-favorites-toggle");
    assert!(rt.state.actions.borrow().contains(&QueryEditorAction::ToggleFavorites));
    click(&mut rt, "editor-format-button");
    assert!(rt.state.actions.borrow().contains(&QueryEditorAction::Format), "non-empty sql enables format");
}

#[test]
fn empty_sql_disables_save_and_format() {
    let mut state = vm();
    state.sql.clear();
    let rt = runtime(scene(state, true), DESIGN);
    assert!(!element(&rt, "editor-save-button").focusable, "save disabled on empty sql");
    assert!(!element(&rt, "editor-format-button").focusable, "format disabled on empty sql");
}

#[test]
fn transaction_badge_switch_and_safe_mode_are_exact() {
    let mut state = vm();
    state.in_transaction = true;
    state.safe_mode = true;
    state.compact_toolbar = false;
    let mut rt = runtime(scene(state, true), DESIGN);
    assert!(has_key(&rt, "query-commit-tx"));
    assert!(has_key(&rt, "query-rollback-tx"));
    assert!(has_key(&rt, "query-in-transaction"));
    assert!(has_key(&rt, "query-safe-mode"));
    click(&mut rt, "query-commit-tx");
    assert!(rt.state.actions.borrow().contains(&QueryEditorAction::CommitTx));
    click(&mut rt, "query-rollback-tx");
    assert!(rt.state.actions.borrow().contains(&QueryEditorAction::RollbackTx));
}

#[test]
fn execution_strategy_dropdown_exposes_all_exact_locale_options() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    assert_eq!(
        element(&rt, "editor-execution-strategy-button").label.as_deref(),
        Some(t(ExecutionStrategy::CurrentStatement.label_key())),
        "the trigger carries the active strategy's locale key"
    );
    click(&mut rt, "editor-execution-strategy-button");
    assert!(has_key(&rt, "execution-strategy-menu"), "dropdown mounts");
    for strategy in ExecutionStrategy::ALL {
        assert!(has_key(&rt, &format!("execution-strategy-{}", strategy.label_key())), "exact option key");
    }
    assert!(
        rt.state.actions.borrow().contains(&QueryEditorAction::ToggleExecutionStrategy),
        "the strategy button emits its host intent"
    );
}

// --- qb context (ported React journey) ---------------------------------------

#[test]
fn qb_rebinds_when_the_query_tab_changes_database_and_schema() {
    let mut rt = runtime(scene(vm(), true), DESIGN);
    pump_rebind(&mut rt);
    assert!(!rt.state.actions.borrow().iter().any(|a| matches!(a, QueryEditorAction::OpenVisualBuilder { .. })), "closed builder does not rebind");

    click(&mut rt, "editor-visual-builder-button");
    let actions = rt.state.actions.borrow();
    let Some(QueryEditorAction::OpenVisualBuilder { panel_id, context_key }) = actions.first() else {
        panic!("the toolbar opens the builder: {actions:?}");
    };
    assert_eq!(panel_id, "query-panel-1");
    assert_eq!(context_key, r#"["connection-1","session-1","app","public"]"#);
    drop(actions);
    pump_rebind(&mut rt);
    assert!(has_key(&rt, "query-builder-host"), "the builder owns the editor area while open");

    // React's rerender with a different database/schema must rebind.
    rt.state.vm.selected_database = Some("analytics".to_owned());
    rt.state.vm.selected_schema = Some("private".to_owned());
    rt.invalidate();
    rt.frame(1.0 / 60.0);
    pump_rebind(&mut rt);
    let opens: Vec<QueryEditorAction> = rt
        .state
        .actions
        .borrow()
        .iter()
        .filter(|a| matches!(a, QueryEditorAction::OpenVisualBuilder { .. }))
        .cloned()
        .collect();
    assert_eq!(opens.len(), 2, "the open builder rebinds once on context change");
    assert_eq!(
        opens[1],
        QueryEditorAction::OpenVisualBuilder {
            panel_id: "query-panel-1".to_owned(),
            context_key: r#"["connection-1","session-1","analytics","private"]"#.to_owned(),
        },
        "the rebind carries the new context key"
    );
}

// --- geometry -----------------------------------------------------------------

#[test]
fn geometry_fits_both_sizes_in_both_themes() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut plain = vm();
            plain.show_idle_hint = true;
            plain.supports_explain = true;
            let rt = runtime(scene(plain, dark), size);
            assert_inside(&rt, size, &format!("plain {size:?} dark={dark}"));

            let mut busy = vm();
            busy.running = true;
            busy.in_transaction = true;
            busy.safe_mode = true;
            busy.qb_toast = Some(t("query.visualBuilder.appliedToast").to_owned());
            let rt = runtime(scene(busy, dark), size);
            assert_inside(&rt, size, &format!("busy {size:?} dark={dark}"));
            assert!(has_key(&rt, "qb-toast"), "toast mounted in busy state");
        }
    }
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
    assert!(outside.is_empty(), "{label}: outside {outside:?}");
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Hardcoding the qb context key to the initial database/schema made
//    `qb_rebinds_when_the_query_tab_changes_database_and_schema` fail with
//    `left: 1 right: 2` (no rebind after the context change).
// 2. Removing the `query-idle-hint` mount made `editor_shell_and_toolbar_keys_are_pinned`
//    fail on `query-idle-hint is mounted`.
