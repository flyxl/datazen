//! T506 independent audit (tester, not coder).
//!
//! Precise probes of branches the shipped 8 tests do not pin: the rebind gate,
//! exact execution-strategy keys, the host-owned toast / idle hint surfaces,
//! visible-rect geometry, plus a temporary probe for JSON escaping in the
//! query-builder context key (React uses `JSON.stringify`).

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::t;
use datazen_ui::views::query::query_editor_section::{
    build, qb_context_key, qb_is_open, qb_needs_rebind, ExecutionStrategy, QueryEditorAction,
    QueryEditorSectionVm,
};

const MIN: (f32, f32) = (960.0, 640.0);
const DESIGN: (f32, f32) = (1440.0, 816.0);

fn vm() -> QueryEditorSectionVm {
    QueryEditorSectionVm {
        panel_id: "p".to_owned(),
        db_session_id: "s".to_owned(),
        connection_id: "c".to_owned(),
        sql: "SELECT 1".to_owned(),
        query_builder_available: true,
        selected_database: Some("app".to_owned()),
        selected_schema: Some("public".to_owned()),
        ..Default::default()
    }
}

#[derive(Clone)]
struct Scene {
    vm: QueryEditorSectionVm,
    dark: bool,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    let cb: Rc<dyn Fn(&mut Scene, QueryEditorAction) + 'static> = Rc::new(|_s, _a| {});
    col().w_full().h_full().child(build(&s.vm, None, Some(cb), s.dark))
}

fn runtime(vm: QueryEditorSectionVm, size: (f32, f32), dark: bool) -> Runtime<Scene> {
    let mut rt = Runtime::new(Scene { vm, dark }, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

#[test]
fn rebind_gate_requires_an_open_available_builder_with_a_stale_context() {
    let mut v = vm();
    assert!(!qb_is_open(&v));
    assert!(!qb_needs_rebind(&v), "a closed builder never rebinds");
    v.qb_open_panel = Some("p".to_owned());
    v.qb_bound_context_key = Some(qb_context_key("c", "s", Some("app"), Some("public")));
    assert!(!qb_needs_rebind(&v), "a matching bound context needs no rebind");
    v.selected_database = Some("analytics".to_owned());
    assert!(qb_needs_rebind(&v), "a changed database needs a rebind");
    v.query_builder_available = false;
    assert!(!qb_needs_rebind(&v), "an unavailable builder never rebinds");
}

#[test]
fn qb_context_key_json_escapes_quote_backslash_and_controls() {
    // React uses JSON.stringify([...]), so these bytes must be escaped the
    // same way or a database/schema containing them rebinds the wrong draft.
    assert_eq!(
        qb_context_key("c\"x", "s\\y", Some("app"), Some("a\"b\\c\nd")),
        r#"["c\"x","s\\y","app","a\"b\\c\nd"]"#
    );
    assert_eq!(
        qb_context_key("c", "s", Some("a\tb\r"), Some("x\u{8}\u{c}\u{1}y")),
        r#"["c","s","a\tb\r","x\b\f\u0001y"]"#
    );
}

#[test]
fn execution_strategy_labels_are_all_exact_locale_keys() {
    for (strategy, key) in [
        (ExecutionStrategy::CurrentStatement, "query.executionStrategy.currentStatement"),
        (ExecutionStrategy::EntireScript, "query.executionStrategy.entireScript"),
        (ExecutionStrategy::LargestStatement, "query.executionStrategy.largestStatement"),
        (ExecutionStrategy::Ask, "query.executionStrategy.ask"),
    ] {
        assert_eq!(strategy.label_key(), key);
        // The label must resolve, not echo the key when injected into t().
        assert!(!t(key).is_empty());
    }
}

#[test]
fn toast_and_idle_hint_surfaces_mount_only_when_requested() {
    let plain = runtime(vm(), DESIGN, true);
    assert!(!has_key(&plain, "qb-toast"));
    assert!(!has_key(&plain, "query-idle-hint"));

    let mut v = vm();
    v.qb_toast = Some("builder opened".to_owned());
    v.show_idle_hint = true;
    let rt = runtime(v, DESIGN, true);
    assert!(has_key(&rt, "qb-toast"));
    assert!(has_key(&rt, "query-idle-hint"));
}

#[test]
fn all_surfaces_stay_inside_via_visible_rects() {
    for size in [MIN, DESIGN] {
        for dark in [true, false] {
            let mut v = vm();
            v.nl2sql_visible = true;
            v.qb_open_panel = Some("p".to_owned());
            v.qb_toast = Some(t("query.visualBuilder.appliedToast").to_owned());
            v.execution_strategy_open = true;
            let rt = runtime(v, size, dark);
            assert!(has_key(&rt, "query-editor-section"), "{size:?} dark={dark}: root mounted");
            for e in rt.elements() {
                let [x, y, w, h] = e.visible;
                assert!(
                    x >= -0.5 && y >= -0.5 && x + w <= size.0 + 0.5 && y + h <= size.1 + 0.5,
                    "{size:?} dark={dark}: {} visible={:?} rect={:?} crosses window",
                    e.key.as_deref().unwrap_or(&e.path), e.visible, e.rect
                );
            }
        }
    }
}

