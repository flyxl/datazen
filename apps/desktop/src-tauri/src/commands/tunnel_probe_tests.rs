//! [tester round 2] Adversarial re-verification of the tunnel probes and of the
//! refactor's production-path behaviour (BUG-001 / BUG-002 re-check).
//!
//! Deliberately kept out of `commands/tunnel.rs` so that file stays close to the
//! 800-line guideline (AGENTS.md, "单文件规模与模块拆分").

use super::tunnel::{get_tunnel_usage_impl, test_tunnel_impl};
use crate::testing::app_state::{sample_postgres_config, TestAppState};
use datazen_driver_api::{SavedTunnel, SshTunnelConfig, TunnelKind};

// ── [tester round 2] adversarial re-verification of BUG-001 ──────

/// `127.0.0.1:1` has no listener (and needs root to ever get one), so it is a
/// deterministic unreachable endpoint with no bind/drop race.
const DEAD_PORT: u16 = 1;

fn saved_proxy_tunnel_at(
    id: &str,
    host: &str,
    port: u16,
    scheme: &str,
    headers: Option<std::collections::HashMap<String, String>>,
    timeout: u32,
) -> SavedTunnel {
    SavedTunnel {
        id: id.into(),
        name: "Proxy".into(),
        kind: TunnelKind::HttpProxy,
        ssh: None,
        http_proxy: Some(datazen_driver_api::HttpProxyTunnelConfig {
            enabled: true,
            host: host.into(),
            port,
            scheme: scheme.into(),
            username: None,
            password: None,
            headers,
            connect_timeout_secs: timeout,
        }),
        websocket: None,
    }
}

fn saved_websocket_tunnel_at(id: &str, url: &str, mode: &str, timeout: u32) -> SavedTunnel {
    SavedTunnel {
        id: id.into(),
        name: "Relay".into(),
        kind: TunnelKind::WebSocket,
        ssh: None,
        http_proxy: None,
        websocket: Some(datazen_driver_api::WebSocketTunnelConfig {
            enabled: true,
            url: url.into(),
            auth_token: None,
            headers: None,
            connect_timeout_secs: timeout,
            ping_interval_secs: 0,
            mode: mode.into(),
        }),
    }
}

fn saved_ssh_tunnel_at(id: &str, host: &str, port: u16) -> SavedTunnel {
    SavedTunnel {
        id: id.into(),
        name: "Bastion".into(),
        kind: TunnelKind::Ssh,
        ssh: Some(SshTunnelConfig {
            enabled: true,
            host: host.into(),
            port,
            username: "ubuntu".into(),
            auth_method: "password".into(),
            password: Some("secret".into()),
            private_key_path: None,
            passphrase: None,
            jump: None,
        }),
        http_proxy: None,
        websocket: None,
    }
}

/// [tester] (a)1 — adversarial re-check of BUG-001, written from scratch:
/// all three kinds pointed at an endpoint with nothing listening must answer
/// `Err`. The pre-fix code answered `Ok(0)` for httpProxy and websocket.
#[tokio::test]
async fn test_tester_every_kind_rejects_an_unreachable_endpoint() {
    let test = TestAppState::new().await;
    test.store
        .save_tunnel(saved_proxy_tunnel_at(
            "t-dead-http",
            "127.0.0.1",
            DEAD_PORT,
            "http",
            None,
            1,
        ))
        .await
        .unwrap();
    test.store
        .save_tunnel(saved_websocket_tunnel_at(
            "t-dead-ws",
            &format!("ws://127.0.0.1:{DEAD_PORT}/tunnel"),
            "datazen_v1",
            1,
        ))
        .await
        .unwrap();
    test.store
        .save_tunnel(saved_ssh_tunnel_at("t-dead-ssh", "127.0.0.1", DEAD_PORT))
        .await
        .unwrap();

    for (id, needle) in [
        ("t-dead-http", "connect to proxy"),
        ("t-dead-ws", "WebSocket connect"),
        ("t-dead-ssh", "SSH connect"),
    ] {
        let err = test_tunnel_impl(&test.state, id.into(), "db.internal".into(), 5432)
            .await
            .expect_err("an unreachable endpoint must not be reported as reachable");
        let text = err.to_string();
        assert!(text.contains(needle), "{id}: unexpected error: {text}");
    }
}

