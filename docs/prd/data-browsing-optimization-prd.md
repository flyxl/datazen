# 数据浏览（Data Browsing）竞品对标与优化方案 PRD

> **文档定位与例外声明**
> 本文件是**用户明确要求入库的一次性方案产物**，不属于 `docs/` 的长期文档纪律范围（[docs/README.md](../README.md) 规定 PRD 类文档不长期维护）。落地后应把结论改写为「已实现事实」并入 [architecture/frontend/components.md](../architecture/frontend/components.md)、[architecture/backend/services.md](../architecture/backend/services.md) 与 [features/](../features/)，随后删除本文件。
>
> **基线**：分支 `feature/data-browsing-prd`（worktree `/Users/flyxl/code/datazen/.worktrees/datazen-data-browsing-prd`），基于 `main` = `e0f6ba28b`（「Merge pull request #57 from flyxl/codex/p5-main-pr」）。分析最初在 detached HEAD `34d862eec` 上进行；已核对 `main..34d862eec` 的 33 个差异文件（672+/377−）**全部属于数据迁移链路**，本文引用的数据浏览相关源码在两者之间逐字一致，故下方代码事实对 `main` 成立。
>
> **对标对象**：**TablePlus**。两套官方文档并存且互相矛盾：`tableplus.com/docs`（现行权威，覆盖 2026 在售版本）与 `docs.tableplus.com`（旧 GitBook，多数页面 3–6 年未更新）。本文件以现行文档为准，旧站仅在它是唯一来源时引用，并标注差异。社区证据来自 GitHub issue、Hacker News 与 Setapp 评论，**与厂商文档严格分开陈述**。
>
> 所有「现状」结论可按《附录 B 符号索引》定位符号核对；所有「对标」结论可按《附录 A 来源》核对。

---

## 1. 结论摘要（TL;DR）

DataZen 的数据浏览在**「读」的骨架上是齐的**（虚拟滚动、服务端筛选/排序/分页、精确行数、类型着色、待提交变更计划 + 指纹、全表流式导出、AI 自然语言筛选），但在**「在网格里干活」的效率维度上整体落后 TablePlus 一个代际**。差距集中在五处：

| # | 结论 | 一句话证据 |
|---|---|---|
| 1 | **没有单元格坐标系** | DataZen 只有整行选择（`TableState.selectedRows`）；TablePlus 自 26.9.6 起支持拖拽矩形区域选择、方向键移动、`Return` 编辑、`⌘+⇧+V` 区域粘贴。 |
| 2 | **写操作只覆盖 UPDATE / DELETE，缺 INSERT** | 计划模型的 `RowChangePlan` 只有 `updates` + `deletes`，driver trait 只有 `build_update_sql` / `build_delete_sql`；TablePlus 有 `+ Row` / `⌘+I` / 双击末尾空白行。 |
| 3 | **剪贴板不互通** | 网格无任何粘贴处理（`DataTable` 内无 `onPaste`，无 `clipboard.readText`）；TablePlus `⌘+C`/`⌘+V`/`⌘+⇧+C`/`⌘+⇧+V` 全套。 |
| 4 | **编辑器是单一字符串输入框** | `EditableCell` 是单行 `<input>` + 类型子串强转；TablePlus 有日期选择器、`DEFAULT`/`NOW()`/`Add a file…` 等 Set Value 菜单（枚举下拉与布尔开关未见于其文档，**不宣称**）。 |
| 5 | **筛选器运算符只有 10 个且不支持逐条 AND/OR** | `FilterOperator` 仅 `eq/ne/gt/lt/gte/lte/like/in/isNull/isNotNull`，全表单一 `filterLogic`；TablePlus 有 `BETWEEN`/`NOT IN`/`Contains`/`Has prefix`/`Has suffix`/`ILIKE`、**Raw SQL**、`Any column`、逐条 AND/OR。 |

另有 4 项「已经做过一半但没接线」的实现缺陷，属于低成本高收益的修补：

- `editBuffer` 是**死属性**：在 `DataTableProps` 声明，但 `DataTable` 的解构参数里没有它，网格内**没有任何「已修改」视觉标记**（TablePlus 会高亮待提交单元格）。
- 网格快捷键只有 `Delete`/`Backspace` 删行与 `⌘/Ctrl+Enter` 提交（`DataTable.handleDeleteKey`、`TableView.handleTableKeyDown`），无 `⌘+S` / `Space` / `⌘+F` / `⌘+I` / `⌘+D`。
- 列宽**不持久化**（`useColumnResize` 无 storage 读写），而 TablePlus 按表持久化宽度**与列顺序**。
- 排序**单列**：`tableDataStore.setSort` 直接写 `sorts: [sort]`，TablePlus 支持右键多列排序。

**方案取向**：不重写网格内核（虚拟滚动、类型色、导出、变更计划都可用且优于多数竞品），而是**在现有 `DataTable` 之上补一层「单元格交互层」**（选择模型 / 键盘 / 剪贴板 / 类型化编辑器），并把 INSERT 沿现有 `RowChangePlan` + driver builder 体系对称补全。P0 共 11 项（DB-01~DB-09，加 DB-14 的写回部分与 DB-18），预计前端 4 条并行轨 + 后端 1 条轨，不引入新的数据通路。

**同时必须指出 DataZen 已有的三处结构性优势**（TablePlus 缺失或其缺陷），不要在对标中把它们一起改掉：

1. **连接级只读模式**：TablePlus 至今没有，社区两次请求（issue #539、#2912）均被关闭且未实现，生产环境安全只靠弹窗与账号权限。
2. **全表流式导出**：TablePlus 导出不流式，官方 issue 记录 2000 万行导出占 8 GB 内存、5100 万行导出吃掉 242 GB 虚拟内存；DataZen 经 `exportCapability` 走流式。
3. **不产生无界 DML**：TablePlus 有过「提交生成无 WHERE 的 UPDATE」的线上事故（issue #3568，仍未闭环）；DataZen 的 `commit_pending_changes_impl` 对每条语句强制 `affected == 1`，非 1 即整批回滚。

---

## 2. 背景与目标

### 2.1 为什么以 TablePlus 为标尺

- 产品定位上 TablePlus 已是明确的追赶对象：连接导入直接支持 `.tableplusconnection`（i18n key `menu.importFromTablePlus`），右键菜单注释写着「对齐 TablePlus」（[components.md](../architecture/frontend/components.md) §9.1），立项博客把 TablePlus/Navicat/DBeaver 列为调研对象（[00-datazen-overview.zh-CN.md](../blogs/00-datazen-overview.zh-CN.md)）。
- 数据浏览是数据库客户端的**最高频路径**：用户多数时间在「看一张表 → 找几行 → 改几个值 → 拿走数据」，而不是在写 DDL。这一环的体验差距直接决定留存，且**无法用 AI 功能弥补**——社区对 TablePlus 的最高评价恰恰落在这一点上：「TablePlus is so good at making table editing feel as fast as excel」「I absolutely love them for the fast editing as well as quick and dirty filters. **Beats typing SQL for simple stuff.**」（Hacker News）。
- 更关键的是：**竞品正在这个领域失分**。2026 年的社区讨论里出现「我在为 TablePlus 写替代品，因为最近几个版本 UI 质量下降」「这些工具老了，不再满足需求」这类表述。这既是窗口期，也意味着**照抄 TablePlus 的现有实现是错的**——应抄其交互范式，避其架构负债（见 §4.12）。
- 项目自身目标里已写明「查看表数据」「编辑行数据」「高性能，能流畅显示大数据集」，本 PRD 是把这三条从「有没有」推进到「好不好用」。

### 2.2 目标用户与关键场景

| 角色 | 场景 | 现状痛点 |
|---|---|---|
| 后端/全栈工程师 | 线上排查一条记录：按 ID 找到行 → 顺着外键看关联订单 → 改一个状态字段 → 提交 | 无外键跳转；无 `⌘+S`；无待提交高亮 |
| DBA / 运维 | 大表抽样核对：10 万行表按 `status IN (...)` + 时间区间筛，看几列，导出筛选结果 | 无 `IN` 多值 UI、无 `BETWEEN`、无「按当前筛选导出」直达 |
| 数据/分析 | 从查询结果里挑几列几行贴进文档/表格 | 无区域选择、无单元格复制、无 HTML/Markdown 复制格式 |
| 测试/QA | 造测试数据：复制一行改几个字段、插入几行 | 无「复制行」、无「新增行」 |
| 新用户（从 TablePlus 迁移） | 肌肉记忆：`⌘+F` 筛选、`Space` 详情、`⌘+I` 插行、`⌘+S` 提交 | 全部无效，判定为「残废」 |

### 2.3 成功指标（可度量）

| 指标 | 现状 | 目标（P0 交付后） |
|---|---|---|
| 核心网格快捷键覆盖率（§4.10 清单） | 2 / 14 | ≥ 12 / 14 |
| 「找一条记录并改一个值」完成步数 | ≥ 8 步（打开抽屉 → 定位字段 → 编辑 → 点提交按钮） | ≤ 4 步 |
| 「复制 N 行 M 列到外部表格」是否保持二维结构 | 否（只能整行 TSV） | 是（区域选择 + `⌘+⇧+C`，粘回 Excel 行列对齐） |
| 大表首屏 P95（超过估算阈值时） | 受精确 `COUNT(*)` 约束 | ≤ 800 ms（估算模式，且必须带 `≈` 标识） |
| 20 列 × 200 行区域的「同一值填充」操作数 | N 次（逐格） | 1 次（区域粘贴） |
| 从 TablePlus 迁移用户的「数据浏览」功能缺失反馈 | 未统计 | 对标矩阵 P0 项 100% 关闭 |

### 2.4 范围与非目标

**范围内**：表格数据标签页（`TableView`）与查询结果标签页（`ResultTableView`）的网格交互；筛选/排序/分页；行变更（增删改）与提交；行/列/单元格的复制与导出；特殊类型展示；网格快捷键；**网格无障碍（ARIA 表格语义 + 受管焦点，对应 C-66 / DB-01 第 6–7 条）**。⚠ 最后这一项虽然**没有 TablePlus 的一手对标证据**，但属于合规底线，不因缺证据而出范围。

**非目标（本期不做，写明避免发散）**：

- 不做无主键表的 UPDATE/DELETE 定位（维持「必须有主键」的强一致立场）。改为**明确禁用 + 说明原因 + 给出 SQL 直达**，而不是假装可编辑——这一点 DataZen 现状已优于 TablePlus（后者在无主键表上直接硬失败，见 §4.12）。
- 不做跨库 JOIN 浏览（TablePlus 亦无；DataZen 的答案应是 Query Builder / Workflow）。
- 不引入新的渲染内核（保留 `@tanstack/react-virtual`）。
- 不为网格编辑引入前端乐观并发之外的分布式事务语义；沿用现有「短事务 / 手动事务」两态。
- 不做 TablePlus 的 **Metrics Board 图表看板**（DataZen 已有 Dashboard/Chart 模块，另行对齐）。
- 不做 mock/假数据生成（TablePlus 亦无原生能力，只有第三方插件）。

---

## 3. DataZen 数据浏览现状（代码事实基线）

### 3.1 组件与数据通路

```
TableView (src/windows/connection/TableView.tsx)
  ├── 工具条：刷新 / 人工筛选面板 / NlFilterInput（AI）/ quick filter 表达式
  ├── 事务条：短事务 ⇄ 手动事务（begin/commit/rollback）
  ├── 待提交变更条：Preview / Commit / Rollback + 计数
  └── DataTable (src/components/DataTable/DataTable.tsx)
        ├── FilterEditor │ FilterBar
        ├── 选择工具条（全选 / 已选 N / 删除行 / 导出 / 列可见性）
        ├── TableHeader（排序图标 + 列筛选入口 + 列宽拖拽）
        ├── VirtualBody → CellRenderer → EditableCell
        ├── Pagination（25/50/100/200/500）
        └── DataExportDialog
```

| 层 | 文件 | 关键符号 |
|---|---|---|
| 状态 | [tableDataStore.ts](../../src/stores/tableDataStore.ts) + [stores/tableData/](../../src/stores/tableData/) | `TableState`、`loadTableData`、`stageCellChange`、`setSort`、`commitPendingChanges` |
| IPC 封装 | `src/commands/database.ts` | `getTableData`（传 `dbSessionId/table/page/pageSize/filters/sorts/filterLogic/skipCount/database/schema`） |
| 命令 | [commands/schema.rs](../../src-tauri/src/commands/schema.rs) | `get_table_data_impl`（取 schema 缓存、算 `effective_skip_count`） |
| 服务 | [services/query_executor.rs](../../src-tauri/src/services/query_executor.rs) | `QueryExecutor::get_table_data`、`build_select_sql`、`build_count_sql`、`format_condition`、`filter_join` |
| 驱动契约 | [packages/driver-api/src/traits.rs](../../packages/driver-api/src/traits.rs) | `skip_count_query`、`supports_offset`、`pagination_syntax`、`format_sql_literal`、`build_update_sql`、`build_delete_sql` |

### 3.2 SQL 生成与行数统计（现状）

```sql
-- 数据页（build_select_sql）
SELECT <cols> FROM <table>
  WHERE <cond> AND|OR <cond> …      -- 由 filter_join 决定单一连接符
  ORDER BY <单列> [DESC]            -- 无排序时默认 ORDER BY <主键列…> ASC，无主键则取第 1 列
  <driver pagination clause>        -- PG: LIMIT n OFFSET m；T-SQL: OFFSET … FETCH NEXT
-- 行数（build_count_sql）
SELECT COUNT(*) FROM <table> WHERE <同条件>
```

- 两条语句通过 `tokio::try_join!` 并发，网络往返已优化；**但精确 `COUNT(*)` 的成本无法隐藏**。
- `skip_count` 只在 `setSort` 时置位，其余加载一律跑 COUNT。
- 无键集（keyset）分页：深翻页一律 `OFFSET`（`supports_offset` 已是 trait 能力位，但只用于语法选择，不做策略切换）。
- **默认注入 `ORDER BY <主键>` 这一条与 TablePlus 同源**，是竞品最深的架构负债（§4.12），DataZen 必须借 DB-18/DB-09 主动拆掉，而不是继承。

### 3.3 现状能力清单

