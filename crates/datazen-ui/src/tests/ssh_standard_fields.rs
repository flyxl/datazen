//! T281: `SshTunnelFields`, `StandardConnectionFields` and the form-body wiring
//! that replaced T103's stand-ins — keys, validation/toggles, callbacks and
//! two-window/two-palette geometry.

use rune::{col, Cx, El, Env, Runtime, TextEdit};
use rune_kit::root;

use crate::components::connection::connection_form_body::{FormBodyVm, ConnectionFormBody};
use crate::components::connection::ssh_tunnel_fields::{
    SshAction, SshAuthMethod, SshFieldsState, SshTunnelFields,
};
use crate::components::connection::standard_connection_fields::{
    StandardConnectionFields, StandardFieldsValue,
};
use crate::i18n::t;

struct Probe {
    dark: bool,
    supports_ssh: bool,
    show_toggle: bool,
    ssh: SshFieldsState,
    host: TextEdit,
    port: TextEdit,
    database: TextEdit,
    username: TextEdit,
    password: TextEdit,
    actions: Vec<SshAction>,
    form: FormBodyVm,
}

fn probe() -> Probe {
    Probe {
        dark: true,
        supports_ssh: true,
        show_toggle: true,
        ssh: {
            let mut state = SshFieldsState::default();
            state.enabled = true;
            state
        },
        host: TextEdit::new("127.0.0.1".to_owned()),
        port: TextEdit::new("5432".to_owned()),
        database: TextEdit::new("postgres".to_owned()),
        username: TextEdit::new("postgres".to_owned()),
        password: TextEdit::new(String::new()),
        actions: Vec::new(),
        form: FormBodyVm::default(),
    }
}

fn ssh_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    SshTunnelFields::<Probe>::new(&s.ssh, s.supports_ssh)
        .show_enable_toggle(s.show_toggle)
        .on_vm(|st: &mut Probe| &mut st.ssh)
        .on_action(|st: &mut Probe, a| st.actions.push(a))
        .build(s.dark)
}

fn std_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let fields = StandardConnectionFields::<Probe>::new(
        StandardFieldsValue {
            host: &s.host,
            port: &s.port,
            database: &s.database,
            username: &s.username,
            password: &s.password,
            host_error: "",
            port_error: "",
            database_error: "",
            has_username: true,
        },
        Some(std::rc::Rc::new(|st: &mut Probe| &mut st.host)),
        Some(std::rc::Rc::new(|st: &mut Probe| &mut st.port)),
        Some(std::rc::Rc::new(|st: &mut Probe| &mut st.database)),
        Some(std::rc::Rc::new(|st: &mut Probe| &mut st.username)),
        Some(std::rc::Rc::new(|st: &mut Probe| &mut st.password)),
    )
    .build(s.dark);
    let mut root = col().key("std-root").w_full().gap(8.0);
    for field in fields {
        root = root.child(field);
    }
    root
}

fn form_view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let mut layer = Vec::new();
    let form = ConnectionFormBody::<Probe>::new(s.form.clone())
        .on_vm(|st: &mut Probe| &mut st.form)
        .build(s.dark, &mut layer);
    root(col().w_full().h_full().child(form), layer)
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

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0);
}

fn refresh(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    // A non-zero dt completes the mount fade-in of nodes revealed by a state
    // change; otherwise they stay at opacity 0 and cannot receive a press.
    rt.frame(1.0);
}

// ---- SSH --------------------------------------------------------------------------------------

#[test]
fn ssh_fields_hide_when_unsupported() {
    let mut p = probe();
    p.supports_ssh = false;
    let rt = runtime(ssh_view, p, true, (960.0, 640.0));
    assert!(has(&rt, "new-conn-ssh-fields-hidden"), "inert marker replaces null");
    assert!(!has(&rt, "new-conn-ssh-fields"), "no fields when unsupported");
}

