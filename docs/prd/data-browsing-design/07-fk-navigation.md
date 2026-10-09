# 07 · 外键跳转与返回栈（DB-07）

> **状态**：目标设计，**未实现**。本册描述的箭头入口、跳转编排、返回栈、`resolve_foreign_key_targets` 命令今天在代码里**全部不存在**；带「新增」标记的符号一律按新增对待。
>
> **依赖**：`00-contracts`（§1.6 驱动契约、§4 选择模型、§6.1 新命令登记、§8 错误前缀、§10 写作模板）；`01-selection`（`src/stores/tableData/gridSelection.ts` 与 `src/hooks/useGridSelection.ts`，以及 `src/components/DataTable/GridCell.tsx` 与 `src/lib/gridEventGuards.ts` 的落地；01 同时负责把 `TableView.tsx` 从 770 行降到 ~660，本册的抽取方案建立在它之上）。
>
> **被谁依赖**：`DB-20`（单元格内 FK 选择器 + 反查引用行）——PRD 路线图明确 DB-20 在 DB-07 完成后开始，且复用本册的 `resolve_foreign_key_targets` 与「目标唯一性」判定。
>
> **预估人日**：4.5 人日（Rust 命令与单测 1.5；前端纯逻辑与返回栈 1.0；UI 接线与 TableView 抽取 1.0；journey/E2E 1.0）。
>
> **不包含什么**：不含 FK 单元格内的**下拉选择器**与**反查引用行（referenced by）**（DB-20，P2）；不含跨库 FK 的元数据扩展（见第 11 节 U-1）；不含「关系视图 / 迷你 ER 图」类新组件（PRD 明确不新增）；不含任何 `driver-api` trait 方法或 `PROTOCOL_VERSION` 改动；不含对 `selectedRows` 语义的修改（那属 01）。

---

## 1. 目标与验收口径

**一句话目标**：让网格里**真正被外键引用的列**（复合键按整组）出现可点击的跳转入口，点击后**开新标签页**并把「被引用列 = 当前值」作为**结构化可见筛选条件**注入该标签页；目标标签页顶部给出「来自 `源表.本地列 → 目标表.被引用列`」面包屑与返回入口，返回时能回到来源标签页并恢复其视口；跳转路径可前后导航、且有界（循环引用不无限增长、失败有可读的拒绝理由）。

### 1.1 验收标准（每条都可执行、可逐字断言）

| # | 验收点 | 命令 / 期望输出 |
| --- | --- | --- |
| A1 | 纯逻辑正确（含复合键与空值） | `npx vitest run src/lib/__tests__/fkNavigation.test.ts src/stores/__tests__/fkNavigationStore.test.ts` → 末尾 `Test Files 2 passed`、`Tests N passed`、**无** `FAIL` 行，退出码 `0` |
| A2 | Rust 命令契约正确 | `cargo test -p datazen --lib commands::fk_navigation` → `test result: ok.`，且**没有** `FAILED`；命令用例覆盖复合键定位、空列、探测失败、候选上限 |
| A3 | 单元格 DOM 与交互 | `npx vitest run src/components/DataTable/__tests__/GridCell.fkArrow.test.tsx` → 全绿；断言 `[data-dt-fk="true"]` 只出现在 FK 组成员格上、NULL 格无箭头 |
| A4 | 连续旅程（状态机全过程） | `npx vitest run src/components/DataTable/__tests__/fkNavigation.journey.test.tsx` → 全绿；三条 journey（单列跳转 / 返回栈 / 循环引用）逐步断言 |
| A5 | 真实鼠标与真实取数 | `pnpm e2e:minimal` 后 `e2e/specs/table-fk-navigation.ts` 全通过（TC-FK-001~006，含复合 FK 种子数据） |
| A6 | 测试文件参与类型检查且干净 | `pnpm typecheck` → 无 `error TS`，退出码 `0` |
| A7 | 单文件不越 800 行红线 | `wc -l src/windows/connection/TableView.tsx src/components/DataTable/DataTable.tsx src/components/DataTable/VirtualBody.tsx src/components/DataTable/GridCell.tsx src/stores/fkNavigationStore.ts src/lib/fkNavigation.ts` → 全部 `< 800`，且 `TableView.tsx ≤ 640`、`DataTable.tsx ≤ 470`（沿用 01 的自我约束） |
| A8 | 零硬编码 | `git diff` 中 `src/` 里**不出现**任何按库名/驱动 id 的分支（无 `=== 'mysql'`、`=== 'postgres'` 式判断）；目标解析全部走宿主 + 驱动元数据 |

> **纪律（AGENTS.md）**：A1~A6 的长输出一律重定向到系统临时目录再 `tail` / `grep`，退出码单独打印并如实记录；门禁运行**首尾各记录一次 HEAD 与工作区 sha**；本轨驱动集为 `--drivers=basic`（postgres / mysql / sqlite / redis），"宿主用例全绿"只在该驱动集语义内成立。

---

## 2. 现状代码事实

> 本节每一条都是**亲自读过源码**后写下的事实；只列**文件路径 + 符号名**，**不写行号**（行号随重构腐烂且无门禁校验）。标「新增」的条目今天**不存在**。

### 2.1 外键元数据从哪来

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `packages/driver-api/src/types.rs` | `ForeignKeyInfo { name, columns: Vec<String>, referenced_table: String, referenced_columns: Vec<String>, on_update: String, on_delete: String, deferrability: ForeignKeyDeferrability }`；`ForeignKeyDeferrability` 默认 `Unknown`，`#[serde(default)]`；**没有** `referenced_schema` / `referenced_database` 字段 |
| 同上 | `TableSchema { table_name, columns, primary_keys, indexes, foreign_keys, check_constraints, table_options }`；`TableSchema::effective_primary_keys()` 优先 `primary_keys`，回退列上的 `is_primary_key`；`IndexInfo { name, columns, is_unique, is_primary, index_type }` |
| `src/types/connection.ts` | TS 侧镜像 `ForeignKeyInfo`（`referencedTable` / `referencedColumns` / `onUpdate?` / `onDelete?` / `deferrability?`）、`TableSchema`、`IndexInfo`、`TableInfo { name, schema?: string \| null, tableType, rowCount?: number \| null }`、`ColumnSchema`、`Value` |
| `src/lib/schemaCache.ts` | `getCachedTableSchema(dbSessionId, tableName, database, schema?)`：走 `schemaClient.readSchema` → 驱动命令 `read_relation_schema`，结果 `deepFreeze` 后缓存；`invalidateSchemaCache` / `subscribeSchemaInvalidation`；`relationKey` 参与缓存键，因此**同名不同 schema 不会撞车** |
| `packages/driver-sdk/src/ipc/schemaClient.ts` | `schemaClient.readSchema(dbSessionId, relation)` 返回 `{ value: RelationSchema }`，即前端 `schemaCache` 的数据源 |
| `src/windows/connection/ForeignKeysView.tsx` | 既有唯一"消费 FK 列表"的视图：`useEffect` + `getCachedTableSchema` 取 `schema.foreignKeys` 后渲染表格；**纯展示，没有跳转** |
| `src/lib/relationPrediction/fromTableSchema.ts` / `src/lib/relationMetadata/types.ts` | `declaredForeignKeys(schema)`、`EditorRelationMetadata.foreignKeys`：SQL 编辑器补全与关系预测消费 FK，**与网格无关** |
| `packages/drivers/postgres/src/schema.rs` | `qualified_pg_table_identity(schema, table) = "{schema}.{table}"`（写入 `referenced_table`，即 **PG 的 `referenced_table` 带 schema 前缀**）；`normalise_fk_columns(columns, referenced_columns)` 把 information_schema 的重复聚合列折叠为有序去重列，无法配对时返回 `None` |
| `packages/drivers/mysql/src/mysql.rs` | `referenced_table: ref_table`（裸表名）⇒ **`referenced_table` 的形态因驱动而异，前端不得自己按 `.` 拆名** |

**结论（本册的核心约束）**：目标表的定位必须由**宿主**结合驱动的 `get_table_schema` 完成，前端只消费宿主归一化后的结果；前端**禁止**解析 `referencedTable` 字符串。

### 2.2 面板与标签页范式

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `src/stores/panelStore.ts` | `panels: Panel[]`、`activePanelId`、`addPanel(panel, activate = true, paneId = DEFAULT_PANE_ID)`（append + 更新 pane 焦点）、`removePanel(panelId)`、`setActivePanel(panelId)`、`updatePanel(panelId, patch)`、`removePanelsForRelation(...)`；模块导出 `nextPanelId` 与 `onPanelClosed`（`createPanelCloseNotifier(usePanelStore)`） |
| `src/stores/panelTypes.ts` | `TablePanel extends PanelBase { type: 'table'; tableName; database; tableSchema: string \| null; subTab: SubTabId; structureEditing?; targetColumn? }`；`SubTabId = 'data' \| 'structure' \| 'indexes' \| 'foreignKeys' \| 'ddl'`；`Panel` 是联合类型；`panelSchema` 辅助 |
| `src/windows/connection/usePanelHandlers.ts` | `handleSelectTable(table, schema, database, subTab?, targetColumn?)`：**按 `(tableName, database)` 去重**——已存在同表标签时只 `updatePanel(subTab/targetColumn)` + `setActivePanel` 后 `return`，**不新建标签、不注入任何筛选**；`handleEditTableStructure` / `handleOpenStructure` 同构；`resolveOpenTarget` 解析显式打开目标 |
| `src/windows/connection/ContentView.tsx` | 只渲染 `activePanel` 的内容（`activePanel && !isKvPanel && …`）⇒ **切换标签会卸载非活动面板的整棵子树** |
| `src/windows/connection/PanelContentRenderer.tsx` | 对 `panel.type === 'table'` 挂 `<TableView panelId database={panel.database} schema={panel.tableSchema ?? null} tableName={panel.tableName} databaseType=… />` |

**结论**：跳转**不能**复用 `handleSelectTable`（去重会命中已打开的同表标签，而筛选状态在 `useTableDataStore.byPanel` 里、不在 `TablePanel` 上，于是"跳过去"会什么都没发生）。跳转必须走**新建标签 + 预置切片**的新编排函数。

### 2.3 网格侧：箭头要落在哪

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `src/windows/connection/TableView.tsx` | `TableViewProps { panelId, dbSessionId, database, tableName, connectionId?, schema?, databaseType?, readOnly?, dataExportCapability? }`；`isConnectionReadOnly` / `isEditable`；`ts = useTableDataStore(s => s.byPanel.get(panelId))`；`actions` 逐动作按 `panelId` 绑定（含 `setFilters` / `clearFilters` / `setPage`）；`displayedColumns` 由 `ts.visibleColumns` 过滤；`rowArrays = rows.map(record => displayedColumns.map(col => record[col.name] ?? null))`；`needsLoad = !hasData \|\| sliceContextKey !== contextKey` + `useEffect` 触发首次 `actions.load()`；`handleTableKeyDown` 今天**只认** `Cmd/Ctrl+Enter`（提交暂存改动）；`PendingPlanDialog` 定义在同一个文件里 |
| `src/stores/tableDataStore.ts` | `byPanel: Map<string, TableState>`；`loadTableData({ panelId, …target, skipCount? })`（首帧用 `existing.filters`）；`reloadPanel(panelId, opts?)`；`setFilters(panelId, filters, logic = 'and')` → `patchPanelForReload` **并立即 `reloadPanel`**；`patchPanelForReload` 只并入 updater 结果 + `requestRevision + 1`；`commitFetchedPage` 清 `selectedRows` / `editingCell`；`patchPanel` 对**没有切片的 panel 是 no-op**；`emptyTableState(context)` |
| `src/stores/tableData/types.ts` | `TableState`：`context`、`columns`、`rows: Record<string, unknown>[]`、`totalRows`、`page/pageSize`、`filters/draftFilters/filterLogic/draftFilterLogic`、`filterPanelOpen`、`sorts`、`editBuffer`、`pendingChanges`、`rowIdentityAnchors`、`selectedRows`、`lastSelectedIndex`、`editingCell`、`visibleColumns`、`requestRevision/loadingRevision` |
| `src/components/DataTable/DataTable.tsx` | `DataTableProps`（含 `columns`、`rows`、`editingCell`、`selectedRows`、`onRowSelect`、`onCellDoubleClick`、`primaryKeyColumns`、`onDeleteRows`、`getContextCellText`、`exportTableName`、`headerActions`、`emptyPlaceholder`）；`const [scrollEl, setScrollEl] = useState<HTMLDivElement \| null>(null)`；滚动容器 `<div ref={setScrollEl} className="min-h-0 flex-1 overflow-auto">`；`VirtualBody` 挂 `scrollElement={scrollEl}` |
| `src/components/DataTable/VirtualBody.tsx` | `VirtualBodyProps` / `VirtualRowProps`；`VirtualRow` 单元格外层 `div` 带 `data-testid="data-table-cell"`、`data-dt-row={vRow.index}`、`data-dt-col={col.name}` 与 `onDoubleClick`；行 div `tabIndex={0}`；用 `useVirtualTable` 只渲染虚拟窗口内的行 |
| `src/components/DataTable/CellRenderer.tsx` | `CellRendererProps { columnName, dataType, value, isEditing, onCommit, onCancel }`；`memo`；`value === null \|\| undefined` → 斜体 `NULL`；**没有任何交互与跳转视觉** |
| `docs/prd/data-browsing-design/01-selection.md`（已登记的 01 计划） | 01 **新增** `src/components/DataTable/GridCell.tsx`（`GridCellProps`：`rowIndex`/`column`/`value`/`width`/`selected`/`editing`/`onCellDoubleClick`/`onCellEdit`/`onCellEditCancel`）与 `src/lib/gridEventGuards.ts`（`isComposingEvent` / `isEditableTarget`）；把 `data-dt-*` 收敛到 `GridCell`；把 `TableView.tsx` 的暂存改动条抽成 `src/windows/connection/TablePendingChangesBar.tsx`（770 → ~660）；`DataTable.tsx` 抽右键菜单到 `useDataTableContextMenu.ts`（628 → ~450） |

