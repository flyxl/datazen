# DataZen v0.1.x Desktop 产品定义（PRD）

> **状态**：Draft
> **目标版本**：v0.1.x
> **产品形态**：Desktop Standalone
> **产品主题**：开发者优先的 SQL-first 数据库客户端稳定版
> **实施方案**：[desktop-v0.1x-implementation.zh-CN.md](../development/desktop-v0.1x-implementation.zh-CN.md)

## 1. 文档目的

本文定义 DataZen v0.1.x 的产品范围、目标用户、核心使用场景、功能优先级、非目标和发布验收标准。

v0.1.x 的任务不是提前实现 Web 平台，而是把现有 Desktop 能力收敛为一个可靠、顺手、可持续扩展的开发者数据库工具，为 v0.2.0 Web 平台化提供稳定的 Core 和前端基础。

## 2. 产品定位

DataZen v0.1.x 是一个面向开发者的多数据库桌面工作台：

```text
连接数据库 → 浏览 Schema → 编写 SQL → 执行与调试 → 查看/修改结果 → 验证变更
```

核心产品形态是：

> **SQL-first database client**

它不是优先面向业务人员的 BI 工具，也不是面向全企业用户的数据库治理平台。

## 3. 目标用户

### 3.1 第一目标用户：开发者

开发者是 v0.1.x 的核心用户，典型任务是：

```text
开发订单功能
  → 查找相关表和字段
  → 编写 SQL
  → 执行并处理错误
  → 使用 EXPLAIN 分析性能
  → 修改测试数据
  → 验证应用行为
```

DataZen 当前最匹配开发者的能力包括：

- SQL Editor
- Schema Explorer
- 多数据库连接
- Result Grid 和数据编辑
- EXPLAIN
- SQL Error Diagnosis
- AI NL2SQL 与上下文引用
- MCP
- Workflow
- Driver architecture

### 3.2 第二目标用户：DBA

DBA 是重要的次级用户，重点使用：

- Schema、Index 和对象浏览
- SQL 执行与 EXPLAIN
- 多数据库连接管理
- Schema Diff
- Data Transfer
- 连接和权限相关操作

v0.1.x 服务 DBA 的目标是提升日常操作效率，不追求替代成熟 DBA 专业平台的全部能力。

### 3.3 第三目标用户：测试人员

测试人员主要使用：

- 查询和验证业务数据
- 修改测试数据
- 导入/导出数据
- 复制一行数据并生成 INSERT/UPDATE
- 执行 Workflow 重复验证场景

测试场景是 v0.1.x 的重要增长方向，但不改变产品的 SQL-first 定位。

### 3.4 非核心用户

数据分析师有一定潜力，但当前还不是 v0.1.x 的主要战场。运营人员和产品经理不作为核心目标用户。

面向运营的参数化 Saved Query、面向分析师的分析画布和面向产品经理的自助数据门户，留待后续独立产品方向评估。

## 4. v0.1.x 目标

### 4.1 用户目标

- 开发者可以快速连接常用数据库并定位目标表、字段和索引。
- 开发者可以稳定执行 SQL，获得清晰的结果、错误和取消反馈。
- 开发者可以在同一应用工作台完成查询、数据修改、性能分析和结果验证。
- 测试人员可以低风险地准备和恢复测试数据。
- 用户可以通过 Query History 找回近期 SQL 和执行结果摘要。
- AI 能够帮助生成 SQL、解释错误、分析 Schema 和提出修复建议。

### 4.2 工程目标

- Core 执行边界稳定，不依赖 Tauri、用户身份、workspace 或 Web 审计。
- SQLite 本地持久化可靠，升级不破坏现有 Desktop 数据。
- Driver Command API、Workflow 和前端页面使用统一语义。
- 关键 Desktop 用户路径具备自动化测试和可诊断错误信息。
- v0.2.0 可以在不重写 Desktop 领域逻辑的前提下接入 Web Transport。

## 5. 非目标

v0.1.x 不做以下事情：

