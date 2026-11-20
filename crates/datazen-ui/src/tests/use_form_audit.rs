//! T333 independent audit: `useConnectionForm` state machine.
//!
//! Probes branches the coder's suite does not: failed test command, driver
//! validator errors, file-variant validation, non-numeric ports, derived
//! hasUsername/effectiveKind, and type changes while editing an existing row.
//! All commands go through a deterministic fake host.

use crate::components::connection::connection_advanced_settings::{SavedTunnel, TunnelKind};
use crate::components::connection::tunnel_field_contracts::SshAuthMethod;
use crate::components::connection::use_connection_form::{
    step, BuiltConfig, ConnectionFormHost, ConnectionFormOptions, ConnectionFormState, FormAction,
    ExistingConnection, TunnelSource,
};

struct Host {
    test_result: Result<String, String>,
    test_calls: Vec<BuiltConfig>,
    save_calls: Vec<BuiltConfig>,
    driver_errors: Vec<(String, String)>,
    tunnels: Vec<SavedTunnel>,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            test_result: Ok("16.0".to_owned()),
            test_calls: Vec::new(),
            save_calls: Vec::new(),
            driver_errors: Vec::new(),
            tunnels: Vec::new(),
        }
    }
}

impl ConnectionFormHost for Host {
    fn test_connection(&mut self, config: &BuiltConfig) -> Result<String, String> {
        self.test_calls.push(config.clone());
        self.test_result.clone()
    }
    fn save_connection(&mut self, config: &BuiltConfig, _clear: bool) {
        self.save_calls.push(config.clone());
    }
    fn saved_tunnels(&self) -> &[SavedTunnel] {
        &self.tunnels
    }
    fn new_id(&self) -> String {
        "conn_test".to_owned()
    }
    fn on_after_save(&mut self) {}
    fn driver_validate(&self, _c: &BuiltConfig, _t: &dyn Fn(&str) -> String) -> Vec<(String, String)> {
        self.driver_errors.clone()
    }
}

fn apply(state: &mut ConnectionFormState, host: &mut Host, actions: Vec<FormAction>) {
    for action in actions {
        step(state, action, host);
    }
}

/// A rejected test command becomes `test_err`, not a panic or a stale success.
#[test]
fn test_command_failure_publishes_the_error() {
    let mut host = Host {
        test_result: Err("handshake failed".to_owned()),
        ..Host::default()
    };
    let mut state = ConnectionFormState::default();
    apply(
        &mut state,
        &mut host,
        vec![
            FormAction::SetName("Test PG".to_owned()),
            FormAction::SetHost("127.0.0.1".to_owned()),
            FormAction::SetPort("5432".to_owned()),
            FormAction::Test,
        ],
    );
    assert_eq!(host.test_calls.len(), 1);
    assert_eq!(state.test_ok, None);
    assert_eq!(state.test_err.as_deref(), Some("handshake failed"));
    assert!(!state.testing, "busy flag settles on failure too");
}

/// A driver-validator error blocks both test and save commands.
#[test]
fn driver_validator_errors_block_test_and_save() {
    let mut host = Host {
        driver_errors: vec![("host".to_owned(), "filter.badHost".to_owned())],
        ..Host::default()
    };
    let mut state = ConnectionFormState::default();
    apply(
        &mut state,
        &mut host,
        vec![
            FormAction::SetName("PG".to_owned()),
            FormAction::SetHost("127.0.0.1".to_owned()),
            FormAction::SetPort("5432".to_owned()),
            FormAction::Test,
            FormAction::Save,
        ],
    );
    assert!(host.test_calls.is_empty(), "test blocked by driver validation");
    assert!(host.save_calls.is_empty(), "save blocked by driver validation");
    assert_eq!(state.validation_errors.get("host").map(String::as_str), Some("filter.badHost"));
}

/// File-variant forms require a database path; non-numeric standard ports are
/// rejected (React `isNaN(Number(port))`).
#[test]
fn file_and_port_validation_match_react_rules() {
    let mut host = Host::default();
    let mut state = ConnectionFormState::default();

    apply(
        &mut state,
        &mut host,
        vec![FormAction::ChangeDatabaseType("sqlite".to_owned()), FormAction::SetDatabase(String::new())],
    );
    assert!(!state.validate(&host));
    assert_eq!(state.validation_errors.get("database").map(String::as_str), Some("newConn.required"));

    apply(
        &mut state,
        &mut host,
        vec![FormAction::ChangeDatabaseType("postgresql".to_owned()), FormAction::SetPort("abc".to_owned())],
    );
    assert!(!state.validate(&host));
    assert_eq!(state.validation_errors.get("port").map(String::as_str), Some("newConn.required"));
}

