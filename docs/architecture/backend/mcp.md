# MCP 模块

> [返回架构总览](../README.md)

### 1.1 MCP Server

DataZen 作为 MCP Server 暴露数据库操作能力给外部 LLM 应用（Claude Desktop、Cursor 等）。**目标分层**（DDD 目标架构 §3.3）：工具业务语义位于 `crates/datazen/src/mcp/`，宿主只留传输、认证与装配。**当前事实**（2026-11-07 核对）：共享层已承载连接/库/namespace scope、权限策略、批量 SQL 与 Redis 只读用例、逐次审批契约与审计决策词汇、无状态工具用例与有状态会话语义；`#[tool]` 宏、Resources、Prompts、stdio / Streamable HTTP 传输、MCP Client 与会话清理 worker 仍在桌面宿主 `crates/datazen-desktop/src/mcp/`。Streamable HTTP 是桌面设置页的显式 opt-in，仅绑定 IPv4 loopback。其他宿主目前仍无法只替换 Server 形态就复用完整 MCP 能力。

```
crates/datazen/src/mcp/   # 已共享：scope、权限、批量/Redis、审批/审计词汇、工具与会话语义
├── domain/
│   ├── allowlist.rs      # 连接白名单（空 = deny-all）
│   ├── approval.rs       # Allow once / Deny / Timeout 契约与安全预览
│   ├── batch.rs          # 批量 SQL 分帧、默认只读拒绝
│   ├── policy.rs         # 三档权限、只读分类与工具/资源门控
│   ├── sql_scope.rs      # 连接/库/namespace scope 校验
│   └── mod.rs            # 白名单导出 + execution::domain::permission 旧路径垫片
└── application/
    ├── describe.rs       # 表描述格式化
    ├── tools.rs          # 无状态工具用例（元数据/查询/工作流）
    ├── batch.rs          # execute_batch 事务/逐条结果/错误脱敏
    ├── redis.rs          # redis_command 只读命令分类与 key/limit/timeout 计划
    ├── session.rs        # 会话注册表、TTL、归属校验、清理登记
    └── session_tools.rs  # open_session / use_session / close_session 编排

crates/datazen-desktop/src/mcp/
├── mod.rs          # 启动/停止与 stdio 传输装配（含断连清理）
├── auth.rs         # stdio 本地 token 认证（`mcp.token` + `DATAZEN_MCP_TOKEN`）
├── http.rs         # opt-in Streamable HTTP：127.0.0.1 bind、Bearer 认证、token 轮换、会话回收
├── (allowlist/permission 在 crates/datazen/src/mcp/domain/，宿主经 `datazen::mcp::` 引用)
├── server.rs       # MCP Server 实例与工具清单 `MCP_ALL_TOOLS`
├── server/         # 集成与有状态协议测试（tests_integration.rs / tests_stateful.rs）
├── tools.rs        # 工具实现的宿主绑定（含 workflow 与有状态会话工具）
├── tool_help.rs    # 工具说明、参数示例与输入 schema
├── types.rs        # 工具入参 DTO（snake_case）
├── handler.rs      # `call_tool` 门控：注册表 → 权限 → 白名单 → 路由
├── session.rs      # `datazen::mcp::session` 的兼容再导出
├── session_open.rs # 有状态会话打开、取消与未交付清理
├── session_cleanup.rs # 会话清理 worker（TTL / 断连 / 未确认释放）
├── resources.rs    # Resources
├── prompts.rs      # Prompts
├── contract_tests.rs # 外部契约快照与 golden 测试
├── fixtures/       # `mcp_external_contract.json` golden 契约
└── client.rs       # MCP Client 管理
```

`crates/datazen/src/mcp/domain/` 只再导出 MCP 专属的权限策略（三档模式、白名单、工具/资源门控）；权限模式与 SQL 分类的唯一定义在共享层 `crates/datazen/src/execution/domain/permission.rs` 与 `crates/datazen/src/execution/domain/sql_guard/`。SQL 主关键字分类（`sql_main_keyword`、`tokenize_sql`、`find_main_verb_index`）与注释剥离等**与传输无关**的 SQL 判读能力位于 `crates/datazen/src/execution/domain/sql_guard/`，宿主直接复用，未来服务端入口共用同一份实现。

Workflow 引擎本身见 [`workflow` 模块](./workflow.md)，MCP 仅通过 Tools/Resources 暴露。

**信任模型（Settings → MCP Server）：**

