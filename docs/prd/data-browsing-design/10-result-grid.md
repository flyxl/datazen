# 10 · 查询结果网格可编辑（DB-14）

> **状态**：目标设计，**未实现**。本文描述的是要落地的形态，不是现状。
> **依赖**：`00-contracts.md`（§1.4 计划/提交链路与 `fingerprint`/`affected == 1`、§3 `CellWrite` 三态、§4.3 选择状态永不参与写操作定位、§8 错误前缀机制、第 10 节写作模板）；`01-selection.md`（选择与写路径解耦、重取数清空选择）；分册 04（行身份）；`05-editors.md`（编辑态与输入法）；`06-dirty-cells.md`（脏单元格呈现与提交入口）。
> **被谁依赖**：`12-testing.md`（本册新增的表驱动负例进入回归矩阵）；后续任何「查询结果集内写入」能力（批量粘贴、结果集批注、AI 生成变更）都必须先经过本册的判定层。
> **预估人日**：3.5 人日（判定层与驱动契约 1.5 / 前端只读化与原因解释 1.0 / 测试 1.0）。
> **不包含**：INSERT 新增行；结果网格内的删除；跨行批量写入（`applyColumnToRows` 语义）；视图与物化视图的可写性；唯一键/伪主键定位；`CellWrite` 在查询结果链路上的完整落地（属分册 06）；驱动侧的 SQL 文本解析；事务语义变更。

---

## 1. 目标与验收口径

**一句话目标**：查询结果网格的单元格只有两种状态——**由驱动提供的来源信息严格证明可写回**，或**明确只读并给出人类可读的原因**；今天「改完就丢」的内存改写路径被彻底关闭。

今天的行为（缺陷本体）是第三种状态：「看着像能改、改了、然后悄悄丢掉」。本册把第三种状态消灭掉。

### 1.1 验收标准

| # | 验收项 | 命令 | 期望输出 |
| --- | --- | --- | --- |
| A-1 | 驱动契约默认实现必须返回「不可判定」 | `cargo test -p datazen-driver-api --lib result_origin` | `test result: ok`，含 `default_impl_is_undeterminable` |
| A-2 | 宿主判定层全部形态（负例表驱动） | `cargo test -p datazen --lib result_edit` | `test result: ok`，负例逐条通过 |
| A-3 | 前端行级判定合并 | `npx vitest run src/lib/__tests__/resultEditability.test.ts` | `Tests  N passed` |
| A-4 | 错误前缀分类与未知回退 | `npx vitest run src/lib/__tests__/gridErrors.test.ts` | `Tests  N passed` |
| A-5 | 只读时禁止编辑（组件层） | `npx vitest run src/windows/connection/__tests__/ContentViewDrawers.resultReadOnly.test.tsx` | `Tests  N passed` |
| A-6 | 类型安全（含测试文件） | `pnpm typecheck` | 无 `error TS`，无 `any` |
| A-7 | E2E 只读链路 | `pnpm e2e:skip-build -- --spec e2e/specs/result-grid-readonly.spec.ts` | `passing` |
| A-8 | 手工黑盒（无来源能力的驱动，即当前全部驱动） | 启动应用，在查询面板执行 `SELECT * FROM <有主键的单表>`，双击单元格 | 不进入编辑态；出现原因提示；数据库侧无 UPDATE 发出 |

### 1.2 「不可写回时禁止编辑」的验证方式（本册最重要的验收项）

必须**同时**满足三条，缺一不算通过：

1. **判定层拒绝**：`evaluate_result_editability(...)`（新增，Rust）与 `resolveGridEditability(...)`（新增，TS）对每一条不可写回形态返回 `writable == false` 且 `reason` 精确等于预期值——表驱动单测逐条断言，禁止只断言 `writable == false`。
2. **界面不产生写意图**：只读时双击 / 回车 / F2 / 直接键入 / 粘贴都必须不进入编辑态，且**不调用** `updateResultCell`，不产生任何 pending change；组件测试用 spy 断言调用次数为 `0`。
3. **不发出写语句**：E2E 在只读形态下执行完整编辑手势后，断言 `preview_pending_changes` 与 `commit_pending_changes` 的调用计数为 `0`，并断言界面未显示「成功」。

### 1.3 判定为可写回时必须满足的全部条件（缺一即只读）

由驱动声明与宿主校验共同构成，**任何一项无法确定即为只读**：

1. 驱动实现了来源判定，且返回 `Determined`（不是 `Undeterminable`）；
2. 结果整体来源形态为「单一基表」（`shape == SingleBaseTable`，`object_kind == BaseTable`，`referenced_tables == 1`）；
3. 未去重（`deduplicated == false`）、未聚合（`aggregated == false`）、无集合运算（`set_operation == None`）、无派生来源（`has_derived_source == false`）；
4. 该表**有主键**（宿主经既有 `SchemaCache::get_columns` 取得的 `CachedColumns.primary_keys` 非空）；
5. 主键的**每一列**都出现在结果列中，且对应 `ResultColumnOrigin.passthrough == true`；
6. 被编辑行的主键值**都不为 NULL**（页面级检查）；
7. 被编辑行的行身份在**已取到的行**内唯一（页面级检查）；
8. 连接会话可写；
9. **单行编辑**：本次编辑只针对一个 `rowIndex`，不得因选择状态扩散到多行。

第 4、5 条的来源事实由驱动给出、主键事实由宿主既有缓存给出，**宿主全程不解析 SQL 文本**。

---

## 2. 现状代码事实

> 本节只列亲自读过的符号。**无行号**，不含推测。

### 2.1 结果网格今天的接线

文件 `src/windows/connection/ContentViewDrawers.tsx`：

- 这是一个「详情抽屉」组件，同时服务两种面板：`activePanel.type === 'table'` 与 `activePanel.type === 'query'`。
- 它从 `usePanelStore` 取出 `updateResultCell` 与 `updateSql`，从 `useTableDataStore` 取出 `tableSlice`（`columns` / `rows` / `selectedRows` / `detailRowIndex`）。
- 它用 `activeQueryExec`（`s.queryExec.get(activePanel.id)`）与 `activeQueryExec.activeResultIdx` 定位当前结果集，得到 `activeQueryResult`，并读取 `activeQueryResult.columns`、`activeQueryResult.rows`、`resultDetailRowIndex`。
- 编辑入口是 `handleDetailFieldEdit(row, col, value)`：
  - `table` 分支：当 `selectedRows.size > 1` 时调用 `store.applyColumnToRows(activePanel.id, col, value, [...selectedRows])`，否则调用 `store.stageCellChange(activePanel.id, row, col, value)`；
  - `query` 分支：调用 `updateResultCell(activePanel.id, activeQueryExec.activeResultIdx, row, col, value)`。
- 结果列的列定义由 `activeQueryResult.columns.map(c => ({ id: c.name, name: c.name, type: c.dataType }))` 构造（`detailColumnDefs`）；行数据经 `rowToRecord(activeQueryResult.rows[i], activeQueryResult.columns)` 转成记录（`detailRow`）。
- 常量回退：`NO_COLUMNS` / `NO_ROWS` / `NO_SELECTED`。
- `updateResultCell` 出现在该 `useCallback` 的依赖数组里。

**观察结论**：`query` 分支的编辑**完全绕过** `table` 分支使用的 pending-change 机制（`stageCellChange` / `applyColumnToRows`），直接改写结果副本。这就是「改完就丢」的接线来源。

### 2.2 `panelStore` 侧的 `updateResultCell`

文件 `src/stores/panelStore.ts`（实测 **728 行**）：

- 接口声明 `updateResultCell(panelId, resultIdx, row, col, value, paneId)`；
- 实现先经 `paneKey(panelId, paneId)`（`src/stores/paneKeys.ts`）取 `queryExec`，再取 `exec.results[idx]`；
- 在 `exec.results[resultIdx].columns` 中按 `c.name === col` 找列下标，改写内存中的那一格。

**观察结论**：该函数**只改本地结果副本**，没有持久化、没有写回数据库、没有计划、没有 `fingerprint`、没有 `affected == 1` 校验。**任何以它为终点的编辑路径必然是数据丢失路径。**

### 2.3 仓库里没有 SQL 结构解析器

`src-tauri/src/sql_guard/` 只有安全扫描，可复用的符号是：`reject_null_bytes`、`normalize_fullwidth`、`strip_sql_comments`、`normalize_sql`、`check_sql`、`is_write_sql`、`apply_params`，外加 `scanner.rs`。

这些符号回答的是「这条 SQL 是否危险 / 是否写语句 / 参数如何绑定」，**没有一个能回答「这条 SELECT 的结果来自哪张表、是不是 JOIN、有没有聚合、某一列是不是直通列」**。因此「复用既有解析器判定可写回性」这条路在本仓库**不存在**。

### 2.4 结果列信息不携带来源

结果集列是普通 `ColumnSchema`（`name` / `data_type` / `nullable` / …），**不含「该列来自哪张表的哪一列」**。前端 `detailColumnDefs` 也只拿到 `c.name` / `c.dataType`，连别名与源列名的区别都看不出来。

### 2.5 行身份的现状承载

