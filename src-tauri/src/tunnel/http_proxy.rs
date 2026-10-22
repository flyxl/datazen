//! HTTP CONNECT tunnel: local TCP listener → HTTP(S) proxy CONNECT → remote DB.

use datazen_driver_api::{DriverError, HttpProxyTunnelConfig};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::{
    pki_types::{IpAddr, ServerName},
    ClientConfig, RootCertStore,
};
use tokio_rustls::TlsConnector;
use tokio_util::sync::CancellationToken;

pub struct HttpProxyTunnel {
    local_port: u16,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl HttpProxyTunnel {
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    pub async fn start(
        proxy: &HttpProxyTunnelConfig,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<Self, DriverError> {
        let scheme = normalize_scheme(proxy)?;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| DriverError::HttpProxyTunnelError(format!("Bind local port: {e}")))?;
        let local_port = listener
            .local_addr()
            .map_err(|e| DriverError::HttpProxyTunnelError(format!("get local port: {e}")))?
            .port();

        let proxy_host = proxy.host.clone();
        let proxy_port = proxy.port;
        let remote_host = remote_host.to_string();
        let timeout = connect_timeout(proxy);
        let extra_headers = proxy_headers(proxy);
        let auth_header = resolve_auth_header(proxy, &extra_headers);
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();

        tracing::info!(
            proxy = %format!("{proxy_host}:{proxy_port}"),
            local_port,
            remote = %format!("{remote_host}:{remote_port}"),
            "HTTP CONNECT tunnel established (listener ready)"
        );

        let task = tokio::spawn(async move {
            loop {
                let (mut inbound, _) = tokio::select! {
                    _ = task_cancel.cancelled() => break,
                    result = listener.accept() => match result {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!("HTTP proxy tunnel accept error: {e}");
                            break;
                        }
                    },
                };

                let proxy_host = proxy_host.clone();
                let remote_host = remote_host.clone();
                let auth_header = auth_header.clone();
                let extra_headers = extra_headers.clone();
                let child_cancel = task_cancel.clone();
                let scheme = scheme.clone();

                tokio::spawn(async move {
                    let result = connect_and_copy(
                        &mut inbound,
                        &scheme,
                        &proxy_host,
                        proxy_port,
                        &remote_host,
                        remote_port,
                        auth_header.as_deref(),
                        &extra_headers,
                        timeout,
                        &child_cancel,
                    )
                    .await;

                    if let Err(e) = result {
                        tracing::error!(error = %e, "HTTP CONNECT tunnel stream failed");
                    }
                });
            }
        });

        Ok(Self {
            local_port,
            cancel,
            task,
        })
    }
}

impl Drop for HttpProxyTunnel {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.task.abort();
    }
}

/// Lowercased and validated proxy scheme (`http` / `https`).
fn normalize_scheme(proxy: &HttpProxyTunnelConfig) -> Result<String, DriverError> {
    let scheme = proxy.scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(DriverError::HttpProxyTunnelError(format!(
            "unsupported HTTP proxy scheme '{scheme}'; use http or https"
        )));
    }
    if proxy.host.trim().is_empty() {
        return Err(DriverError::HttpProxyTunnelError(
            "HTTP proxy host is empty".into(),
        ));
    }
    Ok(scheme)
}

fn proxy_headers(proxy: &HttpProxyTunnelConfig) -> Vec<(String, String)> {
    proxy
        .headers
        .as_ref()
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// An explicit `Proxy-Authorization` header wins; otherwise derive Basic auth.
fn resolve_auth_header(
    proxy: &HttpProxyTunnelConfig,
    extra_headers: &[(String, String)],
) -> Option<String> {
    extra_headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("proxy-authorization"))
        .map(|(_, value)| value.clone())
        .or_else(|| basic_auth_header(proxy.username.as_deref(), proxy.password.as_deref()))
}

