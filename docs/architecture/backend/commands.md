# Tauri IPC 命令层

> Source of truth: `src-tauri/src/commands/`。

Commands 是 React 前端进入 Rust backend 的 IPC 边界。Command 层负责参数边界、错误转换、状态访问和把请求交给相应 domain/service/module。

## 1. 当前模块

`src-tauri/src/commands/mod.rs` 下包含：

- connection / config / context
- query / data / schema / structure
- driver_command
- schema_diff
- sync
- data_transfer
- workflow
- dashboard
- AI / MCP
- history / export / backup
- extensions / theme / window

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

## 3. 专用领域 Command

复杂功能有独立 command module：

- Schema Diff：`commands/schema_diff.rs`
- Data Sync：`commands/sync/`
- Data Transfer：`commands/data_transfer/`
- AI：`commands/ai/`

这些 command 不应把产品流程重新实现一遍；比较、计划、映射和执行等领域逻辑分别位于 `schema_diff/`、`data_sync/`、`data_transfer/` 等模块。

## 4. Driver Command

`commands/driver_command.rs` 是通用 Driver Command IPC 入口。它把 connection/session 上下文交给 Driver Registry，再通过 `DriverCommandDefinition` 做 command discovery 和执行。

Workflow 同样复用 Driver Command Runtime，而不是绕过 Driver API 建立另一套 Driver-specific IPC。

## 5. 错误

Backend 使用结构化错误并在 IPC 边界转换成前端可消费的错误信息。Driver 原始错误不应直接泄漏实现细节到 React。

## 6. 测试

IPC contract 和 command 行为的测试位于：

- `src-tauri/src/commands/**`
- `src-tauri/tests/`
- 对应 frontend `__tests__`
- E2E contract / journey tests
