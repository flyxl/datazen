# 04 · 新增行 INSERT（DB-04）

> **状态**：目标设计，**未实现**（本册只冻结设计，不改代码）。
> **依赖**：`00-contracts.md` —— §1.4（提交链路与五个必须记住的事实）、§3（`CellWrite` 三态契约与校验表）、§5.1（`build_insert_sql` / `insert_returning_clause` 最终签名，已冻结）、§8（错误前缀机制 `grid.<域>.<原因>: `）。
> **被谁依赖**：`10-result-grid.md`（DB-14 查询结果网格可编辑，依赖 01/02/03/**04**/05/06 全部就位）。`03-clipboard.md` 的 `Paste rows` 无区域路径复用本册 INSERT 管线。
> **预估人日**：4.0 人日（拆分 `data.rs` 0.5 / `driver-api` 契约与默认实现 0.5 / 宿主计划与执行 1.0 / 驱动回填覆盖 0.5 / 前端 store 与 UI 1.0 / 测试 0.5）。
> **不包含**：类型感知编辑器与 `Set Value`（NULL / DEFAULT / NOW() 快捷项，属 05）；粘贴矩阵生成新增行计划（属 03，只复用本册管线）；多行 `VALUES` 批量拼接（宿主只产出单行语句，批量语义归驱动覆盖）；`LAST_INSERT_ID()` 类「插入后再查一次」的回填（契约 §5.1 没有该入口，见 §11 Q-2）；无主键表的 UPDATE/DELETE（仍然禁止，本册只放开 INSERT）。

---

## 1. 目标与验收口径

**一句话目标**：让用户在数据网格里新增草稿行，并把草稿行作为 **INSERT** 语句与既有 UPDATE / DELETE 一起走「预览 → 指纹 → 确认」提交链路，落在**同一个事务**里，成功后被正确回填或明确降级，且**绝不会出现「界面显示成功、库里什么都没有」**。

### 1.1 验收标准（命令 + 期望输出）

| # | 命令 | 期望输出 / 期望结果 |
| --- | --- | --- |
| A1 | `cargo test -p datazen --lib` | 退出码 `0`；`test result: ok`；新增用例名含 `insert` 的全部出现在通过列表 |
| A2 | `cargo test -p datazen-driver-api --lib` | 退出码 `0`；`sql_text` 与 `traits` 的 `build_insert_sql` 默认实现用例全部通过 |
| A3 | `cargo test -p datazen-driver-postgres --lib` / `-p datazen-driver-mysql` / `-p datazen-driver-sqlite` / `-p datazen-driver-sqlserver` | 退出码均为 `0`；每个 crate 至少有 1 个 INSERT 三态断言（`NULL` vs `DEFAULT` vs 空串） |
| A4 | `npx vitest run src/lib/__tests__/tableChanges.test.ts src/stores/__tests__/tableDataStore.test.ts` | 退出码 `0`；`Test Files` 全通过；含草稿行计数与 `inserts` 上线用例 |
| A5 | `pnpm typecheck` | 退出码 `0`，**无 `error TS`**（测试文件也在检查范围内） |
| A6 | `pnpm e2e:minimal` | 退出码 `0`；`e2e/specs/table-insert.ts` 用例全部 `passed` |

### 1.2 可验证的行为口径

1. 在无主键表上点「+ 行」→ 可以编辑、可以提交；同表的既有行双击仍然不进入编辑，`UPDATE`/`DELETE` 仍被拒绝（`tableData.noPrimaryKey`）。
2. 只在自增主键列留空（`Unset`）、其余列填值 → 提交后新行出现且主键由数据库分配；语句里**不出现**该自增列。
3. 提交一批「2 插入 + 1 更新」→ 后端 `affected_rows == 3`、`statements` 顺序为 `insert, insert, update`；任一条失败时**整批回滚**，插入的行不残留。
4. `insert_returning_clause` 返回 `None` 的驱动 → 提交后界面**不承诺**新行保持高亮，刷新当前页并给出降级提示。
5. 该 `dbSessionId` 上已有用户手动事务时提交 → 后端**不** `begin_transaction`/`commit`（`MockDriver::open_transaction_count()` 不增长），插入与更新并入用户事务。
6. 回归证明：`inserts` 非空、`updates`/`deletes` 均为空时，`execute_row_change_plan_impl` **必须真的执行语句**（`MockDriver::execute_calls() == 1`），不得提前返回成功。

---

## 2. 现状代码事实

> 本节只列**我亲自读过**的符号。**不含行号**，不做推测。凡「没有」的结论都由 grep 给出。

### 2.1 `src-tauri/src/commands/data.rs`（现状 **1331 行**，见 §6）

**DTO（全部已存在）**：`CellUpdate`、`RowUpdateBatch`、`RowDeleteBatch`、`RowChangeTableContext`、`PendingRowChange`、`PlannedStatement`、`ChangeWarning`、`RowChangePlan`、`CommitPendingChangesRequest`、`RowCommitStatementResult`、`CommitPendingChangesResponse`。

**关键事实（逐条已核）**：

1. `RowChangePlan` 的字段只有 `plan_id` / `fingerprint` / `table` / `updates` / `deletes` / `warnings`，**没有 `inserts`**。
2. `execute_row_change_plan_impl` 的**第一个判断**是 `if plan.updates.is_empty() && plan.deletes.is_empty()` → 直接 `Ok(... affected_rows: 0 ...)`，**不开启事务、不执行任何语句**。加 `inserts` 后这里必须同步扩展（§5 Step 4）。
3. `execute_row_change_plan_impl` 的事务归属：先看 `state.session_transactions.lock().await.contains_key(db_session_id)` 得到 `session_open`；`session_open == false` 时尝试 `driver.begin_transaction(handle)`，返回 `Err(DriverError::TransactionError(_))` 时退化为 `driver.execute(handle, "BEGIN")`；成功且非 `session_open` 时 `driver.commit(tx)` 或 `"COMMIT"`，失败时回滚。`session_open == true` 时**两条返回路径都不提交、不回滚**，直接返回结果/错误。
4. 该函数对 `updates` 与 `deletes` 各要求 `affected == 1`，否则 `CommandError::Validation`，文案为 `UPDATE for one row identity affected {affected} rows; refusing ambiguous write`（DELETE 同构）。语句收集容量是 `plan.updates.len() + plan.deletes.len()`。
5. `build_row_change_plan(driver, table, changes)` 只遍历 `canonicalize_changes(changes)` 的结果；对每条变更按 `delete_marked` 只调用 `driver.build_delete_sql` 或 `driver.build_update_sql`；行内以 `current_values` + `changed_columns` 组装 `set_columns: Vec<(&str, Option<Value>)>`；`parameter_summary` 由 `SET <列>=<值摘要>` 与 `PK <列>=<值摘要>` 拼接；随后追加 `delete-rows` 之类的 `ChangeWarning`。
6. `changes_fingerprint(table, changes: &[PendingRowChange])` 用**函数内局部结构体 `FingerprintPayload { table, changes }`** 做 `serde_json::to_vec` 再 `Sha256`。指纹**只覆盖该 payload**，正文 SQL 与 `parameter_summary` 不参与。
7. `validate_immutable_plan(driver, db_session_id, plan, fingerprint)`：校验 `plan_id` 非空、`table.db_session_id` 一致、`plan.fingerprint == fingerprint`，然后**用 `plan_changes(plan)` 从不可变的变更元数据重建整张计划**，要求 `rebuilt.fingerprint == fingerprint`，最后把 `plan_id` 还原。**重建是信任边界**：客户端给的 `sql_template` / `parameter_summary` 只是展示字段。
8. `plan_changes(plan: &RowChangePlan) -> Vec<PendingRowChange>` **只链 `updates` 与 `deletes`**。
9. `canonicalize_changes` 会：校验 `row_identity` 非空且非 NULL（`identity_key`）、排序并去重 `changed_columns`、**丢掉 `changed_columns` 为空且非删除的行**、校验 `effective_identity` 无重复/碰撞、最后按 `identity_key` 排序。→ **插入行没有身份，不能走这个函数**。
10. `identity_key` 对空 map 直接报 `Row changes require primary-key identity`。
11. `preview_pending_changes_impl(state, table, changes)` 只取 `state.connection_manager.get_session(&table.db_session_id)` 拿到 `(driver, _handle)`，然后调用 `build_row_change_plan`；注释明确「preview 对数据库是纯的：不开事务、不执行语句」。**它目前拿不到任何列元数据。**
12. `commit_pending_changes_impl` 在调用执行前做了：读 session/driver/handle、`get_session_config`、`owner_connection_id` 与 `table.connection_id` 比对、`driver_type` 双比对、`database` 比对、显式 `schema` 比对、`read_only` 拒绝，然后 `validate_immutable_plan`。**只读拒绝的文案是 `Connection is read-only; row changes are not allowed`（没有 `grid.` 前缀）。**
13. `#[cfg(test)] mod tests` 使用 `crate::testing::app_state::TestAppState`，现有用例包括 `preview_builds_driver_sql_without_opening_a_transaction`、`commit_reuses_plan_fingerprint_and_returns_plan_id`、`commit_rejects_stale_fingerprint_before_execution`、`preview_rejects_changes_without_primary_key_identity`、`canonicalize_rejects_null_and_duplicate_composite_identities`、`canonicalize_rejects_composite_current_identity_collision`、`commit_row_updates_joins_open_session_transaction` 等。`commit_reuses_plan_fingerprint_and_returns_plan_id` 的写法是 `options.execute_rows_affected = 1` 后断言 `statements.len() == 2`、`affected_rows == 2`。
14. **所有现有测试都通过 `preview_pending_changes_impl(&state, context, vec![...])` 三参形式调用**——本册要给它加第 4 个参数，这些调用点必须同步改（§5 Step 3）。

### 2.2 `packages/driver-api`（契约层）

- `packages/driver-api/src/traits.rs`：`DatabaseDriver` trait 已声明 `quote_char`、`quote_ident`、`skip_count_query`、`supports_offset`、`pagination_syntax`、`supports_explain`、`has_schema_level`、`has_multi_database`、`default_schema`、`ddl_atomicity`、`format_sql_literal`、`build_update_sql`、`build_delete_sql`、`query`、`query_multi`、`execute`、`begin_transaction`、`commit`、`rollback`；文件顶部有 `mod sql_text;`。**`build_insert_sql` / `insert_returning_clause` 均不存在。**
- `packages/driver-api/src/traits/sql_text.rs`：`quote_ident`（按 `quote_char` 选 `` ` `` 或 `"`，并把引号字符翻倍）、`format_sql_literal`（`None | Some(Null)` → `NULL`，字符串/时间戳/JSON 都先 `replace('\\', "\\\\")` 再 `replace('\'', "''")`，Bytes → `X'…hex…'`）、`build_update_sql`（`UPDATE {q(table)} SET {…} WHERE {…}`，列名与值分别经 `driver.quote_ident` / `driver.format_sql_literal`）、`build_delete_sql`（同构 `DELETE FROM {q(table)} WHERE {…}`）、`pagination_syntax`、`qualified_sql`、`split_restore_sql`，以及私有 `bytes_to_hex`。
- `packages/driver-api/src/reuse.rs`：`ReuseDriver`（MySQL 协议族 / PostgreSQL wire 的兼容包装器）**逐方法转发**给 `inner`，其中就有 `pagination_syntax`、`format_sql_literal`、`build_update_sql`、`build_delete_sql`。→ **新增 trait 方法如果不在 `ReuseDriver` 里补一行转发，包装驱动就会丢掉内层驱动的覆盖。**
- `packages/driver-api/src/types.rs`：`Value`（`#[serde(untagged)]`，`Null`/`Bool`/`Integer`/`Float`/`String`/`Bytes`/`Timestamp`/`Json`，`Default = Null`）、`ColumnSchema { name, data_type, nullable, default_value: Option<String>, comment, is_primary_key, is_auto_increment }`、`TableSchema::effective_primary_keys()`、`TableDataResult`、`PaginationSyntax`。**`CellWrite` 不存在。**
- `packages/driver-api/src/lib.rs`：`PROTOCOL_VERSION: u32 = 4`、`MIN_PROTOCOL_VERSION: u32 = 1`；`packages/driver-api/src/capabilities.rs` 的判定是「低于 MIN 拒绝 / 高于 PROTOCOL 拒绝 / 区间内接受」。
- `packages/driver-api/src/mock_driver.rs`：`MockDriver` 与 `MockDriverOptions`；`execute_rows_affected` **默认为 0**；访问器有 `execute_calls()`、`commit_calls()`、`open_transaction_count()`、`query_calls()`，以及 `set_table_schema_for_test` / `add_table_for_test`；`query_rows` 可配置返回行。

### 2.3 驱动覆盖现状（grep 结论）

