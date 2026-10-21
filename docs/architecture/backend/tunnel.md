# Tunnel

> Source of truth: `src-tauri/src/tunnel/`、`src-tauri/src/ssh_tunnel.rs`、
> `src-tauri/src/platform/session_backend/`、`src-tauri/src/store/tunnels.rs`、
> `packages/driver-api/src/tunnel_types.rs`。

隧道子系统解决的是「数据库不在本机、也不在可达网段」时的连接问题。**核心设计约束只有一条：驱动零改动。** 隧道不进入 `DatabaseDriver` trait，也不进入 `PROTOCOL_VERSION`——桌面 `DesktopSessionBackend` 在 Runtime 打开 Driver resource 前改写地址，并把 Tunnel 句柄保存在该 Runtime resource 上。

用户视角的使用说明见 [Tunnel 使用指南](../../features/tunnel-guide.zh-CN.md)。

## 1. 设计前提：为什么不用改驱动

数据库驱动的连接串最终由驱动自己拼装（`postgres://user:pw@host:port/db`）。四种隧道如果各自改写驱动源码，会立刻带来协议版本耦合与驱动回归。DataZen 选择另一条路：

```text
配置的 host:port                     真实数据库
        │                              ▲
        │  ① 从 tunnels.json 解引用      │ ④ TCP / 隧道转发
        ▼                              │
DesktopSessionBackend                └── 上游（跳板机 / 代理 / 中继）
        │  ② start_for_connection
        ▼
    本机 127.0.0.1:<随机端口>  ────────►
        ▲
        │  ③ rewrite_to_local：host/port 替换为回环地址
        │
   Driver.connect()（完全不知情）
```

驱动只看见 `127.0.0.1:<随机端口>`。加一层隧道、新增一种隧道类型或换一个中继，对驱动与 `packages/driver-api` 契约都没有影响。全仓不存在任何「用隧道配置拼连接串」的代码。

## 2. 四种互斥状态

`TunnelKind`（`packages/driver-api/src/tunnel_types.rs:24`）是互斥枚举，序列化为 `none` / `ssh` / `httpProxy` / `websocket`：

| `TunnelKind` | 配置字段 | 上游形态 |
|---|---|---|
| `none` | — | 直连 |
| `ssh` | `ConnectionConfig.ssh_tunnel`（`SshTunnelConfig`，`types.rs:68`） | SSH 端口转发（`ssh_tunnel::SshTunnel`） |
| `httpProxy` | `ConnectionConfig.http_proxy_tunnel` | 本机监听 → HTTP `CONNECT` 代理 → 真实库（`tunnel/http_proxy.rs`） |
| `websocket` | `ConnectionConfig.websocket_tunnel` | 本机监听 → WebSocket 中继 → 真实库（`tunnel/websocket.rs`） |

`SshTunnelConfig` 支持三种认证（`password` / `private_key` / `agent`）与**递归 ProxyJump**（`jump: Option<Box<SshTunnelConfig>>`），多层跳板由 `_upstream` 持有，任一层随 session 一起拆除。

### 2.1 判定顺序（同时是旧配置的兼容路径）

`resolve_tunnel_kind(&config)`（`tunnel/mod.rs:42`）：

1. `config.tunnel_kind` 为 `Some` → **直接返回，包括 `Some(None)`**（显式声明「不走隧道」时不回落到旧字段）。
2. 否则依次看 `ssh_tunnel.enabled` → `http_proxy_tunnel.enabled` → `websocket_tunnel.enabled`。
3. 全不成立 → `TunnelKind::None`。

`ConnectionConfig.tunnel_kind` 是新契约（`types.rs:112`），三个 `*_tunnel` 字段是历史遗留的承载位。旧配置**不需要迁移**——`resolve_kind_falls_back_to_enabled_ssh` 等单测锁定了这条兼容行为。

> ⚠️ **互斥靠优先级实现，不靠校验。** 代码里没有任何地方拒绝「同时启用了两个隧道块」的配置：优先级低的会被**静默丢弃**。`SavedTunnel` 文档里「只有与 `kind` 匹配的那一个被填充」同样只是文档约定，手工编辑过的 `tunnels.json` 可以带着两个块被保存，错误要到建连时才以 `tunnelKind=X but xTunnel is missing or disabled` 的形式暴露。