fn connect_timeout(proxy: &HttpProxyTunnelConfig) -> Duration {
    Duration::from_secs(u64::from(proxy.connect_timeout_secs.max(1)))
}

/// Dial the proxy host (TLS is applied separately by [`tls_connect`]).
async fn dial_proxy(
    proxy_host: &str,
    proxy_port: u16,
    timeout: Duration,
) -> Result<TcpStream, DriverError> {
    tokio::time::timeout(timeout, TcpStream::connect((proxy_host, proxy_port)))
        .await
        .map_err(|_| DriverError::HttpProxyTunnelError("connect to proxy timed out".into()))?
        .map_err(|e| {
            DriverError::HttpProxyTunnelError(format!(
                "connect to proxy {proxy_host}:{proxy_port}: {e}"
            ))
        })
}

/// Wrap `socket` in TLS for an `https://` proxy.
async fn tls_connect<S>(
    proxy_host: &str,
    socket: S,
    timeout: Duration,
) -> Result<tokio_rustls::client::TlsStream<S>, DriverError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    // `ClientConfig::builder()` panics when both provider features are compiled
    // in and no process default exists (tunnel-backend-BUG-004). Installing one
    // is idempotent, so this is safe on every connection and also covers library
    // and test embedders that never go through `main()`.
    crate::tls::install_default_crypto_provider();

    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect(),
    };
    let tls_config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = if let Ok(ip) = proxy_host.parse::<std::net::IpAddr>() {
        ServerName::IpAddress(IpAddr::from(ip))
    } else {
        ServerName::try_from(proxy_host.to_owned()).map_err(|e| {
            DriverError::HttpProxyTunnelError(format!("invalid HTTPS proxy host: {e}"))
        })?
    };
    let connector = TlsConnector::from(std::sync::Arc::new(tls_config));
    tokio::time::timeout(timeout, connector.connect(server_name, socket))
        .await
        .map_err(|_| DriverError::HttpProxyTunnelError("TLS proxy handshake timed out".into()))?
        .map_err(|e| DriverError::HttpProxyTunnelError(format!("TLS proxy handshake failed: {e}")))
}

/// Prove the proxy endpoint is reachable **and** accepts a `CONNECT` for
/// `remote_host:remote_port`, then close the throwaway connection.
///
/// [`HttpProxyTunnel::start`] is lazy — it only binds the local listener and
/// spawns the accept loop — so on its own it reports success even for an
/// unreachable proxy. This is the step that actually establishes the upstream
/// leg, and it is what the standalone tunnel connectivity probe uses.
///
/// Every stage is bounded by `connect_timeout_secs`: dial, TLS handshake and the
/// CONNECT handshake itself ([`perform_connect_within`]). A proxy that accepts
/// TCP but never answers must yield `Err` instead of hanging the caller forever.
pub(crate) async fn verify_upstream(
    proxy: &HttpProxyTunnelConfig,
    remote_host: &str,
    remote_port: u16,
) -> Result<(), DriverError> {
    let scheme = normalize_scheme(proxy)?;
    let timeout = connect_timeout(proxy);
    let extra_headers = proxy_headers(proxy);
    let auth_header = resolve_auth_header(proxy, &extra_headers);

    let socket = dial_proxy(&proxy.host, proxy.port, timeout).await?;
    if scheme == "https" {
        let mut upstream = tls_connect(&proxy.host, socket, timeout).await?;
        perform_connect_within(
            &mut upstream,
            remote_host,
            remote_port,
            auth_header.as_deref(),
            &extra_headers,
            timeout,
        )
        .await
    } else {
        let mut upstream = socket;
        perform_connect_within(
            &mut upstream,
            remote_host,
            remote_port,
            auth_header.as_deref(),
            &extra_headers,
            timeout,
        )
        .await
    }
}

