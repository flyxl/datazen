//! T281 independent audit: `SshTunnelFields` / `StandardConnectionFields`.
//!
//! Probes branches the coder's suite does not: auth-method value mapping, the
//! `showEnableToggle=false` visibility rule, both path-picker actions, the
//! no-username standard layout, error transitions, and source guards that no
//! stand-in or fabricated IPC remains. Observations come from the tree/state.

use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime, TextEdit};
use rune_kit::root;

use datazen_ui::components::connection::ssh_tunnel_fields::{
    SshAction, SshAuthMethod, SshFieldsState, SshTunnelFields,
};
use datazen_ui::components::connection::standard_connection_fields::{
    StandardConnectionFields, StandardFieldsValue,
};

struct Probe {
    dark: bool,
    ssh: SshFieldsState,
    show_toggle: bool,
    supports_ssh: bool,
    host: TextEdit,
    port: TextEdit,
    database: TextEdit,
    username: TextEdit,
    password: TextEdit,
    host_error: String,
    has_username: bool,
    actions: Vec<SshAction>,
}

fn probe() -> Probe {
    let mut ssh = SshFieldsState::default();
    ssh.enabled = true;
    Probe {
        dark: true,
        ssh,
        show_toggle: true,
        supports_ssh: true,
        host: TextEdit::new("127.0.0.1".to_owned()),
        port: TextEdit::new("5432".to_owned()),
        database: TextEdit::new("postgres".to_owned()),
        username: TextEdit::new("postgres".to_owned()),
        password: TextEdit::new(String::new()),
        host_error: String::new(),
        has_username: true,
        actions: Vec::new(),
    }
}

fn view_ssh(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    SshTunnelFields::<Probe>::new(&s.ssh, s.supports_ssh)
        .show_enable_toggle(s.show_toggle)
        .on_vm(|st: &mut Probe| &mut st.ssh)
        .on_action(|st: &mut Probe, a| st.actions.push(a))
        .build(s.dark)
}

fn view_std(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let value = StandardFieldsValue {
        host: &s.host,
        port: &s.port,
        database: &s.database,
        username: &s.username,
        password: &s.password,
        host_error: &s.host_error,
        port_error: "",
        database_error: "",
        has_username: s.has_username,
    };
    let fields = StandardConnectionFields::<Probe>::new(
        value,
        Some(Rc::new(|st: &mut Probe| &mut st.host)),
        Some(Rc::new(|st: &mut Probe| &mut st.port)),
        Some(Rc::new(|st: &mut Probe| &mut st.database)),
        Some(Rc::new(|st: &mut Probe| &mut st.username)),
        Some(Rc::new(|st: &mut Probe| &mut st.password)),
    )
    .build(s.dark);
    let mut root = col().key("std-root").w_full().gap(8.0);
    for field in fields {
        root = root.child(field);
    }
    root
}

