//! T063: `components/ErrorBoundary.tsx` port — the capture/dismiss state
//! machine, the message/copy fallbacks, and the mounted fallback.

use rune::{col, text, Cx, El, Env, Runtime};

use datazen_ui::components::error_boundary::{ErrorBoundary, ErrorBoundaryState};
use datazen_ui::i18n::{has, t};

const PREFIX: &str = "error-boundary";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

#[derive(Default)]
struct Scene {
    state: ErrorBoundaryState,
    dark: bool,
    reloads: usize,
    copies: Vec<String>,
}

impl Scene {
    fn errored(message: &str, stack: Option<&str>) -> Self {
        let mut scene = Self {
            dark: true,
            ..Self::default()
        };
        scene.state.capture(message, stack.map(|s| s.to_owned()));
        scene
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        ErrorBoundary::<Scene>::new(|s: &Scene| &s.state, |s: &mut Scene| s.state.dismiss())
            .on_reload(|s: &mut Scene| s.reloads += 1)
            .on_copy(|s: &mut Scene, content| s.copies.push(content))
            .build(scene, text("child content").key("child"), scene.dark),
    )
}

fn view_no_callbacks(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        ErrorBoundary::<Scene>::new(|s: &Scene| &s.state, |s: &mut Scene| s.state.dismiss()).build(
            scene,
            text("child content").key("child"),
            scene.dark,
        ),
    )
}

