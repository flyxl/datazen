//! The carried half of `windows/settings/TunnelEditDialog.tsx` (T447,
//! split): `lib/tunnelDraft.ts` — **T637, segment 2**, not landed —
//! carried here verbatim until T637 lands and absorbs it (the
//! convention `migration_job_hydration_helpers.rs` records).
//!
//! String-field draft model for the settings tunnel editor.
//!
//! Ports and timeouts stay strings while editing so a controlled
//! input can be cleared and retyped (converting to a number on
//! every keystroke makes the field impossible to empty); they are
//! parsed once, on save. Everything here is pure so the settings
//! dialogs stay presentational and the conversions are
//! unit-testable.
//!
//! Known deltas: the `SavedTunnel`'s legs ride ui-api's mirrors
//! (`SshTunnelConfig` is the TS `SavedTunnelSshConfig` — the
//! mirror's own note); `Number(v)` parses the strings the same way
//! (`trim`, integer, positive, ≤ 65535).

// No production consumer yet (T447's dialog wires it; T453's
// section reuses the duplicate naming); this crate's tests are the
// callers meanwhile.
#![allow(dead_code)]

use datazen_ui_api::{
    HttpProxyScheme, SavedTunnel, SavedTunnelKind, SshAuthMethod, SshTunnelConfig, WebSocketMode,
};

/// `DEFAULT_SSH_PORT`.
pub const DEFAULT_SSH_PORT: &str = "22";
/// `DEFAULT_HTTP_PROXY_PORT`.
pub const DEFAULT_HTTP_PROXY_PORT: &str = "8080";
/// `DEFAULT_TUNNEL_TIMEOUT`.
pub const DEFAULT_TUNNEL_TIMEOUT: &str = "30";

/// `SAVED_TUNNEL_KINDS`, in declaration order.
pub const SAVED_TUNNEL_KINDS: [SavedTunnelKind; 3] =
    [SavedTunnelKind::Ssh, SavedTunnelKind::HttpProxy, SavedTunnelKind::WebSocket];

/// The kind label's i18n key.
pub fn tunnel_kind_label_key(kind: SavedTunnelKind) -> &'static str {
    match kind {
        SavedTunnelKind::Ssh => "settings.tunnels.kind.ssh",
        SavedTunnelKind::HttpProxy => "settings.tunnels.kind.httpProxy",
        SavedTunnelKind::WebSocket => "settings.tunnels.kind.websocket",
    }
}

/// `SshJumpDraft`.
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

/// `SshDraft`.
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

/// `HttpProxyDraft`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpProxyDraft {
    pub host: String,
    pub port: String,
    /// `'http' | 'https'`.
    pub scheme: String,
    pub username: String,
    pub password: String,
    pub timeout: String,
}

/// `WebSocketDraft`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebSocketDraft {
    pub url: String,
    /// `'datazen_v1' | 'raw_binary'`.
    pub mode: String,
    pub auth_token: String,
    pub timeout: String,
}

/// `TunnelDraft`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunnelDraft {
    /// Id of the entity being edited; `None` while creating a new
    /// one.
    pub id: Option<String>,
    pub name: String,
    pub kind: SavedTunnelKind,
    pub ssh: SshDraft,
    pub http_proxy: HttpProxyDraft,
    pub websocket: WebSocketDraft,
}

/// Why a draft cannot be saved, as a translation key. `None` means
/// valid. The keys live here (not in the dialog) so both the guard
/// and the message stay in sync with the validation rules below.
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
    /// `TUNNEL_DRAFT_PROBLEM_KEYS`.
    pub fn message_key(self) -> &'static str {
        match self {
            Self::Name => "settings.tunnels.validation.name",
            Self::SshHost => "settings.tunnels.validation.sshHost",
            Self::SshPort => "settings.tunnels.validation.sshPort",
            Self::SshUsername => "settings.tunnels.validation.sshUsername",
            Self::SshJumpHost => "settings.tunnels.validation.sshJumpHost",
            Self::SshJumpPort => "settings.tunnels.validation.sshJumpPort",
            Self::SshJumpUsername => "settings.tunnels.validation.sshJumpUsername",
            Self::HttpProxyHost => "settings.tunnels.validation.httpProxyHost",
            Self::HttpProxyPort => "settings.tunnels.validation.httpProxyPort",
            Self::WsUrl => "settings.tunnels.validation.wsUrl",
        }
    }
}

