//! 已保存隧道的连通性探测用例（形态中立）。
//!
//! 这是隧道管理 UI「测试隧道」按钮背后的编排：取定义 → 按种类把隧道**真的
//! 开起来**（SSH 是急切拨号；HTTP 代理 / WebSocket 的 `start` 是惰性绑定，
//! 所以再补一次上游验证）→ 拆掉 → 汇报耗时。它不制造运行时会话、不碰宿主
//! 状态，只依赖 [`TunnelRepository`] / [`KnownHostsRepository`] 两个端口，
//! 因此桌面命令与未来的 web 服务端 handler 共用同一份语义。
//!
//! 归位自桌面 `src/tunnel/mod.rs::test_tunnel`（B2 隧道域刀）：错误文案与
//! 验证顺序逐字保留——它们是 IPC 载荷的一部分，前端把失败原因原样展示。
//!
//! 2026-11-02 结构裁决后自 `tunnel/probe.rs` 迁入本应用层：探测是**用例**
//! （编排两个端口与全部三种机制），不是与 SSH / HTTP 代理 / WebSocket
//! 平级的第四种机制——与 connection / mcp 域的 `application/` 布局同构。

use std::sync::Arc;
use std::time::{Duration, Instant};

use datazen_driver_api::{DriverError, SavedTunnel, TunnelKind};

use crate::shared::scope::OrganizationScope;
use crate::tunnel::domain::{KnownHostsRepository, TunnelRepository};
use crate::tunnel::{
    verify_http_proxy_upstream, verify_websocket_upstream, HttpProxyTunnel, SshTunnel,
    WebSocketTunnel,
};

/// 一条已经开起来的隧道（探测结束后整体 drop 拆除）。
///
/// 桌面运行时把隧道包进自己的 `Tunnel` 枚举挂到连接资源上；探测不需要
/// 那层归属——它只要求「开起来、能拆掉」。
enum StartedTunnel {
    Ssh(SshTunnel),
    HttpProxy(HttpProxyTunnel),
    WebSocket(WebSocketTunnel),
}

/// 取回一条已保存的隧道定义，缺失与端口失败都压成配置错误。
///
/// 「不可用」与「不存在」在探测语义里同形：都解释给最终用户听，且都点名
/// 涉事的隧道 id。
async fn fetch_saved(
    tunnels: &dyn TunnelRepository,
    scope: &OrganizationScope,
    tunnel_id: &str,
) -> Result<SavedTunnel, DriverError> {
    let saved = match tunnels.get(scope, tunnel_id).await {
        Ok(saved) => saved,
        Err(error) => {
            return Err(DriverError::InvalidConfig(format!(
                "tunnel id '{tunnel_id}' unavailable: {error}"
            )));
        }
    };
    let Some(saved) = saved else {
        return Err(DriverError::InvalidConfig(format!(
            "tunnel id '{tunnel_id}' not found"
        )));
    };
    Ok(saved)
}

/// 按定义的种类把隧道开起来。返回 `None` 表示该定义**没有**可用的隧道配置
/// （`TunnelKind::None`）——调用方负责把它与「开不起来」区分开。
async fn start_saved(
    saved: &SavedTunnel,
    known_hosts: &Arc<dyn KnownHostsRepository>,
    target_host: &str,
    target_port: u16,
) -> Result<Option<StartedTunnel>, DriverError> {
    match saved.kind {
        TunnelKind::None => Ok(None),
        TunnelKind::Ssh => {
            let ssh = saved.ssh.as_ref().filter(|s| s.enabled).ok_or_else(|| {
                DriverError::InvalidConfig(
                    "tunnelKind=ssh but sshTunnel is missing or disabled".into(),
                )
            })?;
            Ok(Some(StartedTunnel::Ssh(
                SshTunnel::start(ssh, target_host, target_port, known_hosts.clone()).await?,
            )))
        }
        TunnelKind::HttpProxy => {
            let proxy = saved
                .http_proxy
                .as_ref()
                .filter(|s| s.enabled)
                .ok_or_else(|| {
                    DriverError::InvalidConfig(
                        "tunnelKind=httpProxy but httpProxyTunnel is missing or disabled".into(),
                    )
                })?;
            Ok(Some(StartedTunnel::HttpProxy(
                HttpProxyTunnel::start(proxy, target_host, target_port).await?,
            )))
        }
        TunnelKind::WebSocket => {
            let relay = saved
                .websocket
                .as_ref()
                .filter(|s| s.enabled)
                .ok_or_else(|| {
                    DriverError::InvalidConfig(
                        "tunnelKind=websocket but websocketTunnel is missing or disabled".into(),
                    )
                })?;
            Ok(Some(StartedTunnel::WebSocket(
                WebSocketTunnel::start(relay, target_host, target_port).await?,
            )))
        }
    }
}

