# DataZen 文档索引

> 面向 AI 编程助手与开发者的项目文档入口。产品官网与在线使用手册见 <https://flyxl.github.io/datazen/>（仓库内源文件位于 [site/](../site/)）。

文档分三类：

| 目录 | 定位 | 读者 |
|------|------|------|
| [features/](features/) | **系统功能介绍** — 各功能的用户向使用文档 | 所有用户 |
| [architecture/](architecture/) | **系统架构文档** — 总览与各模块详细设计 | 贡献者 / AI 助手 |
| [development/](development/) | **其他必要文档** — 开发、测试、发布、插件开发流程 | 贡献者 |

---

## 1. 系统功能介绍（docs/features/）

| 文档 | 语言 | 内容 |
|------|------|------|
| [workflow-guide](features/workflow-guide.zh-CN.md) / [en](features/workflow-guide.en.md) | 中 / 英 | Workflow：YAML 编排查询与 AI 步骤、变量模板、错误策略 |
| [data-sync-guide](features/data-sync-guide.zh-CN.md) | 中 | 数据同步（同族库 Compare → Review → Preview → Execute） |
| [data-transfer-guide](features/data-transfer-guide.zh-CN.md) | 中 | 异构数据迁移（跨方言向导、IR 类型映射） |
| [schema-diff-guide](features/schema-diff-guide.zh-CN.md) / [en](features/schema-diff-guide.en.md) | 中 / 英 | 结构对比与 DDL 部署（Compare → Plan → Review → Deploy） |
| [schema-diff-deploy](features/schema-diff-deploy.md) | 英 | Schema Diff Deploy 速览（安全默认值矩阵） |
| [ops-dashboard-guide](features/ops-dashboard-guide.zh-CN.md) / [en](features/ops-dashboard-guide.en.md) | 中 / 英 | 运营看板：Widget、监控调度、告警与导出 |

> 在线版使用手册（含截图，覆盖连接、SQL 编辑、数据编辑、AI、图表、Workflow、Redis、备份等全部功能）：
> 英文 <https://flyxl.github.io/datazen/manual.html> · 中文 <https://flyxl.github.io/datazen/zh/manual.html>

## 2. 系统架构文档（docs/architecture/）

总览入口：[architecture/README.md](architecture/README.md)（分层架构、Driver Command 架构、Workflow 架构）。

### 后端（architecture/backend/）

| 文档 | 内容 |
|------|------|
| [drivers](architecture/backend/drivers.md) | DatabaseDriver trait、驱动注册表、inventory 注册 |
| [services](architecture/backend/services.md) | ConnectionManager、QueryExecutor、DbTools |
| [commands](architecture/backend/commands.md) | Tauri IPC 命令层、AppState、CommandError |
| [cache](architecture/backend/cache.md) | Schema 两级 TTL 缓存与失效策略 |
| [store](architecture/backend/store.md) | AES-256-GCM 持久化、keychain / .key 双后端 |
| [ai](architecture/backend/ai.md) | AI Provider、协议层、PromptResolver |
| [mcp](architecture/backend/mcp.md) | MCP Server / Client、无头 stdio 模式 |
| [workflow](architecture/backend/workflow.md) | Workflow 引擎、Command runtime、连接继承 |
| [dashboard](architecture/backend/dashboard.md) | 运营看板：AppDb、Monitor 调度、导出 v2 |
| [data-sync](architecture/backend/data-sync.md) | 同族数据同步：门闸 / 比较 / ChangeSet |
| [schema-diff](architecture/backend/schema-diff.md) | 结构对比 / DDL plan / 部署 |
| [theme](architecture/backend/theme.md) | 运行时主题包（遗留） |
| [plugins](architecture/backend/plugins.md) | 运行时插件系统（UI 页 + 主题） |

### 前端（architecture/frontend/）

| 文档 | 内容 |
|------|------|
| [state](architecture/frontend/state.md) | Zustand stores、跨窗口通信 |
| [components](architecture/frontend/components.md) | DataTable（--dt-*）、Context Menu、ER 图、主题系统 |
| [ai](architecture/frontend/ai.md) | AI 组件、@ 上下文引用、SQL 编辑器方言 |
| [extensibility](architecture/frontend/extensibility.md) | DB 类型扩展、DatabaseTypeMeta、插件 SDK |

### 横切关注点

| 文档 | 内容 |
|------|------|
| [security](architecture/security.md) | 加密、CSP、路径遍历防护、AI Key 安全 |
| [windows](architecture/windows.md) | 主工作区 Page 导航、子窗口、windowKind 路由 |
| [testing](architecture/testing.md) | 测试策略总览（单测 / E2E 分层与落点） |
| [rfc/unified-panel-store](architecture/rfc/unified-panel-store.md) | 统一 Panel Store RFC |

## 3. 开发 / 发布文档（docs/development/）

| 文档 | 内容 |
|------|------|
| [e2e-testing](development/e2e-testing.md) | WebdriverIO E2E 完整流程（构建、运行、调试） |
| [e2e-coverage](development/e2e-coverage.md) | Host UI E2E 覆盖矩阵与例外登记 |
| [ci-private-plugins](development/ci-private-plugins.md) | 私有 Git 驱动的 CI Deploy Key / Environment 配置 |
| [independent-plugin-development](development/independent-plugin-development.en.md) / [中文](development/independent-plugin-development.zh-CN.md) | 独立仓库驱动插件开发指南 |
| [driver-api-dependency-boundary](development/driver-api-dependency-boundary.md) | datazen-driver-api 公共 API 依赖边界 |
| [optional-drivers](development/optional-drivers.md) | 可选驱动选型（MongoDB / ClickHouse / DuckDB / SQL Server…） |
| [packaging](development/packaging.md) | 打包与发布渠道（Gatekeeper、公证、Linux 包） |
| [updater](development/updater.md) | 自动更新机制 |
| [github-pages](development/github-pages.md) | 官网 / 使用手册的 GitHub Pages 部署与验证 |

---

## 资源

- 官网与手册截图：[site/assets/screenshots/](../site/assets/screenshots/)（README 与官网引用的图片资源均在此）
- 项目 README：[README.md](../README.md) / [README.zh-CN.md](../README.zh-CN.md)
- 贡献指南：[CONTRIBUTING.md](../CONTRIBUTING.md)
