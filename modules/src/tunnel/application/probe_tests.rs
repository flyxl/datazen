//! 已保存隧道探测用例（[`test_saved_tunnel`](super)）的语义测试。
//!
//! 自桌面整体归位（B2 隧道域刀）：原宿主侧有两组测试钉这份编排——
//! `commands/tunnel.rs` 测试模块里的 13 个（经 `test_tunnel_impl` 进）与
//! `commands/tunnel_probe_tests.rs` 里的 2 个（`every_kind_rejects…` 与
//! 真 SSH 堡垒机成功路径）。逻辑收进本模块后测试跟着走，断言逐字保留；
//! 夹具换成 [`MemoryTunnelStore`] / [`MemoryKnownHosts`]（不再经桌面仓储），
//! 真 SSH 堡垒机用 `ssh.rs` 自带的 `test_fixture`。
//!
//! 桌面侧保留的是 C1 命令入口行为（真实 `#[tauri::command]` + 毫秒换算），
//! 见 `apps/desktop/src-tauri/src/commands/tunnel.rs` 的测试模块。上游探针
//! （`verify_http_proxy_upstream` / `verify_websocket_upstream`）与数据路径的
//! 逐字节测试住在各自机制文件旁（本目录的上一级）：
//! `modules/src/tunnel/{http_proxy,websocket}_probe_tests.rs`。

use std::sync::Arc;
use std::time::Duration;

use datazen_driver_api::{SavedTunnel, SshTunnelConfig, TunnelKind};

use super::test_saved_tunnel;
use crate::shared::id::OrganizationId;
use crate::shared::scope::OrganizationScope;
use crate::tunnel::domain::KnownHostsRepository;
use crate::tunnel::ssh::test_fixture;
use crate::tunnel::test_support::{MemoryKnownHosts, MemoryTunnelStore};

fn scope() -> OrganizationScope {
    OrganizationScope::new(OrganizationId::new("org-1"))
}

/// 探测现场的三个注入件：仓储替身 + known-hosts 替身。
struct Probe {
    store: MemoryTunnelStore,
    known_hosts: Arc<dyn KnownHostsRepository>,
}

impl Probe {
    fn new() -> Self {
        Self {
            store: MemoryTunnelStore::new(),
            known_hosts: Arc::new(MemoryKnownHosts::default()),
        }
    }

    async fn run(
        &self,
        tunnel_id: &str,
        target_host: &str,
        target_port: u16,
    ) -> Result<Duration, datazen_driver_api::DriverError> {
        test_saved_tunnel(
            &self.store,
            &scope(),
            &self.known_hosts,
            tunnel_id,
            target_host,
            target_port,
        )
        .await
    }
}

// ── 取定义与配置形态的失败路径 ────────────────────────────────────

#[tokio::test]
async fn test_tunnel_rejects_unknown_tunnel_id() {
    let probe = Probe::new();
    let err = probe
        .run("missing-tunnel", "127.0.0.1", 5432)
        .await
        .expect_err("an unknown tunnel id must fail the probe");
    assert!(
        err.to_string().contains("missing-tunnel"),
        "error must name the offending tunnel: {err}"
    );
}