- `src/stores/tableData/types.ts` 定义 `rowIdentityAnchors: Map<number, string>`（`rowIndex` → 行身份键）。
- `src/stores/tableData/pendingChanges.ts` 的解析入口是 `ts.rowIdentityAnchors.get(rowIndex)`。
- `src/stores/tableDataStore.ts` 在多处维护 `rowIdentityAnchors` 的映射。
- `00-contracts.md` §4.3 把这条链条固定为：`CellCoord.rowIndex → TableState.rowIdentityAnchors.get(rowIndex) → TableState.pendingChanges.get(identityKey) → PendingRowChange.rowIdentity`，并禁止任何 `WHERE ... = <rowIndex>` 形式的路径。
- **注意**：`rowIdentityAnchors` 只服务 **table 面板**。查询结果面板没有也不应有这套按 `rowIndex` 记账的映射——查询结果的行身份必须**每次编辑时从结果行本身 + 驱动来源信息重新构造**（见 §3.6），这样它天然免疫翻页/排序导致的 `rowIndex` 平移。

### 2.6 既有的计划—提交链路（`00-contracts.md` §1.4，直接复用，不新造）

位置 `src-tauri/src/commands/data.rs`（实测 **1331 行**）。符号：`preview_pending_changes` → `build_row_change_plan` → `changes_fingerprint`；`commit_pending_changes` → `commit_pending_changes_impl` → `execute_row_change_plan_impl`；遗留入口 `commit_row_updates` / `commit_row_deletes`。

DTO：`CellUpdate`、`RowUpdateBatch`、`RowDeleteBatch`、`RowChangeTableContext`、`PendingRowChange`、`PlannedStatement`、`RowChangePlan`、`CommitPendingChangesRequest`、`RowCommitStatementResult`、`CommitPendingChangesResponse`。

三条必须复用的纪律：

1. `RowChangePlan` **只有 `updates` 与 `deletes`**，没有 `inserts`——本册因此**不提供新增行**；
2. `execute_row_change_plan_impl` 要求每条语句 `affected == 1`，否则整批失败并报 `CommandError::Validation`——这是本册唯一可信的多行误伤兜底；
3. `fingerprint` 防「计划开裂」，预览与提交之间暂存改动变化必须拒绝提交，**禁止降级为重算**。

### 2.7 既有的 `CellWrite` 三态（`00-contracts.md` §3）

`CellWrite` 定义为 `Null` / `Value(Value)` / `Unset`，Rust 侧位于 `packages/driver-api/src/types.rs`（实测 **939 行，已超 800**），前端镜像位于 `src/lib/tableChanges.ts`（实测 **227 行**），并配 `cellWriteNull` / `cellWriteValue` / `cellWriteUnset` 构造器；「是否真的改动」一律复用 `valuesEqual`。

**本册的取舍**：查询结果链路的**写意图表达**复用 `CellWrite`，但本册不落地 `06-dirty-cells` 的完整脏标记/提交 UI——本册只保证「不可写回时不可能产生任何写意图，可写回时写意图只经计划管线落地」。

### 2.8 规模体检（决定新代码放哪里）

| 文件 | 实测行数 | 结论 |
| --- | --- | --- |
| `src/stores/panelStore.ts` | 728 | **接近上限**，本册只能在其上加最小接线，新逻辑必须另起文件 |
| `packages/driver-api/src/types.rs` | 939 | **已超限**，新增类型不得继续堆在这里 |
| `packages/driver-api/src/traits.rs` | 1020 | **已超限**，只允许追加带默认实现的方法 |
| `src-tauri/src/commands/data.rs` | 1331 | **已超限**，新命令必须另起模块 |
| `src/windows/connection/ContentViewDrawers.tsx` | 228 | 有余量 |
| `src/locales/en/query.ts` | 424 | 有余量 |

---

## 3. 数据结构与接口设计

### 3.1 新增驱动契约：结果来源描述（Rust）

**落点**：新增文件 `packages/driver-api/src/result_origin.rs`，并在 `packages/driver-api/src/lib.rs` 中按该文件既有的模块登记与再导出方式登记（该文件已有 `mod traits;` / `mod types;` 一类的写法）。**不放进 `types.rs`**——它已 939 行。

```rust
// 新增：packages/driver-api/src/result_origin.rs

/// 结果集来源的判定结论。默认实现返回 `Undeterminable`，
/// 因此未实现本能力的驱动下，结果网格保持只读——与今天的行为一致，零回归风险。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum ResultOrigin {
    /// 驱动无法判定（默认实现、方言不支持、元数据不可得）。
    Undeterminable(UndeterminableReason),
    /// 驱动给出了判定。
    Determined(ResultOriginDetail),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UndeterminableReason {
    /// 驱动未实现本能力（默认实现唯一会用的取值）。
    NotImplemented,
    /// 驱动实现了，但当前方言/语句形态超出其能力。
    UnsupportedDialect,
    /// 元数据不可得（例如会话已失效）。
    MetadataUnavailable,
}

/// 结果整体来源形态。只有 `SingleBaseTable` 具备可写回的前提。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultShape {
    SingleBaseTable,
    MultiTable,
    Derived,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetOperation { Union, Intersect, Except }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultObjectKind { BaseTable, View, MaterializedView, Unknown }

/// 一张被引用的表。`aliased` 只表达「SQL 里取了别名」，
/// 不代表可写性——真实表名必须由驱动给出，宿主绝不从 SQL 文本里推。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultTableRef {
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub name: String,
    pub aliased: bool,
}

/// 单个结果列的来源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultColumnOrigin {
    /// 结果集中的列名（即 `ColumnSchema.name`，可能带别名）。
    pub column: String,
    /// 是否直通列：直接取自某个基表列，未经表达式/函数/常量/聚合包装。
    pub passthrough: bool,
    /// 直通时为来源表；非直通为 `None`。
    pub table: Option<ResultTableRef>,
    /// 直通时为**源表上的真实列名**（不是别名）；非直通为 `None`。
    pub source_column: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultOriginDetail {
    pub shape: ResultShape,
    /// 仅当 `shape == SingleBaseTable` 时为 `Some`。
    pub table: Option<ResultTableRef>,
    /// 被引用的表数量。JOIN 的证据由驱动给出，宿主不数 SQL 里的 FROM。
    pub referenced_tables: u32,
    /// 是否经过 DISTINCT / GROUP BY / 去重类语义。
    pub deduplicated: bool,
    /// 是否含聚合函数 / GROUP BY / HAVING / 窗口函数。
    pub aggregated: bool,
    pub set_operation: Option<SetOperation>,
    /// 结果的行来源是否含派生表（`FROM (SELECT ...)`）或投影中的子查询。
    /// `WHERE` 里的子查询**不**置位（列来源未受影响）。
    pub has_derived_source: bool,
    pub object_kind: ResultObjectKind,
    /// 逐列来源。**长度必须等于结果列数**，否则宿主整体降级为 `Undeterminable`。
    pub columns: Vec<ResultColumnOrigin>,
}
```

**逐字段说明**：

| 字段 | 用途 | 缺省 / 不可得时的语义 |
| --- | --- | --- |
| `ResultOrigin::Undeterminable(reason)` | 「我不确定」的显式表达 | 判定层直接判只读，原因是 `unknownOrigin`；`reason` 只用于日志与诊断文案 |
| `ResultOrigin::Determined(detail)` | 「我确定，结论如下」 | 仍须通过 §1.3 的全部条件才可能可写 |
| `ResultShape` | 结果的行来源形态 | `Unknown` 等同不可判定 |
| `SetOperation` | 集合运算 | `Some(_)` 一律只读 |
| `ResultObjectKind` | 目标是基表还是视图 | 阶段一只有 `BaseTable` 可写；`View` / `MaterializedView` 一律只读 |
| `ResultTableRef.catalog/schema` | 写回时构造 `RowChangeTableContext` 需要 | 为 `None` 时按连接的当前库/模式处理，与计划链路既有语义一致 |
| `ResultTableRef.name` | **写回语句里唯一被允许使用的表名** | 驱动必须给真实表名，别名不算 |
| `ResultTableRef.aliased` | 仅用于诊断与文案 | 不影响可写性 |
| `ResultColumnOrigin.column` | 与结果列对齐的键 | 与 `ColumnSchema.name` 精确匹配 |
| `ResultColumnOrigin.passthrough` | 该列能否作为定位列 / 可写列 | `false` 时该列不可写（不报错，只是不可编辑） |
| `ResultColumnOrigin.table` | 直通列的来源表 | 必须与 `detail.table` 一致，否则宿主判「来源歧义」→ 只读 |
| `ResultColumnOrigin.source_column` | **写回 `SET` 子句里唯一被允许使用的列名** | 为 `None` 而 `passthrough == true` 属驱动自相矛盾 → 只读 |
| `ResultOriginDetail.table` | 单基表形态下的目标表 | 与 `shape` 一致性由宿主校验，不一致 → 只读 |
| `referenced_tables` | 交叉校验：`shape == SingleBaseTable` 时必须等于 1 | 不等于 1 → 只读 |
| `deduplicated` / `aggregated` / `set_operation` / `has_derived_source` | 四个独立的一票否决位 | 任一置位 → 只读 |
| `columns` | 逐列来源 | 长度不匹配或含重复 `column` → 整体降级只读 |

