//! Port of `packages/frontend/src/components/connection/useConnectionForm.ts`.
//!
//! React's hook is a bag of `useState`s plus callbacks. Rune is immediate mode,
//! so the same state lives in [`ConnectionFormState`] owned by the host and every
//! setter is a [`FormAction`] applied by [`step`]. Commands (test/save, saved
//! tunnel lookup, id generation) go through the injected [`ConnectionFormHost`]:
//! no IPC, thread or future is created here.
//!
//! The React tests are the spec: defaults, group coercion, per-driver type
//! snapshots, read-only forcing, validation, and inline/saved tunnel flows.
//! `DB_REGISTRY` and the form-to-IPC config builder are documented stand-ins in
//! [`meta`] / [`config`], removed when the shared DTO/registry land.

use std::collections::BTreeMap;

use crate::components::connection::connection_advanced_settings::{SavedTunnel, TunnelKind};
use crate::components::connection::tunnel_field_contracts::{
    HttpProxyScheme, SshAuthMethod, WsMode,
};

mod config;
mod meta;

pub use config::{build_config as build_config_for_test, BuiltConfig, FormSnapshot};
pub use meta::{ExistingConnection, ExistingHttpProxy, ExistingSsh, ExistingWebSocket, GroupInput};

use config::{apply_type_defaults, build_config, capture, restore};
use meta::{coerce_group, db_meta, sanitize_options, ConnectionMode, PRESET_DEVELOPMENT};

/// React `TunnelSource`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelSource {
    None,
    Inline,
    Saved,
}

/// The two commands the hook sends to the host, plus the read-only lookups.
#[allow(dead_code)] // host port; consumer: the connection dialog (not wired).
pub trait ConnectionFormHost {
    /// `connectionCommands.testConnection` — `Ok(server_version)` on success.
    fn test_connection(&mut self, config: &BuiltConfig) -> Result<String, String>;
    fn save_connection(&mut self, config: &BuiltConfig, clear_stored_credentials: bool);
    fn saved_tunnels(&self) -> &[SavedTunnel];
    fn new_id(&self) -> String;
    fn on_after_save(&mut self);
    /// Driver-validator contribution, empty for the built-in stand-ins.
    fn driver_validate(
        &self,
        _config: &BuiltConfig,
        _t: &dyn Fn(&str) -> String,
    ) -> Vec<(String, String)> {
        Vec::new()
    }
    /// React's `t`; the identity default matches the React test mock.
    fn t(&self, key: &str) -> String {
        key.to_owned()
    }
}

/// `UseConnectionFormOptions` (host supplies the snapshot of existing rows).
#[derive(Clone, Debug, Default)]
pub struct ConnectionFormOptions {
    pub edit_id: Option<String>,
    pub existing_connections: Vec<ExistingConnection>,
    pub default_group: Option<String>,
}

/// The hook's whole observable state, mirroring the React form API.
#[allow(dead_code)] // state slice; consumer: the connection dialog (not wired).
#[derive(Clone, Debug)]
pub struct ConnectionFormState {
    pub edit_id: Option<String>,
    pub name: String,
    pub database_type: String,
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
    pub driver_read_only: bool,
    pub show_advanced: bool,
    pub clear_stored_credentials: bool,
    pub options: BTreeMap<String, String>,

    pub tunnel_source: TunnelSource,
    pub tunnel_kind: Option<TunnelKind>,
    pub tunnel_id: Option<String>,
    saved_tunnel_kind: Option<TunnelKind>,
    saved_tunnel_name: Option<String>,
    last_inline_kind: Option<TunnelKind>,
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
    pub http_proxy_host: String,
    pub http_proxy_port: String,
    pub http_proxy_scheme: HttpProxyScheme,
    pub http_proxy_username: String,
    pub http_proxy_password: String,
    pub http_proxy_timeout: String,
    pub ws_url: String,
    pub ws_mode: WsMode,
    pub ws_auth_token: String,
    pub ws_timeout: String,

    pub validation_errors: BTreeMap<String, String>,
    pub testing: bool,
    pub test_ok: Option<String>,
    pub test_err: Option<String>,
    type_snapshots: BTreeMap<String, FormSnapshot>,
    loaded: bool,
}