fn view_custom_key(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        ErrorBoundary::<Scene>::new(|s: &Scene| &s.state, |s: &mut Scene| s.state.dismiss())
            .key("custom-boundary")
            .build(scene, text("child content").key("child"), scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    mount_with(scene, size, view)
}

fn mount_with(
    scene: Scene,
    size: (f32, f32),
    view: fn(&Scene, &mut Cx<Scene>) -> El,
) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    maybe(rt, key).unwrap_or_else(|| panic!("{key} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// ---- state machine: capture, dismiss, fallbacks ----

#[test]
fn capture_marks_the_error_and_dismiss_clears_it() {
    let mut state = ErrorBoundaryState::default();
    assert!(!state.has_error);
    state.capture(
        "render failed",
        Some("Error: render failed\n    at Broken".to_string()),
    );
    assert!(state.has_error);
    assert_eq!(
        state.error.as_ref().map(|e| e.message.as_str()),
        Some("render failed")
    );
    state.dismiss();
    assert!(!state.has_error);
    assert_eq!(state.error, None);
    // A capture after a dismiss replaces the cleared state.
    state.capture("second failure", None);
    assert!(state.has_error);
    assert_eq!(state.display_message(), "second failure");
}

#[test]
fn display_message_falls_back_when_the_message_is_missing_or_empty() {
    let mut state = ErrorBoundaryState::default();
    assert_eq!(state.display_message(), t("backend.unknownError"));
    state.capture("", None);
    assert_eq!(
        state.display_message(),
        t("backend.unknownError"),
        "an empty message counts as absent, as JS || does"
    );
    state.capture("render failed", None);
    assert_eq!(state.display_message(), "render failed");
}

#[test]
fn copy_text_prefers_the_stack_then_the_message_then_the_fallback() {
    let mut state = ErrorBoundaryState::default();
    assert_eq!(state.copy_text(), t("backend.unknownError"));
    state.capture(
        "render failed",
        Some("Error: render failed\n    at Broken".to_string()),
    );
    assert_eq!(state.copy_text(), "Error: render failed\n    at Broken");
    state.capture("render failed", Some(String::new()));
    assert_eq!(
        state.copy_text(),
        "render failed",
        "an empty stack falls through to the message"
    );
    state.capture("", Some(String::new()));
    assert_eq!(
        state.copy_text(),
        t("backend.unknownError"),
        "an empty stack and an empty message fall through to the fallback"
    );
    state.capture("render failed", None);
    assert_eq!(state.copy_text(), "render failed");
}

#[test]
fn every_error_boundary_key_resolves() {
    for key in [
        "common.error",
        "common.close",
        "common.retry",
        "common.copy",
        "backend.unknownError",
    ] {
        assert!(has(key), "{key} is missing from the locale packs");
        assert_ne!(t(key), key, "{key} echoes instead of translating");
    }
}

// ---- mounted boundary: children vs fallback ----

#[test]
fn without_an_error_the_children_render_and_no_fallback_mounts() {
    let rt = mount(Scene::default(), (800.0, 600.0));
    assert!(has_key(&rt, "child"));
    assert!(!has_key(&rt, PREFIX));
    assert!(!has_key(&rt, &format!("{PREFIX}-heading")));
    assert!(!has_key(&rt, &format!("{PREFIX}-close")));
    assert!(!has_key(&rt, &format!("{PREFIX}-retry")));
}

#[test]
fn with_an_error_the_fallback_replaces_the_children() {
    let rt = mount(
        Scene::errored("render failed", Some("Error: render failed\n    at Broken")),
        (800.0, 600.0),
    );
    assert!(!has_key(&rt, "child"), "the children unmount on error");
    assert!(has_key(&rt, PREFIX));
    assert!(has_key(&rt, &format!("{PREFIX}-icon")));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-heading")),
        t("common.error")
    );
    assert_eq!(element(&rt, &format!("{PREFIX}-heading")).role, "heading");
    assert_eq!(text_of(&rt, &format!("{PREFIX}-message")), "render failed");
    assert_eq!(element(&rt, &format!("{PREFIX}-message")).role, "alert");
    assert_eq!(
        element(&rt, &format!("{PREFIX}-close")).label.as_deref(),
        Some(t("common.close")),
        "the close button carries its translated a11y label"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-retry")).label.as_deref(),
        Some(t("common.retry")),
        "the retry button carries its translated a11y label"
    );
}

#[test]
fn an_error_without_a_message_shows_the_unknown_error_fallback() {
    let rt = mount(Scene::errored("", None), (800.0, 600.0));
    assert_eq!(
        text_of(&rt, &format!("{PREFIX}-message")),
        t("backend.unknownError")
    );
}

#[test]
fn dismiss_restores_the_children_and_retry_reloads() {
    let mut rt = mount(Scene::errored("render failed", None), (800.0, 600.0));
    click(&mut rt, &format!("{PREFIX}-close"));
    assert!(has_key(&rt, "child"), "dismiss re-renders the children");
    assert!(!has_key(&rt, PREFIX));

    let mut rt = mount(Scene::errored("render failed", None), (800.0, 600.0));
    click(&mut rt, &format!("{PREFIX}-retry"));
    assert_eq!(rt.state.reloads, 1, "retry runs the host reload");
    assert!(
        has_key(&rt, PREFIX),
        "retry does not clear the error by itself"
    );
}

#[test]
fn copy_sends_the_stack_and_without_callbacks_the_buttons_do_nothing() {
    let mut rt = mount(
        Scene::errored("render failed", Some("Error: render failed\n    at Broken")),
        (800.0, 600.0),
    );
    click(&mut rt, &format!("{PREFIX}-copy"));
    assert_eq!(
        rt.state.copies,
        vec!["Error: render failed\n    at Broken".to_string()],
        "copy sends the stack, not the message"
    );

    let mut rt = mount_with(
        Scene::errored("render failed", Some("stack here")),
        (800.0, 600.0),
        view_no_callbacks,
    );
    click(&mut rt, &format!("{PREFIX}-copy"));
    assert!(rt.state.copies.is_empty());
    click(&mut rt, &format!("{PREFIX}-retry"));
    assert_eq!(
        rt.state.reloads, 0,
        "retry without a reload callback is inert"
    );
    assert!(has_key(&rt, PREFIX), "and the fallback stays up");
}

#[test]
fn a_custom_key_prefix_moves_every_key_under_it() {
    let rt = mount_with(
        Scene::errored("render failed", None),
        (800.0, 600.0),
        view_custom_key,
    );
    assert!(!has_key(&rt, PREFIX));
    assert!(has_key(&rt, "custom-boundary"));
    assert!(has_key(&rt, "custom-boundary-heading"));
    assert_eq!(text_of(&rt, "custom-boundary-message"), "render failed");
}

#[test]
fn the_fallback_fits_both_window_sizes_in_both_themes() {
    for (dark, size, label) in [
        (true, (960.0, 640.0), "dark 960x640"),
        (false, (960.0, 640.0), "light 960x640"),
        (true, (1440.0, 816.0), "dark 1440x816"),
        (false, (1440.0, 816.0), "light 1440x816"),
    ] {
        let mut scene = Scene::errored("render failed", None);
        scene.dark = dark;
        let rt = mount(scene, size);
        let root = element(&rt, PREFIX);
        assert!(
            (root.rect[2] - size.0).abs() < 0.5 && (root.rect[3] - size.1).abs() < 0.5,
            "{label}: the fallback fills the window, got {:?}",
            root.rect
        );
    }
}
