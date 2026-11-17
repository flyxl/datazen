//! T0-11 chrome small parts: Spinner / Badge / Tabs / ToolbarButton / ToolbarShell / Label.

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
    rt.key(KeyEvent { key, mods: Mods::default() });
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

// ---- Spinner -----------------------------------------------------------------

#[derive(Default, Clone)]
struct SpinnerProbe;

fn spinner_plain_view(_s: &SpinnerProbe, _cx: &mut Cx<SpinnerProbe>) -> El {
    col()
        .w_full()
        .h_full()
        .center()
        .child(Spinner::new("sp").size(SpinnerSize::Md).build(true))
}

fn spinner_labeled_view(_s: &SpinnerProbe, _cx: &mut Cx<SpinnerProbe>) -> El {
    col().w_full().h_full().center().child(
        Spinner::new("sp")
            .size(SpinnerSize::Md)
            .tone(SpinnerTone::Muted)
            .variant(SpinnerVariant::Ring)
            .label("Loading")
            .build(true),
    )
}

fn spinner_sizes_view(_s: &SpinnerProbe, _cx: &mut Cx<SpinnerProbe>) -> El {
    row()
        .w_full()
        .h_full()
        .center()
        .gap(12.0)
        .child(Spinner::new("sp-sm").size(SpinnerSize::Sm).build(true))
        .child(Spinner::new("sp-md").size(SpinnerSize::Md).build(true))
        .child(Spinner::new("sp-lg").size(SpinnerSize::Lg).build(true))
}