| 域 | 已具备 | 关键符号 |
|---|---|---|
| 虚拟滚动 | `@tanstack/react-virtual`，`overscan=10`，固定行高 28/32 | `useVirtualTable`、`VirtualBody` |
| 行选择 | 复选框、单击、`Cmd/Ctrl+单击`多选、`Shift+单击`区间、全选（含 indeterminate） | `TableState.selectedRows`、`selectRow`、`toggleSelectAll` |
| 排序 | 单列三态（升/降/取消），服务端 `ORDER BY`；排序变化跳过 COUNT | `tableDataStore.setSort` |
| 筛选（UI） | 列 + 运算符 + 值；全局 AND/OR 单选；草稿态 + Apply；条件 chip 折叠 | `FilterEditor`、`filterDraftEqualsApplied`、`isCompleteFilter` |
| 筛选（表达式） | 快速筛选输入框支持 `col op value`、`AND`/`OR`、括号 | `filterExpression` 的 `FilterExpressionOperator`、`parseFilterForApply` |
| 筛选（AI） | 自然语言 → `FilterCondition[]`；🔒 DataZen 独有 | `NlFilterInput`、`ai_parse_filter_impl` |
| 单元格编辑 | 双击进入；单行 `<input>`；按类型子串强转；`Enter` 提交 / `Esc` 取消 / 失焦提交 | `EditableCell`、`toEditString`、`coerceAndCommit` |
| 变更模型 | `pendingChanges` 按主键身份聚合；UPDATE / DELETE / 置 NULL；批量同值（抽屉多选）；SQL 预览 + 计划指纹 + 警告 | `RowChangePlan`、`PendingRowChange`、`applyColumnToRows` |
| 提交一致性 | 每语句要求 `affected == 1`，否则拒绝并整体回滚；支持加入手动事务 | `commit_pending_changes_impl` |
| 删除 | `Delete`/`Backspace` 或右键删行（需主键）；`confirmOnDelete` 开关 | `DataTable.handleDeleteKey`、`stageRowDelete` |
| 详情抽屉 | 单击行打开；字段搜索；长文本 textarea；`QuickLookDialog`（JSON 美化 / BLOB 字节数），触发方式：中键或悬浮按钮 | `DetailPanel`、`FieldRow`、`QuickLookDialog` |
| 列可见性 | 列选择器 + 搜索 + 全选/反选/重置 | `TableColumnFilter` |
| 列宽 | 拖拽调整 + 按数据自动估算；**不持久化** | `useColumnResize`、`adjustWidthsForSort` |
| 右键菜单 | Copy / Copy Row / Copy as JSON / SQL INSERT / SQL UPDATE / CSV / Copy Column Name / Copy Column Data / Set NULL / Filter by This Value / Copy Selected Rows / Delete Row / Export | `buildDataTableContextMenuItems` |
| 导出 | 当前页 / 选中 / 全表；CSV、TSV、JSON、Markdown、XLSX、SQL INSERT；全表走流式 | `ARRAY_EXPORT_FORMAT_OPTIONS`、`resolveExportScope` |
| 单元格渲染 | NULL 斜体、数值右对齐、时间格式化、JSON 截断 120 字 + `title` 提示；类型色 token | `CellRenderer`、`classifyDataType` |
| 键盘（网格域） | 仅 `Delete`/`Backspace` 删行；`⌘/Ctrl+Enter` 提交变更 | 同上 |
| 查询结果网格 | 多结果集切换；行/列/耗时状态条；截断提示；双击进编辑但**提交即丢弃**；Safe Mode 下禁止编辑 | `ResultTableView` |
| 对象检索 | 模糊搜索库/Schema/表/视图/函数/存储过程/查询（**对象名，不含数据**） | `GlobalObjectSearch`、`searchSchemaObjects` |
| 外键元数据 | `TableSchema.foreignKeys`（含被引用表/列、onUpdate/onDelete）已可得；`ForeignKeysView` 与 ER 图消费 | `ForeignKeyInfo` |
| 安全 | Safe Mode 拦无 `WHERE` 的 UPDATE/DELETE、TRUNCATE/DROP；连接级只读；驱动 `readOnly` 元数据 | `sql_guard`、`TableView` 的 `isEditable` 判定链 |

### 3.4 已确认的结构性缺陷

| ID | 缺陷 | 证据 | 影响 |
|---|---|---|---|
| D-1 | 网格无「已修改」标记 | `DataTableProps.editBuffer` 声明后未被解构；`stageCellChange` 直接改写 `rows`，视觉上与已提交值无差别 | 用户无法分辨哪些值尚未落库，误以为已生效 |
| D-2 | 无 INSERT 通路 | `RowChangePlan` 只有 `updates`/`deletes`；driver trait 无 `build_insert_sql` | 「造数据」场景必须切到 SQL 编辑器 |
| D-3 | 无剪贴板入口 | `DataTable` 无 `onPaste`；`TableView` 无 paste 绑定 | 外部批量数据无法进入网格 |
| D-4 | 编辑器非类型感知 | `EditableCell.coerceAndCommit` 仅强转子串；布尔要手打 `true`，日期无选择器，枚举无下拉，JSON 需整行文本 | 输入错误率高，错误推迟到提交时报 |
| D-5 | 无外键跳转 | `DataTable` / `TableView` 内无外键引用；FK 信息只在 `ForeignKeysView` / ER 图 | 关联排查要手抄 ID 再开新标签页 |
| D-6 | 行数统计策略单一 | `get_table_data_impl` 的 `effective_skip_count` 只有「精确」与「完全不数」两态，无估算 | 亿级表首屏被 COUNT 拖住 |
| D-7 | 筛选表达能力不足 | 运算符 10 个、全表单一逻辑、表达式解析器禁止混用 AND/OR（`filter.mixedLogic`）、`<>`/`==` 直接报错 | 复杂筛选被迫跳去写 SQL |
| D-8 | 排序单列 | `setSort` 写 `sorts: [sort]` | 多级排序（如 `status, created_at`）不可达 |
| D-9 | 列宽不持久化 | `useColumnResize` 无 storage 读 | 每次重开重调 |
| D-10 | 查询结果不可提交 | `ResultTableView.handleCellDoubleClick` 的编辑只关闭编辑器 | 与 TablePlus「结果可直接改并 `⌘+S`」形成体验落差 |
| D-11 | 特殊类型弱 | BLOB 只显示字节数；JSON 只有美化文本；无 hex/图片/数组/几何 | 二进制与大 JSON 无法阅读 |
| D-12 | 网格快捷键缺位 | 网格域仅 2 个绑定 | 迁移用户肌肉记忆全部失效 |
| D-13 | `EditResultCell` 只改本地视图 | 查询结果单元格编辑写入 `panelStore.updateResultCell`，与数据库无关联且无任何「不会落库」提示 | 用户可能误以为改动已生效（比 TablePlus 的「不可编辑」更危险） |

---

## 4. TablePlus 数据浏览能力（对标事实）

> 标记约定：**［文档］** 现行官方文档明载；**［变更］** 仅在 changelog 中记载；**［社区］** 用户 issue/评论，未经官方确认；**［存疑］** 任何来源都查不到，**不等于不存在**。

### 4.1 网格与选择

- **［文档］单元格区域选择**：点击单元格并拖拽形成矩形区域；方向键在单元格间移动；`Return` 编辑选中单元格；区域可 `⌘+⇧+C` 复制、`⌘+⇧+V` 粘贴。**［变更］** 该能力始于 26.9.6（2026-08-12）——**很新**，说明这是竞品正在补的短板，不是它长期领先的强项。
- **［文档］分页而非无限滚动**：默认 **300 行/页**；箭头或 `⌘+←`/`⌘+→` 翻页；底部配置按钮可改 `Limit`、手填 `Offset`、`Go`。
- **［文档］行数**：大表显示估算值（带 `~`），点击即改为 `Count` 精确值；阈值可配（「表行数超过 n 才估算」）。
- **［文档］查询结果改为流式**：结果边加载边显示，加载是后台任务且有 `Cancel`；分页型驱动（如 BigQuery）显示 `Load more`。
- **［文档］交替行背景**（默认开）、滚动条自动隐藏、行号可切换。
- **［存疑］虚拟化**：官方从未记载。唯一的性能声明是 **［变更］** 「Improved data grid performance (10x faster with large data sets)」（6.7.8），无方法论，按营销表述看待。

### 4.2 筛选

单条筛选 = 列 + 运算符 + 值，**［文档］**「TablePlus builds a SELECT query with a WHERE condition from your filters」——即服务端筛选：

| 能力 | 细节 |
|---|---|
| 列选择 | 具体列 / **Any column**（全列搜索） / **Raw SQL**（手写条件，如 `total > 100 AND status = 'paid'`） |
| 运算符 | `=`,`<>`,`<`,`>`,`<=`,`>=`；`IN`,`NOT IN`（值列表）；`IS NULL`,`IS NOT NULL`；`BETWEEN`,`NOT BETWEEN`；`LIKE`；`Contains`,`Not contains`；`Has prefix`,`Has suffix`；PostgreSQL 另有 `ILIKE` 与大小写不敏感的 Contains 家族。**无正则**［存疑］ |
| 多条件 | 逐条勾选 → **Apply All**（`⌘+Return`），默认 `AND`；下拉可切 **Apply All Checked Filters with OR**。**无嵌套/分组布尔 UI**，逃生口是 `Raw SQL` |
| 逐条操作 | Apply/Applied 状态、复制筛选 `⌘+I`、删除筛选 `⌘+⇧+I`、`Clear`、`SQL` 悬层预览生成语句、`Export` 导出筛选结果 |
| 快捷筛选 | 列头右键 `Filter with column`；单元格右键 `Quick Filter`（按值 / Contains / Has prefix / IN / IS NULL） |
| 外键筛选 | 单元格内 FK 箭头点击 → 打开**被引用表并按其被引用行过滤** |
| 个性化 | 默认筛选列（PK / Any column / Raw SQL）、默认运算符（`=` / Contains）、筛选栏状态（**按表记忆上次状态** / 常显 / 常隐）、列名排序（按名称 / 按序号）、**默认表排序**（不排序 / PK 升 / PK 降） |
| 列筛选 | token 输入（逗号分隔或从 `Add a column` 选）；列头右键 `Hide this column` / `Show all columns` |
| 客户端筛选 | **只存在于查询结果**：结果栏 `⌘+F` 过滤**已加载行**，含 Match Case 与查找替换 |
| 全库值搜索 | `Tools → Search in Database`：可指定匹配方式与范围，遍历全库所有表所有列，结果按 Schema 分组给命中行数，可 Pause/Resume；官方自陈「runs a query on every table, which can take a while and adds load to the server」 |

**［社区］已知问题**：试用版高级筛选项上限 **2 条**（免费 Windows 版完全无筛选），且有用户报告该弹窗会卡死整个应用（issue #2085）。**没有试用版行数上限**——这是一个常见误解，不应写进对标结论。2025–26 出现筛选输入延迟回归（「每次按键 1 秒」，issue #3550/#3569，控制台指向 Swift `ConnectionModel` 的 Hashable 缺陷）。

### 4.3 排序

- **［文档］** 单击列头：降序 → 升序 → 取消；列头右键 Ascending/Descending/Remove。
- **［文档］多列排序存在**：逐列右键 `Sort multiple Columns > Ascending/Descending`。
- **［社区］但实现脆弱且从不持久化**：加第二列时前一列排序丢失（issue #2843，仍开、已标 `bug`）；「记住我的排序」有 **5 个独立未闭环请求**（#2913/#2919/#3154/#3812、Windows #708）；刷新或加字段后排序丢失；`Do not sort` 会生成非法 SQL（#3813）；SQLite 整数按字符串排序（#3820）；无 `NULLS FIRST/LAST`（#3629）。
- **→ 结论**：多列排序是 TablePlus 的**名义能力、实际负债**。DataZen 做 DB-11 时应同时做到「顺序正确 + 随表持久化」，这才是真正的超越点。

### 4.4 编辑与提交（Code Review & Safemode 模型）

- **［文档］暂存是架构核心**：「TablePlus does not send the queries that modify the database automatically to the server unless you confirm the changes.」`⌘+S` 提交 · `⌘+⇧+P` 预览生成 SQL · `⌘+⇧+Delete` 丢弃 · `⌘+Z`/`⌘+⇧+Z` 撤销重做。
- **［文档］插入行**：`+ Row` / `⌘+I` / **双击最后一行下方的空白区**。
- **［文档］复制行**：`⌘+D`，或 `⌘+C` + `⌘+V`；多行可一起复制。
- **［文档］编辑**：双击就地编辑；`⌥+Return` 或右键 `Set Value` → `EMPTY` / `NULL` / `DEFAULT` / `NOW()` / `CURRENT_TIMESTAMP` / 日期选择器 / `Add a file...`（二进制列）；JSON 有 PRETTY / MINIFY。
- **［文档］多行批量**：选中多行后在右侧栏（`Space` 切换）改一个字段即写入所有选中行。
- **［文档］查询结果同样可编辑并提交**；**［变更］** 自 6.4.2 / Build 600（2025-03-21）起 JOIN 结果也可编辑。
- **［文档/社区］Safe Mode 五档**：Silent · Alert 1 · Alert 2 · Safe 1（需数据库密码）· Safe 2；可用系统密码/TouchID 解锁。**注意：它不是「WHERE 子句门禁」**——与 DataZen 的 `sql_guard` 不是同一层机制，两者不可互相替代。
- **［社区］提交是最大的失败面**：无 WHERE 的 UPDATE（#3568，30 条评论，厂商补丁未成功）；提交后连接卡死（#3724）；提交后编辑上下文丢失（#2979）；含糊的「All changes were reverted」（#718/#3021）；**批量编辑约 50 行会卡死数分钟且「已存在多年」**（#3239）；`bytea` 主键导致更新失败（#715/#1983/#3586）；Safe Mode 漏报警告（#3257）；2019 年就请求的「回车即提交」仍未实现（#967）。
- **［存疑］填充下拖（fill-down）**：官方未记载；**无「Copy as UPDATE」**，剪贴板 SQL 只有 INSERT。
- **［存疑］无主键表的行编辑**：官方未记载；**［社区］** 无主键表在打开阶段就可能直接失败（见 §4.12）。

### 4.5 值处理

- **［文档］Quick Look**：右键 `Quick Look Editor` / 选中单元格 **`⌘+Return`** / **中键单击**；弹层可看可改长文本、JSON（美化）、二进制；`Esc` 关闭；可 `Open in Window` 脱离。
- **［文档］`Space` 是右侧栏（行详情表单）**，**不是** Quick Look——这是最常被记错的快捷键。
- **［文档］右侧栏**：一字段一行、字段名模糊/包含搜索、多行批量编辑、Auto Pretty JSON 开关。

### 4.6 关系导航

- **［文档］Follow a foreign key**：FK 列单元格显示箭头，点击即打开被引用表并过滤到被引用行；`⌥+click` 亦可；支持复合/多 FK。
- **［文档］前进后退**：`⌘+⌃+←` / `⌘+⌃+→`。
- **［社区］这是最受喜爱也最易碎的特性**：10+ 个 issue。设计层抱怨「不清楚该单击还是双击那个箭头」（#516，2018）；跨库崩溃（#1648）；Oracle 未修复（#2509）；复合 FK（#1931）。**最大的空缺是「单元格内 FK 选择器」**：2020 年提出（#2142，「PHPMyAdmin offers a dropdown… HeidiSQL offers this feature… 这会减少外键不匹配错误」），2026 年重提（#3782）却被**关闭为 duplicate**。另有「FK 点击落到 raw SQL 筛选而非结构化选择器」（#3397）、「右侧栏没有 FK 箭头」（#1678）。
- **［存疑］反查引用行（referenced by）**：官方未记载。**［社区］** 反复出现的品类级抱怨是「我不想看到一个外键 ID，我想看到关联行的一个样本」（HN）。

### 4.7 复制与导出

- **［文档］** `Copy Rows`（`⌘+C`，默认 CSV）、`Copy Rows As`、`Copy Selected Cells`（`⌘+⇧+C`）、`Copy Selected Cells As`、`Copy All Column Values`；**［变更］** `Copy Cell Value` 始于 7.0.0（2026-05-18）。
- **［文档］`Copy … As` 格式**：Plain Text / JSON / **HTML** / **Markdown Table** / CSV / **CSV with Header** / **INSERT Statement**（尊重所选列）。
- **［文档］表格右键另有**：Refresh、Paste、Quick Filter、Import、**Export current page…**。
- **［文档］结果集/整表导出**：CSV / JSON / SQL / **Excel**；结果栏 `Export…` 可选字段，多结果集可 `Export Results from All Tabs`；Message 视图导文本，Chart 视图导 **PNG**。整表导出 `⌘+⇧+E`，跨 Schema 多选；CSV 可配分隔符/引号/换行/小数分隔符/**Convert NULL to EMPTY**/表头；SQL 可含结构 + drop + 数据 + Gzip。有后台导出与通知（可 follow/stop/Open Folder）。
- **［文档］导入**：CSV 向导预览前 100 行 + 列映射 + 匹配策略（名称+顺序 / 名称 / 顺序）+ 插入选项（STOP ON ERROR / UPDATE ON DUPLICATE / REPLACE）；可**新建表**；JSON 与 SQL dump 向导同理。
- **［社区］导出不流式，是竞品最严重的问题之一**：issue #3448 仍开（「约 2000 万行，内存涨到 **8 GB**，能不能加流式」）；#3786 是 v6.8.0 回归（「**5100 万行**的表吃到 **242 GB 虚拟内存**，我以前一直这么干，一直没事」）；另有 gzip 截断（#3955）、CSV 非法（#3733）等导出损坏报告。
- **［社区］「导出筛选后的表」仍是未实现请求**（#3663）。

