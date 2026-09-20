# SavedTunnel：独立隧道实体

## 已完成

| 项 | 位置 |
|----|------|
| `SavedTunnel` 类型 | `packages/driver-api/src/tunnel_types.rs` |
| `ConnectionConfig.tunnel_id` | `packages/driver-api/src/types.rs` |
| 存储 `tunnels.json` + AES 加密敏感字段 | `src-tauri/src/store/tunnels.rs` |
| Store 启动加载 | `src-tauri/src/store/mod.rs` (`mod tunnels` + `load_tunnels_from_disk`) |
| IPC | `get_tunnels` / `get_tunnel` / `save_tunnel` / `delete_tunnel` |
| 连接时按 `tunnel_id` 注入配置 | `ConnectionManager::resolve_tunnel_ref` |
| 前端 `SavedTunnel` 类型 | `src/types/tunnel.ts` |

## 存储路径

应用数据目录（与 `connections.json` 同级）：

```
{app_data_dir}/tunnels.json
```

敏感字段（SSH password/passphrase、HTTP proxy password、WS authToken）与 DB 密码相同：AES-256-GCM + base64。

## 连接引用

```json
{
  "id": "conn-1",
  "name": "Prod PG",
  "tunnelId": "tunnel-corp-proxy",
  "tunnelKind": null
}
```

`tunnelId` 优先：连接时从 `tunnels.json` 解析并写入临时 `ssh_tunnel` / `http_proxy_tunnel` / `websocket_tunnel` 再走现有 tunnel runtime。

## 兼容

- 无 `tunnelId` 时仍可读内嵌的 `sshTunnel` / `httpProxyTunnel` / `websocketTunnel`（旧数据）。
- 新建连接推荐只存 `tunnelId`，便于多连接复用同一隧道。