**结论**：箭头的唯一落点是 01 建立的 `GridCell`（DOM 契约与 `memo` 都在那里），本册**不改** `CellRenderer` 的渲染语义、**不重做** `data-dt-*` 映射。

### 2.4 宿主侧：新命令的形态

| 文件 | 已核实的符号 / 事实 |
| --- | --- |
| `src-tauri/src/commands/schema.rs` | `get_table_data_impl(state, db_session_id, table, page, page_size, filters, sorts, skip_count, filter_logic, database, schema)`：先 `connection_manager.get_session`，再 `get_session_config` 取 `config.database` / `config.schema` 兜底，`metadata_schema(driver, explicit, embedded_schema_of(table), config.schema)` 决定 schema，最后 `QueryExecutor::new(state.schema_cache.clone()).get_table_data(...)`；`embedded_schema_of(table)` 是**该文件的私有函数**；`#[tauri::command] get_table_data` / `get_er_data` 是薄壳 |
| `src-tauri/src/services/schema_scope.rs` | `metadata_schema(driver, explicit, table_schema, config_schema) -> Option<String>`：`!driver.has_schema_level()` 时直接 `None`，否则按 `explicit → table_schema → config_schema → default_schema()` 取第一个非空 |
| `src-tauri/src/services/query_executor.rs` | `QueryExecutor::new(schema_cache)`；`get_table_data(driver, handle, db_session_id, database, schema, table, page, page_size, filters, order_by, skip_count, filter_logic)`：**`skip_count = true` 时只发分页 SELECT，不发 COUNT**（存在性探测与候选值都能直接复用它，无需新 SQL 构造函数） |
| `packages/driver-api/src/traits.rs` | `get_table_schema(&self, handle, table, database, schema) -> Result<TableSchema, DriverError>`（宿主读源表 FK / 目标表 PK+索引的权威入口）；`has_schema_level()` / `has_multi_database()` / `default_schema()` / `quote_ident` / `pagination_syntax` |
| `src-tauri/src/commands/error.rs` | `CommandError` 枚举（`Validation` / `NotFound` / `Driver` / `Internal` …）；`Serialize` 实现**先脱敏再 `serialize_str`**，前端只拿到字符串、**没有 `code` 字段**；`CmdExt::cmd_err(cmd)` 只记录脱敏日志并把错误**原样**返回，**不会**加 `grid.*` 前缀 |
| `src-tauri/src/bootstrap/run.rs` | `tauri::generate_handler![…]` 里逐条列出 `crate::commands::get_table_data` / `get_er_data` 等；新命令必须在此登记 |
| `src-tauri/src/commands/mod.rs` | `pub mod schema;` + `pub use schema::*;`；新命令模块按同一范式登记 |
| `src/commands/database.ts` | `databaseCommands.getTableData(params)` 是前端 IPC 薄封装（camelCase 入参） |
| `src-tauri/src/testing/mod.rs` / `src-tauri/src/testing/app_state.rs` / `packages/driver-api/src/mock_driver.rs` | `crate::testing::mock_driver::{MockDriver, MockDriverOptions}`；`TestAppState::{new, with_options, with_tables, save_and_connect}` 暴露 `state: AppState`；`MockDriverOptions.table_schema: Option<TableSchema>` 会被 `get_table_schema` 原样返回，`query_rows` 提供查询行、`query_error` 模拟查询失败、`count_total` 提供计数 ⇒ **复合外键的 Rust 单测不需要改 mock driver** |
| `src-tauri/src/commands/schema/tests.rs` | 既有范式：`use super::*;` + `TestAppState::with_options(opts).await` + `test.save_and_connect("…").await` 后直接调 `*_impl(&test.state, …)` |

### 2.5 已核实"不存在"的能力（避免误判为既有）

| 目标 | 事实 |
| --- | --- |
| 网格内 FK 跳转 | `src/` 全量 grep `referencedTable` / `referenced_table` 只命中类型定义、`relationPrediction`、`relationMetadata`、编辑器语义层与 `ForeignKeysView`；**没有任何跳转 / 返回栈 / 面包屑实现**；`TablePanel.targetColumn` 只有类型声明与写入点，**没有渲染侧消费者** |
| `src/lib/gridErrors.ts` | 不存在；由 03/05 首次建立（01 的未决问题 Q4 已登记），07 只做**追加** |
| `src/stores/tableData/gridSelection.ts`、`src/hooks/useGridSelection.ts`、`src/components/DataTable/GridCell.tsx` | 均不存在；由 01 建立（本册依赖它们） |
| `src/lib/queryContextPath.ts` / `src/lib/sqlPathPrefix.ts` | 存在 `splitPathHierarchyDatabasePin` / `parseQualifiedPathParents` 等路径工具，但**都不是"把 `schema.table` 拆成两段"的语义**；本册**不**用前端字符串解析定位目标表（理由见 2.1 结论） |

---

## 3. 数据结构与接口设计

### 3.1 Rust：新增 IPC 命令的 DTO（本地文件 `src-tauri/src/commands/fk_navigation.rs`，**新增**）

```rust
// 宿主惯例（已核实）：`commands/schema.rs` 从 `crate::db` 引 `TableSchema` / `TableDataResult`，
// 从 `crate::services` 引 `FilterCondition` / `QueryExecutor`（`query_executor.rs` 再导出
// `datazen_driver_api::filters::{FilterCondition, FilterOperator}`）。本文件沿用同一对来源。
use crate::db::{ForeignKeyInfo, TableSchema, Value};
use crate::services::{FilterCondition, FilterOperator, QueryExecutor};
use serde::{Deserialize, Serialize};

/// 跳转与选择器共用的请求：只描述"源表的哪一组本地列、当前值是什么"。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFkTargetsRequest {
    pub db_session_id: String,
    /// 源标签页绑定的库；缺省时回落到会话配置（与 `get_table_data_impl` 同序）。
    #[serde(default)]
    pub database: Option<String>,
    /// 源表所在 namespace；缺省时按 `metadata_schema` 的既有顺序解析。
    #[serde(default)]
    pub schema: Option<String>,
    pub table: String,
    /// 可选的显式表引用；给出时优先于 `table`（与 `get_table_data_impl` 一致）。
    #[serde(default)]
    pub qualified_table: Option<String>,
    /// 复合外键的本地列，顺序必须与 `ForeignKeyInfo.columns` 一致。
    pub columns: Vec<String>,
    /// 约束名；给出时优先按名字匹配，避免同列多约束时歧义。
    #[serde(default)]
    pub constraint_name: Option<String>,
    /// 本地列当前值，与 `columns` 同长同序。`Value::Null` 允许传入但会被拒绝。
    pub values: Vec<Value>,
    /// 候选值条数上限（DB-20 复用）。缺省 50，硬上限 200。
    #[serde(default)]
    pub limit: Option<u32>,
}

/// 归一化后的目标引用。前端**只**消费这一份，禁止自己解析 `referenced_table`。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FkTargetRef {
    /// 目标表所在库（今天等于源标签页的库；跨库 FK 见第 11 节 U-1）。
    pub database: String,
    /// 目标表所在 namespace；`None` = 该驱动无 schema 层。
    pub schema: Option<String>,
    pub table: String,
    /// 目标表的被引用列，与本地列**同序**（第 i 个本地列对应第 i 个被引用列）。
    pub columns: Vec<String>,
    /// 被引用列的类型名，与 `columns` 同序；供 DB-20 的键入校验与展示。
    pub data_types: Vec<String>,
    /// 目标表的有效主键（`TableSchema::effective_primary_keys()`）。
    pub primary_keys: Vec<String>,
    /// 被引用列是否被主键或**恰好覆盖这些列**的唯一索引约束。
    /// 为 false 时跳转仍然允许（结果可能多行），但 UI 必须给出提示。
    pub unique: bool,
    /// 本次实际生效的候选上限（回显，供 UI 说明截断）。
    pub candidate_limit: u32,
}

/// 解析结果。`exists = false` **不是错误**：目标表可读但那一行不在了。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FkTargetResolution {
    pub target: FkTargetRef,
    /// 目标表里是否存在至少一行满足全部等值条件（LTIMIT 1 探测，不 COUNT）。
    pub exists: bool,
    /// 候选行（每行按 `target.columns` 顺序），最多 `candidate_limit` 行。
    pub candidates: Vec<Vec<Option<Value>>>,
    /// 候选是否被上限截断。
    pub truncated: bool,
}
```

**逐字段说明**

| 字段 | 用途 | 缺省 / 默认 |
| --- | --- | --- |
| `db_session_id` | 运行时会话（`dbSessionId` 语义，永不落盘） | 必填 |
| `database` / `schema` | 源表定位；与既有 `get_table_data` 的解析顺序一致（显式 → 表内嵌 → 会话配置） | `None` ⇒ 走会话配置 |
| `table` / `qualified_table` | 源表名；`qualified_table` 给出时优先（`embedded_schema_of` 会从它里面取内嵌 schema） | `qualified_table = None` |
| `columns` | 复合外键的本地列，**顺序即配对顺序** | 必填、非空 |
| `constraint_name` | 同列多约束时消歧 | `None` ⇒ 按有序 `columns` 精确匹配 |
| `values` | 本地列当前值，逐位对应 | 必填；任一位为 `Null` ⇒ `grid.fk.valueNull` |
| `limit` | 候选值上限（DB-20 选择器复用同一命令） | `None` ⇒ 50，硬夹到 200 |
| `target.unique` | 决定是否显示"结果可能多行"提示；DB-20 用它决定选择器可用性 | 由目标表主键/唯一索引计算 |
| `exists` | `false` ⇒ 目标标签页照常打开但显示"被引用行不存在"提示 | — |
| `candidates` / `truncated` | 07 不使用（跳转不预取行）；DB-20 使用 | `limit` 内 |

**新增命令（`#[tauri::command]`）**

```rust
#[tauri::command]
pub async fn resolve_foreign_key_targets(
    state: State<'_, AppState>,
    request: ResolveFkTargetsRequest,
) -> Result<FkTargetResolution, CommandError>;

pub(crate) async fn resolve_foreign_key_targets_impl(
    state: &AppState,
    request: ResolveFkTargetsRequest,
) -> Result<FkTargetResolution, CommandError>;
```

> **为什么把入参收成一个 `request` 对象**：DB-07（跳转）与 DB-20（选择器）共用同一命令，字段还会增（DB-20 要排序/搜索）；收成结构体后新增字段只需 `#[serde(default)]`，不必改命令签名（契约 §6.2 铁律：新增字段一律 `Option<T>` + `#[serde(default)]`）。

### 3.2 Rust：错误前缀（本册产生的全部 `CommandError`）

`CommandError::Serialize` 会整串脱敏后再序列化为**字符串**，前端没有 `code` 字段。因此本册的错误一律用 `CommandError::Validation(format!("grid.fk.<原因>: <说明>"))` 构造，前缀是契约的一部分。

| 前缀 | 触发条件 | 说明文本要求 |
| --- | --- | --- |
| `grid.fk.shapeMismatch` | `columns.len() != values.len()`；或某侧长度为 0；或 `ForeignKeyInfo.columns.len() != referenced_columns.len()` | 说清"外键的两侧列数不一致"，**不回显值本身** |
| `grid.fk.unknownConstraint` | 源表 `foreign_keys` 里找不到该 `constraint_name`，也找不到有序 `columns` 精确匹配的约束（表结构在会话期间变了） | 提示"源表外键已变化，请刷新结构后重试" |
| `grid.fk.valueNull` | `values` 中任一位为 `Value::Null`（前端本应拦下，后端做纵深防御） | 说明复合键部分列为 NULL 时无法等值定位 |
| `grid.fk.targetUnresolved` | 目标表元数据读不到：`driver.get_table_schema` 失败（表已删除 / 无元数据权限 / 跨库不可见） | 附**已脱敏**的驱动说明；前端显示前缀分类结论 + 原文 |
| `grid.fk.probeFailed` | 目标表元数据可读，但存在性探测 SELECT 失败（行级权限、连接中断等） | 同上 |