impl Default for ConnectionFormState {
    fn default() -> Self {
        Self {
            edit_id: None,
            name: String::new(),
            database_type: "postgresql".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: "5432".to_owned(),
            database: "postgres".to_owned(),
            schema: "default".to_owned(),
            username: "postgres".to_owned(),
            password: String::new(),
            ssl_mode: "prefer".to_owned(),
            group: PRESET_DEVELOPMENT.to_owned(),
            color_tag: "#3b82f6".to_owned(),
            read_only: false,
            driver_read_only: false,
            show_advanced: false,
            clear_stored_credentials: false,
            options: BTreeMap::new(),
            tunnel_source: TunnelSource::None,
            tunnel_kind: None,
            tunnel_id: None,
            saved_tunnel_kind: None,
            saved_tunnel_name: None,
            last_inline_kind: None,
            ssh_enabled: false,
            ssh_host: String::new(),
            ssh_port: "22".to_owned(),
            ssh_username: String::new(),
            ssh_auth_method: SshAuthMethod::Password,
            ssh_password: String::new(),
            ssh_key_path: String::new(),
            ssh_passphrase: String::new(),
            ssh_jump_enabled: false,
            ssh_jump_host: String::new(),
            ssh_jump_port: "22".to_owned(),
            ssh_jump_username: String::new(),
            ssh_jump_auth_method: SshAuthMethod::Password,
            ssh_jump_password: String::new(),
            ssh_jump_key_path: String::new(),
            ssh_jump_passphrase: String::new(),
            http_proxy_host: String::new(),
            http_proxy_port: "8080".to_owned(),
            http_proxy_scheme: HttpProxyScheme::Http,
            http_proxy_username: String::new(),
            http_proxy_password: String::new(),
            http_proxy_timeout: "30".to_owned(),
            ws_url: String::new(),
            ws_mode: WsMode::DatazenV1,
            ws_auth_token: String::new(),
            ws_timeout: "30".to_owned(),
            validation_errors: BTreeMap::new(),
            testing: false,
            test_ok: None,
            test_err: None,
            type_snapshots: BTreeMap::new(),
            loaded: false,
        }
    }
}

impl ConnectionFormState {
    #[allow(dead_code)] // host API; consumer: the connection dialog (not wired).
    pub fn new(options: &ConnectionFormOptions, host: &impl ConnectionFormHost) -> Self {
        let mut state = Self::default();
        state.edit_id = options.edit_id.clone();
        state.group = coerce_group(match &options.default_group {
            Some(value) => GroupInput::Text(value.clone()),
            None => GroupInput::Text(PRESET_DEVELOPMENT.to_owned()),
        });
        state.driver_read_only = db_meta(&state.database_type).is_some_and(|m| m.read_only);
        if let Some(id) = &options.edit_id {
            if let Some(existing) = options.existing_connections.iter().find(|c| &c.id == id) {
                state.load_existing(existing, host);
            }
        }
        state
    }

