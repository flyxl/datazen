<div align="center">

<img src="site/assets/logo.png" width="96" alt="DataZen" />

# DataZen

### 面向开发者与 AI Agent 的数据库工作台

查询 · 排障 · 可视化 · 迁移 · 自动化 · MCP

[![Release](https://img.shields.io/github/v/release/flyxl/datazen?style=flat-square)](https://github.com/flyxl/datazen/releases)
[![License](https://img.shields.io/badge/license-GPLv3-blue?style=flat-square)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-blue?style=flat-square)](#安装)

[下载](https://flyxl.github.io/datazen/zh/download.html) · [官网](https://flyxl.github.io/datazen/zh/) · [English](README.md) · [贡献指南](CONTRIBUTING.md)

</div>

![DataZen 主界面](site/assets/screenshots/01-main-window.png)

## 为什么选择 DataZen？

DataZen 是一款基于 **Tauri + Rust** 构建的桌面数据库工作台。它把连接、理解、修改和自动化数据库的完整流程放在一个应用里。

- **查询** — 编写 SQL、浏览 Schema、编辑数据、查看 ER 图，并使用 Redis Key 浏览器。
- **排障** — 用自然语言生成 SQL、诊断错误、分析 EXPLAIN，并通过只读连接降低误操作风险。
- **可视化** — 查询结果直接生成图表，也可以构建带刷新、历史和告警的 Ops Dashboard。
- **迁移数据** — 备份数据库、同步同族数据库、迁移异构数据库，或审查并部署 Schema 变更。
- **自动化** — 用 YAML Workflow 编排查询、AI、条件和循环，也可以从 UI、MCP 或无头模式运行。
- **重视隐私** — AI 请求只发送给你配置的 Provider。
- **开源且可扩展** — GPLv3 应用、社区 Driver API、沙箱 Workspace App 和宿主扩展点。

## 三分钟开始使用

1. 下载 [DataZen](https://flyxl.github.io/datazen/zh/download.html)。
2. 打开本地 SQLite 文件，或创建 PostgreSQL、MySQL/MariaDB、Redis 连接。
3. 浏览 Schema，编写查询或让 AI 按需求生成 SQL，执行后查看结果并切换为图表。

AI 不是必需品：普通数据库浏览和查询不需要 API Key。

## 一个工作台，解决四类数据库工作

| 工作 | DataZen 提供的能力 |
|---|---|
| **理解数据** | SQL 编辑器、Schema 浏览、ER 图、查询历史、图表和 Redis 工具 |
| **解决问题** | AI 诊断、EXPLAIN 分析、只读连接、SQL 安全门和事务编辑 |
| **移动数据** | Backup、Data Sync、Data Transfer、Schema Diff 和可审查执行计划 |
| **自动运行** | YAML Workflow、Ops Dashboard、MCP Server/Client 和 `--mcp-stdio` 无头模式 |

## 围绕真实数据库工作流构建

### SQL 与数据探索

在现代 SQL 编辑器中编写和执行 SQL，浏览 Schema 和数据，并直接在查询结果与图表之间切换。

![查询结果与图表](site/assets/screenshots/02-query-chart.png)

### AI 辅助数据库开发

DataZen 将 AI 放在数据库旁边，而不是让开发者不断复制 Schema 和错误信息到另一个应用中。

![AI 自然语言生成 SQL](site/assets/screenshots/03-ai-nl2sql.png)

**自然语言 → SQL**

用自然语言描述需求，DataZen 自动将当前数据库 Schema 作为上下文生成 SQL。生成结果可以直接执行，也可以填充到编辑器继续修改。

![AI SQL 错误诊断](site/assets/screenshots/05-ai-diagnosis.png)

**SQL 错误诊断**

SQL 执行失败后，AI 可以结合数据库错误和 Schema 分析原因，并给出修正后的 SQL。

![AI EXPLAIN 分析](site/assets/screenshots/06-ai-explain.png)

**EXPLAIN 分析**

将执行计划可视化，并通过 AI 识别执行瓶颈、扫描方式和潜在优化方向。

![AI Chat](site/assets/screenshots/07-ai-chat.png)

**带数据库上下文的 AI Chat**

AI Sidebar 可以自动获取当前连接的 Schema，上下文中的 SQL 也可以一键插入编辑器。

支持 OpenAI、Anthropic、DeepSeek 以及兼容协议的自定义 Endpoint。

## 查询结果直接变成图表

不需要为了分析数据先导出到 Excel。DataZen 可以根据查询结果自动推荐合适的图表配置，并在表格和图表之间快速切换。

![多种图表类型](site/assets/screenshots/10-chart-types.png)

支持折线图、柱状图、饼图、散点图和面积图，并支持聚合、分组以及 PNG / SVG 导出。

![图表导出](site/assets/screenshots/11-chart-export.png)

## 安全地移动和监控数据

DataZen 不把数据操作包装成不可逆的一键魔法，而是提供清晰、可审查的流程。

- **Data Sync**：同族数据库在结构和主键一致时比较并同步行数据。
- **Data Transfer**：通过映射和预览流程，在异构数据库之间迁移结构和/或数据。
- **Schema Diff**：比较数据库结构，并生成受控的 DDL 部署计划。
- **Ops Dashboard**：定时刷新保存的查询，保留运行历史，并触发阈值告警。

![数据迁移工具](site/assets/screenshots/26-data-sync-en.png)

## 使用 Workflow 自动化数据库工作

DataZen Workflow 使用 YAML 描述可复用的数据库操作，可以组合 Query、AI、Condition 和 Foreach，并让不同步骤使用不同数据库连接。

![Workflow 编辑器](site/assets/screenshots/04-workflow.png)

例如，一个 Workflow 可以从 PostgreSQL 查询订单，再从 MySQL 获取物流信息，最后让 AI 对组合结果进行总结。

![跨数据库 Workflow](site/assets/screenshots/12-workflow-crossdb.png)

Workflow 可以从 UI、AI Sidebar、MCP 运行，也可以由 AI 根据需求生成。

![Workflow 执行](site/assets/screenshots/13-workflow-run.png)

## MCP：连接 AI 工具生态

DataZen 同时支持 **MCP Server** 和 **MCP Client**。

### MCP Server

向外部 AI Agent 暴露数据库查询、Schema、EXPLAIN、Workflow 等能力，并支持 stdio headless 模式，方便自动化和 Agent 集成。

### MCP Client

将外部 MCP Server 接入 DataZen AI Chat，把文件系统、搜索以及第三方工具等能力带入数据库对话。

因此 DataZen 不只是一个 GUI，也可以成为更大的 AI 开发工作流中的数据库工具。

## 可扩展的数据库 Driver 架构

DataZen 通过 **DataZen Driver API** 将应用核心与具体数据库实现解耦。

```text
                         DataZen
                            │
              ┌─────────────┴─────────────┐
              │       DataZen Core        │
              │  UI · Query · AI · MCP   │
              └─────────────┬─────────────┘
                            │
                    DataZen Driver API
                            │
          ┌─────────────────┼─────────────────┐
          │                 │                 │
       PostgreSQL         MySQL          外部 Driver
                                           │
                              ┌────────────┼────────────┐
                              │            │            │
                           MongoDB      ClickHouse    OLAP...
```

Driver **不是通过不稳定的 Rust 动态库 ABI 在运行时加载**，而是在 DataZen 编译期被编译并链接进应用。因此 Driver 可以同时包含 Rust 数据库实现和前端页面，同时保持独立 Git 仓库。

### 独立 Driver 开发

开发 Driver 时，可以使用两个同级 Git 仓库：

```text
workspace/
├── datazen/
└── datazen-driver-mydb/
```

开发阶段 DataZen 的 Driver Registry 可以通过 `source: "path"` 指向本地 Driver 仓库。之后在 DataZen 中编译指定 Driver，即可使用真实 Host 同时调试 Driver 的 Rust 后端和前端页面。

Driver API 本身维护在 DataZen monorepo 的 `packages/driver-api` 中，并作为 MIT License 的 `datazen-driver-api` crate 发布。独立 Driver 通常直接依赖 crates.io 上的 API，不需要为了获得 API 而 clone DataZen。

详细文档：

- **[独立驱动开发指南 — 中文](docs/development/independent-driver-development.zh-CN.md)**
- **[Independent Driver Development — English](docs/development/independent-driver-development.en.md)**
- **[Driver API crate README](packages/driver-api/README.md)**
- **[Driver API 依赖边界](docs/development/driver-api-dependency-boundary.md)**
- **[datazen-driver-api on crates.io](https://crates.io/crates/datazen-driver-api)**

## 支持的数据库

DataZen 默认提供精简的 Driver 集合，也可以在编译时加入更多 Driver。

| 数据库 | 类型 | 说明 |
|---|---|---|
| PostgreSQL | 默认 | SQL、Schema、EXPLAIN、AI 上下文 |
| MySQL / MariaDB | 默认 | SQL、Schema、EXPLAIN |
| SQLite | 默认 | 嵌入式数据库工作流 |
| Redis | 默认 | Key 浏览、命令台、Monitor、Pub/Sub |
| MongoDB | 可选 | 原生 Driver |
| ClickHouse | 可选 | 原生 Driver |
| DuckDB | 可选 | 原生 Driver |
| SQL Server | 可选 | 原生 Driver |
| Presto / Trino 等 OLAP | Plugin | 外部 Driver 架构 |

Driver 集合由编译期配置决定，因此发行版不需要携带所有数据库引擎。

**更多外部驱动正在规划中。**未来将探索支持使用 Go、C++、Rust、Java 等语言实现和接入数据库 Driver，进一步扩展 DataZen 的数据库生态。

## 安装

从 **[下载 DataZen](https://flyxl.github.io/datazen/zh/download.html)** 获取自动匹配的平台安装包；也可以直接浏览 **[GitHub Releases](https://github.com/flyxl/datazen/releases)**。

| 平台 | 安装包 |
|---|---|
| macOS Apple Silicon | `.dmg` |
| macOS Intel | `.dmg` |
| Windows | NSIS `.exe` / Portable `.zip` |
| Linux x86_64 | `.deb` / `.rpm` / `.AppImage` |

DataZen 免费使用，不需要注册账号。

## 从源码构建

### 前置条件

- Node.js >= 20
- pnpm >= 9
- Rust >= 1.77
- Tauri v2 系统依赖

```bash
pnpm install
pnpm tauri dev
```

只构建需要的 Driver：

```bash
# 默认 Driver 集合
pnpm tauri:build

# 全部 path Driver
DATAZEN_DRIVERS=all pnpm tauri:build

# 自定义 Driver 集合
DATAZEN_DRIVERS=postgres,mongodb pnpm tauri:build
```

## 安全与隐私

DataZen 按照数据库访问场景设计：

- AI 请求发送到用户配置的 AI Provider。
- SSH 连接可以直接由应用建立。

使用 AI 功能时，请同时遵守你所配置的 AI Provider / Endpoint 的隐私和安全策略。

## 文档

- [项目官网](https://flyxl.github.io/datazen/zh/) · [使用手册](https://flyxl.github.io/datazen/zh/manual.html) · [User Manual (EN)](https://flyxl.github.io/datazen/manual.html)
- [功能文档](docs/features/) · [架构文档](docs/architecture/README.md) · [开发/发布文档](docs/development/)
- [独立驱动开发指南](docs/development/independent-driver-development.zh-CN.md)
- [English Driver Development Guide](docs/development/independent-driver-development.en.md)
- [Driver API crate](packages/driver-api/README.md)
- [Driver API 依赖边界](docs/development/driver-api-dependency-boundary.md)
- [datazen-driver-api on crates.io](https://crates.io/crates/datazen-driver-api)
- [Workflow 指南](docs/features/workflow-guide.zh-CN.md)
- [贡献指南](CONTRIBUTING.md)

## 参与贡献

欢迎提交 Bug、功能建议、数据库 Driver、文档改进以及代码贡献。

提交 Pull Request 前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。Driver 通常应该在独立 Driver 仓库中开发，再通过 DataZen Driver Registry 集成。

## License

DataZen 使用 **GNU General Public License v3.0** 开源。`packages/driver-api` 下的 `datazen-driver-api` crate 单独采用 **MIT License**。详见 [LICENSE](LICENSE) 和 [packages/driver-api/LICENSE-MIT](packages/driver-api/LICENSE-MIT)。

<div align="center">

**DataZen — 让 AI 处理数据库工作，让数据真正产生洞察。**

</div>