- Web Server、Web UI 和浏览器 Transport。
- users、workspace、membership、RBAC 和企业组织模型。
- 企业席位授权、许可证服务和私有化计费。
- 合规级 SQL Audit；Desktop 只保留 Query History 和必要的运行日志。
- 企业版 Desktop 或 Desktop Agent。
- AI 根据自然语言自动生成并直接运行完整 Workflow。
- 面向分析师的大型图表分析体验重构。
- 面向运营和产品经理的无 SQL 自助查询门户。
- 多节点、高可用、后台 Worker 和 Web 端集中调度。

现有 Dashboard、Workflow、Schema Diff、Data Sync、Data Transfer 等能力可以继续维护和修复，但不在 v0.1.x 进行大范围产品扩张。

## 6. 功能优先级

### 6.1 P0：开发者每日必经路径

#### 连接管理

- 创建、编辑、删除和测试数据库连接。
- 支持当前核心数据库驱动和统一 Driver Registry。
- 区分持久化 `connectionId` 与运行时 `dbSessionId`。
- 连接失败时展示可操作的错误信息。
- 凭据继续使用现有加密存储和系统钥匙串策略。

#### Schema Explorer

- 展示 database、schema、table、column、index 等核心对象。
- 支持按名称搜索和快速定位表、字段。
- 展示字段类型、是否可空、默认值、主键和索引信息。
- 支持从 Schema 对象快速生成 SELECT、INSERT、UPDATE 或 DDL 草稿。
- Schema 加载失败、过期和刷新状态必须可见。

#### SQL Editor

- 支持多 Tab SQL 编辑。
- 支持查询、执行、取消、超时和流式结果。
- 支持多语句拆分，并明确展示 statement index。
- 执行前显示当前连接和数据库上下文。
- 保留 SQL 编辑器中的参数、选区和错误定位能力。
- SQL 执行不因 UI 关闭而留下不可解释的前端状态。

#### Result Grid

- 稳定展示大结果集和分页/流式数据。
- 显示列类型、NULL、长文本和 JSON 等常见值。
- 支持复制、导出和列宽调整。
- 对可编辑结果提供清晰的 dirty state、预览和提交反馈。
- 破坏性修改必须有 Safe Mode 或显式确认。

#### Query History

- 保存近期提交的 SQL、连接、时间、耗时、状态和错误摘要。
- 支持搜索、重新打开和复制 SQL。
- 允许用户清理历史记录。
- 明确标注 Query History 不是合规级 Audit。
- 不默认记录数据库密码、AI Key 或未脱敏的大量结果数据。

#### 错误诊断与性能分析

- 统一展示 Driver 错误、SQL 错误、网络错误和超时错误。
- 支持 EXPLAIN 的驱动能力发现和结果展示。
- 提供 SQL Error Diagnosis 的可读解释、可能原因和修复方向。
- 错误信息包含必要的 statement index、连接和上下文，但不泄露凭据。

### 6.2 P1：提升开发、DBA 和测试效率

#### AI 辅助

- 支持自然语言生成 SQL。
- 支持引用当前连接、Schema、表和列作为上下文。
- 支持解释 SQL、解释执行错误和建议优化方向。
- 支持用户确认后写入编辑器，不默认直接执行高风险 SQL。
- AskQuestion 作为可复用协议稳定下来，为 v0.2.0 AI Workflow 做准备。

v0.1.x 的 AI 目标是“提高 SQL 编写和诊断效率”，不是“替代用户完成执行编排”。

#### 测试数据操作

- 从 Result Grid 复制一行或多行数据。
- 根据修改后的字段生成 INSERT/UPDATE 草稿。
- 执行前展示变更预览和影响范围。
- 支持在事务或 Safe Mode 下完成高风险测试数据修改。
- 对常见测试准备动作提供可复用 Workflow 入口。

#### Workflow 稳定性

- 稳定现有 YAML Workflow 的加载、编辑、执行、取消和错误展示。
- 使用统一 Driver Command API，不在 Workflow 中硬编码驱动方言。
- 支持变量、连接继承和基础错误策略。
- 执行结果可以回到 Query History 或结果面板查看。
- 不在 v0.1.x 引入自然语言直接生成完整 Workflow 的自动化流程。

#### DBA 日常能力