- **没有任何 path 驱动覆盖 `build_update_sql` / `build_delete_sql`**（`grep -rn 'fn build_update_sql\|fn build_delete_sql' packages/drivers/` 无命中）。
- **只有一个 path 驱动覆盖 `pagination_syntax`**：`packages/drivers/sqlserver/src/sqlserver.rs::pagination_syntax`（`OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY` + `requires_order_by: true` + `order_by_fallback: Some("(SELECT NULL)")`）。
- 我读的第 2 个「覆盖实现」是包装层 `packages/driver-api/src/reuse.rs::ReuseDriver::{pagination_syntax, build_update_sql, build_delete_sql, format_sql_literal}`（真实在产代码路径，不是测试替身）。
- 另外读了两个**叶子**覆盖，它们决定 INSERT 的引号与字面量渲染：`packages/drivers/postgres/src/postgres.rs::format_sql_literal`（Bytes → `'\x<hex>'`，字符串只翻倍单引号、**不**转义反斜杠）、`packages/drivers/mysql/src/mysql.rs::{quote_char, format_sql_literal}`（`` ` `` 引号、布尔 → `1`/`0`、字符串先转义反斜杠、Bytes → `X'…'`）。
- grep `RETURNING` / `LAST_INSERT_ID` / `SCOPE_IDENTITY` 在 `packages/drivers/**` 与 `src-tauri/src/**` **无命中**：主键回填是全新能力。

### 2.4 宿主列元数据入口

- `src-tauri/src/commands/mod.rs`：`AppState.schema_cache: Arc<SchemaCache>`。
- `src-tauri/src/cache/schema_cache.rs`：`SchemaCache::get_columns(connection_id, database, schema: Option<&str>, table, driver, handle) -> Result<CachedColumns, DriverError>`（先查全 schema 缓存，再查列缓存，未命中才调 `driver.get_columns`）；`CachedColumns { columns, primary_keys, table_name, cached_at }`。
- `src-tauri/src/testing/app_state.rs`：`TestAppState::{new, with_options, with_tables, save_connection, connect_config, save_and_connect}`、`rich_mock_options()`（**注意：它没有配置 `columns`，`columns` 为空**）、`sample_postgres_config`。

### 2.5 前端现状

- `src/lib/tableChanges.ts`：`RowIdentity`、`TableChangeContext`、`PendingRowChange`（`rowIndex?` / `rowIdentity` / `originalValues` / `currentValues` / `changedColumns` / `deleteMarked`）、`PlannedStatement`、`ChangeWarning`、`RowChangePlan`、`CommitStatementResult`（`operation: 'update' | 'delete'`）、`CommitPendingChangesResponse`、`PendingStatus`、`CommitPendingChangesResult`、`isCompleteTableChangeContext`、`tableChangeContextKey`、`buildRowIdentity`、`rowIdentityKey`、`duplicateRowIdentityKeys`、`valuesEqual`、`clonePendingRowChange`（私有 `valueForColumn` / `stableSerialize` / `isStableIdentityValue`）。**没有 `CellWrite`，没有草稿行概念。**
- `src/stores/tableData/pendingChanges.ts`：`AMBIGUOUS_ROW_IDENTITY_ERROR`、`effectivePendingIdentity`、`rowIdentityIsUnique`、`hasPendingIdentityCollision`、`findPendingForRow`、`rebuildEditBuffer`、`overlayPendingRows`、`pendingChangesSignature`、`pendingChangesForWire`。**`overlayPendingRows` 与 `rebuildEditBuffer` 都先按主键构造身份，主键列为空时直接返回** —— 草稿行不能塞进这套结构。
- `src/stores/tableData/types.ts`：`TableState { context, columns, rows, totalRows, page, pageSize, filters, filterLogic, draftFilters, draftFilterLogic, filterPanelOpen, sorts, editBuffer, pendingChanges: Map<string, PendingRowChange>, rowIdentityAnchors, previewPlan, pendingStatus, selectedRows, lastSelectedIndex, editingCell, detailRowIndex, loading, requestRevision, loadingRevision, error, visibleColumns }`。
- `src/stores/tableData/connectionState.ts`：`toCellValue`、`editKey`。
- `src/stores/tableDataStore.ts`：`TableDataStore` 接口（`stageCellChange` / `stageRowDelete` / `deleteRows` / `rollbackPendingChanges` / `previewPendingChanges` / `commitPendingChanges` / `loadTableData` / `reloadPanel` / `invalidateCachedData` …）；`stageCellChange` 在 `pkCols.length === 0` 时报 `tableData.noPrimaryKey` 并 return；`previewPendingChanges` 在 `ts.pendingChanges.size === 0` 时直接 `return null`；`commitPendingChanges` 同样以 `pendingChanges.size === 0` 判空；两者都用 `pendingChangesSignature` 做「预览期间是否又改了」的一致性检查；提交成功后清空 `pendingChanges` 并 `loadTableData`。
- `src/commands/database.ts`：`databaseCommands.previewPendingChanges({ context, changes })` → `invoke<RowChangePlan>('preview_pending_changes', ...)`、`databaseCommands.commitPendingChanges({ dbSessionId, plan, fingerprint })`；`PreviewPendingChangesRequest { context, changes }`。
- `src/components/DataTable/VirtualBody.tsx`：`VirtualBody`、`VirtualBodyProps { columns, rows, rowHeight, editingCell, selectedRows, highlightedRow?, scrollElement, columnWidths?, onCellDoubleClick, onCellEdit, onCellEditCancel, onRowSelect }`。
- `src/windows/connection/TableView.tsx`：`isEditable = !isConnectionReadOnly`（`readOnlyProp ?? (savedConnection?.readOnly || driverReadOnly)`）、`showReadOnlyTip()`、以及三处 `pendingChanges.size === 0` 的门闸。
- `src/locales/en/query.ts`：字段名以 `tableView.*` / `query.*` / `tableData.*` 开头的英文领域包（`const pack = {...} as const; export default pack;`）；已有 `tableData.noPrimaryKey`、`tableData.commitFailed`、`tableData.pendingChanges`（`{count} unsaved changes`）、`tableData.confirmCommit`（`Commit {updates} updates and {deletes} deletes?`）、`tableData.readOnlyEditDisabled`、`tableData.previewTitle` 等。
- `src/locales/en/index.ts`：`...query` 等域名包被展开进 en 字典。
- `src/locales/index.ts`：`export type I18nKey = TranslationKey | (string & {})` —— 新增 key 不会因 `TranslationKey`（由 `zh-CN` 派生）而 typecheck 失败，但**拼错的 key 也不会报错**（§8 要求测试兜住）。
- `src/lib/gridErrors.ts` **在基线中不存在**；其**创建归属已由契约 §8.1 归属总则冻结给 01**（01 是提交顺序里最先合并的分册）。本册合并时该文件必然已存在，因此本册**只有「追加」这一种合法动作**：按 §8.1 三步追加 `grid.insert.*` 码，**禁止整文件重写**（整文件覆盖不产生编译错误，只会静默抹掉其他分册的码）。原 §11 Q-5 的归属冲突**已关闭**，见该条。

### 2.6 权威契约与 PRD 原文

- `00-contracts.md` §1.4 的五个事实、§3.5「`build_insert_sql` 默认实现的 4 条语义要求」、§3.6「宿主必须拒绝的组合」、**§4.5 写路径硬约束 W-1…W-6（本册的主契约，Step 1 的迁移清单与 §7 的第 17–20 条都引它）**、§5.1 冻结签名、§8.2 错误码清单与文案纪律、§9.1 共用文件归属与追加纪律、§10 本册模板。
- `docs/prd/data-browsing-optimization-prd.md` DB-04 原文：入口为「工具条 `+ 行` / `⌘/Ctrl+I` / 双击最后一行下方空白区」；草稿行可编辑所有非自增列、自增与有默认值的列显示 `DEFAULT`；支持多行草稿、`⌘+D` 复制、`Delete` 删除；提交与 UPDATE/DELETE 同事务同流程、INSERT 影响行数同样要求为 1；**顺序固定 inserts → updates → deletes**；**无主键表允许插入**；`VirtualBody` 渲染 `data-testid="draft-row-N"`；驱动专属测试落 postgres / mysql / sqlite / sqlserver。
- **签名冲突提示**：PRD 原文写的是 `fn build_insert_sql(&self, table: &str, columns: &[(String, CellWrite)])`，而 `00-contracts.md` §5.1 冻结的是 `columns: &[(&str, CellWrite)]`。**以 00 为准**（00 明确「以下方法签名是最终形态，分册不得改动」）。

---

## 3. 数据结构与接口设计

### 3.1 `CellWrite`（Rust，新增，落在 `packages/driver-api/src/types.rs`）

```rust
/// 一次列写入的意图。三态是刻意的：`Null` 与 `Unset` 在 SQL 里是两件不同的事。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum CellWrite {
    /// 该列出现在语句中，写入 SQL NULL：`SET c = NULL` / `INSERT ... VALUES (NULL)`。
    Null,
    /// 该列出现在语句中，写入此值。
    Value(Value),
    /// 该列**不出现在语句中**：
    /// - UPDATE：这一列不被触碰。
    /// - INSERT：由数据库自己填（DEFAULT / 自增 / 生成列 / 触发器）。
    Unset,
}
```

| 字段 | 用途 | 缺省 |
| --- | --- | --- |
| `kind`（serde tag） | 判别三态，线上形如 `{"kind":"unset"}` | 无缺省，**必填** |
| `value`（serde content） | 仅 `kind == "value"` 时存在 | 其余两态不带该键 |

- 只派生 `PartialEq`（`Value` 含 `f64`，不能 `Eq`）。
- `Unset` 不携带任何值：这正是「DEFAULT」在协议上的唯一表达，**不要**新增 `Default` 变体（契约 §3.1 已定名 `Unset`）。
- `#[serde(tag, content)]` 与 `Value` 的 `untagged` 叠加是安全的（只在外层判别）；**禁止**改成 `untagged`（契约 §3.2 已论证 `{"value":null}` 会静默选错分支）。

### 3.2 `build_insert_sql` / `insert_returning_clause`（冻结签名 + 默认实现）

```rust
// packages/driver-api/src/traits.rs —— 签名照抄 00-contracts §5.1，不改一个字符
/// 构造一条插入语句。`Unset` 的列不进语句（见契约第 3.5 节语义要求）。
fn build_insert_sql(&self, table: &str, columns: &[(&str, CellWrite)]) -> String {
    sql_text::build_insert_sql(self, table, columns)
}

/// 方言的新增行主键回填子句；`None` 表示该方言无法回填。
fn insert_returning_clause(&self, columns: &[&str]) -> Option<String> {
    None
}
```

```rust
// packages/driver-api/src/traits/sql_text.rs（新增函数，与 build_update_sql 同址同风格）
pub(crate) fn build_insert_sql<D: DatabaseDriver + ?Sized>(
    driver: &D,
    table: &str,
    columns: &[(&str, CellWrite)],
) -> String {
    let mut names: Vec<String> = Vec::with_capacity(columns.len());
    let mut values: Vec<String> = Vec::with_capacity(columns.len());
    for (column, write) in columns {
        match write {
            CellWrite::Unset => continue,
            CellWrite::Null => {
                names.push(driver.quote_ident(column));
                values.push("NULL".to_string());
            }
            CellWrite::Value(value) => {
                names.push(driver.quote_ident(column));
                values.push(driver.format_sql_literal(&Some(value.clone())));
            }
        }
    }
    if names.is_empty() {
        // 不通用：MySQL 不支持 DEFAULT VALUES，由该驱动覆盖（契约 §3.5 第 2 条）。
        return format!("INSERT INTO {} DEFAULT VALUES", driver.quote_ident(table));
    }
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        driver.quote_ident(table),
        names.join(", "),
        values.join(", ")
    )
}
```

**与 `build_update_sql` 的风格一致性（硬要求）**：表名与列名**只**经 `driver.quote_ident`；字面量**只**经 `driver.format_sql_literal`；不拼接分号；不做 schema 限定（与 `build_update_sql` 一致，目标限定由既有会话/调用方负责）；`format!` 的骨架与空格风格一致；不引入任何按 `driver_type()` 的分支（零硬编码）。

| 方法 | 语义 | `None` / 空语义 |
| --- | --- | --- |
| `insert_returning_clause(columns)` | 返回**追加在插入语句尾部**的方言子句（例：`RETURNING "id"`） | `None` = 该方言不能回填 → 宿主走降级路径 |
| `insert_returning_clause` 返回 `Some("")` | **已内联信号**：子句已被 `build_insert_sql` 写进语句内部（SQL Server 的 `OUTPUT INSERTED.*` 只能出现在列清单与 `VALUES` 之间），宿主**原样**执行语句且**必须**用查询路径读取返回行 | 宿主不得追加任何字符（含空格） |
| `ReuseDriver` | 必须显式转发 `build_insert_sql` 与 `insert_returning_clause` 到 `inner` | 漏转发 = 包装驱动退回默认实现，丢掉内层覆盖 |

**SQL Server 的位置矛盾必须正面处理**：契约规定「宿主把子句追加到插入语句」，而 T-SQL 的 `OUTPUT` 子句**不能追加在 `VALUES` 之后**。因此 SQL Server 驱动的正确实现是：覆盖 `build_insert_sql` 自行内联 `OUTPUT INSERTED.[col]`，并让 `insert_returning_clause` 返回 `Some("")` 表示「已内联、用查询路径读」。这条语义写在 `traits.rs` 的文档注释里，并由 §9 的驱动用例锁死。更干净的替代方案（新增 `InsertReturningPlacement` 枚举或第二个 trait 方法）见 §11 Q-1。

### 3.3 计划 DTO 变更（宿主，`commands/data/row_change_plan.rs`）

```rust
/// 一条新增行的草稿（请求侧，新增）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingRowInsert {
    /// 仅用于前端把提交结果关联回草稿行；宿主不做任何语义解释，只要求非空且唯一。
    pub draft_id: String,
    /// 写入意图，按列名排序（`BTreeMap` 天然有序）。未填列**不出现**，即 `Unset`。
    pub values: BTreeMap<String, CellWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowChangePlan {
    pub plan_id: String,
    pub fingerprint: String,
    pub table: RowChangeTableContext,
    pub updates: Vec<PlannedStatement>,
    pub deletes: Vec<PlannedStatement>,
    /// 新增：插入语句。顺序 = 前端草稿视觉顺序 = 执行顺序（**不得排序**）。
    /// `#[serde(default)]` 只为兼容不发送该字段的旧前端；序列化时**始终输出**（前端 TS 类型是必填数组）。
    #[serde(default)]
    pub inserts: Vec<PlannedStatement>,
    pub warnings: Vec<ChangeWarning>,
}
```

**`PlannedStatement` 是否够用？——够用，但有 3 条必须写死的不变量。** 它**不新增字段**，插入语句复用其全部字段：

| `PlannedStatement` 字段 | INSERT 语义 | 不变量 |
| --- | --- | --- |
| `row_identity` | **空 map**（插入没有行身份）；回填成功后由宿主在**执行结果**里填真实主键 | 插入语句的 `row_identity` 恒为空，前端不得据此定位草稿 |
| `original_values` | **空 map**（新行没有原值） | — |
| `current_values` | 该行**实际写入**的列 → 写值（`None` 或 `Some(Value::Null)` 都表示 SQL `NULL`） | 键集必须**恰好等于** `changed_columns` |
| `changed_columns` | 出现在 `INSERT INTO t (…)` 中的列，**按列名排序** | 「不在其中」= `Unset`。**Null 与 Unset 的区分由「是否在 `changed_columns` 里」承载，而不是由 `Option` 承载** —— 这是复用 `PlannedStatement` 而不丢信息的关键 |
| `sql_template` | 驱动渲染的预览 SQL（**展示用**，执行前一律重建） | 与 `build_insert_sql` + 回填子句一致 |
| `parameter_summary` | 形如 `INSERT <列>=<值摘要>` | 仅展示 |

由此可从插入语句反推 `PendingRowInsert` 的 `values`：`changed_columns` 里的列，`current_values[col]` 为 `None` / `Some(Value::Null)` → `CellWrite::Null`，否则 `CellWrite::Value(v)`；`changed_columns` 之外的列即 `Unset`。这是 `validate_immutable_plan` 重建计划的基础。**反推拿不回的有两样**：(1) `draft_id`（它不进计划，见 §11 Q-4）；(2) 请求侧可能出现的 `CellWrite::Value(Value::Null)`——它与 `CellWrite::Null` 渲染成同一段 SQL，但反推只会得到后者。**第 (2) 项必须在请求侧规范化掉（§3.4 的 V-7），否则该行的指纹在重建时必然不等**；第 (1) 项则必须**排除在指纹之外**（§3.4 的可重建性铁律）。这两条不是可选优化，是「带插入的提交能不能用」的前提。

### 3.4 指纹覆盖插入（`changes_fingerprint` 扩展）

#### 3.4.1 可重建性铁律（本节最重要的一条）

`validate_immutable_plan` 的最后一步是「用 `plan_changes` / `plan_inserts` 从 `RowChangePlan` **重建整张计划**，要求 `rebuilt.fingerprint == fingerprint`」。

由此得到一条**必须先于实现写下**的约束：

> **指纹 payload 里的每一个字节，都必须能从 `RowChangePlan` 重建出来。**
> 否则每一次带插入的提交都会得到 `rebuilt.fingerprint != fingerprint`，被拒为 `grid.commit.stalePlan`——
> 表现是「插入功能完全不可用」，而报错文案指向指纹，极易被误诊成前端签名 bug。

`PendingRowInsert` 有两个字段不满足这条：`draft_id`（计划里根本没有）与未经规范化的 `CellWrite::Value(Value::Null)`（反推只会得到 `CellWrite::Null`）。**两条都必须处理，处理方式不同**：

| 不满足的字段 | 处理方式 | 理由 |
| --- | --- | --- |
| `draft_id` | **结构性排除在 payload 之外** | 它是 UI 行身份，不进任何 SQL。草稿行的防串改由**顺序契约**保证（`RowCommitStatementResult` 与 `plan.inserts` 同序，§11 Q-4），不靠指纹 |
| `CellWrite::Value(Value::Null)` | **请求侧规范化为 `CellWrite::Null`**（V-7） | 两者渲染成同一段 SQL，规范化不损失任何执行语义；不规范化则该行指纹必然不等 |

「结构性排除」的意思是**用一个独立的投影类型把它挡在结构之外**，而不是靠 `skip_serializing_if` 顺手漏掉——后者会让后来的人以为「这个字段本来就没参与哈希」，而真相是「参与了就会炸」。

#### 3.4.2 实现

```rust
/// 指纹用的插入投影：**刻意只含 `values`**。
/// `draft_id` 不在这里（可重建性铁律，§3.4.1）；`sql_template` / `warnings` 同样不进。
#[derive(Serialize)]
struct InsertFingerprintEntry<'a> {
    /// 已规范化的写入意图；`BTreeMap` 按列名有序，序列化结果稳定。
    values: &'a BTreeMap<String, CellWrite>,
}

fn changes_fingerprint(
    table: &RowChangeTableContext,
    changes: &[PendingRowChange],
    inserts: &[PendingRowInsert],   // 新增参数
) -> Result<String, CommandError> {
    #[derive(Serialize)]
    struct FingerprintPayload<'a> {
        table: &'a RowChangeTableContext,
        changes: &'a [PendingRowChange],
        /// 空时**不参与哈希**：保证「没有插入」的计划指纹与今天逐字节相同，
        /// 既有断言 commit_reuses_plan_fingerprint_and_returns_plan_id 不被打破。
        #[serde(skip_serializing_if = "Vec::is_empty")]
        inserts: Vec<InsertFingerprintEntry<'a>>,
    }
    let payload = FingerprintPayload {
        table,
        changes,
        inserts: inserts
            .iter()
            .map(|i| InsertFingerprintEntry { values: &i.values })
            .collect(),
    };
    let bytes = serde_json::to_vec(&payload).map_err(CommandError::Json)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
```

**仍然成立的部分**：`fingerprint` 覆盖插入内容（每个 `values` 的三态，逐条按请求顺序），否则「预览后又在同一条草稿上改了一个列值」不会被察觉，重建校验会放过一个与用户确认内容不同的计划——这是契约 §1.4 事实 5 明确禁止的「计划开裂」。多条草稿的**相对顺序**也被覆盖（数组按序序列化），所以「把两条内容不同的草稿对调」会改变指纹。

**不被覆盖的部分（是有意的）**：草稿的 `draft_id`。两条**内容完全相同**的草稿互换位置，指纹不变——而这在执行上是等价的（SQL 逐字节相同），不是漏洞。



### 3.5 执行结果（`RowCommitStatementResult`）

`operation` 是 `String`，**新增取值 `"insert"`**（不改字段名与类型）。`row_identity` 在插入场景：回填成功 → 真实主键；回填降级（`None` 或表无主键）→ **空 map**；用户显式写满全部主键列 → 即使不给回填子句，宿主也用用户写入的值构造该行的身份。

### 3.6 前端镜像（TS，`src/lib/tableChanges.ts`）

```ts
/** 一次列写入的三态；与 Rust `CellWrite` 的线上形态逐字对应。 */
export type CellWrite =
  | { kind: 'null' }
  | { kind: 'value'; value: Value }
  | { kind: 'unset' };

