//! T007: `ConnectionAdvancedSettings` — real interaction and geometry probes.
//!
//! Everything below drives the rendered element tree through `Runtime`: chevron
//! expansion/collapse, kit `Select` choices, the read-only checkbox, conditional
//! ErrorBanner/alert rendering, the open-settings callback and i18n key echo.
//! Geometry is checked at both window sizes in both themes.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime};

use crate::components::connection::connection_advanced_settings::{
    AdvancedAction, AdvancedSettingsVm, ConnectionAdvancedSettings, SavedTunnel, SelectKind,
    TunnelKind, TunnelSource, COLOR_TAGS,
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

    /// Apply an action the way the real host would, so a second render shows it.
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

fn has_key<S>(rt: &Runtime<S>, k: &str) -> bool {
    // Rune retains a removed node for a frame or two with `exiting: true`
    // (presence animation); a collapsed panel must not count as present.
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(k) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, k: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(k) && !e.exiting)
        .unwrap_or_else(|| panic!("{k} is laid out"))
}

fn click<S>(rt: &mut Runtime<S>, k: &str) {
    let (x, y) = rt.find(k).unwrap_or_else(|| panic!("{k} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

fn expanded_vm() -> AdvancedSettingsVm {
    let mut vm = AdvancedSettingsVm::default();
    vm.show_advanced = true;
    vm.show_tunnel = true;
    vm.group_options = Some(vec![
        ("g1".to_owned(), "Group A".to_owned()),
        ("g2".to_owned(), "Group B".to_owned()),
    ]);
    vm
}

// ---- Section expansion / collapse via the chevron ------------------------------------------

#[test]
fn the_advanced_section_expands_and_collapses_through_its_chevron() {
    let mut rt = runtime(AdvancedSettingsVm::default(), true, (960.0, 640.0));

    // Starts collapsed: right chevron, no panel.
    let toggle = key("advanced-toggle");
    let chevron = key("advanced-toggle-chevron");
    assert_eq!(element(&rt, &toggle).expanded, Some(false));
    assert!(!has_key(&rt, &key("advanced-panel")), "panel not mounted");
    assert!(has_key(&rt, &chevron));

    // Click the chevron child (the press bubbles to the toggle row).
    click(&mut rt, &chevron);
    assert_eq!(
        rt.state.vm.show_advanced, true,
        "chevron click dispatches ToggleAdvanced"
    );
    assert!(has_key(&rt, &key("advanced-panel")), "panel mounted");
    assert_eq!(element(&rt, &toggle).expanded, Some(true));

    click(&mut rt, &chevron);
    assert!(!rt.state.vm.show_advanced, "second click collapses again");
    assert!(!has_key(&rt, &key("advanced-panel")));
    assert_eq!(element(&rt, &toggle).expanded, Some(false));
}

#[test]
fn the_tunnel_section_expands_and_collapses_through_its_chevron() {
    let mut rt = runtime(AdvancedSettingsVm::default(), true, (960.0, 640.0));
    let chevron = key("tunnel-toggle-chevron");
    assert_eq!(element(&rt, &key("tunnel-toggle")).expanded, Some(false));
    assert!(!has_key(&rt, &key("tunnel-panel")));

    click(&mut rt, &chevron);
    assert!(rt.state.vm.show_tunnel, "chevron opens the tunnel section");
    assert!(has_key(&rt, &key("tunnel-panel")));
    assert_eq!(element(&rt, &key("tunnel-toggle")).expanded, Some(true));

    click(&mut rt, &chevron);
    assert!(!rt.state.vm.show_tunnel);
    assert!(!has_key(&rt, &key("tunnel-panel")));
}

#[test]
fn the_tunnel_badge_appears_only_with_an_effective_kind() {
    let mut vm = AdvancedSettingsVm::default();
    vm.effective_tunnel_kind = Some(TunnelKind::Ssh);
    let mut rt = runtime(vm, true, (960.0, 640.0));
    let badge = key("tunnel-badge");
    assert_eq!(element(&rt, &badge).text.as_deref(), Some("SSH"));

    let mut plain = runtime(AdvancedSettingsVm::default(), true, (960.0, 640.0));
    assert!(!has_key(&mut plain, &badge), "no badge without a kind");
}

// ---- Select interactivity -------------------------------------------------------------------

#[test]
fn the_ssl_select_opens_by_pointer_and_a_row_choice_writes_the_value() {
    let vm = expanded_vm();
    let mut rt = runtime(vm, true, (960.0, 640.0));
    let trigger = key("ssl-mode-field-trigger");
    assert!(has_key(&rt, &trigger), "the SSL select trigger is built");

    click(&mut rt, &trigger);
    assert_eq!(
        rt.state.vm.open,
        Some(SelectKind::SslMode),
        "the trigger opens via on_toggle"
    );
    assert!(has_key(&rt, "option2"), "the listbox is mounted");

    click(&mut rt, "option2");
    assert_eq!(rt.state.vm.ssl_mode, "require", "the stored value (not the label)");
    assert_eq!(rt.state.vm.open, None, "a choice closes the listbox");
}

#[test]
fn the_group_select_accepts_a_keyboard_choice() {
    let vm = expanded_vm();
    let mut rt = runtime(vm, true, (960.0, 640.0));
    click(&mut rt, &key("group-field-trigger"));
    assert_eq!(rt.state.vm.open, Some(SelectKind::Group));

    // Pointer open leaves the highlight invisible; two Downs reveal then move it.
    for _ in 0..2 {
        rt.key(KeyEvent {
            key: Key::Down,
            mods: Mods::default(),
        });
        rt.frame(1.0 / 60.0);
    }
    rt.key(KeyEvent {
        key: Key::Enter,
        mods: Mods::default(),
    });
    rt.frame(1.0 / 60.0);
    assert_eq!(rt.state.vm.group, "g2", "Enter picks the second group's value");
    assert_eq!(rt.state.vm.open, None);
}

#[test]
fn the_tunnel_source_and_kind_selects_write_their_enums() {
    let vm = expanded_vm();
    let mut rt = runtime(vm, true, (960.0, 640.0));

    // With no saved tunnels the disabled "saved" row is left out: none + inline.
    click(&mut rt, &key("tunnel-source-field-trigger"));
    assert!(has_key(&rt, "option1"), "two options only");
    assert!(!has_key(&rt, "option2"), "no saved-tunnel row");
    click(&mut rt, "option1");
    assert_eq!(rt.state.vm.tunnel_source, TunnelSource::Inline);
    assert!(has_key(&rt, &key("inline-tunnel")), "inline branch renders");

    // Kind options: SSH, HTTP proxy, WebSocket (supports_ssh is true).
    click(&mut rt, &key("tunnel-kind-field-trigger"));
    assert!(has_key(&rt, "option2"));
    click(&mut rt, "option2");
    assert_eq!(rt.state.vm.tunnel_kind, TunnelKind::WebSocket);
    assert_eq!(rt.state.vm.effective_tunnel_kind, Some(TunnelKind::WebSocket));
}

#[test]
fn the_read_only_checkbox_is_interactive_and_a_locked_driver_disables_it() {
    let mut vm = expanded_vm();
    let mut rt = runtime(vm.clone(), true, (960.0, 640.0));
    let row = key("read-only");
    assert!(element(&rt, &row).focusable, "checkbox is a tab stop");
    click(&mut rt, &row);
    assert!(rt.state.vm.read_only, "checkbox toggles the VM");
    assert!(has_key(&rt, &key("read-only")), "stable key after toggle");

    vm.driver_read_only = true;
    let mut locked = runtime(vm, true, (960.0, 640.0));
    assert!(!element(&locked, &row).focusable, "locked driver: not a tab stop");
    click(&mut locked, &row);
    assert!(!locked.state.vm.read_only, "a disabled checkbox never flips");
}

// ---- Conditional rendering ------------------------------------------------------------------

#[test]
fn the_error_banner_renders_only_with_a_tunnel_error() {
    let mut vm = expanded_vm();
    vm.tunnel_source = TunnelSource::Saved;
    vm.saved_tunnels = vec![SavedTunnel {
        id: "t1".to_owned(),
        name: "prod".to_owned(),
        kind: "ssh".to_owned(),
    }];
    vm.saved_tunnel = Some(vm.saved_tunnels[0].clone());
    vm.tunnel_id = Some("t1".to_owned());

    let mut clean = runtime(vm.clone(), true, (960.0, 640.0));
    let banner = key("tunnel-error-saved");
    assert!(!has_key(&clean, &banner), "no error → no banner");
    assert!(has_key(&clean, &key("saved-tunnel-summary")), "summary shown");

    vm.tunnel_error = "auth failed".to_owned();
    let mut failed = runtime(vm, true, (960.0, 640.0));
    let b = element(&failed, &banner);
    assert_eq!(b.role, "alert", "ErrorBanner keeps its alert role");
    assert!(texts(&failed).iter().any(|x| x == "auth failed"));
}

#[test]
fn the_inline_branch_shows_the_boundary_marker_and_only_enables_save_when_valid() {
    let mut vm = expanded_vm();
    vm.tunnel_source = TunnelSource::Inline;
    let mut rt = runtime(vm.clone(), true, (960.0, 640.0));
    assert!(has_key(&rt, &key("inline-fields-boundary")), "field boundary observable");
    assert!(has_key(&rt, &key("tunnel-save-as")));
    assert!(!has_key(&rt, &key("tunnel-error-inline")), "no error → no span banner");

    vm.tunnel_inline_valid = false;
    vm.tunnel_error = "bad config".to_owned();
    let mut invalid = runtime(vm, true, (960.0, 640.0));
    let span_banner = element(&invalid, &key("tunnel-error-inline"));
    assert_eq!(span_banner.role, "alert");
    assert!(texts(&invalid).iter().any(|x| x == "bad config"));
}

#[test]
fn the_tunnel_missing_alert_is_amber_and_only_mentions_alternatives_when_listed() {
    let mut vm = expanded_vm();
    vm.tunnel_ref_missing = true;
    vm.tunnel_source = TunnelSource::Saved;
    let mut none_listed = runtime(vm.clone(), true, (960.0, 640.0));
    let alert = key("tunnel-missing");
    assert_eq!(element(&none_listed, &alert).role, "alert");
    let msg = element(&none_listed, &key("tunnel-missing-text"))
        .text
        .unwrap();
    assert_eq!(msg, t("newConn.tunnelMissing"), "no alternative sentence");
    assert!(!has_key(&none_listed, &key("tunnel-unbind")), "dead unbind not offered");

    vm.saved_tunnels = vec![SavedTunnel {
        id: "t2".to_owned(),
        name: "staging".to_owned(),
        kind: "httpProxy".to_owned(),
    }];
    let mut listed = runtime(vm, true, (960.0, 640.0));
    let msg = element(&listed, &key("tunnel-missing-text")).text.unwrap();
    assert_eq!(
        msg,
        format!("{} {}", t("newConn.tunnelMissing"), t("newConn.tunnelMissingAlt"))
    );
}

#[test]
fn the_empty_tunnel_actions_fire_create_and_open_settings() {
    let mut rt = runtime(expanded_vm(), true, (960.0, 640.0));
    assert!(has_key(&rt, &key("tunnel-empty")));

    click(&mut rt, &key("tunnel-manage-entry"));
    assert!(
        rt.state.actions.contains(&AdvancedAction::OpenSettings),
        "Manage dispatches OpenSettings (openSettingsWindow('tunnels'))"
    );

    click(&mut rt, &key("tunnel-create-entry"));
    assert_eq!(rt.state.vm.tunnel_source, TunnelSource::Inline);
    assert!(rt.state.actions.contains(&AdvancedAction::CreateEntry));
}

#[test]
fn colour_swatches_are_distinct_keys_and_report_their_value() {
    let mut rt = runtime(expanded_vm(), true, (960.0, 640.0));
    assert_eq!(COLOR_TAGS.len(), 7);
    for i in 0..COLOR_TAGS.len() {
        assert!(has_key(&rt, &key(&format!("color-{i}"))), "swatch {i} exists");
    }
    click(&mut rt, &key("color-3"));
    assert_eq!(rt.state.vm.color_tag, COLOR_TAGS[3].0, "swatch reports its hex value");
}

// ---- i18n ----------------------------------------------------------------------------------

#[test]
fn every_key_used_renders_chinese_and_no_key_is_echoed() {
    let mut vm = expanded_vm();
    vm.tunnel_source = TunnelSource::Saved;
    vm.saved_tunnels = vec![SavedTunnel {
        id: "t1".to_owned(),
        name: "prod".to_owned(),
        kind: "ssh".to_owned(),
    }];
    vm.tunnel_error = "err".to_owned();
    let rt = runtime(vm, true, (960.0, 640.0));

    let keys = [
        "newConn.advanced",
        "newConn.sslMode",
        "newConn.sslNone",
        "newConn.sslPrefer",
        "newConn.sslRequire",
        "newConn.readOnly",
        "newConn.readOnlyHint",
        "newConn.colorTag",
        "newConn.group",
        "newConn.tunnel",
        "newConn.tunnelSource",
        "newConn.tunnelNone",
        "newConn.savedTunnel",
        "newConn.savedTunnelNone",
        "newConn.tunnelEmptyHint",
        "newConn.tunnelCreateEntry",
        "newConn.tunnelManage",
        "newConn.tunnelSavedHint",
        "newConn.tunnelUnbind",
        "newConn.tunnelKind",
        "newConn.tunnelSsh",
        "newConn.tunnelHttpProxy",
        "newConn.tunnelWebSocket",
        "newConn.tunnelSaveAs",
        "newConn.colorRed",
        "newConn.colorGray",
    ];
    for k in keys {
        let s = t(k);
        assert!(s != k, "{k} must resolve to a translation");
        assert!(
            s.chars().any(|c| c.is_ascii() == false),
            "{k} should carry Chinese text, got {s:?}"
        );
    }

    let rendered = texts(&rt);
    for text in &rendered {
        assert!(
            !text.contains("newConn."),
            "a raw i18n key leaked into the tree: {text:?}"
        );
    }
    assert!(
        rendered.iter().any(|x| x == t("newConn.advanced")),
        "the translated label is visible"
    );
}

// ---- Geometry at both windows in both themes ----------------------------------------------

fn assert_inside_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) outside {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[test]
fn every_branch_fits_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            // Collapsed: just the two toggles.
            let collapsed = runtime(AdvancedSettingsVm::default(), dark, (w, h));
            assert_inside_window(
                &collapsed,
                w,
                h,
                &format!("collapsed dark={dark} {w}x{h}"),
            );

            // Advanced section expanded, all selects closed.
            let mut advanced = AdvancedSettingsVm::default();
            advanced.show_advanced = true;
            advanced.group_options = Some(vec![
                ("g1".to_owned(), "A very long group name that must not overflow".to_owned()),
            ]);
            let adv = runtime(advanced, dark, (w, h));
            assert_inside_window(&adv, w, h, &format!("advanced dark={dark} {w}x{h}"));

            // Tunnel section expanded with the empty state and badge.
            let mut tunnel = AdvancedSettingsVm::default();
            tunnel.show_tunnel = true;
            tunnel.effective_tunnel_kind = Some(TunnelKind::HttpProxy);
            let tun = runtime(tunnel, dark, (w, h));
            assert_inside_window(&tun, w, h, &format!("tunnel dark={dark} {w}x{h}"));

            // Saved branch with summary, error banner and missing alert.
            let mut saved = expanded_vm();
            saved.tunnel_source = TunnelSource::Saved;
            saved.saved_tunnels = vec![SavedTunnel {
                id: "t1".to_owned(),
                name: "a-long-production-tunnel-name".to_owned(),
                kind: "httpProxy".to_owned(),
            }];
            saved.saved_tunnel = Some(saved.saved_tunnels[0].clone());
            saved.tunnel_id = Some("t1".to_owned());
            saved.tunnel_error = "a fairly long error message for the banner".to_owned();
            saved.tunnel_ref_missing = true;
            let sv = runtime(saved, dark, (w, h));
            assert_inside_window(&sv, w, h, &format!("saved dark={dark} {w}x{h}"));

            // Inline branch with its error banner.
            let mut inline = expanded_vm();
            inline.tunnel_source = TunnelSource::Inline;
            inline.tunnel_error = "another long inline error message".to_owned();
            let inl = runtime(inline, dark, (w, h));
            assert_inside_window(&inl, w, h, &format!("inline dark={dark} {w}x{h}"));
        }
    }
}

#[test]
fn no_element_wraps_into_a_second_row_when_expanded_at_minimum_width() {
    // A focused check that root children never exceed the window even with both
    // panels open and a long group label (the regression this port guards).
    let mut vm = expanded_vm();
    vm.group_options = Some(vec![(
        "g1".to_owned(),
        "an-exceedingly-long-group-name-for-the-minimum-window".to_owned(),
    )]);
    vm.tunnel_source = TunnelSource::Inline;
    vm.tunnel_error = "long inline error".to_owned();
    let rt = runtime(vm, true, (960.0, 640.0));
    let root = element(&rt, PREFIX);
    assert!(
        root.rect[2] <= 960.5,
        "root must not grow past the 960pt window: {:?}",
        root.rect
    );
    assert_inside_window(&rt, 960.0, 640.0, "fully expanded minimum window");
}