### 4.8 检索

- **［文档］Open Anything**（`⌘+P`）：模糊打开库/Schema/表/视图/物化视图/函数/存储过程/查询。
- **［文档］Search in Database**：见 §4.2 末行。
- **［文档］结果内搜索替换**：`⌘+F` 打开搜索面板，选列或 Any column + Contains / Not contains / Has prefix / Has suffix + Match Case，**过滤已加载行**；可逐条或全部替换，`⌘+S` 提交。
- **［文档］对象内搜索**：结构视图 `⌘+F`；侧栏模糊搜索；Recent；可 Pin。

### 4.9 偏好与性能

- **［文档］Estimate count**（旧站推荐开启，性能更佳）：估算行数 + 阈值。
- **［文档］默认表排序可配**；筛选状态可按表记忆；列宽与列位置按表持久化（存于可同步的 *Others* 目录）。
- **［文档］编辑器行数上限**：`No limit` 或 100…500,000，可**按连接**保存；但默认是 `No limit`，**［社区］** 因此有「我经常一个 `SELECT * FROM ...` 就意外查了百万行表」（#2125）。
- **［文档］查询超时默认 300 s**；keep-alive 30 s；启动时恢复上次工作区。
- **［社区］列级统计/直方图/去重值缺失**：只有表级 Item overview；有请求按选区做 COUNT/COUNT DISTINCT/SUM/AVG（#1394，被标 `new-feature` 关闭）。**无冻结/钉列**（#3752，2026-01 提出）**、无网格内正则搜索**（#2279，2021 起，抱怨「表有 20+ 列时得横向滚很久才能找到列名」）**、无文本换行**（#2497）**、无跳转到第 N 行**。

### 4.10 快捷键（Table Data 域，macOS；Windows/Linux 用 `Ctrl`）

| 快捷键 | 动作 |
|---|---|
| `⌘+F` / `⌥+⌘+F` | 打开行筛选 / 列筛选 |
| `⌘+I` | 插入新行 |
| `⌘+D` | 复制行 |
| `⌘+C` / `⌘+V` | 复制行 / 粘贴行 |
| `⌘+⇧+C` / `⌘+⇧+V` | 复制选中单元格 / 粘贴到选中单元格 |
| `⌘+S` | 提交变更 |
| `⌘+⇧+P` | 预览变更 SQL |
| `⌘+⇧+Delete` | 丢弃变更 |
| `⌘+Z` / `⌘+⇧+Z` | 撤销 / 重做（Workspace 域，是否覆盖数据编辑未明） |
| `Space` | 切换右侧行详情 |
| `⌘+Return` | Quick Look（单元格） |
| `Return` | 编辑选中单元格 |
| `⌥+Return` | Set Value 菜单 |
| `⌥+click` | 快速编辑菜单 |
| `Tab` | 编辑中移动焦点 |
| 方向键 | 单元格间移动 |
| `Delete` | 删除选中行 |
| `⌘+R` | 重载工作区（**不是**执行 SQL） |
| `⌘+P` | Open Anything（**`⌘+K` 是切库，不是快速打开**） |
| `⌘+.` | 进程列表 / 取消 |

### 4.11 TablePlus 相对弱项（DataZen 的既有优势与可直接超越点）

| 维度 | TablePlus | DataZen |
|---|---|---|
| 连接级只读 | **无**（#539、#2912 两次请求均关闭未实现） | ✅ `savedConnection.readOnly` + 驱动 `readOnly` 元数据 |
| 全表导出 | 不流式（2000 万行 → 8 GB；5100 万行 → 242 GB 虚拟内存） | ✅ `exportCapability` + 流式 |
| 提交安全 | 出现过无 WHERE 的 UPDATE（#3568 未闭环） | ✅ 每语句 `affected == 1`，否则整批回滚 |
| 无主键表 | 打开即可能硬失败（`column "id" does not exist`） | ✅ 可浏览，仅禁用定位型写入 |
| 变更审计 | 只有 Code Preview；回滚靠「记录原值」而非撤销 | ✅ 计划指纹 + 警告 + 预览 + 手动事务 |
| 筛选状态 | 按表记忆状态，**无命名持久视图**（7 个未闭环请求） | 本期缺失，DB-08 目标里直接做「命名视图」 |
| 列级统计 | 无（#1394） | 本期缺失，见 DB-12，可直接超越 |
| 列冻结 | 无（#3752） | 本期缺失，见 DB-12，可直接超越 |
| 单元格内 FK 选择器 | 无（#2142 被 duplicate 关闭） | 本期缺失，见 DB-20，可直接超越 |
| AI 自然语言筛选 | 无（其 Assistant 是聊天式，不是筛选器） | ✅ `NlFilterInput` + `ai_parse_filter` |
| 跨驱动一致性 | 体验按库分裂，文档明说「运算符取决于你的数据库」 | ✅ 单一 `driver-api` 契约 + 能力位 |
| 数据迁移能力 | 无 Schema Diff / Data Sync / 异构 Transfer | ✅ 四件套齐备 |

### 4.12 TablePlus 的根因级负债（**不要继承**）

**［社区＋厂商确认］打开一张表执行的是 `SELECT * FROM t ORDER BY <主键…> LIMIT 300 OFFSET 0`，且这从未写进文档。** 这一条默认 `ORDER BY` 单独制造了四个头部问题：

1. **ClickHouse**：`ORDER BY "id","buyer_id",… LIMIT 300 OFFSET 0` → **85.75 s**；去掉 ORDER BY → **375 ms**。「慢查询执行期间整个 TablePlus UI 失去响应。」厂商开发者回应：TablePlus 只在主键列上排序……「去掉排序很容易，但**它会导致分页结果错误**」（issue #3592，18 条评论）。
2. **SingleStore**：`ORDER BY customer_uid, order_uid LIMIT 300 OFFSET 0` >1 分钟，去掉后 <100 ms（#2662，仍开）。
3. **无主键表硬失败**：`column "id" does not exist`。用户原话：「我猜你是为了让行可以就地更新才显式查这个。但有些表就是没有 ID，我们仍然想在里面看数据。」（#651）；`ctid` 还会泄漏进导出（#266）。
4. **提交生成无 WHERE 的 UPDATE**：PG 外部表场景，「预览 SQL 时 WHERE 后面什么都没有」，根因是主键解析失败（把 `schema.table` 传给了只接受 OID 的 `pg_catalog.col_description()`）。**仍开，30 条评论，厂商补丁失败**（#3568）。

**对 DataZen 的直接启示**（必须写进方案，不能只当竞品八卦）：

- DataZen 的 `build_select_sql` **同样在无显式排序时注入 `ORDER BY <主键> ASC`**——这是同一个根因。区别只在 DataZen 没有把主键解析失败静默降级成无界 DML（有 `affected == 1` 兜底）。因此 **DB-09/DB-18 不是「性能优化」，而是拆掉一个已经确认会炸的架构负债**：默认排序必须可配、可关（TablePlus 后来加了 `Default Table Sort: Do not sort` 正是补这个洞），并在关闭排序时**明确提示分页结果不保证稳定**，而不是默默给出可能重复/遗漏的页。
- **绝不允许主键解析失败后继续**：DataZen 现状对无主键表禁用写入是对的，但要确保**浏览路径**（只读分页）不因 PK 缺失而失败或漏页。
- **keyset 分页（DB-18）是唯一能同时解决「深翻页慢」与「默认排序贵」的路径**，优先级应从 P2 上调到与 DB-09 同批评估。

---

## 5. 对标矩阵（差距清单）

判定：**P0** = 数据浏览最低可用线（缺失会被判定为「不能用」）；**P1** = 显著提升效率/可读性；**P2** = 打磨与前瞻。`DS` = DataZen 现状（✅ 有 / 🟡 部分 / ❌ 无）。`TP` 依据来源见 §4 标记。

| ID | 能力（可观察行为） | TP | DS | 级别 | 备注 |
|---|---|---|---|---|---|
| C-01 | 点击并拖拽选择矩形单元格区域 | ✅［文档］ | ❌ | P0 | TablePlus 2026-08 才补上，属「正在补齐的短板」 |
| C-02 | 方向键在单元格间移动，`Home/End/PgUp/PgDn` 跳两端 | ✅ | ❌ | P0 | |
| C-03 | `Return` 进入选中单元格编辑，`Tab` 提交并移动 | ✅ | ❌ | P0 | |
| C-04 | 复制选中单元格（`⌘+⇧+C`），保持二维结构 | ✅ | ❌ | P0 | 只有整行 TSV |
| C-05 | 粘贴到选中单元格区域（`⌘+⇧+V`），自动二维填充 | ✅ | ❌ | P0 | |
| C-06 | 复制行（`⌘+C`）/ 粘贴行（`⌘+V`） | ✅ | ❌ | P0 | |
| C-07 | 复制整列值 | ✅ | ✅ | — | `copyColumnData` |
| C-08 | 复制为 Plain / JSON / HTML / Markdown / CSV / CSV+Header / INSERT | ✅ | 🟡 | P1 | 有 JSON/CSV/Markdown/SQL INSERT，缺 HTML、CSV+Header、Plain |
| C-09 | `+ Row` / `⌘+I` / 双击末尾空白行 新增行 | ✅ | ❌ | P0 | D-2 |
| C-10 | `⌘+D` 复制行 | ✅ | ❌ | P0 | |
| C-11 | `Delete` 删行 | ✅ | ✅ | — | 需主键 |
| C-12 | 待提交单元格高亮标记 | ✅ | ❌ | P0 | D-1，`editBuffer` 未接线 |
| C-13 | `⌘+S` 提交 / `⌘+⇧+P` 预览 / `⌘+⇧+Delete` 丢弃 | ✅ | 🟡 | P0 | 有按钮，无快捷键 |
| C-14 | `⌘+Z`/`⌘+⇧+Z` 撤销重做变更 | ✅［文档］ | 🟡 | P1 | 有全量 `rollbackPendingChanges`，无单步撤销 |
| C-15 | 布尔开关 / 日期选择器 / 枚举下拉 | 🟡［存疑］ | ❌ | P0 | TablePlus 文档只明确日期选择器与 JSON 美化；枚举/布尔未记载，**按「应有」立项而非「对标」** |
| C-16 | Set Value：`EMPTY`/`NULL`/`DEFAULT`/`NOW()`/`CURRENT_TIMESTAMP` | ✅ | 🟡 | P0 | 仅 `Set NULL`；三态区分需 DB-04 的 `CellWrite` |
| C-17 | 从文件加载值到二进制列 | ✅ | ❌ | P2 | |
| C-18 | 多行同字段批量编辑 | ✅ | 🟡 | P0 | 仅抽屉多选路径，网格内不可 |
| C-19 | Quick Look（中键 / `⌘+Return` / 右键） | ✅ | 🟡 | P0 | 仅中键且限抽屉内，无 `⌘+Return`、无网格右键入口 |
| C-20 | JSON 树形查看/编辑（含 PRETTY/MINIFY） | ✅ | 🟡 | P1 | DataZen 的 JSON 树只在 Redis 驱动（`JsonEditor`），未进通用网格 |
| C-21 | BLOB hex / 图片预览 / 存为文件 | ✅ | ❌ | P1 | D-11 |
| C-22 | 外键箭头点击跳转被引用行 | ✅ | ❌ | P0 | D-5 |
| C-23 | 单元格内 FK 选择器（下拉选被引用行） | ❌［社区］ | ❌ | P2 | 两侧都缺；TablePlus 2020 年请求被 duplicate 关闭 → **超越点** |
| C-24 | 反查引用行（referenced by） | ❌［存疑］ | ❌ | P2 | 两侧都缺 → 差异化机会 |
| C-25 | 行筛选：`IN`/`NOT IN` 多值 | ✅ | 🟡 | P0 | 有 `in` 但值需手打逗号 |
| C-26 | `BETWEEN`/`NOT BETWEEN` | ✅ | ❌ | P0 | D-7 |
| C-27 | `Contains`/`Not contains`/`Has prefix`/`Has suffix` | ✅ | 🟡 | P0 | 仅 `like`，需用户手打 `%` |
| C-28 | 大小写不敏感匹配（`ILIKE` 等） | ✅ | ❌ | P0 | |
| C-29 | 正则匹配 | ❌［存疑］ | ❌ | P2 | TablePlus 未记载 → 可作 DataZen 差异点，不作为对标缺口 |
| C-30 | `Any column` 全列搜索 | ✅ | 🟡 | P1 | 有快速筛选但需指定列 |
| C-31 | `Raw SQL` 自由条件 | ✅ | 🟡 | P1（**本期保持排除，记为待排期**） | 表达式受限（8 运算符、禁混用）。**已裁定：本期不做**，安全边界见 §8.4 的条件性条款 |
| C-32 | 多条件逐条 AND/OR | ✅ | ❌ | **P0（已裁定保留在本期范围）** | 全局单一逻辑。证据：`TableView.tsx` 的 `handleQuickFilter` 对混用直接报 `filter.mixedLogic`。实现落在分册 08，**不得**在本期降级（详见 DB-08 与 C-33 的边界） |
| C-33 | 嵌套布尔分组 `(A AND B) OR (C AND D)` | ❌ | ❌ | P2 | 两侧都缺；TablePlus 的逃生口是 Raw SQL |
| C-34 | 生成 SQL 预览 | ✅ | 🟡 | P1 | 变更 SQL 有预览；**筛选** SQL 无预览 |
| C-35 | 按当前筛选一键导出 | ✅ | 🟡 | P1 | 可经导出对话框，但无直达 |
| C-36 | 筛选状态按表记忆 | ✅ | ❌ | P1 | 关标签即丢 |
| C-37 | 命名/持久化网格视图 | ❌［社区］ | ❌ | P1 | TablePlus 7 个未闭环请求 → **差异化机会** |
| C-38 | 默认筛选列/运算符可配 | ✅ | ❌ | P2 | |
| C-39 | 单元格右键快速筛选（按值/前缀/IN/NULL） | ✅ | 🟡 | P1 | 只有 `Filter by This Value`（eq） |
| C-40 | 列头右键筛选 | ✅ | ✅ | — | 表头筛选图标 |
| C-41 | 多列排序（且顺序正确、随表持久化） | 🟡［社区：易丢失、不持久化］ | ❌ | P1 | D-8；**做对即超越** |
| C-42 | 列宽 + 列顺序按表持久化 | ✅ | ❌ | P1 | D-9；TablePlus 连列位置都持久化 |
| C-43 | 列重排 / 冻结 / 钉列 | ❌［社区］ | ❌ | P2 | 两侧都缺（TablePlus 仅结构页可重排）→ 差异化 |
| C-44 | 列级统计（count/distinct/nulls/min/max/avg） | ❌［社区］ | ❌ | P1 | 两侧都缺 → 差异化 |
| C-45 | 行号列（可切换） | ✅ | ❌ | P2 | |
| C-46 | 行数：精确 / 估算 / 不数 三态 + 阈值 | ✅ | 🟡 | P0 | D-6 |
| C-47 | 手填 `Offset` 跳页 | ✅ | ❌ | P2 | 分页器只支持前后翻 |
| C-48 | 深翻页性能（keyset） | ❌ | ❌ | **P0** | 两侧都只有 OFFSET；见 §4.12，这是负债而不仅是优化 |
| C-49 | 查询结果可编辑并提交（含 JOIN 结果） | ✅ | ❌ | P0 | D-10 |
| C-50 | 结果内搜索/替换（客户端） | ✅ | ❌ | P1 | |
| C-51 | 结果导出 Excel | ✅ | ✅ | — | XLSX 已有 |
| C-52 | 多结果集分标签 + 全标签导出 | ✅ | 🟡 | P1 | 有结果集切换，无全标签导出 |
| C-53 | 长查询流式显示 + 后台加载 + 取消 | ✅ | 🟡 | P2 | 有 `query_stream`；表格数据页仍一次性取页 |
| C-54 | 全库值搜索 | ✅ | ❌ | P2 | DataZen 有对象名搜索；数据值搜索可做 AI 增强版 |
| C-55 | 对象模糊快速打开 | ✅ | ✅ | — | `GlobalObjectSearch` |
| C-56 | 交替行背景 / 行高密度 / 滚动条自动隐藏 | ✅ | ❌ | P2 | |
| C-57 | 网格完整快捷键集（§4.10） | ✅ | ❌ | P0 | D-12 |
| C-58 | 无主键表可浏览（不硬失败） | ❌［社区］ | ✅ | — | **DataZen 优势** |
| C-59 | 无主键表写入的明确禁用与解释 | 🟡 | 🟡 | P1 | DataZen 需把「为什么不能改」讲清楚 |
| C-60 | 连接级只读模式 | ❌ | ✅ | — | **DataZen 优势**（#539/#2912） |
| C-61 | 全表导出流式（不爆内存） | ❌ | ✅ | — | **DataZen 优势**（#3448/#3786） |
| C-62 | 提交不产生无界 DML | ❌［社区］ | ✅ | — | **DataZen 优势**（#3568） |
| C-63 | 100 万行级网格流畅度（虚拟滚动） | 🟡［存疑］ | ✅ | — | DataZen 用 `@tanstack/react-virtual`；TablePlus 无虚拟化记载 |
| C-64 | AI 自然语言筛选 | ❌ | ✅ | — | **DataZen 优势，保持并加强** |
| C-65 | 变更计划指纹 + 警告 + 歧义写入拒绝 | ❌ | ✅ | — | **DataZen 优势，保持** |
| C-66 | 无障碍：网格可被读屏软件逐格导航（`role="grid"` 语义 + 受管焦点） | ［无一手证据］ | ❌ | **P0** | **对标缺口两侧都不成立**：TablePlus 侧本次对标**没有采到任何一手证据**（官方文档与 changelog 均未记载读屏支持），因此**不据此判定 TablePlus 缺失或具备**；DataZen 侧是**代码事实**——今天的网格**完全没有 ARIA 角色**，行号槽还是一个裸 `<button>`。这是**可访问性合规底线**，不因「竞品也没证据」而降级 |