**不使用 `CmdExt::cmd_err` 来加前缀**：`cmd_err(cmd)` 只写脱敏日志并原样返回错误，不会给消息加 `grid.*`。正确写法是先拿到 `DriverError`，用 `CommandError::Validation(format!("grid.fk.targetUnresolved: {}", …))` 包装，再交给 `CmdExt::cmd_err("resolve_foreign_key_targets")` 记录日志（`CommandError::Serialize` 会二次脱敏，前缀是纯文本、不受影响）。

**`exists: false` 不是错误码**：目标行不存在是成功返回，前端据 `exists` 显示提示（`fkNav.targetMissing`），**不得**把它做成 `CommandError`——否则"打开目标标签并显示空结果"这条路径会被错误分支吃掉。

### 3.3 TS：IPC 与 DTO（`src/types/connection.ts` + `src/commands/database.ts`）

```ts
// src/types/connection.ts（新增片段；与 Rust camelCase 一一对应）
export interface FkTargetRef {
  database: string;
  schema: string | null;
  table: string;
  /** 与本地列同序的被引用列。 */
  columns: string[];
  /** 与 columns 同序的类型名。 */
  dataTypes: string[];
  primaryKeys: string[];
  /** 被引用列被主键或"恰好覆盖这些列"的唯一索引约束。 */
  unique: boolean;
  candidateLimit: number;
}

export interface FkTargetResolution {
  target: FkTargetRef;
  /** false = 目标表可读但那一行不在了（**不是错误**）。 */
  exists: boolean;
  candidates: (Value | null)[][];
  truncated: boolean;
}

export interface ResolveFkTargetsParams {
  dbSessionId: string;
  database?: string | null;
  schema?: string | null;
  table: string;
  qualifiedTable?: string | null;
  columns: string[];
  constraintName?: string | null;
  values: Value[];
  limit?: number;
}
```

```ts
// src/commands/database.ts（新增方法）
resolveForeignKeyTargets: (params: ResolveFkTargetsParams) =>
  invoke<FkTargetResolution>('resolve_foreign_key_targets', { request: params }),
```

### 3.4 TS：FK 定位与跳转的纯逻辑（`src/lib/fkNavigation.ts`，**新增**）

```ts
import type { FilterCondition, ForeignKeyInfo, Value } from '../types';
import type { FkTargetRef } from '../types';

/** 返回栈最大深度（超出丢最旧一条并置 truncated）。 */
export const FK_NAV_MAX_DEPTH = 32;

/** 离开网格时的视口快照；返回时据此恢复。 */
export interface GridViewport {
  scrollTop: number;
  /** 锚点行下标（页内）；null = 没有锚点。 */
  anchorRow: number | null;
  /** 锚点列名；null = 没有锚点。 */
  anchorColumn: string | null;
}

/** 一个 FK 组在网格里的可用性描述（每个成员列都能查到同一份）。 */
export interface FkGroupAffordance {
  /** 参与该外键的本地列，顺序 = ForeignKeyInfo.columns。 */
  localColumns: string[];
  /** 目标表的被引用列，顺序与 localColumns 对齐。 */
  targetColumns: string[];
  /** 展示用目标标签：`customers.id` 或 `sales.customers.(a, b)`。 */
  targetLabel: string;
  constraintName: string;
  /** 约束名，供命令消歧。 */
  referenceTable: string;
}

/** 把该表的外键摊平成 `列名 → 组` 的只读索引（成员列的每一列都指向同一组）。 */
export function indexForeignKeys(
  foreignKeys: readonly ForeignKeyInfo[],
): ReadonlyMap<string, FkGroupAffordance>;

/**
 * 该行该列是否可跳转：列属于某个 FK 组、组内**全部**本地列在该行的值都非 null，
 * 且该列是组内**第一个可见成员列**（复合键只在首列画箭头）。
 */
export function isFkCellNavigable(input: {
  columnName: string;
  row: Record<string, unknown> | undefined;
  index: ReadonlyMap<string, FkGroupAffordance>;
  visibleColumns: readonly string[];
  editing: boolean;
}): boolean;

/** 由组与实际值构造等值筛选（`referenced_columns[i] = values[i]`，AND 逻辑）。 */
export function buildFkFilters(
  target: Pick<FkTargetRef, 'columns'>,
  values: readonly Value[],
): FilterCondition[] | null; // 任一值为 null/undefined 或长度不等 → null（调用方不得跳转）

/** 目标定位 + 值的稳定签名，用于循环引用判定与请求去重。 */
export function fkNavKey(target: Pick<FkTargetRef, 'database' | 'schema' | 'table' | 'columns'>, values: readonly Value[]): string;

/** 从加载的外键里按约束名优先、否则按有序 columns 精确匹配。 */
export function findForeignKey(
  foreignKeys: readonly ForeignKeyInfo[],
  columns: readonly string[],
  constraintName: string | null,
): ForeignKeyInfo | null;

/** 恢复视口要用：把锚点行换算成安全的目标 scrollTop（夹在 [0, ∞)）。 */
export function scrollTopForAnchorRow(anchorRow: number, rowHeight: number): number;

/** 面包屑文案参数（i18n 只做拼装，不在这里拼字符串）。 */
export interface FkBreadcrumbModel {
  sourceTable: string;
  localColumns: string[];
  targetTable: string;
  targetColumns: string[];
}
```

**逐项说明**

| 符号 | 用途 | 关键约束 |
| --- | --- | --- |
| `indexForeignKeys` | 让 `GridCell` 用**列名**一次 O(1) 查到所属 FK 组；成员列全部入索引 | 只做名字精确匹配（列名两端都来自同一驱动的 `TableSchema`/`TableData`，不存在大小写漂移的合法场景） |
| `isFkCellNavigable` | 决定箭头是否出现 | 复合键**按整组判定**：任一位为 `null`/`undefined` 即整组不可跳；`editing === true` 时该格不画箭头 |
| `buildFkFilters` | 生成"结构化可见条件" | `operator: 'eq'`，第 i 条用 `target.columns[i]`；长度不等或含 null → 返回 `null`（**绝不**退化成 `IS NULL`） |
| `fkNavKey` | 循环引用检测 + 并发去重 | 对 `database/schema/table/columns` 与值的 JSON 规范化字符串取签名；**必须包含 `schema`**，否则同名不同 schema 会误判为同一节点 |
| `findForeignKey` | 对齐源表约束 | 按 `constraintName` 优先；否则按有序 `columns` 精确匹配（顺序敏感，因为配对是位置性的） |
| `GridViewport` / `scrollTopForAnchorRow` | 返回时恢复视口 | 行虚拟化 ⇒ 目标行可能不在 DOM 里，必须用 `anchorRow × rowHeight` 先把滚动位置放回去，再在下一帧按 `data-dt-row`/`data-dt-col` 定位 |
| `FkBreadcrumbModel` | 面包屑数据 | 只承载**模型**，文案与箭头符号由 i18n key 决定 |

### 3.5 TS：返回栈（`src/stores/fkNavigationStore.ts`，**新增**）

```ts
import type { FkTargetRef } from '../types';
import type { GridViewport } from '../lib/fkNavigation';

/** 进入一次跳转目标的完整记录；`panelId` 指向那次跳转打开的标签页。 */
export interface FkNavEntry {
  /** 条目 id（自增，重启不落盘）。 */
  id: string;
  panelId: string;
  /** `fkNavKey(target, values)`：循环引用判定与去重。 */
  navKey: string;
  /** 目标侧面包屑与恢复所需的最小信息。 */
  origin: {
    sourcePanelId: string;
    sourceTable: string;
    localColumns: string[];
    targetTable: string;
    targetColumns: string[];
  };
  /** **离开该标签页时**写入的视口快照（进入时可能还是 null）。 */
  viewport: GridViewport | null;
}

interface FkNavigationState {
  /** 返回栈：栈顶在数组末尾。 */
  back: FkNavEntry[];
  /** 前进栈：由 goBack 产生。 */
  forward: FkNavEntry[];
  /** 是否曾因超过 FK_NAV_MAX_DEPTH 丢弃过最旧条目（面包屑显示"…"）。 */
  truncated: boolean;
}

interface FkNavigationActions {
  /** 压入一次新跳转；同时清空 forward。 */
  push: (entry: Omit<FkNavEntry, 'id'>) => void;
  /** 返回上一条：返回被激活的条目（调用方负责 setActivePanel），栈空返回 null。 */
  goBack: (existingPanelIds: ReadonlySet<string>) => FkNavEntry | null;
  /** 前进：与 goBack 互为逆操作。 */
  goForward: (existingPanelIds: ReadonlySet<string>) => FkNavEntry | null;
  /** 循环引用：把栈裁剪到既有条目（不新建标签、不清 forward 之外的任何状态）。 */
  rewindTo: (entryId: string) => void;
  /** 记录某标签页离开时的视口（同 panelId 只保留最新）。 */
  recordViewport: (panelId: string, viewport: GridViewport) => void;
  /** 首次进入某标签页时取出其快照（取出即消耗，避免重复恢复）。 */
  takeViewport: (panelId: string) => GridViewport | null;
  /** 当前栈里是否已有该 navKey（循环引用判定）。 */
  findEntryIdByNavKey: (navKey: string) => string | null;
  /** 丢弃所有指向已关闭标签页的条目。 */
  prune: (existingPanelIds: ReadonlySet<string>) => void;
  reset: () => void;
}

export const useFkNavigationStore: UseBoundStore<FkNavigationState & FkNavigationActions>;
```

| 设计点 | 结论与理由 |
| --- | --- |
| 落点 | `src/stores/fkNavigationStore.ts`（与 PRD"返回栈放 `panelStore.ts` 派生状态，不落盘"方向一致，但**独立成 store**：`panelStore.ts` 已 728 行，而契约要求单文件 ≤ 800；独立 store 同时避免 `panelStore` 反向依赖 `lib/fkNavigation`） |
| 不落盘 | 纯内存派生状态；应用重启后没有返回栈（与 PRD 一致） |
| `viewport` 写入时机 | **离开时**写（`push` 时目标标签还没渲染过，写不出视口）；返回/前进都由 `takeViewport` 消费一次 |
| 剪枝 | `goBack` / `goForward` / `prune` 都接收"仍然存在的 panelId 集合"，由调用方从 `usePanelStore.getState().panels` 现取——**不在模块加载期订阅 `onPanelClosed`**，避免循环依赖与初始化顺序问题 |
| 循环引用 | `findEntryIdByNavKey` 命中即 `rewindTo`：激活既有标签、把 `back` 裁剪到该条目、清空 `forward`；**不关闭任何标签** |
| 上限 | `FK_NAV_MAX_DEPTH = 32`；超限 `shift()` 最旧并置 `truncated = true` |

### 3.6 TS：面板载荷增量（`src/stores/panelTypes.ts`）

```ts
/** 目标标签页的"来自哪里"信息；只用于面包屑与返回，不承载筛选条件。 */
export interface FkOrigin {
  /** 来源标签页 id（返回时激活它）。 */
  sourcePanelId: string;
  sourceTable: string;
  localColumns: string[];
  targetTable: string;
  targetColumns: string[];
  /** 对应的返回栈条目 id。 */
  navEntryId: string;
}

export interface TablePanel extends PanelBase {
  type: 'table';
  tableName: string;
  database: string;
  tableSchema: string | null;
  subTab: SubTabId;
  structureEditing?: boolean;
  targetColumn?: string;
  /** 新增：本标签页是经外键跳转打开的（面包屑与键盘返回的唯一依据）。 */
  fkOrigin?: FkOrigin;
}
```

> **为什么不在 `TablePanel` 上放 `initialFilters`**：筛选的**唯一真相**是 `useTableDataStore.byPanel` 里的 `TableState.filters`（01 与 02/03 的重新取数纪律都建立在它之上）。把首帧筛选同时挂在面板对象上会产生第二真相：用户 Clear 之后 `panel.fkOrigin` 与 `panel.initialFilters` 仍在，任何"重新挂载即重放"的代码都会把筛选又加回来。因此跳转的筛选**只写进 tableData 切片**（见 3.7），`fkOrigin` 只放导航信息。

### 3.7 TS：首帧预置（`src/stores/tableDataStore.ts` 增量，**新增动作**）

```ts
/** 为刚创建、还没有任何切片的 panel 写入首帧筛选；**不触发取数**（由 TableView 的首次 load 负责）。 */
seedPanel: (
  panelId: string,
  context: TableChangeContext,
  seed: {
    filters: FilterCondition[];
    filterLogic?: 'and' | 'or';
    /** 默认 true：把条件同时放进 draft，使筛选条可见。 */
    openFilterPanel?: boolean;
  },
) => void;
```

**为什么必须是新动作、不能复用 `setFilters`**（已核实）：`setFilters` 走 `patchPanelForReload` 后**立即 `reloadPanel`**，而 `patchPanel` 对**没有切片的 panel 是 no-op** ⇒ "先建面板 → `setFilters`" 这条组合在切片尚未创建时**静默失效**，面板随后以无筛选状态首帧加载，用户会看到"点跳转后闪一下全表"。`seedPanel` 用 `emptyTableState(context)` 建切片并把 `filters` / `draftFilters` / `filterLogic` / `draftFilterLogic` / `filterPanelOpen` 一次写好，**不 bump `requestRevision`、不调 `reloadPanel`**；随后 `TableView` 既有的 `needsLoad` 首次 `loadTableData` 就会带着筛选取数——**恰好一次取数**。

