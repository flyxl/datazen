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