**统计**（逐行实测，非估计）：矩阵共 **66 项**（C-01~C-66），无空缺、无重复编号。

- **按级别**：**P0 25 项**（其中 DS ❌ 18 项、🟡 7 项）、**P1 17 项**、**P2 12 项**、已具备或持平 **12 项**。（C-66 的 TP 单元格为「无一手证据」，**不计入**「TP 有而 DS 没有」那一类统计。）
- **按双方强弱**：TablePlus 有而 DataZen 完全没有 **25 项**（TP ✅ / DS ❌）；TablePlus 有而 DataZen 部分具备 **17 项**（TP ✅ / DS 🟡）；TablePlus 侧本身仅存疑或有缺陷、而 DataZen 完全没有 **2 项**（C-15 枚举与布尔编辑器、C-41 多列排序）；双方都不完整 **1 项**（C-59 无主键表写入）；**DataZen 领先 7 项**（TP ❌ 6 项：C-58/C-60/C-61/C-62/C-64/C-65；TP 存疑 1 项：C-63）；**双方皆缺 8 项**（其中 C-48 键集分页已定为 P0、C-29 正则列为 P2，其余 6 项 C-23/C-24/C-33/C-37/C-43/C-44 属差异化机会）；能力持平 **5 项**（C-07/C-11/C-40/C-51/C-55）。

---

## 6. 差距根因分析

| # | 根因 | 表现 | 影响面 |
|---|---|---|---|
| R1 | **交互坐标停留在「行」粒度** | `selectedRows: Set<number>` 是唯一选择态；`editingCell` 是唯一焦点态；没有 `{row, column}` 结构 | C-01~C-06、C-12、C-18、C-57 全部由它派生 |
| R2 | **写路径按「行身份」建模，天然排除「尚不存在的行」** | `RowIdentity` 是主键快照，`PendingRowChange.originalValues` 必需 | C-09、C-10 无法在现模型内表达 |
| R3 | **编辑器只做「字符串 ↔ 值」的宽松转换** | 类型信息只用于子串判断，无列元数据驱动的控件选择；JSON 解析失败时把原文当值提交 | C-15~C-17、D-4、D-11 |
| R4 | **服务端 SQL 构造器是最小公倍数** | 运算符集合、单一 AND/OR、单列 ORDER BY 都由 host 统一生成，driver 只能改语法不能改能力 | C-25~C-33、C-41 |
| R5 | **性能策略只有「精确」与「放弃」** | `skip_count` 是布尔位，无估算、无阈值、无 keyset | C-46、C-47、C-48、D-6 |
| R6 | **元数据已有但未接到网格上** | FK 在 `TableSchema.foreignKeys` 可得、枚举类型在列类型里、`editBuffer` 已算出——但都没进 `DataTable` 的渲染与事件 | C-12、C-22、C-15（枚举） |
| R8 | **网格从来没有可访问性语义** | 行/单元格只是带 `onClick` 的 `<div>`，**没有任何 ARIA role**；行号槽是裸 `<button>`；每行各自 `tabIndex={0}` 导致 Tab 键逐行穿透 | C-66 |
| R7 | **默认排序是隐式注入且不可配** | `build_select_sql` 无显式排序时注入 `ORDER BY <主键> ASC` | §4.12 的四个头部问题同源；C-48 |

**结论**：R1/R2/R3 是纯前端重构，可在不改 SQL 通路的前提下完成约 70% 的差距修补；R4/R5/R7 需要 `driver-api` 能力位扩展；R6 是「接线」成本最低、体感最强的一类，应作为第一批交付。

---

## 7. 优化方案

### 7.1 方案总览

| 编号 | 方案 | 根因 | 级别 | 依赖 | 主要落点 |
|---|---|---|---|---|---|
| DB-01 | 单元格坐标系与区域选择 | R1 | P0 | — | `useGridSelection`、`VirtualBody`、`CellRenderer` |
| DB-02 | 键盘导航与网格快捷键集 | R1 | P0 | DB-01 | `useGridKeyboard`、`DataTable`、`TableView` |
| DB-03 | 剪贴板互通（区域/行复制粘贴） | R1 | P0 | DB-01 | `dataTableClipboard`、`buildDataTableContextMenuItems` |
| DB-04 | 新增行（INSERT） | R2 | P0 | — | `RowChangePlan`、`commit_pending_changes_impl`、`driver-api` |
| DB-05 | 类型感知编辑器与 Set Value | R3 | P0 | DB-04（三态） | `cellEditorKind`、`DataTable/editors/*` |
| DB-06 | 待提交变更高亮（接通 `editBuffer`） | R6 | P0 | — | `CellRenderer`、`VirtualBody` |
| DB-07 | 外键跳转与返回栈 | R6 | P0 | — | `TableView`、`schemaStore`、导航栈 |
| DB-08 | 筛选器能力补齐 + 命名视图 | R4 | P0 | — | `FilterEditor`、`filterExpression`、`query_executor`、`driver-api` |
| DB-09 | 行数统计三态、估算阈值与默认排序可配 | R5/R7 | P0 | — | `driver-api`、`query_executor`、设置项 |
| DB-10 | 复制格式补齐（HTML / Markdown / CSV+Header / Plain） | — | P1 | DB-03 | `buildDataTableContextMenuItems`、`exportData` |
| DB-11 | 多列排序（顺序正确 + 随表持久化） | R4 | P1 | — | `tableDataStore.setSort`、`build_select_sql` |
| DB-12 | 列工具：重排 / 冻结 / 列宽列序持久化 / 列统计 | — | P1 | — | `TableHeader`、`useColumnResize`、`useColumnStats` |
| DB-13 | 特殊类型可视化（JSON 树 / BLOB hex 与图片 / 数组 / 几何） | R3 | P1 | DB-05 | `@datazen/ui` 抽取 + `CellRenderer` |
| DB-14 | 查询结果可编辑 + 结果内搜索替换 | D-10/D-13 | P0/P1 | DB-01/02/05 | `ResultTableView`、`result-workspace/` |
| DB-15 | 筛选 SQL 预览与「按当前筛选导出」 | — | P1 | DB-08 | `FilterEditor`、`DataExportDialog` |
| DB-16 | 无主键表的显式禁用与解释 + SQL 直达 | **R7** | P1 | DB-09 | `TableView`、i18n；**落地分册 09**（默认排序与「无主键」判定都在那里） |
| DB-17 | 网格密度与外观偏好（行高/交替行/自动隐藏滚动条/行号） | — | P2 | — | `settingsStore`、设置页 |
| DB-18 | keyset 分页策略（驱动能力位），同时拆掉默认排序负债 | R5/R7 | **P0** | DB-09 | `driver-api`、`query_executor` |
| DB-19 | 全库值搜索（AI 增强版） | — | P2 | — | 新命令 + 面板 |
| DB-20 | 单元格内 FK 选择器 + 反查引用行 | — | P2 | DB-07 | `schemaStore`、`TableView` |

---

### 7.2 P0 方案详述

#### DB-01 单元格坐标系与区域选择

**问题**：只有 `TableState.selectedRows: Set<number>`，无法表达「第 3 行的 `email` 列」；`VirtualBody` 的行点击回调只传 row index。

**目标行为**

1. 单击单元格 = 设为焦点单元格并清除区域；`Shift+单击` = 从锚点扩展到该单元格；拖拽 = 矩形区域。
2. 行号列为整行选择入口（保留现有复选框与 `Shift+单击` 行区间语义）。
3. 区域选择时，行级批量动作（删除行、导出选中）**自动降级为「涉及的行集合」**，工具条显示 `N 行 × M 列`。
4. 焦点单元格始终可见（键盘移动时自动滚动到可视区）。
5. 区域选择必须是**单一来源**：`selectedRows` 一律由区域派生，避免两套状态打架导致「删除范围超出预期」这类危险缺陷。
6. **网格必须是可被读屏软件导航的表格**（对应 C-66）：网格根 `role="grid"`、行 `role="row"`、单元格 `role="gridcell"`、**行号槽 `role="rowheader"`**；配 `aria-rowcount` / `aria-colcount` / `aria-activedescendant`。⚠ 关键约束：`role="row"` 的**直接子元素只允许** `gridcell` / `rowheader` / `columnheader`，而今天行号槽渲染的是裸 `<button>` —— 屏幕阅读器会**整段丢弃行号槽**。因此行号槽必须重构成外层 `<div role="rowheader">` + 内层 `<button tabIndex={-1}>`（样式类迁到外层）。
7. **这套角色必须原子落地**：只有 `grid` 角色而没有 `row` / `gridcell` / `rowheader` 与受管焦点，会让读屏软件比「完全没有语义」更困惑（会播报一个空表格）。**与 DB-02 的焦点模型同批合入**，并遵守两条纪律：**① 焦点只落在网格容器上**（容器 `tabIndex={0}` 作为唯一 Tab 停靠点，行改 `tabIndex={-1}`）；**② 本方案拥有 ARIA 角色集合的裁定权，其他方案一律不得再新增或删除任何 role**。

**技术落点**

> ⚠ **本节的技术草图已被[设计契约 §4](data-browsing-design/00-contracts.md) 取代，实现时以契约册与对应分册为准。** 以下差异是**实质性**的，不是措辞差异：
>
> 1. **选择模型不是几何区间。** 本节草案用 `ranges: { top; left; bottom; right }[]` 表达选区；契约 §4.1 冻结为 `anchor` / `focus` / `extraRanges: CellRange[]` / `mode`，并明确**区间的唯一用途是渲染高亮，绝不参与写操作的定位**——写路径必须经 `rowIdentityAnchors` 解析行身份（这正是 Press 表把「区域」误当定位依据时最容易删错行的原因）。
> 2. **「`selectedRows` 一律由区域派生」的方向是反的。** 契约 §4.2 规定：`selectedRows` **只由 `row` 模式维护**，进入 `cell` 模式必须**清空**它；行级动作改由**只读**派生函数 `rowsCoveredBySelection(state)` 取得涉及行集合，且**禁止把派生行集写回 `selectedRows`**。契约的方向更安全：把派生值写回可变状态，恰恰就是本节第 5 条想避免的「两套状态打架」。
> 3. 单元格 `data-*` 属性沿用**既有的** `data-dt-row` / `data-dt-col`（不是本节草案写的 `data-cell-row` / `data-cell-col`），完整属性表见契约 §4.4。
> 4. **ARIA 归属在本册（DB-01），不在 DB-02。** 经裁定：`role="grid"` 骨架与行号槽 `rowheader` 改造**全部归 DB-01**；DB-02 只负责**焦点模型**（roving tabindex、受管焦点、键位），并遵守「**不得新增或删除任何 role**」这条纪律。⚠ 反过来更重要：契约 §4.4 有一条硬约束（`role="row"` 的直接子元素只允许 `gridcell` / `rowheader` / `columnheader`），它意味着**行号槽的 DOM 形态是 DB-01 对 DB-02 的前置条件**——两个方案必须**同批合入**，中间态（有 `grid` 但没有完整三元组，或行号槽仍是裸 `<button>`）对比屏软件比今天更糟。
> 5. 本节把选择模型放在 `src/hooks/useGridSelection.ts` 单个文件里；契约 §4.1 要求**两个都必须存在**的文件——`src/stores/tableData/gridSelection.ts`（纯逻辑，无 React，可单测）+ `src/hooks/useGridSelection.ts`（React 接线）。
>
> 本节其余内容（目标行为 1–4、行号列为整行选择入口、测试落点、焦点单元格始终可见）**仍然有效**，契约未推翻它们。

- 新增纯逻辑模块 `src/hooks/useGridSelection.ts`：
  ```ts
  export interface CellRef { rowIndex: number; columnName: string }
  export interface GridSelection {
    anchor: CellRef | null;
    focus: CellRef | null;
    /** 归一化矩形，支持将来扩展多区域而不改调用方 */
    ranges: { top: number; left: number; bottom: number; right: number }[];
  }
  export function normalizeRange(a: CellRef, b: CellRef, columns: string[]): Range
  export function rangeToCells(range: Range, columns: string[]): CellRef[]
  export function deriveSelectedRows(selection: GridSelection): Set<number>
  ```
- `stores/tableData/types.ts` 的 `TableState` 增加 `gridSelection`；**保留** `selectedRows` 但改为派生字段。
- `tableDataStore` 新增 `setGridFocus`、`extendGridSelection`、`selectGridRange`、`clearGridSelection`。
- `VirtualBody` 按 `ranges` 判定每格选中态；单元格 DOM 带 `data-cell-row` / `data-cell-col`（沿用 `resolveDataTableCellFromEvent` 的 `data-*` 反查约定，禁止依赖几何坐标）。

**验收标准**

- 拖拽 `(r2,c1) → (r5,c3)` 后工具条显示 `4 行 × 3 列`；右键出现 `Copy Selected Cells` 且复制结果为 4×3 TSV。
- `Shift+→` 三次后焦点在 `(r, c+3)`，区域含 4 格。
- 现有行选择 E2E（`e2e/specs/table-batch-ops.ts`，TC-TABLE-009~014）保持绿。
- 区域选择后点「删除行」的确认弹窗必须回显行数，且行数与工具条一致。

**测试落点**：`src/hooks/__tests__/useGridSelection.test.ts`（区间归一化/边界/单格/反向拖拽）、`src/components/DataTable/__tests__/DataTable.selection.test.tsx`、`e2e/specs/table-cell-selection.ts`。

**风险**：`DataTable.tsx` 已 628 行，逼近 800 行上限 → 必须同步把选择/键盘/剪贴板抽成 hook，把 `DataTable` 压回 450 行以内。

---

#### DB-02 键盘导航与网格快捷键集

**问题**：网格域只有 2 个绑定（`DataTable.handleDeleteKey`、`TableView.handleTableKeyDown`）；`useKeyboardShortcuts` 虽支持 `scope: 'table'`，但没有任何网格快捷键注册进它。

