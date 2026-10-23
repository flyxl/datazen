# MCP 模块

> [返回架构总览](../README.md)

### 1.1 MCP Server

DataZen 作为 MCP Server 暴露数据库操作能力给外部 LLM 应用（Claude Desktop、Cursor 等）。共享的工具/资源/提示词逻辑位于 `packages/feature-core/src/mcp/`；`src-tauri/src/mcp/` 负责宿主传输、认证、权限、会话与 Runtime 接线。

```
packages/feature-core/src/mcp/   # 共享 tool/resource/prompt 逻辑与权限策略
src-tauri/src/mcp/
├── mod.rs          # 启动/停止与 stdio 传输装配（含断连清理）
├── auth.rs         # stdio 本地 token 认证（`mcp.token` + `DATAZEN_MCP_TOKEN`）
├── allowlist.rs    # 连接白名单（空 = 拒绝一切）
├── permission.rs   # 三档权限 + SQL/工具/资源门控
├── server.rs       # MCP Server 实例与工具清单 `MCP_ALL_TOOLS`
├── server/         # 有状态会话等 Server 测试
├── tools.rs        # 工具实现的宿主绑定（含 workflow 与有状态会话工具）
├── tool_help.rs    # 工具说明、参数示例与输入 schema
├── types.rs        # 工具入参 DTO（snake_case）
├── handler.rs      # `call_tool` 门控：注册表 → 权限 → 路由
├── session.rs      # 有状态会话引用注册表（仅内存，不落盘）
├── resources.rs    # Resources
├── prompts.rs      # Prompts
├── contract.rs     # 外部契约快照与 golden 测试
├── fixtures/       # `mcp_external_contract.json` golden 契约
└── client.rs       # MCP Client 管理
```

`permission.rs` 只保留 MCP 专属的权限策略（三档模式、白名单、工具/资源门控）。SQL 主关键字分类（`sql_main_keyword`、`tokenize_sql`、`find_main_verb_index`）与注释剥离等**与传输无关**的 SQL 判读能力已下沉到共享层 `packages/kernel/src/sql_guard/`，宿主直接复用，未来服务端入口共用同一份实现。

Workflow 引擎本身见 [`workflow` 模块](./workflow.md)，MCP 仅通过 Tools/Resources 暴露。

**信任模型（Settings → MCP Server）：**

| 控制 | 说明 |
|------|------|
| `mcpPermissionMode` | `read_only` / `safe_write`（默认）/ `high_risk_write` |
| `mcpDisabledTools` | 工具 denylist |
| `mcpAllowedConnectionIds` | 连接白名单；**空数组 = 暴露全部已保存连接** |

修改权限/白名单/工具后，嵌入式 MCP Server 会自动热重载；独立 `datazen --mcp` 进程需重启后生效。

**stdio 本地认证（`--mcp` / `--mcp-stdio`）：**

| 机制 | 说明 |
|------|------|
| `{appData}/mcp.token` | 首次 headless 启动时创建（`0600`）；持久化随机 token |
| `DATAZEN_MCP_TOKEN` | MCP 客户端 env 必须与此文件内容一致，否则进程拒绝启动 |
| 嵌入式 GUI | Settings 内嵌 duplex transport **不**走 token（同进程信任边界） |

Headless 首次 bootstrap 示例：

```bash
# 1) 首次运行会创建 mcp.token 并以 exit 1 提示配置 env
datazen --mcp-stdio

# 2) 将 token 写入 MCP 客户端配置（Claude Desktop / Cursor 等）
export DATAZEN_MCP_TOKEN="$(tr -d '\n' < "$APP_DATA/mcp.token")"
datazen --mcp-stdio
```

**Resource 与 Tool 策略对齐：**