export const cellWriteNull = (): CellWrite => ({ kind: 'null' });
export const cellWriteValue = (value: Value): CellWrite => ({ kind: 'value', value });
export const cellWriteUnset = (): CellWrite => ({ kind: 'unset' });

/** 一条新增行草稿（前端权威形状；`values` 里没有的列即 Unset）。 */
export interface PendingInsert {
  /** UI 关联键，同时是 React key 与 data-testid 后缀的一部分。 */
  draftId: string;
  /** 草稿值。用普通对象而不是 Map：需要被 `pendingChangesSignature` 一类 JSON 序列化直接吃下。 */
  values: Record<string, CellWrite>;
}

export interface RowChangePlan {
  planId: string;
  fingerprint: string;
  table: TableChangeContext;
  updates: PlannedStatement[];
  deletes: PlannedStatement[];
  /** 必填数组：后端始终序列化该字段（空数组表示没有插入）。 */
  inserts: PlannedStatement[];
  warnings: ChangeWarning[];
}

export interface CommitStatementResult {
  operation: 'insert' | 'update' | 'delete';   // 新增 'insert'
  rowIdentity: RowIdentity;
  affectedRows: number;
}
```

```ts
// src/commands/database.ts
export interface PreviewPendingChangesRequest {
  context: TableChangeContext;
  changes: PendingRowChange[];
  /** 新增，可选：不传等价于「没有插入」（后端 #[serde(default)]）。 */
  inserts?: PendingInsert[];
}
```

**逐字段说明与缺省**

| TS 符号 | 用途 | 缺省 / 空值语义 |
| --- | --- | --- |
| `CellWrite.kind` | 三态判别 | 必填；`'unset'` 不允许携带 `value` |
| `PendingInsert.draftId` | 草稿身份，只服务 UI；提交后据此清理与高亮 | 必填、非空、批内唯一（前端生成，`crypto.randomUUID()` 或递增计数器均可，**不得**用行下标当 id：翻页/排序会变） |
| `PendingInsert.values` | 逐列写意图 | 缺省 `{}` = 全列 `Unset` = `DEFAULT VALUES`（是否合法由宿主按表结构裁决） |
| `RowChangePlan.inserts` | 服务端产出的插入语句 | 后端始终输出；前端按**顺序**与自己的草稿顺序对齐（后端保证顺序 = 请求顺序） |
| `CommitStatementResult.operation` | 结果类型 | 新增 `'insert'`；Rust 侧本就是 `String`，属**拓宽**而非改类型 |
| `CommitPendingChangesResult.status` | 既有 `'committed' \| 'failed' \| 'noop'` | 不变；插入参与后可提交的判据变成「`updates ∪ deletes ∪ inserts` 非空」 |

### 3.7 前端 store 状态（`src/stores/tableData/types.ts`）

```ts
export interface TableState {
  // …既有字段全部不动…
  pendingChanges: Map<string, PendingRowChange>;   // 既有行的改动；键是稳定主键身份
  /** 新增：草稿行。键是 draftId。**刻意与 `pendingChanges` 分开**（详见下方理由）。 */
  draftInserts: Map<string, PendingInsert>;
  /** 新增：草稿行创建顺序，决定计划与渲染顺序（Map 迭代序虽稳定，但显式数组更好断言）。 */
  draftInsertOrder: string[];
  /** 新增：提交成功后要短暂高亮的行身份（来自回填或用户写满的主键）；超时后清空。 */
  highlightedNewRowKey: string | null;
}
```

**为什么草稿行不能塞进 `pendingChanges`（正面回答「放哪、如何共存」）**：

1. `PendingRowChange` 的**每一个**消费点都假设「行已存在且有身份」：`findPendingForRow` 先用 `buildRowIdentity` 求身份、`overlayPendingRows` 与 `rebuildEditBuffer` 在 `pkColumns.length === 0` 时直接 return、`stageCellChange` 在无主键时直接报 `tableData.noPrimaryKey`、`rowIdentityKey` 对空身份抛错、`effectivePendingIdentity` / `rowIdentityIsUnique` / `hasPendingIdentityCollision` 全部围绕主键。草稿行没有身份，**且无主键表也允许插入**——塞进去会让上述每一条在无主键表上抛错或静默失效。
2. 后端 `canonicalize_changes` / `identity_key` 同样对空身份直接报错（`Row changes require primary-key identity`）。要复用就必须放宽这条安全校验，等于把「UPDATE/DELETE 必须有身份」这条纪律稀释掉。
3. 两者生命周期不同：`pendingChanges` 的键是**内容派生**的（改了主键值键会变），草稿行的键是**创建时固定**的 `draftId`（还没有主键可改）。

**共存规则（必须逐条实现）**：

| 规则 | 内容 |
| --- | --- |
| R-1 | 两个 Map 互不读写；`draftInserts` 不参与 `overlayPendingRows` / `rebuildEditBuffer` / `findPendingForRow` / `rowIdentityAnchors`。 |
| R-2 | 「有未提交工作」= `pendingChanges.size > 0 \|\| draftInserts.size > 0`。`previewPendingChanges` 与 `commitPendingChanges` 的早退判据、`TableView` 的三处按钮门闸、待提交计数文案全部改用它。 |
| R-3 | 预览一致性签名从 `pendingChangesSignature(pendingChanges)` 变成 `pendingWorkSignature({ pendingChanges, draftInserts, draftInsertOrder })`；否则「预览期间拖入/删掉一条草稿」不会被察觉（沿用现有 `pendingChangesSignature` 的思路，只扩一个入参）。 |
| R-4 | 提交成功后**同时**清空两者；`invalidateCachedData` 的「有暂存改动的面板不失效」判据同样改用 R-2 的口径。 |
| R-5 | 草稿行不写入 `ts.rows`（否则会被 `buildRowIdentity` / 去重 / 导出 / 排序当成真实行）；渲染层用 `rows.concat(draftRows)` 之类的**派生**方式拼出显示数据，草稿行下标永远落在真实行之后。 |
| R-6 | `selectedRows` / `rowIdentityAnchors` 不包含草稿行；行级操作（删除、导出）忽略草稿行。 |

### 3.8 兼容性结论

**是否提升 `PROTOCOL_VERSION`：不需要，保持 4；`MIN_PROTOCOL_VERSION` 保持 1。** 理由：

1. 只**新增** trait 方法（`build_insert_sql` / `insert_returning_clause`），两者都有默认实现，默认行为等于今天（不产生任何 INSERT 语句）；`capabilities.rs` 的区间判定因此对 1..4 全部照旧接受。
2. 不改任何既有方法签名（`build_update_sql` / `build_delete_sql` / `format_sql_literal` / `pagination_syntax` / `execute` / `query` / `begin_transaction` / `commit` / `rollback` 全部逐字不变）。
3. 不改既有 DTO 字段名与类型：`RowChangePlan` 只**新增可选**字段、`Value` / `ColumnSchema` / `PlannedStatement` 不动、`RowCommitStatementResult.operation` 只是多一个字符串取值。前端 TS 侧的 `CommitStatementResult.operation` 由 `'update' | 'delete'` 拓宽为三元联合，属类型拓宽，运行时收到的仍是字符串。
4. `CellWrite` 是全新类型，没有旧数据需要反序列化。
5. **残留风险（不构成版本提升理由）**：钉在旧 `ref` 的 git 驱动不会覆盖 `build_insert_sql`，MySQL 协议族会遇到「全列 `Unset` → `DEFAULT VALUES`」的语法错误。这不是静默错数据，且 MySQL 协议族的 path 驱动与 `ReuseDriver` 内层都在本仓、同一次提交内即可覆盖。**动作**：随本册在 `drivers-registry.json` 的说明/发布说明里登记「建议刷新 git 驱动 `ref`」，**不要**为此提升协议版本（提升会强制所有插件同步发版，收益为零）。

---

## 4. 交互与状态机

### 4.1 草稿行生命周期（三要素齐全）

| 状态 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `idle` | 面板初始；提交成功并刷新完成；`Esc` 在无单元格编辑时清空全部空草稿 | 工具条「+ 行」可用（只读连接除外）；既有行照旧双击编辑 | 满足任一进入条件 → `drafting` |
| `drafting` | ① 点工具条「+ 行」；② `⌘/Ctrl+I`；③ 双击**最后一行下方空白区**；④ `⌘/Ctrl+D` 复制当前草稿行；⑤ 03 的 `Paste rows` 无区域路径回填 | 新草稿行追加到 `draftInsertOrder` 末尾并滚动到可见；每个单元格的空状态显示 `DEFAULT` 占位；编辑草稿格 → 写 `values[col] = CellWrite::Value(v)`；把草稿格清空 → `Unset`（**不是** `Null`）；`Set NULL` 显式动作 → `CellWrite::Null`；`⌘/Ctrl+D` → 复制本行 `values` 成一条新草稿；`Delete`（焦点在草稿行且无格在编辑）→ 移除该草稿；待提交计数 = `pendingChanges.size + draftInserts.size` | 点「预览/提交」→ `previewing`；在任何草稿格里输入但未提交 → 仍 `drafting`；`Esc` 且**所有**草稿行 `values` 为空 → 逐条移除空草稿并回 `idle`（有内容的草稿行**不**被 `Esc` 丢掉，必须显式删除） |
| `previewing` | 点「预览」或「提交」触发 `previewPendingChanges` | 预览对话框同时列出 `inserts` / `updates` / `deletes` 三段与总影响行数；确认文案形如「提交 2 插入、1 更新、0 删除」；指纹可见 | 取消 → `drafting`（草稿与既有改动都保留）；确认 → `committing`；预览报错 → `drafting` 并把 `classifyGridError` 结果落到 `ts.error` |
| `committing` | 预览计划指纹校验通过后调 `commitPendingChanges` | 两个集合都冻结（输入被忽略或在途改动触发签名不一致保护）；按钮 `pendingBusy` 置灰 | 成功 → 清空 `pendingChanges` 与 `draftInserts`、`loadTableData` 刷新当前页 → `highlighting` → `idle`；失败 → 保留两套暂存、`ts.error` 显示分类后文案 → `drafting` |
| `highlighting` | 提交返回且能从回填结果或用户写入值得到新行身份 | 命中行加 1.5s 高亮（复用既有 `highlightedRow` 机制）；`highlightedNewRowKey` 超时清 `null` | 超时 → `idle`；用户翻页/改筛选/改排序 → 立即清空并 → `idle` |

**降级分支（`insert_returning_clause` 为 `None` 或表无主键）**：提交成功后**不进入 `highlighting`**，直接刷新当前页回 `idle`，并在提交结果提示里显示「新行主键无法自动定位，已为你刷新当前页」（`tableData.insertReturningUnavailable`）。**唯一例外**：用户把该行的**全部主键列**都显式写成了值 → 身份已知 → 仍可高亮。

### 4.2 键盘、鼠标、焦点、IME

| 输入 | 行为 |
| --- | --- |
| `⌘/Ctrl+I` | 追加草稿行（只读连接：菜单项/按钮置灰并显示既有 `tableData.readOnlyEditDisabled`，**不弹报错**） |
| `⌘/Ctrl+D` | 复制当前草稿行（值深拷贝；`draftId` 新生成）；焦点在真实行时语义仍归 02 分册，本册不抢 |
| `Delete` / `Backspace` | **仅当**焦点在草稿行行号槽或草稿行内且没有格处于编辑态：移除该草稿（不产生 SQL、不进 `pendingChanges`）。格内编辑时 `Delete` 仍是文本删除 |
| 双击最后一行下方空白区 | 追加草稿行；双击**真实行**单元格仍是既有编辑行为，两者以命中区域区分（`data-row-kind="draft"` vs 既有行） |
| `Esc` | 先退格编辑（既有 `cancelEdit`）；若没有格在编辑，则移除**空**草稿行（见 4.1） |
| `Tab` / `Shift+Tab` | 草稿行内移动焦点到下一个/上一个可写列，**跳过**自增列与「无默认值但 NOT NULL 且用户留空」之外的只读列；跨到草稿行末尾时进入下一草稿行 |
| 焦点 | 草稿行获得焦点时**不得**写入 `ts.rows`；`editingCell` 的 `row` 索引用「真实行数 + 草稿序号」的虚拟坐标，并在所有消费点显式判 `row >= ts.rows.length` |
| IME | 复用既有 `EditableCell` 的组合输入保护：composition 期间不提交 `CellWrite`、不触发签名重算、不响应 `Enter`；composition 结束后一次落值。**禁止**在草稿格里新建一套 `input` 事件处理 |
| 只读 | `isEditable === false` 时「+ 行」不渲染（而不是渲染后报错）；若后端仍返回 `grid.cell.readOnly: …`，前端必须先 `classifyGridError()` 再渲染，未命中前缀则**原样显示**原文（契约 §8.1） |

### 4.3 与既有「待提交」UI 的关系

- 待提交计数文案由 `tableData.pendingChanges`（`{count} unsaved changes`）扩展为 `pendingChanges` + `draftInserts` 之和；插入条数在预览区单独显示（`tableData.pendingInserts`）。
- 既有 `confirmCommit`（`Commit {updates} updates and {deletes} deletes?`）**不改其含义**，另加 `tableData.confirmCommitWithInserts`（`Commit {inserts} inserts, {updates} updates and {deletes} deletes?`），插入为 0 时继续走旧 key，避免动到其他语言已有词条。
- 草稿行在虚拟列表里必须用 `data-testid="draft-row-N"`（PRD 明文要求）与 `data-row-kind="draft"`，供 E2E 与单测稳定定位（禁止用几何坐标反查）。

---

## 5. 实现步骤

> 每步都写「改哪个文件 / 加什么符号 / 为什么 / 怎么自测」。顺序即推荐提交顺序。

### Step 1 · 先拆 `src-tauri/src/commands/data.rs`（1331 行 → 三个文件）

- **改哪个文件**：`src-tauri/src/commands/data.rs` → 变成 `src-tauri/src/commands/data/mod.rs`，并新增 `src-tauri/src/commands/data/row_change_plan.rs`、`src-tauri/src/commands/data/row_change_execute.rs`、`src-tauri/src/commands/data/tests.rs`。
- **加什么符号**：`mod row_change_plan; mod row_change_execute;`（在 `mod.rs` 内）；`row_change_plan.rs` 承载 `RowChangeTableContext` / `PendingRowChange` / `PlannedStatement` / `ChangeWarning` / `RowChangePlan` / `CommitPendingChangesRequest` / `RowCommitStatementResult` / `CommitPendingChangesResponse` + `canonicalize_changes` / `identity_key` / `effective_identity` / `validate_table_context` / `changes_fingerprint` / `value_summary` / `build_row_change_plan` / `plan_changes` / `validate_immutable_plan` / `preview_pending_changes_impl` / `preview_pending_changes`；`row_change_execute.rs` 承载 `execute_row_change_plan_impl` / `commit_pending_changes_impl` / `commit_pending_changes`；`tests.rs` 搬走 `mod tests`（`#[cfg(test)] mod tests;`）。
- **为什么**：该文件**今天就已 1331 行、超过 800 行上限**；本册还要往里加插入逻辑与测试，不拆会同时违反规模纪律并让 review 无法聚焦。模块路径 `commands::data::*` 对外不变（`mod.rs` 兼容），所以拆分是纯结构提交，可独立回滚。
- **怎么自测**：`cargo test -p datazen --lib` 退出码 `0` 且用例数与拆分前**逐个不变**（先记下旧用例清单再比对）；`test result: ok`；`wc -l` 三个文件均 ≤ 800。**不得**在同一提交里改行为。

