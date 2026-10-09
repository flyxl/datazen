# DataZen 文档索引

DataZen 文档只保留两类长期有效内容：**当前功能使用文档**和**当前代码架构/开发文档**。开发过程中的 PRD、进度、Bug List、UI Review、临时 Backlog 等不作为仓库文档长期维护。

## 文档结构

| 目录 | 定位 | 读者 |
|---|---|---|
| [features/](features/) | 当前已实现功能的使用说明 | 用户 / 开发者 |
| [architecture/](architecture/) | 与 main 分支代码对应的架构说明 | 贡献者 / AI 助手 |
| [development/](development/) | 开发、测试、发布、驱动开发流程 | 贡献者 |
| [blogs/](blogs/) | 面向公开发布的架构文章 | 开发者 / 用户 |
| [bugs/](bugs/) | **已知但未修复**的缺陷：现象、影响面、链路实测、修复前置条件 | 贡献者 / AI 助手 |

不在仓库内的目录：

- `posts/` — 推广与发布文案（Product Hunt / V2EX / 头条 / 各版本发布说明）。已在 `.gitignore` 中，属一次性对外物料，不参与构建、不被 CI 校验。
- 本地开发台账（子代理进度、缺陷清单）不落仓库：结论直接写进代码、测试与 `docs/` 正式文档。例外是**已核实复现、且本轮明确不修**的缺陷，按上表的 `bugs/` 单独记录——它写的是可逐行核验的代码事实，不是过程产物。

## 功能文档

- [Workflow](features/workflow-guide.zh-CN.md) / [English](features/workflow-guide.en.md)
- [Schema Diff](features/schema-diff-guide.zh-CN.md) / [English](features/schema-diff-guide.en.md)（[Deploy 索引](features/schema-diff-deploy.md)）
- [Data Sync](features/data-sync-guide.zh-CN.md)
- [Data Transfer](features/data-transfer-guide.zh-CN.md)
- [Ops Dashboard](features/ops-dashboard-guide.zh-CN.md) / [English](features/ops-dashboard-guide.en.md)
- [Tunnel](features/tunnel-guide.zh-CN.md)
- [Query Builder](features/query-builder.md)（SQL Editor Pro 能力）

功能文档描述当前 main 已实现的行为；如果某能力尚未实现，不在这里记录未来计划。

## 架构文档

入口：[architecture/README.md](architecture/README.md)

### 后端

- [Drivers](architecture/backend/drivers.md)
- [Services](architecture/backend/services.md)
- [Tunnel](architecture/backend/tunnel.md)
- [Commands](architecture/backend/commands.md)
- [Cache](architecture/backend/cache.md)
- [Store](architecture/backend/store.md)
- [AI](architecture/backend/ai.md)
- [MCP](architecture/backend/mcp.md)
- [Workflow](architecture/backend/workflow.md)
- [Dashboard](architecture/backend/dashboard.md)
- [Data Transfer](architecture/backend/data-transfer.md)
- [Data Sync](architecture/backend/data-sync.md)
- [Schema Diff](architecture/backend/schema-diff.md)
- [Wapps](architecture/backend/wapps.md)（含主题应用；旧 `theme.md` 已合并至此）

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

### 桌面 / Web 平台演进设计（待实现）

以下文档按用户明确要求保存，是本次设计交付的范围例外。它们标明基线和目标状态，不代表当前 main 已支持团队 Web；落地后应改写为实现事实与开发流程。

概要、契约与计划：

- [系统概要设计](architecture/platform/system-overview.md)：三种运行方式、模块职责、交互与接口。
- [连接管理详细设计](architecture/platform/connection-management.md)：数据契约、状态机、算法、验收标准与 74 个测试用例，是接口契约、错误码与验收的唯一权威。
- [分阶段开发计划](development/platform-development-plan.md)：连接重构、多应用形态、团队服务和多 worker 的依赖与交付门槛，是阶段→补充契约归属的唯一权威。

补充详细设计（P0–P2）：