/// [tester] (e)#2 — the empty-id guard must actually exclude connections
/// whose reference is the empty string (this is what `materialize_tunnel_refs`
/// treats as "no reference"); the pre-fix code matched `Some("")`.
#[tokio::test]
async fn test_tester_usage_with_empty_id_excludes_empty_string_references() {
    let test = TestAppState::new().await;

    let mut empty_ref = sample_postgres_config("c-empty-ref");
    empty_ref.name = "Empty ref".into();
    empty_ref.tunnel_id = Some(String::new());
    test.store.save_connection(empty_ref).await.unwrap();

    let mut real_ref = sample_postgres_config("c-real-ref");
    real_ref.name = "Real ref".into();
    real_ref.tunnel_id = Some("t1".into());
    test.store.save_connection(real_ref).await.unwrap();

    let empty = get_tunnel_usage_impl(&test.state, String::new())
        .await
        .unwrap();
    assert!(
        empty.connection_ids.is_empty() && empty.connection_names.is_empty(),
        "an empty query must not match an empty-string reference: {empty:?}"
    );

    // The genuine reference is still found by its own id.
    let hit = get_tunnel_usage_impl(&test.state, "t1".into())
        .await
        .unwrap();
    assert_eq!(hit.connection_ids, vec!["c-real-ref".to_string()]);
}
// ── [tester round 2] probe internals: real handshakes, not TCP-only ──

/// [tester] (a)2 — the probe must build the real CONNECT request and accept
/// only a `200`. This pins the request line, the `Host` header, the explicit
/// `Proxy-Authorization` priority over derived Basic credentials and the
/// terminating keep-alive headers.
#[tokio::test]
async fn test_tester_http_proxy_probe_performs_a_real_connect_handshake() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while !request.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await.expect("read") == 0 {
                break;
            }
            request.push(byte[0]);
        }
        let _ = tx.send(String::from_utf8_lossy(&request).to_string());
        stream
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await
            .expect("respond 200");
    });

    let mut headers = std::collections::HashMap::new();
    headers.insert("X-Corp".to_string(), "1".to_string());
    headers.insert(
        "Proxy-Authorization".to_string(),
        "Bearer explicit".to_string(),
    );
    let proxy = datazen_driver_api::HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "http".into(),
        username: Some("ignored-user".into()),
        password: Some("ignored-pass".into()),
        headers: Some(headers),
        connect_timeout_secs: 5,
    };

    crate::tunnel::verify_http_proxy_upstream(&proxy, "db.internal", 5432)
        .await
        .expect("a 200 CONNECT must pass the probe");

    let request = rx.await.expect("fixture request");
    assert!(
        request.starts_with("CONNECT db.internal:5432 HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(
        request.contains("\r\nHost: db.internal:5432\r\n"),
        "{request}"
    );
    assert!(
        request.contains("\r\nProxy-Authorization: Bearer explicit\r\n"),
        "{request}"
    );
    assert!(
        !request.contains("Basic "),
        "an explicit Proxy-Authorization header must win over Basic credentials: {request}"
    );
    assert!(request.contains("\r\nX-Corp: 1\r\n"), "{request}");
    assert!(
        request.ends_with("Proxy-Connection: Keep-Alive\r\nConnection: Keep-Alive\r\n\r\n"),
        "{request}"
    );
    fixture.abort();
}