- **迁移清单（不是可选项，契约 §4.5 硬性要求）**：本 Step 拆的是 `commands/data.rs`（1331 行），而该文件里恰好藏着**今天仓库中唯一**真正阻止空 `WHERE` 落到 `build_delete_sql` / `build_update_sql` 的检查——`validate_legacy_pk_columns`。它必须被**逐项**搬进新树，不能只写「拆分文件」：
  1. `validate_legacy_pk_columns` 的函数体 → `row_change_plan.rs`，并按 **W-3 加强**：既有实现只校验「非空且值非 NULL」，**不校验键集是否等于 `CachedColumns.primary_keys`**，因此一张 `(tenant_id, order_no)` 复合主键表只要前端传了 `{tenant_id: 7}` 就会静默生成 `WHERE "tenant_id" = 7` 命中成千上万行。搬过去时补上列集相等断言，语义升级为 W-1。
  2. 其配套测试 `commit_row_deletes_rejects_empty_pk` → `data/tests.rs`，**改名并扩充**为同时覆盖 W-1（行身份为空）、W-2（拒绝发生在 SQL 生成之前）、W-3（复合主键漏一列）的三条用例。
  3. `generate_handler!`（`src-tauri/src/bootstrap/run.rs`）里 `commit_row_updates` / `commit_row_deletes` 两个**遗留写入口**按 **C-1 已裁定删除**：注销注册项 + 删 `commit_row_updates_impl` / `commit_row_deletes_impl` + 删 `tableDataStore.test.ts` 里 4 处 mock + 加回归断言「这两个命令名不再出现在 `generate_handler!` 中」（见分册 12 §7.2.1 的 A-2）。
  4. **顺序**：迁移与删除都在本 Step 同一个提交里完成，因为它们同属一次目录化；拆到两个提交会让中间态既没有旧守卫也没有新守卫。

### Step 2 · `driver-api`：`CellWrite` + 两个新 trait 方法 + 默认实现 + `ReuseDriver` 转发

- **改哪个文件 / 加什么符号**：
  - `packages/driver-api/src/types.rs`：新增 `CellWrite`（§3.1）；`use` 处确保被 re-export（`lib.rs` 的 `pub use types::*` 若已通吃则无需改）。
  - `packages/driver-api/src/traits/sql_text.rs`：新增 `build_insert_sql`（§3.2 函数体）。
  - `packages/driver-api/src/traits.rs`：新增 `build_insert_sql` 与 `insert_returning_clause` 两个带默认实现的方法（签名照抄契约），文档注释写清 `Some("")` 的「已内联」语义（§3.2）。
  - `packages/driver-api/src/reuse.rs`：在 `ReuseDriver` 的 impl 里**补两行转发**到 `self.inner`。
- **为什么**：这是契约层唯一的改动点；`ReuseDriver` 转发是必须的，否则 MySQL 协议族（doris / starrocks / oceanbase 等包装驱动）会退回默认 `DEFAULT VALUES` 而在 MySQL 上报语法错误。
- **怎么自测**：`cargo test -p datazen-driver-api --lib`；新增单测见 §9 的 `insert_*` 用例；另加一条「`ReuseDriver` 转发」用例，用一个覆盖了 `build_insert_sql` 的测试内层驱动，断言包装后输出等于内层输出（而不是 `DEFAULT VALUES`）。

### Step 3 · 宿主：请求侧 `PendingRowInsert` + 计划侧 `inserts` + 校验 + 指纹扩展

- **改哪个文件 / 加什么符号**：
  - `commands/data/row_change_plan.rs`：新增 `PendingRowInsert`；给 `RowChangePlan` 加 `inserts`（带 `#[serde(default)]`，序列化始终输出）；扩展 `changes_fingerprint` 签名与 payload（§3.4）；新增 `canonicalize_inserts(driver, table, inserts, columns) -> Result<Vec<PendingRowInsert>, CommandError>`；`build_row_change_plan` 增加插入分支：为每条草稿调 `canonicalize_inserts` 校验 → 组装 `columns: Vec<(&str, CellWrite)>` → `driver.build_insert_sql` → 追加回填子句（Step 6 落地）→ 生成 `PlannedStatement`（`row_identity` 与 `original_values` 为空 map；`current_values` 与 `changed_columns` 严格由 `values` 派生，`changed_columns` 排序）→ **按请求顺序 `push` 到 `inserts`（不排序）**；`validate_table_context` 保持不变。
  - `commands/data/row_change_plan.rs`：`preview_pending_changes_impl` 增加第 4 个参数 `inserts: Vec<PendingRowInsert>`；`preview_pending_changes`（tauri command）增加 `#[serde(default)] inserts` 参数并透传。
  - `commands/data/row_change_execute.rs`：W-3 的列集相等校验**必须在紧邻生成 SQL 的那一刻**重新读 `SchemaCache::get_columns`，**不得**复用预览阶段缓存下来的主键列表（契约 §11 C-4 的最小成本缓解）。
  - `commands/data/mod.rs`（原 `data.rs` 内的遗留入口按 §11 C-1 裁定删除，不再「不动」）。
- **为什么**：`PlannedStatement` 复用要求「`changed_columns` 是唯一权威写列清单」，所以插入的 `Unset` 必须靠**不在 `changed_columns` 里**表达（§3.3）；`canonicalize_inserts` 必须独立于 `canonicalize_changes`（后者对空身份直接报错，且会丢掉「全列 Unset」的行 —— 而全列 Unset 在 INSERT 语义下是合法的 `DEFAULT VALUES`）。
- **`canonicalize_inserts` 的校验规则（全部在预览期完成，报错即拒绝）**：。**V-7 是「规范化」而不是拒绝，V-8 是「不需要规则」——两条都列出来，是因为它们各自对应一个真实踩过的坑，且都属于「看起来像 bug 的正确代码」**

| # | 规则 | 不满足时 |
| --- | --- | --- |
| V-1 | `draft_id` 非空且批内唯一 | `CommandError::Validation`（前缀 `grid.insert.duplicateDraft`） |
| V-2 | `values` 的列名必须都能在表结构里找到（表结构可用时） | `grid.insert.unknownColumn: <列名>` |
| V-3 | 表结构可用时：存在列满足 `!nullable && default_value.is_none() && !is_auto_increment`，且该列在 `values` 里缺失或为 `Unset` | `grid.insert.noWritableColumn: <列名列表>`（契约 §8.2 指定前缀） |
| V-4 | 表结构不可用（`SchemaCache::get_columns` 返回空列清单）→ **不做** V-2/V-3，附 `ChangeWarning { code: "insert-schema-unavailable" }`，把 NOT NULL / 唯一约束交给数据库报错 | 不拒绝，只警告（避免把「驱动元数据缺失」误判成「用户没填」） |
| V-5 | `is_auto_increment` 列给 `CellWrite::Null` | 允许（交数据库报错），附 warning `insert-null-on-auto-increment`；宿主**不猜** |
| V-6 | 全列 `Unset` | **合法**（`DEFAULT VALUES`），只要 V-3 通过；**禁止**复用「无实际改动就丢弃」的既有规则（那是 `PendingRowChange` 的纪律，见 §10 第 3 条） |
| V-7 | `CellWrite::Value(Value::Null)` | **规范化为 `CellWrite::Null`**，不拒绝。两者渲染成同一段 SQL；不规范化则该草稿的指纹在 `validate_immutable_plan` 重建时必然不等（§3.4.1），表现为「带这条草稿的提交一律被判 `stalePlan`」 |
| V-8 | `values` 里的列名重复 | `BTreeMap` 在反序列化阶段即报错（JSON 对象重复键），请求根本进不到校验层，**不需要**额外规则。列出来是为了让实现者确认「不是漏了检查」 |

