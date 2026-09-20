//! HTTP CONNECT tunnel: local TCP listener → HTTP(S) proxy CONNECT → remote DB.

use crate::db::{DriverError, HttpProxyTunnelConfig};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

pub struct HttpProxyTunnel {
    local_port: u16,
    _task: tokio::task::JoinHandle<()>,
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
        if scheme != "http" {
            // TLS-to-proxy (https) lands in a follow-up; keep surface stable.
            return Err(DriverError::HttpProxyTunnelError(format!(
                "HTTP proxy scheme '{scheme}' is not supported yet; use scheme=http (CONNECT). HTTPS-to-proxy is planned"
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
        let auth_header = basic_auth_header(proxy.username.as_deref(), proxy.password.as_deref());
        let extra_headers: Vec<(String, String)> = proxy
            .headers
            .as_ref()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        tracing::info!(
            proxy = %format!("{proxy_host}:{proxy_port}"),
            local_port,
            remote = %format!("{remote_host}:{remote_port}"),
            "HTTP CONNECT tunnel established (listener ready)"
        );

        let task = tokio::spawn(async move {
            loop {
                let (mut inbound, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("HTTP proxy tunnel accept error: {e}");
                        break;
                    }
                };

                let proxy_host = proxy_host.clone();
                let remote_host = remote_host.clone();
                let auth_header = auth_header.clone();
                let extra_headers = extra_headers.clone();

                tokio::spawn(async move {
                    let result = async {
                        let mut upstream = tokio::time::timeout(
                            timeout,
                            TcpStream::connect((proxy_host.as_str(), proxy_port)),
                        )
                        .await
                        .map_err(|_| {
                            DriverError::HttpProxyTunnelError("connect to proxy timed out".into())
                        })?
                        .map_err(|e| {
                            DriverError::HttpProxyTunnelError(format!(
                                "connect to proxy {proxy_host}:{proxy_port}: {e}"
                            ))
                        })?;

                        perform_connect(
                            &mut upstream,
                            &remote_host,
                            remote_port,
                            auth_header.as_deref(),
                            &extra_headers,
                        )
                        .await?;

                        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut upstream).await;
                        Ok::<(), DriverError>(())
                    }
                    .await;

                    if let Err(e) = result {
                        tracing::error!(error = %e, "HTTP CONNECT tunnel stream failed");
                    }
                });
            }
        });

        // Silence unused warning if Arc not needed — keep JoinHandle owned.
        let _ = Arc::new(());

        Ok(Self {
            local_port,
            _task: task,
        })
    }
}

fn basic_auth_header(user: Option<&str>, pass: Option<&str>) -> Option<String> {
    let user = user.filter(|s| !s.is_empty())?;
    let pass = pass.unwrap_or("");
    let token = base64_encode(format!("{user}:{pass}").as_bytes());
    Some(format!("Basic {token}"))
}

fn base64_encode(bytes: &[u8]) -> String {
    use std::io::Write;
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

async fn perform_connect(
    stream: &mut TcpStream,
    remote_host: &str,
    remote_port: u16,
    auth_header: Option<&str>,
    extra_headers: &[(String, String)],
) -> Result<(), DriverError> {
    let mut req = format!(
        "CONNECT {remote_host}:{remote_port} HTTP/1.1\r\nHost: {remote_host}:{remote_port}\r\n"
    );
    if let Some(auth) = auth_header {
        req.push_str("Proxy-Authorization: ");
        req.push_str(auth);
        req.push_str("\r\n");
    }
    for (k, v) in extra_headers {
        // Skip hop-by-hop / dangerous overrides.
        if k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case("proxy-authorization") {
            continue;
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

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .await
        .map_err(|e| DriverError::HttpProxyTunnelError(format!("read CONNECT status: {e}")))?;

    let code = parse_http_status(&status_line).ok_or_else(|| {
        DriverError::HttpProxyTunnelError(format!(
            "invalid CONNECT response status line: {}",
            status_line.trim()
        ))
    })?;

    // Drain headers until empty line.
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(|e| DriverError::HttpProxyTunnelError(format!("read CONNECT header: {e}")))?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            break;
        }
    }

    if code != 200 {
        return Err(DriverError::HttpProxyTunnelError(format!(
            "proxy CONNECT rejected with status {code}: {}",
            status_line.trim()
        )));
    }

    // BufReader may have buffered bytes; into_inner returns the stream.
    // Any prefetched body bytes would be a proxy bug; we ignore for CONNECT 200.
    let _ = reader;
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
}