async fn connect_and_copy(
    inbound: &mut TcpStream,
    scheme: &str,
    proxy_host: &str,
    proxy_port: u16,
    remote_host: &str,
    remote_port: u16,
    auth_header: Option<&str>,
    extra_headers: &[(String, String)],
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<(), DriverError> {
    let socket = dial_proxy(proxy_host, proxy_port, timeout).await?;

    if scheme == "https" {
        let mut upstream = tls_connect(proxy_host, socket, timeout).await?;
        establish_and_copy(
            inbound,
            &mut upstream,
            remote_host,
            remote_port,
            auth_header,
            extra_headers,
            timeout,
            cancel,
        )
        .await
    } else {
        let mut upstream = socket;
        establish_and_copy(
            inbound,
            &mut upstream,
            remote_host,
            remote_port,
            auth_header,
            extra_headers,
            timeout,
            cancel,
        )
        .await
    }
}

async fn establish_and_copy<S>(
    inbound: &mut TcpStream,
    upstream: &mut S,
    remote_host: &str,
    remote_port: u16,
    auth_header: Option<&str>,
    extra_headers: &[(String, String)],
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<(), DriverError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    perform_connect_within(
        upstream,
        remote_host,
        remote_port,
        auth_header,
        extra_headers,
        timeout,
    )
    .await?;

    tokio::select! {
        _ = cancel.cancelled() => Ok(()),
        result = tokio::io::copy_bidirectional(inbound, upstream) => {
            result.map(|_| ()).map_err(|e| DriverError::HttpProxyTunnelError(format!("proxy stream failed: {e}")))
        }
    }
}

fn basic_auth_header(user: Option<&str>, pass: Option<&str>) -> Option<String> {
    let user = user.filter(|s| !s.is_empty())?;
    let pass = pass.unwrap_or("");
    let token = base64_encode(format!("{user}:{pass}").as_bytes());
    Some(format!("Basic {token}"))
}

fn base64_encode(bytes: &[u8]) -> String {
    // Minimal Base64 without new crate dependency (std only).
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let mut buf = [0u8; 3];
        for (i, b) in chunk.iter().enumerate() {
            buf[i] = *b;
        }
        let n = chunk.len();
        let b0 = buf[0] as u32;
        let b1 = buf[1] as u32;
        let b2 = buf[2] as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((triple >> 18) & 0x3f) as usize]);
        out.push(T[((triple >> 12) & 0x3f) as usize]);
        out.push(if n > 1 {
            T[((triple >> 6) & 0x3f) as usize]
        } else {
            b'='
        });
        out.push(if n > 2 {
            T[(triple & 0x3f) as usize]
        } else {
            b'='
        });
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Send the `CONNECT` request and read its status line, bounded by `timeout`.
///
/// [`perform_connect`] has no internal deadline — it reads the status line byte
/// by byte — so **every** caller must bound it. A proxy that accepts the TCP
/// connection but never answers (a half-dead squid, a port forwarded to a dead
/// backend, a SYN-only firewall) would otherwise hang the caller forever; for
/// the standalone probe that means an IPC command whose future never settles.
///
/// This is the single place where the handshake is bounded, so the data path
/// ([`establish_and_copy`]) and the probe ([`verify_upstream`]) provably share
/// the same timeout semantics.
async fn perform_connect_within<S>(
    stream: &mut S,
    remote_host: &str,
    remote_port: u16,
    auth_header: Option<&str>,
    extra_headers: &[(String, String)],
    timeout: Duration,
) -> Result<(), DriverError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    tokio::time::timeout(
        timeout,
        perform_connect(stream, remote_host, remote_port, auth_header, extra_headers),
    )
    .await
    .map_err(|_| DriverError::HttpProxyTunnelError("CONNECT handshake timed out".into()))?
}