| 控制 | 说明 |
|------|------|
| `mcpPermissionMode` | `read_only` / `safe_write`（默认）/ `high_risk_write` |
| `mcpDisabledTools` | 工具 denylist |
| `mcpAllowedConnectionIds` | 连接白名单；**空数组 = 拒绝一切连接（deny-all 默认）**，必须显式加入连接才可访问 |

修改权限/白名单/工具后，嵌入式 stdio / HTTP MCP Server 会自动热重载；独立 `datazen --mcp` 进程需重启后生效。HTTP listener 启动时取得设置快照，该 listener 下的 session 共享此快照；重载会关闭旧 listener 与其 session，并以新快照重启。

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

**Streamable HTTP（桌面设置页，默认关闭）：**

| 项目 | 行为 |
|------|------|
| 启用 | `mcpHttpEnabled`（默认 `false`）；启用时才创建 listener |
| 绑定 | 固定 `127.0.0.1`，不会监听 `0.0.0.0` 或 IPv6；端口配置为 `0` 时由 OS 分配随机端口，命令/状态返回实际端口 |
| 端点 | `http://127.0.0.1:{port}/mcp` |
| 认证 | 每个 `/mcp` 请求必须带 `Authorization: Bearer <token>`；缺失或错误统一 `401` + `WWW-Authenticate: Bearer`，不做常量时间之外的额外区分 |
| token 生命周期 | 设置页启用时首次生成 32 字节随机 token（base64url），**只返回这一次**；持久化文件 `{appData}/mcp-http-token.json` 只存 SHA-256 hash 与末四位，权限 `0600`；轮换后旧 token 立即失效 |
| 工具语义 | 与 stdio 完全复用 `DataZenMcpServer` 的 15 个工具、连接/库/namespace scope、三档权限、逐次审批与审计 |
| 并发/隔离 | 每个 HTTP MCP session 独立创建 `DataZenMcpServer` 与客户端会话 id；并发 session 不共享有状态会话映射 |
| 断连/Stop | HTTP `DELETE` 或停止 listener 会关闭该 session，释放该 client 打开的有状态数据库会话；进程停止时 listener 一并停止 |

启用后设置页会显示实际 URL、实际端口、token 末四位和运行状态。API 命令：`mcp_http_status` / `mcp_http_enable(port?)` / `mcp_http_disable` / `mcp_http_regenerate_token` / `mcp_http_reload`；启用/轮换响应中的 `token` 字段仅在生成该 token 的那一次出现。

curl 验证（需要先启动 listener 并取得 token；把 `TOKEN` 与 `PORT` 换成设置页显示值）：

```bash
# initialize（服务端用 SSE 事件返回 JSON-RPC；先取 Mcp-Session-Id）
curl -i -sS -X POST "http://127.0.0.1:$PORT/mcp" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"curl","version":"1"}}}'

# 后续请求带上 initialize 响应头里的 Mcp-Session-Id
curl -sS -X POST "http://127.0.0.1:$PORT/mcp" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Accept: application/json, text/event-stream' \
  -H 'Content-Type: application/json' \
  -H "Mcp-Session-Id: $SESSION" \
  -H 'MCP-Protocol-Version: 2025-06-18' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'

# 显式结束 session（释放该 client 的有状态会话）
curl -i -sS -X DELETE "http://127.0.0.1:$PORT/mcp" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Mcp-Session-Id: $SESSION"
```

MCP client 配置（Claude Desktop / Cursor 等支持 Streamable HTTP 的客户端；若客户端要求固定入口，请先配置固定端口再启用）：

```json
{
  "mcpServers": {
    "datazen": {
      "url": "http://127.0.0.1:8787/mcp",
      "headers": { "Authorization": "Bearer <token>" }
    }
  }
}
```

不要把 token 写入仓库、日志或共享配置；设置页只显示末四位，丢失后使用“轮换 token”生成新值并更新客户端。

**Resource 与 Tool 策略对齐：**

| 控制 | Tools | Resources |
|------|-------|-----------|
| `mcpAllowedConnectionIds` | 带 `connection_id` 的工具 | `connections`、`query-history`（按条目 `connectionId` 过滤）、`schema/{connectionId}/…` |
| `read_only` | 屏蔽 `query` / `run_workflow` | `query-history` 从列表隐藏且读取返回 `[]`（不泄露 SQL） |
| `safe_write` / `high_risk_write` | SQL 分类门控（分类器来自 `crates/datazen/src/execution/domain/sql_guard/`） | 全量 resources（仍受 allowlist 约束） |

