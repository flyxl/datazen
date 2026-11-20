//! T358: `useTunnelFormState` hook port — real branch tests.
//!
//! Pinned state mirrors `useTunnelFormState.test.ts`: the three-state machine,
//! refill-on-exit, dangling references, per-kind validity and failure paths.
//! Every answer is fed through `TunnelFormState::step`; no fake IPC or threads.

use crate::components::connection::connection_advanced_settings::{TunnelKind, TunnelSource};
use crate::components::connection::tunnel_field_contracts::{
    HttpProxyScheme, SshAuthMethod, WsMode,
};
use crate::components::connection::use_tunnel_form_state::{
    HttpProxyConfig, SavedSummary, SavedTunnelConfig, SshConfig, SshJumpConfig, TunnelAction,
    TunnelEffect, TunnelFormState, TunnelIoError, TunnelStrings, WebSocketConfig,
};

fn strings() -> TunnelStrings {
    TunnelStrings {
        unbind_kept: "newConn.tunnelUnbindKept".to_owned(),
        save_failed: "newConn.tunnelSaveFailed".to_owned(),
    }
}

fn state() -> TunnelFormState {
    TunnelFormState::new(true)
}

fn loaded(state: &mut TunnelFormState, items: Vec<SavedSummary>) {
    let _ = state.step(
        TunnelAction::SummariesLoaded {
            items,
            error: None,
        },
        &strings(),
    );
}

fn summary(id: &str, name: &str, kind: TunnelKind) -> SavedSummary {
    SavedSummary {
        id: id.to_owned(),
        name: name.to_owned(),
        kind,
    }
}

fn ssh_tunnel(id: &str) -> SavedTunnelConfig {
    SavedTunnelConfig {
        id: id.to_owned(),
        name: "Bastion".to_owned(),
        kind: TunnelKind::Ssh,
        ssh: Some(SshConfig {
            enabled: true,
            host: "bastion".to_owned(),
            port: 22,
            username: "ops".to_owned(),
            auth_method: SshAuthMethod::Agent,
            password: None,
            private_key_path: None,
            passphrase: None,
            jump: None,
        }),
        http_proxy: None,
        websocket: None,
    }
}

#[test]
fn starts_in_none_and_refuses_saved_with_no_candidates() {
    let mut slice = state();
    assert_eq!(slice.source(), TunnelSource::None);
    assert_eq!(slice.kind(), None);
    assert_eq!(slice.tunnel_id(), None);
    assert!(slice.summaries().is_empty());
    assert!(!slice.inline_valid());
    let step = slice.step(TunnelAction::SourceChanged(TunnelSource::Saved), &strings());
    assert!(step.effects.is_empty());
    assert_eq!(slice.source(), TunnelSource::None, "saved needs a candidate");
}

#[test]
fn none_to_inline_defaults_to_ssh_and_switches_sub_kind_without_touching_the_id() {
    let mut slice = state();
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    assert_eq!(slice.source(), TunnelSource::Inline);
    assert_eq!(slice.kind(), Some(TunnelKind::Ssh));
    assert!(slice.ssh().enabled);

    slice.set_ssh_host("bastion.example.com");
    assert!(!slice.inline_valid(), "a host alone is not enough");
    slice.set_ssh_username("ops");
    slice.set_ssh_auth_method(SshAuthMethod::PrivateKey);
    slice.set_ssh_key_path("/home/ops/.ssh/id_rsa");
    assert!(slice.inline_valid());

    let step = slice.step(TunnelAction::KindChanged(Some(TunnelKind::HttpProxy)), &strings());
    assert!(step.effects.is_empty());
    assert_eq!(slice.source(), TunnelSource::Inline);
    assert_eq!(slice.tunnel_id(), None, "an inline switch never touches the id");
    assert!(!slice.inline_valid());
    slice.set_http_proxy_host("proxy.corp.example");
    slice.set_http_proxy_port("3128");
    assert!(slice.inline_valid());
}

