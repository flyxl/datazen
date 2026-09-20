# RFC: HTTP / HTTPS / WebSocket 隧道

> **Status**: Draft → Implementation on branch `feat/http-https-websocket-tunnel`  
> **Related**: existing `src-tauri/src/ssh_tunnel.rs`, `ConnectionManager::maybe_start_tunnel`  
> **Goal**: 在仅能出站 HTTP(S)/WebSocket 的网络环境下，仍能把数据库 TCP 流量安全地转到目标主机。

---

## 1. 背景与动机

DataZen 已支持 **SSH 隧道**（本地 `127.0.0.1:随机端口` → SSH jump → 数据库）。

仍有大量场景 **不能开 22 端口**，只能走企业 HTTP 代理、仅放行 443，或必须经 WebSocket 中继：

| 场景 | SSH 隧道 | HTTP(S)/WS 隧道 |
|------|----------|-----------------|
| 公司仅允许 80/443 出站 | 常失败 | 可行 |
| 强制 HTTP CONNECT 代理 | 需 corkscrew 等外挂 | 原生支持 |
| 自建/SaaS WebSocket 中继 | 不适用 | 可行 |
| 标准 bastion SSH | 最优 | 冗余 |

本功能与 SSH **并列**，不替换 SSH；同一连接配置同一时间只启用一种隧道策略。

---

## 2. 范围（In / Out）

### 2.1 In Scope（本 RFC）

1. **HTTP CONNECT 代理隧道**  
   - 经 HTTP 或 HTTPS 代理建立 `CONNECT host:port`，再透明转发 TCP。  
   - 支持 Basic / 可选 Bearer 代理鉴权。  
   - TLS 到代理（`https://proxy`）与明文代理（`http://proxy`）。

2. **WebSocket 隧道客户端**  
   - 连接 `ws://` / `wss://` 中继。  
   - 约定轻量控制帧：打开目标、双向二进制帧透传、关闭/错误。  
   - 本地同样绑定 `127.0.0.1:0`，对 Driver 透明（与 SSH 相同模式）。

3. **配置、持久化、UI**  
   - 连接表单增加「隧道类型」：无 / SSH / HTTP Proxy / WebSocket。  
   - 密钥进现有加密存储路径；错误信息脱敏。

4. **与 ConnectionManager 集成**  
   - 统一 `TunnelHandle` 抽象；`ActiveSession` 持有生命周期，断开时释放。

### 2.2 Out of Scope（明确不做或后续）

- 在 DataZen 内实现 **服务端中继**（那是独立产品，如 frp/sish）。  
- SOCKS5 代理（可后续加，接口预留）。  
- 与 SSH 链式叠加（HTTP 代理上再套 SSH）—— v1 不做，文档说明可用系统级方案。  
- 透明系统级 VPN / 改路由表。

---

## 3. 用户故事

1. **开发者在公司网**：只能通过 `http://corp-proxy:8080` 出网，用 CONNECT 连云上 PostgreSQL `db.cloud:5432`。  
2. **安全团队要求**：所有出站必须 TLS；用 `https://proxy.corp:443` + CONNECT。  
3. **内网穿透用户**：运维提供 `wss://relay.example.com/tunnel`，DataZen 作为客户端把本地端口转到内网 DB。

---

## 4. 配置模型

### 4.1 设计原则

- **不破坏** 现有 `SshTunnelConfig` 与已存连接 JSON。  
- 新增可选字段；旧配置 `ssh_tunnel` 行为不变。  
- 互斥：`tunnel_kind` 决定启用哪条路径。

### 4.2 建议类型（driver-api / 前端 types 对齐）

```rust
/// 隧道策略。默认 None = 直连；兼容旧数据：仅有 ssh_tunnel.enabled 时视为 Ssh。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TunnelKind {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "ssh")]
    Ssh,
    #[serde(rename = "httpProxy")]
    HttpProxy,
    #[serde(rename = "websocket")]
    WebSocket,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpProxyTunnelConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 例: "proxy.corp.example" 或已含 scheme 的 URL host 部分由 UI 拆分
    pub host: String,
    pub port: u16,
    /// "http" | "https" — 到代理本身的传输
    pub scheme: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// 可选：额外 Proxy-Authorization 或自定义头（谨慎，勿日志打印）
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
    /// CONNECT 超时（秒）
    #[serde(default = "default_timeout_30")]
    pub connect_timeout_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketTunnelConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 完整 URL: wss://relay.example.com/v1/tunnel
    pub url: String,
    /// 可选 Bearer / 自定义子协议
    pub auth_token: Option<String>,
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
    #[serde(default = "default_timeout_30")]
    pub connect_timeout_secs: u32,
    /// 心跳间隔；0 = 关闭
    #[serde(default = "default_ping_30")]
    pub ping_interval_secs: u32,
}
```