**目标行为（macOS 用 `⌘`，Windows/Linux 用 `Ctrl`）**

| 键 | 行为 | 前置条件 |
|---|---|---|
| 方向键 / `Home` / `End` / `PgUp` / `PgDn` | 移动焦点单元格；`Shift` 扩展区域；`Cmd/Ctrl+Home/End` 跳首/末 | 有焦点 |
| `Return` | 进入焦点单元格编辑 | 可编辑 |
| `Tab` / `Shift+Tab` | 提交当前编辑并右/左移一格 | 编辑中 |
| `Esc` | 取消编辑（编辑中）／清除区域（非编辑中） | — |
| `Space` | 切换详情抽屉（多选时进入批量编辑提示） | 有选中行 |
| `⌘/Ctrl+F` | 展开人工筛选面板并把焦点放进第一个条件 | — |
| `⌥/Alt+⌘/Ctrl+F` | 打开列可见性面板 | — |
| `⌘/Ctrl+I` | 插入新行（DB-04 就绪前置灰并提示） | 可编辑 |
| `⌘/Ctrl+D` | 复制选中行 | 有选中行 |
| `⌘/Ctrl+C` / `⌘/Ctrl+V` | 复制行 / 粘贴行 | — |
| `⌘/Ctrl+⇧+C` / `⌘/Ctrl+⇧+V` | 复制选中单元格 / 粘贴到选中区域 | 有区域 |
| `⌘/Ctrl+S` | 提交待变更 | 有待变更 |
| `⌘/Ctrl+⇧+P` | 预览变更 SQL | 有待变更 |
| `⌘/Ctrl+⇧+Delete` | 丢弃待变更 | 有待变更 |
| `⌘/Ctrl+Return` | Quick Look 焦点单元格 | 有焦点 |
| `⌘/Ctrl+Z` / `⌘/Ctrl+⇧+Z` | 撤销 / 重做一处待变更 | 有待变更 |
| `Delete` / `Backspace` | 删除选中行（保持现状） | 有主键 |
| `⌘/Ctrl+A` | 有焦点时全选当前页／无焦点时全选行 | — |
| `?` | 打开快捷键帮助面板 | — |

**技术落点**

- 新增 `src/hooks/useGridKeyboard.ts`，接收 `{ selection, editable, hasPending, columns }` 与动作回调，返回 `onKeyDown`；`DataTable` 只做一次绑定，避免快捷键散落在 5 个子组件里。
- 快捷键以数据形式注册进 `useKeyboardShortcuts`（`scope: 'table'`），并新增「快捷键帮助」面板——`ShortcutDef` 已带 `description`，直接复用。
- **输入法安全**：所有编辑态快捷键必须检查 `nativeEvent.isComposing`（现有 quick filter 输入框已这样做），避免中文输入法回车误触发。
- **受管焦点（roving tabindex）**：网格容器是**唯一**的 Tab 停靠点；行容器改 `tabIndex={-1}`，真实焦点由 `aria-activedescendant` 指向 `{rowIndex, columnIndex}` 对应的单元格 id。⚠ 这不是优化而是**修 bug**：今天每个虚拟行都是 `tabIndex={0}`，Tab 键会**逐行穿透**（1000 行 = 按 1000 次 Tab）。⚠ 既有测试里有 6 处 `[tabindex="0"]` 断言，**必须一并迁移**，否则它们会把新行为判成回归。
- **不抢全局键**：`⌘+S` 等仅在网格面板聚焦（`activePanel?.type === 'table' | 'view'`）时生效。

**验收标准**

- 上表 19 行全部可被 E2E 断言。
- 中文输入法下输入 `测试` 后按 `Enter` 不触发提交/插入行。
- 焦点在 SQL 编辑器时网格快捷键不生效（反向亦然）。
- 网格容器之外按一次 `Tab` 即离开整个网格（不逐行穿透）；在容器内用方向键移动时 `aria-activedescendant` 同步更新，且目标单元格被滚动进可视区。
- 读屏软件能逐格播报行号与列名（行号槽作为 `rowheader` 可被播报）。

**测试落点**：`src/hooks/__tests__/useGridKeyboard.test.ts`（连续击键旅程：进入编辑 → Tab → Enter → Esc 的半途状态）、`e2e/specs/table-keyboard.ts`。

---

#### DB-03 剪贴板互通

**问题**：网格完全没有剪贴板读写；现有只有「序列化导出」（右键 Copy as CSV/JSON/SQL），没有「区域复制 → 粘回 Excel 行列对齐」。

**目标行为**

- `Copy Selected Cells`：矩形区域序列化为 TSV（含引号转义），粘进 Excel/Sheets/Numbers 后行列对齐。
- `Copy Selected Cells As`：Plain / TSV / CSV（含表头开关）/ JSON（二维数组）/ HTML Table / Markdown Table。
- `Paste to Selected Cells`：解析剪贴板（TSV/CSV 自动嗅探，支持带引号的含分隔符字段、CRLF、尾部空行），从区域左上角按矩形铺开；**超出区域自动扩展**并轻提示 `已填充 N 格（扩展至 M 行 × K 列）`。
- `Paste rows`（无区域时）：按列名表头解析为新增行计划（与 DB-04 共用 INSERT 管线）；列名无法匹配时退回「按顺序填充」并提示。
- 每格都走 `stageCellChange`，天然进入现有 `pendingChanges` / 预览 / 指纹 / `affected == 1` 校验链路，**不新增写入通路**。
- 类型转换复用 DB-05 的 `coerceValue`：`'true'` → 布尔真，`''` → `NULL`（可空列），非法值**不静默吞掉**——该格标红、待提交栏显示 `N 格待修正`、提交按钮置灰。

**技术落点**

- 新增 `src/lib/dataTableClipboard.ts`（纯函数，便于单测）：
  ```ts
  export function serializeCells(rows: unknown[][], opts: {
    delimiter: '\t' | ','; includeHeader: boolean; header?: string[];
  }): string
  export function parseDelimited(text: string): { rows: string[][]; delimiter: '\t' | ','; hasHeaderLike: boolean }
  export function planPaste(matrix: string[][], columns: ColumnSchema[], anchor: CellRef, region?: Range): PastePlan
  ```
- `DataTable` 在网格容器 `onPaste` 与右键菜单两处入口调用。
- 与既有 `serializeDataTableRowsAsTsv` / `serializeDataTableRowsAsCsv` 合并，**避免两套 TSV 转义实现**。

**验收标准**

- 从 Excel 复制 3×4（含引号内逗号）→ 粘贴入网格 → 12 格值与类型正确，待提交计数 12。
- 反向：区域复制 → 粘回 Excel → 行列完全对齐。
- 只读连接/驱动下菜单项置灰并提示（复用 `tableData.readOnlyEditDisabled`）。
- 整数列粘贴 `abc`：该格标记错误、提交禁用、文案可读。

**测试落点**：`src/lib/__tests__/dataTableClipboard.test.ts`（转义/嗅探/超区/空行/CRLF/引号嵌套）、`e2e/specs/table-clipboard.ts`。

---

#### DB-04 新增行（INSERT）

**问题**：`RowChangePlan` 只有 `updates` / `deletes`；`commit_pending_changes_impl` 只遍历这两者；driver 只有 `build_update_sql` / `build_delete_sql`。

**目标行为**

1. 入口：工具条 `+ 行`、`⌘/Ctrl+I`、**双击最后一行下方的空白区**。
2. 新行以「草稿行」呈现在结果区，**不是**先插库再改：可编辑所有非自增列，自增/有默认值的列显示 `DEFAULT`。
3. 多行草稿、`⌘+D` 复制草稿行、`Delete` 删除草稿行。
4. 提交与 UPDATE/DELETE 同事务、同「预览 + 指纹 + 确认」流程；INSERT 影响行数同样要求为 1。
5. 提交成功后刷新当前页并高亮新行 1.5 秒。
6. 无主键表**允许**插入（INSERT 不需要行身份），而该表仍禁止 UPDATE/DELETE，UI 明确区分。
7. **顺序固定为 inserts → updates → deletes**（先建父行再改子行，避免同批内瞬时外键违约）；需要别的顺序由驱动声明。

**技术落点**

- 前端 `src/lib/tableChanges.ts`：
  ```ts
  export interface PendingInsert {
    draftId: string;                        // 仅 UI 关联用
    values: Record<string, CellWrite>;      // 未填列不出现
  }
  export interface RowChangePlan {
    /* …既有字段… */
    inserts: PlannedStatement[];            // 复用 PlannedStatement，rowIdentity 为空对象
  }
  ```
- 驱动契约 `packages/driver-api`：
  ```rust
  /// Build `INSERT INTO … (cols) VALUES (…)` for a single-row insert.
  fn build_insert_sql(&self, table: &str, columns: &[(String, CellWrite)]) -> String {
      sql_text::build_insert_sql(self, table, columns)
  }
  ```
  默认实现与 `build_update_sql` 同址（`traits/sql_text.rs`）；方言特权由驱动覆盖（MySQL、SQL Server、ClickHouse 的批量语义，MongoDB/Redis 走 `execute_command`）。
- 提交实现 `src-tauri/src/commands/data.rs`：`inserts` 进入同一事务循环；`RowCommitStatementResult.operation` 增加 `"insert"`。
- 前端 store 新增 `addDraftRow` / `updateDraftRow` / `removeDraftRow` / `duplicateDraftRow`；`VirtualBody` 渲染草稿行（`data-testid="draft-row-N"`）。
- 驱动专属测试落在各自 crate：至少 postgres / mysql / sqlite / sqlserver，断言 `NULL` vs `DEFAULT` vs 空串三者的 SQL 差异。

**验收标准**

- 4 个驱动各插入一行含 `NULL`、`DEFAULT`、空串，落库值与预期一致。
- 违反 NOT NULL/唯一约束时整批回滚，错误信息含列名（走现有 `CommandError` 脱敏管线）。
- 自增主键列不需用户填值；提交后新行可见。
- 无主键表：`+ 行` 可用，既有行的编辑/删除仍明确禁用。

**风险**：`DEFAULT` 与「用户显式输入的空串」必须可区分（`Option<Value>` 表达不了三态）→ 引入 `enum CellWrite { Value(Value), Null, Default }`。这是本方案唯一的破坏性契约变更，必须一次改完并同步所有驱动（`PROTOCOL_VERSION` 是否变更按 `driver-api` 判定，若涉及则所有插件同步发版）。

---

#### DB-05 类型感知编辑器与 Set Value

**问题**：`EditableCell` 是单行 `<input>`，类型判断靠子串（`int`/`bool`/`float`/`json`），其余一律字符串；布尔要手打 `true`，日期无选择器，枚举无下拉。

**目标行为**

| 类型族 | 控件 | 细节 |
|---|---|---|
| 布尔 | 三态（`NULL` / `true` / `false`） | 可空列必须能表达 `NULL` |
| 枚举 / 低基数列 | 可搜索下拉 | 值来源：驱动列类型元数据（PG `enum`、MySQL `enum(...)`）→ 缺失时由用户显式点「加载候选值」触发一次 `SELECT DISTINCT … LIMIT 200` |
| 日期 / 时间 / 时间戳 | 日期时间选择器 + 文本双模式 | 保留手输 ISO 能力；带时区列显示时区；`NOW()`/`CURRENT_TIMESTAMP` 作为快捷项 |
| 数值 | 数字输入 | 整数/浮点/定点分派；长度与精度校验前置 |
| JSON / JSONB | 结构编辑器（复用 DB-13 的 JSON 树） | **非法 JSON 阻止提交**（现状是 `JSON.parse` 失败即把原文当值提交，属隐性数据损坏，行为变更需在 CHANGELOG 标注） |
| 二进制 | 只读展示 + `从文件加载…` | 见 DB-13 |
| 长文本 | 弹层多行编辑 | 双击直接进弹层 |
| 其余 | 现有单行输入 | 保持 |

`Set Value` 菜单（右键单元格 / `⌥+Return`）：`EMPTY`（空串）／`NULL`／`DEFAULT`／`NOW()`／`CURRENT_TIMESTAMP`／`从文件加载…`／`清除该格待变更`；菜单项按列可空性与默认值动态置灰。

**技术落点**

- 新建 `src/components/DataTable/editors/`，单文件均 < 200 行：`BooleanCellEditor` / `DateCellEditor` / `EnumCellEditor` / `NumberCellEditor` / `JsonCellEditor` / `TextCellEditor` / `CellEditorHost`（按族分派）/ `setValueMenu`（纯函数）。
- 类型族判定集中到 `src/lib/cellEditorKind.ts`：
  ```ts
  export type CellEditorKind = 'boolean' | 'enum' | 'date' | 'number' | 'json' | 'binary' | 'longText' | 'text';
  export function resolveCellEditorKind(column: ColumnSchema, driverMeta: DatabaseTypeMeta): CellEditorKind
  export function coerceValue(column: ColumnSchema, text: string, kind: CellEditorKind): Value | typeof INVALID
  ```
  **禁止**再散落子串判断（现有 3 处重复实现：`EditableCell.coerceAndCommit`、`DetailPanel.FieldRow`、`DetailPanel.InlineFieldEditor`）。
- **不得**在打开编辑器时自动发查询（大表代价），候选值一律用户触发。
- i18n 只改 `src/locales/en/`（开发期纪律），新运算符与编辑器文案统一放 `en/query.ts`。

**验收标准**

- 布尔列点击即得三态开关，写入值正确。
- 枚举列下拉可搜索；无元数据时显示「加载候选值」按钮，且未点击前不发任何查询。
- 日期列选择器与手输互不破坏；时区列显示时区标识。
- JSON 列非法输入阻止提交并提示。
- `DEFAULT` 与 `EMPTY` 可区分并各自正确落库（依赖 DB-04 的 `CellWrite`）。

**测试落点**：`src/lib/__tests__/cellEditorKind.test.ts`（覆盖 19 个驱动的代表性类型名）、`src/components/DataTable/editors/__tests__/*.test.tsx`；方言特化只放 `packages/drivers/<id>/ui/__tests__/`。

---

#### DB-06 待提交变更高亮（接通 `editBuffer`）

**问题**：`DataTableProps.editBuffer` 声明后从未被解构；`stageCellChange` 直接改写 `rows`，视觉上与已提交值无异。

**目标行为**

- 已改动单元格：左侧 2px 强调色边 + 角标；悬停提示「原值 → 新值」。
- 已删除行：整行删除线 + 降透明，右键可「恢复此行」。
- 已插入草稿行：行首 `+` 徽标。
- 工具条计数与网格高亮**必须一致且即时更新**（历史上出现过「计数只算第一个」这类缺陷形态）。
- 颜色走 `--dt-dirty-*` token（对齐 `dataTypeColors` 的做法），不硬编码。

**技术落点**

- `DataTable` 解构 `editBuffer` 并透传 `VirtualBody` → `CellRenderer`；`CellRenderer` 增加 `dirty` / `originalValue` / `state: 'clean' | 'dirty' | 'deleted' | 'inserted'`。
- `stores/tableData/pendingChanges.ts` 的 `rebuildEditBuffer` 已按身份重建，需补**排序/翻页后仍命中**的保证（现依赖 `rowIdentityAnchors`）。

**验收标准**

- 改 3 格 → 3 格高亮 + 计数 3；排序后仍高亮在原行上；提交后高亮全部消失。
- 删除 2 行 → 删除线 + 计数 2；恢复后高亮消失。
- 暗/亮主题与第三方主题下高亮均可见。

**测试落点**：`src/components/DataTable/__tests__/CellRenderer.dirty.test.tsx`、`src/stores/__tests__/tableDataStore.editBuffer.test.ts`（排序/翻页/跨页保持）、扩展 `e2e/specs/table-edit.ts`。

---

#### DB-07 外键跳转与返回栈

**问题**：网格内无任何外键引用；FK 元数据只在 `ForeignKeysView` 与 ER 图消费。

