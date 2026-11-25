//! T260 independent audit: `ObjectFilterDialog` + `SaveTunnelDialog` + `shared`.
//!
//! Probes branches the coder's suite does not: Escape exit, reopen reseeding,
//! the host `busy` flag, error-actionability, host-port source guards and a
//! long-value geometry stress. Observations are from the rendered tree / state.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};
use rune_kit::root;

use datazen_ui::components::connection::object_filter_dialog::{
    ObjectFilterDialog, ObjectFilterPrefs, ObjectFilterVm,
};
use datazen_ui::components::connection::save_tunnel_dialog::{SaveTunnelDialog, SaveTunnelVm};
use datazen_ui::i18n::t;

struct Probe {
    dark: bool,
    object: ObjectFilterVm,
    tunnel: SaveTunnelVm,
    object_actions: Vec<String>,
    tunnel_actions: Vec<String>,
}

fn view_object(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let d = ObjectFilterDialog::<Probe>::new(&s.object)
        .on_vm(|st: &mut Probe| &mut st.object)
        .on_action(|st: &mut Probe, a| st.object_actions.push(format!("{a:?}")))
        .build(s.dark, &mut layer);
    root(col().w_full().h_full().child(d), layer)
}

fn view_tunnel(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let d = SaveTunnelDialog::<Probe>::new(&s.tunnel)
        .on_vm(|st: &mut Probe| &mut st.tunnel)
        .on_action(|st: &mut Probe, a| st.tunnel_actions.push(format!("{a:?}")))
        .build(s.dark, &mut layer);
    root(col().w_full().h_full().child(d), layer)
}

fn runtime(view: fn(&Probe, &mut Cx<Probe>) -> El, p: Probe, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        p,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
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

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
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

fn refresh(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    rt.frame(0.0);
}

fn escape(rt: &mut Runtime<Probe>) {
    rt.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    rt.frame(0.0);
    rt.frame(0.0);
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

fn type_into(rt: &mut Runtime<Probe>, key: &str, text: &str) {
    click(rt, key);
    rt.paste(text);
    rt.frame(0.0);
    rt.frame(1.0 / 60.0);
}

/// Both dialogs wire the host `on_close` on Escape, like React's Dialog.
#[test]
fn escape_closes_both_dialogs() {
    let mut p = probe();
    p.object.open(&ObjectFilterPrefs::default());
    let mut rt = runtime(view_object, p, true, (960.0, 640.0));
    escape(&mut rt);
    assert_eq!(rt.state.object_actions.last().map(String::as_str), Some("Close"));

    let mut p = probe();
    p.tunnel.open();
    let mut rt = runtime(view_tunnel, p, true, (960.0, 640.0));
    escape(&mut rt);
    assert_eq!(rt.state.tunnel_actions.last().map(String::as_str), Some("Close"));
}

/// Reopening the object filter reseeds from the newly supplied saved prefs.
#[test]
fn object_filter_reopen_reseeds_from_new_saved_prefs() {
    let mut p = probe();
    p.object.open(&ObjectFilterPrefs {
        hide_system_schemas: true,
        table_name_include: "old_*".to_owned(),
        table_name_exclude: String::new(),
    });
    let mut rt = runtime(view_object, p, true, (960.0, 640.0));
    assert_eq!(rt.state.object.table_name_include.text, "old_*");

    rt.state.object.close();
    refresh(&mut rt);
    rt.state.object.open(&ObjectFilterPrefs {
        hide_system_schemas: false,
        table_name_include: "new_*".to_owned(),
        table_name_exclude: "*_tmp".to_owned(),
    });
    refresh(&mut rt);
    assert_eq!(rt.state.object.table_name_include.text, "new_*");
    assert_eq!(rt.state.object.table_name_exclude.text, "*_tmp");
    assert!(!rt.state.object.hide_system_schemas);
}

/// React's `busy` prop alone (without local `submitting`) must disable both
/// actions and show the in-flight label.
#[test]
fn save_tunnel_busy_prop_disables_actions_and_shows_label() {
    let mut p = probe();
    p.tunnel.open();
    p.tunnel.busy = true;
    let rt = runtime(view_tunnel, p, true, (960.0, 640.0));
    assert!(!element(&rt, "save-tunnel-confirm").focusable);
    assert!(!element(&rt, "save-tunnel-cancel").focusable);
    assert!(
        texts(&rt).iter().any(|x| x == &t("newConn.tunnelSaving")),
        "busy shows the saving label"
    );
}

/// With an error banner shown, React keeps the name field usable so the error
/// is actionable. The port must not disable the field while `error` is set.
#[test]
fn save_tunnel_name_editable_while_error_shown() {
    let mut p = probe();
    p.tunnel.open();
    p.tunnel.error = "tunnel name already taken".to_owned();
    let mut rt = runtime(view_tunnel, p, true, (960.0, 640.0));
    type_into(&mut rt, "save-tunnel-name", "Bastion 2");
    assert_eq!(rt.state.tunnel.name.text, "Bastion 2");
    assert!(has(&rt, "save-tunnel-error"), "banner remains until host clears it");
}

/// Neither dialog may fabricate IPC; all commands go through the host callback.
#[test]
fn no_fake_ipc_in_either_dialog() {
    for src in [
        include_str!("../src/components/connection/object_filter_dialog.rs"),
        include_str!("../src/components/connection/save_tunnel_dialog.rs"),
    ] {
        for forbidden in [
            "connectionCommands",
            "saveConnection(",
            "saveAsTunnel(",
            "getObjectFilter(",
        ] {
            assert!(!src.contains(forbidden), "no fabricated IPC: {forbidden}");
        }
    }
}

/// Long pattern values and a long error must stay inside both windows in both
/// palettes (rect-based observation).
#[test]
fn long_values_fit_both_windows_both_palettes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe();
            p.object.open(&ObjectFilterPrefs {
                hide_system_schemas: true,
                table_name_include: "a".repeat(200),
                table_name_exclude: "b".repeat(200),
            });
            let mut rt = runtime(view_object, p, dark, (w, h));
            check_rects(&rt, w, h, &format!("object dark={dark} {w}x{h}"));

            let mut p = probe();
            p.tunnel.open();
            p.tunnel.error = "a very long tunnel error message that must stay inside".to_owned();
            p.tunnel.name = TextEdit::new("c".repeat(200));
            let mut rt = runtime(view_tunnel, p, dark, (w, h));
            check_rects(&rt, w, h, &format!("tunnel dark={dark} {w}x{h}"));
        }
    }
}

fn check_rects<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(outside.is_empty(), "{label}: {} node(s) outside: {outside:?}", outside.len());
}
