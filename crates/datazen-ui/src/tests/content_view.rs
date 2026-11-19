//! T199: ContentView composition ports — real probes against the rendered
//! element tree for the toolbar/status gates, drawers, split panes and the
//! minimum-window geometry in both themes.

use rune::{col, tok, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::state_def::AppState;
use crate::views::content_view::{build, ContentViewVm};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

thread_local! {
    /// The reused body/toolbar/status views bind handlers to `AppState`, so the
    /// runtime state must be `AppState`; the controlled VM travels via this
    /// test-only fixture instead of a custom state type.
    static FIXTURE: std::cell::RefCell<(ContentViewVm, bool)> =
        std::cell::RefCell::new((ContentViewVm::default(), true));
}

#[derive(Clone)]
struct Probe {
    vm: ContentViewVm,
    dark: bool,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            vm: ContentViewVm {
                react_toolbar_gate: true,
                react_status_gate: true,
                detail_panel_applicable: true,
                ..Default::default()
            },
            dark: true,
        }
    }
}

fn view(state: &AppState, _cx: &mut Cx<AppState>) -> El {
    let (vm, dark) = FIXTURE.with(|f| f.borrow().clone());
    col()
        .w_full()
        .h_full()
        .child(build(state, tok::theme(dark), dark, &vm))
}

fn runtime(probe: Probe, size: (f32, f32)) -> Runtime<AppState> {
    FIXTURE.with(|f| *f.borrow_mut() = (probe.vm, probe.dark));
    let mut rt = Runtime::new(AppState::default(), view, size, env());
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

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) outside {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[test]
fn content_view_composes_the_react_structure_keys() {
    let mut probe = Probe::default();
    probe.vm.has_active_panel = true;
    let rt = runtime(probe, (1440.0, 816.0));
    for key in [
        "content-view",
        "content-view-main",
        "content-view-panes",
        "content-view-dialogs",
        "content-toolbar",
        "tab-strip-absent",
        "content-status-bar",
    ] {
        assert!(has_key(&rt, key), "composed key: {key}");
    }
    // No drawers are open: React renders only the main body.
    assert!(has_key(&rt, "content-view-drawers-absent"));
    assert!(!has_key(&rt, "content-view-detail"));
    assert!(!has_key(&rt, "content-view-ai"));
}

#[test]
fn a_kv_panel_skips_the_toolbar_band_but_keeps_the_status_bar() {
    let mut probe = Probe::default();
    probe.vm.has_active_panel = true;
    probe.vm.is_kv_panel = true;
    let rt = runtime(probe, (960.0, 640.0));

    assert!(has_key(&rt, "content-toolbar-absent"), "KV hides the 48px band");
    assert!(!has_key(&rt, "content-toolbar"), "no toolbar actions on a KV panel");
    assert!(has_key(&rt, "content-status-bar"), "React still mounts the footer");
}

#[test]
fn drawer_states_mount_only_the_requested_panels() {
    let mut probe = Probe::default();
    probe.vm.has_active_panel = true;
    probe.vm.detail_open = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(has_key(&rt, "content-view-detail"));
    assert!(has_key(&rt, "content-view-detail-close"));
    assert!(!has_key(&rt, "content-view-ai"));
    assert_eq!(
        element(&rt, "content-view-detail").label.as_deref(),
        Some(t("detail.title"))
    );

    let mut probe = Probe::default();
    probe.vm.has_active_panel = true;
    probe.vm.ai_chat_open = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(has_key(&rt, "content-view-ai"));
    assert!(has_key(&rt, "content-view-ai-close"));
    assert!(!has_key(&rt, "content-view-detail"));

    let mut probe = Probe::default();
    probe.vm.has_active_panel = true;
    probe.vm.detail_open = true;
    probe.vm.ai_chat_open = true;
    let rt = runtime(probe, (1440.0, 816.0));
    assert!(has_key(&rt, "content-view-detail"));
    assert!(has_key(&rt, "content-view-ai"));

    // detailOpen without an applicable panel mounts no drawer.
    let mut probe = Probe::default();
    probe.vm.detail_open = true;
    probe.vm.detail_panel_applicable = false;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(
        !has_key(&rt, "content-view-detail"),
        "detail drawer must respect detailPanelApplicable"
    );
    assert!(has_key(&rt, "content-view-drawers-absent"));
}

#[test]
fn split_pane_mounts_a_divider_and_a_secondary_pane() {
    let mut probe = Probe::default();
    probe.vm.has_active_panel = true;
    probe.vm.split_secondary = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(has_key(&rt, "content-view-split"));
    assert!(has_key(&rt, "content-view-pane-secondary"));

    let mut probe = Probe::default();
    probe.vm.has_active_panel = true;
    let rt = runtime(probe, (960.0, 640.0));
    assert!(!has_key(&rt, "content-view-split"));
    assert!(!has_key(&rt, "content-view-pane-secondary"));
}

#[test]
fn panel_gates_are_independently_representable() {
    let mut probe = Probe::default();
    probe.vm.has_active_panel = true;
    assert!(probe.vm.show_toolbar());
    assert!(probe.vm.show_status_bar());

    probe.vm.is_kv_panel = true;
    assert!(!probe.vm.show_toolbar(), "KV suppresses the toolbar");
    assert!(probe.vm.show_status_bar(), "KV keeps the footer");

    probe.vm.has_active_panel = false;
    assert!(!probe.vm.show_toolbar());
    assert!(!probe.vm.show_status_bar());
}

#[test]
fn geometry_holds_at_both_sizes_in_both_themes_with_every_block_mounted() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut probe = Probe::default();
            probe.dark = dark;
            probe.vm.has_active_panel = true;
            probe.vm.detail_open = true;
            probe.vm.ai_chat_open = true;
            probe.vm.split_secondary = true;
            let rt = runtime(probe, (w, h));
            assert_in_window(&rt, w, h, &format!("content view {w}x{h} dark={dark}"));
            for key in [
                "content-view",
                "content-view-main",
                "content-view-panes",
                "content-view-split",
                "content-view-pane-secondary",
                "content-view-drawers",
                "content-view-detail",
                "content-view-ai",
                "content-toolbar",
                "content-status-bar",
            ] {
                assert!(has_key(&rt, key), "{key} at {w}x{h} dark={dark}");
            }

            let mut probe = Probe::default();
            probe.dark = dark;
            probe.vm.has_active_panel = true;
            probe.vm.is_kv_panel = true;
            let rt = runtime(probe, (w, h));
            assert_in_window(&rt, w, h, &format!("KV content view {w}x{h} dark={dark}"));
        }
    }
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Making `ContentViewVm::show_toolbar` ignore `is_kv_panel` made
//    `a_kv_panel_skips_the_toolbar_band_but_keeps_the_status_bar` fail with:
//    `content-toolbar-absent` missing (`KV hides the 48px band`).
// 2. Dropping the `detail_panel_applicable` gate from both `show_drawers` and
//    the detail-drawer mount made `drawer_states_mount_only_the_requested_panels`
//    fail with: `detail drawer must respect detailPanelApplicable`.