#[test]
fn save_as_then_unbind_refills_parameters_and_none_clears_the_slice() {
    let mut slice = state();
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    let _ = slice.step(TunnelAction::KindChanged(Some(TunnelKind::HttpProxy)), &strings());
    slice.set_http_proxy_host("proxy.corp.example");
    slice.set_http_proxy_port("3128");

    let step = slice.step(TunnelAction::SaveAs { name: "Corp proxy".to_owned() }, &strings());
    match step.effects.as_slice() {
        [TunnelEffect::SaveTunnel { config }] => {
            assert_eq!(config.name, "Corp proxy");
            assert_eq!(config.kind, TunnelKind::HttpProxy);
            assert_eq!(config.http_proxy.as_ref().map(|p| p.host.as_str()), Some("proxy.corp.example"));
        }
        other => panic!("expected SaveTunnel, got {other:?}"),
    }
    let saved = SavedTunnelConfig {
        id: "tun_1".to_owned(),
        name: "Corp proxy".to_owned(),
        kind: TunnelKind::HttpProxy,
        ssh: None,
        http_proxy: Some(HttpProxyConfig {
            enabled: true,
            host: "proxy.corp.example".to_owned(),
            port: 3128,
            scheme: HttpProxyScheme::Http,
            username: None,
            password: None,
            connect_timeout_secs: 30,
        }),
        websocket: None,
    };
    let _ = slice.step(TunnelAction::TunnelSaved { result: Ok(saved) }, &strings());
    assert_eq!(slice.source(), TunnelSource::Saved);
    assert_eq!(slice.tunnel_id(), Some("tun_1"));

    let refill = slice.step(TunnelAction::Unbind, &strings());
    assert_eq!(
        refill.effects,
        vec![TunnelEffect::FetchTunnel { id: "tun_1".to_owned() }]
    );
    let tunnel = SavedTunnelConfig { id: "tun_1".to_owned(), ..saved_config_with_proxy() };
    let _ = slice.step(TunnelAction::TunnelFetched { result: Ok(Some(tunnel)) }, &strings());
    assert_eq!(slice.source(), TunnelSource::Inline);
    assert_eq!(slice.tunnel_id(), None);
    assert_eq!(slice.kind(), Some(TunnelKind::HttpProxy));
    assert_eq!(slice.http_proxy().host.text, "proxy.corp.example");
    assert_eq!(slice.http_proxy().port.text, "3128");
    assert!(slice.inline_valid());

    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::None), &strings());
    assert_eq!(slice.source(), TunnelSource::None);
    assert_eq!(slice.kind(), None);
    assert!(!slice.ssh().enabled);
    assert_eq!(slice.effective_kind(), None);
}

fn saved_config_with_proxy() -> SavedTunnelConfig {
    SavedTunnelConfig {
        id: "tun_1".to_owned(),
        name: "Corp proxy".to_owned(),
        kind: TunnelKind::HttpProxy,
        ssh: None,
        http_proxy: Some(HttpProxyConfig {
            enabled: true,
            host: "proxy.corp.example".to_owned(),
            port: 3128,
            scheme: HttpProxyScheme::Http,
            username: None,
            password: None,
            connect_timeout_secs: 30,
        }),
        websocket: None,
    }
}

#[test]
fn re_entering_inline_restores_the_last_inline_kind() {
    let mut slice = state();
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    let _ = slice.step(TunnelAction::KindChanged(Some(TunnelKind::WebSocket)), &strings());
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::None), &strings());
    assert_eq!(slice.kind(), None);
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    assert_eq!(slice.kind(), Some(TunnelKind::WebSocket));
}

#[test]
fn changing_kind_while_saved_refills_then_switches() {
    let mut slice = state();
    loaded(&mut slice, vec![summary("tun_ssh", "Bastion", TunnelKind::Ssh)]);
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Saved), &strings());
    assert_eq!(slice.tunnel_id(), Some("tun_ssh"));
    assert_eq!(slice.kind(), Some(TunnelKind::Ssh));

    let step = slice.step(TunnelAction::KindChanged(Some(TunnelKind::WebSocket)), &strings());
    assert_eq!(
        step.effects,
        vec![TunnelEffect::FetchTunnel { id: "tun_ssh".to_owned() }]
    );
    let _ = slice.step(TunnelAction::TunnelFetched { result: Ok(Some(ssh_tunnel("tun_ssh"))) }, &strings());
    assert_eq!(slice.source(), TunnelSource::Inline);
    assert_eq!(slice.tunnel_id(), None);
    assert_eq!(slice.kind(), Some(TunnelKind::WebSocket));
    assert_eq!(slice.ssh().host.text, "bastion", "SSH params were refilled");
}

