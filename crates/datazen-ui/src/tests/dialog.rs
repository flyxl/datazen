//! Modal dialog base: mounting rules, Escape dismissal and focus return, plus
//! the scrolling body under tall content (`ui/dialog.rs`). Layout assertions
//! live in `chrome_metrics.rs`, next to the rest of the window scans.

use super::*;

use rune::{Key, KeyEvent, Mods};

/// The key of the element that currently owns focus, from the runtime's own
/// focus evidence (`ElementInfo::focused`) rather than a headless tree dump.
fn focused_key<S>(runtime: &Runtime<S>) -> Option<String> {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.focused)
        .and_then(|element| element.key)
}

fn rect<S>(runtime: &Runtime<S>, key: &str) -> [f32; 4] {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} present"))
        .rect
}

/// The fill an element paints, from the runtime's own element list.
fn fill<S>(runtime: &Runtime<S>, key: &str) -> rune::Color {
    runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} present"))
        .fill
}

fn press<S>(runtime: &mut Runtime<S>, key: &str) {
    let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("{key} press"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.0);
    runtime.frame(0.0);
}

fn escape<S>(runtime: &mut Runtime<S>) {
    runtime.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    runtime.frame(0.0);
}

/// Tabs until `key` owns focus, as a keyboard user reaches the trigger.
fn tab_to<S>(runtime: &mut Runtime<S>, key: &str) {
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
    assert!(
        runtime.state.connection_dialog_open,
        "space opened the dialog"
    );
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

#[test]
fn scrolling_the_tall_body_to_its_end_keeps_the_footer_pinned() {
    let mut runtime = Runtime::new(
        AppState::default(),
        tall_body_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    let panel = rect(&runtime, "probe-dialog");
    let body = rect(&runtime, "probe-dialog-body");
    let footer = rect(&runtime, "probe-dialog-footer");
    let (bx, by) = runtime.find("probe-dialog-body").expect("the body");
    for _ in 0..4 {
        runtime.wheel(bx, by, -400.0);
        runtime.frame(0.0);
    }
    let content = rect(&runtime, "probe-content");
    assert!(
        (content[1] + content[3] - (body[1] + body[3])).abs() <= 1.0,
        "the body scrolled to its end: content {content:?}, body {body:?}"
    );
    assert!(
        (footer[1] + footer[3] - (panel[1] + panel[3])).abs() <= 0.5,
        "the footer stays on the panel bottom after scrolling: footer {footer:?}, panel {panel:?}"
    );
    assert!(
        panel[1] + panel[3] <= 640.0 + 0.5,
        "the panel stays inside the window: panel {panel:?}"
    );
}

// --- adversarial coverage added by the independent verifier -----------------

#[derive(Default)]
struct OutsideState {
    open: bool,
    outside: bool,
}

fn outside_view(state: &OutsideState, _cx: &mut rune::Cx<OutsideState>) -> rune::El {
    let mut layer = Vec::new();
    crate::ui::dialog::Dialog::new("probe-outside", "Outside")
        .open(state.open)
        .fit()
        .click_outside(state.outside)
        .child(rune::text("probe body"))
        .on_close(|state: &mut OutsideState| state.open = false)
        .build(crate::ui::kit::kit(true), &mut layer);
    rune::col()
        .key("probe-root")
        .w_full()
        .h_full()
        .children(layer)
}

fn outside_runtime(outside: bool) -> Runtime<OutsideState> {
    Runtime::new(
        OutsideState {
            open: true,
            outside,
        },
        outside_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    )
}

#[test]
fn the_scrim_closes_only_when_click_outside_is_enabled() {
    let mut off = outside_runtime(false);
    off.frame(0.0);
    assert!(off.find("probe-outside").is_some(), "the dialog opened");
    off.pointer_down(5.0, 5.0);
    off.pointer_up(5.0, 5.0);
    off.frame(0.0);
    off.frame(0.0);
    assert!(
        off.state.open,
        "with click_outside off the scrim press does nothing"
    );
    assert!(
        off.find("probe-outside").is_some(),
        "the dialog stayed mounted"
    );

    let mut on = outside_runtime(true);
    on.frame(0.0);
    // A press on the panel itself must never dismiss, even when the scrim does.
    let (px, py) = on.find("probe-outside").expect("the panel");
    on.pointer_down(px, py);
    on.pointer_up(px, py);
    on.frame(0.0);
    on.frame(0.0);
    assert!(
        on.state.open,
        "a press inside the panel does not dismiss it"
    );
    on.pointer_down(5.0, 5.0);
    on.pointer_up(5.0, 5.0);
    on.frame(0.0);
    on.frame(0.0);
    assert!(
        !on.state.open,
        "with click_outside on the scrim press dismisses"
    );
    assert!(on.find("probe-outside").is_none(), "the layer unmounted");
}

#[test]
fn the_engine_sidebar_shares_the_panel_and_sets_the_engine() {
    let mut runtime = runtime();
    runtime.state.open_connection_dialog();
    runtime.frame(0.0);
    let panel = rect(&runtime, "connection-dialog");
    let body = rect(&runtime, "connection-dialog-body");
    let option = rect(&runtime, "connection-engine-option-postgresql");
    assert!(
        option[0] >= panel[0] && option[0] + option[2] <= body[0],
        "the engine option sits in the sidebar left of the body: option {option:?}, panel {panel:?}, body {body:?}"
    );
    assert!(
        option[1] >= panel[1] && option[1] + option[3] <= panel[1] + panel[3],
        "the engine option stays inside the panel"
    );
    assert_eq!(runtime.state.connection_engine.text, "");
    press(&mut runtime, "connection-engine-option-mysql");
    assert_eq!(
        runtime.state.connection_engine.text, "mysql",
        "a sidebar click writes the engine field"
    );
}

#[derive(Default)]
struct MinimalState {
    open: bool,
}

fn minimal_view(state: &MinimalState, _cx: &mut rune::Cx<MinimalState>) -> rune::El {
    let mut layer = Vec::new();
    crate::ui::dialog::Dialog::new("probe-min", "")
        .open(state.open)
        .fit()
        .child(rune::text("body"))
        .on_close(|state: &mut MinimalState| state.open = false)
        .build(crate::ui::kit::kit(true), &mut layer);
    rune::col()
        .key("probe-root")
        .w_full()
        .h_full()
        .children(layer)
}

fn no_close_view(state: &MinimalState, _cx: &mut rune::Cx<MinimalState>) -> rune::El {
    let mut layer = Vec::new();
    crate::ui::dialog::Dialog::new("probe-noclose", "No close")
        .open(state.open)
        .fit()
        .child(rune::text("body"))
        .build(crate::ui::kit::kit(true), &mut layer);
    rune::col()
        .key("probe-root")
        .w_full()
        .h_full()
        .children(layer)
}

#[test]
fn a_dialog_without_a_title_or_footer_mounts_only_its_body() {
    let mut runtime = Runtime::new(
        MinimalState { open: true },
        minimal_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    assert!(
        runtime.find("probe-min-title").is_some(),
        "the title key is stable"
    );
    assert!(runtime.find("probe-min-body").is_some(), "the body mounts");
    assert!(
        runtime.find("probe-min-footer").is_none(),
        "no footer is invented"
    );
    assert!(
        runtime.find("probe-min-close").is_some(),
        "the close control is present"
    );
}

#[test]
fn a_dialog_without_a_close_handler_mounts_nothing() {
    let mut runtime = Runtime::new(
        MinimalState { open: true },
        no_close_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    assert!(runtime.find("probe-noclose").is_none(), "no panel");
    assert!(runtime.find("probe-noclose-layer").is_none(), "no layer");
    assert!(runtime.find("probe-noclose-scrim").is_none(), "no scrim");
}

#[test]
fn two_dialog_flags_in_one_frame_mount_one_layer() {
    let mut runtime = runtime();
    runtime.state.open_import_dialog();
    runtime.state.open_connection_dialog();
    runtime.frame(0.0);
    assert!(
        runtime.find("import-dialog").is_some(),
        "the import dialog wins"
    );
    assert!(
        runtime.find("connection-dialog").is_none(),
        "the connection dialog does not stack"
    );
    let scrims = runtime
        .elements()
        .iter()
        .filter(|element| {
            element
                .key
                .as_deref()
                .is_some_and(|key| key.ends_with("-scrim"))
        })
        .count();
    assert_eq!(scrims, 1, "exactly one scrim is mounted");

    escape(&mut runtime);
    assert!(!runtime.state.import_dialog_open);
    assert!(
        runtime.find("connection-dialog").is_some(),
        "the still-open connection flag surfaces once the import dialog closes"
    );
}

#[derive(Default)]
struct FiveState;

fn five_button_view(_state: &FiveState, _cx: &mut rune::Cx<FiveState>) -> rune::El {
    use rune_kit::{Button, ButtonVariant, ControlSize};
    let mut layer = Vec::new();
    let mut dialog = crate::ui::dialog::Dialog::new("probe-five", "Five buttons")
        .open(true)
        .width(920.0)
        .fill_height()
        .max_height(680.0)
        .header_pad((24.0, 16.0))
        .body_pad((24.0, 24.0))
        .footer_pad((24.0, 12.0))
        .footer_height(64.0)
        .child(rune::text("body"));
    for (index, label) in ["测试连接", "取消", "保存", "更多", "确定"]
        .iter()
        .enumerate()
    {
        dialog = dialog.footer(
            Button::new(*label)
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::ExtraLarge)
                .on_press(|_: &mut FiveState| {})
                .build(crate::ui::kit::kit(true))
                .key(format!("probe-five-{index}")),
        );
    }
    dialog
        .on_close(|_: &mut FiveState| {})
        .build(crate::ui::kit::kit(true), &mut layer);
    rune::col()
        .key("probe-root")
        .w_full()
        .h_full()
        .children(layer)
}

#[test]
fn five_buttons_stay_inside_the_footer_at_the_minimum_window() {
    let mut runtime = Runtime::new(
        FiveState,
        five_button_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    let panel = rect(&runtime, "probe-five");
    let footer = rect(&runtime, "probe-five-footer");
    assert!((footer[1] + footer[3] - (panel[1] + panel[3])).abs() <= 0.5);
    for index in 0..5 {
        let button = rect(&runtime, &format!("probe-five-{index}"));
        assert!(
            button[0] >= footer[0] && button[0] + button[2] <= footer[0] + footer[2],
            "button {index} {button:?} stays inside the footer {footer:?}"
        );
        assert!(
            button[1] >= footer[1] && button[1] + button[3] <= footer[1] + footer[3],
            "button {index} stays vertically inside the footer"
        );
    }
}

#[test]
fn the_import_dialog_fits_its_content_at_both_window_sizes() {
    let mut heights = Vec::new();
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut runtime = Runtime::new(
            AppState::default(),
            view,
            (width, height),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        runtime.state.open_import_dialog();
        runtime.frame(0.0);
        let panel = rect(&runtime, "import-dialog");
        let body = rect(&runtime, "import-dialog-body");
        let footer = rect(&runtime, "import-dialog-footer");
        assert!(
            body[3] <= 448.5,
            "the body honours its 448 cap at {width}x{height}: {body:?}"
        );
        assert!(
            (footer[1] + footer[3] - (panel[1] + panel[3])).abs() <= 0.5,
            "the footer is pinned to the panel's bottom at {width}x{height}"
        );
        heights.push(panel[3]);
    }
    assert_eq!(
        heights[0], heights[1],
        "a fitted dialog keeps its content height when the window grows"
    );
}

#[derive(Default)]
struct ScrollState {
    open: bool,
    /// The app's `*_dialog_epoch`: bumped by every open, as
    /// `AppState::open_*_dialog` does, so the base gives the body a new
    /// identity and a fresh scroll viewport.
    epoch: u64,
}

impl ScrollState {
    fn open(&mut self) {
        self.open = true;
        self.epoch = self.epoch.wrapping_add(1);
    }
}

fn scroll_view(state: &ScrollState, _cx: &mut rune::Cx<ScrollState>) -> rune::El {
    let mut layer = Vec::new();
    crate::ui::dialog::Dialog::new("probe-scroll", "Scroll")
        .open(state.open)
        .open_epoch(state.epoch)
        .fit()
        .body_max_height(120.0)
        .child(rune::text("tall").key("probe-scroll-content").h(600.0))
        .footer(rune::text("footer"))
        .on_close(|state: &mut ScrollState| state.open = false)
        .build(crate::ui::kit::kit(true), &mut layer);
    rune::col()
        .key("probe-root")
        .w_full()
        .h_full()
        .children(layer)
}

#[test]
fn reopening_a_scrolled_dialog_starts_at_the_top() {
    let mut runtime = Runtime::new(
        ScrollState {
            open: true,
            epoch: 1,
        },
        scroll_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    let before = rect(&runtime, "probe-scroll-content")[1];
    let (bx, by) = runtime.find("probe-scroll-body").expect("the body");
    runtime.wheel(bx, by, -240.0);
    runtime.frame(0.0);
    runtime.frame(0.0);
    let after = rect(&runtime, "probe-scroll-content")[1];
    assert!(
        after < before,
        "the wheel scrolled the body {before} -> {after}"
    );

    escape(&mut runtime);
    assert!(!runtime.state.open, "escape closed the dialog");
    assert!(
        runtime.find("probe-scroll").is_none(),
        "the dialog unmounted"
    );
    for _ in 0..5 {
        runtime.frame(0.0);
    }
    runtime.update(|state, _| state.open());
    runtime.frame(0.0);
    runtime.frame(0.0);
    assert_eq!(
        rect(&runtime, "probe-scroll-content")[1],
        before,
        "the reopened body starts at the top, not at the old offset"
    );
}

#[test]
fn tab_and_shift_tab_wrap_inside_the_dialog() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    tab_to(&mut runtime, "connection-new");
    runtime.key(KeyEvent {
        key: Key::Text(" ".into()),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    runtime.frame(0.0);
    assert_eq!(
        focused_key(&runtime).as_deref(),
        Some("connection-dialog-close"),
        "the dialog opens with focus on its first tab stop"
    );
    let mut order = vec![focused_key(&runtime)];
    for _ in 0..12 {
        runtime.key(KeyEvent {
            key: Key::Tab,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
        order.push(focused_key(&runtime));
    }
    for key in order.iter().flatten() {
        assert!(
            !key.starts_with("connection-list") && !key.starts_with("rail-"),
            "focus never escapes the dialog: {key}"
        );
    }
    assert_eq!(
        order.last().and_then(|key| key.clone()),
        Some("connection-save".to_owned()),
        "twelve tabs land on the last control: {order:?}"
    );

    runtime.key(KeyEvent {
        key: Key::Tab,
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    assert_eq!(
        focused_key(&runtime).as_deref(),
        Some("connection-dialog-close"),
        "tab past the last control wraps to the close control"
    );
    runtime.key(KeyEvent {
        key: Key::Tab,
        mods: Mods {
            shift: true,
            ..Mods::default()
        },
    });
    runtime.frame(0.0);
    assert_eq!(
        focused_key(&runtime).as_deref(),
        Some("connection-save"),
        "shift+tab from the first control wraps backwards to the last"
    );
}

#[test]
fn typing_then_escape_then_reopening_resets_the_form() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    press(&mut runtime, "connection-new");
    runtime.settle_all();
    runtime.frame(0.0);
    press(&mut runtime, "connection-name-input");
    runtime.frame(0.0);
    runtime.paste("probe-conn");
    runtime.frame(0.0);
    press(&mut runtime, "connection-address-input");
    runtime.paste("127.0.0.1");
    runtime.frame(0.0);
    assert_eq!(runtime.state.connection_name.text, "probe-conn");
    assert_eq!(runtime.state.connection_address.text, "127.0.0.1");

    escape(&mut runtime);
    assert!(
        !runtime.state.connection_dialog_open,
        "escape closed the dialog"
    );
    press(&mut runtime, "connection-new");
    assert!(
        runtime.state.connection_dialog_open,
        "the trigger reopened it"
    );
    assert_eq!(
        runtime.state.connection_name.text, "",
        "reopening resets the name (the React dialog unmounts and re-seeds)"
    );
    assert_eq!(runtime.state.connection_address.text, "");
}

fn long_title_view(_state: &FiveState, _cx: &mut rune::Cx<FiveState>) -> rune::El {
    let mut layer = Vec::new();
    crate::ui::dialog::Dialog::new(
        "probe-long",
        "A very long dialog title that should stay on one line instead of growing the header",
    )
    .open(true)
    .fit()
    .child(rune::text("body"))
    .on_close(|_: &mut FiveState| {})
    .build(crate::ui::kit::kit(true), &mut layer);
    rune::col()
        .key("probe-root")
        .w_full()
        .h_full()
        .children(layer)
}

/// The title's box stays one line and left of the close control even when the
/// text is longer than the space. The elision itself (`El::truncate`, React's
/// `truncate`) is not in the runtime's element list — `text` reports the whole
/// string and the box is the same with and without it — so only the layout
/// invariant is asserted here.
#[test]
fn a_long_title_stays_on_one_line_and_clears_the_close_control() {
    let mut runtime = Runtime::new(
        FiveState,
        long_title_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    let header = rect(&runtime, "probe-long-header");
    let title = rect(&runtime, "probe-long-title");
    let close = rect(&runtime, "probe-long-close");
    assert_eq!(header[3], 54.0, "the header keeps its one-line height");
    assert_eq!(title[3], 20.0, "the title is one line tall");
    assert!(
        title[0] + title[2] <= close[0],
        "the title's box stops before the close control: title {title:?}, close {close:?}"
    );
}

#[test]
fn five_tabs_reach_the_new_connection_trigger_and_escape_returns_there() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    for _ in 0..5 {
        runtime.key(KeyEvent {
            key: Key::Tab,
            mods: Mods::default(),
        });
        runtime.frame(0.0);
    }
    assert_eq!(
        focused_key(&runtime),
        Some("connection-new".to_string()),
        "five tabs reach the new-connection trigger"
    );
    runtime.key(KeyEvent {
        key: Key::Text(" ".to_string()),
        mods: Mods::default(),
    });
    runtime.frame(0.0);
    runtime.frame(0.0);
    assert!(
        runtime.state.connection_dialog_open,
        "space opens the dialog"
    );
    assert!(runtime.find("connection-dialog-layer").is_some());
    escape(&mut runtime);
    assert_eq!(
        focused_key(&runtime),
        Some("connection-new".to_string()),
        "escape returns focus to the trigger"
    );
    assert!(runtime.find("connection-dialog-layer").is_none());
    assert!(runtime.find("connection-dialog").is_none());
}

#[test]
fn the_scrim_dims_the_page_at_half_opacity() {
    // React's `bg-black/50` (`packages/ui/src/Dialog.tsx`) and the pre-port
    // `scrim()` in `views/modals.rs` (`oklcha(0.0, 0.0, 256.0, 0.5)`) both dim
    // the page by half. The kit palette's own scrim is 30%, so the base cannot
    // just paint `k.c.scrim`.
    let mut runtime = runtime();
    runtime.frame(0.0);
    press(&mut runtime, "connection-new");
    let scrim = fill(&runtime, "connection-dialog-scrim");
    assert!(
        (scrim.a - 0.5).abs() <= 0.001,
        "the scrim is 50% opaque, not {:.1}%: {scrim:?}",
        scrim.a * 100.0
    );
    assert!(
        scrim.r.abs() <= 1e-3 && scrim.g.abs() <= 1e-3 && scrim.b.abs() <= 1e-3,
        "the scrim is black, as React's `bg-black/50`: {scrim:?}"
    );
}

#[test]
fn the_panel_draws_the_kit_overlay_elevation() {
    // React's `shadow-xl` on the panel; `rune_kit::Dialog` paints
    // `k.elevation.overlay` the same way. For the neutral palette those layers
    // are an 8pt drop with a 16pt blur plus a zero-blur 0.5pt ring.
    let mut runtime = runtime();
    runtime.frame(0.0);
    press(&mut runtime, "connection-new");
    runtime.frame(0.0);
    let pose = runtime
        .presented_pose("connection-dialog")
        .expect("the panel has presented visuals");
    let shadows = pose.shadows();
    assert_eq!(
        shadows.len(),
        2,
        "the overlay elevation is a drop plus a ring: {shadows:?}"
    );
    assert!(
        (shadows[0].y - 8.0).abs() <= 0.5 && (shadows[0].blur - 16.0).abs() <= 0.5,
        "the drop layer is the kit's overlay drop: {shadows:?}"
    );
    assert!(
        shadows[1].blur.abs() <= 0.5 && (shadows[1].spread - 0.5).abs() <= 0.5,
        "the second layer is the kit's 0.5pt ring: {shadows:?}"
    );
    assert!(
        shadows.iter().any(|shadow| shadow.color.a > 0.0),
        "the elevation is painted, not transparent: {shadows:?}"
    );
}

#[test]
fn reopening_a_dialog_bumps_its_generation() {
    // The scroll reset relies on the app bumping the generation on every open,
    // so a stale body never keeps the offset of the one it replaced.
    let mut runtime = runtime();
    runtime.frame(0.0);
    press(&mut runtime, "connection-new");
    let connection = runtime.state.connection_dialog_epoch;
    escape(&mut runtime);
    press(&mut runtime, "connection-new");
    assert_eq!(
        runtime.state.connection_dialog_epoch,
        connection.wrapping_add(1),
        "reopening the connection dialog bumps its generation"
    );

    escape(&mut runtime);
    press(&mut runtime, "import-connections");
    let import = runtime.state.import_dialog_epoch;
    escape(&mut runtime);
    press(&mut runtime, "import-connections");
    assert_eq!(
        runtime.state.import_dialog_epoch,
        import.wrapping_add(1),
        "reopening the import dialog bumps its generation"
    );
}

#[test]
fn both_dialogs_paint_the_half_opacity_black_scrim() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    press(&mut runtime, "connection-new");
    for key in ["connection-dialog-scrim"] {
        let scrim = fill(&runtime, key);
        assert!(
            (scrim.a - 0.5).abs() <= 0.001,
            "{key} is half opaque, not {}: {scrim:?}",
            scrim.a
        );
        assert!(
            scrim.r.abs() <= 1e-3 && scrim.g.abs() <= 1e-3 && scrim.b.abs() <= 1e-3,
            "{key} is pure black: {scrim:?}"
        );
    }
    escape(&mut runtime);
    press(&mut runtime, "import-connections");
    let scrim = fill(&runtime, "import-dialog-scrim");
    assert!(
        (scrim.a - 0.5).abs() <= 0.001,
        "the import scrim is half opaque, not {}: {scrim:?}",
        scrim.a
    );
    assert!(
        scrim.r.abs() <= 1e-3 && scrim.g.abs() <= 1e-3 && scrim.b.abs() <= 1e-3,
        "the import scrim is pure black: {scrim:?}"
    );
}

#[test]
fn reopening_stamps_a_new_content_generation_but_keeps_the_body_key() {
    let mut runtime = Runtime::new(
        ScrollState {
            open: true,
            epoch: 1,
        },
        scroll_view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    assert!(
        runtime.find("probe-scroll-content-1").is_some(),
        "the first open stamps content-1"
    );
    assert!(
        runtime.find("probe-scroll-body").is_some(),
        "the documented body key is stable"
    );
    let before = rect(&runtime, "probe-scroll-content")[1];
    let (bx, by) = runtime.find("probe-scroll-body").expect("the body");
    runtime.wheel(bx, by, -240.0);
    runtime.frame(0.0);
    runtime.frame(0.0);
    let scrolled = rect(&runtime, "probe-scroll-content")[1];
    assert!(scrolled < before, "the wheel moved the body");

    escape(&mut runtime);
    assert!(runtime.find("probe-scroll-content-1").is_none());
    for _ in 0..5 {
        runtime.frame(0.0);
    }
    runtime.update(|state, _| state.open());
    runtime.frame(0.0);
    runtime.frame(0.0);
    assert!(
        runtime.find("probe-scroll-content-2").is_some(),
        "the second open stamps content-2"
    );
    assert!(
        runtime.find("probe-scroll-content-1").is_none(),
        "the old generation is gone"
    );
    assert!(
        runtime.find("probe-scroll-body").is_some(),
        "the documented body key still finds the body"
    );
    assert_eq!(
        rect(&runtime, "probe-scroll-content")[1],
        before,
        "the new generation starts at the top"
    );
}

#[test]
fn twenty_opens_do_not_accumulate_dialog_elements() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    press(&mut runtime, "connection-new");
    runtime.settle_all();
    let element_count = runtime.elements().len();
    for _ in 1..20 {
        escape(&mut runtime);
        press(&mut runtime, "connection-new");
        runtime.settle_all();
        assert_eq!(
            runtime.elements().len(),
            element_count,
            "the element tree does not grow with the open generation"
        );
    }
    assert_eq!(
        runtime.state.connection_dialog_epoch, 20,
        "every open bumped the generation"
    );
    let content_keys: Vec<String> = runtime
        .elements()
        .into_iter()
        .filter_map(|element| element.key)
        .filter(|key| key.starts_with("connection-dialog-content-"))
        .collect();
    assert_eq!(
        content_keys,
        vec!["connection-dialog-content-20".to_string()],
        "exactly one content generation is mounted"
    );
}

#[test]
fn the_panel_paints_exactly_the_kit_overlay_layers() {
    let kit = crate::ui::kit::kit(true);
    let overlay = kit.elevation.overlay.clone();
    let mut runtime = runtime();
    runtime.frame(0.0);
    press(&mut runtime, "connection-new");
    runtime.frame(0.0);
    let pose = runtime
        .presented_pose("connection-dialog")
        .expect("the panel has presented visuals");
    assert_eq!(
        pose.shadows(),
        overlay.as_slice(),
        "the panel paints the kit's overlay elevation, not a hand-rolled shadow"
    );
    assert_eq!(pose.shadows().len(), 2, "the overlay is a drop plus a ring");
}

#[test]
fn an_open_dialog_keeps_typed_fields_across_an_epoch_rebuild() {
    let mut runtime = runtime();
    runtime.frame(0.0);
    press(&mut runtime, "connection-new");
    runtime.settle_all();
    runtime.frame(0.0);
    press(&mut runtime, "connection-name-input");
    runtime.frame(0.0);
    runtime.paste("epoch-kept");
    runtime.frame(0.0);
    assert_eq!(runtime.state.connection_name.text, "epoch-kept");

    runtime.update(|state, _| {
        state.connection_dialog_epoch = state.connection_dialog_epoch.wrapping_add(1);
    });
    runtime.frame(0.0);
    runtime.frame(0.0);
    assert!(runtime.state.connection_dialog_open);
    assert_eq!(
        runtime.state.connection_name.text, "epoch-kept",
        "the field value lives in AppState, not in the body wrapper"
    );
}
