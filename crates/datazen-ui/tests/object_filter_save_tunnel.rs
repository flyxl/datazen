//! T260: `ObjectFilterDialog`, `SaveTunnelDialog` and `shared` — keys, validation,
//! host-port callbacks, error banner and two-window/two-palette geometry.

use rune::{col, Cx, El, Env, Runtime};
use rune_kit::root;

use datazen_ui::components::connection::object_filter_dialog::{
    ObjectFilterAction, ObjectFilterDialog, ObjectFilterPrefs, ObjectFilterVm,
};
use datazen_ui::components::connection::save_tunnel_dialog::{
    SaveTunnelAction, SaveTunnelDialog, SaveTunnelVm,
};
use datazen_ui::components::connection::shared::{new_id, COLOR_KEYS};
use datazen_ui::i18n::t;

struct Probe {
    dark: bool,
    object: ObjectFilterVm,
    tunnel: SaveTunnelVm,
    object_actions: Vec<ObjectFilterAction>,
    tunnel_actions: Vec<SaveTunnelAction>,
}

fn probe() -> Probe {
    Probe {
        dark: true,
        object: ObjectFilterVm::default(),
        tunnel: SaveTunnelVm::default(),
        object_actions: Vec::new(),
        tunnel_actions: Vec::new(),
    }
}

fn object_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let dialog = ObjectFilterDialog::<Probe>::new(&s.object)
        .on_vm(|st: &mut Probe| &mut st.object)
        .on_action(|st: &mut Probe, a| st.object_actions.push(a))
        .build(s.dark, &mut layer);
    root(col().w_full().h_full().child(dialog), layer)
}

