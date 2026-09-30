<div align="center">
<img src="site/assets/logo.png" width="96" alt="DataZen" />

# DataZen

### 面向开发者与 AI Agent 的开源数据库工作台

**查询 · 排障 · 可视化 · 迁移 · 自动化 · 可被 Agent 驱动**

[![Release](https://img.shields.io/github/v/release/flyxl/datazen?style=flat-square)](https://github.com/flyxl/datazen/releases)
[![License](https://img.shields.io/badge/license-GPLv3-blue?style=flat-square)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-blue?style=flat-square)](#安装)

[下载](https://flyxl.github.io/datazen/zh/download.html) · [官网](https://flyxl.github.io/datazen/zh/) · [English](README.md) · [使用手册](https://flyxl.github.io/datazen/zh/manual.html) · [贡献指南](CONTRIBUTING.md)
</div>

<video src="site/assets/video/demo-recording.mp4" controls width="100%" poster="site/assets/video/demo-poster.png"></video>

<p align="center">
  <b>18 个数据库驱动</b> · <b>10 种界面语言</b> · <b>MCP Server 与 Client</b> · <b>无需账号、不依赖云端</b> · <b>macOS · Windows · Linux</b>
</p>

## 数据库要你做的所有事，一个应用装下

真实的数据库工作往往要在五个工具之间来回切换：一个看 Schema，一个写 SQL，一个做迁移，一个做看板，再把 AI 接进流程里。DataZen 是一个桌面应用，覆盖完整闭环——同时它也完整支持 MCP，你自己的 AI Agent 同样能驱动它。

| 任务 | DataZen 把什么放在一起 |
|---|---|
| **理解** | 懂 Schema 的 SQL 编辑器、对象树、ER 图、表结构、查询历史、图表、Redis 工具 |
| **排障** | AI 诊断、EXPLAIN 分析、只读连接、SQL 安全门网、事务感知的数据编辑 |
| **迁移** | 备份、数据同步、异构数据传输、结构比对——每一次变更都能先看再执行 |
| **监控** | Ops Dashboard：后台刷新、运行历史、阈值告警 |
| **自动化** | YAML Workflow（条件与循环），UI / MCP / 无头三处运行 |
| **扩展** | Driver API、主题包、沙箱 Workspace App、宿主扩展点 |

## 为什么选择 DataZen？

- **一个应用跑完整个闭环。** 连接、探索、编辑、迁移、可视化、自动化，不必中途把数据导出去。
- **AI 看得见你真实的 Schema。** 自然语言生成 SQL、错误诊断、执行计划分析都基于当前连接的真实库结构，而不是把 Schema 猜出来贴进聊天框。
- **迁移在执行前是可读的。** 结构比对产出 DDL 计划，数据同步与数据传输产出预览，破坏性操作不会突然发生。
- **接入你自己的 Agent。** DataZen 既是 MCP Server，把数据库能力开放给外部 Agent；也是 MCP Client，把外部 MCP 工具并进自己的 AI 对话。还可以用 `--mcp-stdio` 无界面启动，嵌进 Agent 流水线。
- **数据属于你。** 无需账号、无遥测、不依赖云端。AI 流量只发往你配置的 Provider——或者本地 Ollama 模型。凭据用 AES-256-GCM 加密。
- **开源，且为扩展而生。** 发布的 MIT 许可 Driver API、静态主题包、沙箱 Workspace App，以及宿主特权扩展点。

## 三分钟开始使用

1. [下载 DataZen](https://flyxl.github.io/datazen/zh/download.html)，安装对应你平台的安装包。
2. 打开一个本地 SQLite 文件，或新建 PostgreSQL、MySQL/MariaDB、Redis 连接。
3. 浏览 Schema，跑一条查询——或者让 AI 写一条——然后把结果一键切成图表。

首次启动的引导向导会带你完成第一次连接，并初始化一个示例 SQLite 数据库，让你先玩明白再指向真实数据。无需账号，AI 完全可选：不配 API Key 也能正常浏览与查询。

## 功能导览

### 真正懂你 Schema 的 SQL 编辑器

它不是一个只有语法高亮的文本框。它会读取你连接的数据库并据此工作：补全来自真实的表和列，悬浮提示展示真实的类型与注释，函数会在你打字时解释自己的参数。

| 能力 | 作用 |
|---|---|
| **Schema 感知补全** | 基于实时元数据的表、列、关键字建议 |
| **悬浮提示** | 悬停即见列类型、注释与表结构 |
| **签名帮助** | 输入函数时行内提示参数 |
| **语句装订线** | 多语句脚本中可单独执行任意一条 |
| **四种执行策略** | 执行当前语句 / 选中部分 / 全部 / 每次询问 |
| **SQL Linter** | 实时的语法与语义错误反馈 |
| **意图（Alt+Enter）** | 上下文感知的快速修复与重构 |
| **参数绑定** | 具名参数与类型化输入，告别字符串拼接字面量 |
| **代码片段** | 把每天重复跑的查询存成带参片段 |
| **事务控制** | 工具栏上的自动提交开关与手动提交 / 回滚 |
| **粘贴为 IN 子句** | 把粘贴的 CSV 值直接转成 `IN (...)` |
| **格式化 SQL** | 一键统一格式 |
| **流式结果** | 结果随数据库产出逐行到达，不必等全量返回 |

![SQL 编辑器](site/assets/screenshots/17-sql-editor.png)

**只执行你真正想执行的那条语句。** 选定执行策略，并让 DataZen 在危险语句执行前拦下它。

![SQL 安全门网](site/assets/screenshots/30-sql-editor-danger-guard.png)

### 可视化构建查询——或者让 AI 写

更想拖拽表来完成？可视化查询构建器提供子句列表、字段选择弹窗，以及可以连线成 JOIN 的画布。外键可以从元数据推断，且这一份推断结果由构建器、编辑器与 ER 图共用同一个开关。

![可视化查询构建器](site/assets/screenshots/14-query-builder.png)

而当你更想描述结果而不是查询本身，就用自然语言把它写下来。提问所依据的是你**已经连上的那份 Schema**——真实的表名与字段名，不是猜的——SQL 会流式写回编辑器，运行之前你可以随意修改。

![自然语言生成 SQL，基于已连接的 Schema 作答](site/assets/screenshots/03-ai-nl2sql.png)

### 查询报错时，问出真正错在哪

数据库的报错很少把话说清楚。DataZen 会把错误与 Schema 结合，说明原因并给出可直接执行的修正 SQL。

![AI 错误诊断](site/assets/screenshots/05-ai-diagnosis.png)

执行计划也是同样的处理：先读懂计划，再让 AI 指出代价高昂的扫描和背后缺失的索引。

![AI EXPLAIN 分析](site/assets/screenshots/06-ai-explain.png)

AI 侧边栏还可以进行懂 Schema 的对话：接着追问、拿到修正后的 SQL，再一键推回编辑器，成为可继续编辑的代码。

![懂 Schema 的 AI 对话，以及它产出的 SQL](site/assets/screenshots/07-ai-chat.png)

AI 是可选的，配置权在你：OpenAI、Anthropic、DeepSeek、Ollama（全本地模型），或任何 OpenAI 兼容端点。严格出网模式默认开启，生成中的任务随时可取消。

![AI 设置](site/assets/screenshots/09-ai-more.png)

### 把结果变成图表，再把图表变成监控

想看懂一份结果集，不该先导出到 Excel。DataZen 会从列结构推断出合适的图表，表格与图表之间可以随时切换。

![图表类型](site/assets/screenshots/10-chart-types.png)

支持折线、柱状、饼图、散点、面积图，可做聚合与分组，并支持 PNG/SVG 导出。

![图表导出](site/assets/screenshots/11-chart-export.png)

**Ops Dashboard** 更进一步：一块由图表组件组成的画布，每个组件绑定 SQL，按间隔后台刷新，保留运行历史，并在越过阈值时触发桌面通知或 webhook 告警。

![Ops Dashboard](site/assets/screenshots/21-dashboard.png)

### 带着可读的方案搬数据

数据操作是最容易事后后悔的一类，所以 DataZen 让它们保持显式与可审阅。

- **备份**——定时与按需的数据库备份，带进度日志。
- **数据同步**——在结构与主键完全一致的同族数据库之间比对并同步行数据。
- **数据传输**——通过映射与预览流程在异构数据库系统之间迁移结构与数据，含外键顺序感知与能力门控。
- **结构比对**——比较两个结构，产出可先审查再应用 DDL 的部署计划。

![数据同步](site/assets/screenshots/26-data-sync-en.png)
![结构比对](site/assets/screenshots/27-schema-diff-en.png)
![数据传输](site/assets/screenshots/28-data-transfer-en.png)

### 把手工重复的部分交给自动化

Workflow 用 YAML 描述数据库操作：查询、驱动命令、AI 步骤、条件、循环、合并与迁移。每个步骤都可以指定自己的连接与目标库，因此一个 Workflow 可以从 PostgreSQL 读订单、从 MySQL 取物流，再让 AI 汇总结果。

![Workflow 编辑器](site/assets/screenshots/04-workflow.png)
![跨库 Workflow](site/assets/screenshots/12-workflow-crossdb.png)

Workflow 可以从 UI、AI 侧边栏、MCP 启动，也可以由 AI 生成——还有间隔调度器负责定时备份与自动化任务。GUI、Tauri IPC 与 MCP 共用同一套执行 runtime，无论从哪里启动，行为完全一致。

### 真正的 Redis 工作台

Redis 在这里不是二等公民。按命名空间树或扁平列表浏览 Key，SCAN 游标保持当前位置，按类型过滤，查看 `MEMORY USAGE`，并以正确的 `KEEPTTL` 与 `EXPIREAT` 语义编辑值。压缩值（gzip、zlib、raw deflate、base64）会被解码查看，JSON 自动格式化。

![Redis 键浏览器](site/assets/screenshots/15-redis.png)
![Redis 控制台工作台](site/assets/screenshots/35-redis-workbench.png)

Console、Slowlog、Pub/Sub 与实时 MONITOR 面板并排就位，Console 命令采用 fail-closed 的危险分类，并在适用时感知集群与哨兵模式。

### 看见数据的形状

![ER 图](site/assets/screenshots/16-er.png)

### 连上无法直连的数据库

有时数据库主机只能通过跳板机、企业代理或 WebSocket 中继访问。DataZen 会在本机开一个回环端口转发连接，驱动看到的只是一个改写后的地址。

- **SSH**——密码、私钥或 SSH Agent，支持 ProxyJump
- **HTTP 代理**——经 HTTP 或 HTTPS 代理做 `CONNECT` 隧道
- **WebSocket**——经 `ws://` 或 `wss://` 中继转发，支持 Bearer Token

已保存的隧道与其余凭据一起加密，并集中管理。

### 接入你自己的 AI Agent

DataZen 既是 **MCP Server**，也是 **MCP Client**。

**作为 Server**，它把数据库本身暴露出去：`list_databases`、`list_tables`、`search_tables`、`describe_table`、`get_schema`、`query`、`explain_query`，以及 `list_workflows` 与 `run_workflow`。Schema、连接与查询历史资源通过 `datazen://` URI 寻址。无头 stdio 模式（`--mcp-stdio`）不开窗口运行同一套工具，可用于 Agent 流水线与 CI。

**作为 Client**，它连接外部 MCP Server，把对方的工具与上下文并入 DataZen 的 AI 对话。

### 隐私与安全是默认项

- AI 请求只发往你配置的 Provider——或者本地 Ollama 模型。
- 连接凭据静态加密（AES-256-GCM），主密钥存放在系统钥匙串。
- 只读连接、SQL 安全门网与显式执行策略，让误操作的语句不至于造成破坏。
- Workspace App 运行在沙箱中；主题是纯静态资源，不执行任何代码。

![安全](site/assets/screenshots/20-security.png)

## 为扩展而生

DataZen 把应用与所有数据库相关、所有展现相关的东西彻底分开：

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
        PostgreSQL         MySQL          外部驱动
                                            │
                              ┌────────────┼────────────┐
                              │            │            │
                           MongoDB      ClickHouse    SQL Server...
```

- **驱动**实现 **DataZen Driver API**，同时带来 Rust 能力与前端 UI。驱动是编译进 DataZen 的，而不是通过不稳定的 Rust 动态库 ABI 加载，因此驱动可以独立仓库开发，同时交付前后端两半。
- **主题**是静态资源包——manifest、设计 token、编辑器与图表配置、图标——零代码执行。
- **扩展点**是宿主特权插槽，承载那些需要对编辑器延迟预算做深度集成的功能。
- **Workspace App** 是沙箱全屏应用，为工作区贡献页面与主题。
- **`@datazen/ui`** 是宿主与插件共用的设计系统。

![Workspace App](site/assets/screenshots/22-wapps.png)

### 编写你自己的驱动

驱动可以在独立仓库中开发，与本地 DataZen 检出并排放置，并通过驱动注册表中的 `source: "path"` 接入：

```text
workspace/
├── datazen/
└── datazen-driver-mydb/
```

多数独立驱动直接依赖已发布的 MIT 许可 `datazen-driver-api` crate，甚至不需要检出 DataZen。

- **[独立驱动开发指南 — 中文](docs/development/independent-driver-development.zh-CN.md)**
- **[Independent Driver Development — English](docs/development/independent-driver-development.en.md)**
- **[Driver API crate README](packages/driver-api/README.md)**
- **[Driver API 依赖边界](docs/development/driver-api-dependency-boundary.md)**
- **[crates.io 上的 datazen-driver-api](https://crates.io/crates/datazen-driver-api)**

## 支持的数据库

| 数据库 | 构建集合 | 说明 |
|---|---|---|
| PostgreSQL | basic | SQL、Schema 浏览、EXPLAIN、AI 上下文、多库 |
| MySQL / MariaDB | basic | SQL、Schema 浏览、EXPLAIN、多库 |
| SQLite | basic | 嵌入式与文件型工作流 |
| Redis | basic | Key 浏览器、Console、工作台、Slowlog、Pub/Sub、MONITOR |
| MongoDB | `all` | 文档数据库 |
| SQL Server | `all` | T-SQL 方言，已活体验证 |
| ClickHouse | `all` | HTTP 接口，多库 |
| DuckDB | `all` | 嵌入式分析 |
| Elasticsearch | `all` | 检索后端 |
| InfluxDB / VictoriaMetrics | `all` | 时序数据库 |
| RQLite / Turso | `all` | 分布式与边缘 SQLite |
| HBase | `all` | REST 接口 |
| 通用向量数据库 | `all` | HTTP 向量端点 |
| Kiwi（云数据库代理） | git 驱动 | 代理的云实例 |
| Presto / Trino | git 驱动 | OLAP 引擎 |
| Superset | git 驱动 | 数据探索平台 |

`basic` 是四个核心驱动，`all` 是全部已注册的 path 驱动；git 驱动需显式选择，例如 `--drivers=basic,kiwi,superset`。驱动集合是构建期决定的，因此一个发行包不必携带所有引擎。协议允许时，线兼容引擎会顺带支持——QuestDB/Cloudberry 走 PostgreSQL 协议，Doris/StarRocks/OceanBase 走 MySQL 协议。

## 十种界面语言

English、简体中文、繁體中文、日本語、한국어、Deutsch、Español、Français、Português (BR)、Русский。

## 安装

从 **[下载 DataZen](https://flyxl.github.io/datazen/zh/download.html)** 获取对应平台的安装包，或直接浏览 **[GitHub Releases](https://github.com/flyxl/datazen/releases)**。

| 平台 | 安装包 |
|---|---|
| macOS Apple Silicon / Intel | `.dmg` |
| Windows | NSIS `.exe` / 便携 `.zip` |
| Linux x86_64 | `.deb` / `.rpm` / `.AppImage` |

DataZen 免费，无需账号。

**macOS Gatekeeper：** 若提示应用已损坏或来自未识别开发者，执行 `xattr -cr /Applications/DataZen.app` 清除隔离，或右键 → 打开。详见 [packaging.md](docs/development/packaging.md)。

## 从源码构建

前置条件：Node **24**、pnpm **11**、Rust **stable**（CI 验证过的工具链），以及 [Tauri v2 系统依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
pnpm install
pnpm tauri:dev --drivers=basic
```

按需构建发行包：

```bash
pnpm tauri:build:community --drivers=basic     # 四个核心驱动
pnpm tauri:build:community --drivers=all       # 全部已注册的 path 驱动
pnpm tauri:build:community --drivers=postgres,mongodb
```

源码构建编译的是 DataZen 的开源版编辑器；官方发布的安装包自带上文所述的完整编辑器体验。完整开发流程见 [CONTRIBUTING.md](CONTRIBUTING.md) 与 [CI 与测试矩阵](docs/development/ci-test-matrix.md)，驱动选择细节见 [optional-drivers.md](docs/development/optional-drivers.md)。

## 文档

- [官网](https://flyxl.github.io/datazen/zh/) · [使用手册 (ZH)](https://flyxl.github.io/datazen/zh/manual.html) · [User Manual (EN)](https://flyxl.github.io/datazen/manual.html)
- [功能指南](docs/features/) · [架构文档](docs/architecture/README.md) · [开发与发布文档](docs/development/)
- [Workflow 指南](docs/features/workflow-guide.zh-CN.md) · [可视化查询构建器](docs/features/query-builder.md) · [Ops Dashboard](docs/features/ops-dashboard-guide.zh-CN.md) · [结构比对部署](docs/features/schema-diff-deploy.md) · [隧道指南](docs/features/tunnel-guide.zh-CN.md)
- [独立驱动开发（中文）](docs/development/independent-driver-development.zh-CN.md) · [English](docs/development/independent-driver-development.en.md)
- [Driver API crate](packages/driver-api/README.md) · [依赖边界](docs/development/driver-api-dependency-boundary.md) · [crates.io](https://crates.io/crates/datazen-driver-api)
- [贡献指南](CONTRIBUTING.md)

## 参与贡献

DataZen 欢迎 bug 反馈、功能建议、数据库驱动、文档改进与代码贡献。提交 PR 前请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。驱动相关工作通常在独立驱动仓库中开发，再通过驱动注册表集成。

## 许可

DataZen 主体基于 **GNU General Public License v3.0 或更高版本** 许可，并附带 [插件、驱动与扩展链接例外条款](LICENSE)：仅依赖公开 SDK 构建的独立驱动、主题、扩展点与 Workspace App，可按其作者自己的条款发布。`packages/driver-api` 下的 `datazen-driver-api` crate 单独采用 **MIT 许可**。详见 [LICENSE](LICENSE) 与 [packages/driver-api/LICENSE-MIT](packages/driver-api/LICENSE-MIT)。

<div align="center">

**DataZen —— 让 AI 处理数据库工作，让数据变成洞察。**

</div>
