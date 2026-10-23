//! Tunnel entity IPC (independent of connection configs).

use super::error::{CmdExt, CommandError};
use super::AppState;
use datazen_driver_api::{SavedTunnel, SavedTunnelSummary, TunnelUsage};
use datazen_platform_api::error::PortError;
use tauri::State;

/// 端口失败 → 命令错误：**原文保留**。
///
/// 桌面实现的失败只有一个来源（存储层），端口把它压成
/// `PortError::BackendUnavailable(<原文>)`；这里把原文原样还回去，因此 IPC 载荷
/// 与端口化之前**逐字相同**（此前这里是 `DesktopRepositoryError`，其 `Display` 已被端口原样带走）。
fn tunnel_port_error(error: PortError) -> CommandError {
    match error {
        PortError::BackendUnavailable(detail) => CommandError::Internal(detail),
        PortError::NotFound(entity) => CommandError::NotFound(entity),
        other => CommandError::Internal(other.to_string()),
    }
}

/// 端口调用 → 命令结果：映射之外**照旧记一条 error 日志**（与 `cmd_err` 同形）。
fn tunnel_cmd<T>(result: Result<T, PortError>, cmd: &str) -> Result<T, CommandError> {
    result.map_err(|error| {
        let err = tunnel_port_error(error);
        let redacted = datazen_platform_infra::redaction::redact_secrets_for_log(&err.to_string());
        tracing::error!(cmd, error = %redacted);
        err
    })
}

pub(crate) async fn get_tunnels_impl(state: &AppState) -> Result<Vec<SavedTunnel>, CommandError> {
    // 端口只有两档：名单（摘要，**不带密钥**）与单条取用（全量）。这个 IPC 的载荷是
    // **全量**且形状冻结，因此由两档组合而成：先取名单，再逐条取用。
    //
    // 生产代码没有它的调用方（所有列表路径都走 `get_tunnel_summaries`），但命令的
    // 形状与载荷不变。
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(state)?;
    let summaries = tunnel_cmd(services.tunnels.list(&scope).await, "get_tunnels")?;
    let mut tunnels = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let Some(tunnel) = tunnel_cmd(
            services.tunnels.get(&scope, &summary.id).await,
            "get_tunnels",
        )?
        else {
            continue;
        };
        tunnels.push(tunnel);
    }
    Ok(tunnels)
}

pub(crate) async fn get_tunnel_impl(
    state: &AppState,
    id: String,
) -> Result<Option<SavedTunnel>, CommandError> {
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(state)?;
    tunnel_cmd(services.tunnels.get(&scope, &id).await, "get_tunnel")
}

pub(crate) async fn save_tunnel_impl(
    state: &AppState,
    tunnel: SavedTunnel,
) -> Result<(), CommandError> {
    tracing::info!(id = %tunnel.id, name = %tunnel.name, kind = ?tunnel.kind, "save_tunnel");
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(state)?;
    tunnel_cmd(services.tunnels.save(&scope, tunnel).await, "save_tunnel")
}

pub(crate) async fn delete_tunnel_impl(state: &AppState, id: String) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_tunnel");
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(state)?;
    tunnel_cmd(services.tunnels.delete(&scope, &id).await, "delete_tunnel")
}

/// Metadata-only projection of the stored tunnels for the selector.
///
/// 走端口的**名单档**：它只带 `id` / `name` / `kind`，**不含**任何凭据——
/// 端口这一档刻意不返回密钥（`get_tunnels` 那种全量列表是另一条路径）。
pub(crate) async fn get_tunnel_summaries_impl(
    state: &AppState,
) -> Result<Vec<SavedTunnelSummary>, CommandError> {
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(state)?;
    tunnel_cmd(services.tunnels.list(&scope).await, "get_tunnel_summaries")
}

/// Which connections reference `id`. `connection_ids` and `connection_names`
/// are positionally aligned.
pub(crate) async fn get_tunnel_usage_impl(
    state: &AppState,
    id: String,
) -> Result<TunnelUsage, CommandError> {
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(state)?;
    tunnel_cmd(
        services.tunnels.usage(&scope, &id).await,
        "get_tunnel_usage",
    )
}

/// Probe a saved tunnel against `target_host:target_port`; returns elapsed ms.
pub(crate) async fn test_tunnel_impl(
    state: &AppState,
    id: String,
    target_host: String,
    target_port: u16,
) -> Result<u64, CommandError> {
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(state)?;
    let elapsed = crate::tunnel::test_tunnel(
        &*services.tunnels,
        &scope,
        &services.known_hosts,
        &id,
        &target_host,
        target_port,
    )
    .await
    .cmd_err("test_tunnel")?;
    let elapsed_ms = elapsed.as_millis() as u64;
    tracing::info!(tunnel_id = %id, elapsed_ms, "test_tunnel OK");
    Ok(elapsed_ms)
}

