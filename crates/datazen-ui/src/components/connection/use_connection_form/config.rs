//! `connectionFormModel` logic used by the form hook: per-type snapshots,
//! type defaults and the plain `ConnectionConfig` builder. No IPC is performed;
//! the host receives a [`BuiltConfig`] value.

use std::collections::BTreeMap;

use super::meta::{self, ConnectionMode, DatabaseFieldType};
use super::ConnectionFormState;
use crate::components::connection::tunnel_field_contracts::{
    HttpProxyScheme, SshAuthMethod, WsMode,
};
use crate::components::connection::connection_advanced_settings::TunnelKind as Kind;

/// React `ConnectionFormSnapshot` — the fields saved when switching driver types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormSnapshot {
    pub name: String,
    pub host: String,
    pub port: String,
    pub database: String,
    pub schema: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: String,
    pub group: String,
    pub color_tag: String,
    pub read_only: bool,
    pub options: BTreeMap<String, String>,
    pub show_advanced: bool,
    pub ssh_enabled: bool,
    pub ssh_host: String,
    pub ssh_port: String,
    pub ssh_username: String,
    pub ssh_auth_method: SshAuthMethod,
    pub ssh_password: String,
    pub ssh_key_path: String,
    pub ssh_passphrase: String,
    pub ssh_jump_enabled: bool,
    pub ssh_jump_host: String,
    pub ssh_jump_port: String,
    pub ssh_jump_username: String,
    pub ssh_jump_auth_method: SshAuthMethod,
    pub ssh_jump_password: String,
    pub ssh_jump_key_path: String,
    pub ssh_jump_passphrase: String,
}

pub fn capture(state: &ConnectionFormState) -> FormSnapshot {
    FormSnapshot {
        name: state.name.clone(),
        host: state.host.clone(),
        port: state.port.clone(),
        database: state.database.clone(),
        schema: state.schema.clone(),
        username: state.username.clone(),
        password: state.password.clone(),
        ssl_mode: state.ssl_mode.clone(),
        group: state.group.clone(),
        color_tag: state.color_tag.clone(),
        read_only: state.read_only,
        options: meta::sanitize_options(&state.options),
        show_advanced: state.show_advanced,
        ssh_enabled: state.ssh_enabled,
        ssh_host: state.ssh_host.clone(),
        ssh_port: state.ssh_port.clone(),
        ssh_username: state.ssh_username.clone(),
        ssh_auth_method: state.ssh_auth_method,
        ssh_password: state.ssh_password.clone(),
        ssh_key_path: state.ssh_key_path.clone(),
        ssh_passphrase: state.ssh_passphrase.clone(),
        ssh_jump_enabled: state.ssh_jump_enabled,
        ssh_jump_host: state.ssh_jump_host.clone(),
        ssh_jump_port: state.ssh_jump_port.clone(),
        ssh_jump_username: state.ssh_jump_username.clone(),
        ssh_jump_auth_method: state.ssh_jump_auth_method,
        ssh_jump_password: state.ssh_jump_password.clone(),
        ssh_jump_key_path: state.ssh_jump_key_path.clone(),
        ssh_jump_passphrase: state.ssh_jump_passphrase.clone(),
    }
}

pub fn restore(state: &mut ConnectionFormState, snapshot: &FormSnapshot) {
    state.name = snapshot.name.clone();
    state.host = snapshot.host.clone();
    state.port = snapshot.port.clone();
    state.database = snapshot.database.clone();
    state.schema = snapshot.schema.clone();
    state.username = snapshot.username.clone();
    state.password = snapshot.password.clone();
    state.ssl_mode = snapshot.ssl_mode.clone();
    state.group = snapshot.group.clone();
    state.color_tag = snapshot.color_tag.clone();
    let target_ro = meta::db_meta(&state.database_type).is_some_and(|m| m.read_only);
    state.read_only = target_ro || snapshot.read_only;
    state.options = meta::sanitize_options(&snapshot.options);
    state.show_advanced = snapshot.show_advanced;
    state.hydrate_tunnel(None, if snapshot.ssh_enabled { Some(Kind::Ssh) } else { None });
    state.ssh_host = snapshot.ssh_host.clone();
    state.ssh_port = snapshot.ssh_port.clone();
    state.ssh_username = snapshot.ssh_username.clone();
    state.ssh_auth_method = snapshot.ssh_auth_method;
    state.ssh_password = snapshot.ssh_password.clone();
    state.ssh_key_path = snapshot.ssh_key_path.clone();
    state.ssh_passphrase = snapshot.ssh_passphrase.clone();
    state.ssh_jump_enabled = snapshot.ssh_jump_enabled;
    state.ssh_jump_host = snapshot.ssh_jump_host.clone();
    state.ssh_jump_port = snapshot.ssh_jump_port.clone();
    state.ssh_jump_username = snapshot.ssh_jump_username.clone();
    state.ssh_jump_auth_method = snapshot.ssh_jump_auth_method;
    state.ssh_jump_password = snapshot.ssh_jump_password.clone();
    state.ssh_jump_key_path = snapshot.ssh_jump_key_path.clone();
    state.ssh_jump_passphrase = snapshot.ssh_jump_passphrase.clone();
}

