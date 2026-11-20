//! Independent audit for T358: `useTunnelFormState` port.
//!
//! Fill branch coverage the coder does not drive: the full positive-port rule,
//! WebSocket validity, the SSH toggle suppression, and reset clearing errors.

use crate::components::connection::connection_advanced_settings::{TunnelKind, TunnelSource};
use crate::components::connection::use_tunnel_form_state::{TunnelAction, TunnelFormState, TunnelStrings};

fn strings() -> TunnelStrings {
    TunnelStrings {
        unbind_kept: "unbindKept".to_owned(),
        save_failed: "saveFailed".to_owned(),
    }
}

#[test]
fn http_proxy_validity_requires_a_positive_finite_port() {
    let mut slice = TunnelFormState::new(true);
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    let _ = slice.step(TunnelAction::KindChanged(Some(TunnelKind::HttpProxy)), &strings());
    slice.set_http_proxy_host("proxy.internal");

    for bad in ["0", "-1", "abc", "", "  "] {
        slice.set_http_proxy_port(bad);
        assert!(!slice.inline_valid(), "port {bad:?} must be rejected");
    }
    for good in ["1", "3128", "65535"] {
        slice.set_http_proxy_port(good);
        assert!(slice.inline_valid(), "port {good:?} must be accepted");
    }
    slice.set_http_proxy_host("  ");
    assert!(!slice.inline_valid(), "a blank host is rejected");
}

#[test]
fn websocket_validity_and_ssh_toggle_suppression() {
    let mut slice = TunnelFormState::new(true);
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    let _ = slice.step(TunnelAction::KindChanged(Some(TunnelKind::WebSocket)), &strings());
    assert!(!slice.inline_valid(), "no URL is invalid");
    slice.set_ws_url("  ");
    assert!(!slice.inline_valid(), "a blank URL is invalid");
    slice.set_ws_url("wss://relay.example/v1");
    assert!(slice.inline_valid());
    assert_eq!(slice.effective_kind(), Some(TunnelKind::WebSocket));

    // SSH with its toggle off carries no config: effective kind is `none`.
    let _ = slice.step(TunnelAction::KindChanged(Some(TunnelKind::Ssh)), &strings());
    let _ = slice.step(TunnelAction::SshEnabled(false), &strings());
    assert_eq!(slice.effective_kind(), None, "a disabled SSH toggle suppresses the kind");
    slice.set_ssh_host("bastion");
    slice.set_ssh_username("ops");
    assert!(!slice.inline_valid(), "SSH is invalid while disabled");
    let _ = slice.step(TunnelAction::SshEnabled(true), &strings());
    assert!(slice.inline_valid(), "SSH becomes valid once enabled");
    assert_eq!(slice.effective_kind(), Some(TunnelKind::Ssh));
}

#[test]
fn kind_none_from_saved_clears_the_reference_without_a_refill() {
    let mut slice = TunnelFormState::new(true);
    let _ = slice.step(
        TunnelAction::SummariesLoaded {
            items: vec![crate::components::connection::use_tunnel_form_state::SavedSummary {
                id: "tun_a".to_owned(),
                name: "A".to_owned(),
                kind: TunnelKind::Ssh,
            }],
            error: None,
        },
        &strings(),
    );
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Saved), &strings());
    assert_eq!(slice.tunnel_id(), Some("tun_a"));

    // React's setTunnelKind('none') delegates to setTunnelSource('none'), which
    // clears the id synchronously — no getTunnel refill is involved.
    let step = slice.step(TunnelAction::KindChanged(None), &strings());
    assert!(step.effects.is_empty(), "no refill on an explicit none transition");
    assert_eq!(slice.source(), TunnelSource::None);
    assert_eq!(slice.kind(), None);
    assert_eq!(slice.tunnel_id(), None, "the saved reference is cleared");
    assert_eq!(slice.effective_kind(), None);
}

#[test]
fn reset_clears_the_error_and_restores_default_ports() {
    let mut slice = TunnelFormState::new(false);
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    slice.set_http_proxy_port("9999");
    slice.set_ws_timeout("99");
    slice.set_ssh_port("2222");
    // Force an error via a failed save answer so it is observable before reset.
    let _ = slice.step(
        TunnelAction::TunnelSaved {
            result: Err(crate::components::connection::use_tunnel_form_state::TunnelIoError::Message("bad".to_owned())),
        },
        &strings(),
    );
    assert_eq!(slice.error(), Some("bad"));

    let _ = slice.step(TunnelAction::Reset, &strings());
    assert_eq!(slice.error(), None, "reset clears the error");
    assert_eq!(slice.source(), TunnelSource::None);
    assert_eq!(slice.kind(), None);
    assert_eq!(slice.http_proxy().port.text, "8080");
    assert_eq!(slice.websocket().timeout.text, "30");
    assert_eq!(slice.ssh().port.text, "22");
    assert!(!slice.inline_valid());
    let _ = slice.step(TunnelAction::SourceChanged(TunnelSource::Inline), &strings());
    assert_eq!(slice.kind(), Some(TunnelKind::HttpProxy), "no-SSH driver defaults to HTTP proxy");
}
