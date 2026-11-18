//! T051: the `App.tsx` root port — the router the host's one window is
//! handed through.
//!
//! The observation channel is the a11y tree (keys, rects and text) of the
//! real `crate::view` — the single entry both the demo `run()` and the
//! host's `run_with_adapter` hand the window to. The routed output IS the
//! main window's composition, so the router's own wiring is pinned against
//! the sources the way the i18n call-site guards pin `t("...")` call sites.

use super::*;

fn root_runtime(state: AppState, size: (f32, f32), dark: bool) -> Runtime<AppState> {
    let mut rt = Runtime::new(
        state,
        view,
        size,
        Env { system_dark: dark, scale: 1.0 },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<AppState>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<AppState>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

#[test]
fn the_first_frame_is_the_window_itself_not_a_loading_chrome() {
    // React mounts `WindowChromeFallback` — the chrome slot plus a lone
    // `Spinner` announcing itself as "Loading" (`WindowChromeFallback.tsx`)
    // — for every frame between the first render and the lazy chunk's
    // arrival (`App.tsx:17-52, 99`). The native binary links the window's
    // module, so the router composes it directly: the process's very first
    // frame is the main window's own composition, complete.
    let rt = root_runtime(AppState::default(), (1440.0, 900.0), true);

    // The routed window is `MainPage`'s composition, whole on frame one —
    // no suspense-shaped gap ever exists.
    for key in [
        "datazen-root",
        "datazen-titlebar",
        "datazen-titlebar-label",
        "datazen-theme-toggle",
        "datazen-body",
        "datazen-page",
        "datazen-rail",
    ] {
        assert!(has_key(&rt, key), "{key} present on the first frame");
    }

    // Content, not a chrome skeleton: the rail carries its sections (the
    // React `WorkspaceModeSidebar` group plus the bottom settings item).
    for section in [
        "connections",
        "workflows",
        "dashboards",
        "workspace",
        "wapps",
        "settings",
    ] {
        assert!(
            has_key(&rt, &format!("rail-{section}")),
            "rail-{section} present on the first frame"
        );
    }

    // Nothing announces itself as the interim state: the one visible token
    // of React's fallback is the spinner's "Loading" label, and it never
    // mounts because composition is direct.
    let loading = rt.elements().iter().any(|e| {
        e.label.as_deref() == Some("Loading") || e.text.as_deref() == Some("Loading")
    });
    assert!(
        !loading,
        "no loading announcement: the window is composed directly"
    );
}

#[test]
fn the_root_fills_the_window_within_bounds_at_both_sizes_and_both_themes() {
    // React's `App` renders the routed window inside the boundary/suspense
    // wrappers, which add no box (`ErrorBoundary` passes children through,
    // `Suspense` adds no node), so the window itself owns the full viewport
    // (`h-screen w-screen` in `MainPage`'s shell). The router must not
    // change that: two window sizes × both themes, no horizontal overflow,
    // the 40pt chrome band on top and the body filling the rest.
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let rt = root_runtime(AppState::default(), (w, h), dark);

            // The root fills the window exactly.
            let root = element(&rt, "datazen-root");
            assert!(
                (root.rect[0] - 0.0).abs() < 0.5
                    && (root.rect[1] - 0.0).abs() < 0.5
                    && (root.rect[2] - w).abs() < 0.5
                    && (root.rect[3] - h).abs() < 0.5,
                "the root fills {w}x{h} (dark={dark}): {:?}",
                root.rect
            );

            // The 40pt chrome band at the top (`app.rs`'s titlebar).
            let titlebar = element(&rt, "datazen-titlebar");
            assert!(
                (titlebar.rect[3] - 40.0).abs() < 0.5,
                "the titlebar keeps its 40pt band: {:?}",
                titlebar.rect
            );

            // The routed window's body fills the rest of the window.
            let body = element(&rt, "datazen-body");
            assert!(
                (body.rect[1] - 40.0).abs() < 0.5
                    && (body.rect[1] + body.rect[3] - h).abs() < 0.5,
                "the body fills the window under the chrome: {:?}",
                body.rect
            );

            // No horizontal overflow at either size.
            for key in ["datazen-root", "datazen-titlebar", "datazen-body", "datazen-page"] {
                let e = element(&rt, key);
                assert!(
                    e.rect[0] >= -0.5 && e.rect[0] + e.rect[2] <= w + 0.5,
                    "{key} out of {w}x{h} (dark={dark}): {:?}",
                    e.rect
                );
            }
        }
    }
}

#[test]
fn the_router_is_wired_view_through_app_root_to_the_main_window() {
    // The router is the port's whole content, and its output equals the
    // main window's, so the a11y tree cannot tell routing from a direct
    // call — a silent regression would be `app.rs` routing straight to
    // `main_page` again, leaving the root unwired with no warning (the
    // module is still re-exported). The wiring is pinned against the
    // sources themselves, the include_str! call-site-pin form the i18n
    // guards already use.
    let app = include_str!("../app.rs");
    assert!(
        app.contains("views::app_root("),
        "app.rs hands its one view entry to the root component"
    );
    assert!(
        !app.contains("views::main_page("),
        "app.rs no longer bypasses the root: App.tsx owns the window routing"
    );

    let root = include_str!("../views/app_root.rs");
    assert!(
        root.contains("main_page(state, cx)"),
        "the root's one reachable arm composes the main window's root"
    );
}