### 3.8 TS：跳转编排（`src/lib/fkNavigation.ts` 的副作用层，或 `src/lib/fkNavigationActions.ts`，**新增**）

```ts
export interface FkJumpOutcome {
  ok: boolean;
  /** ok=false 时的可读原因（已含 grid.fk.* 前缀经 classifyGridError 归一化的结果）。 */
  error?: { code: GridErrorCode | 'unknown'; raw: string };
  /** ok=true 且目标行不存在时置 true（标签已打开，附提示）。 */
  targetMissing?: boolean;
}

/** 唯一允许的跳转入口：解析目标 → 新建标签 → 预置切片 → 记录返回栈 → 激活。 */
export async function openForeignKeyTarget(input: {
  sourcePanelId: string;
  sourceTable: string;
  dbSessionId: string;
  database: string;
  schema: string | null;
  connectionId: string | null;
  driverType: string | null;
  foreignKey: ForeignKeyInfo;
  values: Value[];
}): Promise<FkJumpOutcome>;
```

**该函数的固定顺序（任何一步换序都会产生可复现的缺陷）**：

1. `classifyGridError` 之前先做**前端前置校验**：`values` 长度必须等于 `foreignKey.columns`，且不含 `null`；否则直接返回 `ok:false`（不发 IPC）；
2. `databaseCommands.resolveForeignKeyTargets(...)`；
3. 失败 ⇒ 返回 `ok:false` + `{ code: classifyGridError(message), raw: message }`，**不打开任何标签**；
4. 成功 ⇒ 计算 `navKey`；若 `findEntryIdByNavKey(navKey)` 命中 ⇒ `rewindTo(entryId)` + `setActivePanel(既有 panelId)` + 记录当前视口 + 返回 `ok:true`（**循环引用路径，不新建标签**）；
5. 否则 `nextPanelId('tbl')` 构造 `TablePanel { type:'table', tableName: target.table, database: target.database, tableSchema: target.schema, subTab:'data', fkOrigin }` → `addPanel(panel)`；
6. **紧接着、同一个事件回合内** `useTableDataStore.getState().seedPanel(panel.id, context, { filters: buildFkFilters(target, values) })`（必须早于 React 提交渲染，否则首帧无筛选）；
7. `push(entry)`；记录源标签页视口（`recordViewport(sourcePanelId, snapshot)`）；
8. 返回 `ok:true, targetMissing: !resolution.exists`。

### 3.9 返回栈与视口恢复的数据流

```
GraphCell 箭头 onClick
  └─ TableView.handleFkJump(rowIndex, columnName)
       ├─ 从 ts.rows[rowIndex] 取该 FK 组全部本地列的值（不是从 rowArrays 反查）
       ├─ openForeignKeyTarget(...)
       └─ 失败 ⇒ setFkError({code, raw})（源标签页 inline 错误，不跳转）

返回：useFkNavigationStore.goBack(existingPanelIds)
  ├─ 需要时先把**当前**面板视口写回（离开前快照）
  ├─ 取上一条 entry → setActivePanel(entry.panelId)
  └─ 目标面板挂载后：takeViewport(panelId) → 恢复 scrollTop / 锚点可见性
```

---

## 4. 交互与状态机

### 4.1 箭头可见性状态机（每格独立判定）

| 状态 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `hiddenNotFk` | 该列名不在 `indexForeignKeys()` 里；或该列**不是**组内第一个可见成员列 | 无任何 FK 视觉；`data-dt-fk` **不输出** | 列显隐变化使该列成为首个可见成员 ⇒ `visible` |
| `hiddenNull` | 该列属于某 FK 组，但组内**任一**本地列在该行的值为 `null`/`undefined` | 无箭头（PRD「空值单元格不显示箭头」；复合键按整组降级）；`data-dt-fk="true"` 仍输出（供测试与右键菜单识别） | 该行值被编辑为齐全 ⇒ `visible`；再次变 NULL ⇒ 回到本状态 |
| `hiddenEditing` | `editing === true`（该格正被编辑器占用） | 不画箭头（避免与编辑器叠层争抢点击） | 提交/取消 ⇒ 按值回到 `visible` 或 `hiddenNull` |
| `hiddenResolving` | 同一 `navKey` 的请求在飞 | 箭头替换为 spinner，`disabled`；同 key 的重复点击被去重 | 请求结束 ⇒ `visible` 或 `refused` |
| `visible` | 列属于某 FK 组、是该组首个可见成员、组内全部值非 null、未在编辑、无在飞请求 | 单元格显示箭头按钮；`title` = `fkNav.jumpTo` 插值 `{target}`；`aria-label` 同文案；`data-dt-fk="true"` | 点击 ⇒ `resolving`；值变 NULL ⇒ `hiddenNull`；进入编辑 ⇒ `hiddenEditing` |
| `refused`（源标签页级） | 解析失败（`grid.fk.*`） | 箭头恢复 `visible`；源标签页顶部显示 inline 错误（`CopyableError`），文案 = 分类结论文案 + 原始消息（**未命中前缀时只显示原文**，契约 §8.1） | 用户关闭该错误块 / 重试点击 ⇒ 清空错误 |

**箭头的落点规则（复合外键的唯一定位法则）**

- 组的主键是 `ForeignKeyInfo.columns` 的**有序**列表；
- 箭头只画在**组内第一个可见成员列**（`visibleColumns` 过滤后的显示顺序，不是字典序、不是 `TableData.columns` 的原始下标）；
- 组内其余成员列不画箭头，但输出 `data-dt-fk="true"` 与同一 `title`（悬停任一成员都能知道这是一组外键）；
- 首列被隐藏时箭头自动"移动"到下一个可见成员，**不需要**用户额外操作，也**不允许**因此丢功能。

### 4.2 跳转动作状态机

| 状态 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `idle` | 标签页初始 | 无面包屑（源标签页） | 发生跳转 ⇒ 目标标签页进入 `arrived` |
| `resolving` | 点击箭头 / 快捷键触发跳转 | `resolve_foreign_key_targets` 在飞；源标签页箭头转 spinner | 成功 ⇒ `arrived`；失败 ⇒ `idle` + inline 错误 |
| `arrived` | 目标标签页被 `addPanel` + `activate` | 顶部面包屑「来自 `orders.customer_id → customers.id`」+「返回来源」按钮；筛选条**可见地**列出 1..n 条 `被引用列 = 值`（`op:'eq'`，逻辑 `and`）；`exists === false` 时额外显示 `fkNav.targetMissing` 提示条；`unique === false` 时显示 `fkNav.notUniqueWarning` | 「返回来源」/`⌘/Ctrl+⌥+←` ⇒ `back`；用户 Clear 筛选 ⇒ 仍在 `arrived`（面包屑不消失，返回仍然可用）；关闭标签 ⇒ 栈条目被剪枝 |
| `back` | 返回触发 | 激活来源标签页；恢复其视口（见 4.4）；当前（目标）标签页的视口被记录 | `⌘/Ctrl+⌥+→` ⇒ `forward`；点箭头再次跳转 ⇒ 重新 `arrived` |
| `cycle` | 跳转的目标 `navKey` 已在返回栈里（A→B→A） | **不新建标签**：激活既有面板；`back` 裁剪到该条目（中间条目从栈里移除，但其标签保持打开）；`forward` 清空 | 返回 ⇒ `back` |

**键盘（PRD 要求的前后导航）**：`⌘/Ctrl+⌥+←` = 返回，`⌘/Ctrl+⌥+→` = 前进。绑定在 `TableView` 根 div 的 `onKeyDown` 上（**不注册全局 window 监听**，避免与 SQL 编辑器/输入框抢键），与既有 `Cmd/Ctrl+Enter`（提交暂存改动）共存：

- 进入条件：事件目标在网格容器内、未处于编辑器内（复用 01 的 `isEditableTarget`）、`isComposingEvent(event) === false`；
- 状态内行为：调用 `goBack(existingPanelIds)` / `goForward(...)`；栈空时 **不 `preventDefault` 也不报错**（静默无操作，避免吞掉系统/输入的组合键）；
- 退出：任何一次 `Escape` 不改变导航栈（返回栈的清空只由 `reset()` 与剪枝完成）。

**IME 与焦点**

- `onKeyDown` 必须先过 `isComposingEvent`（01 新增于 `src/lib/gridEventGuards.ts`）；中文输入法组合期间的 `⌥←` 不得触发返回；
- 箭头是原生 `<button type="button">`：Tab 可聚焦、Enter/Space 触发 `click`（原生语义，无需额外键盘代码）；
- 箭头的 `onClick` **必须** `stopPropagation()`：否则会冒泡到行 div 的 `onClick`（整行选择）并可能与 `onDoubleClick`（进入编辑）组合出"跳转 + 进入编辑"的双动作；
- 箭头 `onMouseDown` **不** `preventDefault`（与工具栏按钮相反：工具栏按钮 `preventDefault` 是为了不夺走编辑器焦点，而箭头本身就要求成为焦点目标）。

### 4.3 三要素自查（每个状态都可由状态迁移到达并离开）

上表两处 `hidden*` 状态都不是终点：`hiddenNull` / `hiddenEditing` / `hiddenResolving` 均有明确的离开条件；`refused` 有"关闭错误块"与"重试"两条出口；`cycle` 有 `back` 与再次跳转两条出口。**没有**任何"进入了就再也回不去"的分支。

### 4.4 返回时"原表的选择与滚动是否恢复"——明确结论

**事实（已核实）**：`ContentView` 只渲染 `activePanel` 的内容，因此切换标签会**卸载**非活动面板的 `TableView` 子树。后果分项：

| 状态 | 卸载后是否存活 | 本册结论 |
| --- | --- | --- |
| 滚动位置 `scrollTop` | **丢失**（滚动容器 DOM 被销毁，`DataTable` 内的 `scrollEl` 是组件 state） | **必须显式恢复**：跳转时把 `{scrollTop, anchorRow, anchorColumn}` 写进返回栈条目，返回时取回 |
| `gridSelection`（01 新增） | 存在 `useTableDataStore` 切片里，按 `panelId` 记账 ⇒ **存活** | 天然恢复，**不重算**（禁止从 DOM 反推，契约 §4.3） |
| `selectedRows` / `lastSelectedIndex` | 同上（在切片里）⇒ **存活** | 天然恢复；本册**不**改动它们的语义 |
| `editingCell` | 在切片里 ⇒ 存活 | **刻意不恢复可视编辑态**：返回后不自动打开编辑器（跨标签恢复半截输入会丢草稿并可能误提交），用户双击再编辑 |
| 锚点单元格的 DOM 焦点 | **丢失** | 恢复策略：先按 `scrollTopForAnchorRow(anchorRow, rowHeight)` 把该行滚入渲染窗口，下一帧用 `document.querySelector('[data-dt-row="r"][data-dt-col="c"]')`（契约 §4.4 的属性）定位并聚焦；查不到则**只恢复滚动位置**，不报错 |

**恢复顺序必须是**：`setActivePanel` → 面板挂载 → 恢复 `scrollTop`（一次性）→ 下一帧聚焦锚点。反序会出现"先聚焦，虚拟窗口随后重排把焦点行滚走"。

---

## 5. 实现步骤

> 每一步都写清「改哪个文件、加什么符号、为什么、怎么自测」。**顺序不可交换**：Step 1~3 是后端，Step 4~8 是前端地基，Step 9~12 是 UI 接线，Step 13~15 是文案与测试。

### Step 1 · 新增 Rust 命令模块