fn empty_ssh_jump_draft() -> SshJumpDraft {
    SshJumpDraft {
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

fn empty_ssh_draft() -> SshDraft {
    SshDraft {
        host: String::new(),
        port: DEFAULT_SSH_PORT.to_owned(),
        username: String::new(),
        auth_method: SshAuthMethod::Password,
        password: String::new(),
        key_path: String::new(),
        passphrase: String::new(),
        jump: empty_ssh_jump_draft(),
    }
}

/// `emptyTunnelDraft`.
pub fn empty_tunnel_draft() -> TunnelDraft {
    TunnelDraft {
        id: None,
        name: String::new(),
        kind: SavedTunnelKind::Ssh,
        ssh: empty_ssh_draft(),
        http_proxy: HttpProxyDraft {
            host: String::new(),
            port: DEFAULT_HTTP_PROXY_PORT.to_owned(),
            scheme: "http".to_owned(),
            username: String::new(),
            password: String::new(),
            timeout: DEFAULT_TUNNEL_TIMEOUT.to_owned(),
        },
        websocket: WebSocketDraft {
            url: String::new(),
            mode: "datazen_v1".to_owned(),
            auth_token: String::new(),
            timeout: DEFAULT_TUNNEL_TIMEOUT.to_owned(),
        },
    }
}

fn timeout_text(value: Option<u32>) -> String {
    match value {
        None => DEFAULT_TUNNEL_TIMEOUT.to_owned(),
        Some(value) => value.to_string(),
    }
}

fn ssh_config_field_or_empty(value: &Option<String>) -> String {
    value.clone().unwrap_or_default()
}

fn ssh_draft_from_config(config: Option<&SshTunnelConfig>) -> SshDraft {
    let Some(config) = config else {
        return empty_ssh_draft();
    };
    SshDraft {
        host: config.host.clone(),
        port: config.port.to_string(),
        username: config.username.clone(),
        auth_method: config.auth_method,
        password: ssh_config_field_or_empty(&config.password),
        key_path: ssh_config_field_or_empty(&config.private_key_path),
        passphrase: ssh_config_field_or_empty(&config.passphrase),
        jump: match &config.jump {
            Some(jump) => SshJumpDraft {
                enabled: jump.enabled,
                host: jump.host.clone(),
                port: jump.port.to_string(),
                username: jump.username.clone(),
                auth_method: jump.auth_method,
                password: ssh_config_field_or_empty(&jump.password),
                key_path: ssh_config_field_or_empty(&jump.private_key_path),
                passphrase: ssh_config_field_or_empty(&jump.passphrase),
            },
            None => empty_ssh_jump_draft(),
        },
    }
}

/// Project a stored entity (credentials included) onto the editable
/// draft.
pub fn saved_tunnel_to_draft(tunnel: &SavedTunnel) -> TunnelDraft {
    let draft = empty_tunnel_draft();
    TunnelDraft {
        id: Some(tunnel.id.clone()),
        name: tunnel.name.clone(),
        kind: tunnel.kind,
        ssh: ssh_draft_from_config(tunnel.ssh.as_ref()),
        http_proxy: match &tunnel.http_proxy {
            Some(http_proxy) => HttpProxyDraft {
                host: http_proxy.host.clone(),
                port: http_proxy.port.to_string(),
                scheme: http_proxy.scheme.as_str().to_owned(),
                username: http_proxy.username.clone().unwrap_or_default(),
                password: http_proxy.password.clone().unwrap_or_default(),
                timeout: timeout_text(http_proxy.connect_timeout_secs),
            },
            None => draft.http_proxy,
        },
        websocket: match &tunnel.websocket {
            Some(websocket) => WebSocketDraft {
                url: websocket.url.clone(),
                mode: websocket
                    .mode
                    .map(|mode| mode.as_str())
                    .unwrap_or("datazen_v1")
                    .to_owned(),
                auth_token: websocket.auth_token.clone().unwrap_or_default(),
                timeout: timeout_text(websocket.connect_timeout_secs),
            },
            None => draft.websocket,
        },
    }
}

fn is_positive_port(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    match trimmed.parse::<f64>() {
        Ok(port) => port.fract() == 0.0 && port > 0.0 && port <= 65535.0,
        Err(_) => false,
    }
}

fn positive_port(value: &str, fallback: u16) -> u16 {
    if is_positive_port(value) {
        value.trim().parse::<u16>().unwrap_or(fallback)
    } else {
        fallback
    }
}

fn timeout_seconds(value: &str) -> u32 {
    match value.trim().parse::<f64>() {
        Ok(seconds) if seconds.is_finite() && seconds > 0.0 => seconds as u32,
        _ => DEFAULT_TUNNEL_TIMEOUT.parse::<u32>().unwrap_or(30),
    }
}

fn ssh_auth_secret(auth_method: SshAuthMethod, secret: &str) -> Option<String> {
    match auth_method {
        SshAuthMethod::Password if !secret.is_empty() => Some(secret.to_owned()),
        SshAuthMethod::PrivateKey if !secret.is_empty() => Some(secret.to_owned()),
        _ => None,
    }
}

fn ssh_config_from_draft(draft: &SshDraft) -> SshTunnelConfig {
    SshTunnelConfig {
        enabled: true,
        host: draft.host.trim().to_owned(),
        port: positive_port(&draft.port, DEFAULT_SSH_PORT.parse().unwrap_or(22)),
        username: draft.username.trim().to_owned(),
        auth_method: draft.auth_method,
        password: match draft.auth_method {
            SshAuthMethod::Password => ssh_auth_secret(draft.auth_method, &draft.password),
            _ => None,
        },
        private_key_path: match draft.auth_method {
            SshAuthMethod::PrivateKey => ssh_auth_secret(draft.auth_method, &draft.key_path),
            _ => None,
        },
        passphrase: match draft.auth_method {
            SshAuthMethod::PrivateKey => ssh_auth_secret(draft.auth_method, &draft.passphrase),
            _ => None,
        },
        jump: if draft.jump.enabled {
            Some(Box::new(SshTunnelConfig {
                enabled: true,
                host: draft.jump.host.trim().to_owned(),
                port: positive_port(&draft.jump.port, DEFAULT_SSH_PORT.parse().unwrap_or(22)),
                username: draft.jump.username.trim().to_owned(),
                auth_method: draft.jump.auth_method,
                password: match draft.jump.auth_method {
                    SshAuthMethod::Password => {
                        ssh_auth_secret(draft.jump.auth_method, &draft.jump.password)
                    }
                    _ => None,
                },
                private_key_path: match draft.jump.auth_method {
                    SshAuthMethod::PrivateKey => {
                        ssh_auth_secret(draft.jump.auth_method, &draft.jump.key_path)
                    }
                    _ => None,
                },
                passphrase: match draft.jump.auth_method {
                    SshAuthMethod::PrivateKey => {
                        ssh_auth_secret(draft.jump.auth_method, &draft.jump.passphrase)
                    }
                    _ => None,
                },
                jump: None,
            }))
        } else {
            None
        },
    }
}

/// Build the entity to persist. Only the block matching `kind` is
/// emitted, so a tunnel never carries a stale config from a kind the
/// user switched away from.
pub fn draft_to_saved_tunnel(draft: &TunnelDraft, id: &str) -> SavedTunnel {
    let mut tunnel = SavedTunnel {
        id: id.to_owned(),
        name: draft.name.trim().to_owned(),
        kind: draft.kind,
        ssh: None,
        http_proxy: None,
        websocket: None,
    };
    match draft.kind {
        SavedTunnelKind::Ssh => tunnel.ssh = Some(ssh_config_from_draft(&draft.ssh)),
        SavedTunnelKind::HttpProxy => {
            tunnel.http_proxy = Some(datazen_ui_api::HttpProxyTunnelConfig {
                enabled: true,
                host: draft.http_proxy.host.trim().to_owned(),
                port: positive_port(
                    &draft.http_proxy.port,
                    DEFAULT_HTTP_PROXY_PORT.parse().unwrap_or(8080),
                ),
                scheme: if draft.http_proxy.scheme == "https" {
                    HttpProxyScheme::Https
                } else {
                    HttpProxyScheme::Http
                },
                username: (!draft.http_proxy.username.is_empty())
                    .then(|| draft.http_proxy.username.clone()),
                password: (!draft.http_proxy.password.is_empty())
                    .then(|| draft.http_proxy.password.clone()),
                headers: None,
                connect_timeout_secs: Some(timeout_seconds(&draft.http_proxy.timeout)),
            });
        }
        SavedTunnelKind::WebSocket => {
            tunnel.websocket = Some(datazen_ui_api::WebSocketTunnelConfig {
                enabled: true,
                url: draft.websocket.url.trim().to_owned(),
                mode: Some(if draft.websocket.mode == "raw_binary" {
                    WebSocketMode::RawBinary
                } else {
                    WebSocketMode::DatazenV1
                }),
                auth_token: (!draft.websocket.auth_token.is_empty())
                    .then(|| draft.websocket.auth_token.clone()),
                headers: None,
                connect_timeout_secs: Some(timeout_seconds(&draft.websocket.timeout)),
                ping_interval_secs: None,
            });
        }
    }
    tunnel
}

/// First unmet requirement of the draft, or `None` when it can be
/// saved.
pub fn tunnel_draft_problem(draft: &TunnelDraft) -> Option<TunnelDraftProblem> {
    if draft.name.trim().is_empty() {
        return Some(TunnelDraftProblem::Name);
    }
    match draft.kind {
        SavedTunnelKind::Ssh => {
            if draft.ssh.host.trim().is_empty() {
                return Some(TunnelDraftProblem::SshHost);
            }
            if !is_positive_port(&draft.ssh.port) {
                return Some(TunnelDraftProblem::SshPort);
            }
            if draft.ssh.username.trim().is_empty() {
                return Some(TunnelDraftProblem::SshUsername);
            }
            if draft.ssh.jump.enabled {
                if draft.ssh.jump.host.trim().is_empty() {
                    return Some(TunnelDraftProblem::SshJumpHost);
                }
                if !is_positive_port(&draft.ssh.jump.port) {
                    return Some(TunnelDraftProblem::SshJumpPort);
                }
                if draft.ssh.jump.username.trim().is_empty() {
                    return Some(TunnelDraftProblem::SshJumpUsername);
                }
            }
            None
        }
        SavedTunnelKind::HttpProxy => {
            if draft.http_proxy.host.trim().is_empty() {
                return Some(TunnelDraftProblem::HttpProxyHost);
            }
            if !is_positive_port(&draft.http_proxy.port) {
                return Some(TunnelDraftProblem::HttpProxyPort);
            }
            None
        }
        SavedTunnelKind::WebSocket => {
            if draft.websocket.url.trim().is_empty() {
                return Some(TunnelDraftProblem::WsUrl);
            }
            None
        }
    }
}

/// `isTunnelDraftValid`.
pub fn is_tunnel_draft_valid(draft: &TunnelDraft) -> bool {
    tunnel_draft_problem(draft).is_none()
}

/// `copyTunnelName` — `Bastion` + ` (copy)` → `Bastion (copy)`.
pub fn copy_tunnel_name(name: &str, suffix: &str) -> String {
    format!("{name}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_empty_draft_starts_from_ssh_with_the_default_ports() {
        let draft = empty_tunnel_draft();
        assert_eq!(draft.id, None);
        assert_eq!(draft.kind, SavedTunnelKind::Ssh);
        assert_eq!(draft.ssh.port, "22");
        assert_eq!(draft.http_proxy.port, "8080");
        assert_eq!(draft.http_proxy.scheme, "http");
        assert_eq!(draft.http_proxy.timeout, "30");
        assert_eq!(draft.websocket.mode, "datazen_v1");
        assert_eq!(draft.websocket.timeout, "30");
        assert!(!draft.ssh.jump.enabled);
        assert_eq!(draft.ssh.jump.port, "22", "the jump defaults too");
        assert_eq!(draft.ssh.auth_method, SshAuthMethod::Password);
        // The empty draft's first problem is the name.
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::Name));
    }

    #[test]
    fn the_port_guard_accepts_only_positive_integers() {
        for good in ["22", " 8080 ", "1", "65535"] {
            assert!(is_positive_port(good), "{good} is a port");
        }
        for bad in ["", "  ", "0", "-1", "65536", "abc", "22.5"] {
            assert!(!is_positive_port(bad), "{bad} is not a port");
        }
    }

    #[test]
    fn the_problem_order_follows_the_react_switch() {
        let mut draft = empty_tunnel_draft();
        draft.name = "Bastion".to_owned();
        // SSH: host → port → username → the jump triple.
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshHost));
        draft.ssh.host = "bastion.example.com".to_owned();
        draft.ssh.port = "nope".to_owned();
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshPort));
        draft.ssh.port = "22".to_owned();
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshUsername));
        draft.ssh.username = "ops".to_owned();
        assert_eq!(tunnel_draft_problem(&draft), None, "a valid ssh draft");
        // The jump requirements only while enabled.
        draft.ssh.jump.enabled = true;
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshJumpHost));
        draft.ssh.jump.host = "jump".to_owned();
        draft.ssh.jump.port = "bad".to_owned();
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshJumpPort));
        draft.ssh.jump.port = "2222".to_owned();
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshJumpUsername));
        draft.ssh.jump.username = "ops".to_owned();
        assert_eq!(tunnel_draft_problem(&draft), None);
        // httpProxy: host → port.
        draft.kind = SavedTunnelKind::HttpProxy;
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::HttpProxyHost));
        draft.http_proxy.host = "proxy".to_owned();
        draft.http_proxy.port = "0".to_owned();
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::HttpProxyPort));
        draft.http_proxy.port = "8080".to_owned();
        assert_eq!(tunnel_draft_problem(&draft), None);
        // websocket: the url only.
        draft.kind = SavedTunnelKind::WebSocket;
        assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::WsUrl));
        draft.websocket.url = "wss://relay".to_owned();
        assert_eq!(tunnel_draft_problem(&draft), None);
        assert!(is_tunnel_draft_valid(&draft));
    }

    #[test]
    fn the_message_keys_are_the_react_table() {
        assert_eq!(TunnelDraftProblem::Name.message_key(), "settings.tunnels.validation.name");
        assert_eq!(
            TunnelDraftProblem::SshJumpPort.message_key(),
            "settings.tunnels.validation.sshJumpPort"
        );
        assert_eq!(TunnelDraftProblem::WsUrl.message_key(), "settings.tunnels.validation.wsUrl");
    }

    fn valid_ssh_draft() -> TunnelDraft {
        let mut draft = empty_tunnel_draft();
        draft.name = "Bastion".to_owned();
        draft.ssh.host = "bastion.example.com".to_owned();
        draft.ssh.username = "ops".to_owned();
        draft.ssh.port = "2222".to_owned();
        draft.ssh.auth_method = SshAuthMethod::PrivateKey;
        draft.ssh.key_path = "/home/ops/id_ed25519".to_owned();
        draft.ssh.passphrase = "secret".to_owned();
        draft.ssh.password = "not-used".to_owned();
        draft
    }

    #[test]
    fn the_save_emits_only_the_matching_block() {
        let mut draft = valid_ssh_draft();
        let tunnel = draft_to_saved_tunnel(&draft, "t-9");
        assert_eq!(tunnel.id, "t-9");
        assert_eq!(tunnel.name, "Bastion");
        assert_eq!(tunnel.kind, SavedTunnelKind::Ssh);
        let ssh = tunnel.ssh.as_ref().unwrap();
        assert!(ssh.enabled);
        assert_eq!(ssh.host, "bastion.example.com");
        assert_eq!(ssh.port, 2222);
        assert_eq!(ssh.username, "ops");
        // The private-key auth keeps the key + passphrase and drops
        // the password.
        assert_eq!(ssh.password, None);
        assert_eq!(ssh.private_key_path.as_deref(), Some("/home/ops/id_ed25519"));
        assert_eq!(ssh.passphrase.as_deref(), Some("secret"));
        assert!(ssh.jump.is_none(), "the disabled jump emits nothing");
        assert!(tunnel.http_proxy.is_none());
        assert!(tunnel.websocket.is_none());

        // A password-auth draft keeps the password and drops the key.
        draft.ssh.auth_method = SshAuthMethod::Password;
        let tunnel = draft_to_saved_tunnel(&draft, "t-9");
        let ssh = tunnel.ssh.as_ref().unwrap();
        assert_eq!(ssh.password.as_deref(), Some("not-used"));
        assert_eq!(ssh.private_key_path, None);
        assert_eq!(ssh.passphrase, None);

        // The enabled jump rides with its own auth-scoped secrets.
        draft.ssh.jump.enabled = true;
        draft.ssh.jump.host = "jump".to_owned();
        draft.ssh.jump.username = "jops".to_owned();
        draft.ssh.jump.port = "bad".to_owned(); // falls back to 22
        draft.ssh.jump.auth_method = SshAuthMethod::Password;
        draft.ssh.jump.password = "jump-secret".to_owned();
        let tunnel = draft_to_saved_tunnel(&draft, "t-9");
        let jump = tunnel.ssh.as_ref().unwrap().jump.as_ref().unwrap();
        assert_eq!(jump.port, 22, "the fallback port");
        assert_eq!(jump.password.as_deref(), Some("jump-secret"));

        // Switching the kind leaves no stale block behind.
        draft.kind = SavedTunnelKind::HttpProxy;
        draft.http_proxy.host = "proxy".to_owned();
        let tunnel = draft_to_saved_tunnel(&draft, "t-9");
        assert!(tunnel.ssh.is_none(), "no stale ssh block");
        let proxy = tunnel.http_proxy.as_ref().unwrap();
        assert_eq!(proxy.scheme, HttpProxyScheme::Http);
        assert_eq!(proxy.connect_timeout_secs, Some(30), "the default timeout");
        assert_eq!(proxy.username, None, "empty secrets drop");

        draft.kind = SavedTunnelKind::WebSocket;
        draft.websocket.url = "wss://relay".to_owned();
        draft.websocket.auth_token = "tok".to_owned();
        let tunnel = draft_to_saved_tunnel(&draft, "t-9");
        let ws = tunnel.websocket.as_ref().unwrap();
        assert_eq!(ws.url, "wss://relay");
        assert_eq!(ws.mode, Some(WebSocketMode::DatazenV1));
        assert_eq!(ws.auth_token.as_deref(), Some("tok"));
    }

    #[test]
    fn the_saved_round_trips_onto_the_draft() {
        let mut draft = valid_ssh_draft();
        draft.ssh.jump.enabled = true;
        draft.ssh.jump.host = "jump".to_owned();
        draft.ssh.jump.username = "jops".to_owned();
        draft.ssh.jump.port = "2222".to_owned();
        let tunnel = draft_to_saved_tunnel(&draft, "t-9");
        let back = saved_tunnel_to_draft(&tunnel);
        assert_eq!(back.id, Some("t-9".to_owned()));
        assert_eq!(back.kind, SavedTunnelKind::Ssh);
        assert_eq!(back.ssh.host, "bastion.example.com");
        assert_eq!(back.ssh.port, "2222");
        assert_eq!(back.ssh.key_path, "/home/ops/id_ed25519");
        assert!(back.ssh.jump.enabled);
        assert_eq!(back.ssh.jump.host, "jump");
        // An absent secret projects as the empty string (the
        // controlled input's shape).
        assert_eq!(back.ssh.password, "");
        // The untouched legs keep their empty defaults.
        assert_eq!(back.http_proxy.port, "8080");
        assert_eq!(back.websocket.timeout, "30");

        // An httpProxy entity projects its own block.
        let mut draft = empty_tunnel_draft();
        draft.name = "Proxy".to_owned();
        draft.kind = SavedTunnelKind::HttpProxy;
        draft.http_proxy.host = "proxy".to_owned();
        draft.http_proxy.scheme = "https".to_owned();
        draft.http_proxy.timeout = "60".to_owned();
        let tunnel = draft_to_saved_tunnel(&draft, "t-8");
        let back = saved_tunnel_to_draft(&tunnel);
        assert_eq!(back.http_proxy.scheme, "https");
        assert_eq!(back.http_proxy.timeout, "60");
        assert_eq!(back.ssh.host, "", "the ssh block stays empty");

        // A websocket entity with an absent timeout projects the
        // default.
        let mut draft = empty_tunnel_draft();
        draft.name = "Relay".to_owned();
        draft.kind = SavedTunnelKind::WebSocket;
        draft.websocket.url = "wss://relay".to_owned();
        draft.websocket.mode = "raw_binary".to_owned();
        let tunnel = draft_to_saved_tunnel(&draft, "t-7");
        let back = saved_tunnel_to_draft(&tunnel);
        assert_eq!(back.websocket.mode, "raw_binary");
        assert_eq!(back.websocket.timeout, "30", "absent → the default");
    }

    #[test]
    fn the_copy_name_appends_the_suffix() {
        assert_eq!(copy_tunnel_name("Bastion", " (copy)"), "Bastion (copy)");
    }
}
