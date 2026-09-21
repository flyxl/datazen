//! HTTP CONNECT tunnel: local TCP listener → HTTP(S) proxy CONNECT → remote DB.

use crate::db::{DriverError, HttpProxyTunnelConfig};
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
        let timeout = Duration::from_secs(u64::from(proxy.connect_timeout_secs.max(1)));
        let extra_headers: Vec<(String, String)> = proxy
            .headers
            .as_ref()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        let auth_header = extra_headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("proxy-authorization"))
            .map(|(_, value)| value.clone())
            .or_else(|| basic_auth_header(proxy.username.as_deref(), proxy.password.as_deref()));
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
    let socket = tokio::time::timeout(timeout, TcpStream::connect((proxy_host, proxy_port)))
        .await
        .map_err(|_| DriverError::HttpProxyTunnelError("connect to proxy timed out".into()))?
        .map_err(|e| {
            DriverError::HttpProxyTunnelError(format!(
                "connect to proxy {proxy_host}:{proxy_port}: {e}"
            ))
        })?;

    if scheme == "https" {
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
        let mut upstream = tokio::time::timeout(timeout, connector.connect(server_name, socket))
            .await
            .map_err(|_| DriverError::HttpProxyTunnelError("TLS proxy handshake timed out".into()))?
            .map_err(|e| {
                DriverError::HttpProxyTunnelError(format!("TLS proxy handshake failed: {e}"))
            })?;
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
    tokio::time::timeout(
        timeout,
        perform_connect(
            upstream,
            remote_host,
            remote_port,
            auth_header,
            extra_headers,
        ),
    )
    .await
    .map_err(|_| DriverError::HttpProxyTunnelError("CONNECT handshake timed out".into()))??;

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
}