/// [tester] (a)2 — a reachable proxy that answers anything but 200 must fail
/// with the observed status, so a bare TCP connect can never pass.
#[tokio::test]
async fn test_tester_http_proxy_probe_rejects_non_200_statuses() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    for (status, needle) in [
        (
            "407 Proxy Authentication Required",
            "CONNECT rejected with status 407",
        ),
        ("502 Bad Gateway", "CONNECT rejected with status 502"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fixture");
        let port = listener.local_addr().expect("fixture addr").port();
        let fixture = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let _ = stream
                .write_all(format!("HTTP/1.1 {status}\r\n\r\n").as_bytes())
                .await;
            let _ = stream.shutdown().await;
        });

        let proxy = datazen_driver_api::HttpProxyTunnelConfig {
            enabled: true,
            host: "127.0.0.1".into(),
            port,
            scheme: "http".into(),
            username: None,
            password: None,
            headers: None,
            connect_timeout_secs: 5,
        };
        let err = crate::tunnel::verify_http_proxy_upstream(&proxy, "db.internal", 5432)
            .await
            .expect_err("a non-200 CONNECT must fail the probe");
        assert!(
            err.to_string().contains(needle),
            "expected `{needle}` in: {err}"
        );
        fixture.abort();
    }
}

/// [tester] (a)2 — a socket that answers with a non-HTTP line must not be
/// mistaken for a successful handshake.
#[tokio::test]
async fn test_tester_http_proxy_probe_rejects_a_malformed_status_line() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).await;
        let _ = stream.write_all(b"NOT-HTTP\r\n\r\n").await;
        let _ = stream.shutdown().await;
    });

    let proxy = datazen_driver_api::HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "http".into(),
        username: None,
        password: None,
        headers: None,
        connect_timeout_secs: 5,
    };
    let err = crate::tunnel::verify_http_proxy_upstream(&proxy, "db.internal", 5432)
        .await
        .expect_err("a malformed status line must fail the probe");
    assert!(
        err.to_string()
            .contains("invalid CONNECT response status line"),
        "unexpected error: {err}"
    );
    fixture.abort();
}

/// [tester] (a)1 — config validation happens before any dialing, so the
/// probe rejects an empty host / unsupported scheme with the config error.
#[tokio::test]
async fn test_tester_http_proxy_probe_validates_its_config_first() {
    let empty_host = datazen_driver_api::HttpProxyTunnelConfig {
        enabled: true,
        host: "   ".into(),
        port: DEAD_PORT,
        scheme: "http".into(),
        username: None,
        password: None,
        headers: None,
        connect_timeout_secs: 1,
    };
    let err = crate::tunnel::verify_http_proxy_upstream(&empty_host, "db.internal", 5432)
        .await
        .expect_err("an empty proxy host must be rejected");
    assert!(err.to_string().contains("host is empty"), "{err}");

    let bad_scheme = datazen_driver_api::HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port: DEAD_PORT,
        scheme: "socks5".into(),
        username: None,
        password: None,
        headers: None,
        connect_timeout_secs: 1,
    };
    let err = crate::tunnel::verify_http_proxy_upstream(&bad_scheme, "db.internal", 5432)
        .await
        .expect_err("an unsupported proxy scheme must be rejected");
    assert!(
        err.to_string().contains("unsupported HTTP proxy scheme"),
        "{err}"
    );
}

/// [tester] (a)3 — the reverse fixture: a relay that completes the WS
/// handshake but answers `op=error` must fail the probe, so "unreachable"
/// is not the only failure mode that is caught.
#[tokio::test]
async fn test_tester_websocket_probe_rejects_a_relay_that_errors_the_open() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind relay fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let fixture = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        if let Some(Ok(Message::Text(text))) = ws.next().await {
            let open: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            if open.get("op").and_then(|v| v.as_str()) == Some("open") {
                let _ = ws
                    .send(Message::Text(
                        r#"{"op":"error","message":"no route to host"}"#.to_string().into(),
                    ))
                    .await;
            }
        }
        while let Some(Ok(msg)) = ws.next().await {
            if msg.is_close() {
                break;
            }
        }
    });

    let cfg = datazen_driver_api::WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 5,
        ping_interval_secs: 0,
        mode: "datazen_v1".into(),
    };
    let err = crate::tunnel::verify_websocket_upstream(&cfg, "db.internal", 5432)
        .await
        .expect_err("a relay error ack must fail the probe");
    let text = err.to_string();
    assert!(text.contains("relay rejected open"), "{text}");
    assert!(text.contains("no route to host"), "{text}");
    fixture.abort();
}