- **文件**：`src-tauri/src/commands/fk_navigation.rs`（新增）。
- **新增符号**：`ResolveFkTargetsRequest`、`FkTargetRef`、`FkTargetResolution`、`resolve_foreign_key_targets_impl`、`#[tauri::command] async fn resolve_foreign_key_targets`。
- **实现要点**（顺序固定）：
  1. `columns.len() != values.len()` 或任一为空 ⇒ `grid.fk.shapeMismatch`；`values` 含 `Value::Null` ⇒ `grid.fk.valueNull`；
  2. 解析源表定位：`get_session` → `get_session_config` → `database` / `metadata_schema(driver, schema, embedded_schema_of(qualified_table.unwrap_or(table)), config.schema)`（与 `get_table_data_impl` 逐字同序）；
  3. `driver.get_table_schema(handle, source_table, database, schema)` 取源表 `foreign_keys`；用 `find_fk(foreign_keys, columns, constraint_name)`（与前端 `findForeignKey` 同语义的 Rust 版）匹配，失败 ⇒ `grid.fk.unknownConstraint`；再校验 `fk.columns.len() == fk.referenced_columns.len()`，否则 ⇒ `grid.fk.shapeMismatch`；
  4. 解析目标定位：先按 `(table = referenced_table, schema = 源表 schema)` 试 `get_table_schema`；失败且 `referenced_table` 含 `'.'` 时按**最后一个** `.` 拆成 `(head, tail)` 再试 `(table = tail, schema = Some(head))`——这是**驱动无关的回退阶梯**（PG 的 `referenced_table` 带 schema 前缀，MySQL/SQLite 是裸名，见 2.1），**不是**按库名分支；两次都失败 ⇒ `grid.fk.targetUnresolved`（附脱敏说明）；
  5. 计算 `unique`：`TableSchema::effective_primary_keys()` 与 `referenced_columns` **集合相等且长度相等** ⇒ true；否则遍历 `indexes`，存在 `is_unique || is_primary` 且 `columns` 与 `referenced_columns` 集合相等且长度相等 ⇒ true；
  6. 构造等值筛选（`FilterCondition { column: referenced_columns[i].clone(), operator: FilterOperator::Eq, value: values[i].clone() }`，`filter_logic = Some("and")`）；
  7. 探测存在性：`QueryExecutor::new(state.schema_cache.clone()).get_table_data(&driver, &handle, &db_session_id, &target_database, target_schema.as_deref(), &target_table, 0, 1, Some(filters.clone()), None, true, Some("and"))`——**`skip_count = true`**，即 `WHERE … LIMIT 1`，不 `COUNT(*)`；`Err` ⇒ `grid.fk.probeFailed`；
  8. 候选值：`limit = request.limit.unwrap_or(50).clamp(1, 200)`，同一次调用改为 `page_size = limit`、`skip_count = true`；`truncated = rows.len() as u32 == limit`（多取一条更准：用 `limit + 1` 探测后再截断）；
  9. `exists = probe_rows.is_empty() == false`。
- **零硬编码/纪律**：不做任何库名/驱动 id 判断；生产路径**不使用** `unwrap()` / `expect()`，一律 `ok_or_else` / `map_err`；错误文本不回显列值。
- **自测**：`cargo test -p datazen --lib commands::fk_navigation`（Step 3 写用例）。

### Step 2 · 注册命令与复用私有 helper

- **文件**：`src-tauri/src/commands/fk_navigation.rs`（本文件内 `#[tauri::command]`）、`src-tauri/src/commands/mod.rs`（`pub mod fk_navigation;` + `pub use fk_navigation::*;`）、`src-tauri/src/bootstrap/run.rs`（`tauri::generate_handler![…]` 增加 `crate::commands::resolve_foreign_key_targets`）、`src-tauri/src/commands/schema.rs`（把 `embedded_schema_of` 从 `fn` 提升为 `pub(crate) fn`，**只加可见性、不改实现**）。
- **为什么**：`generate_handler!` 是显式清单（已核实），漏登记 ⇒ 前端 `invoke` 直接 `command not found`，且不会有编译错误；`embedded_schema_of` 的语义已经过测试，复制一份必然漂移。
- **自测**：`cargo build -p datazen` 通过；`cargo test -p datazen --lib bootstrap` （既有 registration-contract 测试会检查 handler 清单形态）；前端 `invoke` 冒烟放到 Step 14 的 journey 测试里（用 mock 的 invoke 层断言命令名逐字为 `resolve_foreign_key_targets`）。

### Step 3 · Rust 单测

- **文件**：`src-tauri/src/commands/fk_navigation/tests.rs`（新增）+ 在 `fk_navigation.rs` 末尾加 `#[cfg(test)] mod tests;`（与 `schema.rs` 的既有范式一致）。
- **新增符号**：`use super::*; use crate::testing::app_state::TestAppState; use crate::testing::mock_driver::MockDriverOptions;` + 第 9 节的用例。
- **怎么造复合外键**：`MockDriverOptions { table_schema: Some(TableSchema { foreign_keys: vec![ForeignKeyInfo { columns: vec!["a","b"], referenced_table: "parents", referenced_columns: vec!["x","y"], .. }], indexes: vec![IndexInfo { is_unique: true, columns: vec!["x","y"], .. }], .. }), query_rows: vec![…], ..Default::default() }`——`MockDriver::get_table_schema` 会把 `opts.table_schema` 原样返回（已核实），**无需改 `driver-api`**。
- **自测**：`cargo test -p datazen --lib commands::fk_navigation` 全绿。

### Step 4 · 前端 IPC 与 DTO

- **文件**：`src/types/connection.ts`（新增 `FkTargetRef` / `FkTargetResolution` / `ResolveFkTargetsParams`）、`src/commands/database.ts`（新增 `resolveForeignKeyTargets`）。
- **为什么**：DTO 与 `ForeignKeyInfo` 同域，放在 `connection.ts` 避免新建类型文件；IPC 薄封装必须与 Rust 的 `{ request }` 包体形状一致（`invoke(name, { request: params })`）。
- **自测**：`pnpm typecheck` 干净；`src/commands/__tests__/` 里若有 IPC 名称断言范式则同步（无则跳过）。

### Step 5 · 纯逻辑 `src/lib/fkNavigation.ts`

- **文件**：`src/lib/fkNavigation.ts`（新增）。
- **新增符号**：3.4 全部（`FK_NAV_MAX_DEPTH`、`GridViewport`、`FkGroupAffordance`、`indexForeignKeys`、`isFkCellNavigable`、`buildFkFilters`、`fkNavKey`、`findForeignKey`、`scrollTopForAnchorRow`、`FkBreadcrumbModel`）。
- **为什么**：交互逻辑必须能脱离 React 单测（AGENTS.md「连续旅程测试」与 00 §4.1 的拆分纪律）；把它与 React 接线分开，也让 07 与 01 的 `gridSelection.ts`（纯逻辑）职责边界一致。
- **自测**：`npx vitest run src/lib/__tests__/fkNavigation.test.ts`（Step 14 写用例），先跑绿"复合键位置配对""含 NULL 返回 null""首列隐藏时箭头移动"三例。

### Step 6 · 返回栈 store

- **文件**：`src/stores/fkNavigationStore.ts`（新增）。
- **新增符号**：3.5 全部（`FkNavEntry`、`FkNavigationState`、`FkNavigationActions`、`useFkNavigationStore`）。
- **为什么**：栈与视口是跨标签页状态，必须脱离任何单个组件存活；独立 store 避免 `panelStore.ts`（728 行）越线与循环依赖。
- **自测**：`npx vitest run src/stores/__tests__/fkNavigationStore.test.ts`（Step 14）。

### Step 7 · 面板载荷与首帧预置

- **文件**：`src/stores/panelTypes.ts`（新增 `FkOrigin`、`TablePanel.fkOrigin?`）、`src/stores/tableDataStore.ts`（新增 `seedPanel`，并在 `TableDataStore` 接口里声明）。
- **为什么**：跳转的筛选必须在首帧就位且**只取数一次**（3.7 已给出 `setFilters` 不可复用的实证）；`fkOrigin` 只承载导航信息，不承载筛选（3.6）。
- **自测**：`npx vitest run src/stores/__tests__/tableDataStore.fkSeed.test.ts src/stores/__tests__/panelTypes.test.ts`；断言 `seedPanel` 后 `filters`/`draftFilters` 同时有值且 `requestRevision` 未变。

### Step 8 · 外键索引 hook

- **文件**：`src/hooks/useTableForeignKeys.ts`（新增）。
- **新增符号**：`useTableForeignKeys(params: { dbSessionId: string; tableName: string; database: string; schema: string | null }): { index: ReadonlyMap<string, FkGroupAffordance>; loading: boolean; error: string | null }`。
- **实现要点**：复用 `getCachedTableSchema`（既有 FK 元数据唯一入口，与 `ForeignKeysView` 同源）；`cancelled` 守卫；`error` 只作为**降级**信息（元数据读不到 ⇒ 不显示任何箭头，**不得**让整个网格报错或白屏）；schema 变化（`tableSchema` 切换）时重新解析。
- **为什么**：`getTableData` **不返回** `foreignKeys`，箭头所需元数据必须另外取；hook 化避免 `TableView` 里再堆一段 `useEffect`。
- **自测**：`npx vitest run src/hooks/__tests__/useTableForeignKeys.test.tsx`（mock `getCachedTableSchema`）；断言失败时不抛异常、`index` 为空、`error` 非空。

### Step 9 · `GridCell` 增加箭头（01 的扩展点）

- **文件**：`src/components/DataTable/GridCell.tsx`（01 新增；本册修改）、`src/components/DataTable/VirtualBody.tsx`、`src/components/DataTable/DataTable.tsx`。
- **新增符号**：
  - `GridCellProps` 增加 `fk?: { targetLabel: string; resolving: boolean } | null`、`fkMember: boolean`、`onFkActivate?: () => void`（**全部可选**，不传 ⇒ 逐位等于 01 的行为）；
  - `VirtualBodyProps` / `VirtualRowProps` 增加 `fkIndex?: ReadonlyMap<string, FkGroupAffordance>`、`fkVisibleColumns?: readonly string[]`、`onFkJump?: (rowIndex: number, columnName: string) => void`；
  - `DataTableProps` 增加 `fkIndex?`、`fkVisibleColumns?`、`onFkJump?`（三个可选，既有 7+ 处 `DataTable` 消费者一行不改）。
- **实现要点**：`GridCell` 收到 `fk` 时在值右侧渲染 `<button type="button" data-dt-fk-click onClick={e => { e.stopPropagation(); onFkActivate?.(); }}>`；`fkMember === true` 时输出 `data-dt-fk="true"`；`resolving` 时渲染 spinner 并 `disabled`；**回调必须是 props 透传的稳定引用**（禁止在 `columns.map` 里内联箭头函数，否则 `memo(GridCell)` 全部失效，大表虚拟滚动会退化——与 01 Step 8 同一条约束）。
- **自测**：`npx vitest run src/components/DataTable/__tests__/GridCell.fkArrow.test.tsx src/components/DataTable/__tests__/VirtualBody.test.tsx src/components/DataTable/__tests__/DataTable.test.tsx`（既有断言必须保持通过：不传新 props 时 DOM 逐字不变）。

### Step 10 · `TableView` 前置抽取（**本册必须做的腾地方动作**）

- **背景（已核实）**：`TableView.tsx` 今天 **770 行**；01 计划把暂存改动条抽成 `TablePendingChangesBar.tsx`（→ ~660）。本册在此之上再抽一块，为面包屑与跳转接线腾出空间且不撞 800 行红线。
- **文件**：`src/windows/connection/TableDataToolbar.tsx`（新增）、`src/windows/connection/TableFkBreadcrumb.tsx`（新增）、`src/windows/connection/TableView.tsx`（修改）。
- **新增符号**：`TableDataToolbarProps` / `TableDataToolbar`（把现有工具栏那一块 JSX 与 `quickFilter` 状态搬过去，**`data-testid` 逐字保留**：`table-data-refresh`、`table-filter-toggle`、`table-quick-filter`、`table-read-only-tip`）；`TableFkBreadcrumbProps` / `TableFkBreadcrumb`（`{ origin: FkOrigin; truncated: boolean; canGoBack: boolean; canGoForward: boolean; onBack: () => void; onForward: () => void; onDismiss?: () => void }`）。
- **为什么**：`TableView` 是 02/03/05/06/07 共同的加线点，不先抽就只能在 800 行上限上走钢丝；抽出的两块都是**纯展示 + 回调**，不引入新状态源。
- **自测**：`npx vitest run src/windows/connection/__tests__/TableView.test.tsx`（既有 testid 断言一行不改即全绿）；`wc -l src/windows/connection/TableView.tsx` ≤ 640。

### Step 11 · `TableView` 接线（跳转 + 面包屑 + 快捷键）

- **文件**：`src/windows/connection/TableView.tsx`（修改）、`src/hooks/useFkNavigationShortcuts.ts`（新增）。
- **新增符号**：`useFkNavigationShortcuts({ enabled, onBack, onForward }): (event: React.KeyboardEvent) => void`；`TableView` 内新增 `handleFkJump(rowIndex, columnName)`、`fkError` state、`const fk = useTableForeignKeys({...})`、`const nav = useFkNavigationStore(...)`。
- **实现要点**：
  - `handleFkJump` 从 `ts.rows[rowIndex]` 按组内 `localColumns` **逐列取值**（不要从 `rowArrays` 反查，也不要从 `CellCoord.rowIndex` 拼 SQL——契约 §4.3 的纪律对读路径同样成立）；
  - 调用 `openForeignKeyTarget`；`ok:false` ⇒ `setFkError(outcome.error)`；`targetMissing` ⇒ 存一个 banner 标记；
  - 面板挂载后（`useEffect`，依赖 `panelId`）：`takeViewport(panelId)` → 恢复 `scrollTop` → 下一帧按 `data-dt-*` 聚焦锚点；离开时（`useEffect` cleanup 或跳转/返回动作内）`recordViewport(panelId, snapshot)`；
  - `onKeyDown` 链：先 `Cmd/Ctrl+Enter`（既有），再 `isEditableTarget`/`isComposingEvent` 守卫后的 `Cmd/Ctrl+Alt+←/→`；
  - 只读连接（`isConnectionReadOnly === true`）**不**影响箭头与跳转（跳转是纯读操作），只在 `editing` 相关分支上沿用既有的 `showReadOnlyTip()`。