**`ResultOrigin` 采用 `tag = "kind"` + `content = "detail"`**，理由与 `CellWrite` 相同：不要两层 `untagged`，否则 `{"detail": null}` 类输入无法判定。

### 3.2 新增 trait 方法（带默认实现）

**落点**：`packages/driver-api/src/traits.rs`（实测 1020 行，只允许追加）。默认实现**必须**返回不可判定。

```rust
// 新增：packages/driver-api/src/traits.rs

/// 描述这条 SQL 的结果来源。宿主**不**解析 SQL，
/// 因此可写回性判定的全部原始输入都来自这里。
///
/// 连接参数沿用本 trait 内既有查询方法的连接句柄类型与 `db_session_id` 形态，
/// 不新增平行概念；若既有查询方法为 async，本方法同形。
fn describe_result_origin(
    &self,
    handle: &/* 既有连接句柄类型 */,
    db_session_id: &str,
    sql: &str,
) -> Result<ResultOrigin, CommandError> {
    let _ = (handle, db_session_id, sql);
    Ok(ResultOrigin::Undeterminable(UndeterminableReason::NotImplemented))
}
```

- **为什么默认返回不可判定**：现有全部驱动都不实现它，于是行为与今天**逐字节一致**（结果网格只读），不需要逐驱动回归。
- **为什么是「可选能力」而非必答**：要求每个驱动立即实现会迫使驱动作者在没有把握时编造答案——那正是本册要消灭的「猜测」。
- **`PROTOCOL_VERSION` 是否提升**：不提升（只新增带默认实现的方法），与 `00-contracts.md` C-3 的建议一致。

### 3.3 宿主判定层（Rust，纯函数）

**落点**：新增文件 `src-tauri/src/services/result_edit.rs`，并在 `src-tauri/src/services/mod.rs` 登记。

```rust
// 新增：src-tauri/src/services/result_edit.rs

/// 判定结论中「不可写回」的原因码。前端据此选 i18n 文案，
/// 不把任何面向用户的字符串放进 IPC。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GridReadOnlyReason {
    UnknownOrigin,      // 驱动不可判定 / 未实现
    AmbiguousOrigin,    // 驱动自相矛盾或长度不匹配（含列来源跨表）
    MultiTable,         // JOIN / 多表
    Aggregate,          // 聚合 / GROUP BY / HAVING / 窗口函数
    Distinct,           // DISTINCT 或等价去重
    SetOperation,       // UNION / INTERSECT / EXCEPT
    DerivedSource,      // 派生表 / 投影子查询
    View,               // 视图 / 物化视图
    NoPrimaryKey,       // 表没有主键
    KeyNotSelected,     // 主键列未完整出现在结果列中，或不是直通列
    SessionReadOnly,    // 会话/连接只读
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultEditability {
    /// 表级判定：能否写回。
    pub writable: bool,
    /// 可写时为「结果列名 → 源表真实列名」的定位键映射（全部为主键列）。
    /// 不可写时为空。
    pub identity_columns: Vec<IdentityColumn>,
    /// 仅当 `writable` 为真时为 `Some`：写回的目标表（真实表名）。
    pub table: Option<ResultTableRef>,
    /// 仅当 `writable` 为假时为 `Some`。
    pub reason: Option<GridReadOnlyReason>,
}

/// 一列定位键的双名映射。写回一律用 `source_column`，展示一律用 `column`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityColumn {
    pub column: String,
    pub source_column: String,
}

/// 表级判定。纯函数，无 IO、无全局状态，便于表驱动测试。
pub fn evaluate_result_editability(
    origin: &ResultOrigin,
    primary_keys: &[String],   // 来自既有 SchemaCache::get_columns → CachedColumns.primary_keys
    session_writable: bool,    // 由调用方从既有连接/会话状态传入；本册不新增该概念
) -> ResultEditability;
```

**判定顺序（短路，先判最不可能误伤的条件）**：

1. `session_writable == false` → `SessionReadOnly`；
2. `origin` 为 `Undeterminable` → `UnknownOrigin`；
3. 自洽性校验（`shape`/`table`/`referenced_tables`/`columns` 长度/列名重复/`passthrough` 与 `source_column` 的匹配/列来源表与 `detail.table` 一致）任一失败 → `AmbiguousOrigin`；
4. `set_operation.is_some()` → `SetOperation`；
5. `has_derived_source` → `DerivedSource`；
6. `aggregated` → `Aggregate`；
7. `deduplicated` → `Distinct`；
8. `object_kind != BaseTable` → `View`；
9. `shape != SingleBaseTable || referenced_tables != 1` → `MultiTable`；
10. `primary_keys.is_empty()` → `NoPrimaryKey`；
11. 任一主键列不在结果列中、或对应 `passthrough == false`、或 `source_column` 为 `None` → `KeyNotSelected`；
12. 全部通过 → `writable: true`，`identity_columns` 为「结果列名 → 源列名」映射。

**关键纪律**：`primary_keys` 必须取 `CachedColumns.primary_keys`（缓存内已由 `effective_primary_keys()` 归一化），**不要**去遍历 `ColumnSchema` 上的 `is_primary_key` 标志——`00-contracts.md` §1.5 已记录该隐患（驱动若填了 `primary_keys` 却没在列上打标志，遍历标志会拿不到主键）。这与分册 09 的修正方向一致。

### 3.4 新增 IPC 命令

**落点**：新增文件 `src-tauri/src/commands/result_origin.rs`，在 `src-tauri/src/commands/mod.rs` 登记（`data.rs` 已 1331 行，不再追加）；前端 wrapper 落在既有 `src/commands/query.ts`。

```rust
// 新增：src-tauri/src/commands/result_origin.rs

/// 解析一条已执行 SQL 的结果来源并给出可写回判定。
/// 前端在结果集首次就绪时异步调用一次用于渲染；**写回前必须重新调用一次**，
/// 不得信任缓存（schema 可能已变）。
#[tauri::command]
pub async fn resolve_result_origin(
    state: State<'_, AppState>,
    db_session_id: String,
    sql: String,
) -> Result<ResultEditability, CommandError>;

/// 按结果行构造一次写回计划（走既有计划管线，不新增执行路径）。
#[tauri::command]
pub async fn preview_result_cell_change(
    state: State<'_, AppState>,
    request: ResultCellChangeRequest,
) -> Result<RowChangePlan, CommandError>;
```

```rust
// 新增：src-tauri/src/commands/result_origin.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultCellChangeRequest {
    pub connection_id: String,
    pub db_session_id: String,
    pub driver_type: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
    /// 结果行上取到的**原始**（未编辑）定位键值；写回时它是 WHERE 的唯一来源。
    pub row_identity: BTreeMap<String, Option<Value>>,
    /// 结果列名 → 源列名，由 `ResultEditability.identity_columns` 原样带回。
    pub identity_columns: Vec<IdentityColumn>,
    /// 结果列名 → 用户输入的新值。
    pub column: String,
    pub value: Value,
    /// 必须为 1：本册禁止多行写入。
    pub row_count: u32,
}
```

**错误前缀**（`00-contracts.md` §8 机制：`CommandError` 序列化到 IPC 是脱敏字符串、无 `code` 字段）：

| 前缀 | 触发条件 | 说明示例 |
| --- | --- | --- |
| `grid.result.originUnavailable: ` | 驱动返回 `Undeterminable` 且前端仍尝试写回 | `grid.result.originUnavailable: the driver cannot report where this result came from` |
| `grid.result.originAmbiguous: ` | 驱动返回的来源信息自相矛盾 / 长度不匹配 | `grid.result.originAmbiguous: column origin count does not match the result column count` |
| `grid.result.rowNotAddressable: ` | 行身份缺失、含 NULL、或在结果页内不唯一 | `grid.result.rowNotAddressable: the result page contains duplicate row identities` |
| `grid.result.multiRowNotAllowed: ` | `row_count != 1` | `grid.result.multiRowNotAllowed: editing many query-result rows at once is not supported` |
| `grid.result.writeDenied: ` | 数据库侧权限拒绝（提交阶段） | `grid.result.writeDenied: permission denied for table orders` |
| `grid.cell.readOnly: ` | 判定为只读时的编辑拒绝 | `grid.cell.readOnly: this result is an aggregate over multiple tables` |

**安全铁律**：`preview_result_cell_change` 与提交链路**必须**在任何可能的 UPDATE 之前拒绝以下情形，且拒绝必须发生在生成 SQL 之前：（a）`row_identity` 为空 map；（b）`row_identity` 的任一值为 `None`；（c）`row_count != 1`；（d）`identity_columns` 与 `row_identity` 的键集合不一致。**任何一条不满足即 `CommandError::Validation`**，绝不允许退化为「无 `WHERE` 的 UPDATE」。

### 3.5 前端镜像与判定合并（TS）

**落点**：新增文件 `src/lib/resultEditability.ts`。无 `any`。