#[test]
fn ssh_fields_keys_and_auth_modes() {
    let rt = runtime(ssh_view, probe(), true, (1440.0, 816.0));
    for key in [
        "new-conn-ssh-fields",
        "new-conn-ssh-tunnel",
        "new-conn-ssh-tunnel-checkbox",
        "new-conn-ssh-host",
        "new-conn-ssh-port",
        "new-conn-ssh-username",
        "new-conn-ssh-auth-method",
        "new-conn-ssh-auth-password",
        "new-conn-ssh-auth-private_key",
        "new-conn-ssh-auth-agent",
        "new-conn-ssh-password",
        "new-conn-ssh-jump-toggle",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    let rendered: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    for key in [
        "newConn.sshTunnel",
        "newConn.sshHost",
        "newConn.sshPort",
        "newConn.sshUsername",
        "newConn.authMethod",
        "newConn.authPassword",
        "newConn.authKey",
        "newConn.authAgent",
        "newConn.sshPassword",
        "newConn.sshJump",
    ] {
        assert!(rendered.iter().any(|x| x == t(key)), "{key}: exact translated text");
    }
}

#[test]
fn ssh_auth_switch_shows_agent_hint_and_key_fields() {
    let mut rt = runtime(ssh_view, probe(), true, (1440.0, 816.0));
    click(&mut rt, "new-conn-ssh-auth-agent");
    assert_eq!(rt.state.actions.last(), Some(&SshAction::SetAuth(SshAuthMethod::Agent)));
    rt.state.ssh.auth_method = SshAuthMethod::Agent;
    refresh(&mut rt);
    assert!(has(&rt, "new-conn-ssh-auth-agent-hint"));
    assert!(!has(&rt, "new-conn-ssh-password"), "agent mode hides the password");

    click(&mut rt, "new-conn-ssh-auth-private_key");
    rt.state.ssh.auth_method = SshAuthMethod::PrivateKey;
    refresh(&mut rt);
    assert!(has(&rt, "new-conn-ssh-key-path"));
    assert!(has(&rt, "new-conn-ssh-passphrase"));
    assert!(!has(&rt, "new-conn-ssh-auth-agent-hint"));
}

#[test]
fn ssh_enable_and_jump_toggles_dispatch_and_reveal_jump_fields() {
    let mut p = probe();
    p.ssh.enabled = false;
    let mut rt = runtime(ssh_view, p, true, (960.0, 640.0));
    assert!(has(&rt, "new-conn-ssh-fields"), "root stays mounted with the toggle");
    assert!(!has(&rt, "new-conn-ssh-fields-panel"), "panel hidden while disabled");
    click(&mut rt, "new-conn-ssh-tunnel-checkbox");
    assert_eq!(rt.state.actions.last(), Some(&SshAction::SetEnabled(true)));
    rt.state.ssh.enabled = true;
    refresh(&mut rt);
    assert!(has(&rt, "new-conn-ssh-fields-panel"), "panel appears once enabled");

    click(&mut rt, "new-conn-ssh-jump-toggle");
    assert_eq!(rt.state.actions.last(), Some(&SshAction::SetJumpEnabled(true)));
    rt.state.ssh.jump_enabled = true;
    refresh(&mut rt);
    for key in [
        "new-conn-ssh-jump-fields",
        "new-conn-ssh-jump-host",
        "new-conn-ssh-jump-port",
        "new-conn-ssh-jump-username",
        "new-conn-ssh-jump-auth-password",
        "new-conn-ssh-jump-auth-private_key",
        "new-conn-ssh-jump-auth-agent",
    ] {
        assert!(has(&rt, key), "{key} is mounted in the jump section");
    }
    click(&mut rt, "new-conn-ssh-jump-auth-private_key");
    assert_eq!(rt.state.actions.last(), Some(&SshAction::SetJumpAuth(SshAuthMethod::PrivateKey)));
}

// ---- Standard ---------------------------------------------------------------------------------

#[test]
fn standard_fields_keys_values_and_validation() {
    let mut p = probe();
    p.host = TextEdit::new("db.example.com".to_owned());
    let mut rt = runtime(std_view, p, true, (1440.0, 816.0));
    for key in [
        "new-conn-host",
        "new-conn-port",
        "new-conn-database",
        "new-conn-username",
        "new-conn-password",
    ] {
        assert!(has(&rt, key), "{key} is mounted");
    }
    assert_eq!(element(&rt, "new-conn-host").label.as_deref(), None);

    rt.state.host = TextEdit::new(String::new());
    refresh(&mut rt);
    assert_eq!(rt.state.host.text, "", "accessor routes edits into the host state");

    rt.state.host = TextEdit::new("db.example.com".to_owned());
    refresh(&mut rt);
    let rendered: Vec<String> = rt.elements().iter().filter_map(|e| e.text.clone()).collect();
    for key in ["newConn.host", "newConn.port", "newConn.database", "newConn.username", "newConn.password"] {
        assert!(rendered.iter().any(|x| x == t(key)), "{key}: exact translated label");
    }
}

// ---- Form body wiring -------------------------------------------------------------------------

#[test]
fn form_body_uses_the_real_standard_fields_port() {
    let rt = runtime(form_view, probe(), true, (1440.0, 816.0));
    for key in ["new-conn-host", "new-conn-port", "new-conn-database", "new-conn-username", "new-conn-password"] {
        assert!(has(&rt, key), "form body still mounts {key} after the stand-in swap");
    }
    // The source no longer defines the local stand-in.
    let src = include_str!("../components/connection/connection_form_body.rs");
    assert!(!src.contains("fn standard_fields"), "the local standard stand-in is gone");
    assert!(src.contains("StandardConnectionFields::new"), "the real port is used");
}

// ---- Geometry ----------------------------------------------------------------------------------

/// The SSH group is embedded in a scrollable form body in production; pin
/// horizontal containment for all nodes and full containment for compact groups.
fn assert_rects_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str, horizontal_only: bool) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        let bad_x = r[0] < -0.5 || r[0] + r[2] > w + 0.5;
        let bad_y = !horizontal_only && (r[1] < -0.5 || r[1] + r[3] > h + 0.5);
        if bad_x || bad_y {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(outside.is_empty(), "{label}: {} node(s) outside {w}x{h}: {outside:?}", outside.len());
}

#[test]
fn both_field_groups_fit_both_windows_in_dark_and_light() {
    for dark in [true, false] {
        for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
            let mut ssh_p = probe();
            ssh_p.ssh.enabled = true;
            ssh_p.ssh.auth_method = SshAuthMethod::PrivateKey;
            ssh_p.ssh.jump_enabled = true;
            ssh_p.ssh.jump_auth_method = SshAuthMethod::PrivateKey;
            let ssh = runtime(ssh_view, ssh_p, dark, (w, h));
            assert_rects_inside(&ssh, w, h, &format!("ssh dark={dark} {w}x{h}"), true);

            let std = runtime(std_view, probe(), dark, (w, h));
            assert_rects_inside(&std, w, h, &format!("std dark={dark} {w}x{h}"), false);
        }
    }
}
