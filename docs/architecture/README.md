# DataZen 系统架构文档

> 本文档为 DataZen 系统架构的总览入口，各模块详细设计请参阅对应子文档。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建，集成 AI 辅助功能。支持 GUI 桌面应用和无头 MCP stdio 服务器两种运行模式。

## 架构全景

```text
┌───────────────────────────────────────────────────────────────────────┐
│                          Tauri Application                            │
│                  (GUI mode / headless MCP stdio mode)                 │
├───────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                     Frontend (React + TS)                        │ │
│  │  Connection / Query / AI / Settings / Workflow / Dashboard       │ │
│  │  Command discovery + schema-driven Workflow Command editor      │ │
│  └──────────────────────────────┬──────────────────────────────────┘ │
│                                 │ Tauri IPC                           │
│                                 ▼                                     │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                      Backend (Rust)                              │ │
│  │  Commands / Services / AI / MCP / Workflow / Dashboard / Store  │ │
│  │                                 │                                │ │
│  │                                 ▼                                │ │
│  │                    Driver Command Runtime                         │ │
│  │              connection → definition → validation                │ │
│  │                         → execute_command                         │ │
│  │                                 │                                │ │
│  │                    Database Drivers Layer                          │ │
│  │             PG / MySQL / SQLite / Redis / Plugins                │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                    │                         │                        │
└────────────────────┼─────────────────────────┼────────────────────────┘
                     ▼                         ▼
             External Databases          LLM Providers
```

## 分层架构

| 层级 | 职责 | 关键特性 |
|------|------|----------|
| **Commands 层** | 处理前端 IPC 调用 | 参数验证、结构化错误、日志记录、Driver Command IPC、查询结果流式 Channel |
| **Services 层** | 业务逻辑处理 | ConnectionManager、QueryExecutor、DbTools |
| **Workflow 层** | YAML 工作流编排 | Command runtime、Connection inheritance、Legacy Query compatibility |
| **Drivers 层** | 数据库驱动抽象 | `DatabaseDriver`、Command Definition、`execute_command`、inventory 插件扩展 |
| **AI 层** | LLM 集成 | 多 Provider、协议复用、流式输出、Prompt Resolver |
| **MCP 层** | 工具协议 | Server / Client、Workflow 调用 |
| **Sync 层** | 数据同步（同族 Diff Sync） | `data_sync`：门闸 / 流式比较 / ChangeSet / 参数化 DML；异构 IR 属 Transfer，V1 不实现 |
| **Stores 层** | 本地持久化 | AES-256-GCM；主密钥在 OS 钥匙串或 `{appData}/.key` |

## Driver Command 架构

Driver 的能力通过 Command 暴露，而不是让 Workflow / UI 根据 Driver 类型写大量特殊分支。

```text
                         DatabaseDriver
                              │
                 ┌────────────┴────────────┐
                 ▼                         ▼
       command_definitions()       execute_command()
                 │                         │
                 ▼                         ▼
      DriverCommandDefinition          CommandResult
       ├── name                       ├── rows / data
       ├── description                ├── affected rows
       └── input_schema               └── metadata
                 │
                 ├───────────────┐
                 ▼               ▼
             Workflow           IPC / UI
                 │               │
                 └───────┬───────┘
                         ▼
                 Command Runtime
```

标准 `query` / `execute` Command 提供默认实现，以兼容现有 Driver。Driver 可以增加任意 Driver-specific Command，例如 NoSQL、KV、搜索或管理类操作。上层只依赖 Command Definition 和 JSON input/output，不需要知道具体 Driver 类型。

## Workflow 架构

Workflow 模块位于 `src-tauri/src/workflow/`，GUI、Tauri IPC、MCP 共用同一个执行引擎。

```text
WorkflowDefinition
       │
       ▼
WorkflowExecutor
       │
       ├── Condition / ForEach / Ai
       │
       └── Command
              │
              ▼
       command_runtime
              │
              ├── effective connection
              ├── command discovery
              ├── template resolution
              ├── input schema validation
              └── Driver::execute_command()
```

### Connection inheritance

Workflow 可以设置默认 connection：