#[tokio::test]
async fn test_tunnel_rejects_tunnel_without_a_tunnel_config() {
    let probe = Probe::new();
    probe.store.save_tunnel(SavedTunnel {
        id: "t-none".into(),
        name: "Disabled".into(),
        kind: TunnelKind::None,
        ssh: None,
        http_proxy: None,
        websocket: None,
    });

    let err = probe
        .run("t-none", "127.0.0.1", 5432)
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
    let probe = Probe::new();
    probe.store.save_tunnel(SavedTunnel {
        id: "t-ssh-broken".into(),
        name: "Broken".into(),
        kind: TunnelKind::Ssh,
        ssh: None,
        http_proxy: None,
        websocket: None,
    });

    let err = probe
        .run("t-ssh-broken", "127.0.0.1", 5432)
        .await
        .expect_err("kind=ssh with no ssh config must not report success");
    assert!(
        err.to_string().contains("sshTunnel is missing or disabled"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn test_tester_test_tunnel_rejects_disabled_ssh_tunnel() {
    let probe = Probe::new();
    let mut tunnel = saved_ssh_tunnel("t-ssh-off", "Disabled");
    if let Some(ssh) = tunnel.ssh.as_mut() {
        ssh.enabled = false;
    }
    probe.store.save_tunnel(tunnel);

    let err = probe
        .run("t-ssh-off", "127.0.0.1", 5432)
        .await
        .expect_err("a disabled ssh tunnel must not report success");
    assert!(
        err.to_string().contains("sshTunnel is missing or disabled"),
        "unexpected error: {err}"
    );
}

/// [tester] Success path: the probe must reach `Ok(elapsed)` and tear the
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

    let probe = Probe::new();
    probe.store.save_tunnel(SavedTunnel {
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
    });

    let elapsed = probe
        .run("t-proxy", "127.0.0.1", 5432)
        .await
        .expect("the probe must succeed for a reachable local proxy fixture");
    assert!(
        elapsed < Duration::from_secs(5),
        "probe took suspiciously long: {elapsed:?}"
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
    let probe = Probe::new();
    let port = closed_local_port().await;
    probe
        .store
        .save_tunnel(saved_proxy_tunnel("t-dead-proxy", port));

    let err = probe
        .run("t-dead-proxy", "127.0.0.1", 5432)
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

    let probe = Probe::new();
    probe
        .store
        .save_tunnel(saved_proxy_tunnel("t-refusing-proxy", port));

    let err = probe
        .run("t-refusing-proxy", "127.0.0.1", 5432)
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
    let probe = Probe::new();
    let port = closed_local_port().await;
    probe
        .store
        .save_tunnel(saved_websocket_tunnel("t-dead-relay", port));

    let err = probe
        .run("t-dead-relay", "127.0.0.1", 5432)
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
                    let open: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
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

    let probe = Probe::new();
    probe
        .store
        .save_tunnel(saved_websocket_tunnel("t-live-relay", port));

    let elapsed = probe
        .run("t-live-relay", "127.0.0.1", 5432)
        .await
        .expect("a relay that acks the open must pass the probe");
    assert!(
        elapsed < Duration::from_secs(5),
        "probe took suspiciously long: {elapsed:?}"
    );

    fixture.abort();
}

/// SSH has always dialed eagerly, so this pins that an unreachable bastion
/// is reported as unreachable rather than optimistically succeeding.
#[tokio::test]
async fn test_tunnel_rejects_unreachable_ssh_bastion() {
    let probe = Probe::new();
    let port = closed_local_port().await;
    probe.store.save_tunnel(SavedTunnel {
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
    });

    let err = probe
        .run("t-dead-bastion", "127.0.0.1", 5432)
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
                tokio::time::sleep(Duration::from_secs(60)).await;
                drop(stream);
            });
        }
    });

    let probe = Probe::new();
    probe
        .store
        .save_tunnel(saved_proxy_tunnel("t-silent-proxy", port));

    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(15),
        probe.run("t-silent-proxy", "127.0.0.1", 5432),
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
        elapsed < Duration::from_secs(10),
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
                tokio::time::sleep(Duration::from_secs(60)).await;
                drop(stream);
            });
        }
    });

    let probe = Probe::new();
    let mut tunnel = saved_proxy_tunnel("t-https-silent", port);
    tunnel
        .http_proxy
        .as_mut()
        .expect("fixture builds an http_proxy tunnel")
        .scheme = "https".into();
    probe.store.save_tunnel(tunnel);

    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(15),
        probe.run("t-https-silent", "127.0.0.1", 5432),
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
        elapsed < Duration::from_secs(10),
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
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            });
        }
    });

    let probe = Probe::new();
    probe
        .store
        .save_tunnel(saved_proxy_tunnel("t-dribble", port));

    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(15),
        probe.run("t-dribble", "127.0.0.1", 5432),
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
        elapsed < Duration::from_secs(10),
        "the probe must stop at connect_timeout_secs (2s), took {elapsed:?}"
    );

    fixture.abort();
}