/// `applyTypeDefaults` from the React hook.
pub fn apply_type_defaults(state: &mut ConnectionFormState, new_type: &str) {
    let Some(new_meta) = meta::db_meta(new_type) else {
        return;
    };
    state.name.clear();
    state.host = if new_meta.default_host.is_empty() {
        "127.0.0.1".to_owned()
    } else {
        new_meta.default_host.to_owned()
    };
    state.port = if new_meta.default_port == 0 {
        String::new()
    } else {
        new_meta.default_port.to_string()
    };
    state.username = new_meta.default_user.to_owned();
    state.ssl_mode = new_meta.default_ssl_mode.to_owned();
    state.reset_tunnel();
    state.database = match new_meta.database_field_type {
        DatabaseFieldType::Index => {
            if new_meta.default_database.is_empty() {
                "0".to_owned()
            } else {
                new_meta.default_database.to_owned()
            }
        }
        _ if new_meta.connection_mode == ConnectionMode::File => String::new(),
        _ => new_meta.default_database.to_owned(),
    };
    if new_meta.connection_includes_schema {
        state.schema = "default".to_owned();
    }
    if new_meta.read_only {
        state.read_only = true;
    }
    state.options = meta::sanitize_options(
        &new_meta
            .default_options
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    );
}

// ---- Plain connection config ------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JumpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
    pub password: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
    pub jump: Option<JumpConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpProxyConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub scheme: HttpProxyScheme,
    pub username: Option<String>,
    pub password: Option<String>,
    pub connect_timeout_secs: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketConfig {
    pub enabled: bool,
    pub url: String,
    pub mode: WsMode,
    pub auth_token: Option<String>,
    pub connect_timeout_secs: u16,
}

/// Plain, JSON-shaped output of `buildConnectionConfig`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltConfig {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ssl_mode: String,
    pub group: Option<String>,
    pub color_tag: Option<String>,
    pub read_only: bool,
    pub tunnel_kind: Option<String>,
    pub tunnel_id: Option<String>,
    pub ssh_tunnel: Option<SshConfig>,
    pub http_proxy_tunnel: Option<HttpProxyConfig>,
    pub websocket_tunnel: Option<WebSocketConfig>,
    pub options: BTreeMap<String, String>,
}

fn value_of_kind(kind: Option<Kind>) -> Option<String> {
    match kind {
        Some(Kind::Ssh) => Some("ssh".to_owned()),
        Some(Kind::HttpProxy) => Some("httpProxy".to_owned()),
        Some(Kind::WebSocket) => Some("websocket".to_owned()),
        None => None,
    }
}

fn ssh_config(state: &ConnectionFormState) -> Option<SshConfig> {
    if state.effective_kind() != Some(Kind::Ssh) || !state.ssh_enabled {
        return None;
    }
    let jump = if state.ssh_jump_enabled {
        Some(JumpConfig {
            enabled: true,
            host: state.ssh_jump_host.clone(),
            port: state.ssh_jump_port.parse::<u16>().unwrap_or(22),
            username: state.ssh_jump_username.clone(),
            auth_method: state.ssh_jump_auth_method,
            password: (state.ssh_jump_auth_method == SshAuthMethod::Password)
                .then(|| state.ssh_jump_password.clone())
                .filter(|v| !v.is_empty()),
            private_key_path: (state.ssh_jump_auth_method == SshAuthMethod::PrivateKey)
                .then(|| state.ssh_jump_key_path.clone())
                .filter(|v| !v.is_empty()),
            passphrase: (state.ssh_jump_auth_method == SshAuthMethod::PrivateKey)
                .then(|| state.ssh_jump_passphrase.clone())
                .filter(|v| !v.is_empty()),
        })
    } else {
        None
    };
    Some(SshConfig {
        enabled: true,
        host: state.ssh_host.clone(),
        port: state.ssh_port.parse::<u16>().unwrap_or(22),
        username: state.ssh_username.clone(),
        auth_method: state.ssh_auth_method,
        password: (state.ssh_auth_method == SshAuthMethod::Password)
            .then(|| state.ssh_password.clone())
            .filter(|v| !v.is_empty()),
        private_key_path: (state.ssh_auth_method == SshAuthMethod::PrivateKey)
            .then(|| state.ssh_key_path.clone())
            .filter(|v| !v.is_empty()),
        passphrase: (state.ssh_auth_method == SshAuthMethod::PrivateKey)
            .then(|| state.ssh_passphrase.clone())
            .filter(|v| !v.is_empty()),
        jump,
    })
}

