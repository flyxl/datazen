# MCP 模块

> [返回架构总览](../README.md)

### 1.1 MCP Server

DataZen 作为 MCP Server 暴露数据库操作能力给外部 LLM 应用（Claude Desktop、Cursor 等）。

```
src-tauri/src/mcp/
├── mod.rs          # MCP 启动/停止, 状态管理
├── allowlist.rs    # 连接白名单（空 = 全部暴露）
├── permission.rs   # 三档权限 + SQL/工具门控
├── server.rs       # MCP Server 实现（含 workflow tools 适配）
└── client.rs       # MCP Client 管理
```

Workflow 引擎本身在 [`workflow` 模块](./workflow.md)（若尚未拆文档，见 `src-tauri/src/workflow/`），MCP 仅通过 Tools/Resources 暴露。

**信任模型（Settings → MCP Server）：**

| 控制 | 说明 |
|------|------|
| `mcpPermissionMode` | `read_only` / `safe_write`（默认）/ `high_risk_write` |
| `mcpDisabledTools` | 工具 denylist |
| `mcpAllowedConnectionIds` | 连接白名单；**空数组 = 暴露全部已保存连接** |

修改权限/白名单/工具后，嵌入式 MCP Server 会自动热重载；独立 `datazen --mcp` 进程需重启后生效。

**Server Tools:**
- `list_connections` / `list_databases` / `list_tables` / `search_tables` / `query` / `get_schema`
- `explain_query` / `describe_table` / `list_workflows` / `run_workflow`

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

### 1.3 连接 ID（connection_id vs db_session_id）

MCP tools 与 GUI IPC 共用 `ConnectionManager` 语义（详见 [服务层 — 连接 ID 约定](./services.md#连接-id-约定)）：

- **`connection_id`**：`connections.json` 中的持久化连接 UUID；`list_connections` 返回值；MCP tools / prompts / AI db tools 入参。
- **`db_session_id`**：GUI `connect` 返回的运行时会话 ID；GUI IPC（`query`、`get_schema` 等）在已连接后传此 ID。

> 历史演进：早期版本 MCP 工具入参叫 `config_id`、运行时句柄叫 `connection_id`。现行为上表术语，且 MCP 侧不保留 `config_id` 兼容别名——旧键会被直接拒绝（见 CHANGELOG 破坏性变更）。