`ConnectionConfig` 增量：

```rust
pub tunnel_kind: Option<TunnelKind>, // 缺省时：ssh_tunnel.enabled → Ssh，否则 None
pub http_proxy_tunnel: Option<HttpProxyTunnelConfig>,
pub websocket_tunnel: Option<WebSocketTunnelConfig>,
// ssh_tunnel: Option<SshTunnelConfig> 保持不变
```

**兼容规则**：

- 读旧配置：无 `tunnel_kind` 且 `ssh_tunnel.enabled` → 按 SSH 处理。  
- 写新配置：始终写显式 `tunnel_kind`。

---

## 5. 运行时架构

与 SSH 对齐的 **本地监听 + 双向拷贝** 模式，Driver 只看见改写后的 `host=127.0.0.1` + `port=local_port`。

```text
┌─────────────┐     localhost:L      ┌──────────────────┐
│ DB Driver   │ ───────────────────► │ Tunnel runtime   │
│ (PG/MySQL…) │                      │ (SSH | HTTP | WS)│
└─────────────┘                      └────────┬─────────┘
                                              │
                     ┌────────────────────────┼────────────────────────┐
                     ▼                        ▼                        ▼
              SSH direct-tcpip         HTTP CONNECT              WebSocket frames
                     │                        │                        │
                     └────────────────────────┴────────────────────────┘
                                              ▼
                                      remote_host:remote_port
```

### 5.1 统一抽象

```rust
// src-tauri/src/tunnel/mod.rs
pub enum Tunnel {
    Ssh(SshTunnel),
    HttpProxy(HttpProxyTunnel),
    WebSocket(WebSocketTunnel),
}

impl Tunnel {
    pub fn local_port(&self) -> u16 { ... }
}

pub async fn start_tunnel_for_connection(
    config: &ConnectionConfig,
    remote_host: &str,
    remote_port: u16,
    known_hosts_path: &Path, // SSH only
) -> Result<Option<Tunnel>, DriverError>;
```

`ConnectionManager::maybe_start_tunnel` 改为调用上述入口；`ActiveSession._tunnel` 类型改为 `Option<Tunnel>`。

### 5.2 HTTP CONNECT 流程

1. TCP（或 TLS）连到代理 `proxy_host:proxy_port`。  
2. 发送：
   ```http
   CONNECT remote_host:remote_port HTTP/1.1
   Host: remote_host:remote_port
   Proxy-Authorization: Basic …   # 若配置
   Proxy-Connection: Keep-Alive
   ```
3. 读响应至空行；状态码必须为 `200`。  
4. 之后连接为 **原始 TCP 字节流**。  
5. 本地 `TcpListener` accept → 对每个入站连接重复 1–4（或连接池复用策略 v1 采用 **每本地连接一条到代理的 CONNECT**，实现简单、行为清晰）。

**依赖建议**：`tokio` + `tokio-rustls`（或项目已有 TLS 栈）+ 手写最小 HTTP/1.1 CONNECT 解析（避免引入过重 HTTP 客户端仅为此）。

### 5.3 WebSocket 流程

**协议（DataZen Tunnel Protocol v1，客户端侧）**

| 方向 | 帧类型 | 载荷 |
|------|--------|------|
| C→S | Text JSON `{"op":"open","host":"…","port":5432,"id":"uuid"}` | 打开逻辑通道 |
| S→C | Text JSON `{"op":"opened","id":"uuid"}` 或 `{"op":"error",…}` | 确认 |
| 双向 | Binary | 原始 TCP 数据（可带 4 字节大端长度前缀 **或** 一帧=一段数据；v1 选 **一帧一段**，简单） |
| C→S | Text `{"op":"close","id":"uuid"}` | 关闭 |
| 双向 | Ping/Pong | 保活 |

说明：若对接 **第三方** 中继，需适配器层；v1 先实现「DataZen 约定 + 可配置 raw 透传模式」（`mode: "datazen_v1" | "raw_binary"`），`raw_binary` 下首条连接即双向二进制，目标由 URL query 指定（`?host=&port=`）。