- Schema Diff、Data Sync、Data Transfer 的现有路径保持可用。
- 连接树、对象菜单和命令发现统一使用 Driver Command API。
- EXPLAIN、对象 DDL、索引和权限相关信息展示保持一致。
- 复杂 DBA 能力以稳定性和错误可解释性为先，不追求功能数量。

### 6.3 P2：探索性能力

- 增强图表和 Dashboard 的分析体验。
- 参数化 Saved Query，降低非 SQL 用户使用门槛。
- 面向分析师的数据探索和报告流程。
- Query History 的标签、收藏和项目化组织。
- 本地加密操作记录或企业端点审计。

P2 需求只有在 P0 路径稳定且有真实用户反馈后进入排期。

### 6.4 基于 main 代码 Review 的 UX 改造清单

以下需求来自对当前主分支核心 UI 的 review，作为 v0.1.x 的具体体验改造项，而不是另起一套产品方向。

#### P0：必须进入核心闭环

1. **快速找到连接**：连接列表支持搜索、最近使用和收藏优先展示；Group 继续保留，但不能作为唯一的组织方式。
2. **数据库对象全局搜索**：提供一级入口，至少覆盖 `table`、`column`、`view`、`function`，结果可直接打开对象或生成 SQL。
3. **统一 Table Panel 上下文**：复用现有 `TablePanel/SubTabBar` 组织 Data、Structure、Indexes、Foreign Keys、DDL；从表打开 SQL 时沿用同一 connection/database/schema/table 上下文，避免跳转后丢失定位。
4. **数据编辑暂存**：编辑不直接提交，形成可见的 pending changes 集合，支持修改、预览 SQL、Commit 和 Rollback。
5. **删除安全门槛**：普通 Delete/Backspace 只创建删除标记；提交必须使用明确操作（如 `Cmd/Ctrl + Enter`）并经过确认，不能直接执行 DELETE。
6. **快速过滤**：支持直接输入 `status = 'paid' AND amount > 100` 一类表达式；结构化 Column/Operator/Value 作为可视化辅助，而不是唯一入口。
7. **过滤与分页一致**：简单条件直接生效；高级组合条件可以 Apply。过滤条件生效后自动回到第一页，并明确显示当前过滤状态。
8. **SQL 上下文压缩**：将多个层级 Select 收敛为 breadcrumb/context selector，减少对 SQL Editor 横向空间的占用。
9. **执行状态贴近 Execute**：始终显示 `Running / Cancel / duration / rows / Error` 等状态，不能只依赖底部 StatusBar；Cancel 必须按 driver capability 呈现，不支持时明确说明且不可点击。
10. **错误快捷动作**：Query Error 直接提供 `Copy Error / Explain / Fix SQL / Retry`，其中 Explain 和 Fix SQL 复用现有 AI 能力。
11. **连接表单分层**：默认只显示 4～5 个必填项，按 `Basic → Advanced → SSH` 分层展开；Object Filter 等低频设置不得干扰首次连接。
12. **从 Table 生成 SQL**：对象入口直接提供 `Open Data / SELECT / INSERT / UPDATE / DDL`，生成内容进入当前 SQL Tab 并带上连接上下文。
13. **Context Menu 分层**：DataTable 一级菜单只保留复制、编辑、过滤、导出等高频操作；JSON、INSERT、UPDATE、CSV、NULL、批量操作等进入二级菜单。

#### P1：完成结果工作区和效率增强

14. **Result Workspace**：查询结果以统一结果工作区承载 Table 和 Chart 两个 View；基础的 Table/Chart 切换进入 v0.1.x，面向分析师的高级图表编排留到 P2。
15. **测试数据闭环**：将 pending changes、SQL Preview、Commit/Rollback 与“复制行 → 修改 → 生成 INSERT/UPDATE → 验证”流程打通。

## 7. 核心用户流程

### 7.1 开发者：从 Schema 到 SQL 验证

```text
打开连接
  → 搜索表/字段
  → 生成或编写 SQL
  → 执行
  → 查看 Result Grid
  → 遇到错误时诊断
  → 使用 EXPLAIN 或 AI 优化
  → 修改代码或测试数据
  → 保存/重用 Query History
```

验收重点：整个流程不需要在多个窗口和不同数据模型之间来回切换；连接、对象、表、SQL、结果和错误均有直接入口，任何失败都能说明原因和下一步操作。

