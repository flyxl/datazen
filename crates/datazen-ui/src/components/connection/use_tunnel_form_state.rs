//! Port of `packages/frontend/src/components/connection/useTunnelFormState.ts`
//! as an immediate-mode slice + effect machine.
//!
//! One three-state machine (`none` / `saved` / `inline`) plus every inline field
//! it owns. React's async tunnel lookups become [`TunnelEffect`]s executed by the
//! host; answers feed back through [`TunnelFormState::step`]. No promise, timer,
//! thread or fake IPC exists here.
//!
//! Key invariants from the source, all preserved:
//!
//! * `tunnel_id` is cleared only by an explicit transition (`none`, unbind,
//!   save-as success) — never as a side effect of changing the kind.
//! * Leaving `saved` always *refills* the inline fields from the entity; if the
//!   entity is gone or unreadable the reference is kept and an error is shown.
//! * `effective_kind` derives from the entity in `saved`, is suppressed for an
//!   SSH toggle that is off in `inline`, and is `None` in `none`.
//!
//! # Reuse / stand-ins
//!
//! `TunnelSource` / `TunnelKind` come from `connection_advanced_settings`;
//! `SshAuthMethod`, `HttpProxyScheme`, `WsMode`, the three field-value VMs and
//! the validity helpers come from `tunnel_field_contracts` (T318). Saved-entity
//! payloads are local plain-data VMs because the backend-client DTOs are not in
//! the Rust port yet; removal stage: use those DTOs when they land.
//!
//! No view consumer yet: every item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the connection view wires this slice".

use super::connection_advanced_settings::{TunnelKind, TunnelSource};
use super::tunnel_field_contracts::{
    http_proxy_inline_valid, ssh_inline_valid, websocket_inline_valid, HttpProxyScheme,
    HttpProxyTunnelFieldsValue, SshAuthMethod, SshTunnelFieldsValue, WsMode,
    WebSocketTunnelFieldsValue,
};

/// React `SavedTunnelSummary` (kind widened to the Rune enum).
#[allow(dead_code)] // local VM; remove with the backend-client DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SavedSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: TunnelKind,
}

#[allow(dead_code)] // local VM; remove with the backend-client DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SshJumpConfig {
    pub(crate) enabled: bool,
    pub(crate) host: String,
    pub(crate) port: u32,
    pub(crate) username: String,
    pub(crate) auth_method: SshAuthMethod,
    pub(crate) password: Option<String>,
    pub(crate) private_key_path: Option<String>,
    pub(crate) passphrase: Option<String>,
}

#[allow(dead_code)] // local VM; remove with the backend-client DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SshConfig {
    pub(crate) enabled: bool,
    pub(crate) host: String,
    pub(crate) port: u32,
    pub(crate) username: String,
    pub(crate) auth_method: SshAuthMethod,
    pub(crate) password: Option<String>,
    pub(crate) private_key_path: Option<String>,
    pub(crate) passphrase: Option<String>,
    pub(crate) jump: Option<SshJumpConfig>,
}

#[allow(dead_code)] // local VM; remove with the backend-client DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HttpProxyConfig {
    pub(crate) enabled: bool,
    pub(crate) host: String,
    pub(crate) port: u32,
    pub(crate) scheme: HttpProxyScheme,
    pub(crate) username: Option<String>,
    pub(crate) password: Option<String>,
    pub(crate) connect_timeout_secs: u32,
}

#[allow(dead_code)] // local VM; remove with the backend-client DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WebSocketConfig {
    pub(crate) enabled: bool,
    pub(crate) url: String,
    pub(crate) mode: WsMode,
    pub(crate) auth_token: Option<String>,
    pub(crate) connect_timeout_secs: u32,
}

/// React `SavedTunnel`, flattened into optional per-kind configs.
#[allow(dead_code)] // local VM; remove with the backend-client DTO.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SavedTunnelConfig {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) kind: TunnelKind,
    pub(crate) ssh: Option<SshConfig>,
    pub(crate) http_proxy: Option<HttpProxyConfig>,
    pub(crate) websocket: Option<WebSocketConfig>,
}

/// What to do once an in-flight refill fetch answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    /// saved → inline (plain unbind / clearing the reference / entering inline).
    EnterInline,
    /// saved → inline and then switch the sub-kind to this value.
    EnterInlineWithKind(TunnelKind),
}