| 控制 | Tools | Resources |
|------|-------|-----------|
| `mcpAllowedConnectionIds` | 带 `connection_id` 的工具 | `connections`、`query-history`（按条目 `connectionId` 过滤）、`schema/{connectionId}/…` |
| `read_only` | 屏蔽 `query` / `run_workflow` | `query-history` 从列表隐藏且读取返回 `[]`（不泄露 SQL） |
| `safe_write` / `high_risk_write` | SQL 分类门控（分类器来自 `packages/kernel/src/sql_guard/`） | 全量 resources（仍受 allowlist 约束） |

**Server Tools（无状态）：**
- `list_connections` / `list_databases` / `list_tables` / `search_tables` / `query` / `get_schema`
- `explain_query` / `describe_table` / `list_workflows` / `run_workflow`

**Server Tools（有状态，`open_session` → `use_session` → `close_session`）：**
- `open_session`：在显式目标上开会话，返回不透明 `clientSessionRef`
- `use_session`：在该引用上执行一条 Driver Command
- `close_session`：释放该引用（幂等）

三者只存在于**这个 MCP 进程实例**的内存里，语义见 [§1.4 有状态会话工具](#14-有状态会话工具)。

**Data Sync (not exposed via MCP in V1):** GUI Data Sync (`compare_data_sync` / `apply_data_sync` / `execute_data_sync`) is intentionally **not** registered as MCP tools. Row-level sync apply is high-risk write; compare would require runtime `dbSessionId` pairs and table mapping context. External agents should use read-only schema tools (`list_tables`, `get_schema`) plus the GUI or future dedicated APIs if added. Any future MCP exposure must reuse the same permission model: compare-only tools allowed in `read_only`; apply/execute tools blocked by default (`mcpDisabledTools`) and must honor target connection `read_only` gates.

`search_tables` is `list_tables` 的补充：当数据库表数量很大（>500）时，LLM 优先使用 `search_tables` 按关键字搜索匹配的表，而不是列出全部表名。

**Server Resources:**
- `datazen://connections` / `datazen://query-history`
- `datazen://schema/{connectionId}/{database}` / `datazen://workflows`

> 资源输出字段同样遵循新术语：`query-history` 条目序列化为 `connectionId`（camelCase），不再出现旧键 `configId`。

**Server Prompts:**
- `nl2sql` / `diagnose_error` / `explain_plan`

### 1.2 MCP Client

连接外部 MCP Server，获取工具能力：
- stdio transport 支持（TokioChildProcess）
- 30s 连接超时保护
- connect 失败时自动进程清理
- 配置持久化于 `AppSettings.mcpClientServers`（command / args / env / enabled）
- 应用启动时对 `enabled` 配置非阻塞 auto-reconnect
- 已连接工具以 `mcp/{serverId}/{toolName}` 命名空间注册，供 AI Chat tool loop 调用

#### MCP Client AI 集成

Settings → **External MCP Servers** 管理已保存配置与运行时连接态（分离 UI）：

| 层 | 职责 |
|----|------|
| `McpClientManager` | stdio 子进程生命周期、工具发现、`call_tool` |
| `commands/mcp.rs` | IPC：`mcp_client_connect` / `disconnect` / `list` / `tools` / `call_tool` |
| `commands/ai/mod.rs` | `collect_mcp_tool_definitions()` 合并 DB tools + MCP tools；`run_streaming_tool_loop` 路由 `mcp/*` 前缀 |
| 前端 `McpClientSection` | 配置 CRUD、env 编辑、连接/重试、运行时工具列表 |

AI Chat（`ai_chat`）在已连接外部 MCP 时将 MCP 工具与内置 DB tools 一并注入 Provider；模型调用 `mcp/…` 工具时由后端经 `McpClientManager` 转发至对应子进程。详见 [`ai.md` — AI Chat MCP 工具](./ai.md#111-ai-chat-mcp-工具)。

### 1.3 连接 ID（connection_id vs db_session_id）

MCP tools 与 GUI IPC 使用相同的 ID 边界，但走各自的 Runtime session / execution path（详见 [connectionId 与 dbSessionId](../naming.md)）：

- **`connection_id`**：`connections.json` 中的持久化连接 UUID；`list_connections` 返回值；MCP tools / prompts / AI db tools 入参。
- **`db_session_id`**：GUI `connect` 返回的运行时会话 ID；GUI IPC（`query`、`get_schema` 等）在已连接后传此 ID。

> 历史演进：早期版本 MCP 工具入参叫 `config_id`、运行时句柄叫 `connection_id`。现行为上表术语，且 MCP 侧不保留 `config_id` 兼容别名——旧键会被直接拒绝（见 CHANGELOG 破坏性变更）。

MCP Server 与 GUI 跑在同一个进程里：内嵌 server 启动时复制 `AppState`，共享 Store、PlatformAdapter、会话事务表与查询执行注册表。DB tools 通过共享 `DatabaseToolHost` 的 Runtime execution path 按 `connectionId` 解析配置并执行；不借用 GUI editor 的 `dbSessionId`，也不使用 Host `ConnectionManager`。有状态工具（`open_session` / `use_session` / `close_session`）由 MCP 自己的 opaque reference 绑定 Runtime session，语义见 [§1.4](#14-有状态会话工具)。

由此得到三条已实现的行为边界：

- **MCP 用持久化 id 入参，经 Runtime 执行**。每个 DB tool 先过连接白名单，再由 `DatabaseToolHost` 使用 Runtime resource execution path 取得执行所需资源；不与 GUI 的编辑器 session 共用事务状态。
- **资源生命周期归 Runtime**。每次操作的执行资源与显式 MCP session 都由 Runtime owner / session 生命周期管理；到期后明确失效，不以原 `dbSessionId` 透明重连。
- **目标完全来自显式参数**。database / schema 按「调用方参数 → 连接配置回退 → 驱动约定」的顺序解析，**从不读取会话当前所在的库**——因为 MCP 是「人在工具调用里打字写表名」的入口，借用会话默认库会让同一份提示在不同会话下得到不同结果。

连接白名单是显式配置：白名单为空时按**拒绝一切**处理，不是「空即放行」。被拒绝的调用返回的是明确的权限错误，而不是静默返回空结果。

MCP 侧只有一个执行类入口会绕过 db tools：`run_workflow` 把 `connection_id` 与 MCP 自己的权限模式交给与 GUI 共用的 Workflow 执行器。

### 1.4 有状态会话工具

`open_session` → `use_session` → `close_session` 是**显式的三步协议**，也是 §1.3 那条「无状态工具不做会话管理」的唯一例外。无状态工具拿的是调用方完整写出的目标；有状态工具让调用方先把「开一条会话」这件事本身做成一个可引用的对象，后续命令只带引用。

- **`open_session(connectionId, database, schema?, ttlMs?)`**：`database` 是必填的，不接受「省略即用当前库」——省略会被明确拒绝，而不是回退到 GUI 当前目标。成功后返回 `{"clientSessionRef", "state", "contextRevision", "expiresInMs"}`。
- **`use_session(clientSessionRef, command, input, timeoutMs?)`**：在映射到的那条会话里跑一个 Driver Command。`query` / `execute` 仍要过 SQL 权限闸（`read_only` 模式下照样被拦），结果行从执行产物（artifact）读出，最多返回 200 行并在 `rowsTruncated` 标记截断。
- **`close_session(clientSessionRef, mode?)`**：关闭会话并删掉映射。`mode` 取 `requireNoTransaction`（默认）或回滚未结束事务后关闭的 `rollbackAndClose`，其他字符串直接拒绝，不会静默降级成「丢弃未提交工作」。

`clientSessionRef` 是**不透明引用**：客户端拿到的 uuid 无法反推 `dbSessionId`、连接或主机进程。`reference → handle` 的映射只存在于 [src-tauri/src/mcp/session.rs](../../../src-tauri/src/mcp/session.rs) 的内存里，绑定 principal、后端生成的 clientInstanceId、运行句柄、`contextRevision` 与到期时刻；它**不写入** Workflow 定义、执行历史或数据库配置，因此进程退出即全部失效，也不会跨 MCP 进程共享。

归属与失效判定有三条硬边界：

1. **clientInstanceId 由后端在 server 启动时生成**（`DataZenMcpServer::new` 调 `new_client_instance_id()`），**绝不取自请求参数**；每个 MCP server 实例一个，调用上下文也由适配器本地组装（组织与主体仍是桌面启动身份）。因此「谁开的会话」不是客户端自称，而是后端记的账。
2. **引用只在归属它的那个 clientInstanceId 下可用**。`use_session` 先查映射再谈别的：他人持有的引用返回 `SessionLost`，已关闭、已过期的引用同样返回 `SessionLost`——丢失的映射**不会用同一个引用重建**，客户端必须重新 `open_session`。查映射发生在 SQL 闸与网关调用之前，所以 `SessionLost` 这条路径根本不需要连数据库。
3. **TTL 有上下限**：`sanitize_ttl` 把缺省与显式 0 归一到 300 秒，把超过 3600 秒的请求截到 3600 秒。到期 lookup 和独立 worker 将 binding 撤销为不可访问的 cleanup record，保存原 client owner 与 Runtime handle；不依赖后续工具请求或 Runtime 的空闲回收。

`use_session` 还按映射里记的 `contextRevision` 做乐观并发（`with_expected_context_revision`），即调用方看到的上下文版本一旦被人改过，这次命令会被拒绝而不是默默换目标执行。

`close_session` 是幂等的，而且**对未知、他人、已过期三种情况一律回 `{"closed": false}`**，不区分「从没存在」与「曾经存在」，避免用过期的引用反推服务端状态；网关拒绝关闭时映射**保留**，让客户端可以重试——顺手删掉就等于泄漏了一条再也无法关闭的数据库会话。

客户端断开时，`release_sessions` 先禁止该 client 新增有状态请求，并撤销所有归属 binding；在途 open 返回的资源也转入清理，不向已断开的 client 发新引用。 `session_open.rs` 由独立任务持有完整 Runtime 获取过程：MCP 请求取消只丢弃等待方，不取消获取 owner；未消费的回复由 RAII guard 撤销 binding，并交给同一 cleanup worker。因而即使请求在获取中被取消、断连后才拿到资源，也不会丢失原 owner/handle。成功交付引用后 guard 解除，正常 TTL 与显式 close 语义继续生效。每个 client 至多一个 `session_cleanup.rs` worker，以原 owner 对原 Runtime 句柄调用 `RollbackAndClose`。它不会按 connectionId 扫描其他消费者资源。只有 `resourceRelease=Confirmed` 删除 cleanup record；错误、Pending、Quarantined 均保留，按 100ms 起、上限 30s 退避。未知效果不改写为已回滚，不重放应用 SQL。已隔离且无法确认物理释放的 owner 记录保持在进程内，不能宣称已清理；进程退出不持久化这些句柄。显式 close 收到未确认释放时返回 `closed:false, cleanupPending:true` 并转入同一路径。worker 在 binding 和 cleanup record 均为空后退出。

请求取消/断连与获取交错、晚到资源及预算回收、正常交付后幂等关闭的 Host Runtime fixture 回归在 `src-tauri/src/mcp/session_open.rs`；这些受控测试不替代 PG/MySQL 实库/传输旅程。Runtime-backed TTL、active transaction 断连回滚、预算/registry 释放、其他 client 隔离与未确认关闭 owner 保留测试在 `src-tauri/src/mcp/session_cleanup.rs`。协议回归测试落在 `src-tauri/src/mcp/server/tests_stateful.rs`（他人引用不可用、未知引用不重建、TTL 过期、关闭被网关拒绝时保留映射、重复关闭幂等、断开只释放自己的映射、归属身份与 Job owner 越权被拒）。

