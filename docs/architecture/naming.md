# ID 术语规范 — connectionId 与 dbSessionId

> [返回架构总览](README.md)

DataZen 中有两类容易混淆的标识符。本文是它们的唯一权威定义：**`connectionId` = 持久化连接配置 id；`dbSessionId` = 运行时数据库会话 id**。所有前后端代码、IPC 契约、文档与外部接口均须遵循本页术语。

## 1. 定义与生命周期

| | **connectionId / connection_id** | **dbSessionId / db_session_id** |
|------|------|------|
| 含义 | 一条持久化连接配置的 id（`ConnectionConfig.id`） | 一次已建立数据库会话的运行时句柄 id（`ConnectionHandle.id`） |
| 存储 | `connections.json`（AES-256-GCM Store），**落盘** | 内存会话池，**永不落盘** |
| 稳定性 | 应用重启后不变，用户可长期引用 | 断开/驱逐后由守护映射按需复活（复用原 id）或随销毁失效 |
| 前端形态 | `connectionId`（camelCase） | `dbSessionId` |
| Rust 形态 | `connection_id`（snake_case） | `db_session_id` |

### connectionId 生命周期

- **生成点**：用户新建连接配置时由前端生成（`src/components/connection/shared.tsx` 的 `newId()`，形如 `conn_xxxxxxxx`），经 `save_connection` IPC 持久化。
- **存续**：随 Store 持久化；重启、编辑配置内容（改名/改密码等）都不会改变它。
- **销毁点**：用户删除该连接配置（`delete_connection`）。删除前若存在派生会话，应先经 UI 断开。

### dbSessionId 生命周期（后端锚点：`src-tauri/src/services/connection_manager.rs`）

- **生成点**：`ConnectionManager::connect(connection_id)` —— 从 Store 读配置 → 按需建 SSH 隧道 → `driver.connect(&effective_config)` 得到 `ConnectionHandle`，其 `id` 即 `db_session_id`；随后注册进活动会话池并写入归属映射。
- **复活**：空闲驱逐后首次 `get_session(db_session_id)` 触发自动重连，**复用原 `db_session_id`**（见第 2 节）。
- **销毁点**：① `disconnect()`（侧边栏断开，同时移除归属映射）；② `release()` 使引用计数归零后拆除；③ 应用退出 `shutdown()` 清空全部会话。

### dbSessionId 生命周期（前端锚点）

- `src/stores/activeConnectionStore.ts`：`connections` 以 `connectionId` 为 key，每个 `ConnectionEntry` 持有 `dbSessionId`（连接中/失败时为空串）。`connect()` 调 `connect(connectionId)` IPC 拿到 `dbSessionId` 后写入 entry，并广播跨窗口事件 `datazen:connection-ready { connectionId, dbSessionId }`；断开走 `disconnect(dbSessionId)`。
- `src/stores/panelStore.ts`：每个 SQL 面板绑定 `{ connectionId, dbSessionId }`——执行/取消查询走 `panel.dbSessionId`，查询历史与收藏按 `panel.connectionId` 过滤。

## 2. 流转全景

```text
        connections.json（Store，持久化）
        ConnectionConfig.id ──────────────► 这就是 connectionId
                     │
                     │ store.get_connection(connection_id)
                     ▼
 GUI                 connect(connection_id)                【后端】
 ┌──────────────────────────────────────────────────┐
 │ activeConnectionStore.connect(config)            │
 │   key = connectionId                             │
 │        │  IPC 'connect'                          │
 │        ▼                                         │
 │   ConnectionManager::connect(connection_id)      │
 │        ├─ maybe_start_tunnel(config)             │
 │        ├─ driver.connect(&effective_config)      │
 │        │    └─ ConnectionHandle.id ══════════►   dbSessionId【生成点】
 │        └─ session_owner_map[db_session_id] =     │
 │                  connection_id  【归属映射】       │
 │        │                                         │
 │        ▼  返回 db_session_id                      │
 │   entry.dbSessionId = db_session_id              │
 │   广播 datazen:connection-ready {connectionId,    │
 │        dbSessionId}                              │
 └──────────────────────────────────────────────────┘
                     │
                     ▼
   后续一切对会话的操作（SQL 执行 / cancel / Schema 浏览 /
   execute_driver_command）一律传 dbSessionId
```

空闲驱逐与自动重连（`cleanup_idle_connections` 默认每 5 min 扫描、空闲 30 min 驱逐）：

```text
 空闲驱逐：从活动池 connections 移除 ActiveSession
           session_owner_map 条目【保留】
                │
                ▼  之后任一次 get_session(db_session_id) 未命中活动池
   reconnect(db_session_id)
        ├─ session_owner_map[db_session_id] ──► owning connection_id
        ├─ 从 Store 重读该 connection_id 的【最新】配置（空闲期改动生效）
        └─ driver.connect(...) ──► 以同一个 db_session_id 重新注册（id 不变）
```

关键不变式：**驱逐不换 id**。调用方持有的 `db_session_id` 在驱逐→重连全程有效；只有显式 disconnect / shutdown 才使其失效。

## 3. 该用哪个？决策表

