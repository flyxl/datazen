use crate::{
    HttpProxyScheme, SavedTunnel, SavedTunnelKind, SshAuthMethod, SshTunnelConfig, WebSocketMode,
    WebSocketTunnelConfig,
};

use super::*;

fn ssh_config(port: u16, auth: SshAuthMethod) -> SshTunnelConfig {
    SshTunnelConfig {
        enabled: true,
        host: "bastion".to_owned(),
        port,
        username: "jumpuser".to_owned(),
        auth_method: auth,
        password: Some("pw".to_owned()),
        private_key_path: Some("/keys/id".to_owned()),
        passphrase: Some("phrase".to_owned()),
        jump: None,
    }
}

fn saved_ssh() -> SavedTunnel {
    let mut ssh = ssh_config(22, SshAuthMethod::Password);
    ssh.jump = Some(Box::new(ssh_config(2222, SshAuthMethod::PrivateKey)));
    SavedTunnel {
        id: "t1".to_owned(),
        name: "Bastion".to_owned(),
        kind: SavedTunnelKind::Ssh,
        ssh: Some(ssh),
        http_proxy: None,
        websocket: None,
    }
}

fn valid_ssh_draft() -> TunnelDraft {
    let mut draft = empty_tunnel_draft();
    draft.name = "Bastion".to_owned();
    draft.ssh.host = " bastion ".to_owned();
    draft.ssh.port = "22".to_owned();
    draft.ssh.username = " jumpuser ".to_owned();
    draft
}

#[test]
fn constants_and_empty_draft_are_pinned() {
    assert_eq!(SAVED_TUNNEL_KINDS, ["ssh", "httpProxy", "websocket"]);
    assert_eq!(DEFAULT_SSH_PORT, "22");
    assert_eq!(DEFAULT_HTTP_PROXY_PORT, "8080");
    assert_eq!(DEFAULT_TUNNEL_TIMEOUT, "30");
    let draft = empty_tunnel_draft();
    assert_eq!(draft.id, None);
    assert_eq!(draft.kind, SavedTunnelKind::Ssh);
    assert_eq!(draft.ssh.port, "22");
    assert_eq!(draft.ssh.jump.port, "22");
    assert_eq!(draft.http_proxy.port, "8080");
    assert_eq!(draft.http_proxy.timeout, "30");
    assert_eq!(draft.websocket.timeout, "30");
}

#[test]
fn saved_tunnel_projects_ssh_credentials_and_jump() {
    let draft = saved_tunnel_to_draft(&saved_ssh());
    assert_eq!(draft.id.as_deref(), Some("t1"));
    assert_eq!(draft.name, "Bastion");
    assert_eq!(draft.ssh.host, "bastion");
    assert_eq!(draft.ssh.port, "22");
    assert_eq!(draft.ssh.password, "pw");
    assert_eq!(draft.ssh.auth_method, SshAuthMethod::Password);
    assert!(draft.ssh.jump.enabled);
    assert_eq!(draft.ssh.jump.port, "2222");
    assert_eq!(draft.ssh.jump.key_path, "/keys/id");
    assert_eq!(draft.ssh.jump.passphrase, "phrase");
}

#[test]
fn saved_tunnel_projects_http_and_websocket_with_fallbacks() {
    let tunnel = SavedTunnel {
        id: "t2".to_owned(),
        name: "Proxy".to_owned(),
        kind: SavedTunnelKind::HttpProxy,
        ssh: None,
        http_proxy: Some(crate::HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy".to_owned(),
            port: 8080,
            scheme: HttpProxyScheme::Https,
            username: Some("user".to_owned()),
            password: None,
            headers: None,
            connect_timeout_secs: Some(45),
        }),
        websocket: Some(WebSocketTunnelConfig {
            enabled: true,
            url: "wss://relay".to_owned(),
            auth_token: None,
            headers: None,
            connect_timeout_secs: None,
            ping_interval_secs: None,
            mode: Some(WebSocketMode::RawBinary),
        }),
    };
    let draft = saved_tunnel_to_draft(&tunnel);
    assert_eq!(draft.http_proxy.scheme, HttpProxyScheme::Https);
    assert_eq!(draft.http_proxy.password, "");
    assert_eq!(draft.http_proxy.timeout, "45");
    assert_eq!(draft.websocket.mode, WebSocketMode::RawBinary);
    assert_eq!(draft.websocket.auth_token, "");
    assert_eq!(draft.websocket.timeout, "30");

    let empty = SavedTunnel {
        id: "t3".to_owned(),
        name: "Empty".to_owned(),
        kind: SavedTunnelKind::WebSocket,
        ssh: None,
        http_proxy: None,
        websocket: None,
    };
    let draft = saved_tunnel_to_draft(&empty);
    assert_eq!(draft.http_proxy, empty_tunnel_draft().http_proxy);
    assert_eq!(draft.websocket, empty_tunnel_draft().websocket);
}