#[test]
fn dangling_reference_is_flagged_only_after_a_clean_load() {
    let mut slice = state();
    loaded(&mut slice, Vec::new());
    let _ = slice.step(
        TunnelAction::HydrateRef {
            id: Some("tun_gone".to_owned()),
            kind: Some(TunnelKind::Ssh),
        },
        &strings(),
    );
    assert_eq!(slice.source(), TunnelSource::Saved);
    assert_eq!(slice.tunnel_id(), Some("tun_gone"));
    assert!(slice.saved_tunnel().is_none());
    assert!(slice.tunnel_ref_missing());
    assert!(!slice.inline_valid());

    let mut failed = state();
    let _ = failed.step(
        TunnelAction::SummariesLoaded {
            items: Vec::new(),
            error: Some("ipc down".to_owned()),
        },
        &strings(),
    );
    let _ = failed.step(
        TunnelAction::HydrateRef {
            id: Some("tun_gone".to_owned()),
            kind: Some(TunnelKind::Ssh),
        },
        &strings(),
    );
    assert!(!failed.tunnel_ref_missing(), "a failed load proves nothing");
}

#[test]
fn unreadable_entity_keeps_the_reference_and_reports_the_error() {
    let mut slice = state();
    loaded(&mut slice, vec![summary("tun_ws", "Relay", TunnelKind::WebSocket)]);
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Saved), &strings());
    let refill = slice.step(TunnelAction::Unbind, &strings());
    assert_eq!(refill.effects.len(), 1);
    let _ = slice.step(TunnelAction::TunnelFetched { result: Ok(None) }, &strings());
    assert_eq!(slice.source(), TunnelSource::Saved, "reference kept");
    assert_eq!(slice.tunnel_id(), Some("tun_ws"));
    assert_eq!(slice.error(), Some("newConn.tunnelUnbindKept"));
}

#[test]
fn clearing_the_reference_unbinds_but_a_no_reference_clear_is_inert() {
    let mut slice = state();
    loaded(&mut slice, vec![summary("tun_ws", "Relay", TunnelKind::WebSocket)]);
    let _ = slice.step(
        TunnelAction::TunnelIdChanged(Some("tun_ws".to_owned())),
        &strings(),
    );
    assert_eq!(slice.source(), TunnelSource::Saved);
    let step = slice.step(TunnelAction::TunnelIdChanged(None), &strings());
    assert_eq!(step.effects.len(), 1);
    let tunnel = SavedTunnelConfig {
        id: "tun_ws".to_owned(),
        name: "Relay".to_owned(),
        kind: TunnelKind::WebSocket,
        ssh: None,
        http_proxy: None,
        websocket: Some(WebSocketConfig {
            enabled: true,
            url: "wss://relay/v1".to_owned(),
            mode: WsMode::DatazenV1,
            auth_token: None,
            connect_timeout_secs: 30,
        }),
    };
    let _ = slice.step(TunnelAction::TunnelFetched { result: Ok(Some(tunnel)) }, &strings());
    assert_eq!(slice.tunnel_id(), None);
    assert_eq!(slice.websocket().url.text, "wss://relay/v1");

    let mut empty = state();
    let none = empty.step(TunnelAction::TunnelIdChanged(None), &strings());
    assert!(none.effects.is_empty(), "no reference means no fetch");
    assert_eq!(empty.source(), TunnelSource::None);
}