| 场景 | 使用 | 理由 |
|------|------|------|
| 连接配置 CRUD（保存/编辑/复制/删除）、鉴权与凭据来源 | `connectionId` | 凭据挂在持久化配置上 |
| 归属判断（这条会话属于哪个连接，`session_owner_map` 反查） | `connectionId` | 归属关系以配置为锚 |
| 调度绑定、Workflow 默认/Step connection 字段 | `connectionId` | 编排引用的是"哪个连接"，非某次会话实例 |
| 查询历史/收藏过滤、MCP/AI 工具入参（契约面） | `connectionId` | 跨重启可稳定引用 |
| UI 归属（侧边栏分组/展开态、面板归属连接） | `connectionId` | UI 状态跟随配置 |
| 对已建立会话的一切操作（SQL 执行、cancel、流式查询、Schema 浏览、`execute_driver_command`、导出执行体） | `dbSessionId` | 操作落在具体会话/连接池上 |

一句话经验法则：**先问"哪个连接"还是"哪条会话"。** 描述"哪个连接"→ `connectionId`；操作"哪条已建立的会话"→ `dbSessionId`。拿不准时记住：**`dbSessionId` 永不落盘**——任何需要持久化、跨窗口长引用或进入日志审计的场景都只能用 `connectionId`。

## 4. MCP / Workflow 入参（仅 connectionId）

`ConnectionManager::resolve_session_for_connection(connection_id)` 是 MCP / Workflow / db_tools 专用的会话解析入口：

1. 入参**只能是**持久化 `connection_id`（`list_connections` 返回值、Workflow YAML 的 `connection` 字段）；
2. 内部经 `get_or_connect_session` 按 `session_owner_map` 复用已有会话，或按需建连；
3. 返回运行时 `db_session_id` 及 driver / handle。

**GUI IPC 路径（schema / query / export / execute_driver_command / get_connection_commands 等）一律调用 `get_session(db_session_id)`，传入 `connectionId` 将明确报错**（`DbSessionNotFound`，提示可能误传了 connectionId）。

| 场景 | 位置 | 说明 |
|------|------|------|
| Workflow runtime | `src-tauri/src/workflow/command_runtime.rs` / `executor.rs` | Step / Workflow 的 `connection` 字段存配置 id；执行时经 `resolve_session_for_connection` 按需建连或复用 |
| MCP DB tools / AI db tools | `src-tauri/src/services/db_tools.rs` `resolve_connection` | 外部契约传 `connection_id`；内部经 `resolve_session_for_connection` 转换 |
| 插件桥透传 | `src/lib/extensionBridge.ts` `handleCommandInvoke` | 插件契约传 `connectionId`；宿主从 `activeConnectionStore` 解析 live `dbSessionId`，**无活动会话则拒绝** |

### P1 迁移摘要（cr-p1-session-id / cr-p1-mcp-connection-only）

| 变更前 | 变更后 |
|--------|--------|
| GUI IPC 经 `resolve_session` 静默把 `connectionId` 当会话 id 并自动建连 | GUI IPC 仅接受 `dbSessionId`；误传 `connectionId` → `DbSessionNotFound` |
| `ConnectionManager::resolve_session` / `resolve_session_for_mcp` 双模（先 dbSessionId 再 connectionId） | 重命名为 `resolve_session_for_connection`，**仅接受 connectionId** |
| `WorkflowChatPanel` 把 `connections[].id`（配置 id）塞入 `dbSessionId` | 从 `activeConnectionStore` 解析已连接会话的 `dbSessionId` |
| `get_connection_commands` IPC 参数 `connectionId` | 改为 `dbSessionId`；未连接时 Workflow 编辑器回退 `get_driver_commands(driverType)` |

规则：**新 GUI 代码必须按第 3 节决策表显式传入正确类型的 id**；MCP / Workflow / db_tools 入参一律为 `connectionId`，不得传 `dbSessionId`。

## 5. 历史改名映射

旧术语 → 新术语对照（本次统一重构，**不留任何兼容别名**）：

| 旧标识符 | 新标识符 | 说明 |
|----------|----------|------|
| `configId` / `config_id` | `connectionId` / `connection_id` | 持久化连接配置 id 改名（原语义不变） |
| `connectionId`（旧·运行时会话含义） | `dbSessionId` / `db_session_id` | 运行时会话 id 改名；注意旧 `connectionId` 与新 `connectionId` **不是同一个东西** |
| Schema Diff v1 载荷 `configId` | v2 `sourceConnectionId` / `targetConnectionId`（`version: 2`） | v1 格式导入被明确拒绝 |
| SyncTask 持久化字段 `sourceConfigId` / `targetConfigId` | `sourceDbSessionId` / `targetDbSessionId` + `sourceConnectionId` / `targetConnectionId` | 会话与归属两类字段分离 |
| 插件桥 `command.invoke` 参数键 `configId` | `connectionId` | plugin-sdk 类型同步更新，无别名 |
| SQLite 历史库列 `query_history.config_id` 等 | `connection_id` | 启动时一次性迁移（schema v3 → v4），数据保留 |

破坏性变更全量清单与迁移指引见 [CHANGELOG.md](../../CHANGELOG.md)。后端的权威实现注释另见 `src-tauri/src/services/connection_manager.rs` 顶部 "ID terminology" 文档块。

## 6. 守护机制

`scripts/check-id-terminology.mjs` 静态扫描 `src/`、`packages/`、`e2e/`（排除 `node_modules` 与 codegen 生成的 `generated*.ts`）：出现禁用模式（旧 `*ConfigId` 命名、把配置 id 装进会话键的装反形态等）且不在白名单内即失败并列出 `文件:行号:内容`。白名单逐条注释了合法保留理由（如描述 v1 拒绝行为的历史格式说明）；新增豁免必须附带理由，防止守护被静默稀释。

```bash
npm run test:ids        # 本守护脚本（package.json script）
```