- **怎么自测**：`cargo test -p datazen --lib commands::data`；用例见 §9 `insert_*`。**必须同步改**的既有测试调用点（都在 `commands/data/tests.rs`）：`preview_builds_driver_sql_without_opening_a_transaction`、`commit_reuses_plan_fingerprint_and_returns_plan_id`、`commit_rejects_stale_fingerprint_before_execution`、`preview_rejects_changes_without_primary_key_identity`、`commit_rejects_database_context_change_without_switching_session`、`commit_allows_concrete_table_schema_when_connection_schema_is_unspecified`、`commit_rejects_explicit_schema_context_change` —— 一律补 `vec![]`（插入为空）。这些用例使用 `rich_mock_options()`，其 `columns` 为空 → 恰好走 V-4 分支，不影响既有断言。

### Step 4 · **陷阱 1**：扩展「空计划提前返回」，并修好 `plan_changes` 的回填

- **改哪个文件 / 加什么符号**（两处缺一不可）：
  1. `commands/data/row_change_execute.rs::execute_row_change_plan_impl`：把首判断改成
     `if plan.inserts.is_empty() && plan.updates.is_empty() && plan.deletes.is_empty() { … }`。
  2. `commands/data/row_change_plan.rs::plan_changes`：把 `plan.inserts` 也**反推**进返回值。`PendingRowChange` 装不下插入（空身份过不了 `identity_key`），所以这一步有两种实现：(a) 把 `plan_changes` 改成返回 `(Vec<PendingRowChange>, Vec<PendingRowInsert>)`（推荐，改动面小、语义直白）；(b) 新增 `plan_inserts(&RowChangePlan) -> Vec<PendingRowInsert>` 并在 `validate_immutable_plan` 里同时调用两者。
- **为什么**：**三个独立的致命点。**
  - (1) 漏改 → `updates`/`deletes` 都为空、只有插入时函数在第一行就 `return Ok(affected_rows: 0)`：**不报错、不开事务、不执行任何语句**，用户在界面上看到「提交成功」，数据库里什么都没有（契约 §1.4 事实 3）。
这是本册最隐蔽的第二个坑：它把「数据丢失」换成了「功能不可用」，两者都必须有测试锁死。
  - (3) **(1)(2) 都做对了仍然全盘失败** —— 这是最难查的一个。`plan_inserts` 按 §3.3 反推出的 `PendingRowInsert` **没有 `draft_id`**（计划里根本没这个字段），且 `CellWrite::Value(Value::Null)` 会被反推成 `CellWrite::Null`。若指纹 payload 直接序列化 `PendingRowInsert`（本册早期版本就是这么写的、含 `draft_id`），重建出的指纹与客户端算出的**必然不等**，于是**每一次带插入的提交都被拒**。两条修法都在 §3.4.1：`draft_id` **结构性排除**（`InsertFingerprintEntry`），`Value(Value::Null)` **请求侧规范化**（V-7）。换句话说：(2) 修的是「重建丢内容」，(3) 修的是「重建拿不回请求侧的额外信息」——**两个是不同的坑，只修一个照样全盘 `stalePlan`**。
- **怎么自测**：
  - 新增回归测试 `commit_executes_insert_only_plan_instead_of_returning_empty_success`：`options.columns` 配好列、`options.execute_rows_affected = 1`，只传 `inserts`（`changes` 为空）→ 断言 `response.affected_rows == 1`、`response.statements.len() == 1`、`response.statements[0].operation == "insert"`、`mock.execute_calls() == 1`、`mock.commit_calls() == 1`。
  - 新增 `commit_rebuilds_plan_with_inserts_without_fingerprint_mismatch`：预览拿到 plan 后立刻提交 → 必须 `Ok`（若 `plan_changes` 漏 inserts，会拿到 `Row change plan fingerprint is stale or was modified`）。
  - 手动反向验证（一次性）：临时删掉 (1) 里的 `plan.inserts.is_empty() &&`，确认回归测试**变红**——测试必须真的能抓住这个坑。
  - 新增 `commit_accepts_insert_with_explicit_null_value_and_multiple_drafts`：两条草稿，第一条把某列写成 `CellWrite::Value(Value::Null)`、第二条 `draft_id` 取随机 UUID → 预览后立刻提交必须 `Ok`。这条同时锁住 V-7 的规范化与 §3.4.1 的 `draft_id` 排除（任一没做本用例就红）。
  - 新增 `fingerprint_is_reconstructible_from_plan_alone`（**纯函数级，不需要驱动**）：同一组 `PendingRowInsert`，断言 `changes_fingerprint(..)` 与「`plan_inserts(&plan)` 反推后再算一次」的结果**相等**。这是 §3.4.1 铁律的可执行形式，比任何端到端用例都更早、更快地失败。
  - 手动反向验证（一次性）：把 `InsertFingerprintEntry` 改回直接序列化 `PendingRowInsert`，确认上面两条用例**变红**。

### Step 5 · **陷阱 2**：把插入放进**同一个**事务执行块

- **改哪个文件 / 加什么符号**：`commands/data/row_change_execute.rs::execute_row_change_plan_impl`。
- **具体做法（顺序即语义）**：
  1. `let mut statements = Vec::with_capacity(plan.inserts.len() + plan.updates.len() + plan.deletes.len());`
  2. 在**既有的** `let result: Result<…> = async { … }.await;` 块内部，**最前面**加 `for planned in &plan.inserts { … }`，然后才是既有的 `updates` 循环、`deletes` 循环。顺序固定 **inserts → updates → deletes**（PRD 明文：先建父行再改子行，避免同批内瞬时外键违约）。
  3. `driver.begin_transaction` / 裸 `"BEGIN"`、`driver.commit(tx)` / `"COMMIT"`、回滚路径**一行都不改**：`session_open == true` 时照旧不自行 begin/commit（插入并入用户事务），否则照旧 begin/commit/rollback。
  4. 插入语句的**执行通道**按 Step 6 的判定选 `driver.execute`（无回填）或 `driver.query`（有回填）；两者都在**同一个** `async` 块里，因此共享同一事务与同一 `handle`。
- **为什么**：函数有**两条**事务归属路径（已有用户事务 → 不自己 begin/commit；否则自行 begin/commit/rollback，驱动不支持 trait 事务时退化为裸 `BEGIN`）。插入如果写在 `async` 块之外（例如在 `begin_transaction` 之前跑，或在 `commit` 之后补跑），就会落在**另一个**事务里，破坏「一次提交要么全成、要么全不成」；而且落在 commit 之后的插入**无法回滚**，是真正的脏数据。
- **怎么自测**：
  - `insert_and_update_share_one_transaction_and_one_commit`：`options.execute_rows_affected = 1`，一批含 1 插入 + 1 更新 → `mock.execute_calls() == 2`（真实执行两次，而不是 1 次）、`mock.commit_calls() == 1`、`open_transaction_count()` 归零、`affected_rows == 2`。
  - `insert_joins_open_user_transaction_instead_of_committing`：先在 `state.session_transactions` 上登记该 `db_session_id`（沿用既有 `commit_row_updates_joins_open_session_transaction` 的手法）→ 断言提交后 `mock.commit_calls()` **不增长**、`open_transaction_count()` 仍为 1，但插入语句**已执行**（`execute_calls()` 增长）。
  - 事务原子性用例（§9 `insert_rolls_back_whole_batch_on_failure`）：让更新语句的 `affected != 1`（把 `execute_rows_affected` 设成 2 或在第二次调用上制造错误）→ 断言整批 `Err`、且**回滚被调用**（`rollback_calls` 增长，若 mock 未暴露该访问器则断言 `open_transaction_count()` 归零 + 无 `commit_calls` 增长）。
  - 用户事务路径失败时**不**回滚用户事务（保持既有语义）：断言 `rollback_calls` 不增长。

### Step 6 · 主键回填与降级（两条路径的用户可见差异）

- **改哪个文件 / 加什么符号**：`commands/data/row_change_execute.rs` 新增私有函数 `insert_row_identity(driver, handle, sql, pk_columns) -> Result<Option<RowIdentity>, CommandError>`（或等价的内联逻辑）；`commands/data/row_change_plan.rs::build_row_change_plan` 在生成插入 `sql_template` 时调用 `driver.insert_returning_clause(&pk_column_names)`。
- **符号与规则**：

| 条件 | 宿主行为 | 用户可见差异 |
| --- | --- | --- |
| 表有主键（`CachedColumns::primary_keys` 非空，或由 `ColumnSchema::is_primary_key` 回退得到）且 `insert_returning_clause(&pk_names) == Some(clause)` 且 `clause` 非空 | `sql = format!("{} {}", sql_template, clause)`；用 `driver.query(handle, &sql)` 取 `QueryResult`，按 `QueryResult.columns[].name` 把第一行的主键列值映射成 `RowIdentity`；非 NULL 且齐全才接受，否则 `CommandError::Validation` 前缀 `grid.insert.returningUnavailable: …`；`RowCommitStatementResult.row_identity` = 回填身份 | 提交后**高亮新行 1.5s**（PRD 第 5 条） |
| `insert_returning_clause(...) == Some("")` | **不追加任何字符**；`driver.query(handle, &sql_template)` 读返回行（SQL Server 的 `OUTPUT INSERTED.*` 已由驱动的 `build_insert_sql` 内联） | 同上一行 |
| `insert_returning_clause(...) == None`，**但**该草稿把全部主键列都显式写了值 | 用用户写入的 PK 值构造 `row_identity`（不再向数据库索取） | 仍高亮新行；**不**提示降级 |
| `insert_returning_clause(...) == None`，且草稿没写满主键 | 不猜测；`row_identity` 留空 map；附 `ChangeWarning { code: "insert-remaining-unavailable", severity: "info" }` | 提交后**刷新当前页**，提示「新行主键无法自动定位，已为你刷新当前页」（`tableData.insertReturningUnavailable`）；**不承诺**高亮 |
| 表**无主键** | 不调用回填（`pk_names` 为空）；同「无法回填」路径 | 插入成功、刷新当前页、不高亮；`+ 行` 可用而既有行编辑/删除仍禁用 |

- **为什么**：契约 §5.1 明文「返回 `Some` 时宿主追加子句并读取返回行以回填主键；返回 `None` 时宿主**不得**猜测主键，而是走『提交后重新拉取当前页』的降级路径，并在 UI 上不承诺『新行会保持高亮』」。回填通道必须用 `query` 而非 `execute`（`execute` 只回 `u64`），且**必须**在同一事务块内。
- **怎么自测**：驱动用例（§9）断言 postgres/sqlite 的 `insert_returning_clause(&["id"])` 返回 `Some("RETURNING \"id\"")` 且 `build_insert_sql` 的骨架里**没有** `RETURNING`（保证「追加」语义）；SQL Server 断言 `build_insert_sql` 内含 `OUTPUT INSERTED.` 且 `insert_returning_clause` 返回 `Some("")`；MySQL 断言 `insert_returning_clause` 返回 `None`。宿主侧用 `MockDriverOptions.query_rows` 造返回行，断言回填后的 `row_identity`。

### Step 7 · 前端 store：草稿状态、动作、计数聚合、预览/提交接线

- **改哪个文件 / 加什么符号**：
  - `src/lib/tableChanges.ts`：`CellWrite` + 三个构造器、`PendingInsert`、`RowChangePlan.inserts`、`CommitStatementResult.operation` 拓宽；新增纯函数 `cellWriteForValue(column: ColumnSchema, value: unknown): CellWrite`（空/`undefined` → `Unset`；显式 `null` → `Null`；否则 `Value`）、`intendedWriteColumns(values: Record<string, CellWrite>): string[]`（排序后的写列）、`draftInsertCount(ts)`。
  - `src/stores/tableData/types.ts`：`TableState` 加 `draftInserts`、`draftInsertOrder`、`highlightedNewRowKey`（§3.7）。
  - `src/stores/tableData/pendingChanges.ts`：新增 `pendingWorkSignature({ pendingChanges, draftInserts, draftInsertOrder })`（在既有 `pendingChangesSignature` 的基础上扩一个入参，不改既有函数签名）、`hasPendingWork(ts)`、`draftsForWire(ts): PendingInsert[]`（按 `draftInsertOrder` 展开、剔除空 `draftId`）。
  - `src/stores/tableDataStore.ts`：`TableDataStore` 接口新增 `addDraftRow(panelId)`、`updateDraftRow(panelId, draftId, column, value)`、`removeDraftRow(panelId, draftId)`、`duplicateDraftRow(panelId, draftId)`、`setDraftCellWrite(panelId, draftId, column, write: CellWrite)`；`previewPendingChanges` / `commitPendingChanges` 的早退判据与签名改用 `hasPendingWork` / `pendingWorkSignature`；`previewPendingChanges` 请求体补 `inserts: draftsForWire(ts)`；`commitPendingChanges` 成功分支同时清空 `draftInserts` / `draftInsertOrder` 并设置 `highlightedNewRowKey`。
  - `src/commands/database.ts`：`PreviewPendingChangesRequest.inserts?`，`previewPendingChanges` 透传 `inserts: params.inserts ?? []`。
- **为什么**：R-2/R-3 不做，就会出现「只有草稿行时按钮永远置灰」或「预览后改草稿仍能提交过期计划」两类回归；`draftsForWire` 必须按 `draftInsertOrder`（不能按 `Map` 迭代序的隐式约定），因为**插入顺序影响自增主键分配**，后端也刻意不排序。
- **怎么自测**：`npx vitest run src/stores/__tests__/tableDataStore.test.ts src/lib/__tests__/tableChanges.test.ts`；新增用例见 §9。

### Step 8 · 前端 UI：入口、草稿行渲染、预览与确认文案

