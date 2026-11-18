//! Independent tester audit for T007 `ConnectionAdvancedSettings`.
//!
//! This file deliberately does NOT reuse the implementation's own test helpers.
//! Its observation channel is the raw `Runtime::elements()` list plus
//! `Runtime::find()` / `Runtime::element_at()`, which rune documents as skipping
//! `exiting` (and their children). That lets us prove, independently of the
//! coder's `!e.exiting` filter, that a collapsed section's controls are really
//! gone from the active tree rather than merely faded out.
//!
//! Covers: collapse/unmount/re-expand, section independence, state in `S`,
//! boundary callbacks firing exactly once, i18n echo + section order, unique
//! keys and rect-based geometry at {960x640, 1440x816} x {dark, light}.

use rune::{Cx, El, Env, Runtime};

use crate::components::connection::connection_advanced_settings::{
    AdvancedAction, AdvancedSettingsVm, ConnectionAdvancedSettings, SavedTunnel, TunnelKind,
    TunnelSource, COLOR_TAGS,
};
use crate::i18n::t;
use crate::ui::kit::kit;

const PREFIX: &str = "new-conn";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

/// The app state: the VM plus a log of every dispatched action.
#[derive(Default)]
struct Probe {
    vm: AdvancedSettingsVm,
    actions: Vec<AdvancedAction>,
}

impl Probe {
    fn with(vm: AdvancedSettingsVm) -> Self {
        Probe {
            vm,
            actions: Vec::new(),
        }
    }

