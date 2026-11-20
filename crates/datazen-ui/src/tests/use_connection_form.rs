//! T333: `useConnectionForm` ported as an immediate-mode state machine.
//!
//! The React hook tests are the spec: defaults, group coercion, per-driver
//! snapshots, read-only forcing, validation, test/save payloads and the
//! inline/saved tunnel state machine. All commands go through a deterministic
//! fake host — no real IPC, thread or future.

use crate::components::connection::connection_advanced_settings::{SavedTunnel, TunnelKind};
use crate::components::connection::tunnel_field_contracts::{HttpProxyScheme, SshAuthMethod, WsMode};
use crate::components::connection::use_connection_form::{
    step, BuiltConfig, ConnectionFormHost, ConnectionFormOptions, ConnectionFormState, FormAction,
    GroupInput, TunnelSource,
};
use crate::components::connection::use_connection_form::{
    ExistingConnection, ExistingHttpProxy, ExistingSsh, ExistingWebSocket,
};

struct TestHost {
    tunnels: Vec<SavedTunnel>,
    test_result: Result<String, String>,
    test_calls: Vec<BuiltConfig>,
    save_calls: Vec<(BuiltConfig, bool)>,
    after_save: usize,
}

impl Default for TestHost {
    fn default() -> Self {
        Self {
            tunnels: Vec::new(),
            test_result: Ok("16.0".to_owned()),
            test_calls: Vec::new(),
            save_calls: Vec::new(),
            after_save: 0,
        }
    }
}

impl ConnectionFormHost for TestHost {
    fn test_connection(&mut self, config: &BuiltConfig) -> Result<String, String> {
        self.test_calls.push(config.clone());
        self.test_result.clone()
    }

    fn save_connection(&mut self, config: &BuiltConfig, clear_stored_credentials: bool) {
        self.save_calls.push((config.clone(), clear_stored_credentials));
    }

    fn saved_tunnels(&self) -> &[SavedTunnel] {
        &self.tunnels
    }

    fn new_id(&self) -> String {
        "conn_test".to_owned()
    }

    fn on_after_save(&mut self) {
        self.after_save += 1;
    }
}

fn host() -> TestHost {
    TestHost {
        test_result: Ok("16.0".to_owned()),
        ..TestHost::default()
    }
}

fn new_state(options: ConnectionFormOptions, host: &TestHost) -> ConnectionFormState {
    ConnectionFormState::new(&options, host)
}

fn apply(state: &mut ConnectionFormState, host: &mut TestHost, actions: Vec<FormAction>) {
    for action in actions {
        step(state, action, host);
    }
}

fn existing(id: &str) -> ExistingConnection {
    ExistingConnection::new(id, "postgresql", id)
}

// ---- Defaults and API surface -----------------------------------------------

#[test]
fn defaults_match_the_react_hook_and_expose_no_draft_or_meta() {
    let state = ConnectionFormState::default();
    assert_eq!(state.name, "");
    assert_eq!(state.database_type, "postgresql");
    assert_eq!(state.host, "127.0.0.1");
    assert_eq!(state.port, "5432");
    assert_eq!(state.database, "postgres");
    assert_eq!(state.schema, "default");
    assert_eq!(state.username, "postgres");
    assert_eq!(state.ssl_mode, "prefer");
    assert_eq!(state.group, "preset:development");
    assert_eq!(state.color_tag, "#3b82f6");
    assert_eq!(state.http_proxy_port, "8080");
    assert_eq!(state.http_proxy_timeout, "30");
    assert_eq!(state.ws_timeout, "30");
    assert!(state.supports_ssh(), "postgresql supports SSH");
    assert!(state.supports_ssl(), "postgresql supports SSL");
    assert!(!state.testing);

    let src = include_str!("../components/connection/use_connection_form.rs");
    assert!(!src.contains("pub draft"), "no draft on the API surface");
    assert!(!src.contains("pub meta"), "no meta on the API surface");
}