- **改哪个文件 / 加什么符号**：
  - `src/components/DataTable/VirtualBody.tsx`：`VirtualBodyProps` 加 `draftRows?: DraftRowViewModel[]`（`{ draftId, cells: Record<string, CellWrite> }`）与 `onDraftCellEdit` / `onDraftRowRemove`；在真实行之后渲染草稿行，节点带 `data-testid="draft-row-N"`、`data-row-kind="draft"`，`Unset` 格显示 `DEFAULT` 占位（`tableData.insertDefault`）。
  - `src/windows/connection/TableView.tsx`：工具条「+ 行」按钮（`tableData.insertRow`，`title` 带 `⌘/Ctrl+I`）、`⌘/Ctrl+I` 与 `⌘/Ctrl+D` 快捷键（与 02 分册的键盘表对齐，避免重复注册）、最后一行下方空白区双击处理、`Delete` 移除草稿；三处 `pendingChanges.size === 0` 门闸改用 `hasPendingWork(ts)`；预览对话框增加 `inserts` 段落（复用既有 `tableData.previewTitle` / `previewSql` / `previewParameters` / `previewWarnings`）；确认文案在 `inserts.length > 0` 时改走 `tableData.confirmCommitWithInserts`。
  - `src/lib/gridErrors.ts`（**01 已建骨架**，本册按契约 §8.1 三步只补码表，**禁止整文件重写**）：见 §8。
- **为什么**：草稿行不能写进 `ts.rows`（§3.7 R-5），所以必须由 `VirtualBody` 的**派生渲染**承接；「+ 行」在只读连接下**不渲染**（契约 R4：能力缺失时隐藏入口，而不是渲染一个点了报错的按钮）。
- **怎么自测**：`pnpm typecheck` + `npx vitest run`（组件级用例见 §9）+ `pnpm e2e:minimal`（`e2e/specs/table-insert.ts`）。

### Step 9 · 驱动覆盖与驱动专属测试（postgres / mysql / sqlite / sqlserver）

- **改哪个文件 / 加什么符号**：
  - `packages/drivers/mysql/src/mysql.rs`：覆盖 `build_insert_sql`——全列 `Unset` 时产出 `INSERT INTO `t` () VALUES ()`（MySQL 不支持 `DEFAULT VALUES`），其余情形与默认实现同构（复用 `sql_text` 风格但走自己的 `quote_char`）。
  - `packages/drivers/postgres/src/postgres.rs`：覆盖 `insert_returning_clause` → `Some(format!("RETURNING {}", cols.map(quote_ident).join(", ")))`。
  - `packages/drivers/sqlite/src/sqlite.rs`：同上（SQLite ≥ 3.35 支持 `RETURNING`；低版本策略见 §11 Q-3）。
  - `packages/drivers/sqlserver/src/sqlserver.rs`：覆盖 `build_insert_sql` 内联 `OUTPUT INSERTED.[col]`，`insert_returning_clause` 返回 `Some(String::new())`。
  - 各驱动 `src/tests.rs`（或该 crate 的既有测试文件）新增三态断言。
- **为什么**：零硬编码——方言差异只能经 `DatabaseDriver` 表达，宿主绝不按库名分支；而这些差异（`DEFAULT VALUES` 的可用性、回填子句的位置与语法）正是驱动该拥有的知识。
- **怎么自测**：`cargo test -p datazen-driver-postgres --lib`、`-p datazen-driver-mysql --lib`、`-p datazen-driver-sqlite --lib`、`-p datazen-driver-sqlserver --lib` 全部退出码 `0`；每个 crate 至少断言 `DEFAULT`（`Unset`）与 `NULL` 与空串三者在 SQL 文本上互不相同。

---

## 6. 文件级改动清单

| 文件 | 新增/修改 | 职责 | 预估行数 | 触及 800 行上限？ |
| --- | --- | --- | --- | --- |
| `src-tauri/src/commands/data.rs` → `data/mod.rs` | 改造 + 迁移 | 保留 IPC 命令壳、遗留批量入口（`commit_row_updates*` / `commit_row_deletes*`）、`get_table_data` 相关命令 | 迁出后 ≈ 700（净 0 新增） | **是（现状 1331 行已超限）→ 必须按 Step 1 拆分** |
| `src-tauri/src/commands/data/row_change_plan.rs` | 新增 | DTO、`canonicalize_changes`、`canonicalize_inserts`、指纹、计划构建、`plan_changes` | ≈ 620（迁移 ≈ 440 + 插入 ≈ 180） | 否 |
| `src-tauri/src/commands/data/row_change_execute.rs` | 新增 | `execute_row_change_plan_impl`、`commit_pending_changes_impl`、回填 | ≈ 350（迁移 ≈ 240 + 插入/回填 ≈ 110） | 否 |
| `src-tauri/src/commands/data/tests.rs` | 新增 | 从 `data.rs` 迁出的 `mod tests` + 本册新增用例 | ≈ 700（迁移 ≈ 460 + 新用例 ≈ 240） | 否（若超 800 再按 plan/execute 拆测试模块） |
| `packages/driver-api/src/types.rs` | 修改 | `CellWrite` | +45 | 否（现 ≈ 939 行 → 加后 **仍超 800，属既有超限**；本册不引入新超限，建议随手登记为技术债） |
| `packages/driver-api/src/traits/sql_text.rs` | 修改 | `build_insert_sql` 默认实现 | +35 | 否（现 161 行） |
| `packages/driver-api/src/traits.rs` | 修改 | 两个新 trait 方法 + 文档注释 | +45 | 否（现 1020 行 → **已超限**，同上，属既有技术债） |
| `packages/driver-api/src/reuse.rs` | 修改 | `ReuseDriver` 转发两行 | +12 | 否（现 1159 行 → **已超限**，同上） |
| `packages/drivers/{mysql,postgres,sqlite,sqlserver}/src/*.rs` | 修改 | 方言覆盖 | 每驱动 +15~35 | 否 |
| `packages/drivers/*/src/tests.rs`（4 个） | 修改 | 三态断言 | 每驱动 +40 | 否 |
| `src/lib/tableChanges.ts` | 修改 | `CellWrite`、`PendingInsert`、计划/结果类型、纯函数 | +90 | 否（现 227 行） |
| `src/lib/gridErrors.ts` | **修改（追加）** | **01 已建骨架**（契约 §8.1 归属总则）；本册只追加 `grid.insert.*` 码，**禁止整文件覆盖** | +15 | 否 |
| `src/stores/tableData/types.ts` | 修改 | `TableState` 三个新字段 | +12 | 否 |
| `src/stores/tableData/pendingChanges.ts` | 修改 | `pendingWorkSignature`、`hasPendingWork`、`draftsForWire` | +50 | 否（现 127 行） |
| `src/stores/tableDataStore.ts` | 修改 | 5 个草稿动作 + 早退判据 + 请求体 | +170 | 否（现 722 行 → 加后 ≈ 890 **会越线 → 见下方拆分**） |
| `src/commands/database.ts` | 修改 | `inserts` 透传 | +6 | 否 |
| `src/components/DataTable/VirtualBody.tsx` | 修改 | 草稿行渲染 | +120 | 否（现 ≈ 250 行） |
| `src/windows/connection/TableView.tsx` | 修改 | 入口、快捷键、门闸、预览与确认 | +160 | 否（现 ≈ 400 行） |
| `src/locales/en/query.ts` | 修改 | 新增词条（§8） | +25 | 否 |
| `src/lib/__tests__/tableChanges.test.ts` / `src/stores/__tests__/tableDataStore.test.ts` | 修改 | 单测 | +220 | 否 |
| `e2e/specs/table-insert.ts` | 新增 | E2E | ≈ 180 | 否 |

### 拆分方案（`data.rs` 与 `tableDataStore.ts` 都已逼近/越线）

1. **Rust（强制，Step 1）**：`data.rs`（1331 行）→ `data/mod.rs` + `data/row_change_plan.rs` + `data/row_change_execute.rs` + `data/tests.rs`，四个文件目标均 ≤ 800。模块路径对 `commands` 外部保持 `commands::data::*` 不变，因此拆分是独立可回滚的纯结构提交。
2. **Rust 既有超限（登记，不在本册强改）**：`packages/driver-api/src/types.rs`（939）、`traits.rs`（1020）、`reuse.rs`（1159）**今天已超 800**。本册只做加法，不做无关重构；建议在 `docs/development/` 的规模纪律条目里登记为技术债（拆法示例：`types.rs` → `types/value.rs` + `types/schema.rs`；`traits.rs` → `traits/` 目录下按域拆并保留 re-export）。
3. **前端（强制）**：`src/stores/tableDataStore.ts` 现 722 行，本册 +170 会越线 → 把草稿行逻辑放进新文件 `src/stores/tableData/draftInserts.ts`（纯函数：增删改、复制、`draftsForWire`、`pendingWorkSignature` 的草稿部分），`tableDataStore.ts` 只保留薄接线（每个动作 3~6 行），加后 ≈ 760 行，仍在 800 以内。

---

## 7. 边界与异常清单

| # | 场景 | 期望行为 | 相关符号 / 前缀 |
| --- | --- | --- | --- |
| 1 | **空表** | 可以新增行；`VirtualBody` 在没有真实行时也能渲染草稿行；提交后刷新仍显示新行 | `VirtualBody.draftRows` |
| 2 | **全列 `Unset`（草稿行一个格都没填）** | 合法，产出 `INSERT INTO t DEFAULT VALUES`（或驱动覆盖的等价形式），条件是没有「NOT NULL 且无默认值且非自增」的列；否则预览期拒绝 | `build_insert_sql` 默认实现、`grid.insert.noWritableColumn` |
| 3 | **仅自增主键、其余列都有默认值** | 草稿行全留空即可提交；语句里**不出现**自增列；提交后新行可见且主键由数据库分配 | V-3 排除 `is_auto_increment` |
| 4 | **复合主键** | 插入不需要身份；回填子句一次带上全部主键列（`RETURNING "tenant_id", "id"`）；用户写满全部主键列时即使无回填也能高亮 | `insert_returning_clause(&["tenant_id","id"])` |
| 5 | **无主键表** | **允许插入**（结论）。理由：INSERT 不需要行身份，`RowChangePlan.inserts` 的语句不含 `WHERE`，`affected` 校验也不依赖身份；代价是无法回填、不高亮。同表的既有行编辑/删除仍禁用（`tableData.noPrimaryKey`），UI 必须明确区分「+ 行可用、双击不可编辑」 | PRD DB-04 第 6 条 |
| 6 | **NOT NULL 且无默认值的列被留空** | 预览期拒绝：`grid.insert.noWritableColumn: column "email" requires a value` | V-3 |
| 7 | **唯一约束冲突** | 由数据库抛错 → 驱动 `Err` → `cmd_err("commit_pending_changes")` → 整批回滚；错误文本含列名（既有脱敏管线）；**不得**被 `affected` 校验误吞 | `CommandError` 序列化为脱敏字符串 |
| 8 | **批量部分失败** | 「一次提交要么全成、要么全不成」：`session_open == false` 时全部成功后 `COMMIT`，任一条失败 `ROLLBACK` 后返回错误；`session_open == true` 时不回滚用户事务，只把错误上抛 | `execute_row_change_plan_impl` |
| 9 | **用户事务已开启** | 不 `begin_transaction`、不 `commit`；插入与其他改动并入用户事务，由用户决定提交/回滚；`statements` 照常返回 | 陷阱 2 / Step 5 |
| 10 | **只读连接** | 预览前就被拒（`Connection is read-only; row changes are not allowed`，**无 `grid.` 前缀**）；前端不渲染「+ 行」 | `commit_pending_changes_impl` |
| 11 | **表结构未知（列清单为空）** | 不做 V-2/V-3，附 `insert-schema-unavailable` warning，把约束判定交给数据库；**不得**因此拒绝插入，也**不得**因此把 `Unset` 列补成 `NULL` | V-4 |
| 12 | **`affected == 0`（驱动不上报计数，如 ClickHouse）** | 视为「驱动不上报计数」而非失败：记 warning `insert-count-unavailable`，不判失败；`affected > 1` 仍判失败（单行 `VALUES` 不可能影响多行） | 见 §3.5 与契约 §8.2「INSERT 除外」 |
| 13 | **回填返回空行 / 主键为 NULL** | `grid.insert.returningUnavailable: driver returned no usable primary-key row`；整批回滚 | Step 6 |
| 14 | **草稿数量很大（如 200 条）** | 预览对话框仍一次展示（虚拟滚动），提交仍是 200 条单行语句；超过前端阈值时提示「将执行 200 条语句」但**不**静默截断（拒绝静默丢失） | `tableData.confirmCommitWithInserts` |
| 15 | **多字节字符 / 超长值 / 引号与反斜杠** | SQL 文本交给驱动 `format_sql_literal`（默认翻倍单引号并转义反斜杠；postgres 覆盖为**不**转义反斜杠）；`Value::String` 原值经例如 `O'Brien\x` → `'O''Brien\\x'`（默认实现） | `format_sql_literal` |
| 16 | **草稿行在预览后被删除/修改** | `pendingWorkSignature` 不一致 → 预览计划作废、要求重新预览（现有纪律，不得降级为「按新数据重算」） | R-3 |
| 17 | **翻页 / 改筛选 / 改排序 / 关闭面板** | `highlightedNewRowKey` 立即清空；面板关闭丢弃草稿（与既有 `pendingChanges` 的丢弃语义一致）；有暂存工作时 `invalidateCachedData` 不触碰该面板（判据改用 R-2） | `invalidateCachedData`、R-4 |
| 18 | **自增列被显式写 `NULL`** | 允许提交并交数据库报错，附 warning `insert-null-on-auto-increment`；宿主**不猜**、不静默改写为 `Unset` | V-5 |
| 19 | **复合主键只提供了部分列** | **W-1/W-3**：在**生成 SQL 之前**就拒绝（`CommandError::Validation`），不是发出去以后再看影响行数。既有 `identity_key` 只要求 map 非空且值非 NULL，**不校验键集相等**——这条必须在本册补上（见 Step 1 迁移清单第 1 项） | `validate_legacy_pk_columns`（加强后） |
| 20 | **两个客户端同时改同一行的不同列** | **W-5**：不检测、不阻断，last-writer-wins。`WHERE` **只带主键谓词**，**禁止**把被编辑列的旧值塞进 `WHERE`（那会让「我改这一列」反过来变成「我只能改这一列」）。文档与 UI 文案**不得**出现「只影响这一行」「不会覆盖别人的修改」 | `build_update_sql` |
| 21 | **唯一约束冲突 / 权限不足等数据库拒绝** | **W-6**：IPC 返回体里 `grid.commit.*` 后面跟**宿主自己的话**（如「本次写入被数据库拒绝（权限或约束）」），**不跟数据库原文**。`CommandError` 的序列化只调凭据脱敏（`redact_secrets_for_log`），而它**只覆盖 4 类凭据模式、不处理绝对路径**——直接透传会把本机目录结构暴露到界面与日志。原始错误只进日志，且同样受日志脱敏约束 | `cmd_err`、契约 §4.5 W-6 |
| 22 | **`parameter_summary` / `value_summary`（120 字符截断）** | **W-6 附则**：截断**不等于脱敏**，截断后仍可能保留可识别的业务数据。这两个字段只用于展示与日志，**不得**进入任何 IPC 返回体的持久化路径 | `RowCommitStatementResult` |
| 23 | **预览到提交之间表结构被改（DDL）** | **C-4 已知残留，不在本批实现**：计划指纹只覆盖列值、不覆盖表结构。缓解只有一条且**必须做**——W-3 的列集相等校验在生成 SQL 的那一刻现读 `SchemaCache::get_columns`，把窗口从「整个面板生命周期」压到「单次提交调用内」；失败时把本次读到的主键列集与语句类型一并写日志。**不做** schema 版本号、不做结构指纹、不做「结构变了就禁编辑」门禁 | 契约 §11 C-4 |