    fn apply(&mut self, a: AdvancedAction) {
        match a.clone() {
            AdvancedAction::ToggleAdvanced => self.vm.show_advanced = !self.vm.show_advanced,
            AdvancedAction::ToggleTunnel => self.vm.show_tunnel = !self.vm.show_tunnel,
            AdvancedAction::OpenSelect(kind) => self.vm.open = kind,
            AdvancedAction::SslMode(v) => self.vm.ssl_mode = v,
            AdvancedAction::ReadOnly(v) => self.vm.read_only = v,
            AdvancedAction::ColorTag(v) => self.vm.color_tag = v,
            AdvancedAction::Group(v) => self.vm.group = v,
            AdvancedAction::TunnelSource(v) => self.vm.tunnel_source = v,
            AdvancedAction::TunnelKind(v) => {
                self.vm.tunnel_kind = v;
                self.vm.effective_tunnel_kind = Some(v);
            }
            AdvancedAction::TunnelId(v) => self.vm.tunnel_id = v,
            AdvancedAction::CreateEntry => self.vm.tunnel_source = TunnelSource::Inline,
            // Boundary actions are recorded but have no in-scope state effect.
            AdvancedAction::OpenSettings | AdvancedAction::Unbind | AdvancedAction::SaveAs => {}
        }
        self.actions.push(a);
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let page = ConnectionAdvancedSettings::<Probe>::new(s.vm.clone())
        .key(PREFIX)
        .on_action(|st: &mut Probe, a| st.apply(a))
        .build(kit(s.vm.is_window), &mut layer);
    rune_kit::root(page, layer)
}

fn runtime(vm: AdvancedSettingsVm, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(Probe::with(vm), view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn key(k: &str) -> String {
    format!("{PREFIX}-{k}")
}

/// Independent observation channels — raw, unfiltered.
fn all_with_key<S>(rt: &Runtime<S>, k: &str) -> Vec<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .filter(|e| e.key.as_deref() == Some(k))
        .collect()
}

fn active_count<S>(rt: &Runtime<S>, k: &str) -> usize {
    all_with_key(rt, k).iter().filter(|e| !e.exiting).count()
}

fn findable<S>(rt: &Runtime<S>, k: &str) -> bool {
    // `find` skips exiting elements AND their children, so a removed panel's
    // controls are unreachable through it even if a node is briefly retained.
    rt.find(k).is_some()
}

fn click<S>(rt: &mut Runtime<S>, k: &str) {
    let (x, y) = rt.find(k).unwrap_or_else(|| panic!("{k} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn advanced_open_vm() -> AdvancedSettingsVm {
    let mut vm = AdvancedSettingsVm::default();
    vm.show_advanced = true;
    vm
}

fn both_open_vm() -> AdvancedSettingsVm {
    let mut vm = advanced_open_vm();
    vm.show_tunnel = true;
    vm
}

fn all_rendered_texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

// ---- CRITICAL: collapsed content is unmounted from the live tree --------------------------------

#[test]
fn collapsing_a_section_unmounts_its_controls_from_the_active_tree() {
    let mut vm = advanced_open_vm();
    vm.group_options = Some(vec![("g1".to_owned(), "Group A".to_owned())]);
    let mut rt = runtime(vm, true, (960.0, 640.0));

    // While open, every advanced-section control is present and interactive.
    let controls = [
        key("advanced-panel"),
        key("ssl-mode-field-trigger"),
        key("read-only"),
        key("color-tag"),
        key("color-0"),
        key("color-6"),
        key("group-label"),
    ];
    for c in &controls {
        assert_eq!(active_count(&rt, c), 1, "{c} mounted while expanded");
        assert!(findable(&rt, c), "{c} findable (interactive) while expanded");
    }
    assert_eq!(element_expanded(&rt, &key("advanced-toggle")), Some(true));

    // Collapse via the chevron (the press bubbles to the toggle row).
    click(&mut rt, &key("advanced-toggle-chevron"));
    assert!(!rt.state.vm.show_advanced, "collapse dispatched");
    assert_eq!(element_expanded(&rt, &key("advanced-toggle")), Some(false));

    // Independent proof #1: rune's `find` treats exiting nodes (and their
    // children) as gone. None of the collapsed controls are reachable.
    for c in &controls {
        assert!(!findable(&rt, c), "{c} still reachable after collapse");
    }

    // Independent proof #2: inspect the RAW element list. Any retained node
    // must be `exiting` AND `inert` — never a live, interactive control.
    for c in &controls {
        for e in all_with_key(&rt, c) {
            assert!(
                e.exiting && e.inert,
                "{c} lingered but is live/interactive: exiting={} inert={} role={}",
                e.exiting, e.inert, e.role
            );
        }
    }

    // Independent proof #3: they do not linger forever. After enough frames
    // the keys are absent from the tree entirely (fully unmounted).
    for _ in 0..120 {
        rt.frame(1.0 / 60.0);
    }
    for c in &controls {
        assert!(
            all_with_key(&rt, c).is_empty(),
            "{c} still mounted after the exit animation"
        );
        assert!(!findable(&rt, c));
    }
}

fn element_expanded<S>(rt: &Runtime<S>, k: &str) -> Option<bool> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(k) && !e.exiting)
        .and_then(|e| e.expanded)
}

#[test]
fn reexpanding_restores_controls_and_the_two_sections_are_independent() {
    let mut rt = runtime(AdvancedSettingsVm::default(), true, (960.0, 640.0));

    // Open the tunnel section only.
    click(&mut rt, &key("tunnel-toggle"));
    assert!(rt.state.vm.show_tunnel);
    assert_eq!(active_count(&rt, &key("tunnel-panel")), 1);
    assert!(findable(&rt, &key("tunnel-source-field-trigger")));
    assert_eq!(active_count(&rt, &key("advanced-panel")), 0);
    assert!(!findable(&rt, &key("read-only")));

    // Open advanced too — tunnel stays open (independent state).
    click(&mut rt, &key("advanced-toggle"));
    assert!(rt.state.vm.show_advanced);
    assert_eq!(active_count(&rt, &key("advanced-panel")), 1);
    assert_eq!(active_count(&rt, &key("tunnel-panel")), 1);
    assert!(findable(&rt, &key("read-only")));
    assert!(findable(&rt, &key("tunnel-source-field-trigger")));

    // Collapse advanced again — tunnel must remain open and interactive.
    click(&mut rt, &key("advanced-toggle"));
    assert!(!rt.state.vm.show_advanced);
    assert!(!findable(&rt, &key("read-only")));
    assert_eq!(active_count(&rt, &key("advanced-panel")), 0);
    assert_eq!(active_count(&rt, &key("tunnel-panel")), 1, "tunnel unaffected");
    assert!(findable(&rt, &key("tunnel-source-field-trigger")));

    // Re-expand restores advanced controls; tunnel is still open.
    click(&mut rt, &key("advanced-toggle"));
    assert_eq!(active_count(&rt, &key("advanced-panel")), 1);
    assert!(findable(&rt, &key("read-only")));
    assert!(findable(&rt, &key("ssl-mode-field-trigger")));
    assert_eq!(active_count(&rt, &key("tunnel-panel")), 1);
}

#[test]
fn collapsed_controls_do_not_receive_a_pointer_press() {
    let mut rt = runtime(advanced_open_vm(), true, (960.0, 640.0));
    let (x, y) = rt.find(&key("color-6")).expect("swatch laid out while open");

    click(&mut rt, &key("advanced-toggle-chevron"));
    // The old swatch position must not pick the (now removed) swatch.
    let picked = rt.element_at(x, y).map(|p| p.key().map(str::to_owned));
    assert_ne!(picked, Some(Some(key("color-6"))), "collapsed swatch is not hit-testable");
    let before = rt.state.vm.color_tag.clone();
    // A press at the old coordinates cannot dispatch ColorTag.
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.vm.color_tag, before, "collapsed swatch cannot change state");
}

// ---- State ownership ---------------------------------------------------------------------------

#[test]
fn state_lives_in_s_two_instances_are_independent_and_survive_reframes() {
    let vm = AdvancedSettingsVm::default();
    let mut a = runtime(vm.clone(), true, (960.0, 640.0));
    let mut b = runtime(vm, false, (1440.0, 816.0));

    click(&mut a, &key("advanced-toggle"));
    assert!(a.state.vm.show_advanced, "A toggled");
    assert!(!b.state.vm.show_advanced, "B did not inherit A's state");
    assert_eq!(active_count(&b, &key("advanced-panel")), 0);

    // Reframes (view() re-runs each frame) must not lose or duplicate state.
    for _ in 0..12 {
        a.frame(1.0 / 60.0);
        b.frame(1.0 / 60.0);
    }
    assert!(a.state.vm.show_advanced, "A survives reframes");
    assert_eq!(active_count(&a, &key("advanced-panel")), 1);
    assert_eq!(a.state.actions.iter().filter(|x| **x == AdvancedAction::ToggleAdvanced).count(), 1);
    assert!(!b.state.vm.show_advanced, "B still untouched");
}

// ---- Boundary honesty: callbacks fire exactly once --------------------------------------------

#[test]
fn boundary_callbacks_fire_exactly_once_per_press() {
    let mut rt = runtime(both_open_vm(), true, (960.0, 640.0));

    click(&mut rt, &key("tunnel-manage-entry"));
    assert_eq!(
        rt.state.actions.iter().filter(|a| **a == AdvancedAction::OpenSettings).count(),
        1,
        "Manage fires OpenSettings exactly once per press"
    );
    click(&mut rt, &key("tunnel-manage-entry"));
    assert_eq!(
        rt.state.actions.iter().filter(|a| **a == AdvancedAction::OpenSettings).count(),
        2,
        "a second press fires exactly one more"
    );

    let mut vm = both_open_vm();
    vm.tunnel_source = TunnelSource::Inline;
    let mut inline = runtime(vm, true, (960.0, 640.0));
    click(&mut inline, &key("tunnel-save-as"));
    assert_eq!(
        inline.state.actions.iter().filter(|a| **a == AdvancedAction::SaveAs).count(),
        1,
        "Save-as fires SaveAs exactly once"
    );

    // The not-yet-ported inline fields are an honest, inert boundary marker:
    // observable, but not a fake interactive control.
    let markers = all_with_key(&inline, &key("inline-fields-boundary"));
    assert_eq!(markers.len(), 1, "boundary marker present");
    assert!(!markers[0].focusable, "boundary marker is not a tab stop");
    assert_ne!(markers[0].role, "button", "boundary marker is not a fake button");
}

// ---- i18n, section order, colour mapping -------------------------------------------------------

#[test]
fn section_order_matches_react_and_no_i18n_key_is_echoed() {
    let mut vm = all_branches_vm();
    vm.show_advanced = true;
    vm.show_tunnel = true;
    let rt = runtime(vm, true, (960.0, 640.0));

    let adv_y = rt.find(&key("advanced-toggle-label")).map(|p| p.1);
    let tun_y = rt.find(&key("tunnel-toggle-label")).map(|p| p.1);
    assert!(adv_y.is_some() && tun_y.is_some(), "both section labels visible");
    assert!(adv_y.unwrap() < tun_y.unwrap(), "advanced section precedes tunnel (React order)");

    let texts = all_rendered_texts(&rt);
    for text in &texts {
        assert!(!text.contains("newConn."), "a raw i18n key was echoed: {text:?}");
    }
    assert!(texts.iter().any(|x| x == t("newConn.advanced")));
    assert!(texts.iter().any(|x| x == t("newConn.tunnel")));
    assert!(texts.iter().any(|x| x == t("newConn.tunnelSource")));
}

fn all_branches_vm() -> AdvancedSettingsVm {
    let mut vm = both_open_vm();
    vm.group_options = Some(vec![("g1".to_owned(), "Group A".to_owned())]);
    vm.effective_tunnel_kind = Some(TunnelKind::Ssh);
    vm.tunnel_ref_missing = true;
    vm.tunnel_error = "an error".to_owned();
    vm.saved_tunnels = vec![SavedTunnel {
        id: "t1".to_owned(),
        name: "prod".to_owned(),
        kind: "ssh".to_owned(),
    }];
    vm
}

#[test]
fn all_seven_colour_swatches_carry_the_react_i18n_key_and_distinct_keys() {
    let rt = runtime(advanced_open_vm(), true, (960.0, 640.0));
    assert_eq!(COLOR_TAGS.len(), 7);
    for (i, (_value, i18n_key)) in COLOR_TAGS.iter().enumerate() {
        let swatches = all_with_key(&rt, &key(&format!("color-{i}")));
        assert_eq!(swatches.len(), 1, "swatch {i} has one key");
        assert_eq!(
            swatches[0].label.as_deref(),
            Some(t(i18n_key)),
            "swatch {i} uses React's {i18n_key}"
        );
        assert!(swatches[0].label.as_ref().is_some_and(|l| l.chars().any(|c| !c.is_ascii())));
    }
}

#[test]
fn active_keys_are_unique_with_both_sections_expanded() {
    let rt = runtime(all_branches_vm(), true, (960.0, 640.0));
    let mut keys: Vec<String> = rt
        .elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key)
        .filter(|k| k.starts_with(PREFIX))
        .collect();
    keys.sort();
    for pair in keys.windows(2) {
        assert_ne!(pair[0], pair[1], "duplicate component key: {}", pair[0]);
    }
}

