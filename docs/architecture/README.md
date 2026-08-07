# DataZen 系统架构文档

> 本文档为 DataZen 系统架构的总览入口，各模块详细设计请参阅对应子文档。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建，集成 AI 辅助功能。支持 GUI 桌面应用和无头 MCP stdio 服务器两种运行模式。

## 架构全景

```
┌───────────────────────────────────────────────────────────────────────┐
│                          Tauri Application                            │
│                  (GUI mode / headless MCP stdio mode)                 │
├───────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                     Frontend (React + TS)                        │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │ │
│  │  │Connection│ │  Query   │ │   AI     │ │ Settings │           │ │
│  │  │ Manager  │ │ Editor + │ │ Features │ │  Panel   │           │ │
│  │  │          │ │  Chart   │ │ +Context │ │          │           │ │
│  │  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘           │ │
│  │       │             │            │             │                  │ │
│  │  ┌────┴─────┐ ┌─────┴────┐ ┌────┴─────┐                        │ │
│  │  │ ER Diag  │ │DataExport│ │WorkflowWin│                        │ │
│  │  └──────────┘ └──────────┘ └──────────┘                         │ │
│  └───────┼────────────┼────────────┼────────────┼─────────────────┘ │
│          └────────────┴────────────┴────────────┘                    │
│                              │ Tauri IPC                             │
│                              ▼                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                      Backend (Rust)                              │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────┐ ┌──────────┐ │ │
│  │  │ Commands │ │ Services │ │    AI    │ │ MCP  │ │  Store   │ │ │
│  │  │(16模块)  │ │(连接/查询)│ │(4 Provid)│ │(S/C) │ │(AES加密) │ │ │
│  │  └────┬─────┘ └────┬─────┘ └────┬─────┘ └──┬───┘ └──────────┘ │ │
│  │       └────────────┴────────────┘           │                   │ │
│  │                    │                         │                   │ │
│  │  ┌─────────────────┴──────────────────┐     │                   │ │
│  │  │        Database Drivers Layer       │     │                   │ │
│  │  │  ┌────┐ ┌────┐ ┌──────┐ ┌─────┐   │     │                   │ │
│  │  │  │ PG │ │ My │ │SQLite│ │Redis│   │     │                   │ │
│  │  │  └────┘ └────┘ └──────┘ └─────┘   │     │                   │ │
│  │  │        + Plugin Drivers             │     │                   │ │
│  │  └────────────────────────────────────┘     │                   │ │
│  └─────────────────────────────────────────────┘                   │ │
│                    │                     │                           │
└────────────────────┼─────────────────────┼───────────────────────────┘
                     ▼                     ▼
            External Databases      LLM Providers
            (PG, MySQL, etc.)   (OpenAI, Anthropic,
                                 DeepSeek, Custom...)
```

## 分层架构

| 层级 | 职责 | 关键特性 |
|------|------|----------|
| **Commands 层** | 处理前端 IPC 调用 | 16 个命令模块、参数验证、结构化错误、日志记录 |
| **Services 层** | 业务逻辑处理 | 连接管理（含去重锁）、查询执行、DbTools 共享工具 |
| **Drivers 层** | 数据库驱动抽象 | 统一接口、连接池管理、编译时插件扩展（inventory） |
| **AI 层** | LLM 集成 | 4 内置 Provider、协议层复用、流式输出、Prompt 资源文件 + 覆盖 |
| **MCP 层** | 工具协议 | Server 暴露能力、Client 连接外部、Workflows 工作流 |
| **Sync 层** | 跨库数据同步 | IR 中间表示、O(N) 适配器 |
| **Stores 层** | 本地持久化 | AES-256-GCM 加密存储、配置管理 |

---

## 后端文档

| 文档 | 内容 |
|------|------|
| [数据库驱动层](backend/drivers.md) | DatabaseDriver trait（含 `supports_offset`/`supports_explain`）、驱动注册表、插件扩展机制 |
| [Schema 缓存](backend/cache.md) | 两级 TTL 缓存架构、缓存失效策略、查询执行优化 |
| [服务层](backend/services.md) | ConnectionManager（连接去重锁）、QueryExecutor（分页）、DbTools（共享工具） |
| [持久化存储](backend/store.md) | AES-256-GCM 加密本地文件存储、配置/历史/收藏/Prompt覆盖管理 |
| [IPC 命令层](backend/commands.md) | 16 个 Tauri Commands 模块、AppState 结构、CommandError 错误处理 |
| [AI 模块](backend/ai.md) | AiProvider trait、4 内置 Provider、protocol 层、PromptResolver（资源文件 + 覆盖） |
| [MCP 模块](backend/mcp.md) | MCP Server（Tools/Resources/Prompts）、MCP Client、双运行模式 |
| [Workflow 模块](backend/workflow.md) | YAML Workflow 引擎、执行历史；GUI/IPC/MCP 共用；用户手册见 [../workflow-guide.md](../workflow-guide.md) |

## 前端文档

| 文档 | 内容 |
|------|------|
| [状态管理](frontend/state.md) | 8 个 Zustand stores、事件处理、跨窗口通信 |
| [组件与布局](frontend/components.md) | DataTable（含数据导出）、ER 图（React Flow）、PathInput、虚拟滚动、图表可视化 |
| [AI 功能](frontend/ai.md) | AI 组件（AiInput、ContextPicker、Chat、NL2SQL）、@ 上下文引用、SQL 编辑器方言 |
| [扩展性](frontend/extensibility.md) | DB 类型扩展、DatabaseTypeMeta、插件系统、plugin-sdk |

## 横切关注点

| 文档 | 内容 |
|------|------|
| [安全措施](security.md) | AES-256-GCM 加密、CSP、路径遍历防护、文件扩展名白名单、AI Key 安全 |
| [窗口管理](windows.md) | 多窗口架构、Rust 端窗口创建、macOS acceptFirstMouse、windowKind URL 路由 |
| [测试策略](testing.md) | Rust / Vitest / E2E 概览；**跑通 E2E 必读** [../e2e-testing.md](../e2e-testing.md) |

## 其他文档

- [竞品对比：Navicat / TablePlus / DataGrip](../competitive-comparison.md)
- [Workflow 使用手册](../workflow-guide.md)（YAML 语法、模板、跨库、排错）
- [产品需求文档 (PRD)](../PRD.md)
- [插件系统 RFC](../rfc-plugin-system.md)
- [AI 功能 RFC](../rfc-ai-features.md)
- [AI 功能开发进度](../ai-features-progress.md)
- [数据同步 IR 设计](../plan-sync-ir.md)
- [插件开发指南](../plugin-development.md)
- [图表可视化设计](../chart-visualization-design.md)
- [代码审查报告](../code-review-2026-08-01-958f1b6.md)
