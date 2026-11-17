//! Modal dialog base: mounting rules, Escape dismissal and focus return, plus
//! the scrolling body under tall content (`ui/dialog.rs`). Layout assertions
//! live in `chrome_metrics.rs`, next to the rest of the window scans.

use super::*;

use rune::{Key, KeyEvent, Mods};

/// The key of the element that currently owns focus, from the runtime's own
/// focus evidence (`ElementInfo::focused`) rather than a headless tree dump.
fn focused_key(runtime: &Runtime<AppState>) -> Option<String> {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.focused)
        .and_then(|element| element.key)
}

fn rect(runtime: &Runtime<AppState>, key: &str) -> [f32; 4] {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} present"))
        .rect
}

fn press(runtime: &mut Runtime<AppState>, key: &str) {
    let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("{key} press"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.frame(0.0);
}

fn escape(runtime: &mut Runtime<AppState>) {
    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    runtime.frame(0.0);
}

/// Tabs until `key` owns focus, as a keyboard user reaches the trigger.
fn tab_to(runtime: &mut Runtime<AppState>, key: &str) {
    for _ in 0..64 {
        runtime.key(KeyEvent {
            key: Key::Tab,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        if focused_key(runtime).as_deref() == Some(key) {
            return;
        }
    }
    panic!("{key} is reachable from the keyboard");
}

#[test]
fn escape_closes_the_connection_dialog_and_unmounts_its_layer() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    press(&mut runtime, "connection-new");
    assert!(runtime.state.connection_dialog_open, "the dialog opened");
    for key in [
        "connection-dialog",
        "connection-dialog-layer",
        "connection-dialog-scrim",
        "connection-dialog-footer",
    ] {
        assert!(runtime.find(key).is_some(), "{key} is mounted");
    }

    escape(&mut runtime);

    assert!(
        !runtime.state.connection_dialog_open,
        "escape closed the connection dialog"
    );
    let mounted = runtime.elements();
    // The dismissal must unmount the whole layer, not just hide the panel: a
    // leftover scrim would keep swallowing presses on the page behind it.
    for key in [
        "connection-dialog",
        "connection-dialog-layer",
        "connection-dialog-scrim",
        "connection-dialog-footer",
    ] {
        assert!(
            !mounted
                .iter()
                .any(|element| element.key.as_deref() == Some(key)),
            "{key} unmounted"
        );
    }
}

#[test]
fn escape_hands_focus_back_to_the_trigger_that_opened_the_dialog() {
    // Measurement convention: the dialog is opened from the keyboard (Space on
    // the focused trigger), because Rune hands focus back to where it was
    // before the modal mounted (`focus_before_commit` in
    // `runtime/input/modal_focus.rs`); a pointer press on the row trigger takes
    // no focus in Rune, so it would have nothing to restore to. The assertion
    // reads `ElementInfo::focused` after the frame settles.
    let mut runtime = runtime();
    runtime.frame(0.0);
    tab_to(&mut runtime, "connection-new");
    runtime.key(KeyEvent {
        key: Key::Text(" ".into()),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    runtime.frame(0.0);
    assert!(runtime.state.connection_dialog_open, "space opened the dialog");
    assert_ne!(
        focused_key(&runtime).as_deref(),
        Some("connection-new"),
        "focus moved into the dialog while it is open"
    );

    escape(&mut runtime);

    assert_eq!(
        focused_key(&runtime).as_deref(),
        Some("connection-new"),
        "escape returned focus to the control that opened the dialog"
    );
}

#[test]
fn a_closed_dialog_mounts_nothing() {
    // The base mirrors `rune_kit::Dialog`: a closed dialog, or one without a
    // dismissal policy, pushes no scrim and no layer.
    let mut runtime = runtime();
    runtime.frame(0.0);
    let mounted = runtime.elements();
    for key in [
        "connection-dialog-layer",
        "connection-dialog-scrim",
        "import-dialog-layer",
        "import-dialog-scrim",
    ] {
        assert!(
            !mounted
                .iter()
                .any(|element| element.key.as_deref() == Some(key)),
            "{key} is absent while its dialog is closed"
        );
    }
}

/// A fitted dialog whose body outgrows its cap: the body scrolls and the footer
/// stays pinned to the panel's bottom edge, so the actions cannot be pushed
/// out of the window by tall content.
fn tall_body_view(state: &AppState, _cx: &mut rune::Cx<AppState>) -> rune::El {
    let _ = state;
    let mut layer = Vec::new();
    crate::ui::dialog::Dialog::new("probe-dialog", "Tall content")
        .open(true)
        .fit()
        .body_max_height(120.0)
        .child(
            rune::col()
                .key("probe-content")
                .w_full()
                .h(600.0)
                .child(rune::text("tall")),
        )
        .footer(rune::row().size(80.0, 32.0))
        .on_close(|_state: &mut AppState| {})
        .build(crate::ui::kit::kit(true), &mut layer);
    rune::col()
        .key("probe-root")
        .w_full()
        .h_full()
        .children(layer)
}

#[test]
fn a_tall_body_scrolls_and_keeps_the_footer_inside_the_panel() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut runtime = Runtime::new(
            AppState::default(),
            tall_body_view,
            (width, height),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        runtime.frame(0.0);
        let panel = rect(&runtime, "probe-dialog");
        let body = rect(&runtime, "probe-dialog-body");
        let content = rect(&runtime, "probe-content");
        let footer = rect(&runtime, "probe-dialog-footer");
        assert!(
            body[3] <= 120.5,
            "{width}x{height}: body {:?} ignores its 120pt cap",
            body
        );
        assert!(
            content[3] >= 600.0,
            "{width}x{height}: content {:?} is not taller than the body",
            content
        );
        assert!(
            footer[1] + footer[3] <= panel[1] + panel[3] + 0.5
                && (footer[1] + footer[3] - (panel[1] + panel[3])).abs() <= 0.5,
            "{width}x{height}: footer {:?} left the panel bottom {:?}",
            footer,
            panel
        );
        assert!(
            panel[1] + panel[3] <= height + 0.5 && panel[1] >= -0.5,
            "{width}x{height}: panel {:?} leaves the window",
            panel
        );
    }
}