/// Probe a saved tunnel against `target_host:target_port`; returns elapsed time.
///
/// 语义（与桌面实现逐字相同）：
///
/// 1. 取定义——失败点名隧道 id；
/// 2. 把隧道**真的**开起来：SSH 的 `start` 急切拨号 bastion；HTTP 代理与
///    WebSocket 的 `start` 只绑定本地端口（惰性），所以第 3 步才是真正的
///    上游证据；
/// 3. 对 HTTP 代理发一条真实的 `CONNECT`、对 `datazen_v1` 中继发一条真实
///    的 `open` 并等 `opened`——每一步都被 `connect_timeout_secs` 封顶；
/// 4. 拆掉隧道（drop），把总耗时交给调用方换算。
///
/// SSH 探测**不开** `direct-tcpip` 通道：会话建立本身就是可达性证据。
pub async fn test_saved_tunnel(
    tunnels: &dyn TunnelRepository,
    scope: &OrganizationScope,
    known_hosts: &Arc<dyn KnownHostsRepository>,
    tunnel_id: &str,
    target_host: &str,
    target_port: u16,
) -> Result<Duration, DriverError> {
    let saved = fetch_saved(tunnels, scope, tunnel_id).await?;
    let started = Instant::now();
    let tunnel = start_saved(&saved, known_hosts, target_host, target_port).await?;
    let Some(tunnel) = tunnel else {
        return Err(DriverError::InvalidConfig(format!(
            "tunnel '{tunnel_id}' resolved to no tunnel configuration"
        )));
    };
    let verified = match saved.kind {
        TunnelKind::HttpProxy => {
            verify_http_proxy_upstream(
                &saved.http_proxy.ok_or_else(|| {
                    DriverError::InvalidConfig(format!(
                        "tunnel '{tunnel_id}' has no httpProxy config"
                    ))
                })?,
                target_host,
                target_port,
            )
            .await
        }
        TunnelKind::WebSocket => {
            verify_websocket_upstream(
                &saved.websocket.ok_or_else(|| {
                    DriverError::InvalidConfig(format!(
                        "tunnel '{tunnel_id}' has no websocket config"
                    ))
                })?,
                target_host,
                target_port,
            )
            .await
        }
        TunnelKind::Ssh | TunnelKind::None => Ok(()),
    };
    tear_down(tunnel);
    verified?;
    Ok(started.elapsed())
}

/// 拆掉一条已经开起来的隧道。
///
/// 逐变体搬出字段再 drop：字段本身**只在 Drop 里被读**（拆隧道就是它的
/// 全部用途），显式解构让「拆掉」这个动作发生在代码里，而不是藏在枚举的
/// 整体 drop 背后。
fn tear_down(tunnel: StartedTunnel) {
    match tunnel {
        StartedTunnel::Ssh(tunnel) => drop(tunnel),
        StartedTunnel::HttpProxy(tunnel) => drop(tunnel),
        StartedTunnel::WebSocket(tunnel) => drop(tunnel),
    }
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