- [共享应用边界与端口详细设计](architecture/platform/shared-boundaries-and-ports.md)：包边界与依赖矩阵、runtime 内部模块、端口 trait 签名、请求上下文三来源与 Tauri 组装根。
- [P0 假资源夹具与基准 harness 详细设计](architecture/platform/fake-runtime-fixtures.md)：transport-neutral fake 的资源契约、F1～F12 故障注入矩阵、CommandJournal、Barrier/DrainBarrier 与 FakeClock。
- [持久化模型详细设计](architecture/platform/persistence-model.md)：可落盘白名单与禁落盘清单、管理库表的 DDL 与 CHECK、本地 Store ↔ 服务端 DB 映射、schema 版本与 expand/contract。
- [驱动能力迁移详细设计](architecture/platform/driver-capability-migration.md)：path 能力基线、迁移批次、协议门槛、adapter 退役及 P10 path/Git 发布验收。

补充详细设计与运维流程（P5–P10）：

- [迁移三件套与 JobRuntime 详细设计](architecture/platform/data-migration-jobs.md)：准备/审阅/应用、计划唯一消费、多端预算、提交边界与跨进程恢复。
- [消费者接入详细设计](architecture/platform/consumer-adapters.md)：AI 共享授权、MCP/Wapp 归属、Dashboard/Monitor 服务身份与原生工具。
- [多 worker 协调详细设计](architecture/platform/multi-worker-coordination.md)：目录 CAS/路由、claim fencing、节点预算账、分区核销与 drain。
- [团队服务部署、升级与恢复流程](development/team-service-operations.md)：配置与健康检查、发布兼容、备份一致性、恢复隔离与演练。
- [Workflow 资源模型详细设计](architecture/platform/workflow-resource-model.md)：step / session block / transaction block 三种执行单元、目标继承链、Session/Lease/Budget 集成与版本兼容边界。
- [团队 Web 服务与认证详细设计](architecture/platform/team-server-and-auth.md)：server crate 形态、中间件链、OIDC/CSRF/RBAC、SSE 回放与错误到 HTTP 的映射。

### 数据浏览竞品对标与优化方案（一次性方案，范围例外）

以下文档同样是**按用户明确要求保存的范围例外**，不属于长期维护的 `features/` / `architecture/` / `development/` 三类。它记录的是对标结论与优化方案，而非已实现事实；方案落地后应逐节改写为已实现事实并入 [architecture/frontend/components.md](architecture/frontend/components.md) 与 [features/](features/)，随后删除本文件。

- [数据浏览（Data Browsing）竞品对标与优化方案 PRD](prd/data-browsing-optimization-prd.md)：DataZen 与 TablePlus 在数据浏览维度的差距矩阵（65 项：P0 24 项 / P1 17 项 / P2 12 项 / 持平 12 项，另含 7 项 DataZen 领先项与 6 项双方皆缺的差异化机会）、根因分析、20 个优化方案与 `driver-api` 扩展设计。文内代码引用一律采用「文件 + 符号」形式，不含行号。
- [数据浏览技术设计总纲](prd/data-browsing-design.md)：把 P0 方案拆成可独立开工、独立验收、独立回滚的 12 份分册，含依赖图、阅读路径、提交切分与「完成定义」。适合作为实际开发入口。
- [数据浏览技术设计分册](prd/data-browsing-design/)（13 份）：`00-contracts`（冻结的共享契约，**权威来源**：`CellWrite` 三态、`driver-api` 新增方法签名、IPC 变更、错误前缀机制、单元格坐标系与选择模型、共享热文件行数总账）、`01-selection`、`02-keyboard`、`03-clipboard`、`04-insert`、`05-editors`、`06-dirty-cells`、`07-fk-navigation`、`08-filters`、`09-count-and-order`、`10-result-grid`、`11-keyset-paging`、`12-testing`。每份都写到「照着做即可」的粒度：现状事实、数据结构、交互状态机、逐步实现、文件级改动清单（含预估行数与 800 行上限核算）、边界与异常、i18n key、测试清单、自查清单、待裁定项。
  - 契约册优先：分册与契约冲突时以契约为准；契约册第 9.1 节的**共享热文件行数总账**是跨分册行数账目的唯一权威（多份分册会改同一批文件，各册独立申报必然互相矛盾）。
  - 分册正文引用的既有符号与路径都经过机械核查（结构完整性、仓库路径是否存在、标识符是否真实存在、公共类型副本是否与契约逐字一致），核查脚本只放系统临时目录、不入库。