**目标行为**

1. FK 列单元格显示可点击箭头（有值时）；点击打开**被引用表**新面板并预置筛选：被引用列 `= 当前值`（复合 FK 按全部列 AND 组合）。
2. 目标面板顶部出现面包屑「来自 `orders.customer_id → customers.id`」，可一键返回来源面板与来源行（**恢复原滚动位置与焦点单元格**）。
3. 空值单元格不显示箭头。
4. `⌘/Ctrl+⌃+←/→` 支持前后导航（对齐 TablePlus 的前进后退）。
5. 跳转落地的筛选必须是**结构化可见条件**，不是隐形 raw SQL（TablePlus 此处的实现被社区抱怨过，issue #3397）。

**技术落点**

- FK 解析：`schemaStore` 的 `getTableSchema` 结果已含 `TableSchema.foreignKeys`；为当前表缓存，单列 FK 直接映射，复合 FK 按 `columns`/`referencedColumns` 顺序组合。
- 跳转实现为「打开新面板 + 预置 filters」，复用 `setFilters` 与现有面板创建路径，不新增「关系视图」组件。
- 返回栈放 `src/stores/panelStore.ts` 派生状态，不落盘。

**验收标准**

- 单列 FK、复合 FK、`NULL` 三种情况行为正确。
- 跳转后筛选条显示可见条件；Clear 后恢复全表。
- 返回后滚动位置与焦点单元格与离开时一致。
- 目标表无主键/无权限时给出明确错误而非白屏。

**测试落点**：`src/lib/__tests__/foreignKeyNavigation.test.ts`（FK → 条件映射，含复合/空值）、`e2e/specs/table-fk-navigation.ts`（需含复合 FK 的种子数据）。

---

#### DB-08 筛选器能力补齐与命名视图

**问题**：运算符仅 10 个；全表单一 `filterLogic`，表达式解析器显式拒绝混用（`filter.mixedLogic`）且拒绝 `<>` / `==`；无 `Any column`、无 `Raw SQL`、无筛选 SQL 预览、无按表记忆、无命名视图。

**目标行为**

1. **运算符补齐**（`FilterOperator` 扩展）：`notLike`、`iLike`、`notILike`、`contains`、`notContains`、`startsWith`、`endsWith`（后四个是 `like` 家族的前端语法糖，落到 SQL 时按方言选 `LIKE`/`ILIKE`）、`between`、`notBetween`、`notIn`、`regex`（**仅驱动声明支持时出现**，PG `~`、MySQL `REGEXP`、ClickHouse `match`；TablePlus 无此能力，属差异点而非对标项）。
2. **多值输入**：`in`/`notIn` 用 token 输入（逗号或回车成 token）；`between` 用两个输入框。
3. **逐条 AND/OR（C-32，P0，不得降级）**：从「全局单开关」升级为扁平列表 + 每条件可选连接符，SQL 按出现顺序**加括号**左结合。关键约束：**新连接符必须是可选的**——未设置或非法时回落到全局 `filterLogic`；当所有条件的连接符都相同时，**生成的 SQL 必须与今天逐字节一致**（这是回归判定线，不是「大致相同」）。⚠ 副作用：`filter.and` / `filter.or` 在 10 种语言里都已存在且都是**逐字节相同的 SQL 关键字**，可直接复用；而 `filter.mixedLogic` 将变为死 key，**必须删除**，否则会留下一个永不再触发的翻译项。嵌套分组（C-33）列入 P2，**不默默降级**。
4. **`Any column`**：生成 `col1 ILIKE x OR col2 ILIKE x …`；列数超阈值（如 30 列）时提示代价。
5. **`Raw SQL` 条件：⛔ 本期保持排除（已裁定，记为 P1 待排期）**。实现时（若未来排期）的准入条件是固定的：只允许作为 WHERE 子句片段，必须过 `sql_guard` 的注释剥离与危险语句扫描，拒绝 `;` 与注释逃逸，错误可读——这条与 §8.4 的条件性条款是同一件事的两面。⚠ **排除它不影响 DB-08 的其余交付**，第 3 条的逐条 AND/OR 已经能覆盖绝大多数「手写 SQL 才能表达」的诉求，因此不要把 Raw SQL 当成 DB-08 的前置条件。
6. **筛选 SQL 预览**：面板内 `SQL` 按钮显示即将执行的数据语句与 COUNT 语句（脱敏后）。
7. **按表记忆 + 命名视图**：
   - 按 `connectionId + database + schema + table` 记住上次筛选/排序/列可见性/列顺序；设置项「默认筛选状态」（记住 / 常显 / 常隐）。
   - **命名视图**：把当前（筛选 + 排序 + 可见列 + 列顺序）存为一组具名视图，可切换/重命名/删除/设为默认。这是 TablePlus 七个未闭环请求指向的同一个缺失概念（issue #3276/#2913/#2919/#3154/#3812），**是本次最明确的差异化机会**。
8. **按当前筛选导出**：筛选面板提供 `导出` 直达 `DataExportDialog`，范围默认「当前筛选结果（全表流式）」。

**技术落点**

- `src/types/index.ts` 的 `FilterOperator` / `FilterCondition` 扩展，**并消除 `src/types/settings.ts` 中的重复定义**（现状两处重复是隐患）。
- Rust：`packages/driver-api` 的 `FilterOperator` 同步扩展；`QueryExecutor::format_condition` 补齐映射；**方言差异走 driver trait**：
  ```rust
  /// SQL spelling for a filter operator; `None` = driver does not support it.
  fn filter_operator_sql(&self, op: FilterOperator) -> Option<FilterSqlSpelling>;
  ```
  默认按 ANSI 给出，PG 覆盖 `regex`/`iLike`，MySQL 覆盖 `regex`；不支持的驱动让 UI 隐藏该运算符，**而不是生成跑不通的 SQL**。
- `filterExpression` 解析器补齐新运算符与 `<>`（映射 `ne`），**保留** `==` 的显式拒绝（避免与赋值混淆）；混用 AND/OR 改为按逐条连接符解析。
- i18n：新增 3 个英文 key（嵌套逻辑说明、连接符重置、全局逻辑被禁用时的提示），复用已有的 `filter.and` / `filter.or`；**删除死 key `filter.mixedLogic`**。
- ⚠ 本节的技术草图已被[设计契约 §5](data-browsing-design/00-contracts.md)取代：冻结后的运算符集合（**13 个运算符，19 条记录**）与逐条连接符字段（`FilterCondition.connector?: 'and' | 'or'`）以契约为准；**`count_strategy` 不在本册**，它归分册 09。
- 偏好持久化：`settingsStore` 新增 `tableViewPrefs: Record<tableKey, TableViewPref>` 与 `namedViews`，带容量上限（如 200 表）与 LRU 淘汰。

**验收标准**

- 每个新运算符在 PG / MySQL / SQLite 至少三驱动生成可执行 SQL。
- 不支持的运算符不出现在 UI（由驱动能力位驱动，不是前端硬编码类型判断）。
- `status IN ('a','b') AND created_at BETWEEN …` 的 SQL 预览与执行结果一致。
- 关闭标签再开、重启应用后筛选/排序/可见列/列顺序一致。
- 命名视图可保存/切换/重命名/删除；切换后网格与服务端查询同步。
- 注入用例（`1=1; DROP TABLE x`）被拒绝且错误可读。

**测试落点**：`src-tauri/src/commands/schema/tests.rs`（运算符 SQL 生成矩阵）、`packages/drivers/<id>/tests/`（PG ILIKE/regex、MySQL REGEXP）、`src/lib/__tests__/filterExpression.test.ts`、`e2e/specs/table-filter.ts` 扩展、`e2e/specs/table-saved-views.ts`。

---

#### DB-09 行数统计三态、估算阈值与默认排序可配

**问题**：`get_table_data_impl` 的 `effective_skip_count` 只有「精确」与「完全不数」；同时 `build_select_sql` 隐式注入 `ORDER BY <主键>`，与 TablePlus 同源（§4.12）。

**目标行为**

| 模式 | 行为 | 适用 |
|---|---|---|
| `exact` | 现有 `COUNT(*)` | 表行数未知或 < 阈值 |
| `estimate` | 用驱动提供的估算手段，界面标注 `≈` | 超过阈值（默认 200000 行） |
| `none` | 不数，分页器显示 `第 N 页`（无总页数） | 用户显式选择 |

- 设置项：`行数统计模式`（自动 / 精确 / 估算 / 不统计）+ `估算阈值`。
- 分页器与状态条对估算值显示 `≈ 1.2M 行`，悬停解释「估算值，可能不精确」；提供「精确统计」一次性按钮。
- 估算策略（驱动可覆盖）：PG `pg_class.reltuples`；MySQL `information_schema.TABLES.TABLE_ROWS`；SQL Server `sys.dm_db_partition_stats`；SQLite 直接精确（COUNT 便宜）；无估算手段则回退 `exact`。
- **类型层面禁止估算冒充精确**：`TotalRows = { kind: 'exact' | 'estimate' | 'unknown'; value: number | null }`。
- **默认排序可配**（对齐 TablePlus 后来的 `Default Table Sort: Do not sort` 补丁，但要做对）：
  - 设置项：`默认表排序`（不排序 / 主键升 / 主键降）；
  - 选「不排序」时**必须在分页器旁提示「未指定排序，翻页结果不保证稳定」**，而不是默默给出可能重复/遗漏的页（这正是 TablePlus 厂商开发者自己指出的权衡：去掉排序「可能导致分页错误」）；
  - 主键无法解析时（视图、外部表、无主键表）**不得静默降级**：显式告知「无法确定稳定排序键」并提供「按首列排序」或「自定义排序」两条出路。

**技术落点**

- `packages/driver-api`：
  ```rust
  fn count_strategy(&self) -> CountStrategy { CountStrategy::Exact }   // Exact | Estimate | None
  async fn estimate_row_count(&self, handle, target: SqlTarget<'_>, table: &str) -> Result<Option<i64>, DriverError>;
  ```
  `skip_count_query()` 保留（OLAP/Kiwi/Superset 继续用它表达「不支持 COUNT」），语义标注为 `CountStrategy::None` 的别名，P2 收敛。
- `QueryExecutor::get_table_data` 返回带 kind 的行数结构；`build_select_sql` 的默认排序改为由参数决定（含 `DefaultSort::Unstable` 分支）。
- 前端 `TableState.totalRows` 同步升级；`Pagination` 与 `ContentStatusBar` 渲染 `≈` 与「精确统计」按钮。

**验收标准**

- 500 万行 PG 表首屏 P95 ≤ 800 ms（估算模式）；切「精确」后显示精确值。
- 估算值必须有 `≈` 与 tooltip，不允许与精确值同形渲染。
- 驱动不支持估算时退化为精确且不报错；`count_strategy = None` 的驱动不显示总页数且不出现 NaN/Infinity。
- `默认表排序 = 不排序` 时出现稳定性提示；主键不可解析时出现明确提示且提供两条出路。
- 无主键表**浏览路径不失败**（这是 TablePlus 的硬失败点，必须作为回归用例盯死）。

**测试落点**：`query_executor` 内联 `#[cfg(test)]`（mock driver 三态）、`packages/drivers/postgres/tests/`（reltuples）、`src/components/DataTable/__tests__/Pagination.estimate.test.tsx`、`e2e/specs/table-no-pk.ts`。

---

#### DB-14 查询结果可编辑与结果内搜索替换

**问题**：`ResultTableView` 的编辑「提交即丢弃」；`panelStore.updateResultCell` 只改本地视图且无提示（D-13）。TablePlus 自 6.4.2 起连 JOIN 结果都可编辑并提交。

**目标行为**

1. 结果网格复用 DB-01/02/05 的交互（区域选择、键盘、类型化编辑器）。
2. **可写回性判定**：结果列能唯一映射到单表列（含该表主键）时允许提交，复用 `RowChangePlan` 链路；否则只允许「本地视图编辑」，并在结果栏显示 `本地编辑 · 不会写回数据库` 的持续标识（不是一次性 toast）。
3. 结果内搜索/替换（`⌘+F`）：选列或 Any column + Contains / Not contains / Has prefix / Has suffix + Match Case，**过滤已加载行**；可逐条或全部替换，`⌘+S` 提交。
4. Safe Mode 下的现有行为（禁止编辑查询结果）保留，并给出可操作的提示。

**技术落点**

- `result-workspace/` 抽出「结果可写性判定」纯函数（依据结果列的 `ColumnSchema` 是否带 `isPrimaryKey` 与来源表信息），与 `isMutationExecution` 邻近放置。
- 「本地视图编辑」与「写回数据库」必须是**两个视觉可区分的状态**（不同边框色 + 常驻标识），不能只靠文案。

**验收标准**

- 单表 `SELECT *` 结果可改并 `⌘+S` 落库。
- JOIN/聚合结果编辑后显示常驻「本地编辑」标识，且不产生任何 DML。
- 多结果集下每个结果各自的本地编辑互不串台。
- 结果内替换可逐条/全部，且在 Safe Mode 下不可用。

**测试落点**：`src/windows/connection/result-workspace/__tests__/`、`e2e/specs/query-result-edit.ts`。

---

### 7.3 P1 方案要点

| 编号 | 目标行为要点 | 落点 | 验收要点 |
|---|---|---|---|
| DB-10 复制格式补齐 | `Copy … As` 增加 Plain / HTML Table / Markdown Table / CSV+Header；统一到 `serializeCells` 一套实现 | `buildDataTableContextMenuItems`、`exportData` | 5 种格式粘进 Notion/Excel/GitHub 预览均正确；转义用例覆盖引号/换行/分隔符 |
| DB-11 多列排序 | 列头右键「排序（多列）」→ 升/降/移除；`sorts: SortCondition[]` 为真正有序数组；表头显示序号徽标；**顺序与内容随表持久化**（避开 TablePlus 的 #2843/#2913 系列缺陷） | `tableDataStore.setSort`、`build_select_sql` | 3 列排序 SQL 顺序正确；加第二列不丢第一列；重开应用顺序一致；`NULLS FIRST/LAST` 行为明确 |
| DB-12 列工具 | 列重排（拖拽表头）＋ 冻结前 N 列 ＋ 列宽与列序持久化 ＋ 列统计（`count`/`distinct`/`nulls`/`min`/`max`/`avg`，右键列头触发） | `TableHeader`、`useColumnResize`、`useColumnStats` | 重开应用列宽/顺序一致；统计对 1 亿行表走服务端聚合而非拉全表；对比 TablePlus 的列统计/冻结缺失（#1394/#3752）作为差异化卖点 |
| DB-13 特殊类型可视化 | JSON 树（tree/raw/pretty/minify 四模式）；BLOB hex + 图片预览 + 存为文件；PG 数组折叠展示；几何 WKT 预览；64 KiB 大值哨兵（先量长度再决定是否整包进编辑器） | 抽到 `packages/ui/src/JsonViewer.tsx` / `BinaryViewer.tsx`，宿主与 Redis 驱动共用 | 1 MB JSON 打开 < 300 ms 且不卡 UI；大值不整包进编辑器；Redis 驱动改用共享组件后测试保持绿 |
| DB-15 筛选 SQL 预览与导出直达 | 见 DB-08 第 6、8 条 | 同 DB-08 | SQL 预览与执行语句一致；导出默认范围为当前筛选结果 |
| DB-16 无主键表显式处理 | 无主键时编辑/删除入口置灰 + tooltip「该表无主键，无法安全定位行」+「用 SQL 编辑」直达查询面板（带 `SELECT * FROM t WHERE …` 模板） | `TableView`、i18n | 不再出现「能点进去但提交必失败」的路径；浏览不受影响 |

### 7.4 P2 / 观察项