/// [tester] (a)3/(c) — the WS probe is bounded by `connect_timeout_secs`
/// even when the relay accepts the handshake and then goes silent (contrast
/// with the HTTP-proxy CONNECT read, see BUG-003).
#[tokio::test]
async fn test_tester_websocket_probe_is_bounded_when_the_relay_never_acks() {
    use futures_util::StreamExt;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind relay fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let fixture = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        // Consume the `open` frame and then never answer it.
        let _ = ws.next().await;
        while let Some(Ok(msg)) = ws.next().await {
            if msg.is_close() {
                break;
            }
            let _ = msg;
        }
    });

    let cfg = datazen_driver_api::WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 1,
        ping_interval_secs: 0,
        mode: "datazen_v1".into(),
    };
    let started = std::time::Instant::now();
    let err = crate::tunnel::verify_websocket_upstream(&cfg, "db.internal", 5432)
        .await
        .expect_err("a silent relay must fail the probe");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the WS probe must honour connect_timeout_secs, took {:?}",
        started.elapsed()
    );
    assert!(
        err.to_string()
            .contains("timed out waiting for WebSocket open ack"),
        "unexpected error: {err}"
    );
    fixture.abort();
}

/// [tester] (a)4 — `raw_binary` has no application-level ack: the probe's
/// contract is "the relay completed a WebSocket handshake for this target".
/// This pins that (a) the injected `host`/`port` query is what the relay
/// sees and (b) a relay that never speaks the application protocol still
/// passes — a documented semantic boundary, not a TCP-only false positive.
#[tokio::test]
async fn test_tester_raw_binary_probe_verifies_the_ws_handshake_and_carries_the_target() {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind relay fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let fixture = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut ws =
            tokio_tungstenite::accept_hdr_async(stream, move |req: &Request, resp: Response| {
                let _ = tx.send(req.uri().to_string());
                Ok(resp)
            })
            .await
            .expect("relay handshake");
        while let Some(Ok(msg)) = ws.next().await {
            if msg.is_close() {
                break;
            }
        }
    });

    let cfg = datazen_driver_api::WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel?token=abc&host=stale&port=1"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 5,
        ping_interval_secs: 0,
        mode: "raw_binary".into(),
    };
    crate::tunnel::verify_websocket_upstream(&cfg, "db.internal", 5432)
        .await
        .expect("a completed WS handshake is the raw_binary probe contract");

    let uri = rx.recv().await.expect("fixture must see the request URI");
    assert!(uri.contains("token=abc"), "{uri}");
    assert!(uri.contains("host=db.internal"), "{uri}");
    assert!(uri.contains("port=5432"), "{uri}");
    assert!(
        !uri.contains("host=stale"),
        "stale target query must be replaced: {uri}"
    );
    fixture.abort();
}

/// [tester] (a)4 — `raw_binary` must still fail when the endpoint is not a
/// WebSocket at all, i.e. the probe is not a bare TCP connect.
#[tokio::test]
async fn test_tester_raw_binary_probe_rejects_a_non_websocket_endpoint() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).await;
        let _ = stream.write_all(b"HTTP/1.1 500 Nope\r\n\r\n").await;
        let _ = stream.shutdown().await;
    });

    let cfg = datazen_driver_api::WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 2,
        ping_interval_secs: 0,
        mode: "raw_binary".into(),
    };
    let err = crate::tunnel::verify_websocket_upstream(&cfg, "db.internal", 5432)
        .await
        .expect_err("a non-WebSocket endpoint must fail the raw_binary probe");
    assert!(
        err.to_string().contains("WebSocket connect failed"),
        "unexpected error: {err}"
    );
    fixture.abort();
}

