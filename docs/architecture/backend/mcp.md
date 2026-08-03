# MCP 模块

> [返回架构总览](../README.md)

### 1.1 MCP Server

DataZen 作为 MCP Server 暴露数据库操作能力给外部 LLM 应用（Claude Desktop、Cursor 等）。

```
src-tauri/src/mcp/
├── mod.rs          # MCP 启动/停止, 状态管理
├── server.rs       # MCP Server 实现
├── client.rs       # MCP Client 管理
└── workflows.rs       # Workflows 系统
```

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

### 1.3 Workflows 系统

用户自定义 AI 工作流：
- YAML 格式定义
- 支持 `query`（SQL 查询）和 `ai`（LLM 推理）两种步骤类型
- 变量替换（`{{var}}` + `{{steps.id.result}}` + 内置变量）
- 路径遍历防护
- 查询结果行数限制（1000）
