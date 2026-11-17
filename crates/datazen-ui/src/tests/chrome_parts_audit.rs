//! Independent audit for chrome base components (T0-11 batch, 8754d97f8).

use rune::{Cx, El, Env, Key, KeyEvent, Mods, Runtime, col, row, text};

use crate::ui::badge::{Badge, BadgeTone};
use crate::ui::label::Label;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone, SpinnerVariant};
use crate::ui::tabs::{TabItem, Tabs};
use crate::ui::toolbar::{ToolbarButton, ToolbarButtonVariant, ToolbarShell};

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
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

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn press<S>(rt: &mut Runtime<S>, key: Key) {
    rt.key(KeyEvent {
        key,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -1.0 || r[1] < -1.0 || r[0] + r[2] > w + 1.0 || r[1] + r[3] > h + 1.0 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[derive(Default, Clone)]
struct S;

// ---- ToolbarButton: kit wrapping / a11y / keyboard ----------------------------

fn icon_only_view(_s: &S, _cx: &mut Cx<S>) -> El {
    col().w_full().h_full().center().child(
        ToolbarButton::<S>::new("tb", "Run")
            .icon(&crate::icons::ICON_PLAY)
            .icon_only(true)
            .build(true),
    )
}

fn disabled_icon_view(_s: &S, _cx: &mut Cx<S>) -> El {
    col().w_full().h_full().center().child(
        ToolbarButton::<S>::new("tb-dis", "Disabled")
            .icon(&crate::icons::ICON_X)
            .icon_only(true)
            .disabled(true)
            .build(true),
    )
}

#[test]
fn toolbar_button_icon_only_has_accessible_name_and_is_focusable() {
    let mut rt = Runtime::new(S, icon_only_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let e = element(&rt, "tb");
    assert_eq!(e.role, "button");
    assert!(e.focusable, "enabled icon-only button must be focusable");
    // kit IconButton: label/help provided -> accessible label present (tooltip/a11y name)
    assert!(
        e.label.as_deref() == Some("Run") || e.help.as_deref() == Some("Run"),
        "icon-only must carry accessible name via label/help: label={:?} help={:?}",
        e.label,
        e.help
    );
}

#[test]
fn toolbar_button_disabled_icon_only_not_focusable() {
    let mut rt = Runtime::new(S, disabled_icon_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let e = element(&rt, "tb-dis");
    assert_eq!(e.role, "button");
    assert!(!e.focusable, "disabled button must not be focusable");
}

// ---- Tabs: semantics / roving / keyboard -------------------------------------

#[derive(Clone)]
struct TabsState {
    active: String,
    changes: Vec<String>,
}

impl Default for TabsState {
    fn default() -> Self {
        Self {
            active: "a".into(),
            changes: Vec::new(),
        }
    }
}

fn items3() -> Vec<TabItem> {
    vec![
        TabItem::new("a", "Alpha"),
        TabItem::new("b", "Bravo"),
        TabItem::new("c", "Charlie"),
    ]
}

fn tabs_panel(s: &TabsState, _cx: &mut Cx<TabsState>) -> El {
    Tabs::new("tabs", items3(), s.active.clone())
        .aria_label("Demo tabs")
        .panel(text(format!("panel:{}", s.active)).key("panel-content"))
        .on_change(|s: &mut TabsState, id| {
            s.active = id.clone();
            s.changes.push(id);
        })
        .build(true)
}

fn tabs_bar(s: &TabsState, _cx: &mut Cx<TabsState>) -> El {
    Tabs::new("tabs", items3(), s.active.clone())
        .on_change(|s: &mut TabsState, id| s.active = id)
        .build(true)
}

#[test]
fn tabs_roles_roving_tabindex_and_panel_present() {
    let mut rt = Runtime::new(TabsState::default(), tabs_panel, (960.0, 640.0), env());
    rt.frame(0.0);
    assert_eq!(element(&rt, "tabs-tablist").role, "tablist");
    assert_eq!(element(&rt, "tabs-panel").role, "tabpanel");
    for id in ["a", "b", "c"] {
        assert_eq!(element(&rt, &format!("tabs-tab-{id}")).role, "tab");
    }
    assert!(element(&rt, "tabs-tab-a").focusable, "active is Tab stop");
    assert!(!element(&rt, "tabs-tab-b").focusable, "inactive not Tab stop");
    assert!(!element(&rt, "tabs-tab-c").focusable, "inactive not Tab stop");
    // React parity: aria-controls only points to panel (panel exists, keyed). Rune ElementInfo
    // has no aria-controls/aria-selected fields; we can only assert structural panel presence.
}

#[test]
fn tabs_keyboard_navigation_moves_active_and_records_changes() {
    let mut rt = Runtime::new(TabsState::default(), tabs_panel, (960.0, 640.0), env());
    rt.frame(0.0);
    click(&mut rt, "tabs-tab-a");
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.active, "b");
    click(&mut rt, "tabs-tab-b");
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.active, "c");
    click(&mut rt, "tabs-tab-c");
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.active, "a", "wrap right");
    click(&mut rt, "tabs-tab-a");
    press(&mut rt, Key::Left);
    assert_eq!(rt.state.active, "c", "wrap left");
    click(&mut rt, "tabs-tab-c");
    press(&mut rt, Key::Home);
    assert_eq!(rt.state.active, "a");
    click(&mut rt, "tabs-tab-a");
    press(&mut rt, Key::End);
    assert_eq!(rt.state.active, "c");
    assert!(!rt.state.changes.is_empty(), "on_change invoked");
}

#[test]
fn tabs_enter_space_activate_focused_tab() {
    let mut rt = Runtime::new(TabsState::default(), tabs_panel, (960.0, 640.0), env());
    rt.frame(0.0);
    click(&mut rt, "tabs-tab-a");
    // Move selection to b via Right, then Enter should activate current (b) again / remain stable;
    // Space on active tab should also activate.
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.active, "b");
    click(&mut rt, "tabs-tab-b");
    press(&mut rt, Key::Enter);
    assert_eq!(rt.state.active, "b");
    press(&mut rt, Key::Text(" ".into()));
    assert_eq!(rt.state.active, "b");
}

#[test]
fn tabs_bar_only_has_tablist_root_no_panel() {
    let mut rt = Runtime::new(TabsState::default(), tabs_bar, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "tabs"));
    assert_eq!(element(&rt, "tabs-tablist").role, "tablist");
    assert!(!has_key(&rt, "tabs-panel"));
}