#[test]
fn spinner_unlabeled_is_decorative_and_hidden_from_a11y() {
    let mut rt = Runtime::new(SpinnerProbe, spinner_plain_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let root = element(&rt, "sp");
    assert_ne!(root.role, "progressbar", "unlabeled spinner must not be progressbar");
    assert!(root.label.is_none(), "unlabeled spinner has no accessible label");
    assert!(has_key(&rt, "sp-indicator"), "indicator present");
    assert!(!has_key(&rt, "sp-label"), "no label text without label");
}

#[test]
fn spinner_labeled_is_progressbar_with_text() {
    let mut rt = Runtime::new(SpinnerProbe, spinner_labeled_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let root = element(&rt, "sp");
    assert_eq!(root.role, "progressbar", "labeled spinner role");
    assert_eq!(root.label.as_deref(), Some("Loading"), "accessible label");
    let label = element(&rt, "sp-label");
    assert_eq!(label.text.as_deref(), Some("Loading"), "visible label text");
    assert!(has_key(&rt, "sp-indicator"), "indicator present");
}

#[test]
fn spinner_sizes_render_and_stay_in_window() {
    let mut rt = Runtime::new(SpinnerProbe, spinner_sizes_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let sm = element(&rt, "sp-sm-indicator").rect;
    let md = element(&rt, "sp-md-indicator").rect;
    let lg = element(&rt, "sp-lg-indicator").rect;
    assert!(sm[2] > 0.0 && sm[3] > 0.0, "sm has size");
    assert!(md[2] >= sm[2] && md[3] >= sm[3], "md >= sm: {sm:?} {md:?}");
    assert!(lg[2] >= md[2] && lg[3] >= md[3], "lg >= md: {md:?} {lg:?}");
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(SpinnerProbe, spinner_plain_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("spinner {w}x{h}"));
    }
}

// ---- Badge -------------------------------------------------------------------

#[derive(Default, Clone)]
struct BadgeProbe;

fn badge_variants_view(_s: &BadgeProbe, _cx: &mut Cx<BadgeProbe>) -> El {
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

fn badge_truncate_view(_s: &BadgeProbe, _cx: &mut Cx<BadgeProbe>) -> El {
    col().w_full().h_full().padding(16.0).child(
        row().w(120.0).child(
            Badge::new("b-long", "A very very very very long badge label that must truncate")
                .tone(BadgeTone::Neutral)
                .build(true),
        ),
    )
}

#[test]
fn badge_variants_render_status_and_text() {
    let mut rt = Runtime::new(BadgeProbe, badge_variants_view, (960.0, 640.0), env());
    rt.frame(0.0);
    for (key, label) in [
        ("b-neutral", "Neutral"),
        ("b-success", "Success"),
        ("b-warning", "Warning"),
        ("b-danger", "Danger"),
        ("b-accent", "Accent"),
    ] {
        let e = element(&rt, key);
        assert_eq!(e.role, "status", "{key} role");
        assert_eq!(e.label.as_deref(), Some(label), "{key} label");
        assert_eq!(
            element(&rt, &format!("{key}-text")).text.as_deref(),
            Some(label),
            "{key} text"
        );
    }
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(BadgeProbe, badge_variants_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("badge {w}x{h}"));
    }
}

#[test]
fn badge_long_text_does_not_break_parent_bounds() {
    let mut rt = Runtime::new(BadgeProbe, badge_truncate_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let b = element(&rt, "b-long").rect;
    assert!(b[2] <= 121.0, "badge width constrained by parent: {b:?}");
    assert!(b[2] > 0.0 && b[3] > 0.0, "badge visible");
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(BadgeProbe, badge_truncate_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("badge truncate {w}x{h}"));
    }
}

// ---- Tabs --------------------------------------------------------------------

#[derive(Clone)]
struct TabsProbe {
    active: String,
    changes: Vec<String>,
}

impl Default for TabsProbe {
    fn default() -> Self {
        Self { active: "a".to_owned(), changes: Vec::new() }
    }
}

fn tab_items() -> Vec<TabItem> {
    vec![
        TabItem::new("a", "Alpha"),
        TabItem::new("b", "Bravo"),
        TabItem::new("c", "Charlie"),
    ]
}

fn tabs_panel_view(state: &TabsProbe, _cx: &mut Cx<TabsProbe>) -> El {
    Tabs::new("tabs", tab_items(), state.active.clone())
        .aria_label("Demo tabs")
        .panel(text(format!("panel:{}", state.active)).key("panel-content"))
        .on_change(|s: &mut TabsProbe, id| {
            s.active = id.clone();
            s.changes.push(id);
        })
        .build(true)
}

fn tabs_bar_view(state: &TabsProbe, _cx: &mut Cx<TabsProbe>) -> El {
    Tabs::new("tabs", tab_items(), state.active.clone())
        .on_change(|s: &mut TabsProbe, id| s.active = id)
        .build(true)
}

#[test]
fn tabs_panel_mode_renders_roles_and_roving_tabindex() {
    let mut rt = Runtime::new(TabsProbe::default(), tabs_panel_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert_eq!(element(&rt, "tabs-tablist").role, "tablist");
    assert_eq!(element(&rt, "tabs-panel").role, "tabpanel");
    for id in ["a", "b", "c"] {
        assert_eq!(element(&rt, &format!("tabs-tab-{id}")).role, "tab", "{id} role");
    }
    assert!(element(&rt, "tabs-tab-a").focusable, "active tab is Tab stop");
    assert!(!element(&rt, "tabs-tab-b").focusable, "inactive tab not Tab stop");
    assert!(!element(&rt, "tabs-tab-c").focusable, "inactive tab not Tab stop");
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(TabsProbe::default(), tabs_panel_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("tabs panel {w}x{h}"));
    }
}

#[test]
fn tabs_keyboard_left_right_home_end_switch_active_tab() {
    let mut rt = Runtime::new(TabsProbe::default(), tabs_panel_view, (960.0, 640.0), env());
    rt.frame(0.0);
    click(&mut rt, "tabs-tab-a"); // focus/select active
    press(&mut rt, Key::Right);
    assert_eq!(rt.state.active, "b", "Right -> next");
    click(&mut rt, "tabs-tab-b");
    press(&mut rt, Key::End);
    assert_eq!(rt.state.active, "c", "End -> last");
    click(&mut rt, "tabs-tab-c");
    press(&mut rt, Key::Home);
    assert_eq!(rt.state.active, "a", "Home -> first");
    click(&mut rt, "tabs-tab-a");
    press(&mut rt, Key::Left);
    assert_eq!(rt.state.active, "c", "Left wraps -> last");
}

#[test]
fn tabs_bar_only_mode_renders_tablist_root_without_panel() {
    let mut rt = Runtime::new(TabsProbe::default(), tabs_bar_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "tabs"), "root present");
    assert_eq!(element(&rt, "tabs-tablist").role, "tablist");
    assert!(!has_key(&rt, "tabs-panel"), "no panel in bar-only mode");
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(TabsProbe::default(), tabs_bar_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("tabs bar {w}x{h}"));
    }
}

// ---- ToolbarButton / ToolbarShell -------------------------------------------