#[test]
fn group_defaults_are_coerced_and_events_rejected() {
    let mut custom = ConnectionFormOptions::default();
    custom.default_group = Some("prod".to_owned());
    assert_eq!(new_state(custom, &host()).group, "prod", "defaultGroup is used");

    let mut h = host();
    let mut state = ConnectionFormState::default();
    apply(&mut state, &mut h, vec![FormAction::SetGroup(GroupInput::Other)]);
    assert_eq!(state.group, "", "a mistaken DOM event becomes the empty group");
    let config = state.on_test_config_for_test(&h);
    assert_eq!(config.group, None, "empty group is omitted from the config");
}

/// Small test-only extension so the group case can inspect the built config
/// without triggering a command (the production API keeps `build_config` host-only).
trait TestConfigProbe {
    fn on_test_config_for_test(&self, host: &TestHost) -> BuiltConfig;
}
impl TestConfigProbe for ConnectionFormState {
    fn on_test_config_for_test(&self, host: &TestHost) -> BuiltConfig {
        crate::components::connection::use_connection_form::build_config_for_test(
            self,
            &host.new_id(),
            "newConn.unnamed",
        )
    }
}

#[test]
fn stored_group_name_is_kept_when_loading_an_existing_connection() {
    let mut existing = existing("c1");
    existing.group = Some("开发环境".to_owned());
    let mut options = ConnectionFormOptions {
        edit_id: Some("c1".to_owned()),
        ..ConnectionFormOptions::default()
    };
    options.existing_connections.push(existing);
    let state = new_state(options, &host());
    assert_eq!(state.group, "开发环境");
    assert!(state.show_advanced, "edit mode opens Advanced");
}

// ---- Driver type changes and snapshots --------------------------------------

#[test]
fn form_variant_comes_from_the_registry() {
    let mut h = host();
    let mut state = ConnectionFormState::default();
    assert_eq!(state.form_variant(), "standard");
    for (type_id, expected) in [
        ("mysql", "standard"),
        ("sqlite", "file"),
        ("redis", "redis"),
        ("kiwi", "kiwi"),
    ] {
        apply(&mut state, &mut h, vec![FormAction::ChangeDatabaseType(type_id.to_owned())]);
        assert_eq!(state.form_variant(), expected, "{type_id} form variant");
    }
}

#[test]
fn switching_types_snapshots_and_restores_per_type_state() {
    let mut h = host();
    let mut state = ConnectionFormState::default();
    assert_eq!(state.database, "postgres");
    assert_eq!(state.username, "postgres");

    apply(&mut state, &mut h, vec![FormAction::SetName("My PG Name".to_owned())]);
    apply(&mut state, &mut h, vec![FormAction::ChangeDatabaseType("mysql".to_owned())]);
    assert_eq!(state.database, "");
    assert_eq!(state.username, "root");
    assert_eq!(state.port, "3306");
    assert_eq!(state.name, "", "first switch to a type clears the name");

    apply(
        &mut state,
        &mut h,
        vec![
            FormAction::SetName("My MySQL Name".to_owned()),
            FormAction::SetHost("mysql.example.com".to_owned()),
        ],
    );
    apply(&mut state, &mut h, vec![FormAction::ChangeDatabaseType("sqlite".to_owned())]);
    assert_eq!(state.database, "");
    assert_eq!(state.username, "");
    assert_eq!(state.name, "");

    apply(&mut state, &mut h, vec![FormAction::ChangeDatabaseType("mysql".to_owned())]);
    assert_eq!(state.name, "My MySQL Name", "returning restores the snapshot");
    assert_eq!(state.host, "mysql.example.com");
    assert_eq!(state.port, "3306");

    apply(&mut state, &mut h, vec![FormAction::ChangeDatabaseType("postgresql".to_owned())]);
    assert_eq!(state.database, "postgres");
    assert_eq!(state.username, "postgres");
    assert_eq!(state.port, "5432");
    assert_eq!(state.name, "My PG Name");
}

#[test]
fn redis_ssl_is_normalized_to_disabled_without_tls_options() {
    let mut existing = existing("redis-old");
    existing.database_type = "redis".to_owned();
    existing.database = Some("0".to_owned());
    existing.options.insert("topology".to_owned(), "standalone".to_owned());
    let options = ConnectionFormOptions {
        edit_id: Some("redis-old".to_owned()),
        existing_connections: vec![existing],
        ..ConnectionFormOptions::default()
    };
    let h = host();
    let state = new_state(options, &h);
    assert_eq!(state.ssl_mode, "prefer", "the form keeps the loaded ssl mode");
    let config = state.on_test_config_for_test(&h);
    assert_eq!(config.ssl_mode, "disable", "the config normalizes legacy redis SSL");
}