    /// `formVariant` from `DB_REGISTRY[databaseType].connectionForm`.
    #[allow(dead_code)] // derived VM; consumer: ConnectionFormBody (not wired).
    pub fn form_variant(&self) -> &'static str {
        db_meta(&self.database_type).map_or("standard", |m| m.connection_form)
    }

    #[allow(dead_code)] // derived VM; consumer: ConnectionFormBody (not wired).
    pub fn has_username(&self) -> bool {
        db_meta(&self.database_type).is_some_and(|m| {
            !m.default_user.is_empty()
                || (m.connection_form != "standard" && m.connection_form != "file")
        })
    }

    #[allow(dead_code)] // derived VM; consumer: ConnectionFormBody (not wired).
    pub fn supports_ssl(&self) -> bool {
        db_meta(&self.database_type).is_some_and(|m| m.supports_ssl)
    }

    #[allow(dead_code)] // derived VM; consumer: ConnectionFormBody (not wired).
    pub fn supports_ssh(&self) -> bool {
        db_meta(&self.database_type).is_some_and(|m| m.supports_ssh)
    }

    #[allow(dead_code)] // derived VM; consumer: ConnectionFormBody (not wired).
    pub fn effective_kind(&self) -> Option<TunnelKind> {
        if self.tunnel_source == TunnelSource::Saved {
            self.saved_tunnel_kind.or(self.tunnel_kind)
        } else if self.tunnel_kind == Some(TunnelKind::Ssh) && !self.ssh_enabled {
            None
        } else {
            self.tunnel_kind
        }
    }

    #[allow(dead_code)] // derived VM; consumer: the tunnel editor (not wired).
    pub fn saved_tunnel(&self) -> Option<(&str, &str)> {
        self.saved_tunnel_name.as_deref().map(|name| (self.tunnel_id.as_deref().unwrap_or(""), name))
    }

    #[allow(dead_code)] // derived VM; validation uses it (G3/P1-8).
    pub fn tunnel_ref_missing(&self) -> bool {
        self.tunnel_id.is_some() && self.saved_tunnel_kind.is_none()
    }

    #[allow(dead_code)] // tunnel state machine; used by load/snapshot restore.
    pub fn hydrate_tunnel(&mut self, id: Option<String>, kind: Option<TunnelKind>) {
        self.hydrate_tunnel_named(id, kind, None);
    }

    fn hydrate_tunnel_named(
        &mut self,
        id: Option<String>,
        kind: Option<TunnelKind>,
        saved_name: Option<String>,
    ) {
        if id.is_some() {
            self.tunnel_source = TunnelSource::Saved;
            self.tunnel_id = id;
            self.tunnel_kind = kind;
            self.ssh_enabled = kind == Some(TunnelKind::Ssh);
            // A reference only resolves when the host actually found the tunnel;
            // otherwise it is dangling even though the kind hint is present.
            self.saved_tunnel_kind = saved_name.as_ref().map(|_| kind).unwrap_or(None);
            self.saved_tunnel_name = saved_name;
            return;
        }
        self.tunnel_id = None;
        self.saved_tunnel_kind = None;
        self.saved_tunnel_name = None;
        match kind {
            None => {
                self.tunnel_source = TunnelSource::None;
                self.tunnel_kind = None;
                self.ssh_enabled = false;
            }
            Some(kind) => {
                self.last_inline_kind = Some(kind);
                self.tunnel_source = TunnelSource::Inline;
                self.tunnel_kind = Some(kind);
                self.ssh_enabled = kind == TunnelKind::Ssh;
            }
        }
    }

    #[allow(dead_code)] // tunnel state machine; used by type defaults.
    pub fn reset_tunnel(&mut self) {
        self.hydrate_tunnel_named(None, None, None);
        self.last_inline_kind = None;
        self.ssh_host.clear();
        self.ssh_port = "22".to_owned();
        self.ssh_username.clear();
        self.ssh_auth_method = SshAuthMethod::Password;
        self.ssh_password.clear();
        self.ssh_key_path.clear();
        self.ssh_passphrase.clear();
        self.ssh_jump_enabled = false;
        self.ssh_jump_host.clear();
        self.ssh_jump_port = "22".to_owned();
        self.ssh_jump_username.clear();
        self.ssh_jump_auth_method = SshAuthMethod::Password;
        self.ssh_jump_password.clear();
        self.ssh_jump_key_path.clear();
        self.ssh_jump_passphrase.clear();
        self.http_proxy_host.clear();
        self.http_proxy_port = "8080".to_owned();
        self.http_proxy_scheme = HttpProxyScheme::Http;
        self.http_proxy_username.clear();
        self.http_proxy_password.clear();
        self.http_proxy_timeout = "30".to_owned();
        self.ws_url.clear();
        self.ws_mode = WsMode::DatazenV1;
        self.ws_auth_token.clear();
        self.ws_timeout = "30".to_owned();
    }

    /// `sslOptions` from the hook (labelled with i18n keys via the host).
    #[allow(dead_code)] // derived VM; consumer: ConnectionFormBody (not wired).
    pub fn ssl_options(&self, host: &impl ConnectionFormHost) -> Vec<(String, String)> {
        [
            ("disable", "Disable", "newConn.sslNone"),
            ("prefer", "Prefer", "newConn.sslPrefer"),
            ("require", "Require", "newConn.sslRequire"),
        ]
        .iter()
        .map(|(value, prefix, key)| (value.to_string(), format!("{prefix} — {}", host.t(key))))
        .collect()
    }

    /// React's `validate` — tunnel prologue plus driver/generic connection rules.
    #[allow(dead_code)] // host API; consumer: test/save commands (not wired).
    pub fn validate(&mut self, host: &impl ConnectionFormHost) -> bool {
        let mut errors = BTreeMap::new();
        if self.tunnel_ref_missing() {
            errors.insert("tunnelId".to_owned(), host.t("newConn.tunnelMissing"));
        }
        let inline = self.tunnel_id.is_none();
        let kind = self.effective_kind();
        if inline && kind == Some(TunnelKind::HttpProxy) {
            if self.http_proxy_host.trim().is_empty() {
                errors.insert("httpProxyHost".to_owned(), host.t("newConn.required"));
            }
            if self.http_proxy_port.trim().is_empty()
                || self.http_proxy_port.parse::<f64>().is_err()
            {
                errors.insert("httpProxyPort".to_owned(), host.t("newConn.required"));
            }
        }
        if inline && kind == Some(TunnelKind::WebSocket) && self.ws_url.trim().is_empty() {
            errors.insert("wsUrl".to_owned(), host.t("newConn.required"));
        }
        if inline && kind == Some(TunnelKind::Ssh) && self.ssh_enabled {
            if self.ssh_host.trim().is_empty() {
                errors.insert("sshHost".to_owned(), host.t("newConn.required"));
            }
            if self.ssh_username.trim().is_empty() {
                errors.insert("sshUsername".to_owned(), host.t("newConn.required"));
            }
        }

        if let Some(meta) = db_meta(&self.database_type) {
            if meta.connection_mode == ConnectionMode::File {
                if self.database.trim().is_empty() {
                    errors.insert("database".to_owned(), host.t("newConn.required"));
                }
            } else if meta.connection_form == "standard" {
                if self.host.trim().is_empty() {
                    errors.insert("host".to_owned(), host.t("newConn.required"));
                }
                if self.port.trim().is_empty() || self.port.parse::<f64>().is_err() {
                    errors.insert("port".to_owned(), host.t("newConn.required"));
                }
            }
        }

        let config = build_config(self, &host.new_id(), &host.t("newConn.unnamed"));
        for (key, message) in host.driver_validate(&config, &|k| host.t(k)) {
            errors.insert(key, message);
        }
        self.validation_errors = errors;
        self.validation_errors.is_empty()
    }

    /// `onTest` without async: the host's synchronous result becomes ok/err state.
    #[allow(dead_code)] // host API; consumer: the connection dialog (not wired).
    pub fn on_test(&mut self, host: &mut impl ConnectionFormHost) {
        if !self.validate(host) {
            return;
        }
        let config = build_config(self, &host.new_id(), &host.t("newConn.unnamed"));
        self.testing = true;
        self.test_ok = None;
        self.test_err = None;
        match host.test_connection(&config) {
            Ok(server_version) => self.test_ok = Some(server_version),
            Err(message) => self.test_err = Some(message),
        }
        self.testing = false;
    }

    #[allow(dead_code)] // host API; consumer: the connection dialog (not wired).
    pub fn on_save(&mut self, host: &mut impl ConnectionFormHost) {
        if !self.validate(host) {
            return;
        }
        let config = build_config(self, &host.new_id(), &host.t("newConn.unnamed"));
        host.save_connection(&config, self.clear_stored_credentials);
        host.on_after_save();
    }

    pub fn handle_database_type_change(
        &mut self,
        new_type: &str,
        _host: &impl ConnectionFormHost,
    ) {
        if new_type == self.database_type {
            return;
        }
        let Some(new_meta) = db_meta(new_type) else {
            return;
        };
        if self.edit_id.is_none() {
            self.type_snapshots
                .insert(self.database_type.clone(), capture(self));
        }
        self.database_type = new_type.to_owned();
        self.driver_read_only = new_meta.read_only;
        if self.edit_id.is_some() {
            self.host = if new_meta.default_host.is_empty() {
                "127.0.0.1".to_owned()
            } else {
                new_meta.default_host.to_owned()
            };
            self.port = if new_meta.default_port == 0 {
                String::new()
            } else {
                new_meta.default_port.to_string()
            };
            self.username = new_meta.default_user.to_owned();
            self.ssl_mode = new_meta.default_ssl_mode.to_owned();
            if !new_meta.supports_ssh
                && self.tunnel_source == TunnelSource::Inline
                && self.tunnel_kind == Some(TunnelKind::Ssh)
            {
                self.hydrate_tunnel_named(None, None, None);
            }
            self.database = match new_meta.database_field_type {
                meta::DatabaseFieldType::Index => {
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
                self.schema = "default".to_owned();
            }
            if new_meta.read_only {
                self.read_only = true;
            }
            self.options = sanitize_options(
                &new_meta
                    .default_options
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect(),
            );
            return;
        }
        if let Some(saved) = self.type_snapshots.get(new_type).cloned() {
            restore(self, &saved);
            return;
        }
        apply_type_defaults(self, new_type);
    }

    fn load_existing(&mut self, existing: &ExistingConnection, host: &impl ConnectionFormHost) {
        self.database_type = existing.database_type.clone();
        self.name = existing.name.clone();
        self.host = existing.host.clone().unwrap_or_else(|| "127.0.0.1".to_owned());
        self.port = existing.port.map(|p| p.to_string()).unwrap_or_else(|| {
            db_meta(&existing.database_type)
                .map(|m| m.default_port.to_string())
                .unwrap_or_default()
        });
        self.database = existing.database.clone().unwrap_or_default();
        self.schema = existing.schema.clone().unwrap_or_else(|| "default".to_owned());
        self.username = existing.username.clone().unwrap_or_default();
        self.password = existing.password.clone().unwrap_or_default();
        self.ssl_mode = existing.ssl_mode.clone();
        self.group = coerce_group(match &existing.group {
            Some(value) => GroupInput::Text(value.clone()),
            None => GroupInput::Other,
        });
        if existing.group.is_none() {
            self.group.clear();
        }
        self.color_tag = existing.color_tag.clone().unwrap_or_else(|| "#3b82f6".to_owned());
        let existing_ro = db_meta(&existing.database_type).is_some_and(|m| m.read_only);
        self.read_only = existing_ro || existing.read_only;
        self.driver_read_only = existing_ro;
        self.options = sanitize_options(&existing.options);

        let saved_name = existing.tunnel_id.as_ref().and_then(|id| {
            host.saved_tunnels()
                .iter()
                .find(|tunnel| &tunnel.id == id)
                .map(|tunnel| tunnel.name.clone())
        });
        self.hydrate_tunnel_named(
            existing.tunnel_id.clone(),
            existing.tunnel_kind,
            saved_name,
        );

        if let Some(ssh) = &existing.ssh {
            if ssh.enabled {
                self.ssh_host = ssh.host.clone();
                self.ssh_port = ssh.port.to_string();
                self.ssh_username = ssh.username.clone();
                self.ssh_auth_method = SshAuthMethod::from_value(&ssh.auth_method);
                self.ssh_password = ssh.password.clone();
                self.ssh_key_path = ssh.private_key_path.clone();
                self.ssh_passphrase = ssh.passphrase.clone();
                if ssh.jump_enabled {
                    self.ssh_jump_enabled = true;
                    self.ssh_jump_host = ssh.jump_host.clone();
                    self.ssh_jump_port = ssh.jump_port.to_string();
                    self.ssh_jump_username = ssh.jump_username.clone();
                    self.ssh_jump_auth_method = SshAuthMethod::from_value(&ssh.jump_auth_method);
                    self.ssh_jump_password = ssh.jump_password.clone();
                    self.ssh_jump_key_path = ssh.jump_private_key_path.clone();
                    self.ssh_jump_passphrase = ssh.jump_passphrase.clone();
                }
            }
        }
        if let Some(proxy) = &existing.http_proxy {
            if proxy.enabled {
                self.http_proxy_host = proxy.host.clone();
                self.http_proxy_port = proxy.port.to_string();
                self.http_proxy_scheme = HttpProxyScheme::from_value(&proxy.scheme);
                self.http_proxy_username = proxy.username.clone();
                self.http_proxy_password = proxy.password.clone();
                self.http_proxy_timeout = proxy.connect_timeout_secs.to_string();
            }
        }
        if let Some(ws) = &existing.websocket {
            if ws.enabled {
                self.ws_url = ws.url.clone();
                self.ws_mode = WsMode::from_value(&ws.mode);
                self.ws_auth_token = ws.auth_token.clone();
                self.ws_timeout = ws.connect_timeout_secs.to_string();
            }
        }
        self.show_advanced = true;
        self.loaded = true;
    }
}

