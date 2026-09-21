//! WebSocket tunnel: local TCP listener -> WS/WSS relay -> remote DB.
//! Modes: `datazen_v1` (JSON open/opened + binary) | `raw_binary` (immediate binary).

use crate::db::{DriverError, WebSocketTunnelConfig};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{HeaderName, HeaderValue};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug)]
pub struct WebSocketTunnel {
    local_port: u16,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl WebSocketTunnel {
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    pub async fn start(
        cfg: &WebSocketTunnelConfig,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<Self, DriverError> {
        if cfg.url.trim().is_empty() {
            return Err(DriverError::WebSocketTunnelError(
                "WebSocket tunnel URL is empty".into(),
            ));
        }
        let mode = cfg.mode.to_ascii_lowercase();
        if mode != "datazen_v1" && mode != "raw_binary" {
            return Err(DriverError::WebSocketTunnelError(format!(
                "Unknown WebSocket tunnel mode '{mode}'; use datazen_v1 or raw_binary"
            )));
        }

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| DriverError::WebSocketTunnelError(format!("Bind local port: {e}")))?;
        let local_port = listener
            .local_addr()
            .map_err(|e| DriverError::WebSocketTunnelError(format!("get local port: {e}")))?
            .port();

        let url = if mode == "raw_binary" {
            raw_binary_url(&cfg.url, &remote_host, remote_port)?
        } else {
            cfg.url.clone()
        };
        let auth_token = cfg.auth_token.clone();
        let extra_headers: Vec<(String, String)> = cfg
            .headers
            .as_ref()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        let timeout = Duration::from_secs(u64::from(cfg.connect_timeout_secs.max(1)));
        let ping_interval = if cfg.ping_interval_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(u64::from(cfg.ping_interval_secs)))
        };
        let remote_host = remote_host.to_string();
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();

        tracing::info!(
            url = %url,
            local_port,
            remote = %format!("{remote_host}:{remote_port}"),
            mode = %mode,
            "WebSocket tunnel listener ready"
        );

