//! Tunnel entity IPC (independent of connection configs).
//!
//! B2 隧道域刀：`*_impl` 转发层已拆除，接线直接住在 `#[tauri::command]` 体里。
//! 两条曾经散在宿主的逻辑收进了共享 crate 的唯一权威，命令壳只做装配：
//!
//! * 名单/反向投影（`id`/`name`/`kind` 摘要与 usage 位置对齐）——
//!   `datazen::tunnel::domain` 的 `summarize` / `usage_from_connections`，
//!   桌面仓储（`store/tunnels.rs`）的 `list` / `usage` 实现调同一份；
//! * 「测试隧道」探测编排（取定义 → 按种类开隧道 → 惰性种类补上游验证 →
//!   拆掉 → 报耗时）—— `datazen::tunnel::test_saved_tunnel`。
//!
//! IPC 契约逐字不变：命令名、参数、返回形状与错误文案（见文末
//! `commands/ipc_surface_tests.rs` 的入口面守卫）。

use super::error::{CmdExt, CommandError};
use super::AppState;
use datazen::shared::error::PortError;
use datazen_driver_api::{SavedTunnel, SavedTunnelSummary, TunnelUsage};
use tauri::State;

/// 端口失败 → 命令错误：**原文保留**。
///
/// 桌面实现的失败只有一个来源（存储层），端口把它压成
/// `PortError::BackendUnavailable(<原文>)`；这里把原文原样还回去，因此 IPC 载荷
/// 与端口化之前**逐字相同**（此前这里是 `DesktopRepositoryError`，其 `Display` 已被端口原样带走）。
/// 端口调用 → 命令结果（映射与日志见 [`port_cmd`](crate::commands::port_call::port_cmd)）。
///
/// 保留这个同义名，是为了让本文件几十处调用点逐字不变——隧道族的可读性来自
/// 「`tunnel_cmd(...)` 就是在调隧道端口」。
fn tunnel_cmd<T>(result: Result<T, PortError>, cmd: &str) -> Result<T, CommandError> {
    crate::commands::port_call::port_cmd(result, cmd)
}