/// [tester] (c) — the refactor extracted `proxy_headers` /
/// `resolve_auth_header` / `dial_proxy` out of the *production* data path.
/// This drives `HttpProxyTunnel::start` (not the probe) and pins the request
/// bytes plus end-to-end forwarding, so behaviour drift cannot hide behind
/// the pre-existing forwarding test.
#[tokio::test]
async fn test_tester_connect_proxy_data_path_keeps_auth_priority_and_forwarding() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind proxy fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while !request.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await.expect("read") == 0 {
                break;
            }
            request.push(byte[0]);
        }
        let _ = tx.send(String::from_utf8_lossy(&request).to_string());
        stream
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await
            .expect("respond 200");
        let mut payload = [0u8; 4];
        stream.read_exact(&mut payload).await.expect("payload");
        assert_eq!(&payload, b"ping");
        stream.write_all(b"pong").await.expect("pong");
    });

    let mut headers = std::collections::HashMap::new();
    headers.insert("X-Corp".to_string(), "1".to_string());
    headers.insert(
        "Proxy-Authorization".to_string(),
        "Bearer explicit".to_string(),
    );
    let config = datazen_driver_api::HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "http".into(),
        username: Some("u".into()),
        password: Some("p".into()),
        headers: Some(headers),
        connect_timeout_secs: 3,
    };
    let tunnel = crate::tunnel::HttpProxyTunnel::start(&config, "db.internal", 5432)
        .await
        .expect("start data-path tunnel");
    let mut local = tokio::net::TcpStream::connect(("127.0.0.1", tunnel.local_port()))
        .await
        .expect("connect local listener");
    local.write_all(b"ping").await.expect("write ping");
    let mut response = [0u8; 4];
    local.read_exact(&mut response).await.expect("read pong");
    assert_eq!(&response, b"pong");

    let request = rx.await.expect("fixture request");
    assert!(
        request.starts_with("CONNECT db.internal:5432 HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(
        request.contains("\r\nProxy-Authorization: Bearer explicit\r\n"),
        "{request}"
    );
    assert!(
        !request.contains("Basic "),
        "the explicit header must still win in the data path: {request}"
    );
    assert!(request.contains("\r\nX-Corp: 1\r\n"), "{request}");
    assert!(
        request.ends_with("Proxy-Connection: Keep-Alive\r\nConnection: Keep-Alive\r\n\r\n"),
        "{request}"
    );
    fixture.await.expect("fixture");

    // (c) `Drop for HttpProxyTunnel` must release the local port.
    let port = tunnel.local_port();
    drop(local);
    drop(tunnel);
    assert!(
        rebindable(port).await,
        "dropping an HttpProxyTunnel must release 127.0.0.1:{port}"
    );
}