fn http_proxy_config(state: &ConnectionFormState) -> Option<HttpProxyConfig> {
    if state.effective_kind() != Some(Kind::HttpProxy) {
        return None;
    }
    Some(HttpProxyConfig {
        enabled: true,
        host: state.http_proxy_host.clone(),
        port: state.http_proxy_port.parse::<u16>().unwrap_or(8080),
        scheme: state.http_proxy_scheme,
        username: Some(state.http_proxy_username.clone()).filter(|v| !v.is_empty()),
        password: Some(state.http_proxy_password.clone()).filter(|v| !v.is_empty()),
        connect_timeout_secs: state.http_proxy_timeout.parse::<u16>().unwrap_or(30),
    })
}

fn websocket_config(state: &ConnectionFormState) -> Option<WebSocketConfig> {
    if state.effective_kind() != Some(Kind::WebSocket) {
        return None;
    }
    Some(WebSocketConfig {
        enabled: true,
        url: state.ws_url.clone(),
        mode: state.ws_mode,
        auth_token: Some(state.ws_auth_token.clone()).filter(|v| !v.is_empty()),
        connect_timeout_secs: state.ws_timeout.parse::<u16>().unwrap_or(30),
    })
}

/// `buildConnectionConfig` with the tunnel rules: a saved reference skips all
/// inline tunnel configs but keeps the kind hint.
pub fn build_config(
    state: &ConnectionFormState,
    new_id: &str,
    unnamed_label: &str,
) -> BuiltConfig {
    let meta = meta::db_meta(&state.database_type);
    let effective_ssl = if meta.is_some_and(|m| m.default_ssl_mode == "disable")
        && !meta::has_enabled_tls_option(&state.options)
    {
        "disable".to_owned()
    } else {
        state.ssl_mode.clone()
    };
    let saved = state.tunnel_id.is_some();
    let kind = value_of_kind(state.effective_kind());
    let (host, port, database, schema, username, password) = match meta {
        None => (None, None, Some(state.database.clone()), None, None, None),
        Some(m) if m.connection_mode == ConnectionMode::File => {
            (None, None, Some(state.database.clone()), None, None, None)
        }
        Some(m) => {
            let db = match m.database_field_type {
                DatabaseFieldType::Index => {
                    let index = state.database.parse::<u16>().unwrap_or(0);
                    Some(index.to_string())
                }
                _ => Some(state.database.clone()).filter(|v| !v.is_empty()),
            };
            let user = if !m.default_user.is_empty() || m.connection_includes_schema {
                Some(state.username.clone()).filter(|v| !v.is_empty())
            } else {
                None
            };
            (
                Some(state.host.clone()).filter(|v| !v.is_empty()),
                state.port.parse::<u16>().ok(),
                db,
                m.connection_includes_schema.then(|| {
                    if state.schema.trim().is_empty() {
                        "default".to_owned()
                    } else {
                        state.schema.trim().to_owned()
                    }
                }),
                user,
                Some(state.password.clone()).filter(|v| !v.is_empty()),
            )
        }
    };

    BuiltConfig {
        id: state.edit_id.clone().unwrap_or_else(|| new_id.to_owned()),
        name: if state.name.is_empty() {
            unnamed_label.to_owned()
        } else {
            state.name.clone()
        },
        database_type: state.database_type.clone(),
        host,
        port,
        database,
        schema,
        username,
        password,
        ssl_mode: effective_ssl,
        group: Some(state.group.clone()).filter(|v| !v.is_empty()),
        color_tag: Some(state.color_tag.clone()).filter(|v| !v.is_empty()),
        read_only: meta.is_some_and(|m| m.read_only) || state.read_only,
        tunnel_kind: kind.filter(|v| v != "none"),
        tunnel_id: state.tunnel_id.clone(),
        ssh_tunnel: if saved { None } else { ssh_config(state) },
        http_proxy_tunnel: if saved { None } else { http_proxy_config(state) },
        websocket_tunnel: if saved { None } else { websocket_config(state) },
        options: state.options.clone(),
    }
}

// Leftover L1: `BuiltConfig` duplicates the driver-sdk `ConnectionConfig` DTO;
// replace it with the shared DTO when that lands in datazen-ui.