#[test]
fn driver_read_only_forces_the_checkbox_on() {
    let mut h = host();
    let mut state = ConnectionFormState::default();
    state.driver_read_only = true;
    apply(&mut state, &mut h, vec![FormAction::SetReadOnly(false)]);
    assert!(state.read_only, "a read-only driver cannot be unchecked");
    state.driver_read_only = false;
    apply(&mut state, &mut h, vec![FormAction::SetReadOnly(false)]);
    assert!(!state.read_only, "a normal driver accepts the change");
}

// ---- Test / save commands ----------------------------------------------------

#[test]
fn on_test_sends_a_plain_config_and_publishes_the_server_version() {
    let mut h = host();
    let mut state = ConnectionFormState::default();
    apply(
        &mut state,
        &mut h,
        vec![
            FormAction::SetName("Test PG".to_owned()),
            FormAction::SetHost("127.0.0.1".to_owned()),
            FormAction::SetPort("5432".to_owned()),
            FormAction::Test,
        ],
    );
    assert_eq!(h.test_calls.len(), 1);
    let config = &h.test_calls[0];
    assert_eq!(config.database_type, "postgresql");
    assert_eq!(config.name, "Test PG");
    assert!(!format!("{config:?}").contains("setName"), "no form setters leak into the config");
    assert_eq!(state.test_ok.as_deref(), Some("16.0"));
    assert_eq!(state.test_err, None);
    assert!(!state.testing, "the busy flag settles synchronously");
}

#[test]
fn on_save_passes_the_clear_credentials_choice_on_an_edit() {
    let mut existing = existing("c1");
    existing.name = "Local database".to_owned();
    let options = ConnectionFormOptions {
        edit_id: Some("c1".to_owned()),
        existing_connections: vec![existing],
        ..ConnectionFormOptions::default()
    };
    let mut h = host();
    let mut state = new_state(options, &h);
    apply(
        &mut state,
        &mut h,
        vec![FormAction::SetClearStoredCredentials(true), FormAction::Save],
    );
    assert_eq!(h.save_calls.len(), 1);
    assert_eq!(h.save_calls[0].0.id, "c1");
    assert_eq!(h.save_calls[0].0.password, None);
    assert!(h.save_calls[0].1, "the explicit clear choice is passed through");
    assert_eq!(h.after_save, 1);
}

#[test]
fn a_dangling_tunnel_reference_blocks_save_before_any_command() {
    let mut existing = existing("c-dangling");
    existing.tunnel_id = Some("tun_gone".to_owned());
    existing.tunnel_kind = Some(TunnelKind::Ssh);
    let options = ConnectionFormOptions {
        edit_id: Some("c-dangling".to_owned()),
        existing_connections: vec![existing],
        ..ConnectionFormOptions::default()
    };
    let h = host();
    let mut state = new_state(options, &h);
    assert_eq!(state.tunnel_id.as_deref(), Some("tun_gone"));
    assert_eq!(state.tunnel_source, TunnelSource::Saved);
    assert!(state.tunnel_ref_missing());

    let mut h = host();
    apply(&mut state, &mut h, vec![FormAction::Save]);
    assert!(h.save_calls.is_empty(), "save is refused");
    assert!(!state.validate(&h));
    assert_eq!(state.validation_errors.get("tunnelId").map(String::as_str), Some("newConn.tunnelMissing"));
}

// ---- Tunnels -----------------------------------------------------------------

#[test]
fn ssh_agent_and_jump_tunnel_is_built_into_the_config() {
    let mut h = host();
    let mut state = ConnectionFormState::default();
    apply(
        &mut state,
        &mut h,
        vec![
            FormAction::SetSshEnabled(true),
            FormAction::SetSshHost("bastion".to_owned()),
            FormAction::SetSshPort("22".to_owned()),
            FormAction::SetSshUsername("ops".to_owned()),
            FormAction::SetSshAuthMethod(SshAuthMethod::Agent),
            FormAction::SetSshJumpEnabled(true),
            FormAction::SetSshJumpHost("inner".to_owned()),
            FormAction::SetSshJumpUsername("db".to_owned()),
            FormAction::SetReadOnly(true),
        ],
    );
    let config = state.on_test_config_for_test(&h);
    let ssh = config.ssh_tunnel.unwrap_or_else(|| panic!("SSH inline config is present"));
    assert_eq!(ssh.auth_method, SshAuthMethod::Agent);
    assert_eq!(ssh.jump.unwrap_or_else(|| panic!("jump is present")).host, "inner");
    assert!(config.read_only);
}