---

## 8. i18n key 清单

**落点：领域包文件 `src/locales/en/query.ts`**（新增行文案按语义就近放 `query.ts`；**不要**新建域名，**不要**改 `src/locales/en.ts`——它只有一行再导出，改它无效）。完整 key 路径如下（`pack` 对象的字面量键）：

| 完整 key | 英文文案 | 用途 |
| --- | --- | --- |
| `tableData.insertRow` | `Insert Row` | 工具条按钮 / 菜单项 |
| `tableData.insertRowHint` | `Insert Row ({shortcut})` | 按钮 `title` |
| `tableData.duplicateDraftRow` | `Duplicate Row` | `⌘/Ctrl+D` 菜单项 |
| `tableData.removeDraftRow` | `Discard Row` | `Delete` 菜单项 / 草稿行行号槽菜单 |
| `tableData.draftRowBadge` | `NEW` | 草稿行前缀标记 |
| `tableData.draftRowUnsaved` | `New row (not yet saved)` | 草稿行 `aria-label` |
| `tableData.insertDefault` | `DEFAULT` | 草稿格 `Unset` 占位符 |
| `tableData.insertSetNull` | `Set NULL` | 右键菜单项（与 05 共用，本册先落 key） |
| `tableData.pendingInserts` | `{count} inserts` | 待提交计数分段 |
| `tableData.confirmCommitWithInserts` | `Commit {inserts} inserts, {updates} updates and {deletes} deletes?` | 确认文案（插入为 0 时继续用既有 `tableData.confirmCommit`） |
| `tableData.insertFailed` | `Failed to insert rows` | 提交失败兜底 |
| `tableData.insertNoWritableColumn` | `Fill at least one column: {columns} requires a value.` | 对应 `grid.insert.noWritableColumn` |
| `tableData.insertUnknownColumn` | `Column {column} is not part of this table.` | 对应 `grid.insert.unknownColumn` |
| `tableData.insertDuplicateDraft` | `The same new row was staged twice; reopen the preview.` | 对应 `grid.insert.duplicateDraft` |
| `tableData.insertReturningUnavailable` | `The new row's primary key could not be located automatically; the page was refreshed.` | 对应 `grid.insert.returningUnavailable` 与降级 warning |
| `tableData.insertCountUnavailable` | `The database did not report how many rows were inserted.` | 对应 `affected == 0` 的 warning |
| `tableData.insertSchemaUnavailable` | `Column metadata is unavailable; the database will validate this row.` | 对应 `insert-schema-unavailable` warning |
| `tableData.highlightNewRow` | `New row` | 高亮行提示 |

**后端 `CommandError` 文案前缀（英文，不走 i18n，前端按前缀分类后再渲染本地化文案）**：

| 前缀 | 触发点 | 前端映射 |
| --- | --- | --- |
| `grid.insert.noWritableColumn: ` | V-3 | `tableData.insertNoWritableColumn` |
| `grid.insert.unknownColumn: ` | V-2 | `tableData.insertUnknownColumn` |
| `grid.insert.duplicateDraft: ` | V-1 | `tableData.insertDuplicateDraft` |
| `grid.insert.returningUnavailable: ` | Step 6 回填失败/空行 | `tableData.insertReturningUnavailable` |
| `grid.commit.affectedMismatch: ` | `affected > 1`（插入）或 `!= 1`（更新/删除） | 既有 `tableData.commitFailed` + 明细 |
| `grid.commit.stalePlan: ` | 指纹/重建不一致 | 提示重新预览 |
| `grid.commit.conflictingIntents: ` | 同一行既删除又有列写入 | 指出冲突行 |
| `grid.cell.readOnly: ` | 只读/生成列 | 说明具体原因 |

**前端分类器**：`src/lib/gridErrors.ts`（**01 已创建骨架**；本册按契约 §8.1 三步**只追加**上述 `grid.insert.*` 码值，**不新建第二份骨架、也不整文件重写**）：

```ts
export type GridErrorCode =
  | 'grid.insert.noWritableColumn'
  | 'grid.insert.unknownColumn'
  | 'grid.insert.duplicateDraft'
  | 'grid.insert.returningUnavailable'
  | 'grid.commit.stalePlan'
  | 'grid.commit.affectedMismatch'
  | 'grid.commit.conflictingIntents'
  | 'grid.cell.readOnly'
  /* …03/05 追加的码值… */;

export const GRID_ERROR_CODES: readonly GridErrorCode[];
export function classifyGridError(message: string): GridErrorCode | 'unknown';
```

**纪律**：只改英文侧；`src/locales/index.ts` 的 `I18nKey = TranslationKey | (string & {})` 意味着新增 key 不会 typecheck 失败，**拼错的 key 也不会报错** —— 所以 §9 必须有一条「所有新增 key 都存在于 `en/query.ts` 的 pack 里」的用例。开发期在非英文语言下这些词条会回退为 key 本身，发布前由 `scripts/i18n-sync-check.mjs` 补齐其他语言。

---

## 9. 测试清单

### 9.1 Rust 单元（Host：`src-tauri/src/commands/data/tests.rs`）

| 用例名 | 断言要点 |
| --- | --- |
| `commit_executes_insert_only_plan_instead_of_returning_empty_success` | **陷阱 1 回归**：只有 `inserts` 时 `affected_rows == 1`、`statements.len() == 1`、`operation == "insert"`、`mock.execute_calls() == 1`、`mock.commit_calls() == 1`（把提前返回的判断写错时此用例必须变红） |
| `fingerprint_is_reconstructible_from_plan_alone` | **陷阱 1(3) 回归（纯函数级）**：同一组 `PendingRowInsert`，`changes_fingerprint(..)` 与「`plan_inserts(&plan)` 反推后重算」**相等**。这是 §3.4.1 铁律的可执行形式 |
| `insert_canonicalization_normalizes_value_null_to_null_write` | `Value(Value::Null)` 进、`CellWrite::Null` 出，且 `changed_columns` 含该列（V-7） |
| `commit_accepts_insert_with_explicit_null_value_and_multiple_drafts` | **陷阱 1(3) 端到端**：一条草稿用 `Value(Value::Null)`、另一条随机 `draft_id` → 预览后立刻提交 `Ok`。任一修法缺失即红 |
| `commit_rebuilds_plan_with_inserts_without_fingerprint_mismatch` | **陷阱 1 第二半**：`plan_changes` / `plan_inserts` 漏掉 inserts 时，本用例会拿到 `Row change plan fingerprint is stale or was modified` 而失败 |
| `insert_canonicalization_sorts_columns_and_keeps_order_of_rows` | 同一行内 `changed_columns` 排序稳定；**多行之间不排序**（顺序 = 请求顺序） |
| `canonicalize_inserts_rejects_missing_not_null_without_default` | V-3 → `grid.insert.noWritableColumn`；错误文本含列名 |
| `canonicalize_inserts_rejects_unknown_column` | V-2 → `grid.insert.unknownColumn` |
| `canonicalize_inserts_allows_all_unset_when_defaults_exist` | V-6：全 `Unset` 且 `columns` 里每个 NOT NULL 列都有 `default_value` 或 `is_auto_increment` → 计划里出现一条 `DEFAULT VALUES` 语句（**不是**被当作「无改动」丢掉） |
| `canonicalize_inserts_skips_schema_checks_when_columns_are_unknown` | V-4：`options.columns` 为空（如 `rich_mock_options()`）→ 不拒绝，附 `insert-schema-unavailable` warning |
| `fingerprint_covers_insert_values_but_stays_stable_without_inserts` | 只有 `changes` 时指纹与旧实现**逐字节相同**（保护 `commit_reuses_plan_fingerprint_and_returns_plan_id`）；改一个插入列值 → 指纹变化；**只改 `draft_id` → 指纹不变**（§3.4.1 的结构性排除，这条必须钉死，否则会有人「顺手把它加回去」）；对调两条内容不同的草稿 → 指纹变化 |
| `insert_and_update_share_one_transaction_and_one_commit` | **陷阱 2**：`execute_calls() == 2`（真执行两次）、`commit_calls() == 1`、`open_transaction_count() == 0`、`affected_rows == 2` |
| `insert_joins_open_user_transaction_instead_of_committing` | 预置 `session_transactions` → `commit_calls()` 不增长、`open_transaction_count()` 不归零、但插入已执行 |
| `insert_rolls_back_whole_batch_on_failure` | **事务原子性**：让某条语句失败（`execute_rows_affected = 2` 触发 `grid.commit.affectedMismatch`，或用 `MockDriverOptions.execute_error` 让执行直接报错）→ 整批 `Err`、`commit_calls()` 不增长、`open_transaction_count()` 归零。**注意**：`MockDriver` 目前只把 `rollback_calls` 记在内部、**没有**公开访问器，因此要么补一个 `rollback_calls()` 访问器（测试基建的纯新增），要么就用「无 commit + 事务计数归零」间接断言；不要为了断言而在生产路径加钩子 |
| `insert_joins_user_transaction_and_does_not_roll_it_back_on_failure` | `session_open == true` 且失败 → 不回滚用户事务（`open_transaction_count()` 保持）且错误上抛 |
| `insert_uses_query_path_when_returning_clause_is_present` | `insert_returning_clause` 非 `None` 时走 `driver.query`（`mock.query_calls()` 增长、`execute_calls()` 不增长），回填后的 `row_identity` 等于 `MockDriverOptions.query_rows` 给出的主键值 |
| `insert_identity_falls_back_to_written_primary_key_when_returning_is_none` | 驱动返回 `None` 但草稿写满全部主键列 → `row_identity` 非空且等于用户写入值，**不**出现降级 warning |
| `insert_without_returning_reports_degradation_warning` | 驱动返回 `None` 且未写主键 → warning `insert-remaining-unavailable`、`row_identity` 为空 map |
| `insert_affected_zero_is_tolerated_but_greater_than_one_is_rejected` | `execute_rows_affected = 0` → `Ok` + `insert-count-unavailable` warning；`= 2` → `Err` + `grid.commit.affectedMismatch` |

### 9.2 Rust 单元（`driver-api`：`packages/driver-api/src/traits/sql_text.rs` 的 `#[cfg(test)]` 与 `reuse.rs` 测试）

| 用例名 | 断言要点 |
| --- | --- |
| `build_insert_sql_default_lists_only_null_and_value_columns` | **必含**：入参 `[("id", Unset), ("name", Value("a")), ("note", Null)]` → 输出**不含** `id`，含 `"name"` 与 `"note"`，`VALUES` 段为 `'a', NULL` |
| `build_insert_sql_default_all_unset_is_default_values` | **必含**：全部 `Unset` → 恰好等于 `INSERT INTO "t" DEFAULT VALUES` |
| `build_insert_sql_default_escapes_quotes_and_backslashes` | **必含**：`Value::String("O'Brien\\x")` → 文本含 `'O''Brien\\x'`（先转义反斜杠再翻倍单引号）；再断言 `Value::Bytes` → `X'00ff'`、`Value::Bool(true)` → `TRUE` |
| `build_insert_sql_matches_build_update_sql_style` | **必含（风格一致性）**：对同一驱动、同一表名与列名，断言 `build_insert_sql` 与 `build_update_sql` / `build_delete_sql` 使用**同一个**引号形态（`format!("\"{}\"")` vs 反引号由 `quote_char` 决定）：即 `build_insert_sql(..).contains(&build_update_sql(..))` 之外，显式断言两处表名渲染**逐字相同**、且列名渲染相同（把 `quote_ident` 换掉时两条同时变红） |
| `build_insert_sql_never_emits_null_for_unset_columns` | 反例锁：输出里 `"id"` 出现次数为 0（防「为凑齐列给 Unset 补 NULL」的回归） |
| `insert_returning_clause_default_is_none` | 默认实现返回 `None`（保证未声明驱动走降级路径） |
| `reuse_driver_forwards_insert_builders` | 内层驱动覆盖 `build_insert_sql` 返回哨兵字符串 → `ReuseDriver` 包装后输出等于哨兵（不转发时输出 `DEFAULT VALUES`，用例变红） |
| `reuse_driver_forwards_insert_returning_clause` | 同上，覆盖回填子句 |

### 9.3 驱动 crate 测试（`packages/drivers/<id>/`）