- **自测**：`npx vitest run src/windows/connection/__tests__/TableView.test.tsx`；再跑 `pnpm e2e:minimal` 的 `table-edit.ts`（暂存改动条与事务控件**逐字**不回归）。

### Step 12 · 错误分类增量

- **文件**：`src/lib/gridErrors.ts`（03/05 建立；本册**追加**）。
- **新增内容**：`GridErrorCode` 联合类型增加 `'grid.fk.shapeMismatch' | 'grid.fk.unknownConstraint' | 'grid.fk.valueNull' | 'grid.fk.targetUnresolved' | 'grid.fk.probeFailed'`，并同步 `GRID_ERROR_CODES`；`classifyGridError` 的实现**不改**（前缀匹配天然覆盖新成员）。
- **为什么**：契约 §8.1 要求错误码是前端分类结果；前缀匹配表是唯一的码表来源，新增前缀必须进同一张表，否则前端只能显示原文（不算错，但用户看不到可读结论）。
- **自测**：`npx vitest run src/lib/__tests__/gridErrors.test.ts`（01/03/05 已有该文件；断言每个新码 `classifyGridError(code + ': 任意说明') === code`，且未知消息仍是 `'unknown'` 并保留原文）。

### Step 13 · i18n 文案

- **文件**：`src/locales/en/schema.ts`（唯一允许改的文案文件）。
- **新增 key**：见第 8 节（`fkNav.*` 命名空间，紧邻既有 `fk.*`）。
- **为什么**：FK 的目标/约束词汇已经在这个领域包里（`fk.loadFailed` / `fk.refTable` / `fk.refColumn` 已核实存在），DB-20 的选择器也会落在这里；**不**新建域名、**不**改 `src/locales/en.ts`（它只是 `export { default } from './en/index'`，改它无效）、**不**动其他语言。
- **自测**：`npx vitest run src/locales/locales.test.ts`；`pnpm typecheck` 干净（`I18nKey` 允许 en-only 新 key）。

### Step 14 · 前端测试四类

- **文件**：见第 9 节清单。
- **自测**：逐条跑第 9 节的命令；**连续旅程测试必须覆盖残缺中间态**（在飞请求、值在半途被改成 NULL、组合输入法状态下按返回键）。

### Step 15 · E2E 与种子数据

- **文件**：`e2e/specs/table-fk-navigation.ts`（新增）、`e2e/` 既有种子脚本族（新增"父表/子表 + 复合外键"种子，沿用既有 sqlite 建库脚本与 demo 数据脚本的范式，见 2.2/2.5 之外的实际脚本：`e2e/create-sqlite-test-db.mjs`、`e2e/setup-demo-data.sh`）。
- **为什么**：PRD 明确要求 E2E 含**复合 FK 种子数据**；没有复合键种子，本册最核心的规则（整组定位）在 E2E 层等于没测。
- **自测**：`pnpm e2e:minimal`（构建方式必须走 `pnpm tauri:build:webdriver` / `pnpm e2e`，**禁止**裸 `cargo build`）。

---

## 6. 文件级改动清单

| 文件 | 新增/修改 | 职责 | 预估行数 | 是否触及 800 行上限 |
| --- | --- | --- | --- | --- |
| `src-tauri/src/commands/fk_navigation.rs` | **新增** | 新命令 DTO + `resolve_foreign_key_targets_impl` + handler | ~240 | 否 |
| `src-tauri/src/commands/fk_navigation/tests.rs` | **新增** | Rust 单测（复合键定位、探测、上限、错误前缀） | ~260 | 否 |
| `src-tauri/src/commands/mod.rs` | 修改 | 登记模块与再导出 | +2 | 否 |
| `src-tauri/src/commands/schema.rs` | 修改 | `embedded_schema_of` 提为 `pub(crate)`（仅可见性） | 0（385 → 385） | 否 |
| `src-tauri/src/bootstrap/run.rs` | 修改 | `generate_handler!` 登记新命令 | +1 | 否 |
| `src/lib/fkNavigation.ts` | **新增** | 纯逻辑：FK 索引、可见性、条件构造、navKey、视口换算 | ~200 | 否 |
| `src/lib/fkNavigationActions.ts` | **新增** | `openForeignKeyTarget` 编排（跨 store，唯一跳转入口） | ~150 | 否 |
| `src/stores/fkNavigationStore.ts` | **新增** | 返回/前进栈 + 视口快照 + 剪枝 + 循环引用裁剪 | ~180 | 否 |
| `src/hooks/useTableForeignKeys.ts` | **新增** | 按表加载 FK 元数据并摊平成列索引 | ~85 | 否 |
| `src/hooks/useFkNavigationShortcuts.ts` | **新增** | `⌘/Ctrl+⌥+←/→` 守卫与派发 | ~70 | 否 |
| `src/windows/connection/TableFkBreadcrumb.tsx` | **新增** | 面包屑 + 返回/前进按钮 + 截断标记 | ~95 | 否 |
| `src/windows/connection/TableDataToolbar.tsx` | **新增** | 从 `TableView` 抽出的工具栏（刷新/筛选/快速筛选/只读提示），testid 逐字保留 | ~155 | 否 |
| `src/components/DataTable/GridCell.tsx` | 修改（01 新增） | 增加 `fk` / `fkMember` / `onFkActivate` 与 `data-dt-fk` | +30（~90 → ~120） | 否 |
| `src/components/DataTable/VirtualBody.tsx` | 修改（01 已改） | 透传 FK 索引与跳转回调 | +18 | 否 |
| `src/components/DataTable/DataTable.tsx` | 修改（01 已拆分） | 3 个可选 props 透传 | +15（须保持 ≤ 470 的自约束，01 后约 450 → ~465） | 否 |
| `src/windows/connection/TableView.tsx` | 修改 | 抽取工具栏后接线：FK hook、跳转、面包屑、快捷键、视口恢复 | 770 → ~660（01）→ ~640 | 否（**必须依赖 Step 10 抽取**） |
| `src/stores/panelTypes.ts` | 修改 | `FkOrigin` + `TablePanel.fkOrigin?` | +22（202 → ~224） | 否 |
| `src/stores/tableDataStore.ts` | 修改 | `seedPanel` + 接口声明 | +28（01 后约 700 → ~728） | **接近**（01 要求 ≤ 700；本册加 28 后需在 02/03 之前复核，见 U-4） |
| `src/types/connection.ts` | 修改 | 3 个 DTO | +42（250 → ~292） | 否 |
| `src/commands/database.ts` | 修改 | `resolveForeignKeyTargets` 薄封装 | +6（161 → ~167） | 否 |
| `src/lib/gridErrors.ts` | 修改（03/05 新增） | `GridErrorCode` + 码表追加 5 个 `grid.fk.*` | +10（~60 → ~70） | 否 |
| `src/locales/en/schema.ts` | 修改 | 追加 `fkNav.*`（第 8 节） | +18（306 → ~324） | 否 |
| `src/lib/__tests__/fkNavigation.test.ts` | **新增** | 纯逻辑单测 | ~230 | 否 |
| `src/lib/__tests__/fkNavigationActions.test.ts` | **新增** | 编排测试（mock IPC + 两个 store） | ~200 | 否 |
| `src/stores/__tests__/fkNavigationStore.test.ts` | **新增** | 栈 / 剪枝 / 循环引用 / 视口单测 | ~210 | 否 |
| `src/stores/__tests__/tableDataStore.fkSeed.test.ts` | **新增** | `seedPanel` 首帧语义与"只取数一次" | ~120 | 否 |
| `src/hooks/__tests__/useTableForeignKeys.test.tsx` | **新增** | 元数据加载与失败降级 | ~90 | 否 |
| `src/components/DataTable/__tests__/GridCell.fkArrow.test.tsx` | **新增** | 箭头可见性与 DOM 属性 | ~130 | 否 |
| `src/components/DataTable/__tests__/fkNavigation.journey.test.tsx` | **新增** | 连续旅程测试（三组必测） | ~300 | 否 |
| `e2e/specs/table-fk-navigation.ts` | **新增** | 单列 / 复合 / 返回 / 循环 / NULL / 无权限 | ~260 | 否 |
| `e2e/create-sqlite-test-db.mjs` 或 `e2e/setup-demo-data.sh` | 修改 | 追加"父表 / 子表 + 单列 FK + 复合 FK"种子 | +30 | 否 |

**需要同步修改的既有测试**：目标为**零**。

- 新增 `DataTableProps` / `GridCellProps` / `VirtualBodyProps` 字段全部**可选**，不传时 DOM 与行为逐字不变（`GridCell` 的既有断言、`DataTable.test.tsx`、`VirtualBody.test.tsx`、`table-edit.ts` 都不应改动）；
- `TablePanel` 只加可选字段，`panelTypes.test.ts` 的既有断言是逐字段的，不应破；
- 若出现"必须改既有断言才能通过"的情况，说明本册破坏了兼容性，**先改设计**而不是改断言。

---

## 7. 边界与异常清单

| # | 场景 | 期望行为 |
| --- | --- | --- |
| 1 | **单列外键**（`columns = ["customer_id"]`，`referenced_columns = ["id"]`） | 箭头只出现在 `customer_id` 列的有值单元格；点击后新标签页顶部筛选条可见地出现 1 条 `id = <值>`，面包屑 `orders.customer_id → customers.id`；`unique === true`（命中主键）时无额外提示 |
| 2 | **复合外键**（`columns = ["a","b"]` → `referenced_columns = ["x","y"]`） | 箭头只画在 `a`（组内首个可见成员）；点击后筛选条出现**两条**条件 `x = a值` **AND** `y = b值`；**禁止**只取第一列；配对严格按位置（`columns[i] ↔ referenced_columns[i]`）；两侧长度不等时前端直接拒绝（不发 IPC），后端也会以 `grid.fk.shapeMismatch` 兜底 |
| 3 | **复合键部分列为 NULL**（`a` 有值、`b` 为 NULL） | **不显示箭头**（组内任一列为 NULL ⇒ 整组不可跳）；不产生任何请求；**禁止**退化成 `b IS NULL` 或只按 `a` 跳转（都会给出误导性的多行结果）；用户补齐 `b` 后箭头自动出现 |
| 4 | **被引用行不存在** | 命令正常返回 `exists: false`（**不是错误**）；目标标签页照常打开、筛选照常可见，顶部显示 `fkNav.targetMissing` 提示条；**禁止**白屏、禁止静默不跳转、禁止把这件事做成 `grid.fk.*` 错误 |
| 5 | **循环引用 A→B→A** | 第二次跳转的 `navKey` 已在返回栈里 ⇒ 走 `cycle`：激活**已存在的** A 标签、`back` 裁剪到该条目、清空 `forward`；**不**新建第三个标签；连续往返 N 次后标签数恒为 2、`back` 长度恒为 1；`FK_NAV_MAX_DEPTH` 只是第二道保险 |
| 6 | **跨 schema**（PG 的 `referenced_table` = `"sales.customers"`） | 宿主按回退阶梯解析：先按 `(referenced_table, 源表 schema)` 试，失败且含 `.` 时按最后一个 `.` 拆为 `(tail, head)` 再试；成功 ⇒ 目标标签页 `tableSchema = "sales"`、面包屑显示 `sales.customers.id`；两次都失败 ⇒ `grid.fk.targetUnresolved`；**前端不得**自己拆字符串 |
| 7 | **跨库**（MySQL `REFERENCES otherdb.parents`） | 今天**无法表达**（`ForeignKeyInfo` 没有 referenced database 字段，宿主只能用源表所在的库解析），因此一律以 `grid.fk.targetUnresolved` 拒绝，并提示"目标表不在当前库；请在对象树中打开目标表"；**不得**静默按同名表跳转（跳错库比拒绝更糟）；扩展见 U-1 |
| 8 | **目标表无权限**（元数据可见、行不可读） | 元数据阶段通过 ⇒ 探测阶段失败 ⇒ `grid.fk.probeFailed`，源标签页 inline 错误显示分类结论 + 脱敏原文；**不打开**目标标签页（避免打开一个必然报错的空壳）；提示里给出"该表可能无读取权限"的通用说明，**不**按驱动/库名硬编码权限判定 |
| 9 | **目标表已删除**（元数据也读不到） | `grid.fk.targetUnresolved`；源标签页不跳转、显示错误；同时 `invalidateSchemaCache(dbSessionId, tableName)` 的既有失效链路负责让下一次 FK 元数据重新解析（本册只调用既有函数，不自建缓存） |
| 10 | **只读连接**（`isConnectionReadOnly === true`） | 箭头、跳转、返回、前后导航**全部可用**（纯读）；不弹 `tableData.readOnlyEditDisabled`；只有"进入单元格编辑"的分支保持今天的禁用语义与提示 |
| 11 | **同名不同 schema**（`public.orders` 与 `audit.orders` 同时打开） | `fkNavKey` 含 `schema`，两者是不同节点，不会误判为循环引用；目标标签页各自绑定 `tableSchema`，两个标签互不覆盖（`PanelContentRenderer` 已按 `panel.tableSchema` 传参） |
| 12 | **被引用列不是主键/唯一键** | 跳转**照常允许**（结果可能多行）：`unique === false` ⇒ 目标标签页显示 `fkNav.notUniqueWarning`；**不**自动选中第一行、**不**自动打开详情；DB-20 的选择器在此情形下应禁用（本册只提供判定字段） |
| 13 | **空表 / 单行 / 单列** | 空表无单元格 ⇒ 无箭头、无跳转；单行时跳转/返回照常；单列时若该列就是 FK 成员则箭头照常（组内首个可见成员即该列） |
| 14 | **列被隐藏（FK 首列被隐藏）** | 箭头自动落到组内下一个可见成员列；`visibleColumns` 为空（全部隐藏）时没有箭头，且**不得**报错 |
| 15 | **超长值 / 多字节值** | 值只作为`FilterCondition.value` 传递，不参与任何字符串拼接；面包屑与提示中的表名/列名按原样渲染；`fkNavKey` 用 `JSON.stringify` 而非字符串拼接，避免分隔符碰撞 |
| 16 | **二进制 / 时间戳作为外键值** | 原样透传（`Value` 既有枚举），不做前端类型归一化；后端 `format_sql_literal` 负责字面量形态（已由驱动实现） |
| 17 | **同一 FK 组在源表出现两次约束**（同列不同 `name`） | 有 `constraintName` 时按名字取；没有时按有序 `columns` 取**第一个**匹配，并在 dev 日志记录歧义（生产路径不 panic、不阻断） |
| 18 | **元数据加载失败**（`getCachedTableSchema` 抛错） | 不显示任何箭头（降级），网格其余功能不受影响；错误只记在 `useTableForeignKeys` 的 `error` 上，**不**渲染成全屏错误 |
| 19 | **目标标签页在飞请求期间用户再次跳转** | 同一 `navKey` 的重复请求被去重；不同 `navKey` 允许并发，但每一条各自记栈，栈上限保护生效 |
| 20 | **目标标签页被用户手动关闭后再按返回** | `prune(existingPanelIds)` 已丢弃该条目 ⇒ 返回操作跳到下一个仍然存在的条目；栈空则静默无操作 |
| 21 | **`⌘/Ctrl+⌥+←` 在编辑器内 / IME 组合中** | 被 `isEditableTarget` / `isComposingEvent` 拦下，不改变导航栈、不 `preventDefault` |
| 22 | **返回时锚点行已不在当前页**（用户跳转后目标页翻过页，返回的是**来源**页，来源页未翻页所以不适用；但目标页在前进时可能已翻页） | 恢复 `scrollTop` 时把 `anchorRow` 夹到 `[0, rows.length - 1]`；夹不住时只恢复 `scrollTop`，不聚焦、不报错 |
| 23 | **键集分页（DB-18）落地后** | 跳转仍然只注入结构化筛选并走既有分页取数；本册**不**假设 offset 分页存在（不拼 `LIMIT/OFFSET`、不拼 SQL） |