### 2.2 启动是惰性的（关键差异）

`start_for_connection`（`tunnel/mod.rs:77`）分派到 `start_ssh` / `start_http_proxy` / `start_websocket`，三者都返回「改写后的 `ConnectionConfig` + `Option<Tunnel>`」。但**它们证明的事情完全不同**：

| | 启动时做了什么 | 「启动成功」实际证明了什么 |
|---|---|---|
| SSH | 拨跳板机 + 完成认证（`ssh_tunnel.rs:129-265`） | 跳板机可达且认证通过 |
| HTTP 代理 | **只绑定本机监听**（`http_proxy.rs:32-38`） | 仅「本地监听器已就绪」 |
| WebSocket | **只绑定本机监听**（`websocket.rs:36-42`） | 仅「本地监听器已就绪」 |

后两者**从不拨上游**。因此「隧道启动成功」对 HTTP/WebSocket 隧道不等于「代理/中继可达」——这正是 `test_tunnel` 及其 `verify_upstream` 存在的理由（见 §6）。

### 2.3 `rewrite_to_local`

`rewrite_to_local(config, local_port)`（`tunnel/mod.rs:191`）是驱动零改动承诺的落地点：

```rust
config.host = Some("127.0.0.1".to_string());
config.port = Some(local_port);
config.tunnel_kind = Some(TunnelKind::None);
config.ssh_tunnel = None;
config.http_proxy_tunnel = None;
config.websocket_tunnel = None;
```

最后四行不是冗余清理，而是**防重入护栏**：下游逻辑或驱动若再次读到隧道配置并尝试启动，会在回环地址上再套一层隧道。清空后 `resolve_tunnel_kind` 必得 `None`。

## 3. WebSocket 中继的两种模式

`WebSocketTunnelConfig.mode`（`tunnel_types.rs:70`）是普通 `String` 而非 Rust 枚举，取值在 `websocket.rs:124` 校验。TS 侧收窄为联合类型（`src/types/tunnel.ts:43`）。

| mode | 控制帧 | 目标寻址 |
|---|---|---|
| `datazen_v1`（默认） | 握手后发 `{"op":"open","host","port","id"}`，等 `{"op":"opened"}` 或 `{"op":"error"}`；EOF 时发 `{"op":"close"}`（`websocket.rs:288-353`） | 在 `open` 帧里携带 |
| `raw_binary` | **无控制帧**，直接进入二进制 | `raw_binary_url` 把它作为 query 参数注入 URL（`websocket.rs:193`） |

`raw_binary` 会**剥掉 URL 上原有的 `host` / `port` query 再追加自己的**，因此一个本身携带这两项的中继 URL 会被改写。

## 4. SavedTunnel 实体与持久化

### 4.1 引用而非拷贝

`SavedTunnel`（`tunnel_types.rs:80`）独立存放于 `tunnels.json`，连接侧只保留 `ConnectionConfig.tunnel_id`。保存连接时若设了 `tunnelId`，前端只写 `tunnelId` 与一个 `tunnelKind` 提示位，三个内联配置字段被主动省略（`src/lib/connectionFormModel.ts:117-136`）。

`resolve_saved_tunnel`（`src-tauri/src/tunnel/mod.rs`）在**每次建连时**把 `tunnel_id` 解引用回内联配置：

```text
ConnectionConfig { tunnel_id: "prod-bastion", ... }
   ↓ resolve_saved_tunnel → store.get_tunnel(id)
ConnectionConfig { tunnel_kind: Ssh, ssh_tunnel: {...}, ... }
   ↓ start_for_connection
ConnectionConfig { host: 127.0.0.1, port: <ephemeral>, tunnel_kind: None, ... }
```

两个刻意的设计：

