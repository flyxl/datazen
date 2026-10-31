//! WebSocket 中继的**上游探针**与数据路径测试（`verify_upstream` /
//! `WebSocketTunnel::start`）。
//!
//! 自桌面 `commands/tunnel_probe_tests.rs` 整体归位（B2 隧道域刀）：这些测试
//! 钉的是机制层语义——`datazen_v1` 的 `open`/`opened` 握手、`raw_binary` 的
//! 「WS 握手完成即证据」边界、静默中继的超时封顶、token 不回显——机制住在
//! `websocket.rs`，测试跟着机制走，断言逐字保留。宿主侧不再保留副本。

use datazen_driver_api::WebSocketTunnelConfig;

use super::{verify_upstream, WebSocketTunnel};
use crate::tunnel::test_support::rebindable;

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

    let cfg = WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 5,
        ping_interval_secs: 0,
        mode: "datazen_v1".into(),
    };
    let err = verify_upstream(&cfg, "db.internal", 5432)
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

    let cfg = WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 1,
        ping_interval_secs: 0,
        mode: "datazen_v1".into(),
    };
    let started = std::time::Instant::now();
    let err = verify_upstream(&cfg, "db.internal", 5432)
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

    let cfg = WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel?token=abc&host=stale&port=1"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 5,
        ping_interval_secs: 0,
        mode: "raw_binary".into(),
    };
    verify_upstream(&cfg, "db.internal", 5432)
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

    let cfg = WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 2,
        ping_interval_secs: 0,
        mode: "raw_binary".into(),
    };
    let err = verify_upstream(&cfg, "db.internal", 5432)
        .await
        .expect_err("a non-WebSocket endpoint must fail the raw_binary probe");
    assert!(
        err.to_string().contains("WebSocket connect failed"),
        "unexpected error: {err}"
    );
    fixture.abort();
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

    let cfg = WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 5,
        ping_interval_secs: 1,
        mode: "datazen_v1".into(),
    };
    let tunnel = WebSocketTunnel::start(&cfg, "db.internal", 5432)
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
    let relay = WebSocketTunnelConfig {
        enabled: true,
        url: format!("ws://127.0.0.1:{port}/tunnel"),
        auth_token: Some("s3cr3t-ws-token".into()),
        headers: Some(headers),
        connect_timeout_secs: 5,
        ping_interval_secs: 0,
        mode: "datazen_v1".into(),
    };
    verify_upstream(&relay, "db.internal", 5432)
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
    let err = verify_upstream(&relay, "db.internal", 5432)
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

    let relay_at = |url: String| WebSocketTunnelConfig {
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
    let err = verify_upstream(
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
    let err = verify_upstream(
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
    let err = verify_upstream(
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

/// [tester] (协调者追加项 1) — the `wss://` variant of the TLS defect: a
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

    let relay = WebSocketTunnelConfig {
        enabled: true,
        url: format!("wss://127.0.0.1:{port}/tunnel"),
        auth_token: None,
        headers: None,
        connect_timeout_secs: 3,
        ping_interval_secs: 0,
        mode: "datazen_v1".into(),
    };
    let outcome = verify_upstream(&relay, "db.internal", 5432).await;
    assert!(
        outcome.is_err(),
        "a plaintext endpoint under wss:// must yield Err, got {outcome:?}"
    );
    fixture.abort();
}