#[test]
fn set_tunnel_kind_enables_ssh_and_builds_proxy_and_ws_configs() {
    let mut h = host();
    let mut state = ConnectionFormState::default();
    apply(
        &mut state,
        &mut h,
        vec![
            FormAction::SetTunnelKind(Some(TunnelKind::Ssh)),
            FormAction::SetSshHost("bastion".to_owned()),
            FormAction::SetSshUsername("ops".to_owned()),
            FormAction::SetSshAuthMethod(SshAuthMethod::Agent),
        ],
    );
    assert!(state.ssh_enabled);
    assert_eq!(state.tunnel_kind, Some(TunnelKind::Ssh));

    apply(
        &mut state,
        &mut h,
        vec![
            FormAction::SetTunnelKind(Some(TunnelKind::HttpProxy)),
            FormAction::SetHttpProxyHost("proxy.example".to_owned()),
            FormAction::SetHttpProxyPort("8080".to_owned()),
            FormAction::SetHttpProxyScheme(HttpProxyScheme::Https),
        ],
    );
    let config = state.on_test_config_for_test(&h);
    assert_eq!(config.tunnel_kind.as_deref(), Some("httpProxy"));
    let proxy = config.http_proxy_tunnel.unwrap_or_else(|| panic!("proxy config is present"));
    assert_eq!(proxy.host, "proxy.example");
    assert_eq!(proxy.scheme, HttpProxyScheme::Https);

    apply(
        &mut state,
        &mut h,
        vec![
            FormAction::SetTunnelKind(Some(TunnelKind::WebSocket)),
            FormAction::SetWsUrl("wss://relay.example/v1".to_owned()),
            FormAction::SetWsMode(WsMode::RawBinary),
            FormAction::SetWsAuthToken("tok".to_owned()),
        ],
    );
    let config = state.on_test_config_for_test(&h);
    assert_eq!(config.tunnel_kind.as_deref(), Some("websocket"));
    let ws = config.websocket_tunnel.unwrap_or_else(|| panic!("WS config is present"));
    assert_eq!(ws.url, "wss://relay.example/v1");
    assert_eq!(ws.mode, WsMode::RawBinary);
}

#[test]
fn tunnel_validation_requires_proxy_host_port_and_ws_url() {
    let mut h = host();
    let mut state = ConnectionFormState::default();
    apply(
        &mut state,
        &mut h,
        vec![
            FormAction::SetHost("127.0.0.1".to_owned()),
            FormAction::SetPort("5432".to_owned()),
            FormAction::SetTunnelKind(Some(TunnelKind::HttpProxy)),
            FormAction::SetHttpProxyHost(String::new()),
            FormAction::SetHttpProxyPort(String::new()),
        ],
    );
    assert!(!state.validate(&h));
    assert_eq!(state.validation_errors.get("httpProxyHost").map(String::as_str), Some("newConn.required"));
    assert_eq!(state.validation_errors.get("httpProxyPort").map(String::as_str), Some("newConn.required"));

    apply(
        &mut state,
        &mut h,
        vec![FormAction::SetTunnelKind(Some(TunnelKind::WebSocket)), FormAction::SetWsUrl(String::new())],
    );
    assert!(!state.validate(&h));
    assert_eq!(state.validation_errors.get("wsUrl").map(String::as_str), Some("newConn.required"));

    apply(
        &mut state,
        &mut h,
        vec![
            FormAction::SetTunnelKind(Some(TunnelKind::Ssh)),
            FormAction::SetSshHost(String::new()),
            FormAction::SetSshUsername(String::new()),
        ],
    );
    assert!(!state.validate(&h));
    assert_eq!(state.validation_errors.get("sshHost").map(String::as_str), Some("newConn.required"));
    assert_eq!(state.validation_errors.get("sshUsername").map(String::as_str), Some("newConn.required"));
}

