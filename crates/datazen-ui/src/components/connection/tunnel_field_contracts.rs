//! Port of `packages/frontend/src/components/connection/tunnelFieldContracts.ts`
//! plus the pure tunnel validation rules the field ports were mirroring.
//!
//! React's contracts are structural subsets of `ConnectionFormState` with plain
//! strings and setters. Rune's form is an immutable snapshot plus `TextEdit`
//! accessors owned by the host, so each value struct here mirrors the same
//! fields with `TextEdit`s; the setters live in the field ports' `on_vm`/
//! `on_*` callbacks. This module is now the single definition site: the field
//! ports re-export these types under their old local names, so no duplicate
//! enum or DTO remains.
//!
//! Defaults and validation come from React's `useTunnelFormState.ts` and
//! `useConnectionForm.ts` (ssh port `22`, http-proxy port `8080`, timeout `30`,
//! WebSocket mode `datazen_v1`, SSH auth `password`).

use std::rc::Rc;

use rune::TextEdit;

/// React `SshAuthMethod` (`password | private_key | agent`).
#[allow(dead_code)] // shared contract; consumed by the SSH field port and form host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SshAuthMethod {
    Password,
    PrivateKey,
    Agent,
}

#[allow(dead_code)] // shared contract; remove allows when all three ports are wired to the form host.
impl SshAuthMethod {
    pub(crate) fn from_value(value: &str) -> Self {
        match value {
            "private_key" => Self::PrivateKey,
            "agent" => Self::Agent,
            _ => Self::Password,
        }
    }

    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::PrivateKey => "private_key",
            Self::Agent => "agent",
        }
    }

    /// i18n key for the kit option label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Password => "newConn.authPassword",
            Self::PrivateKey => "newConn.authKey",
            Self::Agent => "newConn.authAgent",
        }
    }
}

impl Default for SshAuthMethod {
    fn default() -> Self {
        Self::Password
    }
}

/// React `HttpProxyTunnelFieldsValue['httpProxyScheme']`.
#[allow(dead_code)] // shared contract; consumed by the HTTP proxy field port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HttpProxyScheme {
    Http,
    Https,
}

#[allow(dead_code)] // shared contract; remove allows when the proxy port is wired to the form host.
impl HttpProxyScheme {
    pub(crate) fn from_value(value: &str) -> Self {
        if value == "https" {
            Self::Https
        } else {
            Self::Http
        }
    }

    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Http => "HTTP",
            Self::Https => "HTTPS",
        }
    }
}

impl Default for HttpProxyScheme {
    fn default() -> Self {
        Self::Http
    }
}

/// React `WebSocketTunnelFieldsValue['wsMode']`.
#[allow(dead_code)] // shared contract; consumed by the WebSocket field port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WsMode {
    DatazenV1,
    RawBinary,
}

#[allow(dead_code)] // shared contract; remove allows when the WS port is wired to the form host.
impl WsMode {
    /// The component's `onChange`: only the exact raw-binary value survives.
    pub(crate) fn from_value(value: &str) -> Self {
        if value == "raw_binary" {
            Self::RawBinary
        } else {
            Self::DatazenV1
        }
    }

    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::DatazenV1 => "datazen_v1",
            Self::RawBinary => "raw_binary",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DatazenV1 => "newConn.wsModeDatazen",
            Self::RawBinary => "newConn.wsModeRaw",
        }
    }
}

impl Default for WsMode {
    fn default() -> Self {
        Self::DatazenV1
    }
}

/// Mirror of React `SshTunnelFieldsValue`. The former local name
/// `SshFieldsState` is re-exported by `ssh_tunnel_fields` for existing callers.
#[allow(dead_code)] // shared contract; owned by the connection form host.
#[derive(Clone, Debug)]
pub(crate) struct SshTunnelFieldsValue {
    pub enabled: bool,
    pub host: TextEdit,
    pub port: TextEdit,
    pub username: TextEdit,
    pub auth_method: SshAuthMethod,
    pub password: TextEdit,
    pub key_path: TextEdit,
    pub passphrase: TextEdit,
    pub jump_enabled: bool,
    pub jump_host: TextEdit,
    pub jump_port: TextEdit,
    pub jump_username: TextEdit,
    pub jump_auth_method: SshAuthMethod,
    pub jump_password: TextEdit,
    pub jump_key_path: TextEdit,
    pub jump_passphrase: TextEdit,
}

/// React defaults: port `22` and jump port `22` (`useTunnelFormState.ts`).
#[allow(dead_code)] // shared contract; owned by the connection form host.
impl Default for SshTunnelFieldsValue {
    fn default() -> Self {
        Self {
            enabled: false,
            host: TextEdit::new(String::new()),
            port: TextEdit::new("22".to_owned()),
            username: TextEdit::new(String::new()),
            auth_method: SshAuthMethod::Password,
            password: TextEdit::new(String::new()),
            key_path: TextEdit::new(String::new()),
            passphrase: TextEdit::new(String::new()),
            jump_enabled: false,
            jump_host: TextEdit::new(String::new()),
            jump_port: TextEdit::new("22".to_owned()),
            jump_username: TextEdit::new(String::new()),
            jump_auth_method: SshAuthMethod::Password,
            jump_password: TextEdit::new(String::new()),
            jump_key_path: TextEdit::new(String::new()),
            jump_passphrase: TextEdit::new(String::new()),
        }
    }
}

