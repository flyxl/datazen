# MCP 模块

> [返回架构总览](../README.md)

### 1.1 MCP Server

DataZen 作为 MCP Server 暴露数据库操作能力给外部 LLM 应用（Claude Desktop、Cursor 等）。

```
src-tauri/src/mcp/
├── mod.rs          # MCP 启动/停止, 状态管理
├── server.rs       # MCP Server 实现（含 workflow tools 适配）
└── client.rs       # MCP Client 管理
```

Workflow 引擎本身在 [`workflow` 模块](./workflow.md)（若尚未拆文档，见 `src-tauri/src/workflow/`），MCP 仅通过 Tools/Resources 暴露。

**Server Tools:**
- `list_connections` / `list_databases` / `list_tables` / `query` / `get_schema`
- `explain_query` / `describe_table` / `list_workflows` / `run_workflow`

**Server Resources:**
- `datazen://connections` / `datazen://query-history`
- `datazen://schema/{id}/{db}` / `datazen://workflows`

**Server Prompts:**
- `nl2sql` / `diagnose_error` / `explain_plan`

### 1.2 MCP Client

连接外部 MCP Server，获取工具能力：
- stdio transport 支持（TokioChildProcess）
- 30s 连接超时保护
- connect 失败时自动进程清理

### 1.3 连接 ID（config_id vs connection_id）

MCP tools 与 GUI IPC 共用 `ConnectionManager` 语义（详见 [服务层 — 连接 ID 约定](./services.md#连接-id-约定)）：

- **`config_id`**：`connections.json` 中的持久化连接 UUID；`connect` / `list_connections` 使用。
- **`connection_id`**：`connect` 返回的运行时会话 ID；`query`、`get_schema` 等需已连接会话的工具使用。