### 7.2 测试人员：准备测试数据并验证

```text
查询目标数据
  → 复制一行
  → 修改字段
  → 预览 INSERT/UPDATE
  → 显式确认
  → 执行并记录结果
  → 重新查询验证
```

验收重点：避免误修改生产数据，明确展示影响行数、失败原因和是否需要回滚。

### 7.3 DBA：定位对象并分析 SQL

```text
打开连接
  → 浏览对象和索引
  → 查看 DDL
  → 执行 SQL
  → EXPLAIN
  → 对比或迁移结构/数据
```

验收重点：驱动能力差异必须通过能力发现和清晰提示表达，不通过 UI 假设所有数据库行为一致。

## 8. 产品原则

### 8.1 SQL-first

SQL Editor、Schema Explorer、Result Grid、Query History 和错误诊断是最核心的产品闭环。图表、Dashboard 和无 SQL 入口不能牺牲 SQL 主流程的效率。

### 8.2 结果优先于装饰

优先解决连接成功率、SQL 执行反馈、错误可理解性、结果可操作性和数据修改安全性，再扩展视觉化和外围入口。

### 8.3 统一能力，不复制逻辑

- 前端通过统一命令和 Store 使用 Core 能力。
- Workflow、AI、MCP 和 GUI 复用 Driver Command API。
- 驱动差异由 Driver metadata 和 command definitions 表达。
- 不在 Host 中按具体驱动 ID 编写分支。

### 8.4 安全默认值

- 危险 SQL 显式确认。
- Safe Mode 默认保护高风险修改。
- 凭据不进入 SQL、普通日志和 AI Prompt。
- Query History 和错误日志默认脱敏。
- 执行取消是 best-effort，结果状态必须诚实表达，不伪造成功或失败。

### 8.5 为 Web 演进，但不提前 Web 化

v0.1.x 可以稳定 `PlatformClient`、Core execution DTO 和 Persistence 接口，但不引入 users、workspace、Web Audit 或 HTTP 业务逻辑。Web 的身份和资源隔离留给 v0.2.0 Application Service。

## 9. 非功能需求

### 9.1 可靠性

- Desktop 现有数据目录能够平滑迁移。
- 连接、查询、取消、窗口关闭和应用重启不会破坏本地状态。
- 大结果集不能阻塞 UI。
- 驱动错误、连接断开和超时均有明确 terminal state。

### 9.2 性能

- Schema 首次加载和刷新有可见进度。
- 查询结果使用流式或分页策略，不一次性渲染无限行。
- Query History 写入不阻塞 SQL 主流程。
- AI 上下文构建不重复加载不必要的完整 Schema。

### 9.3 安全

- 连接密码、AI Key 和本地密钥继续使用 AES-256-GCM / OS Keychain 方案。
- 日志和 Query History 不记录敏感凭据。
- 数据修改有明确的连接、数据库和影响范围提示。
- 插件、文件导入和导出继续遵循现有安全边界。

## 10. 版本拆分

### v0.1.0：Desktop 稳定基线

- 连接、Schema、SQL Editor、Result Grid 和 Query History 稳定。
- 完成连接搜索/最近使用/收藏、数据库对象全局搜索和 Table Panel 统一导航/上下文。
- 完成数据编辑 pending changes、SQL Preview、Commit/Rollback 和删除安全门槛。
- 完成快速过滤、过滤后回到第一页、压缩 SQL 上下文和贴近 Execute 的执行状态。
- 完成 Query Error 快捷动作、连接表单 Basic/Advanced/SSH 分层、Context Menu 分层和从 Table 生成 SQL。
- 查询取消、超时、错误展示和 EXPLAIN 闭环可用。
- 主要驱动的核心路径通过契约测试和 E2E。
- AI Chat、MCP、Workflow 保持现有能力可用，不引入新编排语义。
- 完成 Core 执行 DTO 和持久化接口的兼容性整理。

### v0.1.1：质量与体验修复

- 修复 v0.1.0 真实用户路径中的高频问题。
- 改进连接/对象搜索、Table Panel、结果表格、历史搜索和错误诊断。
- 改进大结果集、断线、取消和恢复体验。
- 补齐驱动兼容性和升级迁移问题。