## 开发与发布

- [E2E Testing](development/e2e-testing.md)
- [E2E Coverage](development/e2e-coverage.md)
- [驱动测试覆盖矩阵](development/driver-test-coverage.md)
- [E2E IPC Migration](development/e2e-ipc-migration-guide.md)
- [CI Test Matrix](development/ci-test-matrix.md)
- [CI Private Drivers](development/ci-private-drivers.md)
- [Independent Driver Development](development/independent-driver-development.en.md) / [中文](development/independent-driver-development.zh-CN.md)
- [Driver API Dependency Boundary](development/driver-api-dependency-boundary.md)
- [Optional Drivers](development/optional-drivers.md)
- [External Contract Policy](development/external-contract-policy.md)
- [Panic Policy](development/panic-policy.md)
- [Interaction & Testing Principles](development/interaction-and-testing-principles.md)
- [Wapp Development](development/wapp-development.zh-CN.md)（中文）
- [SQL Editor Pro Development](development/sql-editor-pro-development.zh-CN.md)（中文）
- [Packaging](development/packaging.md)
- [Updater](development/updater.md)
- [GitHub Pages](development/github-pages.md)

## 公开架构文章

见 [blogs/README.md](blogs/README.md)。

## 维护规则

1. 文档中的文件路径、命令、IPC 名称和能力矩阵必须以 main 分支代码为准。
2. 已实现功能写入 features；架构事实写入 architecture；开发流程写入 development。
3. 临时实施计划、PRD、设计稿、原型图、进度、Bug List 和评审记录不提交到长期文档索引。
4. 设计提案不长期留在 `architecture/`：要么在 `architecture/` 落为「已实现」的事实文档，要么不入库。
5. 对外发布文案写入本地 `posts/`，不提交。
6. 删除或重构代码时，同步删除失效文档引用。
7. **不新增带行号的代码引用。** 禁止写「`文件名` + `:行号`」这种形式的引用；定位一律写成「**文件 + 符号名 / 小节名**」，例如 `scripts/check-driver-type-isolation.mjs` 的 F-01 规则 `forbiddenCrates`、`docs/development/platform-development-plan.md` §6「P2：Driver 固定资源与可选能力契约」。行号是一次性坐标，代码增删一行即失效，而**没有任何门禁校验它**——实测把 `docs/architecture/platform/shared-boundaries-and-ports.md` 里指向 `.mjs` 第 99 行的引用全部改成不存在的第 14001 行，5 个门禁依旧全部 EXIT=0。**错的行号比没有行号更糟**：读者会信任这个精确坐标，跳过去发现对不上，然后连累整篇文档一起被降权；没有行号时读者反而会自己找符号。
8. **复核既有引用前，先确认被引用对象是谁。** 判断一条行号是否失效，第一步不是打开那个行号看内容，而是先确认这条引用说的究竟哪个文件——它的路径、所在 revision、总行数落在哪个量级。**两个方向都要走这一步**：判定「已失效」之前要确认对象，否则会把本来正确的引用改坏；判定「仍然正确」之前同样要确认，否则会漏掉真的漂移了的那一条。**候选对象之间文件名相近、行数接近时，逐个都量，不要挑一个就开始找证据**——E2E 预置脚本里 `e2e/setup-e2e-env.sh` 与 `e2e/setup-sync-dbs.sh` 同在一条调用链上、职责也都是建库，复核时分别只有 125 行和 117 行；一次真实的误判就发生在把它们当成同一个文件数行数的场合，而那批引用最终被证明**七处全部准确、无一失效**——这个结论只有在数对文件之后才成立。**确认对象是这类判断的前置条件，不是可选的核查步骤。** 复核确认引用确已失效时，**不要把它重新锚回它「曾经所在的那一行」**：那一行现在装的是别的内容，补一个新行号只是让它接着腐烂；只能锚回它原本想指的那个东西，即脚本 + 段落 + 语句，与第 7 条同形。