| 编号 | 内容 | 触发条件 |
|---|---|---|
| DB-17 | 网格密度（紧凑/标准/宽松）、交替行背景、自动隐藏滚动条、行号开关 | 有明确用户反馈后再做；全部为设置项，不改默认 |
| DB-18 | keyset 分页：driver 声明分页策略，有主键且排序键唯一时用 `WHERE pk > :last` 替代 `OFFSET`。**已在 §4.12 上调为与 DB-09 同批评估的 P0** | 与 DB-09 同批立项，不单独等性能投诉 |
| DB-19 | 全库值搜索（对标 Search in Database），AI 增强：自然语言 → 候选表/列 + 值模式，先给「最可能的 3 张表」再全库扫；必须有 pause/resume 与代价提示 | 需要用户授权遍历全库 |
| DB-20 | 单元格内 FK 选择器（下拉选被引用行）+ 反查引用行（referenced by） | DB-07 完成后；TablePlus 两项皆缺（#2142 被 duplicate 关闭）→ 明确超越点 |
| — | 嵌套布尔分组 `(A AND B) OR (C AND D)` | 待 DB-08 的逐条连接符上线后，用埋点判断是否真有需求 |
| — | TablePlus Metrics Board（图表看板 + 参数联动） | DataZen Dashboard 已有类似能力，按 Dashboard 路线图对齐，不重复建设 |
| — | 单步撤销/重做（`⌘+Z` 覆盖数据变更） | 需变更历史栈；先上「丢弃全部」 |
| — | 正则筛选 | TablePlus 无此能力，优先级低于对标项；驱动能力位就绪后再放 UI |

---

## 8. 架构与扩展点设计

### 8.1 driver-api 扩展（保持「零硬编码」）

| 新增/变更 | 签名 | 默认实现 | 需覆盖的驱动 |
|---|---|---|---|
| INSERT 构造 | `fn build_insert_sql(&self, table: &str, columns: &[(String, CellWrite)]) -> String` | `sql_text::build_insert_sql` | MySQL / SQL Server / ClickHouse / NoSQL（走 `execute_command`） |
| 三态单元格写入 | `enum CellWrite { Value(Value), Null, Default }` | 类型定义 | 全部（`PROTOCOL_VERSION` 若变更需同步所有插件） |
| 筛选运算符方言 | `fn filter_operator_sql(&self, op: FilterOperator) -> Option<FilterSqlSpelling>` | ANSI 映射 | PG（`ILIKE`/`~`）、MySQL（`REGEXP`）、SQLite（无 regex → `None`） |
| 行数统计 | `fn count_strategy(&self) -> CountStrategy` + `async fn estimate_row_count(...)` | `Exact` / `None` | PG、MySQL、SQL Server、SQLite；OLAP/Kiwi/Superset 维持 `None` |
| 分页策略 | `fn pagination_strategy(&self) -> PaginationStrategy`（Offset / Keyset / DriverDefined） | `Offset` | DB-18 就绪后按驱动声明 |
| 大值加载 | `async fn load_cell_value(&self, handle, target, table, pk, column) -> Result<CellPayload, DriverError>`（截断 + 长度，语义对齐 Redis 的 `get_key_raw`） | 无 | DB-13 大值场景；至少 PG、MySQL |

约束（沿用现有纪律）：驱动专属实现与测试**必须**落在 `packages/drivers/<id>/`；新能力位一律「能力 + 默认实现」，Host 不得按 `DatabaseType` 分支；前端差异走 `DatabaseTypeMeta`（`src/lib/databaseMeta.ts`），与 Rust 能力位一一对应。

### 8.2 `@datazen/ui` 抽取

| 组件 | 来源 | 消费者 |
|---|---|---|
| `JsonViewer` / `JsonModeBar` | Redis 驱动的 `JsonEditor` + `jsonModes`（已含 tree/raw/pretty/minify 四模式与「非法 JSON 保真」策略） | 宿主通用网格、Redis、MongoDB、Wapp |
| `BinaryViewer` | 新增（hex / 图片 / 存文件） | 宿主通用网格、Redis |
| 大值哨兵 | Redis 的 `redisBigValue`（64 KiB 阈值语义） | 通用网格的大值判定 |
| `DateCellEditor` / `EnumCellEditor` / `TokenInput` | 新增 | 宿主通用网格、驱动专属网格、DataTable 派生视图 |

抽取原则：**纯 React + 零 IPC**（`@datazen/ui` 定位），取数由调用方传入。

### 8.3 前端状态与组件拆分（守住 800 行红线）

```
src/components/DataTable/
├── DataTable.tsx        # 容器：组合 + 布局（目标 ≤ 380 行）
├── GridSurface.tsx      # 滚动容器 + 选择/键盘/剪贴板事件绑定
├── GridCell.tsx         # 单元格（渲染态 + dirty 标记 + 选中态）
├── VirtualBody.tsx      # 虚拟行（保留）
├── TableHeader.tsx      # 表头（+ 多列排序徽标、列重排、列统计入口）
├── editors/             # DB-05
├── Pagination.tsx       # + 估算态 + 稳定性提示
└── __tests__/
src/hooks/
├── useGridSelection.ts  # DB-01（纯逻辑可单测）
├── useGridKeyboard.ts   # DB-02
└── useColumnStats.ts    # DB-12
src/lib/
├── dataTableClipboard.ts  # DB-03（纯函数）
└── cellEditorKind.ts      # DB-05（纯函数）
```

`TableState` 增量字段：`gridSelection`、`draftRows`、`columnOrder`、`frozenColumns`、`totalRows`（带 kind）。

### 8.4 安全与一致性边界（不得放松）

- **粘贴/批量编辑不得绕过现有约束**：全部走 `stageCellChange` → `pendingChanges` → `RowChangePlan`（含指纹与警告）→ `commit_pending_changes_impl`（`affected == 1` 强校验）。这正是 TablePlus 出过无 WHERE UPDATE 的那条路径，DataZen 已筑好的闸不许为了新功能开洞。
- **`Raw SQL` 筛选必须过 `sql_guard`**（**条件性条款**）：注释剥离 + 危险语句扫描 + 仅 WHERE 片段；拒绝多语句。⚠ 本期 **`Raw SQL` 条件保持排除**（裁定见 C-31 与分册 08 的未决问题 Q-9，记为 **P1 待排期**），因此该条款**当前不生效**；一旦排期实现，它是**不可协商的准入条件**，不得因为「只给内部用户用」而放宽。
- **只读连接/驱动**：粘贴、`Set Value`、`+ 行`、删除一律不可用（复用 `TableView` 的 `isEditable` 判定链）。
- **Safe Mode**：结果网格的现有行为同步到 DB-14；「本地视图编辑」与「写回数据库」必须视觉可分。
- **凭据与数据**：新错误文案走现有 `CommandError` 脱敏管线；日志不得打印单元格值（可能含 PII）。

---

## 9. 测试与验收

| 层级 | 覆盖对象 | 命令 |
|---|---|---|
| 纯逻辑单测 | `useGridSelection`、`dataTableClipboard`、`cellEditorKind`、`foreignKeyNavigation`、`filterExpression` 新运算符 | `npx vitest run`（须过 `pnpm typecheck`，测试文件参与类型检查） |
| 组件单测 | 选择旅程、键盘旅程（含 IME 与残缺中间态）、dirty 高亮、草稿行、估算分页器 | `npx vitest run` |
| Host Rust 单测 | 运算符 SQL 生成矩阵、INSERT 计划、提交顺序与回滚、`affected != 1` 拒绝、三态行数 | `cargo test -p datazen --lib` |
| 驱动契约测试 | `build_insert_sql` 三态、`filter_operator_sql` 方言、`estimate_row_count` | `cargo test -p datazen-driver-<id>` |
| 驱动 UI 单测 | 方言特化编辑器/渲染 | `pnpm test:unit:drivers` |
| Host E2E | `table-cell-selection`、`table-keyboard`、`table-clipboard`、`table-insert-row`、`table-fk-navigation`、`table-saved-views`、`table-no-pk`、`query-result-edit`；扩展 `table-filter`、`table-edit`、`table-batch-ops` | `pnpm e2e:minimal`（驱动注入参数变更后必须用 `pnpm tauri:build:webdriver` 构建） |
| 契约矩阵 | 跨库 journeys（插入/编辑/提交/筛选） | `pnpm e2e:contract:matrix` |

**防回归纪律（必守）**：

- 交互逻辑必须写**连续旅程测试**（击键全过程，含残缺中间态），禁止只断言静态合法终态（[interaction-and-testing-principles.md](../development/interaction-and-testing-principles.md)）。
- 选择/粘贴的定位一律走 DOM `data-*` 属性，禁止依赖视口几何坐标。
- 长输出测试命令重定向到系统 temp 再 `tail`，退出码单独打印（[AGENTS.md](../../AGENTS.md)）。
- 门禁重跑首尾各记录一次 HEAD 与工作区 sha；**同一棵工作树不得同时被提交方与验证方使用**（隔离边界与共享引用风险见 [worktree-isolation.md](../development/worktree-isolation.md)）——主检出的 HEAD 在本次分析期间被移动过，任何门禁运行都必须记录首尾 HEAD 才能证明运行期间没被动过。
- **驱动集必须写进结论**：本轨 worktree 以 `--drivers=basic` 铺设（postgres / mysql / sqlite / redis），而 CI 常以 `--drivers=all` 运行。`DB_REGISTRY` 直接合并 `generated.ts` 的 `DRIVER_DB_ENTRIES`，驱动集一变，同一份宿主单测读到的注册表就变——所以「宿主用例全绿」只在本驱动集语义内成立，**不能**据此推断 CI 结果。DB-08 的验收要求 PG/MySQL/SQLite 三驱动，正好落在 basic 集合内；一旦需要 SQL Server / MongoDB / ClickHouse，必须先重铺 `node scripts/resolve-drivers.mjs --codegen-only --drivers=all`。
- 本轨 worktree 内的 `packages/pro-extensions/sql-editor-pro` 克隆自 Pro `main`（`extensionPointsVersion=1.0.0`），与宿主 `1.1.0` 不一致：`packages/extension-points/src/__tests__/security.test.ts` 的版本一致性用例会断言失败，**与本次文档改动无关**，不要记为缺陷。
- 测试输出若可能含凭据或数据值，只摘录结构性结论，不转发原始输出。

---

## 10. 度量与埋点

| 事件 | 属性 | 用途 |
|---|---|---|
| `grid.selection.created` | kind(cell/range/row)、面积 | 判断区域选择是否真被用 |
| `grid.clipboard.paste` | 格数、目标类型族、失败格数 | 粘贴成功率与类型转换失败热点 |
| `grid.row.inserted` | 驱动、列数、含 DEFAULT 列数 | INSERT 采用率 |
| `grid.edit.commit` | 处数、耗时、是否失败、失败原因码 | 提交链路健康度 |
| `grid.count.mode` | exact/estimate/none、决策依据 | 估算策略命中率与阈值调优 |
| `grid.default_sort` | 不排序/主键/自定义、是否触发稳定性提示 | 验证 §4.12 的负债是否真被拆掉 |
| `grid.filter.operator` | 运算符、是否 Raw SQL、是否 Any column | 决定嵌套分组（C-33）是否值得做 |
| `grid.saved_view.used` | 切换次数、视图名哈希 | 命名视图的采用率（差异化卖点验证） |
| `grid.fk.follow` | 跳转次数、是否返回 | 关系导航价值 |
| `grid.shortcut.used` | 键名 | 快捷键可发现性；`?` 帮助面板打开率 |

隐私边界：**不采集单元格值、不采集 SQL 原文**（只采集运算符/类型族/计数）。埋点默认关闭或本地聚合，遵循现有 settings 开关体系。

---

## 11. 里程碑与依赖

| 阶段 | 内容 | 依赖 | 建议并行轨 |
|---|---|---|---|
| M1（接线，最低风险） | DB-06 高亮 → DB-02 快捷键 → DB-16 无主键显式化 → DB-09 估算与默认排序可配 | 无 | 轨 A（前端交互）、轨 B（后端 count/sort 策略） |
| M2（交互内核） | DB-01 选择模型（**含 ARIA 角色三元组与行号槽 `rowheader` 改造**）→ DB-03 剪贴板 → DB-04 新增行 | M1 的 DB-02（**焦点模型必须同批就位**，否则 `role="grid"` 没有受管焦点） | 轨 A（选择+剪贴板）、轨 C（INSERT 全栈） |
| M3（表达力） | DB-08 筛选器 + 命名视图 → DB-11 多列排序 → DB-15 预览/导出直达 | M1 | 轨 D（筛选+驱动方言）、轨 B（SQL 生成） |
| M4（可读性与结果） | DB-05 类型编辑器 → DB-13 特殊类型（含 `@datazen/ui` 抽取）→ DB-14 结果可编辑 → DB-10 复制格式 | M2 | 轨 E（编辑器 + UI 包）、轨 A（结果网格） |
| M5（打磨与超越） | DB-12 列工具、DB-20 FK 选择器与反查、DB-18 keyset、DB-17/19 视数据决定 | M3、M4 | — |

**关键串行点**

1. `CellWrite` 三态契约（DB-04）触及所有驱动，必须一次改完并同步 `PROTOCOL_VERSION`，不能分两批。
2. `DataTable` 拆分必须先于 DB-01/02/03 合并，否则单文件超限。
3. DB-09 与 DB-18 应合并评估：只做估算而不拆默认排序，等于把 §4.12 的负债留在原地。

**上线策略**：M1/M2 默认开；DB-14 的「写回数据库」与 DB-08 的 `Raw SQL` 建议先进设置项开关（默认关），收集一期反馈后转默认开。

---

## 12. 风险

| 风险 | 影响 | 缓解 |
|---|---|---|
| `CellWrite` 三态破坏驱动契约 | 全部插件需同步发版 | 单独发版窗口；先在 4 个主力驱动验证；`PROTOCOL_VERSION` 变更走既定流程 |
| 区域选择与现有行选择语义冲突 | 删除/导出范围出错（危险） | 选择态单一来源 + 派生 `selectedRows`；删除行做二次确认并回显范围 |
| 粘贴造成误写 | 数据损坏 | 一律进 `pendingChanges`（不直接执行）；提交前预览 SQL + 指纹；非法格阻止提交 |
| `Raw SQL` 筛选成为注入面 | 安全 | 强制 `sql_guard`；仅 WHERE 片段；拒绝多语句；E2E 覆盖注入用例 |
| 估算行数与精确值不一致引发投诉 | 信任受损 | 强制 `≈` 标识 + tooltip + 一键精确统计；类型层面区分 kind |
| 默认排序改动引入分页重复/遗漏（**竞品已踩过的坑**） | 数据误读，比性能问题更严重 | 关排序时显式提示不稳定；主键不可解析时明确报出而非静默降级；keyset 作为正解同批评估 |
| `DataTable` 拆分引入回归 | 现有 E2E 全红 | 拆分与功能分开提交；`e2e/specs/table-*.ts` 作为回归门禁 |
| i18n 膨胀（约 130 个新 key） | 发布前补齐成本 | 开发期只改 `en`；发布前跑 `node scripts/i18n-sync-check.mjs` + i18n-sync skill |
| 偏好与命名视图落盘膨胀 | 设置文件变大 | LRU 上限 + 一键清空偏好 |
| 主检出为 detached HEAD 且 HEAD 在分析期间被移动过；分支引用在多 worktree 间共享（[worktree-isolation.md](../development/worktree-isolation.md) §1） | 门禁结果无法归属到某个提交 | 本轨道在 `feature/data-browsing-prd` 的独立 worktree 内进行；门禁前后各记录 HEAD/工作区 sha；提交方与验证方使用独立 worktree |

---

## 13. 附录 A：对标来源

### A.1 TablePlus 官方文档（现行权威）