/// Every mutation the hook exposes, including the three commands.
#[allow(dead_code)] // host API; consumer: the connection dialog (not wired).
#[derive(Clone, Debug)]
pub enum FormAction {
    SetName(String),
    SetHost(String),
    SetPort(String),
    SetDatabase(String),
    SetSchema(String),
    SetUsername(String),
    SetPassword(String),
    SetSslMode(String),
    SetColorTag(String),
    SetGroup(GroupInput),
    SetReadOnly(bool),
    SetClearStoredCredentials(bool),
    SetShowAdvanced(bool),
    SetOptions(BTreeMap<String, String>),
    ChangeDatabaseType(String),
    SetTunnelKind(Option<TunnelKind>),
    SetTunnelId(Option<String>),
    SetTunnelSource(TunnelSource),
    SetSshEnabled(bool),
    SetSshHost(String),
    SetSshPort(String),
    SetSshUsername(String),
    SetSshAuthMethod(SshAuthMethod),
    SetSshPassword(String),
    SetSshKeyPath(String),
    SetSshPassphrase(String),
    SetSshJumpEnabled(bool),
    SetSshJumpHost(String),
    SetSshJumpPort(String),
    SetSshJumpUsername(String),
    SetSshJumpAuthMethod(SshAuthMethod),
    SetSshJumpPassword(String),
    SetSshJumpKeyPath(String),
    SetSshJumpPassphrase(String),
    SetHttpProxyHost(String),
    SetHttpProxyPort(String),
    SetHttpProxyScheme(HttpProxyScheme),
    SetHttpProxyUsername(String),
    SetHttpProxyPassword(String),
    SetHttpProxyTimeout(String),
    SetWsUrl(String),
    SetWsMode(WsMode),
    SetWsAuthToken(String),
    SetWsTimeout(String),
    Test,
    Save,
}