```ts
// 新增：src/lib/resultEditability.ts

export type ResultShape = 'singleBaseTable' | 'multiTable' | 'derived' | 'unknown';
export type SetOperation = 'union' | 'intersect' | 'except';
export type ResultObjectKind = 'baseTable' | 'view' | 'materializedView' | 'unknown';

export interface ResultTableRef {
  catalog: string | null;
  schema: string | null;
  name: string;
  aliased: boolean;
}

export interface ResultColumnOrigin {
  column: string;
  passthrough: boolean;
  table: ResultTableRef | null;
  sourceColumn: string | null;
}

export interface ResultOriginDetail {
  shape: ResultShape;
  table: ResultTableRef | null;
  referencedTables: number;
  deduplicated: boolean;
  aggregated: boolean;
  setOperation: SetOperation | null;
  hasDerivedSource: boolean;
  objectKind: ResultObjectKind;
  columns: ResultColumnOrigin[];
}

export type ResultOrigin =
  | { kind: 'undeterminable'; detail: 'notImplemented' | 'unsupportedDialect' | 'metadataUnavailable' }
  | { kind: 'determined'; detail: ResultOriginDetail };

export interface IdentityColumn { column: string; sourceColumn: string }

/** 表级判定（来自 IPC，由 Rust 计算）。前端**不**自行重算表级结论。 */
export type ServerEditability =
  | { writable: true; identityColumns: IdentityColumn[]; table: ResultTableRef; reason: null }
  | { writable: false; identityColumns: []; table: null; reason: GridReadOnlyReason };

/** 全部只读原因码，与 Rust `GridReadOnlyReason` 一一对应，另加两个页面级原因。 */
export type GridReadOnlyReason =
  | 'unknownOrigin' | 'ambiguousOrigin' | 'multiTable' | 'aggregate' | 'distinct'
  | 'setOperation' | 'derivedSource' | 'view' | 'noPrimaryKey' | 'keyNotSelected'
  | 'sessionReadOnly'
  | 'keyValueNull'        // 页面级：定位键取值为 NULL
  | 'duplicateIdentity';  // 页面级：定位键在已取到的行内重复

export type GridEditability =
  | { writable: true; identityColumns: IdentityColumn[]; table: ResultTableRef }
  | { writable: false; reason: GridReadOnlyReason };

/**
 * 把服务端表级判定与「当前已取到的行」合并成最终判定。
 * 页面级检查**只做纯数据判断**（NULL / 重复），不做任何 SQL 语义推断。
 */
export function resolveGridEditability(
  server: ServerEditability,
  rows: ReadonlyArray<ReadonlyArray<unknown>>,
  columns: ReadonlyArray<{ name: string }>,
  rowIndex: number,
): GridEditability;

/** i18n 文案 key 后缀映射，供 `t()` 使用；不含任何硬编码自然语言。 */
export function gridReadOnlyMessageKey(reason: GridReadOnlyReason): string;
```

**为什么判定分两段**：表级结论需要驱动真相（只能服务端算），行级结论依赖本页已取到的数据（只能在客户端算）。两者用 `AND` 合并，**任一段失败即只读**。前端不得自行重算表级结论，避免双份实现漂移出「前端说能写、后端说不能」的分裂。

### 3.6 写回通道：复用既有计划管线，不新造执行路径

编辑一次查询结果单元格的完整链路（**必须**按此顺序，任何一步失败即中止）：

```
用户在结果网格输入新值
  → resolveGridEditability(...)                     // 页面级合并判定
      ├─ 不可写 → 抛 grid.cell.readOnly:<reason> ，不产生任何 pending change（出口见 §4）
      └─ 可写
          → 从结果行取原始值构造 row_identity：
              identity_columns.map(ic => [ic.sourceColumn, originalValue(row, ic.column)])
          → resolveResultOrigin(...)                // 写前重新判定，不信任缓存
          → preview_result_cell_change(...)         // 服务端再次校验并产出 RowChangePlan（含 fingerprint）
          → commit_pending_changes(fingerprint, plan)  // 既有命令，含 affected == 1 校验
              ├─ 成功 → 刷新该结果集（或局部同步本地副本）
              └─ 失败 → 展示数据库原文，界面**不得**显示成功，**不得**本地写入
```

**为什么不复用 `rowIdentityAnchors`**：那是按 `rowIndex` 记账的映射，只适用 table 面板且必须在每次重取数时清空（`00-contracts.md` §4.3）。查询结果面板**每行自带身份**（驱动已保证定位键列是直通列且在结果中），因此直接由「结果行 + 来源信息」构造行身份，天然免疫翻页/排序导致的 `rowIndex` 平移。

**为什么禁止多行写入**：`selectedRows` 是 UI 选择状态，`00-contracts.md` §4.3 明确禁止它参与写操作定位。查询结果面板一律执行**单行编辑**；`application to many rows` 路径（`applyColumnToRows`）在 `query` 分支**没有入口**。即使未来放开，也必须走 `affected == 1` 逐行校验，而不是一条批量 UPDATE。

**为什么必须复用 `fingerprint` 与 `affected == 1`**：来源判定只能证明「来源是一张可定位的基表」，不能证明「这条 WHERE 恰好命中一行」——后者只有影响行数能证明。绕过它就等于把「判定错一次 → 误伤多行」的风险重新引入。

### 3.7 为什么不用宿主侧的 SQL 文本解析（本册最核心的否决）

1. **仓库里没有结构解析器**（§2.3）。`sql_guard` 全部符号都是安全扫描语义，无法回答来源问题。要新增一个解析器，等于在宿主里手写 SQL 方言解析——而 DataZen 支持多方言，宿主一旦按方言分支就违反「零硬编码」。
2. **正则/手写解析必然出错**：`SELECT *` 后面可能是 JOIN；`FROM t1, t2` 是隐式 JOIN；CTE、派生表、`WITH ... AS MATERIALIZED`、方言特有语法（`QUALIFY`、`UNNEST`、`LATERAL`、`PIVOT`）会让任何启发式破产。**判断错一次的代价不是少一个功能，而是生成一条会误伤多行的 UPDATE。**
3. **结果集的列信息不携带来源**（§2.4）。就算解析出 `FROM orders`，宿主也无法知道结果第 3 列来自 `orders.total` 还是 `orders.quantity * orders.price`；不知道这一点就无法写出正确的 `SET` 列名。
4. **别名会让文本解析定位错列**：`SELECT id AS order_id` 时按结果列名写回会写成 `SET order_id = ...`（错列或不存在）；`FROM orders o` 时按解析出的 token 写回会写成 `SET ... UPDATE o`（错表名）。
5. **正确的位置在驱动**：只有驱动持有 prepared statement / statement handle 的元数据，能给出权威的列来源。因此本册把这件事**定义为驱动契约**，并让默认实现明说「不知道」。

### 3.8 为什么不能靠「用户说这是单表查询」

1. 用户对「单表」的判断基于**他看到的 SQL 文本**，而文本可能含 JOIN、CTE、视图展开——用户认为的「一张表」与数据库眼里的行来源不是一回事。
2. 用户的意图声明**不可验证、不可审计、不可测试**：一条 `SELECT * FROM a JOIN b USING (id)` 在用户看来「就查了一张主要的表」，在数据库看来每行可能对应多行。
3. 一旦引入「用户声明」这个入口，它就会成为把安全责任转嫁给用户的通道，而**用户无法承担这个责任**——他不知道 `affected == 1` 会拦下什么。
4. 而且它并不必要：真正安全的场景（单基表 + 主键在结果中）驱动完全有能力自动识别，不需要用户点任何开关。

---

## 4. 交互与状态机

### 4.1 状态定义

| 状态 | 含义 |
| --- | --- |
| `Resolving` | 结果集就绪后，异步向宿主请求来源判定中 |
| `ReadOnly(reason)` | 判定为不可写回，展示原因 |
| `Writable` | 判定为可写回，单元格可进入编辑 |
| `Editing(row, col)` | 单元格编辑中（输入法组合期内值不落地） |
| `Dirty(row, col, value)` | 已编辑、未提交（复用分册 06 的脏标记呈现） |
| `Submitting` | 计划已生成，提交中 |
| `Failed(message)` | 提交失败，展示数据库原文，**不**本地写入 |

### 4.2 三要素（进入条件 / 状态内行为 / 退出跃迁）

**`Resolving`**
- 进入：结果集就绪（`activeQueryResult` 非空）、结果集/面板切换、SQL 重新执行。
- 状态内行为：单元格**保持只读**（保守默认——判定未回来之前不放行编辑）；不阻塞渲染、不弹模态。
- 退出：判定返回 → `Writable` 或 `ReadOnly(reason)`；IPC 失败 → `ReadOnly(unknownOrigin)`（**失败一律降级为只读，绝不降级为可写**）。

**`ReadOnly(reason)`**
- 进入：判定不可写回。
- 状态内行为：单元格不进入编辑态（双击 / 回车 / F2 / 直接键入 / 粘贴全部无效）；鼠标悬停给出原因；**必须**在结果网格可见位置提供一次性原因说明，而不是一个不响应的灰格子。
- 状态内行为（原因必须区分）：`aggregate`「这是聚合结果」／`multiTable`「这是多表连接的结果」／`noPrimaryKey`「来源表没有主键」／`unknownOrigin`「无法确定数据来源」／`view`「这是视图」／`keyNotSelected`「主键列不在结果中」／`distinct`／`setOperation`／`derivedSource`／`sessionReadOnly`／`keyValueNull`／`duplicateIdentity`／`ambiguousOrigin`。
- **退出跃迁（本支的全部出口，逐条列出）**：
  1. 用户在面板里重新执行 SQL（`updateSql` / 执行入口）→ `Resolving`；
  2. 用户切换结果标签页（`activeResultIdx` 变化）→ `Resolving`；
  3. 用户关闭面板 / 关闭窗口 → 状态与原因提示一并销毁；
  4. 会话可写性变化（连接被置为只读或恢复）→ `Resolving`；
  5. 用户在 table 面板（非 query 面板）编辑 → 与本状态无关，本册不改 table 分支；
  6. 判定与结果集不再匹配（结果集被替换）→ `Resolving` 并**丢弃**旧的判定缓存。