---

## 8. i18n key 清单

**唯一落点（已核实）**：`src/locales/en/schema.ts`。

**为什么是 `schema.ts` 而不是 `query.ts`**：`schema.ts` 里已有 FK 词汇领域块（`fk.loadFailed`、`fk.loading`、`fk.noForeignKeys`、`fk.constraintName`、`fk.localColumn`、`fk.refTable`、`fk.refColumn`），DB-20 的 FK 选择器也会落在这里；`query.ts` 承载的是 `tableView.*` / `dataTable.*` / `filter.*`（网格通用交互）。外键跳转的文案属于 FK 语义，放 `schema.ts` 的 `fkNav.*` 子命名空间，**不新建域名**、**不改** `src/locales/en.ts`（它只有一行再导出）、**不动**其他语言文件。

| 完整 key 路径 | 文案（en） | 用途 |
| --- | --- | --- |
| `schema.ts → fkNav.jumpTo` | `Jump to {target}` | 箭头 `title` / `aria-label`；`{target}` 形如 `customers.id` 或 `sales.parents.(x, y)` |
| `schema.ts → fkNav.jumpToComposite` | `Jump to {table} ({columns})` | 复合键的 tooltip 变体（与上一条二选一，由 `FkBreadcrumbModel` 决定） |
| `schema.ts → fkNav.resolving` | `Resolving foreign key…` | 请求在飞时的 `title` 与 `aria-live` 文本 |
| `schema.ts → fkNav.breadcrumbFrom` | `From {source}` | 面包屑前缀；`{source}` = `orders.customer_id → customers.id` |
| `schema.ts → fkNav.backToSource` | `Back to {table}` | 「返回来源」按钮文案 |
| `schema.ts → fkNav.back` | `Back` | 快捷键提示（`⌘/Ctrl+⌥+←`）与前进按钮的 `aria-label` |
| `schema.ts → fkNav.forward` | `Forward` | 前进按钮 `aria-label` |
| `schema.ts → fkNav.stackTruncated` | `Earlier navigation steps were dropped` | `truncated === true` 时的面包屑提示 |
| `schema.ts → fkNav.targetMissing` | `The referenced row no longer exists` | `exists === false` 的目标标签页提示条 |
| `schema.ts → fkNav.notUniqueWarning` | `{columns} is not a primary or unique key; the jump may return many rows` | `unique === false` 的信息条 |
| `schema.ts → fkNav.error.shapeMismatch` | `This foreign key's columns do not line up` | `grid.fk.shapeMismatch` 的分类文案 |
| `schema.ts → fkNav.error.unknownConstraint` | `The foreign key changed; refresh the table structure` | `grid.fk.unknownConstraint` |
| `schema.ts → fkNav.error.valueNull` | `Part of the composite key is NULL` | `grid.fk.valueNull` |
| `schema.ts → fkNav.error.targetUnresolved` | `Cannot reach {target}; it may be missing, in another database, or not readable` | `grid.fk.targetUnresolved` |
| `schema.ts → fkNav.error.probeFailed` | `The target table was found but could not be read` | `grid.fk.probeFailed` |
| `schema.ts → fkNav.error.unknown` | `Foreign key navigation failed` | 未命中任何前缀时的兜底标题（**正文仍显示原始消息**，不得吞掉） |

**复用而不新增**：`fk.constraintName`、`fk.refTable`、`fk.refColumn`（面包屑的明细行如需表格化展示时复用）；`common.retry`（失败重试按钮）。

**错误渲染纪律**：先 `classifyGridError(message)`；命中 ⇒ 显示「分类文案 + 原始消息」，未命中 ⇒ **只显示原始消息**（契约 §8.1，未知前缀不得吞掉）。

---

## 9. 测试清单

### 9.1 Rust 单测（`src-tauri/src/commands/fk_navigation/tests.rs`，**新增**）

命令：`cargo test -p datazen --lib commands::fk_navigation`

| 用例名 | 断言要点 |
| --- | --- |
| `resolve_fk_targets_single_column_maps_values_to_referenced_columns` | 单列 FK：返回 `target.columns == ["id"]`、`candidates` 行来自 mock 的 `query_rows`、`exists == true` |
| `resolve_fk_targets_composite_pairs_columns_positionally` | 复合 FK `["a","b"] → ["x","y"]`：`target.columns` 顺序为 `["x","y"]`、`dataTypes` 与之一一对应；探测用的条件列顺序为 `x,y`（用 mock 的 `query_rows` 与结果行列数间接断言） |
| `resolve_fk_targets_rejects_length_mismatch_with_shape_mismatch_prefix` | `columns.len() != values.len()` ⇒ `Err`，`to_string()` **以** `grid.fk.shapeMismatch` 开头 |
| `resolve_fk_targets_rejects_null_component_with_value_null_prefix` | `values` 含 `Value::Null` ⇒ `grid.fk.valueNull` 前缀 |
| `resolve_fk_targets_unknown_constraint_prefix` | 源表 `foreign_keys` 为空 ⇒ `grid.fk.unknownConstraint` 前缀 |
| `resolve_fk_targets_missing_target_row_is_not_an_error` | `query_rows = vec![]` ⇒ `Ok(..)`、`exists == false`、`candidates` 为空 |
| `resolve_fk_targets_unresolved_metadata_uses_target_unresolved_prefix` | 目标 `get_table_schema` 失败（用 `columns_by_database` 缺失 + 空 `table_schema` 或 mock 的错误注入）⇒ `grid.fk.targetUnresolved` 前缀，且**不含**列值 |
| `resolve_fk_targets_probe_failure_uses_probe_failed_prefix` | `query_error: Some(..)` ⇒ `grid.fk.probeFailed` 前缀（元数据阶段已通过） |
| `resolve_fk_targets_flags_non_unique_referenced_columns` | 无主键、无覆盖唯一索引 ⇒ `target.unique == false`；唯一索引恰好覆盖 ⇒ `true`；唯一索引覆盖 `["x","y","z"]` 而引用 `["x","y"]` ⇒ `false` |
| `resolve_fk_targets_caps_candidates_and_sets_truncated` | `limit = 1_000` ⇒ `target.candidateLimit == 200`；行数等于上限 ⇒ `truncated == true` |
| `resolve_fk_targets_probe_uses_limit_without_count` | 探测路径不产生 COUNT：断言 mock 的 `count_total` 未被消费（或以 `skip_count` 语义断言返回 `total_rows == None`） |
| `resolve_fk_targets_qualified_referenced_table_falls_back_to_split_schema` | `referenced_table = "sales.parents"` 且按裸表名解析失败 ⇒ 走拆分回退并命中 `schema = Some("sales")` |

### 9.2 前端纯逻辑单测（`src/lib/__tests__/fkNavigation.test.ts`，**新增**）

命令：`npx vitest run src/lib/__tests__/fkNavigation.test.ts`

| 用例名 | 断言要点 |
| --- | --- |
| `indexForeignKeys maps every composite member to the same group` | 成员列的每一列都命中同一 `FkGroupAffordance`，`localColumns` 顺序与 `ForeignKeyInfo.columns` 一致 |
| `isFkCellNavigable draws the arrow only on the first visible member` | 首列可见时只有首列 `true`；隐藏首列后第二个可见成员变 `true`；全隐藏时全 `false` |
| `isFkCellNavigable returns false when any composite component is null` | 组内任一列为 `null`/`undefined` ⇒ `false`；编辑器占用 ⇒ `false` |
| `buildFkFilters pairs composite columns positionally` | 结果长度 = 组长度，第 i 条 `column === target.columns[i]`、`operator === 'eq'`、`value` 逐位相等 |
| `buildFkFilters returns null for any null component or length mismatch` | 含 `null` 或长度不等 ⇒ `null`（调用方据此不跳转） |
| `fkNavKey includes schema and values` | 同表不同 schema ⇒ 不同 key；同表同 schema 不同值 ⇒ 不同 key；相同输入 ⇒ 稳定相同 |
| `findForeignKey prefers the constraint name` | 同名两约束时按 `constraintName` 命中；无名字时按有序 `columns` 精确匹配（顺序不同 ⇒ 不命中） |
| `scrollTopForAnchorRow clamps to zero` | 锚点行 0 ⇒ `0`；负值 ⇒ `0`；行 5、行高 28 ⇒ `140` |

### 9.3 编排与 store 单测

命令：`npx vitest run src/lib/__tests__/fkNavigationActions.test.ts src/stores/__tests__/fkNavigationStore.test.ts src/stores/__tests__/tableDataStore.fkSeed.test.ts src/hooks/__tests__/useTableForeignKeys.test.tsx`

| 用例名 | 断言要点 |
| --- | --- |
| `openForeignKeyTarget seeds the first frame exactly once` | `addPanel` 一次、`seedPanel` 一次、`loadTableData` **恰好一次**（IPC mock 计数为 1），且首次请求的 `filters` 已含跳转条件 |
| `openForeignKeyTarget refuses without opening a panel on ipc failure` | IPC 拒绝 ⇒ `ok:false`、`panels` 长度不变、错误 `code` 来自 `classifyGridError` |
| `openForeignKeyTarget reports targetMissing but still opens the tab` | `exists:false` ⇒ `ok:true`、`targetMissing:true`、标签已创建、`seedPanel` 已调用 |
| **`goBack restores the source viewport and selection`**（**返回栈必测组**） | push 两条后 `goBack`：`setActivePanel` 目标 = 上一条 `panelId`；`takeViewport` 返回先前 `recordViewport` 写入的同一对象；`gridSelection` 与 `selectedRows` **未被本册改写**（仍等于切片里的值） |
| `goBack at the bottom of the stack is a no-op` | 空栈 ⇒ 返回 `null`、`back`/`forward` 不变、不调用 `setActivePanel` |
| `goForward is the inverse of goBack` | back → forward → 回到原面板；两次操作后两个栈的长度互换 |
| **`revisit rewinds instead of pushing a duplicate`**（**循环引用必测组**） | A→B→A：`findEntryIdByNavKey` 命中 ⇒ `push` 未被调用、`panels` 长度仍为 2、`back` 长度裁剪为 1、`forward` 为空 |
| **`cycle repeated ten times stays bounded`**（**循环引用必测组**） | 往返 10 次后 `panels` 长度恒为 2、`back` 长度 ≤ 1、无重复 `navKey` |
| `maxDepth drops the oldest entry and sets truncated` | 压入 `FK_NAV_MAX_DEPTH + 1` 条 ⇒ `back.length === FK_NAV_MAX_DEPTH`、`truncated === true`、最旧的 entry 不在栈里 |
| `prune drops entries whose panel is gone` | 面板被移除后 `goBack` 跳过该条目 |
| `recordViewport keeps only the latest snapshot per panel` | 同 panel 写两次 ⇒ `takeViewport` 得到后者 |
| `seedPanel never bumps requestRevision and never reloads` | `requestRevision` 不变、`loadTableData` 未被调用、`filters` 与 `draftFilters` 同时有值、`filterPanelOpen === true` |
| `useTableForeignKeys degrades silently on metadata failure` | `getCachedTableSchema` 拒绝 ⇒ `index.size === 0`、`error` 非空、不抛出 |