- **解引用发生在建连，而不是打开表单时。** 改 `tunnels.json` 里的跳板机对所有引用它的连接立即生效，不必逐个改连接。
- **空字符串 `tunnel_id` 视为「无引用」**（`tunnels.rs:26-28`），与导出导入侧 `materialize_tunnel_refs` 的约定一致。而 `tunnel_id` 指向不存在的隧道会在建连时报 `tunnel id '<id>' not found`——**fail-closed，不静默降级为直连**。

### 4.2 加密与最小暴露

`store/tunnels.rs` 的凭据处理与连接密码同源（同一把 AES-256-GCM 主密钥，`store/mod.rs:161-199`）。**落盘前加密 6 个字段**：

`ssh.password`、`ssh.passphrase`、`ssh.jump.password`、`ssh.jump.passphrase`、`http_proxy.password`、`websocket.authToken`

读取后由 `load_tunnels_from_disk`（`tunnels.rs:12`）在应用启动时（`store/mod.rs:205`）一次性加载并解密进内存缓存。**解密失败降级为 `None` 并 `warn!`**，不让单条坏记录阻断整份加载。

已知主机校验文件为 `{appData}/ssh_known_hosts.json`（`src-tauri/src/tunnel/mod.rs`），采用 TOFU 策略：首次接受并记录指纹，不匹配则拒绝并回传精确错误（`ssh_tunnel.rs:37-95`）。

**凭据绝不下发到渲染进程。** 选择器用 `SavedTunnelSummary`（`tunnel_types.rs:99`），只有 `id` / `name` / `kind`；`summary_serializes_metadata_only` 单测逐个断言 `password` / `passphrase` / `authToken` / `ssh` / `httpProxy` / `websocket` 均不在序列化结果中。删除前的引用方查询用 `TunnelUsage`（`tunnel_types.rs:108`），同样只带连接 id 与名称。

## 5. 建连路径中的隧道

隧道在**物理 resource 打开之前**启动（`src-tauri/src/platform/session_backend/opening.rs`）：Driver 从未见过真实 `host:port`。建 session 和独立测试连接共用 `start_for_connection`：

| 入口 | 场景 | 隧道句柄归宿 |
|---|---|---|
| `platform/session_backend/opening.rs` | 建立 Runtime session 的物理 resource | Tunnel Arc 保存在 Runtime Resource；确认关闭后随资源释放 |
| `tunnel/mod.rs::test_connection` | 独立 Driver 连通性测试，不创建 Runtime session | `_tunnel` 局部持有，函数返回时 drop |
| `tunnel/mod.rs::test_tunnel` | 只测试隧道本身，不创建 Runtime session | 探测结束后 drop Tunnel |

## 6. IPC 与探测

`src-tauri/src/commands/tunnel.rs` 暴露 7 个命令：

| 命令 | 作用 | 生产调用方 |
|---|---|---|
| `get_tunnels` | 全量 `SavedTunnel`（**含解密凭据**） | **无**（见 §8.11） |
| `get_tunnel` | 按 id 取单条（含凭据） | 编辑对话框、复制、解绑回填 |
| `save_tunnel` | 新建 / 更新 | `tunnelStore` |
| `delete_tunnel` | 删除（**后端不做引用检查**） | `tunnelDeletion` |
| `get_tunnel_summaries` | **无凭据**摘要 | 设置页列表、连接表单选择器 |
| `get_tunnel_usage` | 该隧道被哪些连接引用 | 删除确认对话框 |
| `test_tunnel` | 独立连通性探测 | `TunnelTestDialog` |

每个文件都把逻辑放在可脱离 Tauri `State` 单测的 `*_impl` 自由函数里，`#[tauri::command]` 只是薄包装。注册表由 `commands/ipc_surface_tests.rs` 用 `include_str!` 断言，防止命令被静默摘除。

### `test_tunnel` 为什么必须存在

`tunnel::test_tunnel`（`src-tauri/src/tunnel/mod.rs`）用一份 `id = __tunnel_test__{id}` 的合成配置走完整链路；它不创建 Runtime session，也不解析数据库 driver——只有隧道字段和转发目标会进入隧道运行时。探测后立即 drop Tunnel，靠 `Drop` 清理转发器、监听端口、任务与 SSH 会话。

返回的是**实测耗时**（`Duration`），不只是布尔值，因为「可用」和「够快」是两个不同的问题。