        let task = tokio::spawn(async move {
            loop {
                let (inbound, _) = tokio::select! {
                    _ = task_cancel.cancelled() => break,
                    result = listener.accept() => match result {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!("WebSocket tunnel accept error: {e}");
                            break;
                        }
                    },
                };
                let url = url.clone();
                let auth_token = auth_token.clone();
                let extra_headers = extra_headers.clone();
                let remote_host = remote_host.clone();
                let mode = mode.clone();
                let child_cancel = task_cancel.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(
                        inbound,
                        &url,
                        auth_token.as_deref(),
                        &extra_headers,
                        &remote_host,
                        remote_port,
                        &mode,
                        timeout,
                        ping_interval,
                        &child_cancel,
                    )
                    .await
                    {
                        tracing::error!(error = %e, "WebSocket tunnel stream failed");
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

impl Drop for WebSocketTunnel {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.task.abort();
    }
}

fn raw_binary_url(
    raw_url: &str,
    remote_host: &str,
    remote_port: u16,
) -> Result<String, DriverError> {
    let mut url = url::Url::parse(raw_url)
        .map_err(|e| DriverError::WebSocketTunnelError(format!("invalid WebSocket URL: {e}")))?;
    let pairs = url
        .query_pairs()
        .filter(|(key, _)| key != "host" && key != "port")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    {
        let mut query = url.query_pairs_mut();
        query.clear();
        for (key, value) in pairs {
            query.append_pair(&key, &value);
        }
        query
            .append_pair("host", remote_host)
            .append_pair("port", &remote_port.to_string());
    }
    Ok(url.into())
}

async fn handle_client(
    mut inbound: TcpStream,
    url: &str,
    auth_token: Option<&str>,
    extra_headers: &[(String, String)],
    remote_host: &str,
    remote_port: u16,
    mode: &str,
    timeout: Duration,
    ping_interval: Option<Duration>,
    cancel: &CancellationToken,
) -> Result<(), DriverError> {
    let mut ws = connect_ws(url, auth_token, extra_headers, timeout).await?;
    let channel_id = if mode == "datazen_v1" {
        Some(open_datazen_channel(&mut ws, remote_host, remote_port, timeout).await?)
    } else {
        None
    };
    pipe_tcp_ws(
        &mut inbound,
        &mut ws,
        ping_interval,
        channel_id.as_deref(),
        cancel,
    )
    .await;
    Ok(())
}

async fn connect_ws(
    url: &str,
    auth_token: Option<&str>,
    extra_headers: &[(String, String)],
    timeout: Duration,
) -> Result<WsStream, DriverError> {
    let mut request = url
        .into_client_request()
        .map_err(|e| DriverError::WebSocketTunnelError(format!("invalid WebSocket URL: {e}")))?;

    if let Some(token) = auth_token.filter(|s| !s.is_empty()) {
        let value = HeaderValue::from_str(&format!("Bearer {token}")).map_err(|e| {
            DriverError::WebSocketTunnelError(format!("invalid auth token header: {e}"))
        })?;
        request.headers_mut().insert("Authorization", value);
    }
    for (k, v) in extra_headers {
        if k.eq_ignore_ascii_case("authorization") || k.eq_ignore_ascii_case("host") {
            continue;
        }
        let name: HeaderName = k.parse().map_err(|e| {
            DriverError::WebSocketTunnelError(format!("invalid header name {k}: {e}"))
        })?;
        let value = HeaderValue::from_str(v).map_err(|e| {
            DriverError::WebSocketTunnelError(format!("invalid header value for {k}: {e}"))
        })?;
        request.headers_mut().insert(name, value);
    }

    let (ws, _resp) = tokio::time::timeout(timeout, connect_async(request))
        .await
        .map_err(|_| DriverError::WebSocketTunnelError("WebSocket connect timed out".into()))?
        .map_err(|e| DriverError::WebSocketTunnelError(format!("WebSocket connect failed: {e}")))?;
    Ok(ws)
}

async fn open_datazen_channel(
    ws: &mut WsStream,
    remote_host: &str,
    remote_port: u16,
    timeout: Duration,
) -> Result<String, DriverError> {
    let id = uuid::Uuid::new_v4().to_string();
    let open = serde_json::json!({
        "op": "open",
        "host": remote_host,
        "port": remote_port,
        "id": id,
    });
    ws.send(Message::Text(open.to_string().into()))
        .await
        .map_err(|e| DriverError::WebSocketTunnelError(format!("send open: {e}")))?;

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(DriverError::WebSocketTunnelError(
                "timed out waiting for WebSocket open ack".into(),
            ));
        }
        let msg = tokio::time::timeout(remaining, ws.next())
            .await
            .map_err(|_| {
                DriverError::WebSocketTunnelError("timed out waiting for WebSocket open ack".into())
            })?
            .ok_or_else(|| {
                DriverError::WebSocketTunnelError("WebSocket closed before open ack".into())
            })?
            .map_err(|e| DriverError::WebSocketTunnelError(format!("read open ack: {e}")))?;

        match msg {
            Message::Text(text) => {
                let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                    DriverError::WebSocketTunnelError(format!("invalid open ack JSON: {e}"))
                })?;
                match v.get("op").and_then(|x| x.as_str()).unwrap_or("") {
                    "opened" => return Ok(id),
                    "error" => {
                        let detail = v
                            .get("message")
                            .and_then(|x| x.as_str())
                            .unwrap_or("relay error");
                        return Err(DriverError::WebSocketTunnelError(format!(
                            "relay rejected open: {detail}"
                        )));
                    }
                    _ => continue,
                }
            }
            Message::Ping(data) => {
                let _ = ws.send(Message::Pong(data)).await;
            }
            Message::Close(_) => {
                return Err(DriverError::WebSocketTunnelError(
                    "WebSocket closed before open ack".into(),
                ));
            }
            _ => continue,
        }
    }
}