#[test]
fn existing_connections_hydrate_inline_tunnels() {
    let mut http = existing("c-http");
    http.tunnel_kind = Some(TunnelKind::HttpProxy);
    http.http_proxy = Some(ExistingHttpProxy {
        enabled: true,
        host: "proxy.example".to_owned(),
        port: 3128,
        scheme: "http".to_owned(),
        username: "u".to_owned(),
        password: "p".to_owned(),
        connect_timeout_secs: 20,
    });
    let h = host();
    let state = new_state(
        ConnectionFormOptions {
            edit_id: Some("c-http".to_owned()),
            existing_connections: vec![http],
            ..ConnectionFormOptions::default()
        },
        &h,
    );
    assert_eq!(state.tunnel_kind, Some(TunnelKind::HttpProxy));
    assert_eq!(state.http_proxy_host, "proxy.example");
    assert_eq!(state.http_proxy_port, "3128");

    let mut ws = existing("c-ws");
    ws.tunnel_kind = Some(TunnelKind::WebSocket);
    ws.websocket = Some(ExistingWebSocket {
        enabled: true,
        url: "wss://relay.example/t".to_owned(),
        mode: "datazen_v1".to_owned(),
        auth_token: "abc".to_owned(),
        connect_timeout_secs: 15,
    });
    let state = new_state(
        ConnectionFormOptions {
            edit_id: Some("c-ws".to_owned()),
            existing_connections: vec![ws],
            ..ConnectionFormOptions::default()
        },
        &h,
    );
    assert_eq!(state.tunnel_kind, Some(TunnelKind::WebSocket));
    assert_eq!(state.ws_url, "wss://relay.example/t");
    assert_eq!(state.ws_auth_token, "abc");
}

#[test]
fn saved_reference_is_kept_when_the_inline_kind_changes() {
    let mut h = host();
    h.tunnels = vec![SavedTunnel {
        id: "tun_ws".to_owned(),
        name: "Relay".to_owned(),
        kind: "websocket".to_owned(),
    }];
    let mut state = ConnectionFormState::default();
    apply(&mut state, &mut h, vec![FormAction::SetTunnelId(Some("tun_ws".to_owned()))]);
    assert_eq!(state.tunnel_source, TunnelSource::Saved);
    assert_eq!(state.tunnel_id.as_deref(), Some("tun_ws"));
    assert!(!state.tunnel_ref_missing());

    apply(&mut state, &mut h, vec![FormAction::SetTunnelKind(Some(TunnelKind::HttpProxy))]);
    assert_eq!(state.tunnel_id.as_deref(), Some("tun_ws"), "the reference survives");
    assert_eq!(state.tunnel_source, TunnelSource::Saved);
    let config = state.on_test_config_for_test(&h);
    assert_eq!(config.tunnel_id.as_deref(), Some("tun_ws"));
    assert!(config.http_proxy_tunnel.is_none(), "a saved reference never embeds inline config");
}

#[test]
fn a_resolved_saved_tunnel_does_not_embed_inline_config() {
    let mut h = host();
    h.tunnels = vec![SavedTunnel {
        id: "tun_ssh".to_owned(),
        name: "Bastion".to_owned(),
        kind: "ssh".to_owned(),
    }];
    let mut existing = existing("c-saved");
    existing.tunnel_id = Some("tun_ssh".to_owned());
    existing.tunnel_kind = Some(TunnelKind::Ssh);
    let state = new_state(
        ConnectionFormOptions {
            edit_id: Some("c-saved".to_owned()),
            existing_connections: vec![existing],
            ..ConnectionFormOptions::default()
        },
        &h,
    );
    assert_eq!(state.tunnel_source, TunnelSource::Saved);
    assert_eq!(state.saved_tunnel().map(|(_, name)| name), Some("Bastion"));
    assert!(!state.tunnel_ref_missing());
    let config = state.on_test_config_for_test(&h);
    assert_eq!(config.tunnel_id.as_deref(), Some("tun_ssh"));
    assert_eq!(config.tunnel_kind.as_deref(), Some("ssh"));
    assert!(config.ssh_tunnel.is_none());
}