// ---- Geometry: rect-based observation at both windows in both themes --------------------------

fn assert_rects_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(outside.is_empty(), "{label}: {} node(s) outside {w}x{h}: {outside:?}", outside.len());
}

#[test]
fn every_state_fits_both_windows_in_both_themes_by_rect() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let collapsed = runtime(AdvancedSettingsVm::default(), dark, (w, h));
            assert_rects_inside(&collapsed, w, h, &format!("collapsed dark={dark}"));

            let advanced = runtime(advanced_open_vm(), dark, (w, h));
            assert_rects_inside(&advanced, w, h, &format!("advanced dark={dark}"));

            let both = runtime(all_branches_vm(), dark, (w, h));
            assert_rects_inside(&both, w, h, &format!("all branches dark={dark}"));

            let mut inline = both_open_vm();
            inline.tunnel_source = TunnelSource::Inline;
            inline.tunnel_error = "a long inline error message that must stay in the window".to_owned();
            let inline_rt = runtime(inline, dark, (w, h));
            assert_rects_inside(&inline_rt, w, h, &format!("inline dark={dark}"));
        }
    }
}

#[test]
fn an_expand_collapse_cycle_keeps_every_rect_inside_the_minimum_window() {
    let mut rt = runtime(both_open_vm(), true, (960.0, 640.0));
    for _ in 0..3 {
        click(&mut rt, &key("advanced-toggle-chevron"));
        click(&mut rt, &key("tunnel-toggle-chevron"));
        assert_rects_inside(&rt, 960.0, 640.0, "after expand/collapse cycle");
    }
}
