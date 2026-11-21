use crate::{HttpProxyScheme, SshAuthMethod, WebSocketMode};

pub const DEFAULT_SSH_PORT: &str = "22";
pub const DEFAULT_HTTP_PROXY_PORT: &str = "8080";
pub const DEFAULT_TUNNEL_TIMEOUT: &str = "30";

pub const SAVED_TUNNEL_KINDS: [&str; 3] = ["ssh", "httpProxy", "websocket"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelDraftProblem {
    Name,
    SshHost,
    SshPort,
    SshUsername,
    SshJumpHost,
    SshJumpPort,
    SshJumpUsername,
    HttpProxyHost,
    HttpProxyPort,
    WsUrl,
}

impl TunnelDraftProblem {
    pub fn as_str(self) -> &'static str {
        match self {
            TunnelDraftProblem::Name => "name",
            TunnelDraftProblem::SshHost => "sshHost",
            TunnelDraftProblem::SshPort => "sshPort",
            TunnelDraftProblem::SshUsername => "sshUsername",
            TunnelDraftProblem::SshJumpHost => "sshJumpHost",
            TunnelDraftProblem::SshJumpPort => "sshJumpPort",
            TunnelDraftProblem::SshJumpUsername => "sshJumpUsername",
            TunnelDraftProblem::HttpProxyHost => "httpProxyHost",
            TunnelDraftProblem::HttpProxyPort => "httpProxyPort",
            TunnelDraftProblem::WsUrl => "wsUrl",
        }
    }

    pub fn message_key(self) -> &'static str {
        match self {
            TunnelDraftProblem::Name => "settings.tunnels.validation.name",
            TunnelDraftProblem::SshHost => "settings.tunnels.validation.sshHost",
            TunnelDraftProblem::SshPort => "settings.tunnels.validation.sshPort",
            TunnelDraftProblem::SshUsername => "settings.tunnels.validation.sshUsername",
            TunnelDraftProblem::SshJumpHost => "settings.tunnels.validation.sshJumpHost",
            TunnelDraftProblem::SshJumpPort => "settings.tunnels.validation.sshJumpPort",
            TunnelDraftProblem::SshJumpUsername => "settings.tunnels.validation.sshJumpUsername",
            TunnelDraftProblem::HttpProxyHost => "settings.tunnels.validation.httpProxyHost",
            TunnelDraftProblem::HttpProxyPort => "settings.tunnels.validation.httpProxyPort",
            TunnelDraftProblem::WsUrl => "settings.tunnels.validation.wsUrl",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshJumpDraft {
    pub enabled: bool,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_method: SshAuthMethod,
    pub password: String,
    pub key_path: String,
    pub passphrase: String,
}

impl Default for SshJumpDraft {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: DEFAULT_SSH_PORT.to_owned(),
            username: String::new(),
            auth_method: SshAuthMethod::Password,
            password: String::new(),
            key_path: String::new(),
            passphrase: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshDraft {
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_method: SshAuthMethod,
    pub password: String,
    pub key_path: String,
    pub passphrase: String,
    pub jump: SshJumpDraft,
}

impl Default for SshDraft {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: DEFAULT_SSH_PORT.to_owned(),
            username: String::new(),
            auth_method: SshAuthMethod::Password,
            password: String::new(),
            key_path: String::new(),
            passphrase: String::new(),
            jump: SshJumpDraft::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpProxyDraft {
    pub host: String,
    pub port: String,
    pub scheme: HttpProxyScheme,
    pub username: String,
    pub password: String,
    pub timeout: String,
}

impl Default for HttpProxyDraft {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: DEFAULT_HTTP_PROXY_PORT.to_owned(),
            scheme: HttpProxyScheme::Http,
            username: String::new(),
            password: String::new(),
            timeout: DEFAULT_TUNNEL_TIMEOUT.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketDraft {
    pub url: String,
    pub mode: WebSocketMode,
    pub auth_token: String,
    pub timeout: String,
}

impl Default for WebSocketDraft {
    fn default() -> Self {
        Self {
            url: String::new(),
            mode: WebSocketMode::DatazenV1,
            auth_token: String::new(),
            timeout: DEFAULT_TUNNEL_TIMEOUT.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunnelDraft {
    pub id: Option<String>,
    pub name: String,
    pub kind: crate::SavedTunnelKind,
    pub ssh: SshDraft,
    pub http_proxy: HttpProxyDraft,
    pub websocket: WebSocketDraft,
}

impl Default for TunnelDraft {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            kind: crate::SavedTunnelKind::Ssh,
            ssh: SshDraft::default(),
            http_proxy: HttpProxyDraft::default(),
            websocket: WebSocketDraft::default(),
        }
    }
}