#[tauri::command]
pub async fn get_tunnels(state: State<'_, AppState>) -> Result<Vec<SavedTunnel>, CommandError> {
    get_tunnels_impl(&state).await
}

#[tauri::command]
pub async fn get_tunnel(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<SavedTunnel>, CommandError> {
    get_tunnel_impl(&state, id).await
}

#[tauri::command]
pub async fn save_tunnel(
    state: State<'_, AppState>,
    tunnel: SavedTunnel,
) -> Result<(), CommandError> {
    save_tunnel_impl(&state, tunnel).await
}

#[tauri::command]
pub async fn delete_tunnel(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    delete_tunnel_impl(&state, id).await
}

#[tauri::command]
pub async fn get_tunnel_summaries(
    state: State<'_, AppState>,
) -> Result<Vec<SavedTunnelSummary>, CommandError> {
    get_tunnel_summaries_impl(&state).await
}

#[tauri::command]
pub async fn get_tunnel_usage(
    state: State<'_, AppState>,
    id: String,
) -> Result<TunnelUsage, CommandError> {
    get_tunnel_usage_impl(&state, id).await
}

#[tauri::command]
pub async fn test_tunnel(
    state: State<'_, AppState>,
    id: String,
    target_host: String,
    target_port: u16,
) -> Result<u64, CommandError> {
    test_tunnel_impl(&state, id, target_host, target_port).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tunnel_summary_tests::saved_ssh_tunnel;
    use crate::testing::app_state::TestAppState;
    use datazen_driver_api::{SshTunnelConfig, TunnelKind};

    #[tokio::test]
    async fn test_tunnel_rejects_unknown_tunnel_id() {
        let test = TestAppState::new().await;
        let err = test_tunnel_impl(
            &test.state,
            "missing-tunnel".into(),
            "127.0.0.1".into(),
            5432,
        )
        .await
        .expect_err("an unknown tunnel id must fail the probe");
        assert!(
            err.to_string().contains("missing-tunnel"),
            "error must name the offending tunnel: {err}"
        );
    }

    #[tokio::test]
    async fn test_tunnel_rejects_tunnel_without_a_tunnel_config() {
        let test = TestAppState::new().await;
        test.store
            .save_tunnel(SavedTunnel {
                id: "t-none".into(),
                name: "Disabled".into(),
                kind: TunnelKind::None,
                ssh: None,
                http_proxy: None,
                websocket: None,
            })
            .await
            .unwrap();

        let err = test_tunnel_impl(&test.state, "t-none".into(), "127.0.0.1".into(), 5432)
            .await
            .expect_err("a tunnel with no configuration must not report success");
        assert!(
            err.to_string().contains("resolved to no tunnel"),
            "unexpected error: {err}"
        );
    }

    // ── [tester] independent verification additions ──────────────────

    #[tokio::test]
    async fn test_tester_test_tunnel_rejects_ssh_kind_without_ssh_config() {
        let test = TestAppState::new().await;
        test.store
            .save_tunnel(SavedTunnel {
                id: "t-ssh-broken".into(),
                name: "Broken".into(),
                kind: TunnelKind::Ssh,
                ssh: None,
                http_proxy: None,
                websocket: None,
            })
            .await
            .unwrap();

        let err = test_tunnel_impl(&test.state, "t-ssh-broken".into(), "127.0.0.1".into(), 5432)
            .await
            .expect_err("kind=ssh with no ssh config must not report success");
        assert!(
            err.to_string().contains("sshTunnel is missing or disabled"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn test_tester_test_tunnel_rejects_disabled_ssh_tunnel() {
        let test = TestAppState::new().await;
        let mut tunnel = saved_ssh_tunnel("t-ssh-off", "Disabled");
        if let Some(ssh) = tunnel.ssh.as_mut() {
            ssh.enabled = false;
        }
        test.store.save_tunnel(tunnel).await.unwrap();

        let err = test_tunnel_impl(&test.state, "t-ssh-off".into(), "127.0.0.1".into(), 5432)
            .await
            .expect_err("a disabled ssh tunnel must not report success");
        assert!(
            err.to_string().contains("sshTunnel is missing or disabled"),
            "unexpected error: {err}"
        );
    }

    /// [tester] Success path: the probe must reach `Ok(elapsed_ms)` and tear the
    /// tunnel down without panicking.
    ///
    /// The fixture is a local TCP listener that answers an HTTP `CONNECT` with
    /// `200`, so the assertion also holds if the lazily-started proxy path is
    /// ever made to dial eagerly (see `tunnel-backend-BUG-001`).
    #[tokio::test]
    async fn test_tester_test_tunnel_success_path_with_local_proxy_fixture() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind proxy fixture");
        let proxy_port = listener.local_addr().expect("fixture addr").port();
        let fixture = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    let _ = stream
                        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                        .await;
                    let _ = stream.shutdown().await;
                });
            }
        });

        let test = TestAppState::new().await;
        test.store
            .save_tunnel(SavedTunnel {
                id: "t-proxy".into(),
                name: "Local proxy".into(),
                kind: TunnelKind::HttpProxy,
                ssh: None,
                http_proxy: Some(datazen_driver_api::HttpProxyTunnelConfig {
                    enabled: true,
                    host: "127.0.0.1".into(),
                    port: proxy_port,
                    scheme: "http".into(),
                    username: None,
                    password: None,
                    headers: None,
                    connect_timeout_secs: 5,
                }),
                websocket: None,
            })
            .await
            .unwrap();

        let elapsed_ms = test_tunnel_impl(&test.state, "t-proxy".into(), "127.0.0.1".into(), 5432)
            .await
            .expect("the probe must succeed for a reachable local proxy fixture");
        assert!(
            elapsed_ms < 5_000,
            "probe took suspiciously long: {elapsed_ms}ms"
        );

        fixture.abort();
    }

    // ── tunnel-backend-BUG-001: unreachable proxy / relay must not pass ──

    /// A `127.0.0.1` port the OS just handed out and that nothing listens on.
    async fn closed_local_port() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind probe port");
        let port = listener.local_addr().expect("probe addr").port();
        drop(listener);
        port
    }

    fn saved_proxy_tunnel(id: &str, port: u16) -> SavedTunnel {
        SavedTunnel {
            id: id.into(),
            name: "Proxy".into(),
            kind: TunnelKind::HttpProxy,
            ssh: None,
            http_proxy: Some(datazen_driver_api::HttpProxyTunnelConfig {
                enabled: true,
                host: "127.0.0.1".into(),
                port,
                scheme: "http".into(),
                username: None,
                password: None,
                headers: None,
                connect_timeout_secs: 2,
            }),
            websocket: None,
        }
    }

    fn saved_websocket_tunnel(id: &str, port: u16) -> SavedTunnel {
        SavedTunnel {
            id: id.into(),
            name: "Relay".into(),
            kind: TunnelKind::WebSocket,
            ssh: None,
            http_proxy: None,
            websocket: Some(datazen_driver_api::WebSocketTunnelConfig {
                enabled: true,
                url: format!("ws://127.0.0.1:{port}/tunnel"),
                auth_token: None,
                headers: None,
                connect_timeout_secs: 2,
                ping_interval_secs: 0,
                mode: "datazen_v1".into(),
            }),
        }
    }

    /// BUG-001 regression: `HttpProxyTunnel::start` only binds the local
    /// listener, so the probe used to answer `Ok(0)` for a proxy on a closed
    /// port. It must report `Err`.
    #[tokio::test]
    async fn test_tunnel_rejects_unreachable_http_proxy_endpoint() {
        let test = TestAppState::new().await;
        let port = closed_local_port().await;
        test.store
            .save_tunnel(saved_proxy_tunnel("t-dead-proxy", port))
            .await
            .unwrap();

        let err = test_tunnel_impl(&test.state, "t-dead-proxy".into(), "127.0.0.1".into(), 5432)
            .await
            .expect_err("an unreachable proxy must not be reported as reachable");
        assert!(
            err.to_string().contains("connect to proxy"),
            "unexpected error: {err}"
        );
    }

    /// A reachable proxy that *rejects* the CONNECT must fail too: this proves
    /// the probe performs a real CONNECT handshake, not a bare TCP connect.
    #[tokio::test]
    async fn test_tunnel_rejects_proxy_that_refuses_connect() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind proxy fixture");
        let port = listener.local_addr().expect("fixture addr").port();
        let fixture = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
                    let _ = stream.shutdown().await;
                });
            }
        });

        let test = TestAppState::new().await;
        test.store
            .save_tunnel(saved_proxy_tunnel("t-refusing-proxy", port))
            .await
            .unwrap();

        let err = test_tunnel_impl(
            &test.state,
            "t-refusing-proxy".into(),
            "127.0.0.1".into(),
            5432,
        )
        .await
        .expect_err("a proxy that refuses CONNECT must not be reported as reachable");
        assert!(
            err.to_string().contains("CONNECT rejected"),
            "unexpected error: {err}"
        );

        fixture.abort();
    }

    /// BUG-001 regression: `WebSocketTunnel::start` only binds the local
    /// listener, so the probe used to answer `Ok(0)` for a relay on a closed
    /// port. It must report `Err`.
    #[tokio::test]
    async fn test_tunnel_rejects_unreachable_websocket_endpoint() {
        let test = TestAppState::new().await;
        let port = closed_local_port().await;
        test.store
            .save_tunnel(saved_websocket_tunnel("t-dead-relay", port))
            .await
            .unwrap();

        let err = test_tunnel_impl(&test.state, "t-dead-relay".into(), "127.0.0.1".into(), 5432)
            .await
            .expect_err("an unreachable relay must not be reported as reachable");
        assert!(
            !err.to_string().is_empty(),
            "the probe error must carry a message"
        );
    }

    /// The mirror of the failure tests: a relay that really answers the
    /// `datazen_v1` `open` with `opened` must pass, so the fix cannot be
    /// "always return Err for WebSocket".
    #[tokio::test]
    async fn test_tunnel_probes_websocket_relay_end_to_end() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind relay fixture");
        let port = listener.local_addr().expect("fixture addr").port();
        let fixture = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    if let Some(Ok(Message::Text(text))) = ws.next().await {
                        let open: serde_json::Value =
                            serde_json::from_str(&text).unwrap_or_default();
                        let ack = if open.get("op").and_then(|v| v.as_str()) == Some("open") {
                            r#"{"op":"opened"}"#
                        } else {
                            r#"{"op":"error","message":"unexpected frame"}"#
                        };
                        let _ = ws.send(Message::Text(ack.to_string().into())).await;
                        // Hold the socket until the probe closes it.
                        while let Some(Ok(msg)) = ws.next().await {
                            if msg.is_close() {
                                break;
                            }
                        }
                    }
                });
            }
        });

        let test = TestAppState::new().await;
        test.store
            .save_tunnel(saved_websocket_tunnel("t-live-relay", port))
            .await
            .unwrap();

        let elapsed_ms =
            test_tunnel_impl(&test.state, "t-live-relay".into(), "127.0.0.1".into(), 5432)
                .await
                .expect("a relay that acks the open must pass the probe");
        assert!(
            elapsed_ms < 5_000,
            "probe took suspiciously long: {elapsed_ms}ms"
        );

        fixture.abort();
    }

    /// SSH has always dialed eagerly, so this pins that an unreachable bastion
    /// is reported as unreachable rather than optimistically succeeding.
    #[tokio::test]
    async fn test_tunnel_rejects_unreachable_ssh_bastion() {
        let test = TestAppState::new().await;
        let port = closed_local_port().await;
        test.store
            .save_tunnel(SavedTunnel {
                id: "t-dead-bastion".into(),
                name: "Bastion".into(),
                kind: TunnelKind::Ssh,
                ssh: Some(SshTunnelConfig {
                    enabled: true,
                    host: "127.0.0.1".into(),
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
            })
            .await
            .unwrap();

        let err = test_tunnel_impl(
            &test.state,
            "t-dead-bastion".into(),
            "127.0.0.1".into(),
            5432,
        )
        .await
        .expect_err("an unreachable bastion must not be reported as reachable");
        assert!(
            err.to_string().contains("SSH connect"),
            "unexpected error: {err}"
        );
    }

    // ── tunnel-backend-BUG-003: the CONNECT probe must be bounded ─────

    /// BUG-003 regression: a proxy that **accepts TCP but never answers** the
    /// `CONNECT` request used to hang the probe forever, so the IPC future never
    /// settled and the management UI spun indefinitely.
    ///
    /// `saved_proxy_tunnel` pins `connect_timeout_secs = 2`. The probe is itself
    /// wrapped in a generous test-level timeout so that, if the fix regresses,
    /// this case fails instead of wedging the whole suite.
    #[tokio::test]
    async fn test_tunnel_times_out_against_a_proxy_that_never_answers_connect() {
        use tokio::io::AsyncReadExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind silent proxy");
        let port = listener.local_addr().expect("fixture addr").port();
        let fixture = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    // Consume the CONNECT request, then deliberately never reply.
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    drop(stream);
                });
            }
        });

        let test = TestAppState::new().await;
        test.store
            .save_tunnel(saved_proxy_tunnel("t-silent-proxy", port))
            .await
            .unwrap();

        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            test_tunnel_impl(
                &test.state,
                "t-silent-proxy".into(),
                "127.0.0.1".into(),
                5432,
            ),
        )
        .await;
        let elapsed = started.elapsed();

        let result = outcome.expect(
            "the probe must not hang against a silent proxy: BUG-003 regression \
             (CONNECT handshake not bounded by connect_timeout_secs)",
        );
        let err = result.expect_err("a proxy that never answers CONNECT must fail the probe");
        assert!(
            err.to_string().contains("timed out"),
            "the timeout must be identifiable in the error: {err}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "the probe must be bounded by connect_timeout_secs (2s), took {elapsed:?}"
        );

        fixture.abort();
    }

    /// [tester] BUG-003 (a)1 — the **`https` branch** of `verify_upstream` must
    /// be bounded exactly like the plaintext one.
    ///
    /// The Coder's guard above only covers the plaintext branch. This is the
    /// encrypted variant: a plain TCP endpoint that accepts but never speaks
    /// TLS. `tls_connect` is bounded by `connect_timeout_secs`, so the probe
    /// must return `Err` inside the budget rather than hang the IPC future.
    ///
    /// Reaching the CONNECT-*read* stage of the `https` branch would require a
    /// TLS server whose certificate chains to a `webpki-roots` CA, which cannot
    /// be fabricated hermetically. That stage is therefore bounded where it is
    /// actually implemented: `tunnel::http_proxy::tests::test_tester_connect_read_is_bounded_when_the_peer_never_answers`
    /// exercises `perform_connect_within` — the very helper both branches call.
    #[tokio::test]
    async fn test_tester_https_probe_is_bounded_against_a_silent_endpoint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind silent https fixture");
        let port = listener.local_addr().expect("fixture addr").port();
        let fixture = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    // Hold the TCP connection open and never answer the ClientHello.
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    drop(stream);
                });
            }
        });

        let test = TestAppState::new().await;
        let mut tunnel = saved_proxy_tunnel("t-https-silent", port);
        tunnel
            .http_proxy
            .as_mut()
            .expect("fixture builds an http_proxy tunnel")
            .scheme = "https".into();
        test.store.save_tunnel(tunnel).await.unwrap();

        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            test_tunnel_impl(
                &test.state,
                "t-https-silent".into(),
                "127.0.0.1".into(),
                5432,
            ),
        )
        .await;
        let elapsed = started.elapsed();

        let result = outcome.expect(
            "the https probe must not hang: BUG-003 regression (the https branch \
             has no deadline)",
        );
        let err = result.expect_err("a silent endpoint under scheme=https must fail the probe");
        assert!(
            err.to_string().contains("timed out"),
            "the timeout must be identifiable in the error: {err}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "the https probe must be bounded by connect_timeout_secs (2s), took {elapsed:?}"
        );

        fixture.abort();
    }

    /// [tester] BUG-003 (a)3 — the deadline must bound the **whole** CONNECT
    /// handshake, not each individual `read`.
    ///
    /// `perform_connect` reads the status line byte by byte. A per-read timeout
    /// would never fire against a proxy that keeps dribbling bytes (every read
    /// makes progress), so the probe would still hang forever. This fixture
    /// answers one byte every 250 ms and never terminates the header, so only an
    /// absolute deadline can stop it.
    #[tokio::test]
    async fn test_tester_probe_deadline_covers_a_dribbling_proxy() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind dribbling proxy");
        let port = listener.local_addr().expect("fixture addr").port();
        let fixture = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf).await;
                    // Progress on every read, but the header terminator never arrives.
                    loop {
                        if stream.write_all(b"a").await.is_err() {
                            break;
                        }
                        let _ = stream.flush().await;
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    }
                });
            }
        });

        let test = TestAppState::new().await;
        test.store
            .save_tunnel(saved_proxy_tunnel("t-dribble", port))
            .await
            .unwrap();

        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            test_tunnel_impl(&test.state, "t-dribble".into(), "127.0.0.1".into(), 5432),
        )
        .await;
        let elapsed = started.elapsed();

        let result = outcome.expect(
            "a dribbling proxy must not hang the probe: the deadline must cover \
             the whole handshake, not each read (BUG-003 regression)",
        );
        let err = result.expect_err("a never-terminated CONNECT response must fail the probe");
        assert!(
            err.to_string().contains("timed out"),
            "the timeout must be identifiable in the error: {err}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "the probe must stop at connect_timeout_secs (2s), took {elapsed:?}"
        );

        fixture.abort();
    }
}