async fn rebindable(port: u16) -> bool {
    for _ in 0..100 {
        if tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    false
}

/// [tester] (c) — the extracted `ws_ping_interval` must still drive the data
/// path's keepalive: an idle local connection makes the tunnel emit WS pings.
#[tokio::test]
async fn test_tester_websocket_data_path_still_sends_ping_frames() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind relay fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
    let fixture = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        if let Some(Ok(Message::Text(text))) = ws.next().await {
            let open: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            if open.get("op").and_then(|v| v.as_str()) == Some("open") {
                let _ = ws
                    .send(Message::Text(r#"{"op":"opened"}"#.to_string().into()))
                    .await;
            }
        }
        let mut saw_ping = false;
        while let Some(Ok(msg)) = ws.next().await {
            if msg.is_ping() {
                saw_ping = true;
                break;
            }
            if msg.is_close() {
                break;
            }
        }
        let _ = tx.send(saw_ping);
    });

    let cfg = datazen_driver_api::WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 5,
        ping_interval_secs: 1,
        mode: "datazen_v1".into(),
    };
    let tunnel = crate::tunnel::WebSocketTunnel::start(&cfg, "db.internal", 5432)
        .await
        .expect("start data-path tunnel");
    // Keep the local socket open and idle so only the ping timer can fire.
    let _local = tokio::net::TcpStream::connect(("127.0.0.1", tunnel.local_port()))
        .await
        .expect("connect local listener");

    let saw_ping = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
        .await
        .expect("the relay must receive a ping within 5s")
        .expect("fixture result");
    assert!(saw_ping, "the data path must keep sending keepalive pings");

    // (c) `Drop for WebSocketTunnel` must release the local port.
    let port = tunnel.local_port();
    drop(_local);
    drop(tunnel);
    assert!(
        rebindable(port).await,
        "dropping a WebSocketTunnel must release 127.0.0.1:{port}"
    );
    fixture.abort();
}
/// [tester] (a)1/(a)5 — the SSH **success** path through the real IPC entry
/// point, which the previous round could not exercise ("needs a live SSH
/// server"). Runs against the in-process bastion fixture and also pins the
/// SSH semantic boundary: the probe must not open a `direct-tcpip` channel
/// for the target, and `test_tunnel`'s `drop` must not leak the session.
#[tokio::test]
async fn test_tester_test_tunnel_succeeds_against_a_real_ssh_bastion() {
    use std::sync::atomic::Ordering;

    let bastion = super::tunnel_probe_fixture::spawn_bastion().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let key_path = super::tunnel_probe_fixture::write_client_key(dir.path());

    let test = TestAppState::new().await;
    let mut tunnel = saved_ssh_tunnel_at("t-live-ssh", "127.0.0.1", bastion.port);
    if let Some(ssh) = tunnel.ssh.as_mut() {
        ssh.auth_method = "private_key".into();
        ssh.password = None;
        ssh.username = "tester".into();
        ssh.private_key_path = Some(key_path.to_string_lossy().into_owned());
    }
    test.store.save_tunnel(tunnel).await.unwrap();

    let elapsed_ms = test_tunnel_impl(&test.state, "t-live-ssh".into(), "127.0.0.1".into(), 5432)
        .await
        .expect("a reachable bastion must pass the probe");
    assert!(elapsed_ms < 5_000, "probe took {elapsed_ms}ms");
    assert_eq!(
        bastion.channel_opens.load(Ordering::SeqCst),
        0,
        "the SSH probe must not open a direct-tcpip channel for the target"
    );

    let mut closed = false;
    for _ in 0..300 {
        if bastion.live_sessions.load(Ordering::SeqCst) == 0 {
            closed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        closed,
        "test_tunnel must tear the SSH session down, still live: {}",
        bastion.live_sessions.load(Ordering::SeqCst)
    );
    bastion.shutdown();
}

/// [tester] (协调者追加项 2) — the probe must send the **Basic** credentials
/// derived from username/password (no explicit header), and must never echo
/// them into an error message (RFC §5.4: "日志与 UI 不得打印代理密码 / token").
#[tokio::test]
async fn test_tester_http_proxy_probe_sends_basic_credentials_without_echoing_them() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while !request.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await.expect("read") == 0 {
                break;
            }
            request.push(byte[0]);
        }
        let _ = tx.send(String::from_utf8_lossy(&request).to_string());
        stream
            .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
            .await
            .expect("respond 200");
    });

    let mut proxy = datazen_driver_api::HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "http".into(),
        username: Some("user".into()),
        password: Some("s3cr3t".into()),
        headers: None,
        connect_timeout_secs: 5,
    };
    crate::tunnel::verify_http_proxy_upstream(&proxy, "db.internal", 5432)
        .await
        .expect("a 200 CONNECT must pass the probe");

    let request = rx.await.expect("fixture request");
    assert!(
        request.contains("\r\nProxy-Authorization: Basic dXNlcjpzM2NyM3Q=\r\n"),
        "the probe must derive Basic credentials: {request}"
    );
    fixture.abort();

    // A rejecting proxy must not leak the password into the error message.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while !request.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).await.expect("read") == 0 {
                break;
            }
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
            .await
            .expect("respond 407");
    });
    proxy.port = port;
    let err = crate::tunnel::verify_http_proxy_upstream(&proxy, "db.internal", 5432)
        .await
        .expect_err("407 must fail the probe");
    let text = err.to_string();
    assert!(
        !text.contains("s3cr3t") && !text.contains("dXNlcjpzM2NyM3Q="),
        "the error message must not carry proxy credentials: {text}"
    );
    fixture.abort();
}