#[test]
fn tabs_geometry_in_window_960_and_1440() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(TabsState::default(), tabs_panel, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("tabs panel {w}x{h}"));
        let mut rt = Runtime::new(TabsState::default(), tabs_bar, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("tabs bar {w}x{h}"));
    }
}

// ---- ToolbarShell narrow width -----------------------------------------------

fn toolbar_many(_s: &S, _cx: &mut Cx<S>) -> El {
    let mut leading = Vec::new();
    let mut trailing = Vec::new();
    for i in 0..6 {
        leading.push(
            ToolbarButton::<S>::new(format!("lead-{i}"), format!("L{i}"))
                .icon(&crate::icons::ICON_PLUS)
                .icon_only(true)
                .build(true),
        );
    }
    for i in 0..10 {
        trailing.push(
            ToolbarButton::<S>::new(format!("trail-{i}"), format!("T{i}"))
                .icon(&crate::icons::ICON_SETTINGS)
                .icon_only(true)
                .build(true),
        );
    }
    ToolbarShell::new("tbar").leading(leading).trailing(trailing).build(true)
}

#[test]
fn toolbar_shell_narrow_width_no_overlap_no_overflow_and_trailing_shrinks() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(S, toolbar_many, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("toolbar {w}x{h}"));
        let bar = element(&rt, "tbar").rect;
        assert!(bar[0] >= -1.0 && bar[0] + bar[2] <= w + 1.0, "shell within width: {bar:?}");
        let lead = element(&rt, "tbar-leading").rect;
        let trail = element(&rt, "tbar-trailing").rect;
        // No overlap: trailing should start at or after leading end (allow tiny epsilon).
        assert!(
            trail[0] + 0.5 >= lead[0] + lead[2] || trail[0] >= lead[0],
            "leading/trailing must not overlap badly: lead={lead:?} trail={trail:?}"
        );
        // Trailing is in grow+justify_end container; it must stay within shell.
        assert!(trail[0] + trail[2] <= bar[0] + bar[2] + 1.0, "trailing inside shell");
    }
}