**依赖建议**：`tokio-tungstenite`（与 tokio 生态一致）。

### 5.4 错误类型

扩展 `DriverError`：

```rust
HttpProxyTunnelError(String),
WebSocketTunnelError(String),
```

或统一 `TunnelError { kind, message }` 再映射到现有 IPC 错误码。日志与 UI **不得** 打印代理密码 / token。

---

## 6. 前端改动

1. **连接表单**（New / Edit Connection）  
   - 隧道类型 Select：直连 / SSH / HTTP 代理 / WebSocket。  
   - 条件渲染对应字段（复用现有 SSH 区块模式）。  
2. **i18n**：`connection.tunnel.*` 键（en + zh-CN 至少）。  
3. **类型**：`src/types` 与 `ConnectionConfig` 同步 camelCase。  
4. **导入连接**：TablePlus/DBeaver 等若有 HTTP 代理字段可后续映射；v1 可不做导入。

---

## 7. 安全

| 项 | 要求 |
|----|------|
| 密钥存储 | 与 SSH 密码相同：落盘加密 / OS keychain 路径 |
| TLS | HTTPS 代理与 WSS 默认校验证书；可选「允许无效证书」仅调试且默认关 |
| 日志 | host/port 可记；password/token 禁止 |
| SSRF | CONNECT/WS 目标来自用户配置的 DB host，不从不可信远程指令接收 |
| 本地绑定 | 仅 `127.0.0.1`，禁止 `0.0.0.0` |

---

## 8. 测试计划

1. **单元**  
   - CONNECT 请求拼装、200/403 响应解析。  
   - WS open/error JSON 解析。  
   - `tunnel_kind` 兼容旧 JSON。  
2. **集成**（可用 `tokio::io` mock 或本地 tiny proxy）  
   - 本地 echo server + mock CONNECT 代理 → 双向 echo。  
   - 可选：docker `squid` / `tinyproxy` CI 任务（非阻塞，标记 ignore 若环境无）。  
3. **回归**  
   - 现有 SSH 隧道测试与连接用例全部通过。

---

## 9. 分阶段实现

| 阶段 | 交付 | 状态 |
|------|------|------|
| **P0** | 本 RFC 文档合入功能分支 | 进行中 |
| **P1** | `tunnel` 模块骨架 + `Tunnel` 枚举；ConnectionManager 接线；配置类型与 serde 兼容 | 下一步 |
| **P2** | HTTP CONNECT 实现 + 单测 | |
| **P3** | WebSocket 客户端（datazen_v1 + raw_binary）+ 单测 | |
| **P4** | 连接表单 UI + i18n | |
| **P5** | 文档（用户手册 features）+ CHANGELOG 草稿 | |

每阶段保持 **可编译、SSH 行为不回归**。

---

## 10. 文件改动清单（预期）

```text
docs/architecture/rfc/http-https-websocket-tunnel.zh-CN.md   # 本文件
packages/driver-api/src/types.rs                            # 配置类型
src/types/index.ts                                          # 前端类型
src-tauri/src/tunnel/mod.rs                                 # 统一入口
src-tauri/src/tunnel/http_proxy.rs                          # CONNECT
src-tauri/src/tunnel/websocket.rs                           # WS
src-tauri/src/ssh_tunnel.rs                                 # 保持；或 re-export
src-tauri/src/services/connection_manager.rs                # maybe_start_tunnel
src-tauri/Cargo.toml                                        # tokio-tungstenite, rustls 等
src/components/connection/*                                 # 表单
src/locales/**/connection*.json                             # 文案
```

---

## 11. 风险与缓解

| 风险 | 缓解 |
|------|------|
| 代理对 CONNECT 目标端口白名单 | UI 提示；错误信息展示代理响应体摘要 |
| 部分代理不支持 HTTPS 到代理 | scheme 可选 http |
| WS 中继协议不统一 | `mode` 可切换；文档写明 DataZen v1 约定 |
| 配置字段膨胀 | 嵌套 struct + 条件 UI |
| 依赖体积 | 优先复用现有 TLS；WS 按需 feature |

---

## 12. 决议摘要

1. 与 SSH **并列**，`tunnel_kind` 互斥选择。  
2. 对 Driver **保持**「本地端口转发」契约。  
3. 先 **HTTP CONNECT**，再 **WebSocket**。  
4. **不** 在应用内实现中继服务端。  
5. 所有代码只在分支 `feat/http-https-websocket-tunnel` 提交。