/// [tester] (协调者追加项 2) — `auth_token` + custom headers on the WS probe
/// path (no existing probe test set either), and the token must not surface in
/// an error message.
#[tokio::test]
async fn test_tester_websocket_probe_sends_the_bearer_token_and_custom_headers() {
    use futures_util::{SinkExt, StreamExt};
    use std::sync::{Arc, Mutex};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind relay fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let seen: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let seen_in_fixture = seen.clone();
    let fixture = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut ws = tokio_tungstenite::accept_hdr_async(
            stream,
            move |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
                  resp: tokio_tungstenite::tungstenite::handshake::server::Response| {
                let mut out = String::new();
                for (k, v) in req.headers() {
                    out.push_str(&format!("{}: {}\n", k.as_str(), v.to_str().unwrap_or("?")));
                }
                *seen_in_fixture.lock().expect("lock") = out;
                Ok(resp)
            },
        )
        .await
        .expect("accept ws");
        if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(_))) = ws.next().await {
            let _ = ws
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    r#"{"op":"opened","id":"x"}"#.into(),
                ))
                .await;
        }
        let _ = ws.next().await;
    });

    let mut headers = std::collections::HashMap::new();
    headers.insert("X-Relay".to_string(), "1".to_string());
    let relay = datazen_driver_api::WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: Some("s3cr3t-ws-token".into()),
        headers: Some(headers),
        connect_timeout_secs: 5,
        ping_interval_secs: 0,
        mode: "datazen_v1".into(),
    };
    crate::tunnel::verify_websocket_upstream(&relay, "db.internal", 5432)
        .await
        .expect("a relay that acks the open must pass the probe");

    let handshake = seen.lock().expect("lock").clone();
    assert!(
        handshake.contains("authorization: Bearer s3cr3t-ws-token"),
        "the probe must send the bearer token: {handshake}"
    );
    assert!(
        handshake.contains("x-relay: 1"),
        "the probe must send custom headers: {handshake}"
    );
    fixture.abort();

    // A relay that rejects the open must not leak the token.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind relay fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let fixture = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        while let Some(Ok(msg)) = ws.next().await {
            if msg.is_text() {
                let _ = ws
                    .send(tokio_tungstenite::tungstenite::Message::Text(
                        r#"{"op":"error","message":"no route"}"#.into(),
                    ))
                    .await;
                break;
            }
        }
    });
    let mut relay = relay;
    relay.url = format!("ws://127.0.0.1:{port}/tunnel");
    let err = crate::tunnel::verify_websocket_upstream(&relay, "db.internal", 5432)
        .await
        .expect_err("an error ack must fail the probe");
    let text = err.to_string();
    assert!(
        !text.contains("s3cr3t-ws-token"),
        "the error message must not carry the bearer token: {text}"
    );
    fixture.abort();
}

