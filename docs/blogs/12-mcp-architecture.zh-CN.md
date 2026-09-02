# DataZen 架构设计（十二）：让数据库能力进入 AI 生态——MCP 架构

> MCP 让外部 Agent 可以调用 DataZen 的数据库能力，也让 DataZen 能接入其他 MCP Server。真正的难点不是实现协议，而是保证协议入口不复制一套连接、权限和执行逻辑。

## Server 与 Client

DataZen 作为 MCP Server 暴露 Tools、Resources 和 Prompts；作为 MCP Client 连接外部 Server，取得工具并供 AI Chat 使用。两者都位于 `src-tauri/src/mcp/`，但共享的是 Host 的服务和 Command Runtime，而不是相互耦合实现。

Server 工具包括 `list_connections`、`list_databases`、`list_tables`、`search_tables`、`query`、`get_schema`、`explain_query`、`describe_table`、`list_workflows` 和 `run_workflow`。

## 为什么 MCP 使用 connectionId

MCP 请求可能来自独立的 stdio 进程，没有 GUI 面板，因此使用可持久化的 `connection_id`。Server 通过 `db_tools::resolve_connection` 按需调用 `ConnectionManager`，获得运行时会话后再进入 Driver Command Runtime。

GUI 的 SQL IPC 仍然使用 `dbSessionId`。两种入口在协议层不同，在执行核心处汇合。

## 两种 Server 运行模式

嵌入式 GUI 模式与 `--mcp-stdio` 无头模式共享 AppState。GUI 使用同进程 duplex transport，不需要 token；headless 模式首次启动会创建 `{appData}/mcp.token`，客户端必须通过 `DATAZEN_MCP_TOKEN` 提供匹配值。

无头模式不创建窗口，只建立 Tokio Runtime 和 MCP Server。它可以被 Claude Desktop、Cursor 等外部 Agent 作为本地进程启动。

## 权限模型

MCP Server 有三档权限：`read_only`、`safe_write`、`high_risk_write`。同时还有连接白名单 `mcpAllowedConnectionIds` 和工具 denylist `mcpDisabledTools`。

权限检查至少发生两次：MCP 层根据工具和模式进行门控，Command Runtime/Driver 层根据 Command Definition 再次校验。空白白名单意味着暴露所有已保存连接，因此设置界面必须明确提示。

在 `read_only` 模式下，查询和 Workflow 工具被屏蔽，query-history 资源也不应泄露 SQL；Schema 和连接摘要仍需遵循 allowlist。

## Resources 与 Prompts

Resources 提供 `datazen://connections`、`datazen://query-history`、`datazen://schema/{connectionId}/{database}` 和 `datazen://workflows`。它们是只读上下文，不应变成绕过工具权限的隐形执行通道。

Prompts（`nl2sql`、`diagnose_error`、`explain_plan`）复用 PromptResolver 和 SchemaContextBuilder，确保 MCP 发起的 AI 请求与 GUI AI 面板使用相同的模板和敏感信息边界。

## MCP Client 的资源管理

Client 通过 stdio 启动外部进程，连接有 30 秒超时；失败时必须清理子进程，避免留下孤儿进程。发现到的工具先转成内部 Tool Definition，再交给 AI Chat 的工具调用层。

外部工具结果是不可信输入。Host 不能因为工具描述或返回文本要求它读取本地文件、发送凭据或改变权限；执行仍由用户配置和 Host 权限模式决定。

## 为什么 Data Sync 不直接暴露

V1 不把 Data Synchronization 的 compare/apply/execute 注册为 MCP Tool。它需要成对运行时会话、表映射和高风险写入确认。未来如果开放，也必须复用同一权限模型，默认只允许比较，执行保持高风险门控。

## 结语

MCP 在 DataZen 中是协议适配层，不是第二套数据库后端。Server 把外部请求转成 DbTools/Command，Client 把外部工具转成 AI 可用能力；连接、权限、审计和资源生命周期仍由 Host 统一控制。下一篇将比较 Schema Diff、Data Sync 和 Transfer，说明为什么相似的“同步”必须分成三种产品边界。

相关资料：[MCP 架构](../architecture/backend/mcp.md) · [MCP 命令](../../src-tauri/src/commands/mcp.rs) · [Workflow](10-workflow-engine.zh-CN.md)