- **禁止**：任何只有进入、没有上述出口的死锁式只读状态。

**`Writable`**
- 进入：判定可写回。
- 状态内行为：单元格 hover 显示「可编辑」语义；双击 / 回车 / F2 进入 `Editing`。
- 退出：`Editing`；或上述 1–6 任一出口 → `Resolving`。

**`Editing(row, col)`**
- 进入：`Writable` 下用户发起编辑手势。
- 状态内行为：输入框获得焦点；输入法组合期内（composition）值不落地、不触发行身份计算；`Esc` 取消。
- 退出：提交（Enter / 失焦）→ `Dirty`；`Esc` → `Writable`（值丢弃）；结果集被替换 / 面板切换 → `Resolving` 且编辑中断（**丢弃未提交输入**，不得写入）。

**`Dirty(row, col, value)`**
- 进入：编辑提交到暂存。
- 状态内行为：单元格显示脏标记（复用分册 06 呈现）；再次编辑同一格覆盖旧值。
- 退出：提交 → `Submitting`；撤销 → `Writable`；任一重取数出口 → `Resolving` 并**清空**脏状态。

**`Submitting`**
- 进入：提交用户显式触发提交。
- 状态内行为：禁止再次编辑该格；显示进行中。
- 退出：成功 → 刷新结果集并回到 `Resolving`（重新判定）；失败 → `Failed(message)`。

**`Failed(message)`**
- 进入：计划生成或提交失败（含 `grid.result.writeDenied`、`affected == 1` 失败、`fingerprint` 不匹配）。
- 状态内行为：单元格保留**原值**（不显示用户输入的新值，避免「看起来成功了」）；展示数据库原文；提供「重新执行查询」入口。
- 退出：重新执行 / 重新编辑 → `Resolving` / `Writable`。

### 4.3 键盘、鼠标、焦点、IME

- 鼠标：单击选中（不进入编辑）；双击进入 `Editing`；`ReadOnly` 下双击**不进入编辑**，改为展示原因。
- 键盘：`Enter` / `F2` 进入 `Editing`；`Esc` 退出并丢弃；`Tab` 在 `ReadOnly` 下正常移动焦点（不得吞掉焦点，否则用户无法离开该网格）。
- 焦点：`ReadOnly` 下网格仍可获得焦点与滚动（只读 ≠ 不可用）。
- IME：组合期内不落地值；`compositionend` 之后才触发提交路径，与分册 05 一致。
- **多行选择不参与写操作定位**：查询结果面板即使存在行选择，也不提供「应用到选中行」入口。

---

## 5. 实现步骤

> **Step 1 必须是来源判定与它的测试，不许先做 UI。** 先有判定层，UI 才可能有正确的只读出口；反过来做会先产出一个「看起来能编辑」的界面。

### Step 1 · 驱动契约 + 默认实现 + 默认实现的测试

- **改哪个文件**：新增 `packages/driver-api/src/result_origin.rs`；改 `packages/driver-api/src/lib.rs`（登记模块与再导出）；改 `packages/driver-api/src/traits.rs`（追加 `describe_result_origin` 默认实现）。
- **加什么符号**：`ResultOrigin`、`UndeterminableReason`、`ResultShape`、`SetOperation`、`ResultObjectKind`、`ResultTableRef`、`ResultColumnOrigin`、`ResultOriginDetail`、`DatabaseDriver::describe_result_origin`。
- **为什么**：可写回性的全部原始输入只能来自驱动（§3.7、§3.8）。默认返回不可判定保证零回归。
- **怎么自测**：`cargo test -p datazen-driver-api --lib result_origin` —— 断言默认实现返回 `Undeterminable(NotImplemented)`；断言语义化往返（serde）不丢字段；断言 `{"kind":"undeterminable","detail":"notImplemented"}` 能正确反序列化。

### Step 2 · 宿主判定层 + 表驱动测试

- **改哪个文件**：新增 `src-tauri/src/services/result_edit.rs`；改 `src-tauri/src/services/mod.rs`。
- **加什么符号**：`evaluate_result_editability`、`ResultEditability`、`GridReadOnlyReason`、`IdentityColumn`。
- **为什么**：判定必须是**一个纯函数**——可测、可复现、无 IO，才能穷举所有形态。判定放在服务层而非命令层，命令只做桥接。
- **怎么自测**：`cargo test -p datazen --lib result_edit` —— 表驱动负例逐条（见 §9），每条断言 `reason` 精确值；正向用例断言 `identity_columns` 是「结果列名 → 源列名」而非结果列名重复。

### Step 3 · IPC 命令 + 错误前缀

- **改哪个文件**：新增 `src-tauri/src/commands/result_origin.rs`；改 `src-tauri/src/commands/mod.rs`；改 `src/commands/query.ts`（前端 wrapper）。
- **加什么符号**：`resolve_result_origin`、`preview_result_cell_change`、`ResultCellChangeRequest`；前端 `resolveResultOrigin`、`previewResultCellChange`。
- **为什么**：宿主必须在**生成 SQL 之前**做一次权威校验（§3.4 铁律）；错误前缀让前端能识别而不需要 `CommandError` 带 `code`。
- **怎么自测**：Rust 单测断言 `row_identity` 为空 / 含 `None` / `row_count != 1` / 键集合不一致 四种输入均返回 `CommandError::Validation` 且消息以对应前缀开头；断言这四种情况下**没有**任何 SQL 被渲染。

### Step 4 · 前端镜像类型 + 行级合并判定

- **改哪个文件**：新增 `src/lib/resultEditability.ts`。
- **加什么符号**：`ResultOrigin`、`ResultOriginDetail`、`ResultColumnOrigin`、`ResultTableRef`、`ServerEditability`、`GridEditability`、`GridReadOnlyReason`、`IdentityColumn`、`resolveGridEditability`、`gridReadOnlyMessageKey`。
- **为什么**：行级检查（NULL 定位键、重复行身份）只能在客户端做；表级结论必须原样采信服务端，前端不重算。
- **怎么自测**：`npx vitest run src/lib/__tests__/resultEditability.test.ts` —— 断言 NULL 键 → `keyValueNull`；两条相同身份 → `duplicateIdentity`；`rowIndex` 越界 → 只读；服务端只读时客户端**不**因数据合法而翻转为可写。

### Step 5 · 错误分类器

- **改哪个文件**：新增 `src/lib/gridErrors.ts`。
- **加什么符号**：`classifyGridError`、`GridErrorClass`。
- **为什么**：`CommandError` 到 IPC 是无 `code` 的脱敏字符串，前端只能按前缀匹配；未知前缀必须回退原文，避免把新错误吞成「未知错误」。
- **怎么自测**：`npx vitest run src/lib/__tests__/gridErrors.test.ts` —— 覆盖全部 6 个前缀；断言未知前缀回退为 `{ kind: 'unknown', message: <原文> }` 且原文一字不改。

### Step 6 · 结果网格接线（含只读出口）

- **改哪个文件**：改 `src/windows/connection/ContentViewDrawers.tsx`（228 行，有余量）；新增 `src/stores/panelResultEdit.ts`（判定缓存与状态切片）；改 `src/stores/panelStore.ts`（**只做最小接线，≤ 15 行**，因为它已 728 行）。
- **加什么符号**：`handleDetailFieldEdit` 的 `query` 分支改造；新增 `usePanelResultEdit`（或等价的 store 切片）暴露 `editability` / `resolving` / `resolveForResult` / `clearForResult`。
- **为什么**：`query` 分支今天直接调 `updateResultCell`（§2.1），必须替换为「判定 → 只读出口 或 计划管线」。新逻辑放新文件是因为 `panelStore.ts` 接近 800 行上限。
- **关键约束**：
  - `query` 分支**不得**读取 `selectedRows`；
  - 只读时**不得**调用 `updateResultCell`；
  - `updateResultCell` 从「编辑入口」降级为「提交成功后同步本地结果副本」的内部手段（或直接被重新执行取代）。
- **怎么自测**：`npx vitest run src/windows/connection/__tests__/ContentViewDrawers.resultReadOnly.test.tsx` —— 用 spy 断言只读时 `updateResultCell` 调用次数为 `0`；断言双击 / 回车 / F2 / 键入 / 粘贴都不进入编辑态；断言原因文案随 `reason` 变化。

### Step 7 · i18n

- **改哪个文件**：`src/locales/en/query.ts`（424 行，有余量）。**不改 `en.ts`**（它只有一行再导出，改它无效），不动其他语言，不新建域名。
- **加什么符号**：见 §8 的 key 清单（全部为 `query.` 前缀的点号扁平 key）。
- **为什么**：原因必须是人能看懂的话，而不是一个灰按钮。
- **怎么自测**：`pnpm typecheck`；人工检查每个 `GridReadOnlyReason` 都有对应 key。