#[test]
fn ssh_toggle_off_is_effective_none_but_keeps_the_kind() {
    let mut slice = state();
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    assert_eq!(slice.effective_kind(), Some(TunnelKind::Ssh));
    let _ = slice.step(TunnelAction::SshEnabled(false), &strings());
    assert_eq!(slice.kind(), Some(TunnelKind::Ssh));
    assert_eq!(slice.effective_kind(), None);
    assert!(!slice.inline_valid());
    let _ = slice.step(TunnelAction::SshEnabled(true), &strings());
    assert_eq!(slice.effective_kind(), Some(TunnelKind::Ssh));
}

#[test]
fn hydrate_uses_the_entity_kind_not_a_stale_hint() {
    let mut slice = state();
    let tunnel = ssh_tunnel("tun_ssh");
    let _ = slice.step(TunnelAction::TunnelSaved { result: Ok(tunnel) }, &strings());
    loaded(&mut slice, vec![summary("tun_ssh", "Bastion", TunnelKind::Ssh)]);
    let _ = slice.step(
        TunnelAction::HydrateRef {
            id: Some("tun_ssh".to_owned()),
            kind: None,
        },
        &strings(),
    );
    assert_eq!(slice.source(), TunnelSource::Saved);
    assert_eq!(slice.kind(), None);
    assert_eq!(slice.effective_kind(), Some(TunnelKind::Ssh), "entity wins");
    assert!(!slice.tunnel_ref_missing());
}

#[test]
fn reset_and_supports_ssh_defaults() {
    let mut slice = state();
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    slice.set_ssh_host("bastion");
    slice.set_ssh_username("ops");
    let _ = slice.step(TunnelAction::Reset, &strings());
    assert_eq!(slice.source(), TunnelSource::None);
    assert_eq!(slice.kind(), None);
    assert_eq!(slice.ssh().host.text, "");
    assert_eq!(slice.ssh().port.text, "22");
    assert!(!slice.inline_valid());

    let mut no_ssh = TunnelFormState::new(false);
    let _ = no_ssh.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    assert_eq!(no_ssh.kind(), Some(TunnelKind::HttpProxy));
    assert!(!no_ssh.ssh().enabled);
}

#[test]
fn react_falls_back_for_a_zero_port() {
    let mut slice = state();
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    let _ = slice.step(TunnelAction::KindChanged(Some(TunnelKind::HttpProxy)), &strings());
    slice.set_http_proxy_host("proxy.example");
    slice.set_http_proxy_port("0");
    slice.set_http_proxy_timeout("0");
    let config = slice.build_inline("Zero").expect("inline config");
    let proxy = config.http_proxy.expect("http proxy config");
    assert_eq!(
        proxy.port, 8080,
        "React falls back for a zero port; left: {} right: 8080",
        proxy.port
    );
    assert_eq!(proxy.connect_timeout_secs, 30);

    let mut ssh = state();
    let _ = ssh.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    ssh.set_ssh_port("0");
    ssh.set_ssh_jump_port("0");
    ssh.set_ssh_host("host");
    ssh.set_ssh_username("user");
    let ssh_config = ssh.build_inline("Zero ssh").expect("ssh config");
    let ssh_leg = ssh_config.ssh.expect("ssh leg");
    assert_eq!(ssh_leg.port, 22);
    assert_eq!(ssh_leg.jump, None);
}

#[test]
fn react_clears_the_error_on_refill() {
    let mut slice = state();
    loaded(&mut slice, vec![summary("tun_ws", "Relay", TunnelKind::WebSocket)]);
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Saved), &strings());

    let first = slice.step(TunnelAction::Unbind, &strings());
    assert_eq!(first.effects.len(), 1);
    let _ = slice.step(TunnelAction::TunnelFetched { result: Ok(None) }, &strings());
    assert!(slice.error().is_some(), "the first refill failed");
    assert_eq!(slice.source(), TunnelSource::Saved, "reference kept");

    let second = slice.step(TunnelAction::Unbind, &strings());
    assert_eq!(second.effects.len(), 1);
    let _ = slice.step(
        TunnelAction::TunnelFetched { result: Ok(Some(ssh_tunnel("tun_ws"))) },
        &strings(),
    );
    assert_eq!(
        slice.error(),
        None,
        "React clears the error on refill; left: {:?} right: None",
        slice.error()
    );
    assert_eq!(slice.source(), TunnelSource::Inline);
}