**Server Tools（无状态，12 个；另有 3 个有状态工具，合计 15）：**
- 元数据/查询：`list_connections` / `list_databases` / `list_tables` / `search_tables` / `query` / `get_schema`
- 解释与描述：`explain_query` / `describe_table`
- Workflow：`list_workflows` / `run_workflow`
- 批量与 Redis：`execute_batch` / `redis_command`

**Server Tools（有状态，`open_session` → `use_session` → `close_session`）：**
- `open_session`：在显式目标上开会话，返回不透明 `clientSessionRef`
- `use_session`：在该引用上执行一条 Driver Command
- `close_session`：释放该引用（幂等）

三者只存在于**这个 MCP 进程实例**的内存里，语义见 [§1.4 有状态会话工具](#14-有状态会话工具)。

**`execute_batch`：** 先按顶层分号把 SQL 分成语句列表，硬上限 50 条，默认仍只读；`transaction:true` 时显式开启事务并保证所有语句在同一物理连接上执行。返回结构化 `{ok, stoppedAt, rolledBack, statements[]}`，逐条带 index/verb/rowCount/decision/error；前面语句失败后不再执行后续语句，事务模式下回滚。`limit` 默认 100、上限 5000；timeout 上限 60s。驱动错误只映射为稳定 code 与固定文案，不把驱动原文（可能含 SQL/参数/标识符/凭据）回传。

**`redis_command`：** 只读命令白名单（GET/MGET/SCAN/TYPE/TTL/INFO 等）；WRITE/DANGEROUS/ADMIN/UNSUPPORTED 一律拒绝，命令名必须单个 token、参数单独传入，避免把整条命令拼进一个字符串。支持 `key_pattern`、有界 `limit` 与 timeout；返回结构和拒绝原因固定，不泄露服务端错误正文。

**逐次审批与审计（T19）：** 非只读 SQL、非只读 Workflow、非只读 Redis/桥接调用在 tool body 执行前走 `McpApprovalPort`：Allow once / Deny / 5 分钟超时 / Stop 取消。审批预览只展示经过脱敏的 SQL/参数摘要，不能借预览把凭据带出。审计追加到 App data 下的 `mcp-audit.jsonl`，字段为 who/when/connection/tool/rowCount/decision/requestId，不写 SQL、参数或凭据；查询命令 `mcp_list_audit_log`。审计写失败只 `warn` 并广播 `mcp:audit-error`，不阻断工具调用。批量为每条已执行语句写一行审计，未执行语句也保留 denied/skipped 记录。

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
| `McpClientManager`（`crates/datazen-desktop/src/mcp/client.rs`） | stdio 子进程生命周期、工具发现、`call_tool` |
| `crates/datazen-desktop/src/commands/mcp.rs` | IPC：`mcp_client_connect` / `disconnect` / `list` / `tools` / `call_tool` |
| `crates/datazen-desktop/src/commands/ai/chat.rs` | IPC 薄壳：取 `datazen::ai::mcp_tools` 的工具定义与执行端口 |
| `crates/datazen/src/ai/application/mcp_tools.rs` | 共享层合并 DB tools + MCP tools（`mcp/{serverId}/{toolName}`）并路由 `mcp/*` |
| `crates/datazen/src/ai/application/tool_loop.rs` | 流式工具循环与工具调用分发 |
| `packages/frontend/src/windows/settings/McpClientSection.tsx` | 配置 CRUD、env 编辑、连接/重试、运行时工具列表 |

AI Chat（`ai_chat`）在已连接外部 MCP 时将 MCP 工具与内置 DB tools 一并注入 Provider。内置 DB tools 在 Ask 模式为零，在 Agent 模式至少有 5 个只读元数据工具；模型 Profile 允许查询时还会加入 `query_readonly` / `sample_rows` / `propose_write`。模型调用 `mcp/…` 工具时由后端经 `McpClientManager` 转发至对应子进程；只读策略与逐次审批同时适用于 MCP 工具。详见 [`ai.md`](./ai.md)。

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

- **`open_session(connection_id, database, schema?, ttl_ms?)`**：`database` 是必填的，不接受「省略即用当前库」——省略会被明确拒绝，而不是回退到 GUI 当前目标。成功后返回 `{"clientSessionRef", "state", "contextRevision", "expiresInMs"}`。
- **`use_session(client_session_ref, command, input, timeout_ms?)`**：在映射到的那条会话里跑一个 Driver Command。`query` / `execute` 仍要过 SQL 权限闸（`read_only` 模式下照样被拦），结果行从执行产物（artifact）读出，最多返回 200 行并在 `rowsTruncated` 标记截断。
- **`close_session(client_session_ref, mode?)`**：关闭会话并删掉映射。`mode` 取 `requireNoTransaction`（默认）或回滚未结束事务后关闭的 `rollbackAndClose`，其他字符串直接拒绝，不会静默降级成「丢弃未提交工作」。

`clientSessionRef` 是**不透明引用**：客户端拿到的 uuid 无法反推 `dbSessionId`、连接或主机进程。`reference → handle` 的映射只存在于 [crates/datazen/src/mcp/application/session.rs](../../../crates/datazen/src/mcp/application/session.rs) 的内存里，绑定 principal、后端生成的 clientInstanceId、运行句柄、`contextRevision` 与到期时刻；它**不写入** Workflow 定义、执行历史或数据库配置，因此进程退出即全部失效，也不会跨 MCP 进程共享。

归属与失效判定有三条硬边界：

1. **clientInstanceId 由后端在 server 启动时生成**（`DataZenMcpServer::new` 调 `new_client_instance_id()`），**绝不取自请求参数**；每个 MCP server 实例一个，调用上下文也由适配器本地组装（组织与主体仍是桌面启动身份）。因此「谁开的会话」不是客户端自称，而是后端记的账。
2. **引用只在归属它的那个 clientInstanceId 下可用**。`use_session` 先查映射再谈别的：他人持有的引用返回 `SessionLost`，已关闭、已过期的引用同样返回 `SessionLost`——丢失的映射**不会用同一个引用重建**，客户端必须重新 `open_session`。查映射发生在 SQL 闸与网关调用之前，所以 `SessionLost` 这条路径根本不需要连数据库。
3. **TTL 有上下限**：`sanitize_ttl` 把缺省与显式 0 归一到 300 秒，把超过 3600 秒的请求截到 3600 秒。到期 lookup 和独立 worker 将 binding 撤销为不可访问的 cleanup record，保存原 client owner 与 Runtime handle；不依赖后续工具请求或 Runtime 的空闲回收。

`use_session` 还按映射里记的 `contextRevision` 做乐观并发（`with_expected_context_revision`），即调用方看到的上下文版本一旦被人改过，这次命令会被拒绝而不是默默换目标执行。

`close_session` 是幂等的，而且**对未知、他人、已过期三种情况一律回 `{"closed": false}`**，不区分「从没存在」与「曾经存在」，避免用过期的引用反推服务端状态；网关拒绝关闭时映射**保留**，让客户端可以重试——顺手删掉就等于泄漏了一条再也无法关闭的数据库会话。

客户端断开时，`release_sessions` 先禁止该 client 新增有状态请求，并撤销所有归属 binding；在途 open 返回的资源也转入清理，不向已断开的 client 发新引用。 `session_open.rs` 由独立任务持有完整 Runtime 获取过程：MCP 请求取消只丢弃等待方，不取消获取 owner；未消费的回复由 RAII guard 撤销 binding，并交给同一 cleanup worker。因而即使请求在获取中被取消、断连后才拿到资源，也不会丢失原 owner/handle。成功交付引用后 guard 解除，正常 TTL 与显式 close 语义继续生效。每个 client 至多一个 `session_cleanup.rs` worker，以原 owner 对原 Runtime 句柄调用 `RollbackAndClose`。它不会按 connectionId 扫描其他消费者资源。只有 `resourceRelease=Confirmed` 删除 cleanup record；错误、Pending、Quarantined 均保留，按 100ms 起、上限 30s 退避。未知效果不改写为已回滚，不重放应用 SQL。已隔离且无法确认物理释放的 owner 记录保持在进程内，不能宣称已清理；进程退出不持久化这些句柄。显式 close 收到未确认释放时返回 `closed:false, cleanupPending:true` 并转入同一路径。worker 在 binding 和 cleanup record 均为空后退出。

请求取消/断连与获取交错、晚到资源及预算回收、正常交付后幂等关闭的 Host Runtime fixture 回归在 `crates/datazen-desktop/src/mcp/session_open.rs`；这些受控测试不替代 PG/MySQL 实库/传输旅程。Runtime-backed TTL、active transaction 断连回滚、预算/registry 释放、其他 client 隔离与未确认关闭 owner 保留测试在 `crates/datazen-desktop/src/mcp/session_cleanup.rs`。协议回归测试落在 `crates/datazen-desktop/src/mcp/server/tests_stateful.rs`（他人引用不可用、未知引用不重建、TTL 过期、关闭被网关拒绝时保留映射、重复关闭幂等、断开只释放自己的映射、归属身份与 Job owner 越权被拒）。

