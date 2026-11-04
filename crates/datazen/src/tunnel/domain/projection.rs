//! 名单档与反向投影：隧道域的两条**脱敏映射**。
//!
//! 桌面仓储实现（`DesktopRepository`）与未来的 web 形态服务端仓储都要回答
//! 同两个问题：「给我隧道名单（**不带密钥**）」和「谁在引用这条隧道」。
//! 端口（[`TunnelRepository::list`](super::TunnelRepository::list) /
//! [`TunnelRepository::usage`](super::TunnelRepository::usage)）只承诺形状，
//! 映射本体曾经散在每个实现方手里——桌面那份在 `store/tunnels.rs`，将来
//! 服务端还得再抄一份，两份必然漂移。本文件把两条映射收成**唯一权威**：
//!
//! * [`summarize`]——全量定义 → 摘要（`id` / `name` / `kind`，仅此三项）；
//!   名单要进选择器与 IPC 载荷，任何口令、密钥或 token 都不得混进去
//!   （[隧道 §4.2](../../../../docs/architecture/backend/tunnel.md#42-加密与最小暴露)）。
//! * [`usage_from_connections`]——从连接清单折出反向投影；
//!   `connection_ids` 与 `connection_names` **位置对齐**，且空 id 不扫面
//!   （`materialize_tunnel_refs` 语义里 `""` 就是「无隧道引用」）。
//!
//! 映射是纯函数：不碰存储、不碰宿主。实现方在自己的 `list` / `usage` 里
//! 调用它们，即得到与桌面逐字相同的 IPC 载荷。

use datazen_driver_api::{ConnectionConfig, SavedTunnel, SavedTunnelSummary, TunnelUsage};

/// 名单档投影：全量隧道定义 → 摘要。
///
/// 摘要**只有** `id` / `name` / `kind` 三个字段——`SavedTunnelSummary` 的
/// 序列化形状由 driver 契约 crate 冻结（名单进选择器与 IPC 载荷），本函数
/// 是往那个形状里装内容的唯一权威：密钥落在地上不是因为谁忘了序列化它们，
/// 而是因为投影根本不取那些字段。
pub fn summarize(tunnel: &SavedTunnel) -> SavedTunnelSummary {
    SavedTunnelSummary {
        id: tunnel.id.clone(),
        name: tunnel.name.clone(),
        kind: tunnel.kind,
    }
}

/// 反向投影：哪些连接引用了 `tunnel_id`。
///
/// `connection_ids` 与 `connection_names` 按连接清单的**原顺序**位置对齐；
/// 连接清单的顺序由实现方的存储决定（桌面是插入序）。
///
/// 空 `tunnel_id` **不做扫面**：`materialize_tunnel_refs` 把 `""` 当「无隧道
/// 引用」，一个空查询不得把引用同样为空串的连接扫进来，也不得把所有直连
/// （`None`）扫进来。
pub fn usage_from_connections<'a, I>(connections: I, tunnel_id: &str) -> TunnelUsage
where
    I: IntoIterator<Item = &'a ConnectionConfig>,
{
    let mut connection_ids = Vec::new();
    let mut connection_names = Vec::new();
    // An empty query must not sweep in connections whose reference is itself
    // empty: `materialize_tunnel_refs` treats "" as "no tunnel reference".
    if !tunnel_id.is_empty() {
        for conn in connections {
            if conn.tunnel_id.as_deref() == Some(tunnel_id) {
                connection_ids.push(conn.id.clone());
                connection_names.push(conn.name.clone());
            }
        }
    }
    TunnelUsage {
        connection_ids,
        connection_names,
    }
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