```yaml
connection: mysql-prod
steps:
  - type: command
    command: query
    input:
      sql: SELECT * FROM users
  - type: command
    connection: postgres-prod
    command: query
    input:
      sql: SELECT * FROM orders
```

Step 未指定 connection 时继承 Workflow 默认值；Step 显式指定时覆盖默认值。这样常见的多 Step 单 Connection 场景不需要重复配置连接。

### Legacy Query

旧版：

```yaml
type: query
connection: mysql-prod
database: reporting
sql: SELECT * FROM users
```

仍然支持。执行前转换为内部 `Command("query")`，`database` 等旧字段保持兼容。旧 Query 和新 Command 进入相同 runtime。

### Command Discovery

前端编辑 Command Step 时通过当前有效 Connection 获取 Driver Command Definition，并根据 `input_schema` 生成输入编辑器：

```text
Effective Connection
       ↓
get_connection_commands()
       ↓
Driver::command_definitions()
       ↓
Command selector
       ↓
input_schema
       ↓
Schema-driven form
```

Connection 变化后重新 discovery；没有 Step override 时使用 Workflow 默认 connection。

## 后端文档

| 文档 | 内容 |
|------|------|
| [数据库驱动层](backend/drivers.md) | DatabaseDriver trait、驱动注册表、插件扩展机制 |
| [Schema 缓存](backend/cache.md) | 两级 TTL 缓存架构、缓存失效策略、查询执行优化 |
| [服务层](backend/services.md) | ConnectionManager、QueryExecutor、DbTools |
| [持久化存储](backend/store.md) | AES-256-GCM；主密钥 keychain / `.key` 双后端 |
| [IPC 命令层](backend/commands.md) | Tauri Commands、AppState、CommandError |
| [运行时主题包](backend/theme.md) | 本地 ZIP、`--c-*` / `--dt-*` token、与驱动独立 |
| [AI 模块](backend/ai.md) | AiProvider、Provider protocol、PromptResolver |
| [MCP 模块](backend/mcp.md) | MCP Server、MCP Client、双运行模式 |
| [Workflow 模块](backend/workflow.md) | YAML Workflow、Command runtime、Connection inheritance、Legacy Query、执行历史 |
| [数据看板](backend/dashboard.md) | AppDb 统一存储、Widget→finalOutput、Monitor 调度、导出 v2 |
| [Schema Diff Deploy](backend/schema-diff.md) | Schema diff / DDL plan / deploy |
| [数据同步](backend/data-sync.md) | 同族 Diff Sync、硬门闸、`inspect_data_sync` / `execute_data_sync`、覆盖拷贝已拆除 |

## 前端文档

| 文档 | 内容 |
|------|------|
| [状态管理](frontend/state.md) | Zustand stores、事件处理、跨窗口通信 |
| [组件与布局](frontend/components.md) | DataTable（`--dt-*`）、Context Menu、ER 图、统一工作区、主题系统 |
| [AI 功能](frontend/ai.md) | AI 组件、@ 上下文引用、SQL 编辑器方言 |
| [扩展性](frontend/extensibility.md) | DB 类型扩展、DatabaseTypeMeta、插件系统、plugin-sdk |

## 横切关注点

| 文档 | 内容 |
|------|------|
| [ID 术语规范](naming.md) | `connectionId` / `dbSessionId` 定义、生命周期、流转图、双模适配点与守护规则 |
| [安全措施](security.md) | AES-256-GCM + key_store、CSP、路径遍历防护、文件扩展名白名单、AI Key 安全 |
| [窗口管理](windows.md) | 主工作区 Page（Welcome / Connection / Settings 等）、子窗口、`windowKind` 路由、Docs 官网跳转 |
| [测试策略](testing.md) | Rust / Vitest / E2E 概览；Host UI 路径覆盖见 [e2e-coverage.md](../development/e2e-coverage.md) |

## 相关文档索引

- 功能使用文档（用户向）：[docs/features/](../features/)
- 开发 / 发布 / 测试流程文档：[docs/development/](../development/)
- 文档总索引：[docs/README.md](../README.md)