| 主题 | URL |
|---|---|
| Filter（运算符、多条件 AND/OR、Raw SQL、Any column、快捷筛选、外键箭头、筛选设置、列筛选） | https://tableplus.com/docs/gui-tools/filter |
| Row（插入/复制/编辑/Set Value/区域选择/多列排序/Quick Look/外键跳转/删除/复制格式） | https://tableplus.com/docs/gui-tools/working-with-table/row |
| Table Basics（默认 300 行/页、Limit/Offset/Go） | https://tableplus.com/docs/gui-tools/working-with-table/table |
| Search in Database（全库值搜索、暂停/继续、按 Schema 分组） | https://tableplus.com/docs/gui-tools/search-in-database |
| Working With Query Results（结果可编辑、复制格式、搜索替换、Chart、导出） | https://tableplus.com/docs/query-editor/working-with-query-results |
| Streaming Results & Async Loading | https://tableplus.com/docs/query-editor/streaming-results-and-async-loading |
| Editor Basics（行数上限菜单，默认 No limit） | https://tableplus.com/docs/query-editor/editor-basics |
| Code Review & Safemode（暂存/提交/预览/丢弃、五档安全模式） | https://tableplus.com/docs/gui-tools/code-review-and-safemode |
| Preferences → Table Data（Estimate count、交替行、滚动条） | https://tableplus.com/docs/preferences/general/table-data |
| Shortcut Keys | https://tableplus.com/docs/utilities/shortcut-keys |
| Open Anything（`⌘+P` 模糊打开对象） | https://tableplus.com/docs/gui-tools/open-anything |
| ER Diagram | https://tableplus.com/docs/gui-tools/diagram |
| Import & Export（格式选项、后台导出） | https://tableplus.com/docs/gui-tools/import-and-export |
| Metrics board | https://tableplus.com/docs/gui-tools/metrics-board |
| Version History / Changelogs | https://tableplus.com/blog/2017/02/changelogs.html |

### A.2 TablePlus 旧站（GitBook，仅在为唯一来源时引用，需注意与现行文档的矛盾）

- 文档索引与 Markdown 直取约定：https://docs.tableplus.com/llms.txt
- Code Preview / Commit changes / Safe mode / Discard changes：`https://docs.tableplus.com/gui-tools/code-review-and-safemode/*.md`
- Right sidebar（多行批量编辑、字段搜索）：https://docs.tableplus.com/gui-tools/the-interface/right-sidebar.md
- Quick Look（`⌘+Return` / 中键）：https://docs.tableplus.com/gui-tools/the-interface/quick-look.md
- Multi Tabs / Workspaces / Windows：https://docs.tableplus.com/gui-tools/the-interface/multi-tabs-workspaces-windows.md
- Table Data 偏好（Estimate count 旧描述）：https://docs.tableplus.com/preferences/general/table-data.md
- LLM Plugin（旧隐私声明：「你的数据行永远不会发送给 LLM」——与现行「助手可执行查询」存在矛盾，**不作断言**）：https://docs.tableplus.com/llm-plugin.md

### A.3 社区证据（GitHub issue / 论坛）

| 主题 | 链接 |
|---|---|
| 打开表注入 `ORDER BY <主键>` 致 ClickHouse 85.75 s、UI 无响应；厂商回应「去排序可能导致分页错误」 | https://github.com/TablePlus/TablePlus/issues/3592 |
| 同一根因在 SingleStore 上 >1 分钟 | https://github.com/TablePlus/TablePlus/issues/2662 |
| 无主键表硬失败 `column "id" does not exist` | https://github.com/TablePlus/TablePlus/issues/651 |
| `ctid` 泄漏进导出 | https://github.com/TablePlus/TablePlus/issues/266 |
| 提交生成无 WHERE 的 UPDATE（PG 外部表），仍开、厂商补丁失败 | https://github.com/TablePlus/TablePlus/issues/3568 |
| 多列排序丢失前一列（仍开、已标 bug） | https://github.com/TablePlus/TablePlus/issues/2843 |
| 「记住排序」系列请求 | https://github.com/TablePlus/TablePlus/issues/2913 · /2919 · /3154 · /3812 · https://github.com/TablePlus/tableplus-windows/issues/708 |
| `Do not sort` 生成非法 SQL | https://github.com/TablePlus/TablePlus/issues/3813 |
| SQLite 整数按字符串排序 | https://github.com/TablePlus/TablePlus/issues/3820 |
| 缺 `NULLS FIRST/LAST` | https://github.com/TablePlus/TablePlus/issues/3629 |
| 批量编辑约 50 行卡死数分钟（16 条评论） | https://github.com/TablePlus/TablePlus/issues/3239 |
| 提交后连接卡死 / 编辑上下文丢失 | https://github.com/TablePlus/TablePlus/issues/3724 · /2979 |
| 含糊的「All changes were reverted」 | https://github.com/TablePlus/TablePlus/issues/718 · /3021 |
| `bytea` 主键导致更新失败 | https://github.com/TablePlus/TablePlus/issues/715 · /1983 · /3586 |
| Safe Mode 漏报警告 | https://github.com/TablePlus/TablePlus/issues/3257 |
| 「回车即提交」自 2019 年未实现 | https://github.com/TablePlus/TablePlus/issues/967 |
| 无只读连接模式（两次请求均关闭未实现） | https://github.com/TablePlus/TablePlus/issues/539 · /2912 |
| 导出不流式：2000 万行 → 8 GB | https://github.com/TablePlus/TablePlus/issues/3448 |
| 导出回归：5100 万行 → 242 GB 虚拟内存 | https://github.com/TablePlus/TablePlus/issues/3786 |
| 导出损坏（gzip 截断 / CSV 非法） | https://github.com/TablePlus/TablePlus/issues/3955 · /3733 |
| 「导出筛选后的表」仍未实现 | https://github.com/TablePlus/TablePlus/issues/3663 |
| FK 箭头单击/双击语义不清 | https://github.com/TablePlus/TablePlus/issues/516 |
| 复合 FK 问题 / 跨库崩溃 / Oracle 未修 | https://github.com/TablePlus/TablePlus/issues/1931 · /1648 · /2509 |
| 单元格内 FK 选择器缺失（2020 提出，2026 重提被 duplicate 关闭） | https://github.com/TablePlus/TablePlus/issues/2142 · /3782 |
| FK 点击落到 raw SQL 筛选 | https://github.com/TablePlus/TablePlus/issues/3397 |
| 右侧栏无 FK 箭头 | https://github.com/TablePlus/TablePlus/issues/1678 |
| 列冻结请求 | https://github.com/TablePlus/TablePlus/issues/3752 |
| 列级统计请求（COUNT/COUNT DISTINCT/SUM/AVG） | https://github.com/TablePlus/TablePlus/issues/1394 |
| 网格内正则搜索请求（「20+ 列时得横向滚很久找列名」） | https://github.com/TablePlus/TablePlus/issues/2279 |
| 文本换行请求 | https://github.com/TablePlus/TablePlus/issues/2497 |
| 命名/持久筛选视图请求 | https://github.com/TablePlus/TablePlus/issues/3276 |
| 大表分页失效（SQL Server 视图 / DuckDB / offset 过大） | https://github.com/TablePlus/TablePlus/issues/3234 · /3498 · /3643 |
| 无自动加载请求（2018 至今未实现） | https://github.com/TablePlus/TablePlus/issues/759 |
| 5 万张表 → 约 500 s 且阻塞 UI | https://github.com/TablePlus/TablePlus/issues/2295 |
| 默认 No limit 导致误查百万行表 | https://github.com/TablePlus/TablePlus/issues/2125 |
| 筛选输入延迟回归（每次按键 1 秒） | https://github.com/TablePlus/TablePlus/issues/3550 · /3569 |
| 试用版 2 条高级筛选 + 弹窗卡死 | https://github.com/TablePlus/TablePlus/issues/2085 |
| LIKE 运算符替代方案（官方回复 contains/has prefix 等） | https://github.com/TablePlus/TablePlus/issues/440 |
| 缺 NOT LIKE 等价项 | https://github.com/tableplus/tableplus-windows/issues/73 |
| 平台与用户评价（速度、表格像 Excel、价格、迁移意愿） | Hacker News 与 Setapp 评论，2022–2026（见附录 C 检索说明） |

### A.4 存疑项（本 PRD 未据此下断言）

- TablePlus 是否虚拟化网格渲染；是否支持网格内枚举下拉与布尔开关；是否有 JSON 复制 path；是否支持网格内列冻结/钉列（仅结构页可重排）；无主键表行编辑的官方语义；`⌘+Z` 是否覆盖数据编辑；Postgres 数组/hstore/复合类型的渲染质量（只有间接社区证据：「TablePlus is good but not great with arrays, hstore etc」）；跨连接/跨库 JOIN 到同一网格。
- **给落地团队的手工验证建议**（无法从文档判定、且价值高）：100 万行表的滚动流畅度 · 无主键表就地编辑 · 布尔/枚举单元格编辑器 · JSON 复制 path · PG 数组/hstore 渲染 · 列级聚合与直方图。

---

## 14. 附录 B：DataZen 符号索引（替代行号引用）

> 仓库纪律（[docs/README.md](../README.md) 维护规则 7）禁止「文件名 + 行号」形式的引用：行号是一次性坐标，代码增删一行即失效，且没有任何门禁校验。因此本文所有代码引用一律给出**文件 + 符号**。定位时用 CodeGraph 或按符号名检索。

| 事实 | 文件 | 符号 / 位置 |
|---|---|---|
| `editBuffer` 在 props 中声明但未使用 | `src/components/DataTable/DataTable.tsx` | `DataTableProps.editBuffer`（未出现在 `DataTable` 解构参数中） |
| 网格键盘仅 Delete/Backspace | `src/components/DataTable/DataTable.tsx` | `handleDeleteKey` |
| 面板键盘仅 Cmd/Ctrl+Enter | `src/windows/connection/TableView.tsx` | `handleTableKeyDown` |
| 编辑器单行 input + 类型子串强转 | `src/components/DataTable/EditableCell.tsx` | `EditableCell`、`coerceAndCommit`、`toEditString` |
| 排序单列 | `src/stores/tableDataStore.ts` | `setSort`（写 `sorts: [sort]`） |
| `stageCellChange` 直接改写 rows、无 dirty 标记 | `src/stores/tableDataStore.ts` | `stageCellChange` |
| 行选择状态定义 | `src/stores/tableData/types.ts` | `TableState.selectedRows` |
| 分页页大小选项 | `src/components/DataTable/Pagination.tsx` | 页大小 `Select` 的 `options` |
| 筛选运算符全集 | `src-tauri/src/services/query_executor.rs` | `QueryExecutor::format_condition` |
| WHERE 拼接与单一逻辑 | `src-tauri/src/services/query_executor.rs` | `build_select_sql`、`build_count_sql`、`filter_join` |
| SELECT/COUNT 生成与并发执行 | `src-tauri/src/services/query_executor.rs` | `QueryExecutor::get_table_data` |
| 默认 `ORDER BY <主键>` 注入 | `src-tauri/src/services/query_executor.rs` | `build_select_sql` 的 `has_order_by` 分支 |
| `effective_skip_count` 决策 | `src-tauri/src/commands/schema.rs` | `get_table_data_impl` |
| 表达式解析器运算符与拒绝规则 | `src/lib/filterExpression.ts` | `FilterExpressionOperator`、`Tokenizer` 的运算符分支 |
| 混用 AND/OR 被拒 | `src/windows/connection/TableView.tsx` | `handleQuickFilter`（`filter.mixedLogic`） |
| 计划模型只有 updates/deletes | `src/lib/tableChanges.ts` | `RowChangePlan`、`PendingRowChange` |
| 提交循环与 `affected == 1` 强校验 | `src-tauri/src/commands/data.rs` | `commit_pending_changes_impl` |
| driver 只有 update/delete 构造器 | `packages/driver-api/src/traits.rs` + `src/traits/sql_text.rs` | `build_update_sql`、`build_delete_sql` |
| 能力位：跳过计数 / 支持 offset / 分页语法 | `packages/driver-api/src/traits.rs` | `skip_count_query`、`supports_offset`、`pagination_syntax` |
| FK 元数据类型 | `src/types/connection.ts` | `ForeignKeyInfo`、`TableSchema.foreignKeys` |
| 查询结果网格编辑即丢弃 | `src/windows/connection/result-workspace/ResultTableView.tsx` | `handleCellDoubleClick`、只读结果 `DataTable` 装配 |
| 结果单元格本地改写 | `src/stores/panelStore.ts` | `updateResultCell` |
| 单元格截断 120 字 | `src/components/DataTable/CellRenderer.tsx` | `CellRenderer` 的文本/JSON 分支 |
| QuickLook 触发与弹层 | `src/components/DataTable/DetailPanel.tsx` | `FieldRow`、`QuickLookDialog` |
| 导出格式选项 | `src/lib/exportDialogShared.ts` | `ARRAY_EXPORT_FORMAT_OPTIONS`、`OBJECT_EXPORT_FORMAT_OPTIONS` |
| 全表流式导出能力位 | `src/lib/exportCapability.ts` | `resolveExportScope`、`supportsFullTableExport` |
| 列宽无持久化 | `src/hooks/useColumnResize.ts` | `useColumnResize`（无 storage 读写） |
| 列工具只有可见性 | `src/windows/connection/TableColumnFilter.tsx` | `TableColumnFilter`、`onChange` |
| 右键菜单全量条目 | `src/lib/dataTableContextMenu.ts` | `buildDataTableContextMenuItems` |
| 虚拟滚动 | `src/hooks/useVirtualTable.ts` | `useVirtualTable`（`overscan`） |
| 网格主体与单元格反查 | `src/components/DataTable/VirtualBody.tsx` + `src/lib/dataTableContextMenu.ts` | `VirtualBody`、`resolveDataTableCellFromEvent` |
| AI 自然语言筛选 | `src/components/ai/NlFilterInput.tsx` + `src-tauri/src/commands/ai/generate.rs` | `NlFilterInput`、`ai_parse_filter_impl` |
| 对象名搜索（非数据搜索） | `src/windows/connection/navigator/GlobalObjectSearch.tsx` + `src/lib/schemaObjectSearch.ts` | `GlobalObjectSearch`、`searchSchemaObjects` |
| 驱动元数据（前端差异位） | `src/lib/databaseMeta.ts` | `DatabaseTypeMeta`（`defaultPageSize`、`exportScope`、`readOnly`） |
| 可复用的 Redis JSON 四模式编辑器 | `packages/drivers/redis/ui/value-editors/` | `JsonEditor`、`jsonModes`、`JsonModeBar` |
| 大值哨兵 64 KiB | `packages/drivers/redis/ui/value-editors/redisBigValue.ts` | `BIG_VALUE_SENTINEL_BYTES`、`BigValueVerdict` |
| 组件架构与 DataTable 说明 | `docs/architecture/frontend/components.md` | §9.1「DataTable 组件架构」 |
| 数据浏览状态归属 | `docs/architecture/frontend/state.md` | `tableDataStore` 条目 |

---

## 15. 附录 C：调研方法与局限（供复核者判断结论强度）

1. **两套官方文档并存且互相矛盾**（`tableplus.com/docs` 与 `docs.tableplus.com`），本文件以现行文档为准，旧站仅在为唯一来源时引用并标注。凡是两者冲突处（如快捷键、隐私声明），一律记为存疑。
2. **竞品大量网格行为未写进文档**——最关键的一条（打开表注入 `ORDER BY <主键> LIMIT 300 OFFSET 0`）只存在于厂商开发者对 issue 的回复与用户抓包中。因此本文件把 `［变更］`/`［社区］` 与 `［文档］` 严格分离。
3. **GitHub issue 的「closed」多为分诊而非已交付**；凡涉及结论的关键 issue，均引用其关闭理由而非仅看状态。
4. **未实际运行 TablePlus 做黑盒验证**：文档结论强度为「来源声明」，社区结论强度为「用户主张」。上表 A.4 列出了应当手工验证的项。
5. **Reddit 未能直接检索**（平台检索 404，无 OAuth 授权），r/SQL 与 r/PostgreSQL 的 GUI 对比讨论未覆盖；G2/TrustRadius/Product Hunt 为 JS/登录墙，仅 Setapp 被完整抓取，Capterra 为摘要级。这些缺口会影响「用户情绪」类结论的完整性，但不影响功能矩阵。