#[test]
fn draft_to_saved_emits_only_the_matching_block() {
    let mut draft = valid_ssh_draft();
    draft.http_proxy.host = "ignored".to_owned();
    draft.websocket.url = "ignored".to_owned();
    let saved = draft_to_saved_tunnel(&draft, "new-id");
    assert_eq!(saved.id, "new-id");
    assert_eq!(saved.name, "Bastion");
    assert_eq!(saved.kind, SavedTunnelKind::Ssh);
    assert!(saved.ssh.is_some());
    assert!(saved.http_proxy.is_none());
    assert!(saved.websocket.is_none());
    let ssh = saved.ssh.expect("ssh");
    assert_eq!(ssh.host, "bastion");
    assert_eq!(ssh.username, "jumpuser");
    assert!(ssh.jump.is_none());
}

#[test]
fn ssh_credentials_are_gated_by_auth_method() {
    let mut draft = valid_ssh_draft();
    draft.ssh.auth_method = SshAuthMethod::PrivateKey;
    draft.ssh.password = "secret".to_owned();
    draft.ssh.key_path = " /keys/id ".to_owned();
    draft.ssh.passphrase = " phrase ".to_owned();
    draft.ssh.jump.enabled = true;
    draft.ssh.jump.host = "inner".to_owned();
    draft.ssh.jump.username = "u".to_owned();
    draft.ssh.jump.auth_method = SshAuthMethod::Agent;
    draft.ssh.jump.password = "ignored".to_owned();
    draft.ssh.jump.key_path = "ignored".to_owned();
    let ssh = draft_to_saved_tunnel(&draft, "id").ssh.expect("ssh");
    assert_eq!(ssh.password, None, "password is dropped for private_key auth");
    assert_eq!(ssh.private_key_path.as_deref(), Some(" /keys/id "), "keyPath is not trimmed");
    assert_eq!(ssh.passphrase.as_deref(), Some(" phrase "), "passphrase is not trimmed");
    let jump = ssh.jump.expect("jump");
    assert_eq!(jump.password, None);
    assert_eq!(jump.private_key_path, None);
}

#[test]
fn http_and_websocket_drafts_apply_defaults_and_trim() {
    let mut draft = empty_tunnel_draft();
    draft.kind = SavedTunnelKind::HttpProxy;
    draft.http_proxy.host = " proxy ".to_owned();
    draft.http_proxy.port = "bad".to_owned();
    draft.http_proxy.timeout = "0".to_owned();
    let saved = draft_to_saved_tunnel(&draft, "id");
    let http = saved.http_proxy.expect("http");
    assert_eq!(http.host, "proxy");
    assert_eq!(http.port, 8080);
    assert_eq!(http.connect_timeout_secs, Some(30));
    assert_eq!(http.username, None);

    let mut draft = empty_tunnel_draft();
    draft.kind = SavedTunnelKind::WebSocket;
    draft.websocket.url = " wss://relay ".to_owned();
    draft.websocket.auth_token = "token".to_owned();
    let saved = draft_to_saved_tunnel(&draft, "id");
    let ws = saved.websocket.expect("ws");
    assert_eq!(ws.url, "wss://relay");
    assert_eq!(ws.auth_token.as_deref(), Some("token"));
    assert_eq!(ws.connect_timeout_secs, Some(30));
}

