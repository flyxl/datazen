//! HTTP CONNECT 代理的**上游探针**与数据路径测试（`verify_upstream` /
//! `HttpProxyTunnel::start`）。
//!
//! 自桌面 `commands/tunnel_probe_tests.rs` 整体归位（B2 隧道域刀）：这些测试
//! 钉的是机制层的逐字节语义——CONNECT 请求行与请求头、非 200 拒绝、
//! `connect_timeout_secs` 封顶、凭据不回显——机制住在 `http_proxy.rs`，测试
//! 跟着机制走，断言逐字保留。宿主侧不再保留副本（桌面命令入口的烟测在
//! `commands/tunnel.rs` 测试模块）。

use datazen_driver_api::HttpProxyTunnelConfig;

use super::{verify_upstream, HttpProxyTunnel};
use crate::tunnel::test_support::rebindable;

/// `127.0.0.1:1` has no listener (and needs root to ever get one), so it is a
/// deterministic unreachable endpoint with no bind/drop race.
const DEAD_PORT: u16 = 1;

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
    let proxy = HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "http".into(),
        username: Some("ignored-user".into()),
        password: Some("ignored-pass".into()),
        headers: Some(headers),
        connect_timeout_secs: 5,
    };

    verify_upstream(&proxy, "db.internal", 5432)
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

        let proxy = HttpProxyTunnelConfig {
            enabled: true,
            host: "127.0.0.1".into(),
            port,
            scheme: "http".into(),
            username: None,
            password: None,
            headers: None,
            connect_timeout_secs: 5,
        };
        let err = verify_upstream(&proxy, "db.internal", 5432)
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

    let proxy = HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "http".into(),
        username: None,
        password: None,
        headers: None,
        connect_timeout_secs: 5,
    };
    let err = verify_upstream(&proxy, "db.internal", 5432)
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
    let empty_host = HttpProxyTunnelConfig {
        enabled: true,
        host: "   ".into(),
        port: DEAD_PORT,
        scheme: "http".into(),
        username: None,
        password: None,
        headers: None,
        connect_timeout_secs: 1,
    };
    let err = verify_upstream(&empty_host, "db.internal", 5432)
        .await
        .expect_err("an empty proxy host must be rejected");
    assert!(err.to_string().contains("host is empty"), "{err}");

    let bad_scheme = HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port: DEAD_PORT,
        scheme: "socks5".into(),
        username: None,
        password: None,
        headers: None,
        connect_timeout_secs: 1,
    };
    let err = verify_upstream(&bad_scheme, "db.internal", 5432)
        .await
        .expect_err("an unsupported proxy scheme must be rejected");
    assert!(
        err.to_string().contains("unsupported HTTP proxy scheme"),
        "{err}"
    );
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
    let config = HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "http".into(),
        username: Some("u".into()),
        password: Some("p".into()),
        headers: Some(headers),
        connect_timeout_secs: 3,
    };
    let tunnel = HttpProxyTunnel::start(&config, "db.internal", 5432)
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

    let mut proxy = HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "http".into(),
        username: Some("user".into()),
        password: Some("s3cr3t".into()),
        headers: None,
        connect_timeout_secs: 5,
    };
    verify_upstream(&proxy, "db.internal", 5432)
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
    let err = verify_upstream(&proxy, "db.internal", 5432)
        .await
        .expect_err("407 must fail the probe");
    let text = err.to_string();
    assert!(
        !text.contains("s3cr3t") && !text.contains("dXNlcjpzM2NyM3Q="),
        "the error message must not carry proxy credentials: {text}"
    );
    fixture.abort();
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

    let proxy = HttpProxyTunnelConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        scheme: "https".into(),
        username: None,
        password: None,
        headers: None,
        connect_timeout_secs: 3,
    };
    let outcome = verify_upstream(&proxy, "db.internal", 5432).await;
    assert!(
        outcome.is_err(),
        "a plaintext endpoint under scheme=https must yield Err, got {outcome:?}"
    );
    fixture.abort();
}
