# Tauri IPC 命令层

> Source of truth: `src-tauri/src/commands/`。

Commands 是 React 前端进入 Rust backend 的 IPC 边界。Command 层负责参数边界、错误转换、状态访问和把请求交给相应 domain/service/module。

## 1. 当前模块

`src-tauri/src/commands/mod.rs` 下包含：

- connection / connection_import / config / context
- query / data / data_session / schema / sample
- driver_command / driver_protocol
- schema_diff
- sync
- data_transfer
- workflow
- dashboard
- AI / MCP / native_tool
- history / export / backup / restore_job / file_jobs / app_archive
- wapps / theme / window / dialog / tunnel / clipboard

具体目录结构以 `src-tauri/src/commands/` 为准。

## 2. 通用调用链

```text
React component / hook
        ↓
src/commands/*.ts
        ↓
Tauri invoke
        ↓
src-tauri/src/commands/*
        ↓
service / domain module
        ↓
Driver / Store / runtime
```

前端 IPC wrapper 与 Rust command 名称保持显式映射，不在组件中直接拼接后端协议。

进入 `service / domain module` 之前，SQL 会先过安全门网：注释剥离、主关键字分类、只读 / Safe Mode 校验与参数绑定。该门网实现在共享层 `packages/kernel/src/sql_guard/`，宿主 command 以 `datazen_runtime::sql_guard::…` 调用，未来的服务端入口复用同一份实现。

## 3. 连接 ID 参数约定

Command 的参数名携带语义，不是随意的命名。`connection_id` 是持久化连接配置 ID，`db_session_id` 是运行时会话 ID，二者不可互换，也不存在「一个参数兼容两种语义」的回退路径：

| 参数 | 语义 | 典型命令 |
|---|---|---|
| `connection_id` | 查配置、排任务、算归属 | `connect`、`connect_dedicated`、`workflow_execute`、`ai_diagnose_connection`、MCP 的全部 DB tools |
| `db_session_id` | 操作已建立的会话 | `execute_query`、`execute_query_stream`、`get_explain`、`close_database`、`release_connection`、`disconnect`、`preview_pending_changes`、`execute_row_change_plan`、`commit_pending_changes`、`write_data_file`、Schema Diff 的一整组 IPC、`ai_generate_sql`、`ai_diagnose_error`、`ai_analyze_explain`、`ai_generate_schema_doc` |
| `source_/target_db_session_id` | 迁移三件套的成对运行时目标 | Schema Diff 的比较 / 应用 / 预览类 IPC |

参数的语义决定了会不会创建会话：**创建发生在入口，不发生在操作里**。真正会建立会话的只有三处路径——连接类 command（`connect` 共享 / `connect_dedicated` 专用）、Workflow 的 step 执行（见 [workflow.md](./workflow.md)）、以及 Data Sync 的端点解析（任务保存了非空目标库时开专用会话，否则复用共享会话，见 [data-sync.md](./data-sync.md)）。其余 command 一律假定会话已存在，查找失败只走「用同一个 `dbSessionId` 重建物理连接」这一条透明路径。需要把运行时 ID 换回持久化 ID 时（例如 DataTable 提交前校验归属、AI 查询历史按连接过滤、Schema Diff 落库），统一走 `owner_connection_id(db_session_id)` 反查 owner 映射，不做「两种 ID 都能试」的兼容。

参数语义由测试钉住，而不是靠约定：`commands/ai/ipc_contract_guards.rs` 的契约测试直接对命令源码做断言，要求会话语义命令的形参包含 `db_session_id` 且**不含** `connection_id`，配置语义命令反之；同文件的另一组断言禁止把名为 `connection_id` 的变量喂给 `get_session` / `owner_connection_id`。历史上出现过「改了名字但语义反了」的回归，这组断言就是为堵住它。

## 4. 专用领域 Command

复杂功能有独立 command module：

- Schema Diff：`commands/schema_diff.rs`
- Data Sync：`commands/sync/`
- Data Transfer：`commands/data_transfer/`
- AI：`commands/ai/`

这些 command 不应把产品流程重新实现一遍；比较、计划、映射和执行等领域逻辑分别位于 `packages/schema-diff/`、`packages/data-sync/`、`packages/data-transfer/` 等模块。

## 5. Driver Command

`commands/driver_command/mod.rs` 是通用 Driver Command IPC 入口。它把 connection/session 上下文交给 Driver Registry，再通过 `DriverCommandDefinition` 做 command discovery 和执行。

Workflow 同样复用 Driver Command Runtime，而不是绕过 Driver API 建立另一套 Driver-specific IPC。

## 6. 错误

Backend 使用结构化错误并在 IPC 边界转换成前端可消费的错误信息。Driver 原始错误不应直接泄漏实现细节到 React。

## 7. 测试

IPC contract 和 command 行为的测试位于：

- `src-tauri/src/commands/**`
- `src-tauri/tests/`
- 对应 frontend `__tests__`
- E2E contract / journey tests