// ── [tester round 2] adversarial re-verification of BUG-001 ──────

/// `127.0.0.1:1` has no listener (and needs root to ever get one), so it is a
/// deterministic unreachable endpoint with no bind/drop race.
const DEAD_PORT: u16 = 1;

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

fn saved_ssh_tunnel(id: &str, name: &str) -> SavedTunnel {
    let mut tunnel = saved_ssh_tunnel_at(id, "bastion.internal", 2222);
    tunnel.name = name.into();
    tunnel
}

/// [tester] (a)1 — adversarial re-check of BUG-001, written from scratch:
/// all three kinds pointed at an endpoint with nothing listening must answer
/// `Err`. The pre-fix code answered `Ok(0)` for httpProxy and websocket.
#[tokio::test]
async fn test_tester_every_kind_rejects_an_unreachable_endpoint() {
    let probe = Probe::new();
    probe.store.save_tunnel(saved_proxy_tunnel_at(
        "t-dead-http",
        "127.0.0.1",
        DEAD_PORT,
        "http",
        None,
        1,
    ));
    probe.store.save_tunnel(saved_websocket_tunnel_at(
        "t-dead-ws",
        &format!("ws://127.0.0.1:{DEAD_PORT}/tunnel"),
        "datazen_v1",
        1,
    ));
    probe
        .store
        .save_tunnel(saved_ssh_tunnel_at("t-dead-ssh", "127.0.0.1", DEAD_PORT));

    for (id, needle) in [
        ("t-dead-http", "connect to proxy"),
        ("t-dead-ws", "WebSocket connect"),
        ("t-dead-ssh", "SSH connect"),
    ] {
        let err = probe
            .run(id, "db.internal", 5432)
            .await
            .expect_err("an unreachable endpoint must not be reported as reachable");
        let text = err.to_string();
        assert!(text.contains(needle), "{id}: unexpected error: {text}");
    }
}

/// [tester] (a)1/(a)5 — the SSH **success** path through the real use case,
/// which the previous round could not exercise ("needs a live SSH server").
/// Runs against the in-process bastion fixture and also pins the SSH
/// semantic boundary: the probe must not open a `direct-tcpip` channel
/// for the target, and `test_saved_tunnel`'s `drop` must not leak the session.
#[tokio::test]
async fn test_tester_test_tunnel_succeeds_against_a_real_ssh_bastion() {
    use std::sync::atomic::Ordering;

    let bastion = test_fixture::spawn_bastion().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let key_path = test_fixture::write_client_key(dir.path());

    let probe = Probe::new();
    let mut tunnel = saved_ssh_tunnel_at("t-live-ssh", "127.0.0.1", bastion.port);
    if let Some(ssh) = tunnel.ssh.as_mut() {
        ssh.auth_method = "private_key".into();
        ssh.password = None;
        ssh.username = "tester".into();
        ssh.private_key_path = Some(key_path.to_string_lossy().into_owned());
    }
    probe.store.save_tunnel(tunnel);

    let elapsed = probe
        .run("t-live-ssh", "127.0.0.1", 5432)
        .await
        .expect("a reachable bastion must pass the probe");
    assert!(elapsed < Duration::from_secs(5), "probe took {elapsed:?}");
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
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        closed,
        "test_saved_tunnel must tear the SSH session down, still live: {}",
        bastion.live_sessions.load(Ordering::SeqCst)
    );
    bastion.shutdown();
}

/// 与桌面侧 `saved_proxy_tunnel_at` 同形的参数化代理夹具（[tester round 2]）。
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

/// 与桌面侧 `saved_websocket_tunnel_at` 同形的参数化中继夹具（[tester round 2]）。
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