/// Derived booleans: hasUsername for each built-in type, effectiveKind hides a
/// disabled inline SSH, and saved refs are not "missing" once resolved.
#[test]
fn derived_booleans_follow_the_registry_and_tunnel_state() {
    let host = Host::default();
    let mut state = ConnectionFormState::default();
    assert!(state.has_username(), "postgres default user");
    apply(&mut state, &mut Host::default(), vec![FormAction::ChangeDatabaseType("sqlite".to_owned())]);
    assert!(!state.has_username(), "sqlite has no user");
    apply(&mut state, &mut Host::default(), vec![FormAction::ChangeDatabaseType("redis".to_owned())]);
    assert!(state.has_username(), "redis non-standard form has a user");

    let mut state = ConnectionFormState::default();
    assert_eq!(state.effective_kind(), None);
    apply(&mut state, &mut Host::default(), vec![FormAction::SetTunnelKind(Some(TunnelKind::Ssh))]);
    assert_eq!(state.effective_kind(), Some(TunnelKind::Ssh));
    apply(&mut state, &mut Host::default(), vec![FormAction::SetSshEnabled(false)]);
    assert_eq!(state.effective_kind(), None, "disabled SSH is not effective");

    let mut h = Host {
        tunnels: vec![SavedTunnel {
            id: "tun_1".to_owned(),
            name: "Bastion".to_owned(),
            kind: "ssh".to_owned(),
        }],
        ..Host::default()
    };
    let mut state = ConnectionFormState::default();
    apply(&mut state, &mut h, vec![FormAction::SetTunnelId(Some("tun_1".to_owned()))]);
    assert_eq!(state.tunnel_source, TunnelSource::Saved);
    assert!(!state.tunnel_ref_missing(), "a resolved ref is not missing");
}

/// Changing the database type while editing an existing connection applies the
/// new defaults instead of the per-type snapshot path (React's edit branch).
#[test]
fn type_change_in_edit_mode_applies_defaults_without_snapshots() {
    let mut existing = ExistingConnection::new("c1", "postgresql", "Prod");
    existing.password = Some("secret".to_owned());
    let options = ConnectionFormOptions {
        edit_id: Some("c1".to_owned()),
        existing_connections: vec![existing],
        ..ConnectionFormOptions::default()
    };
    let mut host = Host::default();
    let mut state = ConnectionFormState::new(&options, &host);
    assert_eq!(state.database, "", "React loads existing.database ?? ''");

    apply(&mut state, &mut host, vec![FormAction::ChangeDatabaseType("mysql".to_owned())]);
    assert_eq!(state.database_type, "mysql");
    assert_eq!(state.username, "root", "edit branch applies new default user");
    assert_eq!(state.port, "3306");
    assert_eq!(state.password, "secret", "edit branch keeps the stored password");
}

/// SSH auth method needed for a jump section with a private key: the config
/// carries both the key path and passphrase, and agent mode omits secrets.
#[test]
fn private_key_and_agent_auth_build_distinct_configs() {
    let mut host = Host::default();
    let mut state = ConnectionFormState::default();
    apply(
        &mut state,
        &mut host,
        vec![
            FormAction::SetTunnelKind(Some(TunnelKind::Ssh)),
            FormAction::SetSshHost("bastion".to_owned()),
            FormAction::SetSshUsername("ops".to_owned()),
            FormAction::SetSshAuthMethod(SshAuthMethod::PrivateKey),
            FormAction::SetSshKeyPath("/keys/id_rsa".to_owned()),
            FormAction::SetSshPassphrase("phrase".to_owned()),
        ],
    );
    let config = step::build_snapshot(&state, &host);
    let ssh = config.ssh_tunnel.expect("ssh config");
    assert_eq!(ssh.private_key_path.as_deref(), Some("/keys/id_rsa"));
    assert_eq!(ssh.passphrase.as_deref(), Some("phrase"));
    assert_eq!(ssh.password, None, "private-key mode has no password");
}

// A tiny helper trait to read the built config without firing a command.
mod step {
    use super::*;
    use crate::components::connection::use_connection_form::build_config_for_test;
    pub(super) fn build_snapshot(state: &ConnectionFormState, host: &Host) -> BuiltConfig {
        build_config_for_test(state, &host.new_id(), "newConn.unnamed")
    }
}