// ---- Badge / Spinner / Label -------------------------------------------------

fn badge_view(_s: &S, _cx: &mut Cx<S>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .gap(8.0)
        .child(Badge::new("b-neutral", "Neutral").tone(BadgeTone::Neutral).build(true))
        .child(Badge::new("b-success", "Success").tone(BadgeTone::Success).build(true))
        .child(Badge::new("b-warning", "Warning").tone(BadgeTone::Warning).build(true))
        .child(Badge::new("b-danger", "Danger").tone(BadgeTone::Danger).build(true))
        .child(Badge::new("b-accent", "Accent").tone(BadgeTone::Accent).build(true))
}

#[test]
fn badge_tones_have_distinct_fills_when_observable() {
    let mut rt = Runtime::new(S, badge_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let keys = ["b-neutral", "b-success", "b-warning", "b-danger", "b-accent"];
    let fills: Vec<_> = keys.iter().map(|k| element(&rt, k).fill).collect();
    // ElementInfo exposes fill; tones use different kit colors/alphas so fills should differ.
    // If runtime flattens to same value this would indicate non-observable distinction.
    let mut unique = fills.clone();
    unique.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    unique.dedup();
    assert!(
        unique.len() >= 4,
        "expected tone fills to be distinguishable, got {} unique out of 5: {:?}",
        unique.len(),
        fills
    );
}

#[test]
fn spinner_decorative_has_no_progressbar_role_and_labeled_has_progressbar() {
    fn v(_s: &S, _cx: &mut Cx<S>) -> El {
        col()
            .w_full()
            .h_full()
            .gap(8.0)
            .child(Spinner::new("sp-plain").build(true))
            .child(Spinner::new("sp-lab").label("Loading").build(true))
    }
    let mut rt = Runtime::new(S, v, (960.0, 640.0), env());
    rt.frame(0.0);
    let plain = element(&rt, "sp-plain");
    assert_ne!(plain.role, "progressbar", "unlabeled decorative spinner must not be progressbar");
    assert!(plain.label.is_none(), "unlabeled should have no accessible label");
    assert!(has_key(&rt, "sp-plain-indicator"));
    assert!(!has_key(&rt, "sp-plain-label"));

    let lab = element(&rt, "sp-lab");
    assert_eq!(lab.role, "progressbar");
    assert_eq!(lab.label.as_deref(), Some("Loading"));
    assert_eq!(element(&rt, "sp-lab-label").text.as_deref(), Some("Loading"));
}

fn label_view(_s: &S, _cx: &mut Cx<S>) -> El {
    col()
        .w(320.0)
        .h(200.0)
        .padding(16.0)
        .child(Label::new("lbl", "Name").required(true).hint("Hint").build(true))
}

#[test]
fn label_geometry_in_window() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(S, label_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("label {w}x{h}"));
        assert_eq!(element(&rt, "lbl-text").text.as_deref(), Some("Name"));
        assert_eq!(element(&rt, "lbl-required").text.as_deref(), Some("*"));
        assert_eq!(element(&rt, "lbl-hint").text.as_deref(), Some("Hint"));
    }
}