/// Apply one action to the host-owned state.
#[allow(dead_code)] // host API; consumer: the connection dialog (not wired).
pub fn step<H: ConnectionFormHost>(state: &mut ConnectionFormState, action: FormAction, host: &mut H) {
    match action {
        FormAction::SetName(v) => state.name = v,
        FormAction::SetHost(v) => state.host = v,
        FormAction::SetPort(v) => state.port = v,
        FormAction::SetDatabase(v) => state.database = v,
        FormAction::SetSchema(v) => state.schema = v,
        FormAction::SetUsername(v) => state.username = v,
        FormAction::SetPassword(v) => state.password = v,
        FormAction::SetSslMode(v) => state.ssl_mode = v,
        FormAction::SetColorTag(v) => state.color_tag = v,
        FormAction::SetGroup(v) => state.group = coerce_group(v),
        FormAction::SetReadOnly(v) => state.read_only = if state.driver_read_only { true } else { v },
        FormAction::SetClearStoredCredentials(v) => state.clear_stored_credentials = v,
        FormAction::SetShowAdvanced(v) => state.show_advanced = v,
        FormAction::SetOptions(v) => state.options = sanitize_options(&v),
        FormAction::ChangeDatabaseType(v) => state.handle_database_type_change(&v, host),
        FormAction::SetTunnelKind(kind) => set_tunnel_kind(state, kind),
        FormAction::SetTunnelId(id) => set_tunnel_id(state, id, host),
        FormAction::SetTunnelSource(source) => set_tunnel_source(state, source, host),
        FormAction::SetSshEnabled(v) => {
            state.ssh_enabled = v;
            if v {
                state.last_inline_kind = Some(TunnelKind::Ssh);
                state.tunnel_kind = Some(TunnelKind::Ssh);
                if state.tunnel_source != TunnelSource::Saved {
                    state.tunnel_source = TunnelSource::Inline;
                }
            }
        }
        FormAction::SetSshHost(v) => state.ssh_host = v,
        FormAction::SetSshPort(v) => state.ssh_port = v,
        FormAction::SetSshUsername(v) => state.ssh_username = v,
        FormAction::SetSshAuthMethod(v) => state.ssh_auth_method = v,
        FormAction::SetSshPassword(v) => state.ssh_password = v,
        FormAction::SetSshKeyPath(v) => state.ssh_key_path = v,
        FormAction::SetSshPassphrase(v) => state.ssh_passphrase = v,
        FormAction::SetSshJumpEnabled(v) => state.ssh_jump_enabled = v,
        FormAction::SetSshJumpHost(v) => state.ssh_jump_host = v,
        FormAction::SetSshJumpPort(v) => state.ssh_jump_port = v,
        FormAction::SetSshJumpUsername(v) => state.ssh_jump_username = v,
        FormAction::SetSshJumpAuthMethod(v) => state.ssh_jump_auth_method = v,
        FormAction::SetSshJumpPassword(v) => state.ssh_jump_password = v,
        FormAction::SetSshJumpKeyPath(v) => state.ssh_jump_key_path = v,
        FormAction::SetSshJumpPassphrase(v) => state.ssh_jump_passphrase = v,
        FormAction::SetHttpProxyHost(v) => state.http_proxy_host = v,
        FormAction::SetHttpProxyPort(v) => state.http_proxy_port = v,
        FormAction::SetHttpProxyScheme(v) => state.http_proxy_scheme = v,
        FormAction::SetHttpProxyUsername(v) => state.http_proxy_username = v,
        FormAction::SetHttpProxyPassword(v) => state.http_proxy_password = v,
        FormAction::SetHttpProxyTimeout(v) => state.http_proxy_timeout = v,
        FormAction::SetWsUrl(v) => state.ws_url = v,
        FormAction::SetWsMode(v) => state.ws_mode = v,
        FormAction::SetWsAuthToken(v) => state.ws_auth_token = v,
        FormAction::SetWsTimeout(v) => state.ws_timeout = v,
        FormAction::Test => state.on_test(host),
        FormAction::Save => state.on_save(host),
    }
}