async fn perform_connect<S>(
    stream: &mut S,
    remote_host: &str,
    remote_port: u16,
    auth_header: Option<&str>,
    extra_headers: &[(String, String)],
) -> Result<(), DriverError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut req = format!(
        "CONNECT {remote_host}:{remote_port} HTTP/1.1\r\nHost: {remote_host}:{remote_port}\r\n"
    );
    if let Some(auth) = auth_header {
        req.push_str("Proxy-Authorization: ");
        req.push_str(auth);
        req.push_str("\r\n");
    }
    for (k, v) in extra_headers {
        // These headers are controlled by the tunnel implementation.
        if k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case("proxy-authorization") {
            continue;
        }
        if k.contains(['\r', '\n']) || v.contains(['\r', '\n']) {
            return Err(DriverError::HttpProxyTunnelError(
                "proxy header contains an invalid line break".into(),
            ));
        }
        req.push_str(k);
        req.push_str(": ");
        req.push_str(v);
        req.push_str("\r\n");
    }
    req.push_str("Proxy-Connection: Keep-Alive\r\n");
    req.push_str("Connection: Keep-Alive\r\n\r\n");

    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| DriverError::HttpProxyTunnelError(format!("write CONNECT: {e}")))?;

    let mut response = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while response.len() < 16 * 1024 {
        let read = stream.read(&mut byte).await.map_err(|e| {
            DriverError::HttpProxyTunnelError(format!("read CONNECT response: {e}"))
        })?;
        if read == 0 {
            break;
        }
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") || response.ends_with(b"\n\n") {
            break;
        }
    }
    let response_text = String::from_utf8_lossy(&response);
    let status_line = response_text.lines().next().unwrap_or_default();

    let code = parse_http_status(status_line).ok_or_else(|| {
        DriverError::HttpProxyTunnelError(format!(
            "invalid CONNECT response status line: {}",
            status_line.trim()
        ))
    })?;

    if code != 200 {
        return Err(DriverError::HttpProxyTunnelError(format!(
            "proxy CONNECT rejected with status {code}: {}",
            status_line.trim()
        )));
    }
    Ok(())
}

