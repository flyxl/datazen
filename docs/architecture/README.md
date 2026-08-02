# DataZen 系统架构文档

> 本文档为 DataZen 系统架构的总览入口，各模块详细设计请参阅对应子文档。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建，集成 AI 辅助功能。

## 架构全景

```
┌───────────────────────────────────────────────────────────────────────┐
│                          Tauri Application                            │
├───────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                     Frontend (React + TS)                        │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │ │
│  │  │Connection│ │  Query   │ │   AI     │ │ Settings │           │ │
│  │  │ Manager  │ │  Editor  │ │ Features │ │  Panel   │           │ │
│  │  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘           │ │
│  └───────┼────────────┼────────────┼────────────┼─────────────────┘ │
│          └────────────┴────────────┴────────────┘                    │
│                              │ Tauri IPC                             │
│                              ▼                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                      Backend (Rust)                              │ │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────┐ ┌──────────┐ │ │
│  │  │ Commands │ │ Services │ │    AI    │ │ MCP  │ │  Store   │ │ │
│  │  │(IPC层)   │ │(业务逻辑)│ │(Provider)│ │(S/C) │ │(持久化)  │ │ │
│  │  └────┬─────┘ └────┬─────┘ └────┬─────┘ └──┬───┘ └──────────┘ │ │
│  │       └────────────┴────────────┘           │                   │ │
│  │                    │                         │                   │ │
│  │  ┌─────────────────┴──────────────────┐     │                   │ │
│  │  │        Database Drivers Layer       │     │                   │ │
│  │  │  ┌────┐ ┌────┐ ┌──────┐ ┌─────┐   │     │                   │ │
│  │  │  │ PG │ │ My │ │SQLite│ │Redis│   │     │                   │ │
│  │  │  └────┘ └────┘ └──────┘ └─────┘   │     │                   │ │
│  │  └────────────────────────────────────┘     │                   │ │
│  └─────────────────────────────────────────────┘                   │ │
│                    │                     │                           │
└────────────────────┼─────────────────────┼───────────────────────────┘
                     ▼                     ▼
            External Databases      LLM Providers
            (PG, MySQL, etc.)    (OpenAI, Anthropic...)
```

## 分层架构

| 层级 | 职责 | 关键特性 |
|------|------|----------|
| **Commands 层** | 处理前端 IPC 调用 | 参数验证、结构化错误、日志记录 |
| **Services 层** | 业务逻辑处理 | 连接管理、查询执行、事务控制 |
| **Drivers 层** | 数据库驱动抽象 | 统一接口、连接池管理、插件扩展 |
| **AI 层** | LLM 集成 | 多 Provider 支持、流式输出、上下文构建 |
| **MCP 层** | 工具协议 | Server 暴露能力、Client 连接外部、Skills 工作流 |
| **Stores 层** | 本地持久化 | 加密存储、配置管理 |

---

## 后端文档

| 文档 | 内容 |
|------|------|
| [数据库驱动层](backend/drivers.md) | DatabaseDriver trait、驱动注册表、插件扩展机制、添加新 DB 类型检查清单 |
| [Schema 缓存](backend/cache.md) | 多级缓存架构、缓存失效策略、查询执行优化 |
| [服务层](backend/services.md) | ConnectionManager 连接管理、资源安全、连接泄露防护 |
| [持久化存储](backend/store.md) | 本地文件存储、AES-256-GCM 加密、配置/历史/收藏管理 |
| [IPC 命令层](backend/commands.md) | Tauri Commands 模块划分、AppState 结构、CommandError 错误处理 |
| [AI 模块](backend/ai.md) | AiProvider trait、Provider 实现、SchemaContextBuilder、PromptBuilder |
| [MCP 模块](backend/mcp.md) | MCP Server（Tools/Resources/Prompts）、MCP Client、Skills 系统 |

## 前端文档

| 文档 | 内容 |
|------|------|
| [状态管理](frontend/state.md) | Zustand stores 设计、事件处理、跨窗口通信 |
| [组件与布局](frontend/components.md) | 核心组件设计、DataTable、虚拟滚动、响应式布局、主题系统 |
| [AI 功能](frontend/ai.md) | AI 组件（NL2SQL、诊断、Chat）、SQL 编辑器方言、aiStore |
| [扩展性](frontend/extensibility.md) | DB 类型扩展、插件系统、DB_REGISTRY 元数据驱动 |

## 横切关注点

| 文档 | 内容 |
|------|------|
| [安全措施](security.md) | 密码加密、CSP、Argon2id KDF、路径遍历防护、AI Key 安全 |
| [窗口管理](windows.md) | 多窗口架构、Rust 端窗口创建、macOS acceptFirstMouse |
| [测试策略](testing.md) | Rust 单元/集成测试、Vitest 前端测试、WebdriverIO E2E、手工黑盒测试 (`test/`) |

## 其他文档

- [产品需求文档 (PRD)](../PRD.md)
- [插件系统 RFC](../rfc-plugin-system.md)
- [AI 功能 RFC](../rfc-ai-features.md)
- [AI 功能开发进度](../ai-features-progress.md)
- [数据同步 IR 设计](../plan-sync-ir.md)
- [插件开发指南](../plugin-development.md)
- [代码审查报告](../code-review-2026-08-01-958f1b6.md)