fn set_tunnel_kind(state: &mut ConnectionFormState, kind: Option<TunnelKind>) {
    match kind {
        None => state.hydrate_tunnel_named(None, None, None),
        Some(kind) => {
            // A saved reference survives an inline sub-kind change (the React
            // fake host cannot refill it, so the reference must be kept).
            state.last_inline_kind = Some(kind);
            state.tunnel_kind = Some(kind);
            state.ssh_enabled = kind == TunnelKind::Ssh;
            if state.tunnel_source != TunnelSource::Saved {
                state.tunnel_source = TunnelSource::Inline;
            }
        }
    }
}

fn set_tunnel_id<H: ConnectionFormHost>(
    state: &mut ConnectionFormState,
    id: Option<String>,
    host: &H,
) {
    match id {
        Some(id) => {
            let tunnel = host.saved_tunnels().iter().find(|t| t.id == id).cloned();
            let kind = tunnel.as_ref().and_then(|t| TunnelKind::from_value(&t.kind));
            state.hydrate_tunnel_named(
                Some(id),
                kind,
                tunnel.as_ref().map(|t| t.name.clone()),
            );
        }
        None => {
            if state.tunnel_id.is_none() {
                state.tunnel_id = None;
            }
        }
    }
}

fn set_tunnel_source<H: ConnectionFormHost>(
    state: &mut ConnectionFormState,
    source: TunnelSource,
    host: &H,
) {
    match source {
        TunnelSource::None => state.hydrate_tunnel_named(None, None, None),
        TunnelSource::Saved => {
            if let Some(tunnel) = host.saved_tunnels().first().cloned() {
                let kind = TunnelKind::from_value(&tunnel.kind);
                state.hydrate_tunnel_named(Some(tunnel.id), kind, Some(tunnel.name));
            }
        }
        TunnelSource::Inline => {
            if state.tunnel_source == TunnelSource::Saved && state.tunnel_id.is_some() {
                return; // refill first; the reference is not dropped here
            }
            let kind = state.last_inline_kind;
            state.hydrate_tunnel_named(None, kind, None);
        }
    }
}

// Leftover L1 (removal stage): this slice exists until `views/connections.rs`
// owns the shared connection-form state; then bind actions directly to that
// slice, replace `BuiltConfig` with the driver-sdk DTO, drop the `DB_REGISTRY`
// stand-in and remove the per-item allows.
