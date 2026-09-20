# 隧道连接（SSH / HTTP Proxy / WebSocket）

> 对应代码：`src-tauri/src/ssh_tunnel.rs`、`src-tauri/src/tunnel/`、连接表单 Advanced → Tunnel。  
> 设计文档：`docs/architecture/rfc/http-https-websocket-tunnel.zh-CN.md`。

## 1. 概述

当数据库主机不能直连时，DataZen 可在本机监听 `127.0.0.1:随机端口`，把流量经隧道转到真实 `host:port`。  
驱动层只看到改写后的回环地址，与是否使用隧道无关。

| 类型 | 适用场景 | 配置入口 |
|------|----------|----------|
| **无（直连）** | 网络可达 | 默认 |
| **SSH** | 有跳板机 / bastion | 隧道类型 = SSH |
| **HTTP 代理** | 仅能出站走企业 HTTP CONNECT 代理 | 隧道类型 = HTTP 代理 |
| **WebSocket** | 经自建/SaaS WebSocket 中继 | 隧道类型 = WebSocket |

同一连接 **同时只启用一种** 隧道策略（`tunnelKind` 互斥）。

## 2. SSH 隧道

与既有行为一致：密码 / 私钥 / SSH Agent，支持一层 ProxyJump。

## 3. HTTP CONNECT 代理

配置项：

- 代理主机、端口
- scheme：`http`（推荐）或 `https`
- 可选用户名/密码（Basic）
- 连接超时（秒）

流程：本机 accept → 连接代理 → 发送 `CONNECT db-host:db-port` → 200 后双向拷贝。

## 4. WebSocket 隧道

配置项：

- **URL**：`ws://` 或 `wss://` 中继地址
- **模式**
  - `datazen_v1`：先发 JSON `{"op":"open","host","port","id"}`，等待 `opened`，再传二进制帧
  - `raw_binary`：连接后直接双向二进制透传
- 可选 Bearer Token、连接超时

**说明**：DataZen **不**内置中继服务端；需自备兼容的中继。

## 5. 配置示例

```json
{
  "tunnelKind": "httpProxy",
  "httpProxyTunnel": {
    "enabled": true,
    "host": "proxy.corp.example",
    "port": 8080,
    "scheme": "http",
    "username": "user",
    "password": "secret",
    "connectTimeoutSecs": 30
  },
  "host": "db.internal",
  "port": 5432
}
```

旧配置仅含 `sshTunnel.enabled: true`、无 `tunnelKind` 时，运行时仍按 SSH 处理。

## 6. 安全注意

- 本地监听仅绑定 `127.0.0.1`
- 代理密码 / Token 走与 SSH 密码相同的加密存储路径；日志不打印密钥
- 目标 host/port 来自用户连接配置，不由远程中继指令决定