async fn pipe_tcp_ws(
    inbound: &mut TcpStream,
    ws: &mut WsStream,
    ping_interval: Option<Duration>,
    channel_id: Option<&str>,
    cancel: &CancellationToken,
) {
    let mut buf = vec![0u8; 32 * 1024];
    let mut ping = ping_interval.map(tokio::time::interval);
    loop {
        tokio::select! {
            read = inbound.read(&mut buf) => {
                match read {
                    Ok(0) => {
                        if let Some(id) = channel_id {
                            let close = serde_json::json!({"op": "close", "id": id});
                            let _ = ws.send(Message::Text(close.to_string().into())).await;
                        }
                        let _ = ws.send(Message::Close(None)).await;
                        break;
                    }
                    Ok(n) => {
                        if ws.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() { break; }
                    }
                    Err(_) => break,
                }
            }
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        if inbound.write_all(&data).await.is_err() { break; }
                    }
                    Some(Ok(Message::Ping(data))) => { let _ = ws.send(Message::Pong(data)).await; }
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Text(_))) | Some(Ok(Message::Frame(_))) => {}
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                }
            }
            _ = async {
                if let Some(ref mut interval) = ping {
                    interval.tick().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                if ws.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
            }
            _ = cancel.cancelled() => {
                if let Some(id) = channel_id {
                    let close = serde_json::json!({"op": "close", "id": id});
                    let _ = ws.send(Message::Text(close.to_string().into())).await;
                }
                let _ = ws.send(Message::Close(None)).await;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::accept_async;

    #[test]
    fn rejects_empty_url() {
        let cfg = WebSocketTunnelConfig {
            enabled: true,
            url: "  ".into(),
            auth_token: None,
            headers: None,
            connect_timeout_secs: 5,
            ping_interval_secs: 0,
            mode: "datazen_v1".into(),
        };
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(WebSocketTunnel::start(&cfg, "db", 5432))
            .unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[tokio::test]
    async fn forwards_datazen_v1_bytes_and_sends_close_control() {
        let relay = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = relay.local_addr().unwrap();
        let relay_task = tokio::spawn(async move {
            let (stream, _) = relay.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let open = ws.next().await.unwrap().unwrap();
            let id = match open {
                Message::Text(text) => {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(value["op"], "open");
                    assert_eq!(value["host"], "db.internal");
                    assert_eq!(value["port"], 5432);
                    value["id"].as_str().unwrap().to_string()
                }
                other => panic!("expected open control frame, got {other:?}"),
            };
            ws.send(Message::Text(
                serde_json::json!({"op": "opened", "id": id})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            assert_eq!(
                ws.next().await.unwrap().unwrap(),
                Message::Binary(b"ping".to_vec().into())
            );
            ws.send(Message::Binary(b"pong".to_vec().into()))
                .await
                .unwrap();
            match ws.next().await.unwrap().unwrap() {
                Message::Text(text) => {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(value["op"], "close");
                    assert_eq!(value["id"], id);
                }
                other => panic!("expected close control frame, got {other:?}"),
            }
        });

        let config = WebSocketTunnelConfig {
            enabled: true,
            url: format!("ws://{relay_addr}/tunnel"),
            auth_token: None,
            headers: None,
            connect_timeout_secs: 3,
            ping_interval_secs: 0,
            mode: "datazen_v1".into(),
        };
        let tunnel = WebSocketTunnel::start(&config, "db.internal", 5432)
            .await
            .unwrap();
        let mut local = TcpStream::connect(("127.0.0.1", tunnel.local_port()))
            .await
            .unwrap();
        local.write_all(b"ping").await.unwrap();
        let mut response = [0u8; 4];
        local.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"pong");
        local.shutdown().await.unwrap();
        relay_task.await.unwrap();
    }

    #[test]
    fn raw_binary_url_injects_target_without_duplicate_keys() {
        let url =
            raw_binary_url("ws://relay/tunnel?token=x&host=old", "db.internal", 5432).unwrap();
        assert_eq!(url, "ws://relay/tunnel?token=x&host=db.internal&port=5432");
    }
}
