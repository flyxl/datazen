# DataZen 文档索引

DataZen 文档只保留两类长期有效内容：**当前功能使用文档**和**当前代码架构/开发文档**。开发过程中的 PRD、进度、Bug List、UI Review、临时 Backlog 等不作为仓库文档长期维护。

## 文档结构

| 目录 | 定位 | 读者 |
|---|---|---|
| [features/](features/) | 当前已实现功能的使用说明 | 用户 / 开发者 |
| [architecture/](architecture/) | 与 main 分支代码对应的架构说明 | 贡献者 / AI 助手 |
| [development/](development/) | 开发、测试、发布、驱动开发流程 | 贡献者 |
| [blogs/](blogs/) | 面向公开发布的架构文章 | 开发者 / 用户 |

## 功能文档

- [Workflow](features/workflow-guide.zh-CN.md) / [English](features/workflow-guide.en.md)
- [Schema Diff](features/schema-diff-guide.zh-CN.md) / [English](features/schema-diff-guide.en.md)
- [Data Sync](features/data-sync-guide.zh-CN.md)
- [Data Transfer](features/data-transfer-guide.zh-CN.md)
- [Ops Dashboard](features/ops-dashboard-guide.zh-CN.md) / [English](features/ops-dashboard-guide.en.md)

功能文档描述当前 main 已实现的行为；如果某能力尚未实现，不在这里记录未来计划。

## 架构文档

入口：[architecture/README.md](architecture/README.md)

### 后端

- [Drivers](architecture/backend/drivers.md)
- [Services](architecture/backend/services.md)
- [Commands](architecture/backend/commands.md)
- [Cache](architecture/backend/cache.md)
- [Store](architecture/backend/store.md)
- [AI](architecture/backend/ai.md)
- [MCP](architecture/backend/mcp.md)
- [Workflow](architecture/backend/workflow.md)
- [Dashboard](architecture/backend/dashboard.md)
- [Data Sync](architecture/backend/data-sync.md)
- [Schema Diff](architecture/backend/schema-diff.md)
- [Theme](architecture/backend/theme.md)
- [Extensions](architecture/backend/extensions.md)

### 前端

- [State](architecture/frontend/state.md)
- [Components](architecture/frontend/components.md)
- [AI](architecture/frontend/ai.md)
- [Extensibility](architecture/frontend/extensibility.md)

### 横切

- [Naming](architecture/naming.md)
- [Security](architecture/security.md)
- [Windows](architecture/windows.md)
- [Testing](architecture/testing.md)

## 开发与发布

- [E2E Testing](development/e2e-testing.md)
- [E2E Coverage](development/e2e-coverage.md)
- [CI Private Plugins](development/ci-private-plugins.md)
- [Independent Driver Development](development/independent-driver-development.en.md) / [中文](development/independent-driver-development.zh-CN.md)
- [Driver API Dependency Boundary](development/driver-api-dependency-boundary.md)
- [Optional Drivers](development/optional-drivers.md)
- [Packaging](development/packaging.md)
- [Updater](development/updater.md)
- [GitHub Pages](development/github-pages.md)

## 公开架构文章

见 [blogs/README.md](blogs/README.md)。

## 维护规则

1. 文档中的文件路径、命令、IPC 名称和能力矩阵必须以 main 分支代码为准。
2. 已实现功能写入 features；架构事实写入 architecture；开发流程写入 development。
3. 临时实施计划、PRD、进度、Bug List 和评审记录不提交到长期文档索引。
4. 删除或重构代码时，同步删除失效文档引用。