#[derive(Default, Clone)]
struct ToolProbe {
    pressed: usize,
}

fn toolbar_view(_s: &ToolProbe, _cx: &mut Cx<ToolProbe>) -> El {
    let leading = vec![
        ToolbarButton::<ToolProbe>::new("tb-icon", "Run")
            .icon(&crate::icons::ICON_PLAY)
            .icon_only(true)
            .variant(ToolbarButtonVariant::Ghost)
            .on_press(|s: &mut ToolProbe| s.pressed += 1)
            .build(true),
        ToolbarButton::<ToolProbe>::new("tb-disabled", "Disabled")
            .icon(&crate::icons::ICON_X)
            .icon_only(true)
            .disabled(true)
            .build(true),
    ];
    let trailing = vec![ToolbarButton::<ToolProbe>::new("tb-trail", "Settings")
        .icon(&crate::icons::ICON_SETTINGS)
        .icon_only(true)
        .build(true)];
    ToolbarShell::new("tbar")
        .leading(leading)
        .trailing(trailing)
        .build(true)
}

fn toolbar_many_view(_s: &ToolProbe, _cx: &mut Cx<ToolProbe>) -> El {
    let mut items = Vec::new();
    for i in 0..12 {
        items.push(
            ToolbarButton::<ToolProbe>::new(format!("tb-{i}"), format!("Action {i}"))
                .icon(&crate::icons::ICON_PLUS)
                .icon_only(true)
                .build(true),
        );
    }
    ToolbarShell::new("tbar").children(items).build(true)
}

#[test]
fn toolbar_button_icon_only_disabled_not_focusable_and_carries_a11y_label() {
    let mut rt = Runtime::new(ToolProbe::default(), toolbar_view, (960.0, 640.0), env());
    rt.frame(0.0);
    let icon_btn = element(&rt, "tb-icon");
    assert_eq!(icon_btn.role, "button");
    assert!(icon_btn.focusable, "enabled icon button focusable");
    assert!(
        icon_btn.label.as_deref() == Some("Run") || icon_btn.help.is_some(),
        "tooltip/a11y label carried: label={:?}",
        icon_btn.label
    );
    let disabled = element(&rt, "tb-disabled");
    assert!(!disabled.focusable, "disabled button must not be focusable");
    click(&mut rt, "tb-icon");
    assert_eq!(rt.state.pressed, 1, "press reaches handler");
}

#[test]
fn toolbar_shell_groups_and_stays_in_window_at_small_size() {
    let mut rt = Runtime::new(ToolProbe::default(), toolbar_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, "tbar"), "shell root");
    assert!(has_key(&rt, "tbar-leading"), "leading group");
    assert!(has_key(&rt, "tbar-trailing"), "trailing group");
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(ToolProbe::default(), toolbar_many_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("toolbar many {w}x{h}"));
        // no horizontal overflow of container/root content beyond window
        let bar = element(&rt, "tbar").rect;
        assert!(bar[0] >= -1.0 && bar[0] + bar[2] <= w + 1.0, "shell within width: {bar:?}");
    }
}

// ---- Label -------------------------------------------------------------------

#[derive(Default, Clone)]
struct LabelProbe;

fn label_view(_s: &LabelProbe, _cx: &mut Cx<LabelProbe>) -> El {
    col()
        .w(320.0)
        .h(200.0)
        .padding(16.0)
        .gap(12.0)
        .child(Label::new("lbl-a", "Name").required(true).hint("Shown on profile").build(true))
        .child(Label::new("lbl-b", "Notes").build(true))
}

#[test]
fn label_renders_text_required_marker_and_hint() {
    let mut rt = Runtime::new(LabelProbe, label_view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert_eq!(element(&rt, "lbl-a-text").text.as_deref(), Some("Name"));
    assert_eq!(element(&rt, "lbl-a-required").text.as_deref(), Some("*"));
    assert_eq!(element(&rt, "lbl-a-hint").text.as_deref(), Some("Shown on profile"));
    assert!(has_key(&rt, "lbl-b-text"));
    assert!(!has_key(&rt, "lbl-b-required"), "no required marker when not required");
    assert!(!has_key(&rt, "lbl-b-hint"), "no hint when absent");
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let mut rt = Runtime::new(LabelProbe, label_view, (w, h), env());
        rt.frame(0.0);
        assert_in_window(&rt, w, h, &format!("label {w}x{h}"));
    }
}
