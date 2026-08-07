# Workflow 模块

> [返回架构总览](../README.md)

用户自定义 AI/SQL 工作流引擎。独立于 MCP 协议；GUI、Tauri IPC 与 MCP Server 均为调用方。

**用户手册（YAML 语法与用法）：** [Workflow 使用手册](../../workflow-guide.md)

```
src-tauri/src/workflow/
├── mod.rs           # 模块出口
├── workflows.rs     # 定义 / 注册表 / 执行器 / 模板引擎
└── history.rs       # 执行历史持久化
```

**能力概要：**
- YAML 格式定义（Query / Ai / Condition / ForEach）
- 变量替换（`{{var}}`、`{{steps.id.result}}`、深层路径与通配符）
- 跨库：步骤可绑定不同 connection
- 错误策略：abort / skip / fallback
- 路径遍历防护；查询结果行数限制

**入口：**
- IPC：`commands/ai.rs` → `workflow_*` / `workflow_history_*`
- GUI：`WorkflowPanel` / `WorkflowWindow`
- MCP：`list_workflows` / `run_workflow`、`datazen://workflows`（实现位于 `mcp/server.rs`）