#[test]
fn port_and_timeout_parsing_follows_the_rules() {
    assert!(is_positive_port("22"));
    assert!(is_positive_port("65535"));
    assert!(!is_positive_port(""));
    assert!(!is_positive_port("0"));
    assert!(!is_positive_port("65536"));
    assert!(!is_positive_port("1.5"));
    assert!(!is_positive_port("abc"));
    assert_eq!(positive_port("bad", 22), 22);
    assert_eq!(positive_port("1234", 22), 1234);
    assert_eq!(timeout_seconds("12"), 12);
    assert_eq!(timeout_seconds("-1"), 30);
    assert_eq!(timeout_seconds(""), 30);
}

#[test]
fn validation_reports_the_first_unmet_requirement() {
    assert_eq!(tunnel_draft_problem(&empty_tunnel_draft()), Some(TunnelDraftProblem::Name));
    let mut draft = valid_ssh_draft();
    assert_eq!(tunnel_draft_problem(&draft), None);
    assert!(is_tunnel_draft_valid(&draft));

    draft.ssh.host.clear();
    assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshHost));
    draft.ssh.host = "h".to_owned();
    draft.ssh.port = "bad".to_owned();
    assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshPort));
    draft.ssh.port = "22".to_owned();
    draft.ssh.username = " ".to_owned();
    assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshUsername));
    draft.ssh.username = "u".to_owned();
    draft.ssh.jump.enabled = true;
    assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshJumpHost));
    draft.ssh.jump.host = "inner".to_owned();
    draft.ssh.jump.port = "0".to_owned();
    assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshJumpPort));
    draft.ssh.jump.port = "22".to_owned();
    draft.ssh.jump.username = "".to_owned();
    assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::SshJumpUsername));
}

#[test]
fn http_and_websocket_validation_and_message_keys() {
    let mut draft = valid_ssh_draft();
    draft.kind = SavedTunnelKind::HttpProxy;
    assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::HttpProxyHost));
    draft.http_proxy.host = "proxy".to_owned();
    draft.http_proxy.port = "bad".to_owned();
    assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::HttpProxyPort));

    draft.kind = SavedTunnelKind::WebSocket;
    assert_eq!(tunnel_draft_problem(&draft), Some(TunnelDraftProblem::WsUrl));
    draft.websocket.url = "wss://relay".to_owned();
    assert_eq!(tunnel_draft_problem(&draft), None);
    assert_eq!(
        TunnelDraftProblem::WsUrl.message_key(),
        "settings.tunnels.validation.wsUrl"
    );
    assert_eq!(TunnelDraftProblem::SshJumpPort.as_str(), "sshJumpPort");
}

#[test]
fn validation_output_carries_no_credentials() {
    let mut draft = valid_ssh_draft();
    draft.ssh.password = "super-secret".to_owned();
    draft.ssh.jump.enabled = true;
    let problem = tunnel_draft_problem(&draft).expect("a problem");
    assert_eq!(problem, TunnelDraftProblem::SshJumpHost);
    assert!(!problem.message_key().contains("super-secret"));
}

#[test]
fn copy_name_concatenates_the_suffix() {
    assert_eq!(copy_tunnel_name("Bastion", " (copy)"), "Bastion (copy)");
    assert_eq!(copy_tunnel_name("", " (copy)"), " (copy)");
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::empty_tunnel_draft;
    let _ = crate::saved_tunnel_to_draft;
    let _ = crate::draft_to_saved_tunnel;
    let _ = crate::tunnel_draft_problem;
    let _ = crate::is_tunnel_draft_valid;
    let _ = crate::copy_tunnel_name;
    let _ = crate::is_positive_port;
    let _ = crate::positive_port;
    let _ = crate::timeout_seconds;
    assert_eq!(crate::DEFAULT_SSH_PORT, "22");
    assert_eq!(crate::SAVED_TUNNEL_KINDS.len(), 3);
    let _: Option<crate::TunnelDraft> = None;
    let _: Option<crate::SshDraft> = None;
    let _: Option<crate::SshJumpDraft> = None;
    let _: Option<crate::HttpProxyDraft> = None;
    let _: Option<crate::WebSocketDraft> = None;
    let _: Option<crate::TunnelDraftProblem> = None;
}