| crate | 用例名 | 断言要点 |
| --- | --- | --- |
| `datazen-driver-postgres` | `insert_sql_distinguishes_null_default_and_empty_string` | `Null` → `NULL`；`Unset` → 列不出现；`Value(String(""))` → `''`（三者互不相同）；`insert_returning_clause(&["id"])` → `Some("RETURNING \"id\"")` |
| `datazen-driver-mysql` | `insert_sql_all_unset_uses_empty_values_list` | 全 `Unset` → `INSERT INTO `t` () VALUES ()`（**不是** `DEFAULT VALUES`）；`` ` `` 引号；布尔 → `1`/`0`；`insert_returning_clause` → `None` |
| `datazen-driver-sqlite` | `insert_sql_returns_returning_clause` | 同 postgres（`RETURNING`）；空串列为 `''` |
| `datazen-driver-sqlserver` | `insert_sql_inlines_output_inserted_and_signals_inlined_returning` | `build_insert_sql` 含 `OUTPUT INSERTED.` 且位置在列清单与 `VALUES` **之间**；`insert_returning_clause` → `Some(s)` 且 `s.is_empty()` |

### 9.4 前端单测（Vitest）

| 文件 / 用例名 | 断言要点 |
| --- | --- |
| `src/lib/__tests__/tableChanges.test.ts::cell_write_builders_round_trip_the_wire_shape` | `cellWriteNull()` / `cellWriteValue(1)` / `cellWriteUnset()` 序列化后恰好是 `{"kind":"null"}` / `{"kind":"value","value":1}` / `{"kind":"unset"}`（与 Rust 侧线上形态逐字一致） |
| `…::cell_write_for_value_maps_empty_to_unset_and_null_to_null` | `undefined` / `''`（按 05 的约定，本册只测 `undefined`）→ `Unset`；显式 `null` → `Null`；`0` / `false` / `''` 不得被误判成空 → `Value` |
| `…::intended_write_columns_excludes_unset` | 混合三态 → 只返回排序后的非 `Unset` 列 |
| `…::draft_insert_count_and_has_pending_work_cover_both_collections` | `pendingChanges` 空 + `draftInserts` 非空 → 仍算「有待提交工作」 |
| `src/stores/__tests__/tableDataStore.test.ts::preview_sends_drafts_in_creation_order` | `databaseCommands.previewPendingChanges` 收到 `inserts`，且顺序 = `draftInsertOrder`（不是按列名或行下标） |
| `…::commit_clears_drafts_and_marks_highlighted_row` | 提交成功后 `draftInserts.size === 0`、`draftInsertOrder` 为空、`highlightedNewRowKey` 被设置 |
| `…::preview_is_invalidated_when_a_draft_changes` | 预览返回前修改一条草稿 → `previewPlan` **不**被写入（沿用既有签名保护思想） |
| `…::no_primary_key_table_allows_drafts_but_not_cell_edits` | 无主键表：`addDraftRow` 生效；`stageCellChange` 仍报 `tableData.noPrimaryKey` |
| `…::new_locale_keys_exist_in_en_query_pack` | 遍历 §8 的新 key，断言都能在 `src/locales/en/query.ts` 的默认导出里取到非空字符串（防 `I18nKey` 的 `string & {}` 逃逸） |
| `src/lib/__tests__/gridErrors.test.ts::classify_maps_insert_prefixes_and_falls_back_to_raw` | `classifyGridError('grid.insert.noWritableColumn: column "email" requires a value')` → `'grid.insert.noWritableColumn'`；无关消息 → `'unknown'` 且调用方拿到**原始字符串** |

### 9.5 连续旅程测试（journey）

`src/stores/__tests__/draftInserts.journey.test.ts`：模拟完整击键序列并断言每一步的状态跃迁（进入条件 / 状态内行为 / 退出跃迁，含残缺中间态）：

1. 打开无主键表 → 点「+ 行」→ 断言 `draftInserts.size === 1`、`ts.rows` **未变**（R-5）。
2. 在 `name` 格输入 `ab` → 断言 `values.name === {kind:'value'}`、`changedColumns`（派生）为 `['name']`。
3. 再输入 `c`（模拟 IME composition 中间态）→ 断言 composition 期间不产生新 `CellWrite`、不触发 `previewPendingChanges`。
4. `Esc` → 断言**有内容**的草稿行**不**被移除、回到 `drafting` 而不是 `idle`。
5. `⌘+D` → 断言两条草稿、`draftId` 不同、值深拷贝（改一条不影响另一条）。
6. `Delete`（焦点在草稿行号槽）→ 删一条，剩一条。
7. 点「提交」→ 断言请求体 `inserts.length === 1` 且按 `draftInsertOrder` 排序、`changes` 为空数组。
8. 提交失败 → 断言草稿**全部保留**、`ts.error` 是 `classifyGridError` 分类后的文案。
9. 提交成功 → 断言两个集合清空、刷新被调用一次、`highlightedNewRowKey` 在 1.5s 后被清空（用 fake timers）。

### 9.6 E2E（`e2e/specs/table-insert.ts`，Host 通用路径）

| 用例 | 断言要点 |
| --- | --- |
| `insert row into a table with auto-increment pk` | 点「+ 行」→ 填非自增列 → 提交 → 新行可见、主键非空；草稿行消失 |
| `insert row with NULL and DEFAULT and empty string` | 三格分别 `Set NULL` / 留空 / 输入空串 → 提交后三列落库值互不相同 |
| `insert into a table without a primary key` | 「+ 行」可用；双击既有行不进入编辑并出现只读提示；提交成功 |
| `insert and update in one batch commit together` | 一批 1 插入 + 1 更新 → 预览显示两段；提交后两者都生效 |
| `commit failure rolls back the insert` | 制造唯一约束冲突 → 报错含列名；重新加载后**没有**新行残留 |
| `draft row order matches execution order` | 连加 3 行、填可区分的值 → 提交后（按自增主键排序断言）插入顺序 = 草稿视觉顺序 |

### 9.7 门禁与记录纪律

- 所有测试命令的输出重定向到系统临时目录，并**退出码单独打印**（`cargo test` / `vitest` / `typecheck` 的结论都在末尾，回显会被截断）。
- 门禁首尾各记录一次 `git rev-parse HEAD` 与工作区 `sha`，证明运行期间工作树未被他人改动。
- 本册新增的驱动方言测试**不得**写到 Host（`src-tauri/` 或 `e2e/specs/` 里不得出现 postgres 方言断言）。

---

## 10. 自查清单

| # | 错误做法 | 正确做法 | 后果 |
| --- | --- | --- | --- |
| 1 | 给 `RowChangePlan` 加上 `inserts` 就算完，忘了改 `execute_row_change_plan_impl` 的**首个空计划判断** | 三个集合一起判空：`inserts.is_empty() && updates.is_empty() && deletes.is_empty()` | **界面显示提交成功、数据库里什么都没有，而且不报错**（契约 §1.4 事实 3） |
| 2 | 忘了让 `plan_changes` / `validate_immutable_plan` 把 `inserts` 反推回来 | 重建计划时必须带上插入（`plan_changes` 返回二元组或新增 `plan_inserts`） | 每次带插入的提交都被判「指纹已过期/计划被动过」，功能完全不可用，且错误文案会把人引向错误方向 |
| 3 | 把插入语句写在第 `async` 块之外（`begin_transaction` 之前，或 `commit` 之后补跑） | 插入循环放在**同一个** `async {}` 执行块内，顺序 `inserts → updates → deletes` | 插入落在另一个事务（或落在已提交之后无法回滚）：用户事务路径与自动事务路径都破坏原子性，产生**真正没法回滚的脏数据** |
| 4 | 复用 `canonicalize_changes` 处理插入，于是「全列 `Unset` 的行」被当成「无改动」丢掉 | 独立的 `canonicalize_inserts`；全 `Unset` 在 INSERT 语义下是合法的 `DEFAULT VALUES`，只能由 V-3 的 NOT NULL 规则否决 | 用户点「提交」后没有错误、也没有插入（静默丢行），高发在「只想插一行全默认值」的场景 |
| 5 | 给 `Unset` 的列补 `NULL` 以「凑齐列」 | `Unset` 列完全不出现在语句里 | 自增列、`DEFAULT now()`、生成列全部失效或直接报错（正是契约 §3.1 要消灭的动机） |
| 6 | 让插入也走 `affected == 1` 的严格校验，并把 0 当失败 | 插入的判据是「语句执行未报错」+ 拒绝 `affected > 1`；`affected == 0` 记 warning 不作失败 | 不上报行数的驱动（如 ClickHouse）上所有插入提交都被拒；或者反过来，把真正的失败当成 0 放过去 |
| 7 | `insert_returning_clause` 返回 `None` 时「猜」主键（例如取当前页最大主键 + 1） | `None` 时**绝不猜**：走「提交后重新拉当前页」的降级路径，UI 明确不承诺高亮 | 并发插入下高亮/身份指向错误的行，后续基于该身份的编辑会改错行 |
| 8 | 把 SQL Server 的 `OUTPUT INSERTED.*` 当普通子句**追加**在 `VALUES` 之后 | 由驱动的 `build_insert_sql` 内联到列清单与 `VALUES` 之间，`insert_returning_clause` 返回 `Some("")` 表示「已内联、请用查询路径读」 | T-SQL 语法错误，或（更糟）回填读不到行导致每次提交都报 `returningUnavailable` |
| 9 | 新增 trait 方法后忘了在 `ReuseDriver` 里补转发 | 在 `ReuseDriver` 的 impl 里显式转发 `build_insert_sql` / `insert_returning_clause` | 所有 MySQL 协议族包装驱动（doris / starrocks / oceanbase…）退回默认实现，`DEFAULT VALUES` 在 MySQL 上直接语法错误 |
| 10 | 把草稿行塞进 `pendingChanges`，或写进 `ts.rows` | 独立的 `draftInserts` + 派生渲染，永不写 `rows` | 无主键表上 `buildRowIdentity`/`rowIdentityKey` 抛错或静默失效；真实行集合被污染（导出、去重、排序、行数统计全部错） |
| 11 | 新增 trait 方法时顺手改了 `build_update_sql` 的签名或 `Value` 的序列化形态，却没提升 `PROTOCOL_VERSION` | 只新增不动既有签名；确需改既有形态时**必须**提升版本并同步所有 git 驱动 `ref` | 版本号与实际契约不符，宿主与插件在「接受的版本区间」内却互相误解协议 |
| 12 | 后端报错写成 `no writable column` 这类没有前缀的裸文本，前端用 `includes('column')` 判断 | 后端一律 `grid.<域>.<原因>: ` 前缀；前端用**前缀匹配**的 `classifyGridError()`，未知回退显示原文 | 文案里恰好出现同一个词就误判；错误分类在两侧都无法逐字断言 |

---

## 11. 未决问题

| # | 问题 | 建议选项 |
| --- | --- | --- |
| Q-1 | **回填子句的位置**：契约 §5.1 只冻结了 `insert_returning_clause()`，宿主「追加」的语义对 T-SQL 不成立（`OUTPUT` 必须在列清单与 `VALUES` 之间）。是否新增位置枚举/新 trait 方法？ | 建议**不新增**：维持「`Some("")` = 已内联、宿主用查询路径读」的约定，在 `traits.rs` 文档注释写清并由驱动测试锁死（零新增方法、不破冻结签名）。备选：新增 `insert_returning_placement()`（带默认实现，不破兼容）——需要人工裁定是否值得为此扩契约 |
| Q-2 | **MySQL 族的主键回填**：契约把 MySQL 归入「用 `LAST_INSERT_ID()`，由驱动另行处理」，但 §5.1 没有任何「插入后再查一次」的入口，因此 MySQL 只能返回 `None` 走降级（无高亮）。 | 建议本册先按 `None` 降级交付（不猜、不假装）；若产品要求 MySQL 也高亮，另开一条契约改动（新增 `post_insert_identity_query(&self) -> Option<String>` 类方法，带默认实现，不破兼容）——**不属于本册范围**，需人工确认优先级 |
| Q-3 | **SQLite 的 `RETURNING` 版本门槛**：SQLite 3.35 起支持 `RETURNING`，更早版本语法错误。驱动该在连接时探测版本，还是一律返回 `None`？ | 建议**在连接时读版本并缓存到驱动实例**，`insert_returning_clause` 按版本返回 `Some`/`None`；不引入新 trait 方法。备选：一律 `None`（最保守，退回降级路径） |
| Q-4 | **草稿行与提交结果的关联**：`PlannedStatement` 刻意不加 `draftId`，前端靠「插入结果与 `plan.inserts` 同序」对齐。是否要显式字段？ | 建议**不加字段**（复用 PRD 已定的 `PlannedStatement`，靠顺序契约 + 测试锁死）；若人工裁定要更强健的关联，再加 `#[serde(default)] draft_id: Option<String>` 到 `PlannedStatement` 与 `RowCommitStatementResult`（纯新增，不破兼容） | **本条同时决定了指纹的形状**：`draft_id` 不能进指纹 payload（计划里没有它，重建时拿不回来），所以 §3.4.1 把它结构性排除；代价是「两条内容相同的草稿互换位置」不会被指纹察觉，而这在执行上等价。**若人工裁定改为加 `draftId` 字段**，则 §3.4 的 payload 必须同步改成包含它，且 `plan_inserts` 要能重建出该字段——两处要一起改，只改一处就是 §3.4.1 说的全盘 `stalePlan` |
| Q-5 | **`src/lib/gridErrors.ts` 的归属**：01 分册把它登记为「由 04/05 首次建立」，03 分册的文件清单里已把它列为新增（冲突）。 | **已裁定并关闭：归属冻结给 01**（契约 §8.1 归属总则）。理由不是「以先落地者为准」——那仍然依赖不可知的合并时序——而是 **01 是提交顺序里唯一最早的分册**，因此无论并行开发如何交错，它必然先落地。本册只需按三步追加 `grid.insert.*`，无需请求任何分册为它让路。**附带硬约束：禁止整文件重写该文件**（并行下会静默抹掉他人码值且无编译错误） |
| Q-6 | **执行顺序是否要驱动可声明**：PRD 提到「顺序固定 inserts → updates → deletes；需要别的顺序由驱动声明」，但 00-contracts §5 冻结的新增方法只有 5 个，其中没有顺序声明入口。 | **已裁定并冻结顺序声明**：① 顺序**恒为** `inserts → updates → deletes`，同一类内部按 `plan` 数组顺序（插入按请求顺序，即前端草稿视觉顺序）；② **不提供驱动声明入口**——契约 §5 不加方法，宿主**禁止**按 `driver_type()` 分支（违反零硬编码）；③ 确需别的顺序时，走**新的契约改动**（新增带默认实现的方法，默认实现即本顺序，所以未覆写的驱动零影响），不得在宿主里私开分支；④ 这条顺序与「同一事务内执行」是两件事：原子性见 Step 5 的陷阱 2，顺序见本条，两条都要有测试 |
| Q-7 | **`affected == 0` 的容忍度**：本册按「驱动不上报计数」容忍 0 并记 warning。是否要更严格的「至少 1」？ | 建议保持容忍：真正的失败一定表现为 `driver.execute`/`query` 返回 `Err`，而 0 行影响对单行 `VALUES` 无信息量；严格化会误杀一批不上报计数的驱动（且违背契约 §8.2 把 INSERT 排除在 `affected != 1` 之外的口径）。**需要人工确认可接受** |