/// [tester] (协调者追加项 2) — a token embedded in the relay URL (the
/// `raw_binary` shape) must not leak through connect errors either.
#[tokio::test]
async fn test_tester_websocket_probe_never_echoes_a_url_embedded_token() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let relay_at = |url: String| datazen_driver_api::WebSocketTunnelConfig {
        enabled: true,
        url,
        auth_token: None,
        headers: None,
        connect_timeout_secs: 3,
        ping_interval_secs: 0,
        mode: "raw_binary".into(),
    };

    // (1) reachable but not a WebSocket endpoint
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf).await;
        let _ = stream
            .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n")
            .await;
    });
    let err = crate::tunnel::verify_websocket_upstream(
        &relay_at(format!("ws://127.0.0.1:{port}/tunnel?token=URLTOKEN123")),
        "db.internal",
        5432,
    )
    .await
    .expect_err("a non-WebSocket endpoint must fail the probe");
    assert!(
        !err.to_string().contains("URLTOKEN123"),
        "connect errors must not echo the relay URL token: {err}"
    );
    fixture.abort();

    // (2) unreachable relay with a token in the query
    let err = crate::tunnel::verify_websocket_upstream(
        &relay_at("ws://127.0.0.1:1/tunnel?token=URLTOKEN123".into()),
        "db.internal",
        5432,
    )
    .await
    .expect_err("an unreachable relay must fail the probe");
    assert!(
        !err.to_string().contains("URLTOKEN123"),
        "connect errors must not echo the relay URL token: {err}"
    );

    // (3) malformed URL that embeds the token
    let err = crate::tunnel::verify_websocket_upstream(
        &relay_at("not a url?token=URLTOKEN123".into()),
        "db.internal",
        5432,
    )
    .await
    .expect_err("a malformed relay URL must fail the probe");
    assert!(
        !err.to_string().contains("URLTOKEN123"),
        "URL parsing errors must not echo the relay URL token: {err}"
    );
}

/// [tester] (协调者追加项 1) — `scheme = "https"` must report a TLS failure as
/// `Err`, never panic or hang.
///
/// `#[ignore]` removed after the BUG-004 fix (process-wide rustls
/// `CryptoProvider` installed in `main()` and at every tunnel TLS construction
/// site); the body is unchanged and now runs as the standing regression guard
/// (plain TCP listener that never speaks TLS → must be `Err`).
#[tokio::test]
async fn test_tester_https_proxy_probe_reports_tls_failure_instead_of_panicking() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind plaintext fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut buf = [0u8; 512];
        let _ = stream.read(&mut buf).await;
        let _ = stream.write_all(b"not a TLS server\r\n").await;
    });

    let proxy = datazen_driver_api::HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "https".into(),
        username: None,
        password: None,
        headers: None,
        connect_timeout_secs: 3,
    };
    let outcome = crate::tunnel::verify_http_proxy_upstream(&proxy, "db.internal", 5432).await;
    assert!(
        outcome.is_err(),
        "a plaintext endpoint under scheme=https must yield Err, got {outcome:?}"
    );
    fixture.abort();
}

/// [tester] (协调者追加项 1) — the `wss://` variant of the same defect: a
/// `wss` relay probe must return `Err` on TLS failure, not panic.
///
/// `#[ignore]` removed after the BUG-004 fix, same as the `https` case; the body
/// is unchanged and now runs as the standing regression guard.
#[tokio::test]
async fn test_tester_wss_relay_probe_reports_tls_failure_instead_of_panicking() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind plaintext fixture");
    let port = listener.local_addr().expect("fixture addr").port();
    let fixture = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut buf = [0u8; 512];
        let _ = stream.read(&mut buf).await;
        let _ = stream.write_all(b"not a TLS server\r\n").await;
    });

    let relay = datazen_driver_api::WebSocketTunnelConfig {
        enabled: true,
        url: format!("wss://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 3,
        ping_interval_secs: 0,
        mode: "datazen_v1".into(),
    };
    let outcome = crate::tunnel::verify_websocket_upstream(&relay, "db.internal", 5432).await;
    assert!(
        outcome.is_err(),
        "a plaintext endpoint under wss:// must yield Err, got {outcome:?}"
    );
    fixture.abort();
}