/// Errors the host may report for tunnel lookups/saves.
#[allow(dead_code)] // host result; remove with the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TunnelIoError {
    Message(String),
    Other,
}

impl TunnelIoError {
    fn message_or(self, fallback: &str) -> String {
        match self {
            Self::Message(message) if !message.is_empty() => message,
            _ => fallback.to_owned(),
        }
    }
}

/// The hook's whole state, owned by the host.
#[allow(dead_code)] // slice; remove with the connection view.
#[derive(Clone, Debug)]
pub(crate) struct TunnelFormState {
    supports_ssh: bool,
    source: TunnelSource,
    kind: Option<TunnelKind>,
    tunnel_id: Option<String>,
    last_inline_kind: TunnelKind,
    ssh: SshTunnelFieldsValue,
    http_proxy: HttpProxyTunnelFieldsValue,
    websocket: WebSocketTunnelFieldsValue,
    summaries: Vec<SavedSummary>,
    summaries_loaded: bool,
    summaries_error: Option<String>,
    busy: bool,
    error: Option<String>,
    pending: Option<Pending>,
}

#[allow(dead_code)] // slice; remove with the connection view.
impl TunnelFormState {
    pub(crate) fn new(supports_ssh: bool) -> Self {
        Self {
            supports_ssh,
            source: TunnelSource::None,
            kind: None,
            tunnel_id: None,
            last_inline_kind: if supports_ssh {
                TunnelKind::Ssh
            } else {
                TunnelKind::HttpProxy
            },
            ssh: SshTunnelFieldsValue::default(),
            http_proxy: HttpProxyTunnelFieldsValue::default(),
            websocket: WebSocketTunnelFieldsValue::default(),
            summaries: Vec::new(),
            summaries_loaded: false,
            summaries_error: None,
            busy: false,
            error: None,
            pending: None,
        }
    }

    // --- observable state ---------------------------------------------------

    pub(crate) fn source(&self) -> TunnelSource {
        self.source
    }

    pub(crate) fn kind(&self) -> Option<TunnelKind> {
        self.kind
    }

    pub(crate) fn tunnel_id(&self) -> Option<&str> {
        self.tunnel_id.as_deref()
    }

    pub(crate) fn summaries(&self) -> &[SavedSummary] {
        &self.summaries
    }

    pub(crate) fn saved_tunnel(&self) -> Option<&SavedSummary> {
        self.tunnel_id
            .as_deref()
            .and_then(|id| self.summaries.iter().find(|summary| summary.id == id))
    }

    pub(crate) fn tunnel_ref_missing(&self) -> bool {
        self.tunnel_id.is_some()
            && self.summaries_loaded
            && self.summaries_error.is_none()
            && self.saved_tunnel().is_none()
    }