### 9.4 组件测试

命令：`npx vitest run src/components/DataTable/__tests__/GridCell.fkArrow.test.tsx src/components/DataTable/__tests__/VirtualBody.test.tsx src/windows/connection/__tests__/TableView.test.tsx`

| 用例名 | 断言要点 |
| --- | --- |
| `renders the fk arrow only when every composite component is non-null` | 值齐全 ⇒ 箭头存在；任一列 NULL ⇒ 无箭头（`container.querySelector('[data-dt-fk-click]')` 为 null） |
| `hides the arrow for null cells and while editing` | `value === null` ⇒ 无箭头；`editing === true` ⇒ 无箭头 |
| `arrow click does not start editing and stops propagation` | `onCellDoubleClick` 未被调用、行 `onRowSelect` 未被调用、`onFkActivate` 调用一次 |
| `data-dt-fk marks every member cell of the group` | 组内两格的 `data-dt-fk` 均存在，非 FK 列不存在；不传 `fkIndex` 时该属性一律不输出 |
| `no fk props keeps the existing cell DOM byte-identical` | 快照/属性集合与 01 版本一致（回归护栏） |

### 9.5 连续旅程测试（`src/components/DataTable/__tests__/fkNavigation.journey.test.tsx`，**新增**）

命令：`npx vitest run src/components/DataTable/__tests__/fkNavigation.journey.test.tsx`

| 用例名 | 断言要点（模拟完整击键/点击序列，含残缺中间态） |
| --- | --- |
| **`journey: single-column jump opens a filtered tab and back restores scroll`** | ① 点击箭头 ⇒ 断言 IPC 名为 `resolve_foreign_key_targets` 且入参 `columns/values` 与源行一致；② 目标标签出现 ⇒ 筛选条可见 1 条 `eq` 条件；③ 面包屑文本含 `→`；④ 按 `⌘+⌥+←` ⇒ 源标签激活且 `scrollTop` 回到离开时的值（±1 行）；⑤ 再按 `⌘+⌥+→` ⇒ 回到目标标签 |
| **`journey: composite jump uses both columns and survives a mid-flight edit`**（**复合外键必测组**） | ① 复合行点击箭头 ⇒ 两条条件、`AND`；② 在飞期间把第二列改成 NULL ⇒ 断言不发起第二个请求且箭头消失；③ 请求成功后目标标签仍带两条原始值条件（不因中途编辑而篡改） |
| **`journey: A to B to A rewinds and never opens a third tab`**（**循环引用必测组**） | ① A→B；② B 里点击指回 A 的箭头 ⇒ 标签数仍为 2、激活的是原 A 标签；③ 断言 `back` 长度 1；④ 连续 3 次往返后标签数不变 |
| **`journey: escape and IME leave the stack untouched`** | ① 组合输入态（`isComposing: true`）按 `⌘+⌥+←` ⇒ 栈不变、`preventDefault` 未被调用；② 在编辑器内按同一组合键 ⇒ 栈不变 |

### 9.6 E2E（`e2e/specs/table-fk-navigation.ts`，**新增**）

前置：`pnpm e2e:minimal`（内部走 `pnpm tauri:build:webdriver`，禁止裸 `cargo build`）；种子含 `orders(customer_id)` 单列 FK 与 `tickets(a, b) → parents(x, y)` 复合 FK、以及一行 `customer_id IS NULL`。

| 用例 | 断言要点 |
| --- | --- |
| `TC-FK-001 single-column jump seeds a visible filter` | 点击带值单元格的箭头 ⇒ 新标签出现、筛选条含 1 条可见条件、`data-dt-row` 首行值等于来源值；Clear 后恢复全表且面包屑仍在 |
| **`TC-FK-002 composite fk jump uses an AND group`**（**复合外键必测组**） | 复合 FK 行 ⇒ 筛选条两条条件且逻辑为 AND；结果集行数 = 种子中满足两列的值数；不出现"只按第一列"的多行结果 |
| **`TC-FK-003 back restores the source panel position`**（**返回栈必测组**） | 先在源表滚到第 N 行、选中一个区域；跳转后返回 ⇒ 断言滚动容器 `scrollTop` 与离开时一致（±1 行高）且该行仍可见；`[data-dt-selected="true"]` 数量与离开时一致（01 已落地时） |
| **`TC-FK-004 A to B to A keeps exactly two tabs`**（**循环引用必测组**） | 往返两次后标签数为 2；`back` 栈不增长（用工具栏/面包屑的可见状态间接断言）；无重复标签标题 |
| `TC-FK-005 null fk cell shows no arrow` | NULL 行单元格内无 `[data-dt-fk-click]`；点击该单元格不产生新标签 |
| `TC-FK-006 unreachable target shows a classified error, not a blank screen` | 用无读取权限/已删除的目标表 ⇒ 源标签显示错误文本、**不**出现空白页、标签数不变 |

---

## 10. 自查清单

| # | 错误做法 | 正确做法 | 后果 |
| --- | --- | --- | --- |
| 1 | 按列名匹配到 FK 后**只取 `columns[0]` 的值**去跳转 | 用 `ForeignKeyInfo.columns` 的**整组**取值、按位置与 `referenced_columns` 配对、逻辑固定 `and` | 复合外键定位到错误的父行（甚至静默命中一行），业务上等价于"跳到了别人家的订单"，是最严重的一类错误 |
| 2 | 走既有 `handleSelectTable` 打开目标表 | 新编排函数：`addPanel` **强制新建** + `seedPanel` 预置切片 | `handleSelectTable` 按 `(tableName, database)` 去重 ⇒ 已打开同表时**只切换标签、筛选完全不生效**，用户以为跳转坏了 |
| 3 | 建完面板后调用 `setFilters` 注入条件 | 用 `seedPanel` 一次写好首帧切片 | `patchPanel` 对无切片的面板是 no-op，而 `setFilters` 又立刻 `reloadPanel` ⇒ 首帧无筛选 + 多余请求，出现"闪一下全表" |
| 4 | 把首帧筛选也写进 `TablePanel.initialFilters` | 筛选唯一真相留在 `TableState.filters`，`TablePanel` 只放 `fkOrigin` | 用户 Clear 后重新挂载标签，`initialFilters` 把条件又加回来；两套真相必然打架 |
| 5 | 返回时从 DOM 反推选中了哪些格 | 选择状态读 `TableState.gridSelection`（01 已按 `panelId` 记账）；只把滚动位置与锚点写进返回栈 | DOM 是虚拟滚动的产物，反推会漏格、误格；契约 §4.3 明令禁止 |
| 6 | 把 `exists: false` 做成 `CommandError` | 正常返回 `exists:false`，前端显示提示条并照常打开标签 | 目标行被并发删除后用户被弹错误、拿不到"空结果 + 可见条件"的上下文，无法自查 |
| 7 | 为了让提示更清楚，把 `referenced_table` 在前端按 `.` 拆开 | 定位全部交给宿主（`resolve_foreign_key_targets`），前端只消费 `FkTargetRef` | PG 带 schema 前缀、MySQL 裸名；前端拆名会写出按库分支，直接违反零硬编码，且引号/大小写方言会让它错 |
| 8 | 跳转时顺便 `COUNT(*)` 判断"有没有多行" | 存在性用 `LIMIT 1`（复用 `get_table_data` 的 `skip_count = true`），候选值有硬上限 | 大表上每次点箭头都全表计数，一次交互就是几秒的卡顿 |
| 9 | 循环引用（A→B→A）时继续开新标签 | `navKey` 命中就 `rewindTo` 既有条目（不新建标签、不无限增长），`FK_NAV_MAX_DEPTH` 只作兜底 | 用户来回点几下就堆出几十个标签，并且每个都带筛选，关标签成了体力活 |
| 10 | 把箭头回调写成 `columns.map` 里的内联箭头函数 | 从 `DataTable` → `VirtualBody` → `GridCell` 逐层透传**稳定引用** | `memo(GridCell)` 全部失效，大表虚拟滚动每帧重渲染全部可见格，滚动掉帧 |
| 11 | 未命中 `grid.fk.*` 前缀时显示空白或"未知错误" | `classifyGridError` 返回 `'unknown'` 时**原样显示原始消息** | 用户拿不到任何可行动信息，问题无法上报（契约 §8.1 明确禁止吞掉） |

---

## 11. 未决问题

| # | 问题 | 建议选项 |
| --- | --- | --- |
| U-1 | **跨库 / 跨 schema 外键无法可靠表达**：`ForeignKeyInfo` 只有 `referenced_table`（PG 把 schema 拼在字符串里、MySQL/SQLite 只有裸名），没有 `referenced_schema` / `referenced_database` 字段 | **建议（本册采用）**：不改 `driver-api`，用 3.1 Step 1-④ 的"先整体试、再按最后 `.` 拆分回退"阶梯解决跨 schema；跨库一律 `grid.fk.targetUnresolved` 并提示用户从对象树打开目标表。**备选（后续单独提案）**：给 `ForeignKeyInfo` 增加 `#[serde(default)] referenced_schema: Option<String>` 与 `referenced_database: Option<String>`（只新增字段、带默认值 ⇒ 按契约 §1.7 无需提升 `PROTOCOL_VERSION`），由驱动侧填充；PG 可顺势不再把 schema 拼进 `referenced_table`。需人工在"现在就扩契约"与"先靠阶梯回退"之间裁定 |
| U-2 | 箭头的视觉与触发方式 | **建议**：概念上采用"值右侧一个 12×12 的箭头按钮 + 悬停 tooltip"，具体图标取 `lucide-react` 的 `ArrowUpRight`（`TableView.tsx` 已经在用 `lucide-react`）；**不建议**双击单元格作为跳转入口（双击今天等于"进入编辑"，改语义会回归 `table-edit.ts`）。需要人工确认是否还要提供右键菜单入口（"跳转到引用表"） |
| U-3 | 返回栈是否要跨应用重启保留 | **建议**：不保留（纯内存），与 PRD"不落盘"一致。若产品要求保留，需要新的持久化面（`settingsStore`）与失效策略（表被重命名/删除后栈条目失效），成本远超本册 |
| U-4 | `tableDataStore.ts` 行数预算冲突 | 01 要求 `tableDataStore.ts ≤ 700`，本册 `seedPanel` 追加约 28 行 ⇒ 建议把 `seedPanel` 放进 01 已新建的 `src/stores/tableData/selectionActions.ts` 同类位置，**或**新建 `src/stores/tableData/panelSeed.ts` 承载 `seedPanel` 并在 store 里薄委托。需人工确认拆到哪个文件（本册倾向后者，因为 `seedPanel` 与选择无关） |
| U-5 | 默认排序（DB-09）与跳转筛选的相互作用 | 目标标签页的 `sorts` 为空 ⇒ DB-09 的默认排序会生效。若默认排序落在未被引用的列上，跳转的结果顺序可能与来源页不一致。**建议**：跳转标签页显式把"被引用列升序"作为初始排序（一次查询内可见地排在筛选条旁），**或**保持空 `sorts` 让 DB-09 决定。需要人工在"一致性"与"复用 DB-09"之间裁定；本册默认取后者（空 `sorts`），因为 FK 列通常已是目标表的主键/唯一索引，默认排序大概率就是它 |
| U-6 | 目标标签页的命名 | **建议**：沿用 `TablePanel` 的默认标题（表名），追加一个不落盘的返回栈指示（面包屑已足够）；**不建议**把标题改成 `customers (from orders)`——标签标题是用户识别表的主要线索，混入来源会让多标签场景更难读 |
| U-7 | `⌘/Ctrl+⌥+←/→` 与系统/其它应用快捷键冲突（macOS 上 `⌥←` 是"按词移动"） | **建议**：只在网格容器聚焦时绑定（不注册 window 监听），并对 `isEditableTarget` 目标放行；同时在 U-2 的右键菜单里提供"跳转到引用表 / 返回来源"作为不依赖快捷键的路径。需人工确认是否改用 `⌘/Ctrl+[ ]` 之类的组合 |
| U-8 | DB-20 复用本命令时的候选值上限与排序 | **建议**：`limit` 默认 50、硬上限 200，排序由 DB-20 自己传（本册命令今天不支持排序参数，绝不偷偷按主键排序）；若 DB-20 需要搜索框，再给 `ResolveFkTargetsRequest` 增加 `#[serde(default)] search: Option<String>`（只新增字段，命令签名不变） |