fn runtime(view: fn(&Probe, &mut Cx<Probe>) -> El, p: Probe) -> Runtime<Probe> {
    let mut rt = Runtime::new(
        p,
        view,
        (1440.0, 816.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
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

fn refresh(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    rt.frame(1.0);
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

#[test]
fn auth_method_value_mapping_round_trips() {
    assert_eq!(SshAuthMethod::from_value("password"), SshAuthMethod::Password);
    assert_eq!(SshAuthMethod::from_value("private_key"), SshAuthMethod::PrivateKey);
    assert_eq!(SshAuthMethod::from_value("agent"), SshAuthMethod::Agent);
    assert_eq!(SshAuthMethod::from_value("unknown"), SshAuthMethod::Password);
    for method in [SshAuthMethod::Password, SshAuthMethod::PrivateKey, SshAuthMethod::Agent] {
        assert_eq!(SshAuthMethod::from_value(method.value()), method);
    }
}

/// React: `(showEnableToggle ? form.sshEnabled : true)` — hiding the toggle
/// always shows the panel, even when the local enabled flag is false.
#[test]
fn hidden_enable_toggle_always_reveals_the_panel() {
    let mut p = probe();
    p.show_toggle = false;
    p.ssh.enabled = false;
    let rt = runtime(view_ssh, p);
    assert!(!has(&rt, "new-conn-ssh-tunnel-checkbox"), "no toggle when hidden");
    assert!(has(&rt, "new-conn-ssh-fields-panel"), "panel shown regardless of enabled");
}

/// Both key-path browse buttons report the correct boolean to `PickPath`
/// (false = primary, true = jump host).
#[test]
fn both_key_path_browses_dispatch_the_right_pick_path() {
    let mut p = probe();
    p.ssh.auth_method = SshAuthMethod::PrivateKey;
    let mut rt = runtime(view_ssh, p);
    click(&mut rt, "new-conn-ssh-key-path-browse");
    assert_eq!(rt.state.actions.last(), Some(&SshAction::PickPath(false)));

    let mut p = probe();
    p.ssh.jump_enabled = true;
    p.ssh.jump_auth_method = SshAuthMethod::PrivateKey;
    let mut rt = runtime(view_ssh, p);
    click(&mut rt, "new-conn-ssh-jump-key-path-browse");
    assert_eq!(rt.state.actions.last(), Some(&SshAction::PickPath(true)));
}

/// React hides the username field when `hasUsername` is false but always shows
/// the password.
#[test]
fn standard_fields_without_username_keep_password() {
    let mut p = probe();
    p.has_username = false;
    let rt = runtime(view_std, p);
    assert!(!has(&rt, "new-conn-username"));
    assert!(has(&rt, "new-conn-password"));
    assert!(has(&rt, "new-conn-host"));
}

/// Validation error nodes appear and disappear with the host error strings.
#[test]
fn standard_field_error_nodes_follow_state() {
    let mut p = probe();
    p.host_error = "required".to_owned();
    let mut rt = runtime(view_std, p);
    assert_eq!(element(&rt, "new-conn-host-error").text.as_deref(), Some("required"));
    assert!(!has(&rt, "new-conn-port-error"));

    rt.state.host_error.clear();
    refresh(&mut rt);
    assert!(!has(&rt, "new-conn-host-error"));
}

/// The form-body source no longer carries the T103 stand-in and neither dialog
/// fabricates an IPC command.
#[test]
fn no_local_stand_in_and_no_fake_ipc() {
    let body = include_str!("../src/components/connection/connection_form_body.rs");
    assert!(!body.contains("fn standard_fields"), "local stand-in removed");
    assert!(body.contains("StandardConnectionFields::new"), "real port used");

    let ssh = include_str!("../src/components/connection/ssh_tunnel_fields.rs");
    let std = include_str!("../src/components/connection/standard_connection_fields.rs");
    for src in [ssh, std] {
        for forbidden in ["connectionCommands", "invoke(", "spawn("] {
            assert!(!src.contains(forbidden), "no fabricated IPC: {forbidden}");
        }
    }
}

/// Long host / passphrase / jump values stay inside both windows and palettes.
#[test]
fn long_values_stay_inside_both_windows_both_palettes() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut p = probe();
            p.ssh.auth_method = SshAuthMethod::PrivateKey;
            p.ssh.jump_enabled = true;
            p.ssh.jump_auth_method = SshAuthMethod::PrivateKey;
            p.ssh.host = TextEdit::new("h".repeat(120));
            p.ssh.passphrase = TextEdit::new("p".repeat(120));
            p.dark = dark;
            let mut rt = Runtime::new(
                p,
                view_ssh,
                (w, h),
                Env { system_dark: dark, scale: 1.0 },
            );
            rt.frame(0.0);
            let mut outside = Vec::new();
            for e in rt.elements() {
                let r = e.rect;
                if r[0] < -0.5 || r[0] + r[2] > w + 0.5 {
                    outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
                }
            }
            assert!(outside.is_empty(), "ssh dark={dark} {w}x{h}: {outside:?}");
        }
    }
}
