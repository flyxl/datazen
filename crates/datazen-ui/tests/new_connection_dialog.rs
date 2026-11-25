//! T239: `NewConnectionDialog` — open/close, driver picker, form/action wiring,
//! validation, exact keys/i18n, and two-window/two-palette geometry.

use rune::{col, Cx, El, Env, Key, KeyEvent, Mods, Runtime, TextEdit};
use rune_kit::root;

use datazen_ui::components::connection::new_connection_dialog::{
    NewConnectionAction, NewConnectionDialog, NewConnectionDialogVm,
};
use datazen_ui::components::connection::connection_form_body::FormAction;
use datazen_ui::i18n::t;

struct Probe {
    dark: bool,
    vm: NewConnectionDialogVm,
    actions: Vec<NewConnectionAction>,
}

fn probe(open: bool) -> Probe {
    let mut vm = NewConnectionDialogVm::default();
    if open {
        vm.open(None, None);
    }
    Probe {
        dark: true,
        vm,
        actions: Vec::new(),
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let dialog = NewConnectionDialog::<Probe>::new(&s.vm)
        .on_vm(|st: &mut Probe| &mut st.vm)
        .on_action(|st: &mut Probe, a| st.actions.push(a))
        .build(s.dark, &mut layer);
    root(col().w_full().h_full().child(dialog), layer)
}

fn rt(open: bool, dark: bool, size: (f32, f32)) -> Runtime<Probe> {
    let mut p = probe(open);
    p.dark = dark;
    let mut runtime = Runtime::new(
        p,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    runtime.frame(0.0);
    runtime
}

fn has<S>(r: &Runtime<S>, key: &str) -> bool {
    r.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn element<S>(r: &Runtime<S>, key: &str) -> rune::ElementInfo {
    r.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts<S>(r: &Runtime<S>) -> Vec<String> {
    r.elements().iter().filter_map(|e| e.text.clone()).collect()
}

fn click<S>(r: &mut Runtime<S>, key: &str) {
    let (x, y) = r.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    r.pointer_down(x, y);
    r.pointer_up(x, y);
    r.frame(0.0);
    r.frame(0.0);
}

fn refresh(r: &mut Runtime<Probe>) {
    r.invalidate();
    r.frame(0.0);
}

// ---- Keys / open / close -----------------------------------------------------------------------

#[test]
fn closed_dialog_mounts_no_panel_but_keeps_the_portal_marker() {
    let r = rt(false, true, (960.0, 640.0));
    assert!(has(&r, "new-connection-dialog-root"));
    assert!(has(&r, "new-connection-dialog-backdrop"), "backdrop testid survives");
    assert!(!has(&r, "new-connection-dialog"), "no panel while closed");
    assert!(!has(&r, "new-conn-save"), "no actions while closed");
}

#[test]
fn open_dialog_shows_all_react_keys_and_exact_i18n() {
    let r = rt(true, true, (1440.0, 816.0));
    for key in [
        "new-connection-dialog",
        "new-connection-dialog-backdrop",
        "new-conn-select-db-type",
        "new-conn-driver-search",
        "new-conn-driver-postgresql",
        "new-conn-driver-mysql",
        "new-conn-driver-sqlite",
        "new-conn-test-connection",
        "new-conn-cancel",
        "new-conn-save",
        "new-conn-body",
    ] {
        assert!(has(&r, key), "{key} is mounted");
    }
    let rendered = texts(&r);
    for key in [
        "common.newConnection",
        "newConn.selectDbType",
        "newConn.testConnection",
        "common.cancel",
        "common.save",
        "newConn.connectionConfig",
    ] {
        assert!(rendered.iter().any(|x| x == t(key)), "{key}: exact translated text visible");
    }
    // The driver search hint is a placeholder (a drawing hint, not a `text`
    // node); pin its exact key at the source call site.
    assert!(
        include_str!("../src/components/connection/new_connection_dialog.rs")
            .contains("t(\"newConn.searchDrivers\")"),
        "the driver search uses React's placeholder key"
    );
    assert_eq!(
        element(&r, "new-connection-dialog").label.as_deref(),
        Some(t("common.newConnection"))
    );
}

#[test]
fn edit_mode_uses_the_edit_connection_title() {
    let mut p = probe(false);
    p.vm.open(Some("cfg-1".to_owned()), None);
    let mut r = Runtime::new(p, view, (1440.0, 816.0), env(true));
    r.frame(0.0);
    assert_eq!(
        element(&r, "new-connection-dialog").label.as_deref(),
        Some(t("common.editConnection"))
    );
    assert!(texts(&r).iter().any(|x| x == t("common.editConnection")));
}

#[test]
fn backdrop_has_no_dismiss_handler_but_close_cancel_and_escape_do() {
    let mut r = rt(true, true, (960.0, 640.0));
    // The backdrop marker is inert; pressing it does not report Close.
    click(&mut r, "new-connection-dialog-backdrop");
    assert!(r.state.actions.is_empty(), "backdrop never dismisses (React parity)");

    click(&mut r, "new-conn-cancel");
    assert_eq!(r.state.actions.last(), Some(&NewConnectionAction::Close));
    r.state.actions.clear();

    click(&mut r, "new-connection-dialog-close");
    assert_eq!(r.state.actions.last(), Some(&NewConnectionAction::Close));
    r.state.actions.clear();

    r.key(KeyEvent {
        key: Key::Escape,
        mods: Mods::default(),
    });
    r.frame(0.0);
    r.frame(0.0);
    assert_eq!(r.state.actions.last(), Some(&NewConnectionAction::Close), "Escape dismisses");
}

// ---- Picker / action wiring --------------------------------------------------------------------

#[test]
fn driver_rows_are_filtered_by_the_search_box_and_select_reports_the_value() {
    let mut r = rt(true, true, (1440.0, 816.0));
    assert!(has(&r, "new-conn-driver-postgresql"));
    assert!(has(&r, "new-conn-driver-mysql"));
    assert!(has(&r, "new-conn-driver-sqlite"));

    r.state.vm.driver_query = TextEdit::new("mysql".to_owned());
    refresh(&mut r);
    assert!(has(&r, "new-conn-driver-mysql"), "match stays");
    assert!(!has(&r, "new-conn-driver-postgresql"), "non-match filtered out");

    r.state.vm.driver_query = TextEdit::new("nope".to_owned());
    refresh(&mut r);
    assert!(has(&r, "new-conn-no-drivers"), "no-match hint shown");

    r.state.vm.driver_query.replace_text(String::new());
    refresh(&mut r);
    click(&mut r, "new-conn-driver-mysql");
    assert_eq!(
        r.state.actions.last(),
        Some(&NewConnectionAction::SelectType("mysql".to_owned()))
    );
}

#[test]
fn test_and_save_actions_are_wired_and_disabled_while_in_flight() {
    let mut r = rt(true, true, (1440.0, 816.0));
    click(&mut r, "new-conn-test-connection");
    assert_eq!(r.state.actions.last(), Some(&NewConnectionAction::Test));
    click(&mut r, "new-conn-save");
    assert_eq!(r.state.actions.last(), Some(&NewConnectionAction::Save));

    r.state.vm.testing = true;
    r.state.vm.saving = true;
    refresh(&mut r);
    assert!(!element(&r, "new-conn-test-connection").focusable, "testing disables Test");
    assert!(!element(&r, "new-conn-save").focusable, "saving disables Save");
    let rendered = texts(&r);
    assert!(rendered.iter().any(|x| x == t("newConn.testing")), "in-flight label shown");
    r.state.actions.clear();
    click(&mut r, "new-conn-test-connection");
    click(&mut r, "new-conn-save");
    assert!(r.state.actions.is_empty(), "disabled buttons emit nothing");
}

#[test]
fn form_edits_are_forwarded_to_the_host() {
    let mut r = rt(true, true, (1440.0, 816.0));
    // The form body's own select and advanced toggle dispatch FormAction edits.
    click(&mut r, "new-conn-advanced-toggle-chevron");
    assert!(
        r.state.actions.iter().any(|a| matches!(a, NewConnectionAction::FormEdit(FormAction::Advanced(_)))),
        "advanced toggle forwarded as a form edit"
    );
}

// ---- Validation / reset -----------------------------------------------------------------------

#[test]
fn validation_errors_from_the_form_are_visible_in_the_dialog() {
    let mut r = rt(true, true, (1440.0, 816.0));
    r.state.vm.form.host_error = t("newConn.required").to_owned();
    r.state.vm.form.port_error = t("newConn.required").to_owned();
    refresh(&mut r);
    for key in ["new-conn-host-error", "new-conn-port-error"] {
        assert_eq!(
            element(&r, key).text.as_deref(),
            Some(t("newConn.required")),
            "{key} shows the validation error"
        );
    }
}

#[test]
fn reopening_in_create_mode_resets_the_form_and_bumps_the_epoch() {
    let mut r = rt(true, true, (1440.0, 816.0));
    let first_epoch = r.state.vm.open_epoch;
    r.state.vm.form.name = TextEdit::new("My Test Connection".to_owned());
    r.state.vm.driver_query = TextEdit::new("mysql".to_owned());
    r.state.vm.close();
    refresh(&mut r);
    assert!(!has(&r, "new-connection-dialog"));

    r.state.vm.open(None, None);
    refresh(&mut r);
    assert_eq!(r.state.vm.open_epoch, first_epoch + 1, "each open gets a fresh epoch");
    assert_eq!(r.state.vm.form.name.text, "", "name reset");
    assert_eq!(r.state.vm.form.host.text, "127.0.0.1", "default host restored");
    assert_eq!(r.state.vm.form.port.text, "5432", "default port restored");
    assert!(r.state.vm.driver_query.text.is_empty(), "driver query reset");
    assert!(has(&r, "new-connection-dialog"), "panel is back");
}

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

// ---- Geometry ----------------------------------------------------------------------------------

fn assert_rects_inside<S>(r: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in r.elements() {
        let rect = e.rect;
        if rect[0] < -0.5 || rect[1] < -0.5 || rect[0] + rect[2] > w + 0.5 || rect[1] + rect[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), rect));
        }
    }
    assert!(outside.is_empty(), "{label}: {} node(s) outside {w}x{h}: {outside:?}", outside.len());
}

#[test]
fn the_dialog_fits_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let dialog = rt(true, dark, (w, h));
            assert_rects_inside(&dialog, w, h, &format!("dialog dark={dark} {w}x{h}"));
            let mut with_error = rt(true, dark, (w, h));
            with_error.state.vm.form.test_err = "a fairly long connection failure message".to_owned();
            refresh(&mut with_error);
            assert_rects_inside(&with_error, w, h, &format!("error dark={dark} {w}x{h}"));
        }
    }
}