fn tunnel_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let dialog = SaveTunnelDialog::<Probe>::new(&s.tunnel)
        .on_vm(|st: &mut Probe| &mut st.tunnel)
        .on_action(|st: &mut Probe, a| st.tunnel_actions.push(a))
        .build(s.dark, &mut layer);
    root(col().w_full().h_full().child(dialog), layer)
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn runtime(view: fn(&Probe, &mut Cx<Probe>) -> El, p: Probe, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(p, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn type_into(rt: &mut Runtime<Probe>, key: &str, text: &str) {
    click(rt, key);
    rt.paste(text);
    rt.frame(0.0);
}

fn refresh(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    rt.frame(0.0);
}

// ---- shared ----------------------------------------------------------------------------------

#[test]
fn shared_helpers_preserve_react_contract() {
    let a = new_id();
    let b = new_id();
    assert!(a.starts_with("conn_"), "React's id prefix: {a}");
    assert_ne!(a, b, "each id is fresh");
    assert_eq!(a.len(), 13, "conn_ + 8 hex digits");
    assert_eq!(COLOR_KEYS.len(), 7);
    for (value, key) in COLOR_KEYS {
        assert!(value.starts_with('#') && value.len() == 7, "hex data {value}");
        assert!(t(key) != key, "{key} resolves to a translation");
    }
}

// ---- ObjectFilterDialog -----------------------------------------------------------------------

#[test]
fn object_filter_closed_has_no_panel() {
    let rt = runtime(object_view, probe(), true, (960.0, 640.0));
    assert!(has(&rt, "object-filter-dialog-root"));
    assert!(!has(&rt, "object-filter-dialog"));
    assert!(!has(&rt, "object-filter-save"));
}

#[test]
fn object_filter_open_shows_exact_keys_and_i18n() {
    let mut p = probe();
    p.object.open(&ObjectFilterPrefs {
        hide_system_schemas: true,
        table_name_include: "app_*".to_owned(),
        table_name_exclude: "*_tmp".to_owned(),
    });
    let rt = runtime(object_view, p, true, (960.0, 640.0));
    for key in [
        "object-filter-dialog",
        "object-filter-hide-system",
        "object-filter-include",
        "object-filter-exclude",
        "object-filter-hint",
        "object-filter-cancel",
        "object-filter-save",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in [
        "common.objectFilter",
        "objectFilter.hideSystemSchemas",
        "objectFilter.include",
        "objectFilter.exclude",
        "objectFilter.hint",
        "common.cancel",
        "common.save",
    ] {
        assert!(rendered.iter().any(|x| x == t(key)), "{key}: exact translated text");
    }
    assert_eq!(rt.state.object.table_name_include.text, "app_*");
    assert_eq!(rt.state.object.table_name_exclude.text, "*_tmp");
    assert!(rt.state.object.hide_system_schemas);
}

#[test]
fn object_filter_checkbox_edits_local_state_and_save_sends_trimmed_prefs() {
    let mut p = probe();
    p.object.open(&ObjectFilterPrefs::default());
    let mut rt = runtime(object_view, p, true, (1440.0, 816.0));

    click(&mut rt, "object-filter-hide-system");
    assert_eq!(
        rt.state.object_actions.last(),
        Some(&ObjectFilterAction::ToggleHide(true)),
        "checkbox is a local edit, not a save"
    );
    rt.state.object.hide_system_schemas = true;
    refresh(&mut rt);

    type_into(&mut rt, "object-filter-include", "  app_*  ");
    type_into(&mut rt, "object-filter-exclude", "  *_tmp  ");
    click(&mut rt, "object-filter-save");
    assert_eq!(
        rt.state.object_actions.last(),
        Some(&ObjectFilterAction::Save(ObjectFilterPrefs {
            hide_system_schemas: true,
            table_name_include: "app_*".to_owned(),
            table_name_exclude: "*_tmp".to_owned(),
        })),
        "patterns are trimmed before the host sees them"
    );
}

#[test]
fn object_filter_save_is_disabled_while_saving() {
    let mut p = probe();
    p.object.open(&ObjectFilterPrefs::default());
    let mut rt = runtime(object_view, p, true, (960.0, 640.0));
    rt.state.object.saving = true;
    refresh(&mut rt);
    assert!(!element(&rt, "object-filter-save").focusable, "saving disables Save");
    rt.state.object_actions.clear();
    click(&mut rt, "object-filter-save");
    assert!(rt.state.object_actions.is_empty());
}

// ---- SaveTunnelDialog -------------------------------------------------------------------------

#[test]
fn save_tunnel_closed_has_no_panel() {
    let rt = runtime(tunnel_view, probe(), true, (960.0, 640.0));
    assert!(has(&rt, "save-tunnel-dialog-root"));
    assert!(!has(&rt, "save-tunnel-dialog"));
}

#[test]
fn save_tunnel_open_keys_labels_and_validation() {
    let mut p = probe();
    p.tunnel.open();
    let rt = runtime(tunnel_view, p, true, (1440.0, 816.0));
    for key in [
        "save-tunnel-dialog",
        "save-tunnel-name-label",
        "save-tunnel-name",
        "save-tunnel-cancel",
        "save-tunnel-confirm",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered = texts(&rt);
    for key in [
        "newConn.tunnelSaveTitle",
        "newConn.tunnelSaveDesc",
        "newConn.tunnelName",
        "newConn.tunnelSaveConfirm",
        "common.cancel",
    ] {
        assert!(rendered.iter().any(|x| x == t(key)), "{key}: exact translated text");
    }
    assert!(
        include_str!("../src/components/connection/save_tunnel_dialog.rs")
            .contains("t(\"newConn.tunnelNamePlaceholder\")"),
        "the name field uses React's placeholder key"
    );
    assert!(!element(&rt, "save-tunnel-confirm").focusable, "empty name disables confirm");
    assert!(!has(&rt, "save-tunnel-error"), "no banner without an error");
    assert_eq!(element(&rt, "save-tunnel-dialog").label.as_deref(), Some(t("newConn.tunnelSaveTitle")));
}

#[test]
fn save_tunnel_whitespace_name_stays_disabled_and_long_name_is_accepted() {
    let mut p = probe();
    p.tunnel.open();
    let mut rt = runtime(tunnel_view, p, true, (1440.0, 816.0));

    type_into(&mut rt, "save-tunnel-name", "   ");
    assert!(!element(&rt, "save-tunnel-confirm").focusable, "whitespace-only stays disabled");
    rt.state.tunnel_actions.clear();
    click(&mut rt, "save-tunnel-confirm");
    assert!(rt.state.tunnel_actions.is_empty(), "no submit while invalid");

    rt.state.tunnel.name.replace_text("B".repeat(512));
    refresh(&mut rt);
    assert!(element(&rt, "save-tunnel-confirm").focusable, "React has no max-length guard");
    click(&mut rt, "save-tunnel-confirm");
    assert_eq!(
        rt.state.tunnel_actions.last(),
        Some(&SaveTunnelAction::Submit("B".repeat(512)))
    );
}

#[test]
fn save_tunnel_submits_trimmed_name_and_uses_in_flight_states() {
    let mut p = probe();
    p.tunnel.open();
    let mut rt = runtime(tunnel_view, p, true, (960.0, 640.0));
    type_into(&mut rt, "save-tunnel-name", "  Bastion  ");
    click(&mut rt, "save-tunnel-confirm");
    assert_eq!(
        rt.state.tunnel_actions.last(),
        Some(&SaveTunnelAction::Submit("Bastion".to_owned()))
    );

    rt.state.tunnel.submitting = true;
    refresh(&mut rt);
    assert!(!element(&rt, "save-tunnel-confirm").focusable);
    assert!(!element(&rt, "save-tunnel-cancel").focusable, "cancel disabled while pending");
    assert!(texts(&rt).iter().any(|x| x == t("newConn.tunnelSaving")), "saving label shown");
}

#[test]
fn save_tunnel_error_banner_is_boxed_and_alert() {
    let mut p = probe();
    p.tunnel.open();
    p.tunnel.error = "tunnel name already taken".to_owned();
    let mut rt = runtime(tunnel_view, p, true, (960.0, 640.0));
    let banner = element(&rt, "save-tunnel-error");
    assert_eq!(banner.role, "alert", "ErrorBanner announces failures");
    assert_eq!(
        element(&rt, "save-tunnel-error-text").text.as_deref(),
        Some("tunnel name already taken"),
        "the host's error text is shown"
    );
    // Boxed variant paints the danger surface (not the page surface).
    let k = datazen_ui::ui::kit::kit(true);
    assert_eq!(banner.fill, k.c.danger.bg, "boxed banner uses danger.bg");

    rt.state.tunnel.error.clear();
    refresh(&mut rt);
    assert!(!has(&rt, "save-tunnel-error"), "banner removed with the error");
}

#[test]
fn save_tunnel_reopen_resets_the_name() {
    let mut p = probe();
    p.tunnel.open();
    let mut rt = runtime(tunnel_view, p, true, (960.0, 640.0));
    type_into(&mut rt, "save-tunnel-name", "Bastion");
    rt.state.tunnel.close();
    refresh(&mut rt);
    assert!(!has(&rt, "save-tunnel-dialog"));
    rt.state.tunnel.open();
    refresh(&mut rt);
    assert_eq!(rt.state.tunnel.name.text, "", "fresh form on reopen");
    assert!(has(&rt, "save-tunnel-dialog"));
}

// ---- Geometry ----------------------------------------------------------------------------------

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
fn both_dialogs_fit_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut object_p = probe();
            object_p.object.open(&ObjectFilterPrefs::default());
            let object = runtime(object_view, object_p, dark, (w, h));
            assert_rects_inside(&object, w, h, &format!("object dark={dark} {w}x{h}"));

            let mut tunnel_p = probe();
            tunnel_p.tunnel.open();
            tunnel_p.tunnel.error = "a long tunnel name already taken message".to_owned();
            let tunnel = runtime(tunnel_view, tunnel_p, dark, (w, h));
            assert_rects_inside(&tunnel, w, h, &format!("tunnel dark={dark} {w}x{h}"));
        }
    }
}