fn parse_http_status(line: &str) -> Option<u16> {
    // HTTP/1.1 200 Connection established
    let mut parts = line.split_whitespace();
    let _version = parts.next()?;
    parts.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_200() {
        assert_eq!(
            parse_http_status("HTTP/1.1 200 Connection established\r\n"),
            Some(200)
        );
        assert_eq!(parse_http_status("HTTP/1.0 403 Forbidden\r\n"), Some(403));
    }

    #[test]
    fn basic_auth_encodes() {
        let h = basic_auth_header(Some("user"), Some("pass")).expect("auth");
        assert!(h.starts_with("Basic "));
        // user:pass -> dXNlcjpwYXNz
        assert_eq!(h, "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn base64_padding() {
        assert_eq!(base64_encode(b"a"), "YQ==");
        assert_eq!(base64_encode(b"ab"), "YWI=");
        assert_eq!(base64_encode(b"abc"), "YWJj");
    }

    #[tokio::test]
    async fn forwards_bytes_through_connect_proxy() {
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy.local_addr().unwrap();
        let proxy_task = tokio::spawn(async move {
            let (mut client, _) = proxy.accept().await.unwrap();
            let mut request = Vec::new();
            let mut byte = [0u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                client.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("CONNECT db.internal:5432 HTTP/1.1"));
            assert!(request.contains("Proxy-Authorization: Basic dTpw"));
            client
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await
                .unwrap();
            let mut payload = [0u8; 4];
            client.read_exact(&mut payload).await.unwrap();
            assert_eq!(&payload, b"ping");
            client.write_all(b"pong").await.unwrap();
        });

        let config = HttpProxyTunnelConfig {
            enabled: true,
            host: proxy_addr.ip().to_string(),
            port: proxy_addr.port(),
            scheme: "http".into(),
            username: Some("u".into()),
            password: Some("p".into()),
            headers: None,
            connect_timeout_secs: 3,
        };
        let tunnel = HttpProxyTunnel::start(&config, "db.internal", 5432)
            .await
            .unwrap();
        let mut local = TcpStream::connect(("127.0.0.1", tunnel.local_port()))
            .await
            .unwrap();
        local.write_all(b"ping").await.unwrap();
        let mut response = [0u8; 4];
        local.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"pong");
        proxy_task.await.unwrap();
    }
    /// [tester] (c) — the refactor extracted these helpers out of the
    /// production data path. Pin each one's semantics directly, including the
    /// `connect_timeout_secs.max(1)` floor that no end-to-end test reaches.
    #[test]
    fn test_tester_extracted_helpers_keep_their_semantics() {
        let base = HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy.internal".into(),
            port: 3128,
            scheme: "HTTP".into(),
            username: None,
            password: None,
            headers: None,
            connect_timeout_secs: 0,
        };

        assert_eq!(normalize_scheme(&base).expect("scheme"), "http");
        assert_eq!(
            connect_timeout(&base),
            Duration::from_secs(1),
            "a zero timeout must fall back to 1s"
        );

        let mut configured = base.clone();
        configured.connect_timeout_secs = 7;
        assert_eq!(connect_timeout(&configured), Duration::from_secs(7));

        assert!(normalize_scheme(&configured).is_ok());

        let mut bad_scheme = base.clone();
        bad_scheme.scheme = "socks5".into();
        assert!(normalize_scheme(&bad_scheme)
            .expect_err("socks5 must be rejected")
            .to_string()
            .contains("unsupported HTTP proxy scheme"));

        let mut empty_host = base.clone();
        empty_host.host = "   ".into();
        assert!(normalize_scheme(&empty_host)
            .expect_err("blank host must be rejected")
            .to_string()
            .contains("host is empty"));

        assert!(proxy_headers(&base).is_empty());
        let mut with_headers = base.clone();
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Corp".to_string(), "1".to_string());
        with_headers.headers = Some(headers);
        assert_eq!(
            proxy_headers(&with_headers),
            vec![("X-Corp".to_string(), "1".to_string())]
        );

        // Explicit header wins over derived Basic credentials.
        let mut both = with_headers.clone();
        both.username = Some("u".into());
        both.password = Some("p".into());
        let mut explicit = std::collections::HashMap::new();
        explicit.insert(
            "proxy-authorization".to_string(),
            "Bearer explicit".to_string(),
        );
        both.headers = Some(explicit);
        let resolved = resolve_auth_header(&both, &proxy_headers(&both)).expect("explicit auth");
        assert_eq!(resolved, "Bearer explicit");

        // Basic credentials are derived when no header is present.
        let derived = resolve_auth_header(&with_headers_with_credentials(), &[]).expect("basic");
        assert_eq!(derived, "Basic dTpw");

        // No credentials and no header -> no auth header.
        assert!(resolve_auth_header(&base, &[]).is_none());
    }

    fn with_headers_with_credentials() -> HttpProxyTunnelConfig {
        HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy.internal".into(),
            port: 3128,
            scheme: "http".into(),
            username: Some("u".into()),
            password: Some("p".into()),
            headers: None,
            connect_timeout_secs: 3,
        }
    }

    /// `tunnel-backend-BUG-004` evidence ②: the **data path** must surface a TLS
    /// failure as `Err` instead of panicking.
    ///
    /// `connect_and_copy` is the exact function the accept loop's per-connection
    /// task runs, so before the fix `ClientConfig::builder()` panicked inside
    /// that spawned task: the forwarder died silently and the tunnel still looked
    /// alive. A panic here would fail this test instead of being swallowed by a
    /// detached `JoinHandle`.
    #[tokio::test]
    async fn https_data_path_reports_tls_failure_without_panicking() {
        // A plain TCP "https proxy": it accepts, then never speaks TLS.
        let proxy = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fixture");
        let proxy_port = proxy.local_addr().expect("fixture addr").port();
        let proxy_task = tokio::spawn(async move {
            let (stream, _) = proxy.accept().await.expect("accept");
            // Hold the connection open without answering the ClientHello.
            tokio::time::sleep(Duration::from_secs(30)).await;
            drop(stream);
        });

        // The inbound (client) half of the local listener. `connect_and_copy`
        // only touches it after the CONNECT succeeds, so any live stream works.
        let inbound_listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind inbound");
        let inbound_addr = inbound_listener.local_addr().expect("inbound addr");
        let (connected, accepted) =
            tokio::join!(TcpStream::connect(inbound_addr), inbound_listener.accept());
        let mut inbound = connected.expect("connect inbound");
        let _server_side = accepted.expect("accept inbound");

        let cancel = CancellationToken::new();
        let outcome = tokio::time::timeout(
            Duration::from_secs(10),
            connect_and_copy(
                &mut inbound,
                "https",
                "127.0.0.1",
                proxy_port,
                "db.internal",
                5432,
                None,
                &[],
                Duration::from_secs(1),
                &cancel,
            ),
        )
        .await
        .expect("the https data path must be bounded, not hang");

        assert!(
            outcome.is_err(),
            "a non-TLS endpoint under scheme=https must fail, got {outcome:?}"
        );

        proxy_task.abort();
    }

    /// [tester] BUG-003 (a)2/(a)3 — `perform_connect_within` is the single
    /// bounded entry that **both** `verify_upstream` branches (`http` and
    /// `https`) and the data path call for the CONNECT read. A peer that accepts
    /// the TCP connection and then never answers (the "SYN-only firewall" /
    /// half-dead-squid shape) must yield `Err("CONNECT handshake timed out")`
    /// inside the budget instead of pending forever.
    ///
    /// This is the strongest hermetic proof available for the `https` branch's
    /// CONNECT stage: the branch reaches this exact helper, but a `https` fixture
    /// cannot get past `tls_connect` without a certificate that chains to a
    /// `webpki-roots` CA.
    #[tokio::test]
    async fn test_tester_connect_read_is_bounded_when_the_peer_never_answers() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind silent peer");
        let addr = listener.local_addr().expect("fixture addr");
        let fixture = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            // Never write a byte, never close.
            tokio::time::sleep(Duration::from_secs(60)).await;
            drop(stream);
        });

        let mut stream = TcpStream::connect(addr).await.expect("connect silent peer");
        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_secs(10),
            perform_connect_within(
                &mut stream,
                "db.internal",
                5432,
                None,
                &[],
                Duration::from_millis(700),
            ),
        )
        .await
        .expect("the CONNECT read must be bounded, not hang");
        let elapsed = started.elapsed();

        let err = outcome.expect_err("a silent peer must fail the CONNECT handshake");
        assert!(
            err.to_string().contains("timed out"),
            "the timeout must be identifiable in the error: {err}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "the CONNECT read must stop at the deadline, took {elapsed:?}"
        );

        fixture.abort();
    }

    /// [tester] BUG-003 (a)3 — characterisation guard for the root cause:
    /// `perform_connect` itself has **no** internal deadline, so the
    /// `perform_connect_within` wrapper is load-bearing.
    ///
    /// If this ever starts returning inside the window, the inner function grew
    /// its own deadline and the wrapper's rationale (and the doc comment on it)
    /// must be revisited.
    #[tokio::test]
    async fn test_tester_raw_perform_connect_is_unbounded_so_the_wrapper_is_load_bearing() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind silent peer");
        let addr = listener.local_addr().expect("fixture addr");
        let fixture = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            tokio::time::sleep(Duration::from_secs(60)).await;
            drop(stream);
        });

        let mut stream = TcpStream::connect(addr).await.expect("connect silent peer");
        let outcome = tokio::time::timeout(
            Duration::from_millis(500),
            perform_connect(&mut stream, "db.internal", 5432, None, &[]),
        )
        .await;
        assert!(
            outcome.is_err(),
            "perform_connect must have no internal deadline (that is why \
             perform_connect_within exists), but it returned: {outcome:?}"
        );

        fixture.abort();
    }
}