/// Mirror of React `HttpProxyTunnelFieldsValue` (former local `HttpProxyValue`).
#[allow(dead_code)] // shared contract; owned by the connection form host.
#[derive(Clone, Debug)]
pub(crate) struct HttpProxyTunnelFieldsValue {
    pub host: TextEdit,
    pub port: TextEdit,
    pub scheme: HttpProxyScheme,
    /// Rune's Select needs its open state from the caller; React keeps it inside.
    pub scheme_open: bool,
    pub username: TextEdit,
    pub password: TextEdit,
    pub timeout: TextEdit,
}

/// React defaults: port `8080`, timeout `30`, scheme `http`.
#[allow(dead_code)] // shared contract; owned by the connection form host.
impl Default for HttpProxyTunnelFieldsValue {
    fn default() -> Self {
        Self {
            host: TextEdit::new(String::new()),
            port: TextEdit::new("8080".to_owned()),
            scheme: HttpProxyScheme::Http,
            scheme_open: false,
            username: TextEdit::new(String::new()),
            password: TextEdit::new(String::new()),
            timeout: TextEdit::new("30".to_owned()),
        }
    }
}

/// Mirror of React `WebSocketTunnelFieldsValue` (former local `WebSocketValue`).
#[allow(dead_code)] // shared contract; owned by the connection form host.
#[derive(Clone, Debug)]
pub(crate) struct WebSocketTunnelFieldsValue {
    pub url: TextEdit,
    pub mode: WsMode,
    /// Rune's Select needs its open state from the caller; React keeps it inside.
    pub mode_open: bool,
    pub timeout: TextEdit,
    pub auth_token: TextEdit,
}

/// React defaults: mode `datazen_v1`, timeout `30`.
#[allow(dead_code)] // shared contract; owned by the connection form host.
impl Default for WebSocketTunnelFieldsValue {
    fn default() -> Self {
        Self {
            url: TextEdit::new(String::new()),
            mode: WsMode::DatazenV1,
            mode_open: false,
            timeout: TextEdit::new("30".to_owned()),
            auth_token: TextEdit::new(String::new()),
        }
    }
}

/// React `noOpTabFill`: a factory whose returned key handler fills nothing.
/// Rune key events come from the runtime, so the inner handler takes no event.
#[allow(dead_code)] // contract port; consumers: tunnel field hosts without clipboard history.
pub(crate) type TabFill = Rc<dyn Fn() + 'static>;

#[allow(dead_code)] // contract port; consumers: tunnel field hosts without clipboard history.
pub(crate) fn noop_tab_fill() -> TabFill {
    Rc::new(|| {})
}

/// The form-track fields a settings-editor adapter must also drive (the React
/// tester's `FORM_TRACK_FIELDS`, in declaration order).
#[allow(dead_code)] // parity pin; consumed by the contract test.
pub(crate) const FORM_TRACK_FIELDS: [&str; 12] = [
    "supports_ssh",
    "enabled",
    "set_enabled",
    "jump_enabled",
    "set_jump_enabled",
    "jump_host",
    "jump_port",
    "jump_username",
    "jump_auth_method",
    "jump_password",
    "jump_key_path",
    "jump_passphrase",
];

// ---- Pure validation from React's form hooks --------------------------------

/// `isPositivePort` from `useTunnelFormState.ts`: non-empty, finite and > 0.
#[allow(dead_code)] // form rule; consumed by the connection form host.
pub(crate) fn is_positive_port(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    match trimmed.parse::<f64>() {
        Ok(port) => port.is_finite() && port > 0.0,
        Err(_) => false,
    }
}

/// `validationErrors.wsUrl = t('newConn.required')` when the URL is blank.
#[allow(dead_code)] // form rule; consumed by the WebSocket field host.
pub(crate) fn is_ws_url_valid(url: &str) -> bool {
    !url.trim().is_empty()
}

/// The three `tunnelInlineValid` branches from `useTunnelFormState.ts`.
#[allow(dead_code)] // form rule; consumed by the tunnel settings editor.
pub(crate) fn ssh_inline_valid(enabled: bool, host: &str, username: &str) -> bool {
    enabled && !host.trim().is_empty() && !username.trim().is_empty()
}

#[allow(dead_code)] // form rule; consumed by the tunnel settings editor.
pub(crate) fn http_proxy_inline_valid(host: &str, port: &str) -> bool {
    !host.trim().is_empty() && is_positive_port(port)
}

#[allow(dead_code)] // form rule; consumed by the WebSocket field host.
pub(crate) fn websocket_inline_valid(url: &str) -> bool {
    is_ws_url_valid(url)
}

// Leftover L1 (removal stage): the three value structs exist only until the
// connection form owns the shared `ConnectionFormState` slice; then read the
// fields directly from that slice and drop these mirrors and their allows.