按 kind 分派探针：

- `Ssh` → `start` 本身已拨号并认证，建隧道即探针（`tunnels.rs:148`）。
- `HttpProxy` → `verify_http_proxy_upstream`：拨代理 + 可选 TLS + 真实发一次 `CONNECT`，每段都被 `connect_timeout_secs` 兜住（`http_proxy.rs:215-249`）。
- `WebSocket` → `verify_websocket_upstream`：完整 WS 握手；`datazen_v1` 模式额外走一次 `open`/`opened` 往返再发 `close`（`websocket.rs:173-191`）。
- 解析结果没有隧道 → 报错而非报成功。`tunnelKind=none` 会在这里**假阳性**，代码显式堵住了这个口子。

`perform_connect` 自身**没有内部截止时间**，因此 `perform_connect_within`（`http_proxy.rs:373`）是数据路径与探测路径共享的唯一兜底——「接受 TCP 但永不回应」的代理否则会永久挂起（`commands/tunnel.rs:544` 的回归测试锁定）。

## 7. 隧道与 session 生命周期

`Tunnel`（`src-tauri/src/tunnel/mod.rs`）枚举持有三种运行时句柄，统一暴露 `local_port()`。它保存在 `DesktopSessionBackend::Resource._tunnel`，不作为可执行句柄暴露给 consumer。没有独立的 `close_tunnel()` 路径：

- Runtime 确认物理 resource 关闭后移除 `Resource` → Tunnel 随值 drop；资源进入 quarantine 或复用状态时 Tunnel 继续存活。
- Runtime idle reclamation 默认在 30 分钟 session deadline 后关闭 resource；桌面每 15 秒扫描。过期会话失效，调用方显式建新 session，不保留 owner map 做同 ID 自动重连。
- **持有即存活**是这个子系统唯一的所有权模型；任何「按需保活/惰性关闭」的想法都需要先引入显式引用计数，目前不存在。

## 8. 前端

| 文件 | 职责 |
|---|---|
| `src/types/tunnel.ts` | Rust 类型的 TS 镜像（`camelCase`，含 `TunnelSource`） |
| `src/commands/tunnel.ts` | IPC 客户端 |
| `src/stores/tunnelStore.ts` | 全应用共享的**摘要**集合；去重、失败降级为空集合 |
| `src/lib/tunnelDraft.ts` | 纯字符串草稿模型 + 校验键，保证端口/超时可直接编辑 |
| `src/lib/tunnelDeletion.ts` | 解绑→删除的事务式流程（失败逆序回滚） |
| `useTunnelFormState.ts` | `none` / `saved` / `inline` 三态状态机 |
| `SshTunnelFields` / `HttpProxyTunnelFields` / `WebSocketTunnelFields` | 三类字段组（`direct` 无字段） |
| `tunnelFieldContracts.ts` | 三组受控契约，让设置页编辑器复用**同一批组件**（而非第二份拷贝） |
| `ConnectionAdvancedSettings.tsx` | 连接表单里的隧道区块 |
| `TunnelSettingsSection` / `TunnelEditDialog` / `TunnelDeleteDialog` / `TunnelTestDialog` | 设置页管理面 |

### 异步竞态的三层防护

1. **store 去重**：`loaded` 标志 + 早退，并发消费者只发一次 IPC。
2. **首个消费者触发加载**：无 app 根预加载，`useTunnelFormState` / `TunnelSettingsSection` 各自在 mount 时 `load()`。
3. **悬空引用门控**（`useTunnelFormState.ts:176-182`）：`tunnelId` 存在、集合已加载、无错误、但实体取不到 → 才判定为悬空。**加载失败不能等同于「隧道不存在」**，否则一次网络抖动就会让用户无法保存本来合法的连接。

### 引用完整性只在前端

后端 `delete_tunnel` 是裸的 `retain`（`store/tunnels.rs:181`），不做任何引用检查。完整性是**纯前端责任**：`tunnelDeletion.ts:50-105` 先逐个解绑，任一解绑失败就逆序回滚并中止删除。任何新增的 `delete_tunnel` 调用方（脚本、未来的宿主面板）都会重新引入悬空引用缺陷。

