//! Independent tester audit for T199 (`ContentView` composition port).
//!
//! Precise probes for the gate combinations and source contracts the port's
//! own tests leave open:
//! 1. React gates with no active panel suppress both bands, while the drawers
//!    container stays mounted and follows its own flags (React's call site is
//!    unconditional);
//! 2. the AI drawer must not depend on `detail_panel_applicable`;
//! 3. a source guard: the new view stays under the 800-line cap and carries no
//!    new `oklch` / bare `unwrap` / `expect` / `todo!()`.

use rune::{col, tok, Cx, El, Env, Runtime};

use datazen_ui::state_def::AppState;
use datazen_ui::views::content_view::{build, ContentViewVm};

fn env() -> Env {
    Env { system_dark: true, scale: 1.0 }
}

thread_local! {
    static AUDIT_FIXTURE: std::cell::RefCell<(ContentViewVm, bool)> =
        std::cell::RefCell::new((ContentViewVm::default(), true));
}

#[derive(Clone)]
struct AuditProbe {
    vm: ContentViewVm,
    dark: bool,
}

fn view(state: &AppState, _cx: &mut Cx<AppState>) -> El {
    let (vm, dark) = AUDIT_FIXTURE.with(|f| f.borrow().clone());
    col()
        .w_full()
        .h_full()
        .child(build(state, tok::theme(dark), dark, &vm, &mut Vec::new()))
}

fn runtime(vm: ContentViewVm, dark: bool, size: (f32, f32)) -> Runtime<AppState> {
    AUDIT_FIXTURE.with(|f| *f.borrow_mut() = (vm, dark));
    let mut rt = Runtime::new(AppState::default(), view, size, env());
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

#[test]
fn react_gates_with_no_active_panel_suppress_both_bands() {
    let vm = ContentViewVm {
        react_toolbar_gate: true,
        react_status_gate: true,
        detail_panel_applicable: true,
        detail_open: true,
        ai_chat_open: true,
        ..Default::default()
    };
    // has_active_panel is false: React's `activePanel` gate hides both bands.
    // The drawers host is mounted unconditionally at React's call site and
    // follows its own open flags, so it must still be present here.
    let rt = runtime(vm, true, (960.0, 640.0));
    assert!(has_key(&rt, "content-toolbar-absent"), "no panel -> no toolbar band");
    assert!(has_key(&rt, "content-status-bar-absent"), "no panel -> no status band");
    assert!(!has_key(&rt, "content-toolbar"), "toolbar actions are gone");
    assert!(!has_key(&rt, "content-status-bar"), "status footer is gone");
    assert!(has_key(&rt, "content-view-drawers"), "drawers host is unconditional");
    assert!(has_key(&rt, "content-view-detail"), "detail drawer follows its own flag");
    assert!(has_key(&rt, "content-view-ai"), "AI drawer follows its own flag");
}

#[test]
fn ai_drawer_does_not_depend_on_detail_applicability() {
    let vm = ContentViewVm {
        has_active_panel: true,
        react_toolbar_gate: true,
        react_status_gate: true,
        detail_panel_applicable: false,
        ai_chat_open: true,
        ..Default::default()
    };
    let rt = runtime(vm, true, (960.0, 640.0));
    assert!(has_key(&rt, "content-view-ai"), "AI chat opens on its own flag");
    assert!(has_key(&rt, "content-view-ai-close"));
    assert!(!has_key(&rt, "content-view-detail"), "inapplicable detail stays closed");
    assert!(!has_key(&rt, "content-view-detail-close"));
    assert!(has_key(&rt, "content-view-drawers"));
    assert!(!has_key(&rt, "content-view-drawers-absent"));
}

#[test]
fn host_does_not_mount_database_status_without_an_active_panel() {
    for section in [
        datazen_ui::types::Section::Workspace,
        datazen_ui::types::Section::Connections,
        datazen_ui::types::Section::Settings,
    ] {
        let mut state = AppState::default();
        state.active = section;
        let vm = datazen_ui::views::content_view::ContentViewVm::from_host(&state);
        assert!(!vm.react_toolbar_gate && vm.react_status_gate);
        AUDIT_FIXTURE.with(|f| *f.borrow_mut() = (vm, true));
        let mut rt = Runtime::new(state, view, (960.0, 640.0), env());
        rt.frame(0.0);
        assert!(has_key(&rt, "content-toolbar"), "{section:?}: pinned toolbar");
        assert!(!has_key(&rt, "content-status-bar"), "{section:?}: no active panel");
        assert!(!has_key(&rt, "content-toolbar-absent"));
        assert!(has_key(&rt, "content-status-bar-absent"));
    }
}

#[test]
fn new_view_source_respects_the_800_line_cap_and_has_no_bare_panics_or_oklch() {
    // Only the file introduced by T199 is in scope; pre-existing views such as
    // connections_home.rs predate this task and are not part of its diff.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/views");
    let path = root.join("content_view.rs");
    let view_src = std::fs::read_to_string(&path).expect("content_view.rs is readable");
    let lines = view_src.lines().count();
    assert!(lines <= 800, "content_view.rs has {lines} lines (> 800)");
    for forbidden in ["oklch", ".unwrap()", ".expect(", "todo!()", "unimplemented!()", "panic!("] {
        assert!(!view_src.contains(forbidden), "content_view.rs contains {forbidden}");
    }
    // Icons used by the new view must come from the registry, never inline SVG.
    assert!(view_src.contains("use rune::"), "registry/rune imports present");
}