### Step 8 · 连续旅程测试与 E2E

- **改哪个文件**：新增 `src/windows/connection/__tests__/resultGridEditability.journey.test.ts`；新增 `e2e/specs/result-grid-readonly.spec.ts`。
- **加什么符号**：旅程用例见 §9。
- **为什么**：交互逻辑禁止只测静态状态，必须覆盖残缺中间态与退出跃迁。
- **怎么自测**：`npx vitest run`；`pnpm e2e:skip-build -- --spec e2e/specs/result-grid-readonly.spec.ts`。

### Step 9 · 规模体检与收尾

- **改哪个文件**：不新增代码；核对 §6 表格中每个文件的最终行数。
- **为什么**：`panelStore.ts` 728 → 加 15 行仍在上限内；若接线超过 15 行，必须把逻辑整体搬进 `panelResultEdit.ts`，**不得**放任 `panelStore.ts` 突破 800 行。
- **怎么自测**：`wc -l` 逐个核对；`pnpm typecheck`；`cargo test -p datazen --lib result_edit`。

---

## 6. 文件级改动清单

| 文件 | 新增/修改 | 职责 | 预估行数 | 是否触及 800 行上限 |
| --- | --- | --- | --- | --- |
| `packages/driver-api/src/result_origin.rs` | **新增** | 结果来源契约类型（`ResultOrigin` 及配套枚举/结构体） | ~150 | 否（新文件） |
| `packages/driver-api/src/lib.rs` | 修改 | 登记新模块与再导出 | ~3 | 否（远低于上限） |
| `packages/driver-api/src/traits.rs` | 修改 | 追加 `describe_result_origin` 默认实现 | ~15 | **已 1020 行，本就超限**；本次仅追加带默认实现的方法，不加剧问题，拆分另立任务 |
| `src-tauri/src/services/result_edit.rs` | **新增** | 纯函数判定层 + 单测 | ~260（含 `#[cfg(test)]`） | 否（新文件） |
| `src-tauri/src/services/mod.rs` | 修改 | 登记 `result_edit` | ~2 | 否 |
| `src-tauri/src/commands/result_origin.rs` | **新增** | `resolve_result_origin` / `preview_result_cell_change` + 写前校验 | ~220（含单测） | 否（新文件） |
| `src-tauri/src/commands/mod.rs` | 修改 | 登记新命令模块 | ~2 | 否 |
| `src/commands/query.ts` | 修改 | 两个 IPC wrapper | ~25 | 否 |
| `src/lib/resultEditability.ts` | **新增** | 前端镜像类型 + `resolveGridEditability` + 文案 key 映射 | ~180 | 否（新文件） |
| `src/lib/gridErrors.ts` | **新增** | `classifyGridError` 前缀分类 | ~60 | 否（新文件） |
| `src/stores/panelResultEdit.ts` | **新增** | 判定缓存与结果网格编辑状态切片 | ~140 | 否（新文件） |
| `src/stores/panelStore.ts` | 修改 | 最小接线（暴露切片入口 / 复用 `updateResultCell`） | **≤ 15** | **实测 728 行 → 加 15 行 = 743，仍在上限内**；超过 15 行即视为失败，须整体搬入新文件 |
| `src/windows/connection/ContentViewDrawers.tsx` | 修改 | `handleDetailFieldEdit` 的 `query` 分支改为判定驱动；只读出口 | ~55 | 否（实测 228 行） |
| `src/locales/en/query.ts` | 修改 | 追加只读原因与错误文案 key | ~30 | 否（实测 424 行） |
| `src/lib/__tests__/resultEditability.test.ts` | **新增** | 行级合并判定单测 | ~140 | 否 |
| `src/lib/__tests__/gridErrors.test.ts` | **新增** | 前缀分类单测 | ~70 | 否 |
| `src/windows/connection/__tests__/ContentViewDrawers.resultReadOnly.test.tsx` | **新增** | 只读时禁止编辑的组件测试 | ~160 | 否 |
| `src/windows/connection/__tests__/resultGridEditability.journey.test.ts` | **新增** | 连续旅程（击键全流程）测试 | ~200 | 否 |
| `e2e/specs/result-grid-readonly.spec.ts` | **新增** | E2E 只读链路与「零写调用」断言 | ~120 | 否 |
| `packages/drivers/*/`（任一驱动若要实现来源能力） | **新增（驱动侧，非本册交付）** | `describe_result_origin` 的驱动实现 + 该 crate 内单测 | 视驱动 | 按各驱动现状评估；**测试必须落在该驱动 crate 内，禁止放宿主** |

---

## 7. 边界与异常清单

| # | 场景 | 期望行为 | 原因码 |
| --- | --- | --- | --- |
| B-1 | **JOIN**（`SELECT ... FROM a JOIN b ON ...`） | 只读；文案「这是多表连接的结果」 | `multiTable` |
| B-2 | **聚合**（`COUNT` / `SUM` / `GROUP BY` / `HAVING` / 窗口函数） | 只读；文案「这是聚合结果」 | `aggregate` |
| B-3 | **`DISTINCT`** | 只读。理由：去重后结果行与原表行不再一一对应，行身份可重复 | `distinct` |
| B-4 | **`UNION` / `INTERSECT` / `EXCEPT`** | 只读 | `setOperation` |
| B-5 | **子查询** | 分两种：`WHERE` / `HAVING` 中的子查询**不**因它本身只读（列来源未受影响），其余条件照常判定；`FROM (SELECT ...)` 派生表或投影中的子查询 → 只读 | `derivedSource` |
| B-6 | **视图 / 物化视图**（`objectKind != BaseTable`） | 只读；阶段一不承诺可写视图，即使视图背后是单表。放开需要驱动给出额外证据（见 Q3） | `view` |
| B-7 | **无主键表** | 只读；文案「来源表没有主键」 | `noPrimaryKey` |
| B-8 | **主键列不在结果里**（`SELECT name FROM t` 而主键是 `id`），或主键列存在但不是直通列（`SELECT id + 0 AS id`） | 只读；文案「主键列不在结果中」 | `keyNotSelected` |
| B-9 | **列有别名**（`SELECT id AS order_id, total AS amount FROM orders`） | **可写**（只要 `passthrough == true`）。写回必须用 `sourceColumn`（`id` / `total`），**绝不用**结果列名（`order_id` / `amount`） | — |
| B-10 | **表有别名**（`SELECT o.id, o.total FROM orders o`） | **可写**，前提是驱动给出真实表名 `orders`。`aliased: true` 只用于诊断；若驱动把别名 `o` 当作 `name` 返回 → 自洽性校验失败 → 只读 | `ambiguousOrigin` |
| B-11 | **`SELECT *`** | `SELECT *` 本身**不**构成可写理由。单表 `SELECT *` 且主键在结果中 → 可写；JOIN 上的 `SELECT *` → `multiTable` 只读。**判定完全依赖驱动声明，不依赖 SQL 文本** | 视情形 |
| B-12 | **结果为空** | 表级判定照常计算并展示（可写则显示可写态）；无行可编辑；不报错、不弹提示 | — |
| B-13 | **只读连接 / 只读会话** | 只读；文案「当前连接是只读的」。判定层消费 `session_writable`，接线时复用既有连接只读标志，**不新增平行概念** | `sessionReadOnly` |
| B-14 | **无权限** | 判定层**不猜**权限（不因「可能有权限」放行，也不因「可能没权限」拦截）。提交阶段由数据库拒绝 → `grid.result.writeDenied: <数据库原文>`，界面显示失败、保留原值、**绝不**显示成功 | — |
| B-15 | **行被他人改动** | `WHERE` 用的是取数时的原值快照；若原值已被改，WHERE 不命中 → `affected == 0` → 计划链路拒绝（既有 `affected == 1` 校验）。界面提示「该行已被他人修改，请重新执行查询」，保留原值 | — |
| B-16 | **定位键取值为 NULL** | 页面级判定 → 只读（`NULL` 无法参与 `=` 比较，写 WHERE 会命中 0 行或误伤） | `keyValueNull` |
| B-17 | **定位键在已取到的行内重复** | 页面级判定 → 只读。即使 `affected == 1` 能兜住，也应在判定层拒绝，避免用户在编辑后才发现 | `duplicateIdentity` |
| B-18 | **驱动谎报**（`columns` 长度与结果列数不一致 / 列名重复 / `passthrough` 为真但 `source_column` 为 `None` / 列来源表与 `detail.table` 不一致） | 整体降级 `unknownOrigin` 语义 → 只读，原因码 `ambiguousOrigin`；同时记 warn 日志，便于定位驱动缺陷 | `ambiguousOrigin` |
| B-19 | **IPC 判定失败 / 超时** | 降级为只读（**绝不**降级为可写）；不弹模态，静默保持只读 | `unknownOrigin` |
| B-20 | **多行选择 + 编辑** | 查询结果面板不提供多行写入入口；`row_count != 1` 一律拒绝 | — |
| B-21 | **多字节字符 / 超长值 / IME 组合中** | 值以字符串原样传递，不做长度截断；组合期内不落地；超长值交由数据库裁决并原文回显 | — |
| B-22 | **单行结果 / 单列结果** | 与普通结果一致；单列即主键列时仍可写 | — |