## 9. 已知边界

以下是当前实现的客观事实，接手时不要误读为「已保证」：

1. **不附带 WebSocket 中继服务端。** `tunnel/websocket.rs` 只是客户端；`datazen_v1` 所需的 `open` / `opened` / `error` / `close` 四操作 JSON 协议，DataZen **没有实现**任何一端。中继需自行提供。
2. **内联（非保存）的 HTTP 代理密码与 WS token 明文落盘。** `store/connections.rs:64-126` 只加密 `conn.password` 与 `ssh_tunnel.password/passphrase`——**既不覆盖 jump 跳板，也不覆盖 `http_proxy_tunnel.password` 与 `websocket_tunnel.authToken`**。同一份凭据存进 `tunnels.json` 是加密的。这是一处真实的加密覆盖缺口，不是设计取舍。
3. **`headers` 一律明文**（两个存储都如此）。把密钥放进自定义 header 就会明文落盘。
4. **无文件权限加固。** 整个 store 的写盘路径只有「临时文件 + rename」，没有 `set_permissions`；`tunnels.json`（含加密密文）与 `ssh_known_hosts.json` 的权限继承 umask。
5. **解密失败静默置空。** 密钥轮换后隧道会以「配置仍在、凭据为空」的降级形态继续工作。
6. **`SavedTunnel` 的互斥从不校验**（§2.1），重复实现散落在后端优先级、前端 `effectiveTunnelKind`、写侧剪枝三处且互不联通。
7. **三种探针的证明力不等价。** SSH 只到跳板机（不开 `direct-tcpip` 通道），`raw_binary` 只到中继握手。UI 用 `scope.*` 文案逐 kind 明说了这件事——不要把「探测通过」理解成「数据库可达」。
8. **导出格式只能表达 SSH。** `materialize_tunnel_refs`（`connection_import/ipc.rs:189-227`）只materialize SSH，HTTP/WebSocket 隧道与悬空引用会**静默丢失**。导出再导入会丢掉这两类隧道。
9. **`raw_binary` 会改写中继 URL 的 query**，且无法上报通道级失败——中继侧拒绝目标与成功建隧道不可区分，只能等数据库协议超时。
10. **转发器的错误只进日志。** `http_proxy.rs:91` / `websocket.rs:93` 记 `tracing::error!` 后返回；驱动只会看到自己的 socket 错误，没有从转发器回传的通道。
11. **`getTunnels`（含解密凭据的完整列表）在生产代码中无调用方。** 所有列表路径都走 `getTunnelSummaries`。该命令仍暴露在 IPC 上。
12. **无 proxy/WS 的能力门控。** 只有 `supportsSSH`（见下），`httpProxy` 与 `websocket` 对所有驱动无条件提供，包括文件型驱动。

## 10. 能力位：Rust 侧没有，TS 侧只有一个

- **Rust 契约零隧道概念。** `DatabaseDriver` trait 与 `PROTOCOL_VERSION = 6` 完全不含隧道方法；`packages/drivers/*/src/` 中没有任何驱动读取或声明隧道支持（仅有测试夹具把 `ConnectionConfig` 的五个隧道字段置 `None`）。这与 `supports_offset()` / `supports_explain()` 的模型不同，勿混。
- **`DB_REGISTRY[...].supportsSSH` 是 UI 元数据，不是协议契约。** 它只决定**连接表单是否渲染 SSH 区块**，共三处消费：内联 kind 下拉列表（`ConnectionAdvancedSettings.tsx:53`）、SSH 字段挂载守卫（`:326`、`SshTunnelFields.tsx:28`）、切换驱动时的降级（`useConnectionForm.ts:443`）。取 `false` 的是无 host:port 形态的驱动（sqlite、kiwi）。它**不参与 Rust 契约、不影响隧道运行时**。
- 设置页的隧道编辑器**硬编码 `supportsSSH: true`**（`TunnelEditDialog.tsx:129`）——保存的 SSH 隧道不受任何驱动开关影响，这与「隧道是宿主层实体」的定位一致。