### v0.1.2 及以后：效率增强

- Result Workspace 的基础 Table/Chart View 切换。
- 测试数据复制与安全修改的完整闭环。
- AI SQL 诊断和上下文体验增强。
- Workflow 稳定性和可复用模板。
- 根据数据验证结果决定是否推进图表、Saved Query 或本地企业审计。

## 11. 验收标准

### 11.1 核心用户路径

- 新用户可以完成“创建连接 → 浏览 Schema → 执行查询 → 查看结果”的闭环。
- 连接列表可以通过搜索、最近使用和收藏快速找到目标连接；首次创建连接默认只面对 Basic 字段。
- 用户可以从全局对象搜索直接找到 table、column、view 或 function，并从对象入口打开数据、结构或 SQL。
- 用户可以在同一 Table Panel 完成 Data、Structure 和 DDL 等对象操作；打开 SQL 时仍保留完整的连接、database、schema 和 table 上下文。
- 开发者可以从 SQL 错误进入诊断，并回到编辑器修正 SQL。
- 对支持取消的驱动，用户可以取消正在执行的查询并看到明确的最终状态；不支持取消的驱动显示原因，不伪造 Cancelled。
- 用户可以从 Query History 重新打开近期 SQL。
- Execute 附近始终显示运行状态、取消入口、耗时、影响行数或错误信息。
- 用户可以直接输入常用过滤表达式；过滤生效后自动回到第一页，简单过滤无需额外 Apply。
- 用户可以在 Safe Mode 下完成一次“修改 → 暂存 → 预览 SQL → Commit/Rollback”的数据修改；Delete/Backspace 不会直接执行 DELETE。
- Query Error 提供 Copy Error、Explain、Fix SQL 和 Retry，不需要用户重新打开 AI。
- DataTable Context Menu 一级入口保持精简，高频操作可直接找到，低频操作进入二级菜单。

### 11.2 工程质量

- Core、Host 和驱动测试落点符合项目现有约定。
- 关键 Host UI 路径有 E2E 覆盖；驱动方言测试位于对应驱动 crate。
- 迁移前后 SQLite 数据可读，升级失败可恢复。
- 日志、Query History 和 AI 上下文不泄露凭据。
- 不引入身份、workspace、Web Audit 或企业席位逻辑到 Core。

### 11.3 用户体验指标

- 连接失败时用户能理解失败原因并知道如何处理。
- SQL 执行状态、影响行数、耗时和错误不会被隐藏在不可见日志中。
- 破坏性操作不会因为误点而直接执行。
- 搜索、对象定位、Table View 切换和从 Table 生成 SQL 不要求用户手工记忆内部路径。
- 过滤条件变化不会保留过期页码并造成“无数据”的错觉。
- 用户完成核心查询任务时，不需要了解 Tauri、IPC、Driver 或内部实现细节。

## 12. 关键风险与决策

### 风险：功能范围继续扩张

**决策**：v0.1.x 只围绕开发者核心闭环排期；图表、运营门户和企业治理不抢占 P0 资源。

### 风险：为了 Web 提前污染 Desktop/Core

**决策**：只抽取通用接口和 DTO，不把身份、workspace、Audit 或 Web Session 放进 Core。

### 风险：AI 自动执行带来数据修改事故

**决策**：v0.1.x AI 默认生成建议和编辑器内容，执行仍由用户确认；自动生成完整 Workflow 留给 v0.2.0。

### 风险：本地审计泄露敏感 SQL 和参数

**决策**：v0.1.x 不做合规级 SQL Audit；Query History 最小化存储并默认脱敏。

## 13. 与 v0.2.0 的关系

```text
v0.1.x Desktop 稳定闭环
  → 稳定 Core / Driver / DTO / Persistence 边界
  → v0.2.0 接入 Web Transport 和 Web Application Service
  → 增加 users / workspace / RBAC / SQL Audit / Worker
```

v0.1.x 的完成标准不是“功能最多”，而是：

> **开发者每天使用 DataZen 完成数据库开发、调试和验证时，核心路径稳定、快速、可解释。**