---

## 8. i18n key 清单

**领域包文件**：`src/locales/en/query.ts`（**不是** `en.ts`——它只有一行 `export { default } from './en/index'`，改它无效；也**不要**新建域名或改动其他语言）。

该文件是**点号扁平 key 的对象**（`const pack = { 'query.execute': '...', ... }`），因此下面每条都是 key 的完整字面值。

| 完整 key 路径 | 用途 | 英文文案（示意） |
| --- | --- | --- |
| `query.gridReadOnly.aggregate` | 聚合结果 | `This result is an aggregate; individual rows cannot be edited.` |
| `query.gridReadOnly.multiTable` | 多表连接 | `This result joins multiple tables; individual rows cannot be edited.` |
| `query.gridReadOnly.distinct` | DISTINCT | `This result is de-duplicated; rows cannot be matched back to table rows.` |
| `query.gridReadOnly.setOperation` | UNION 等 | `This result is a set operation (UNION); rows cannot be edited.` |
| `query.gridReadOnly.derivedSource` | 派生表 / 投影子查询 | `This result comes from a subquery; rows cannot be matched back to table rows.` |
| `query.gridReadOnly.view` | 视图 | `This result comes from a view; editing views is not supported.` |
| `query.gridReadOnly.noPrimaryKey` | 无主键 | `The source table has no primary key, so rows cannot be identified.` |
| `query.gridReadOnly.keyNotSelected` | 主键列不在结果中 | `The primary key columns are not all present in this result.` |
| `query.gridReadOnly.unknownOrigin` | 驱动不可判定 | `Cannot determine where this result came from, so it is read-only.` |
| `query.gridReadOnly.ambiguousOrigin` | 驱动信息自相矛盾 | `The driver reported inconsistent column origins for this result.` |
| `query.gridReadOnly.sessionReadOnly` | 只读连接 | `The current connection is read-only.` |
| `query.gridReadOnly.keyValueNull` | 定位键为 NULL | `This row has a NULL primary key value and cannot be located.` |
| `query.gridReadOnly.duplicateIdentity` | 行身份重复 | `This page contains duplicate row identities; editing is disabled.` |
| `query.gridReadOnly.tooltip` | 只读单元格悬停提示（带 `{reason}`） | `Read-only: {reason}` |
| `query.gridEdit.editHint` | 可编辑单元格提示 | `Double-click to edit this cell` |
| `query.gridEdit.resolving` | 判定中 | `Checking whether this result can be edited…` |
| `query.gridEdit.submitting` | 提交中 | `Saving change…` |
| `query.gridEdit.empty` | 空结果 | `This result has no rows.` |

**错误文案**（`CommandError` 到 IPC 是无 `code` 的脱敏字符串，前端按前缀分类；这些前缀由 Rust 侧构造，不是 i18n key）：

| 前缀 | 触发 |
| --- | --- |
| `grid.result.originUnavailable: ` | 来源不可判定时仍尝试写回 |
| `grid.result.originAmbiguous: ` | 来源信息自相矛盾 / 长度不匹配 |
| `grid.result.rowNotAddressable: ` | 行身份缺失 / 含 NULL / 页内不唯一 |
| `grid.result.multiRowNotAllowed: ` | `row_count != 1` |
| `grid.result.writeDenied: ` | 数据库权限拒绝 |
| `grid.cell.readOnly: ` | 判定为只读时的编辑拒绝 |

前端 `classifyGridError()`（新增）按上述前缀匹配；未知前缀**回退原文**，不得吞掉信息。

---

## 9. 测试清单

### 9.1 Rust 单测

| 用例名 | 断言要点 |
| --- | --- |
| `default_impl_is_undeterminable` | `describe_result_origin` 默认返回 `Undeterminable(NotImplemented)` |
| `result_origin_serde_roundtrip` | `tag = "kind"` + `content = "detail"` 线上形态往返不丢字段；`{"kind":"undeterminable","detail":"notImplemented"}` 可反序列化 |
| `evaluate_single_base_table_with_pk_is_writable` | `writable == true`；`identity_columns` 是「结果列名 → `source_column`」 |
| `evaluate_rejects_alias_as_table_name` | 驱动把别名当真实表名 → `ambiguousOrigin` |
| **负例表驱动**（逐条，见 9.1.1） | 每条断言 `writable == false` **且** `reason` 精确相等 |
| `preview_rejects_empty_row_identity` | `CommandError::Validation`，前缀 `grid.result.rowNotAddressable: `，**且未渲染任何 SQL** |
| `preview_rejects_null_identity_value` | 同上 |
| `preview_rejects_multi_row` | 前缀 `grid.result.multiRowNotAllowed: ` |
| `preview_rejects_identity_key_mismatch` | `identity_columns` 键集合与 `row_identity` 不一致 → 拒绝 |
| `fingerprint_mismatch_rejects_commit` | 复用既有链路：指纹不匹配必须拒绝，不得重算（回归保护） |
| `affected_rows_not_one_rejects_commit` | 复用既有链路：`affected != 1` 整批失败（回归保护） |

#### 9.1.1 表驱动负例（**必含**，把所有不可写回的 SQL/结果形态各列一条）

每条用例给一份 `ResultOrigin`（模拟驱动输出）与 `primary_keys`，断言 `writable == false` 且 `reason` 精确：

| # | 形态 | 期望 reason |
| --- | --- | --- |
| N-1 | JOIN（`shape = MultiTable`，`referenced_tables = 2`） | `multiTable` |
| N-2 | 聚合（`aggregated = true`） | `aggregate` |
| N-3 | `DISTINCT`（`deduplicated = true`） | `distinct` |
| N-4 | `UNION`（`set_operation = Union`） | `setOperation` |
| N-5 | 派生表 / 投影子查询（`has_derived_source = true`） | `derivedSource` |
| N-6 | 视图（`object_kind = View`） | `view` |
| N-7 | 物化视图（`object_kind = MaterializedView`） | `view` |
| N-8 | 无主键表（`primary_keys` 为空） | `noPrimaryKey` |
| N-9 | 主键列不在结果列中 | `keyNotSelected` |
| N-10 | 主键列在结果中但 `passthrough = false`（表达式/函数包装） | `keyNotSelected` |
| N-11 | 驱动返回 `Undeterminable(NotImplemented)` | `unknownOrigin` |
| N-12 | 驱动返回 `Undeterminable(UnsupportedDialect)` | `unknownOrigin` |
| N-13 | `columns` 长度与结果列数不一致 | `ambiguousOrigin` |
| N-14 | `columns` 含重复列名 | `ambiguousOrigin` |
| N-15 | `passthrough = true` 但 `source_column = None` | `ambiguousOrigin` |
| N-16 | 某直通列的来源表与 `detail.table` 不一致 | `ambiguousOrigin` |
| N-17 | `shape = SingleBaseTable` 但 `referenced_tables = 2`（自相矛盾） | `ambiguousOrigin` |
| N-18 | `shape = SingleBaseTable` 但 `table = None` | `ambiguousOrigin` |
| N-19 | `shape = Unknown` | `unknownOrigin` |
| N-20 | `session_writable = false`（即使其他条件全满足） | `sessionReadOnly` |
| N-21 | 主键列是别名列且 `source_column` 给出（正向：**可写**） | `writable == true`，且 `identity_columns[].sourceColumn` 不等于结果列名 |
| N-22 | 表有别名且驱动给出真实表名（正向：**可写**） | `writable == true`，`table.name` 为真实表名 |

> N-21 / N-22 是混在负例表里的**正向对照**：用来证明「别名不必然只读，错的是按别名定位」。

### 9.2 前端单测

| 用例名（文件） | 断言要点 |
| --- | --- |
| `resolveGridEditability` 服务端只读时一律只读（`resultEditability.test.ts`） | 即使行数据完美，也不得翻转成可写 |
| `resolveGridEditability` NULL 定位键（`resultEditability.test.ts`） | `keyValueNull` |
| `resolveGridEditability` 重复行身份（`resultEditability.test.ts`） | `duplicateIdentity` |
| `resolveGridEditability` `rowIndex` 越界（`resultEditability.test.ts`） | 只读，不抛异常 |
| `service 判定与页面判定合并为 AND`（`resultEditability.test.ts`） | 任一侧只读 → 整体只读 |
| `classifyGridError` 六个前缀（`gridErrors.test.ts`） | 每个前缀映射到对应类别 |
| `classifyGridError` 未知前缀回退原文（`gridErrors.test.ts`） | 返回原文，一字不改 |

### 9.3 组件测试

| 用例名（`ContentViewDrawers.resultReadOnly.test.tsx`） | 断言要点 |
| --- | --- |
| 只读时双击不进入编辑态 | 无输入框出现；`updateResultCell` spy 调用次数 `0` |
| 只读时回车 / F2 / 直接键入 / 粘贴均无效 | 四种手势逐个断言；`stageCellChange` 调用次数 `0` |
| 只读原因随 reason 变化 | 12 个 reason 逐个渲染，断言取到对应 key 的文案 |
| 只读时 `selectedRows.size > 1` 不触发 `applyColumnToRows` | `applyColumnToRows` spy 调用次数 `0` |
| 判定未回来（`Resolving`）时保持只读 | 不进入编辑态 |
| `query` 分支不读 `selectedRows` | 断言渲染路径未依赖选择状态 |