#[tauri::command]
pub async fn get_tunnels(state: State<'_, AppState>) -> Result<Vec<SavedTunnel>, CommandError> {
    // 端口只有两档：名单（摘要，**不带密钥**）与单条取用（全量）。这个 IPC 的载荷是
    // **全量**且形状冻结，因此由两档组合而成：先取名单，再逐条取用。
    //
    // 生产代码没有它的调用方（所有列表路径都走 `get_tunnel_summaries`），但命令的
    // 形状与载荷不变。
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(&state)?;
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

#[tauri::command]
pub async fn get_tunnel(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<SavedTunnel>, CommandError> {
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(&state)?;
    tunnel_cmd(services.tunnels.get(&scope, &id).await, "get_tunnel")
}

#[tauri::command]
pub async fn save_tunnel(
    state: State<'_, AppState>,
    tunnel: SavedTunnel,
) -> Result<(), CommandError> {
    tracing::info!(id = %tunnel.id, name = %tunnel.name, kind = ?tunnel.kind, "save_tunnel");
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(&state)?;
    tunnel_cmd(services.tunnels.save(&scope, tunnel).await, "save_tunnel")
}

#[tauri::command]
pub async fn delete_tunnel(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_tunnel");
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(&state)?;
    tunnel_cmd(services.tunnels.delete(&scope, &id).await, "delete_tunnel")
}

/// Metadata-only projection of the stored tunnels for the selector.
///
/// 走端口的**名单档**：它只带 `id` / `name` / `kind`，**不含**任何凭据——
/// 端口这一档刻意不返回密钥（`get_tunnels` 那种全量列表是另一条路径）。
/// 投影本体是 `datazen::tunnel::domain` 的共享映射，桌面仓储调同一份。
#[tauri::command]
pub async fn get_tunnel_summaries(
    state: State<'_, AppState>,
) -> Result<Vec<SavedTunnelSummary>, CommandError> {
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(&state)?;
    tunnel_cmd(services.tunnels.list(&scope).await, "get_tunnel_summaries")
}

/// Which connections reference `id`. `connection_ids` and `connection_names`
/// are positionally aligned.
#[tauri::command]
pub async fn get_tunnel_usage(
    state: State<'_, AppState>,
    id: String,
) -> Result<TunnelUsage, CommandError> {
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(&state)?;
    tunnel_cmd(
        services.tunnels.usage(&scope, &id).await,
        "get_tunnel_usage",
    )
}

/// Probe a saved tunnel against `target_host:target_port`; returns elapsed ms.
///
/// 编排在 `datazen::tunnel::test_saved_tunnel`（形态中立）：取定义 → 按种类把
/// 隧道真的开起来 → 惰性种类（HTTP 代理 / WebSocket）补一次上游验证 → 拆掉。
/// 错误文案是 IPC 载荷，由那份用例逐字持有。
#[tauri::command]
pub async fn test_tunnel(
    state: State<'_, AppState>,
    id: String,
    target_host: String,
    target_port: u16,
) -> Result<u64, CommandError> {
    let scope = state.organization_scope()?;
    let services = crate::platform::ipc::require_application_services(&state)?;
    let elapsed = datazen::tunnel::test_saved_tunnel(
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

// 测试宿主（cfg(test)）：把 harness 的 AppState 挂进 mock tauri app，让测试
// 能以真实 `#[tauri::command]` 入口驱动（`State<'_, AppState>` 只能来自
// tauri `Manager`）。
#[cfg(test)]
#[path = "tunnel_command_test_host.rs"]
mod tunnel_command_test_host;

#[cfg(test)]
mod tests {
    //! 命令壳的**留守测试**（B2 隧道域刀后的分类）：
    //!
    //! * **C4 适配器烟测**——真实命令入口 + `DesktopRepository`：脱敏映射经由
    //!   桌面仓储的 `list` / `usage`（它们调用 `datazen::tunnel::domain` 的共享
    //!   投影）端到端走一遍。映射本体的语义深度（顺序、位置对齐、空 id 不
    //!   扫面）已随映射迁入 `modules/src/tunnel/domain/projection_tests.rs`。
    //! * **C1 命令入口行为**——`test_tunnel` 的参数面与毫秒换算：错误路径点名
    //!   隧道 id、成功路径返回 `u64` 毫秒。探测语义（按种类开隧道、上游验证、
    //!   拆除、BUG-001/BUG-003 回归矩阵）已随用例迁入
    //!   `modules/src/tunnel/probe_tests.rs`；上游探针与数据路径的逐字节测试在
    //!   `modules/src/tunnel/{http_proxy,websocket}_probe_tests.rs`。

    use super::*;
    use super::tunnel_command_test_host::host_command_app;
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use datazen_driver_api::{SshTunnelConfig, TunnelKind};

    const SSH_PASSWORD: &str = "ssh-password-secret";
    const SSH_PASSPHRASE: &str = "ssh-passphrase-secret";
    const SSH_HOST: &str = "bastion.internal";

    fn saved_ssh_tunnel(id: &str, name: &str) -> SavedTunnel {
        SavedTunnel {
            id: id.into(),
            name: name.into(),
            kind: TunnelKind::Ssh,
            ssh: Some(SshTunnelConfig {
                enabled: true,
                host: SSH_HOST.into(),
                port: 2222,
                username: "ubuntu".into(),
                auth_method: "password".into(),
                password: Some(SSH_PASSWORD.into()),
                private_key_path: None,
                passphrase: Some(SSH_PASSPHRASE.into()),
                jump: None,
            }),
            http_proxy: None,
            websocket: None,
        }
    }

    fn assert_no_secret_leak(json: &str) {
        for needle in [
            SSH_PASSWORD,
            SSH_PASSPHRASE,
            SSH_HOST,
            "ubuntu",
            "password",
            "passphrase",
            "authToken",
            "auth_token",
            "privateKeyPath",
        ] {
            assert!(
                !json.contains(needle),
                "IPC payload leaked `{needle}`: {json}"
            );
        }
    }

    // ── C4: adapter smoke — real command entry + DesktopRepository ──

    #[tokio::test]
    async fn summaries_drop_every_tunnel_secret() {
        let test = TestAppState::new().await;
        test.store
            .save_tunnel(saved_ssh_tunnel("t1", "Prod Bastion"))
            .await
            .unwrap();

        let app = host_command_app(test);
        let summaries = get_tunnel_summaries(app.state()).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, "t1");
        assert_eq!(summaries[0].name, "Prod Bastion");
        assert_eq!(summaries[0].kind, TunnelKind::Ssh);

        let json = serde_json::to_string(&summaries).unwrap();
        assert_no_secret_leak(&json);
        let value: serde_json::Value = serde_json::to_value(&summaries).unwrap();
        assert_eq!(
            value,
            serde_json::json!([{ "id": "t1", "name": "Prod Bastion", "kind": "ssh" }]),
            "the summary must expose exactly id/name/kind"
        );
    }

    #[tokio::test]
    async fn usage_lists_referencing_connections_in_order() {
        let test = TestAppState::new().await;
        test.store
            .save_tunnel(saved_ssh_tunnel("t1", "Prod Bastion"))
            .await
            .unwrap();
        test.store
            .save_tunnel(saved_ssh_tunnel("t2", "Other Bastion"))
            .await
            .unwrap();

        let mut hit = sample_postgres_config("conn-hit");
        hit.name = "Hit".into();
        hit.tunnel_id = Some("t1".into());
        test.store.save_connection(hit).await.unwrap();

        let mut other = sample_postgres_config("conn-other");
        other.name = "Other".into();
        other.tunnel_id = Some("t2".into());
        test.store.save_connection(other).await.unwrap();

        let app = host_command_app(test);
        let usage = get_tunnel_usage(app.state(), "t1".into()).await.unwrap();
        assert_eq!(usage.connection_ids, vec!["conn-hit".to_string()]);
        assert_eq!(usage.connection_names, vec!["Hit".to_string()]);

        let json = serde_json::to_string(&usage).unwrap();
        assert_no_secret_leak(&json);

        // Miss path: a stored tunnel nobody references returns empty arrays.
        let miss = get_tunnel_usage(app.state(), "t2-missing".into())
            .await
            .unwrap();
        assert!(miss.connection_ids.is_empty());
        assert!(miss.connection_names.is_empty());
    }

    // ── C1: command-entry behavior for the probe ────────────────────

    #[tokio::test]
    async fn test_tunnel_rejects_unknown_tunnel_id() {
        let test = TestAppState::new().await;
        let app = host_command_app(test);
        let err = test_tunnel(
            app.state(),
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

    /// [tester] Success path through the real command entry: the probe must
    /// reach `Ok(elapsed_ms)` (the `u64` millisecond conversion included) and
    /// tear the tunnel down without panicking.
    ///
    /// The fixture is a local TCP listener that answers an HTTP `CONNECT` with
    /// `200`, so the assertion also holds if the lazily-started proxy path is
    /// ever made to dial eagerly (see `tunnel-backend-BUG-001`). The deeper
    /// probe semantics live in `modules/src/tunnel/probe_tests.rs`.
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

        let app = host_command_app(test);
        let elapsed_ms = test_tunnel(
            app.state(),
            "t-proxy".into(),
            "127.0.0.1".into(),
            5432,
        )
        .await
        .expect("the probe must succeed for a reachable local proxy fixture");
        assert!(
            elapsed_ms < 5_000,
            "probe took suspiciously long: {elapsed_ms}ms"
        );

        fixture.abort();
    }
}