    pub(crate) fn busy(&self) -> bool {
        self.busy
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn ssh(&self) -> &SshTunnelFieldsValue {
        &self.ssh
    }

    pub(crate) fn http_proxy(&self) -> &HttpProxyTunnelFieldsValue {
        &self.http_proxy
    }

    pub(crate) fn websocket(&self) -> &WebSocketTunnelFieldsValue {
        &self.websocket
    }

    /// React `effectiveTunnelKind`.
    pub(crate) fn effective_kind(&self) -> Option<TunnelKind> {
        if self.source == TunnelSource::Saved {
            if let Some(saved) = self.saved_tunnel() {
                return Some(saved.kind);
            }
        }
        if self.source == TunnelSource::Inline
            && self.kind == Some(TunnelKind::Ssh)
            && !self.ssh.enabled
        {
            return None;
        }
        self.kind
    }

    /// React `tunnelInlineValid`.
    pub(crate) fn inline_valid(&self) -> bool {
        if self.source != TunnelSource::Inline {
            return false;
        }
        match self.kind {
            Some(TunnelKind::Ssh) => ssh_inline_valid(
                self.ssh.enabled,
                &self.ssh.host.text,
                &self.ssh.username.text,
            ),
            Some(TunnelKind::HttpProxy) => http_proxy_inline_valid(
                &self.http_proxy.host.text,
                &self.http_proxy.port.text,
            ),
            Some(TunnelKind::WebSocket) => websocket_inline_valid(&self.websocket.url.text),
            None => false,
        }
    }

    // --- inline field setters (React's per-kind state setters) --------------

    pub(crate) fn set_ssh_host(&mut self, value: impl Into<String>) {
        self.ssh.host = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ssh_port(&mut self, value: impl Into<String>) {
        self.ssh.port = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ssh_username(&mut self, value: impl Into<String>) {
        self.ssh.username = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ssh_password(&mut self, value: impl Into<String>) {
        self.ssh.password = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ssh_key_path(&mut self, value: impl Into<String>) {
        self.ssh.key_path = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ssh_passphrase(&mut self, value: impl Into<String>) {
        self.ssh.passphrase = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ssh_auth_method(&mut self, value: SshAuthMethod) {
        self.ssh.auth_method = value;
    }
    pub(crate) fn set_ssh_jump_host(&mut self, value: impl Into<String>) {
        self.ssh.jump_host = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ssh_jump_port(&mut self, value: impl Into<String>) {
        self.ssh.jump_port = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ssh_jump_username(&mut self, value: impl Into<String>) {
        self.ssh.jump_username = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ssh_jump_auth_method(&mut self, value: SshAuthMethod) {
        self.ssh.jump_auth_method = value;
    }
    pub(crate) fn set_http_proxy_host(&mut self, value: impl Into<String>) {
        self.http_proxy.host = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_http_proxy_port(&mut self, value: impl Into<String>) {
        self.http_proxy.port = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_http_proxy_scheme(&mut self, value: HttpProxyScheme) {
        self.http_proxy.scheme = value;
    }
    pub(crate) fn set_http_proxy_username(&mut self, value: impl Into<String>) {
        self.http_proxy.username = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_http_proxy_password(&mut self, value: impl Into<String>) {
        self.http_proxy.password = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_http_proxy_timeout(&mut self, value: impl Into<String>) {
        self.http_proxy.timeout = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ws_url(&mut self, value: impl Into<String>) {
        self.websocket.url = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ws_mode(&mut self, value: WsMode) {
        self.websocket.mode = value;
    }
    pub(crate) fn set_ws_auth_token(&mut self, value: impl Into<String>) {
        self.websocket.auth_token = rune::TextEdit::new(value.into());
    }
    pub(crate) fn set_ws_timeout(&mut self, value: impl Into<String>) {
        self.websocket.timeout = rune::TextEdit::new(value.into());
    }

    // --- transitions --------------------------------------------------------

    pub(crate) fn step(
        &mut self,
        action: TunnelAction,
        strings: &TunnelStrings,
    ) -> TunnelStep {
        let mut effects = Vec::new();
        match action {
            TunnelAction::SourceChanged(source) => self.set_source(source, &mut effects),
            TunnelAction::KindChanged(kind) => self.set_kind(kind, &mut effects),
            TunnelAction::TunnelIdChanged(id) => self.set_tunnel_id(id, &mut effects),
            TunnelAction::HydrateRef { id, kind } => self.hydrate(id, kind),
            TunnelAction::SshEnabled(enabled) => self.set_ssh_enabled(enabled),
            TunnelAction::Unbind => self.unbind(&mut effects),
            TunnelAction::SaveAs { name } => self.save_as(&name, &mut effects, strings),
            TunnelAction::Reset => *self = Self::new(self.supports_ssh),
            TunnelAction::SummariesLoaded { items, error } => {
                self.summaries = items;
                self.summaries_loaded = true;
                self.summaries_error = error;
            }
            TunnelAction::TunnelFetched { result } => {
                self.busy = false;
                self.finish_refill(result, strings);
            }
            TunnelAction::TunnelSaved { result } => {
                self.busy = false;
                self.finish_save(result, strings);
            }
        }
        TunnelStep { effects }
    }

    fn set_source(&mut self, source: TunnelSource, effects: &mut Vec<TunnelEffect>) {
        match source {
            TunnelSource::None => {
                self.source = TunnelSource::None;
                self.tunnel_id = None;
                self.kind = None;
                self.ssh.enabled = false;
            }
            TunnelSource::Saved => {
                if self.summaries.is_empty() {
                    return;
                }
                let target = self
                    .tunnel_id
                    .as_deref()
                    .and_then(|id| self.summaries.iter().find(|s| s.id == id))
                    .cloned()
                    .unwrap_or_else(|| self.summaries[0].clone());
                self.tunnel_id = Some(target.id);
                self.kind = Some(target.kind);
                self.ssh.enabled = target.kind == TunnelKind::Ssh;
                self.source = TunnelSource::Saved;
            }
            TunnelSource::Inline => {
                if self.source == TunnelSource::Saved && self.tunnel_id.is_some() {
                    self.pending = Some(Pending::EnterInline);
                    self.error = None;
                    if let Some(id) = self.tunnel_id.clone() {
                        effects.push(TunnelEffect::FetchTunnel { id });
                    }
                    self.busy = true;
                    return;
                }
                self.kind = Some(self.last_inline_kind);
                self.ssh.enabled = self.last_inline_kind == TunnelKind::Ssh;
                self.source = TunnelSource::Inline;
            }
        }
    }

    fn set_kind(&mut self, kind: Option<TunnelKind>, effects: &mut Vec<TunnelEffect>) {
        let Some(kind) = kind else {
            self.set_source(TunnelSource::None, effects);
            return;
        };
        if self.source == TunnelSource::Saved && self.tunnel_id.is_some() {
            self.pending = Some(Pending::EnterInlineWithKind(kind));
            self.error = None;
            if let Some(id) = self.tunnel_id.clone() {
                effects.push(TunnelEffect::FetchTunnel { id });
            }
            self.busy = true;
            return;
        }
        self.last_inline_kind = kind;
        self.kind = Some(kind);
        self.ssh.enabled = kind == TunnelKind::Ssh;
        self.source = TunnelSource::Inline;
    }

    fn set_tunnel_id(&mut self, id: Option<String>, effects: &mut Vec<TunnelEffect>) {
        if let Some(id) = id {
            self.tunnel_id = Some(id.clone());
            if let Some(summary) = self.summaries.iter().find(|summary| summary.id == id) {
                self.kind = Some(summary.kind);
                self.ssh.enabled = summary.kind == TunnelKind::Ssh;
            }
            self.source = TunnelSource::Saved;
            return;
        }
        if self.tunnel_id.is_some() {
            self.pending = Some(Pending::EnterInline);
            self.error = None;
            if let Some(current) = self.tunnel_id.clone() {
                effects.push(TunnelEffect::FetchTunnel { id: current });
            }
            self.busy = true;
            return;
        }
        self.tunnel_id = None;
    }

    fn hydrate(&mut self, id: Option<String>, kind: Option<TunnelKind>) {
        if let Some(id) = id {
            self.tunnel_id = Some(id);
            self.source = TunnelSource::Saved;
            self.kind = kind;
            self.ssh.enabled = kind == Some(TunnelKind::Ssh);
            return;
        }
        self.tunnel_id = None;
        let Some(kind) = kind else {
            self.source = TunnelSource::None;
            self.kind = None;
            self.ssh.enabled = false;
            return;
        };
        self.last_inline_kind = kind;
        self.source = TunnelSource::Inline;
        self.kind = Some(kind);
        self.ssh.enabled = kind == TunnelKind::Ssh;
    }

    fn set_ssh_enabled(&mut self, enabled: bool) {
        self.ssh.enabled = enabled;
        if !enabled {
            return;
        }
        self.last_inline_kind = TunnelKind::Ssh;
        self.kind = Some(TunnelKind::Ssh);
        if self.source != TunnelSource::Saved {
            self.source = TunnelSource::Inline;
        }
    }

    fn unbind(&mut self, effects: &mut Vec<TunnelEffect>) {
        if self.tunnel_id.is_none() {
            // React's latent inconsistency: inline source, kind untouched.
            self.source = TunnelSource::Inline;
            return;
        }
        self.pending = Some(Pending::EnterInline);
        self.error = None;
        if let Some(id) = self.tunnel_id.clone() {
            effects.push(TunnelEffect::FetchTunnel { id });
        }
        self.busy = true;
    }

    fn finish_refill(&mut self, result: Result<Option<SavedTunnelConfig>, TunnelIoError>, strings: &TunnelStrings) {
        match result {
            Ok(Some(tunnel)) => {
                self.apply_tunnel(&tunnel);
                self.tunnel_id = None;
                self.source = TunnelSource::Inline;
            }
            Ok(None) => self.error = Some(strings.unbind_kept.clone()),
            Err(error) => self.error = Some(error.message_or(&strings.unbind_kept)),
        }
        if let Some(pending) = self.pending.take() {
            if self.error.is_none() {
                if let Pending::EnterInlineWithKind(kind) = pending {
                    self.last_inline_kind = kind;
                    self.kind = Some(kind);
                    self.ssh.enabled = kind == TunnelKind::Ssh;
                }
            }
        }
    }

    fn apply_tunnel(&mut self, tunnel: &SavedTunnelConfig) {
        self.last_inline_kind = tunnel.kind;
        self.kind = Some(tunnel.kind);
        self.ssh.enabled = tunnel.kind == TunnelKind::Ssh;
        if let Some(ssh) = &tunnel.ssh {
            self.ssh.host = rune::TextEdit::new(ssh.host.clone());
            self.ssh.port = rune::TextEdit::new(ssh.port.to_string());
            self.ssh.username = rune::TextEdit::new(ssh.username.clone());
            self.ssh.auth_method = ssh.auth_method;
            self.ssh.password = rune::TextEdit::new(ssh.password.clone().unwrap_or_default());
            self.ssh.key_path = rune::TextEdit::new(ssh.private_key_path.clone().unwrap_or_default());
            self.ssh.passphrase = rune::TextEdit::new(ssh.passphrase.clone().unwrap_or_default());
            if let Some(jump) = &ssh.jump {
                self.ssh.jump_enabled = jump.enabled;
                self.ssh.jump_host = rune::TextEdit::new(jump.host.clone());
                self.ssh.jump_port = rune::TextEdit::new(jump.port.to_string());
                self.ssh.jump_username = rune::TextEdit::new(jump.username.clone());
                self.ssh.jump_auth_method = jump.auth_method;
                self.ssh.jump_password = rune::TextEdit::new(jump.password.clone().unwrap_or_default());
                self.ssh.jump_key_path = rune::TextEdit::new(jump.private_key_path.clone().unwrap_or_default());
                self.ssh.jump_passphrase = rune::TextEdit::new(jump.passphrase.clone().unwrap_or_default());
            } else {
                self.ssh.jump_enabled = false;
            }
        }
        if let Some(proxy) = &tunnel.http_proxy {
            self.http_proxy.host = rune::TextEdit::new(proxy.host.clone());
            self.http_proxy.port = rune::TextEdit::new(proxy.port.to_string());
            self.http_proxy.scheme = proxy.scheme;
            self.http_proxy.username = rune::TextEdit::new(proxy.username.clone().unwrap_or_default());
            self.http_proxy.password = rune::TextEdit::new(proxy.password.clone().unwrap_or_default());
            self.http_proxy.timeout = rune::TextEdit::new(proxy.connect_timeout_secs.to_string());
        }
        if let Some(ws) = &tunnel.websocket {
            self.websocket.url = rune::TextEdit::new(ws.url.clone());
            self.websocket.mode = ws.mode;
            self.websocket.auth_token = rune::TextEdit::new(ws.auth_token.clone().unwrap_or_default());
            self.websocket.timeout = rune::TextEdit::new(ws.connect_timeout_secs.to_string());
        }
    }

    fn save_as(&mut self, name: &str, effects: &mut Vec<TunnelEffect>, strings: &TunnelStrings) {
        let Some(config) = self.build_inline(name) else {
            return;
        };
        if !self.inline_valid() {
            return;
        }
        self.busy = true;
        self.error = None;
        effects.push(TunnelEffect::SaveTunnel { config });
        let _ = &strings;
    }

    fn finish_save(&mut self, result: Result<SavedTunnelConfig, TunnelIoError>, strings: &TunnelStrings) {
        match result {
            Ok(tunnel) => {
                self.last_inline_kind = tunnel.kind;
                self.tunnel_id = Some(tunnel.id);
                self.kind = Some(tunnel.kind);
                self.ssh.enabled = tunnel.kind == TunnelKind::Ssh;
                self.source = TunnelSource::Saved;
            }
            Err(error) => self.error = Some(error.message_or(&strings.save_failed)),
        }
    }

    /// React `buildInlineTunnel` — the saved-entity payload for the inline form.
    pub(crate) fn build_inline(&self, name: &str) -> Option<SavedTunnelConfig> {
        let kind = self.kind?;
        let base = (name.trim().to_owned(), kind);
        match kind {
            TunnelKind::Ssh => Some(SavedTunnelConfig {
                id: String::new(),
                name: base.0,
                kind,
                ssh: Some(SshConfig {
                    enabled: true,
                    host: self.ssh.host.text.to_owned(),
                    port: parse_port(&self.ssh.port.text, 22),
                    username: self.ssh.username.text.to_owned(),
                    auth_method: self.ssh.auth_method,
                    password: optional(&self.ssh.password.text, self.ssh.auth_method == SshAuthMethod::Password),
                    private_key_path: optional(&self.ssh.key_path.text, self.ssh.auth_method == SshAuthMethod::PrivateKey),
                    passphrase: optional(&self.ssh.passphrase.text, self.ssh.auth_method == SshAuthMethod::PrivateKey),
                    jump: if self.ssh.jump_enabled {
                        Some(SshJumpConfig {
                            enabled: true,
                            host: self.ssh.jump_host.text.to_owned(),
                            port: parse_port(&self.ssh.jump_port.text, 22),
                            username: self.ssh.jump_username.text.to_owned(),
                            auth_method: self.ssh.jump_auth_method,
                            password: optional(&self.ssh.jump_password.text, self.ssh.jump_auth_method == SshAuthMethod::Password),
                            private_key_path: optional(&self.ssh.jump_key_path.text, self.ssh.jump_auth_method == SshAuthMethod::PrivateKey),
                            passphrase: optional(&self.ssh.jump_passphrase.text, self.ssh.jump_auth_method == SshAuthMethod::PrivateKey),
                        })
                    } else {
                        None
                    },
                }),
                http_proxy: None,
                websocket: None,
            }),
            TunnelKind::HttpProxy => Some(SavedTunnelConfig {
                id: String::new(),
                name: base.0,
                kind,
                ssh: None,
                http_proxy: Some(HttpProxyConfig {
                    enabled: true,
                    host: self.http_proxy.host.text.to_owned(),
                    port: parse_port(&self.http_proxy.port.text, 8080),
                    scheme: self.http_proxy.scheme,
                    username: optional(&self.http_proxy.username.text, true),
                    password: optional(&self.http_proxy.password.text, true),
                    connect_timeout_secs: parse_port(&self.http_proxy.timeout.text, 30),
                }),
                websocket: None,
            }),
            TunnelKind::WebSocket => Some(SavedTunnelConfig {
                id: String::new(),
                name: base.0,
                kind,
                ssh: None,
                http_proxy: None,
                websocket: Some(WebSocketConfig {
                    enabled: true,
                    url: self.websocket.url.text.to_owned(),
                    mode: self.websocket.mode,
                    auth_token: optional(&self.websocket.auth_token.text, true),
                    connect_timeout_secs: parse_port(&self.websocket.timeout.text, 30),
                }),
            }),
        }
    }
}

fn optional(value: &str, keep: bool) -> Option<String> {
    if keep && !value.is_empty() {
        Some(value.to_owned())
    } else {
        None
    }
}

fn parse_port(value: &str, fallback: u32) -> u32 {
    // React's `Number(x) || fallback` uses the fallback for 0 as well as for
    // non-numeric input; a negative or overflowing value also lands on it here.
    match value.trim().parse::<u32>() {
        Ok(0) | Err(_) => fallback,
        Ok(value) => value,
    }
}

/// Translated fallback strings (React calls `t()` directly).
#[allow(dead_code)] // stand-in; remove when the view resolves these with t().
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TunnelStrings {
    pub(crate) unbind_kept: String,
    pub(crate) save_failed: String,
}

/// Work the host performs for the injected tunnel commands.
#[allow(dead_code)] // effects; remove with the connection view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TunnelEffect {
    FetchTunnel { id: String },
    SaveTunnel { config: SavedTunnelConfig },
}

#[allow(dead_code)] // input; remove with the connection view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TunnelAction {
    SourceChanged(TunnelSource),
    KindChanged(Option<TunnelKind>),
    TunnelIdChanged(Option<String>),
    HydrateRef {
        id: Option<String>,
        kind: Option<TunnelKind>,
    },
    SshEnabled(bool),
    Unbind,
    SaveAs {
        name: String,
    },
    Reset,
    SummariesLoaded {
        items: Vec<SavedSummary>,
        error: Option<String>,
    },
    TunnelFetched {
        result: Result<Option<SavedTunnelConfig>, TunnelIoError>,
    },
    TunnelSaved {
        result: Result<SavedTunnelConfig, TunnelIoError>,
    },
}

#[allow(dead_code)] // step result; remove with the connection view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TunnelStep {
    pub(crate) effects: Vec<TunnelEffect>,
}