### 9.4 连续旅程测试（Journey）

文件 `src/windows/connection/__tests__/resultGridEditability.journey.test.ts`，模拟击键全过程，逐步断言状态跃迁：

- **J-1 只读旅程**：执行 SQL → `Resolving` → `ReadOnly(multiTable)` → 点击单元格（选中，不编辑）→ 双击（仍不编辑，出现原因）→ 键入字符（值不落地）→ `Tab` 移出焦点（焦点可达）→ 切换结果 tab → `Resolving` → 回到 `ReadOnly` 且原因提示文案已刷新 → 关闭面板 → 状态销毁。
- **J-2 可写旅程**：`Resolving` → `Writable` → 双击 → `Editing` → IME 组合中输入（值不落地）→ `compositionend` → Enter → `Dirty` → 提交 → `Submitting` → 提交失败（`grid.result.writeDenied: `）→ `Failed`，断言单元格显示**原值**、未显示成功、未调用 `updateResultCell`。
- **J-3 退出跃迁**：`ReadOnly` 下依次触发 §4.2 列出的 6 个出口，每次断言状态回到 `Resolving` 且原因提示不残留。
- **J-4 重取数与选择解耦**：`Writable` 下选中若干行 → 重新执行查询 → 断言选择状态被清空（与分册 01 结论一致）且编辑态被丢弃。
- **J-5 残缺中间态**：判定请求进行中用户已开始双击；判定返回只读时断言编辑被撤销、值不落地。

### 9.5 E2E

文件 `e2e/specs/result-grid-readonly.spec.ts`：

- 在只读形态下对单元格执行完整编辑手势，断言 `preview_pending_changes` 与 `commit_pending_changes` 调用计数为 `0`；
- 断言界面未出现「成功」提示；
- 断言只读原因文案可见且非空；
- 断言数据库内容未变化（用查询面板重新执行同一 SQL 比对）。

所有测试命令的长输出**重定向到系统临时目录**再 `tail` / `grep` 取结论行，并单独打印退出码。

---

## 10. 自查清单

| # | 错误做法 | 正确做法 | 后果 |
| --- | --- | --- | --- |
| 1 | **以为 `SELECT *` 就能写回**，看一眼 SQL 是 `SELECT *` 就放行编辑 | `SELECT *` 只是文本事实。可写回必须由驱动声明「单基表 + 直通列 + 主键列在结果中」 | `SELECT * FROM a JOIN b` 被放行 → 一条 UPDATE 误伤多行；`affected == 1` 会拦下一部分，但拦不住「恰好命中一行」的假成功 |
| 2 | **忽略列别名导致定位错列**：按结果列名（`order_id` / `amount`）写回 | 写回一律用 `ResultColumnOrigin.source_column`（`id` / `total`）；结果列名只用于 UI 对齐与展示 | 生成 `SET order_id = ...`：轻则报「列不存在」，重则改到同名的另一列上，静默改错数据 |
| 3 | 把表别名当成表名写进 UPDATE | 用 `ResultTableRef.name` 的**真实表名**；`aliased` 只用于诊断 | `UPDATE o SET ...` 报错或匹配到同名对象，数据写进错误的表 |
| 4 | 判定失败 / IPC 超时后「先放行，等报错再说」 | **失败一律降级为只读**；只有明确证明可写才放行 | 判定服务一抖动，全体结果网格变成可写，等于把安全门完全打开 |
| 5 | 只在 UI 上把单元格变灰，宿主仍接受写请求 | 服务端在生成 SQL 之前独立复验（行身份非空、无 NULL、`row_count == 1`、键集合一致） | 绕过 UI 直接调 IPC 就能写入；UI 不是安全边界 |
| 6 | 用 `rowIndex` 或 `selectedRows` 定位要写的行 | 行身份只从「结果行的原始值 + 驱动来源信息」构造；选择状态永不参与写定位（契约 §4.3） | 翻页/排序后 `rowIndex` 平移，改写「新页第 3 行」而不是用户看到的那一行 |
| 7 | 继续让 `updateResultCell` 作为编辑入口 | 它降级为「提交成功后同步本地副本」；编辑入口改为判定驱动的计划管线 | 界面显示成功、数据库没有、重新执行即消失——就是今天 DB-14 的缺陷本体 |
| 8 | 把不可写回统一成一句「此结果不可编辑」 | 区分「聚合结果 / 多表连接 / 无主键 / 无法确定来源 / 主键列不在结果中 / 只读连接」等 12 个原因码 | 用户无法判断「是我查错了，还是功能不支持」，只能反复试错 |
| 9 | 自己写正则解析 SQL 判断 JOIN / 聚合 | **不解析**：来源是驱动契约，默认实现说「不知道」 | 任何启发式都会在 CTE / 派生表 / 方言语法上破产；判断错一次就是误伤多行的 UPDATE |
| 10 | 为了「省一次 IPC」缓存可写回判定且写前不复验 | 写回**前**必须重新判定（schema 可能已变）；缓存只用于渲染 | 缓存过期后按旧结论写回，`source_column` 已不存在或语义已变 |
| 11 | 让前端也实现一份表级判定 | 表级判定只在 Rust 算，前端只渲染；只有行级纯数据检查（NULL / 重复）在客户端 | 两份实现漂移出「前端说能写、后端说不能」，用户看到可编辑却总是失败 |
| 12 | 把 `primary_keys` 判定写成「遍历列上的 `is_primary_key` 标志」 | 读 `CachedColumns.primary_keys`（已由 `effective_primary_keys()` 归一化） | 驱动填了 `primary_keys` 却没打列标志时，把有主键的表判成无主键 → 本可编辑的结果被误判只读 |

---

## 11. 未决问题

| # | 问题 | 建议 |
| --- | --- | --- |
| **Q-1** | **本册是否只做「明确禁止并解释」这一步？** 即阶段一只交付「全部只读 + 12 类原因」，把真正的写回执行整体推到下一册 | **建议分两步走但同册交付判定层**：Step 1–5（驱动契约 + 判定层 + 前端只读化 + 解释）为本册硬交付；Step 6 中「可写时的计划管线接线」作为**条件交付**——仅当至少有一个驱动实现了 `describe_result_origin` 才启用；在没有任何驱动实现的当下，产品行为就是「全只读 + 解释」。理由：今天没有任何驱动能可靠回答来源问题（§2.3、§2.4），先让判定层与解释落地，收益立即可得且零风险；写回执行没有驱动支撑时无法被真实验证 |
| **Q-2** | **来源信息走哪种 IPC 形态？** (A) 在查询结果封套上追加可选字段，随执行结果一并返回；(B) 新增带默认实现的驱动方法 `describe_result_origin(handle, db_session_id, sql)`，宿主按需调用 | **建议 B**。理由：(1) 与 `00-contracts.md` C-3 的「只新增带默认实现的方法、不提升 `PROTOCOL_VERSION`」一致，对既有执行/流式 DTO 零侵入；(2) 按需调用天然支持「写前复验」；(3) 不碰 `ColumnSchema`，避免来源信息污染 schema 树。代价是多一次 IPC 往返与驱动侧一次重新描述，可接受。若后续证明「重新描述」在某些驱动上不可行（例如只能从 statement handle 取元数据），再改为 A |
| **Q-3** | **视图是否永远只读？** | 建议阶段一恒只读，并要求驱动在 `ResultObjectKind` 上如实报告。放开需要驱动额外给出「该视图可更新 + 底层映射到单基表 + 主键来源」三项证据，属于新能力，另立分册 |
| **Q-4** | **唯一键能否作为定位键？** 现实中大量表无主键但有唯一非空索引 | 建议阶段一只接受主键（`CachedColumns.primary_keys`）。理由：唯一键的「非空 + 唯一」约束在既有 `SchemaCache` 里不可得，自行查询会引入新的缓存失效面；且唯一键可能可空，`NULL` 参与 `=` 的语义在各方言不一致。放开需先扩展 schema 缓存 |
| **Q-5** | **判定结果要不要缓存、缓存多久？** | 建议：**内存缓存，仅用于渲染**；任何写回前必须复验。缓存随结果集替换、SQL 重新执行、结果 tab 切换、面板关闭失效；不落盘、不跨会话复用 |
| **Q-6** | **`WHERE` 里的子查询是否真的安全？** 本册按「列来源未受影响」判为不置位 `has_derived_source` | 建议保持当前结论（B-5），但要求驱动在文档中明确其子查询检测范围，并在负例表里各留一条 `WHERE` 子查询的正向用例，防止驱动实现者把「任何子查询」都置位导致过度只读 |
| **Q-7** | **只读原因用哪种呈现？** 单元格级 tooltip / 结果网格顶部一次性横幅 / 状态栏提示 | 建议：**单元格 hover tooltip + 首次进入只读结果时的一次性轻量提示**，不用模态。理由：原因需要「可发现但不可打扰」；模态会打断「连续执行多条 SQL」的主流程 |
