# 04 · 新增行 INSERT（DB-04）

> **状态**：目标设计，**未实现**（本册只冻结设计，不改代码）。
> **依赖**：`00-contracts.md` —— §1.4（提交链路与五个必须记住的事实）、§3〔落地校正〕（三态写入语义由「`changed_columns` 成员关系 + `Option<Value>`」承载）、§4.5（W-1…W-6，本册的主契约）、§5.1（`build_insert_statement` / `insert_returning_clause` 最终签名，已冻结）、§8（错误前缀机制 `grid.<域>.<原因>: `）。
> **〔契约审计修订 2026-10-21〕T2-3 / T2-4 / R-C / R-D：本节已从「读的时候自行替换」的补丁升级为正文同步**——§3.1 / §3.2 / §3.3 / §3.4 / §3.6 与 §5 各步已全部改写成下面这套形态，正文里不再残留旧写法：
> - 驱动方法：`build_insert_statement(&self, table: &str, columns: &[(&str, Option<Value>)]) -> Result<BoundSqlStatement, DriverError>`（与 `build_update_statement` / `build_delete_statement` 同构；`ReuseDriver` 逐方法转发；同一个 `max_bound_parameters()` 预算，超预算**在返回 SQL 之前**失败）；
> - **三态由两件事承载**：列不在 `columns` 切片里 = 不写（INSERT 时用库默认：自增 / 生成列 / `DEFAULT` / 触发器）；出现且 `None`（或 `Some(Value::Null)`）= 写 SQL `NULL`；出现且 `Some(v)` = 写 `v`。`changed_columns` 的派生规则仍然有效；
> - **不存在**三态枚举类型、其构造器、以及任何「把插入值拼进 SQL 文本」的 builder（R-D）。`packages/driver-api/src/types.rs` 因此**本册零改动**；
> - **新行草稿在线上就是 `PendingRowChange`（`insert_marked = true`）**，不新增独立 DTO、**不给** `preview_pending_changes` 加第 4 个参数——IPC 形状不变（T2-4）。
> **实装已确认的额外事实**（`feature/insert-backend` 分支，按代码为准）：`RowChangePlan.inserts`（`#[serde(default)]`）与 `PendingRowChange.insert_marked`（`#[serde(default, skip_serializing_if = "is_false")]`，`false` 时不进 JSON，故**不含插入的旧计划指纹逐字节不变**）已落地；`insert_returning_clause` 只有契约默认 `None` + `ReuseDriver` / `MockDriver` 转发，**宿主尚未调用**，因此插入结果的 `row_identity` 恒为空、也没有真正的 RETURNING 回填；执行器对 INSERT **同样**要求 `affected == 1`（R-C，**不豁免**）。
> **被谁依赖**：`10-result-grid.md`（DB-14 查询结果网格可编辑，依赖 01/02/03/**04**/05/06 全部就位）。`03-clipboard.md` 的 `Paste rows` 无区域路径复用本册 INSERT 管线。
> **预估人日**：4.0 人日（拆分 `data.rs` 0.5 / `driver-api` 契约与默认实现 0.5 / 宿主计划与执行 1.0 / 驱动回填覆盖 0.5 / 前端 store 与 UI 1.0 / 测试 0.5）。
> **不包含**：类型感知编辑器与 `Set Value`（NULL / DEFAULT / NOW() 快捷项，属 05）；粘贴矩阵生成新增行计划（属 03，只复用本册管线）；多行 `VALUES` 批量拼接（宿主只产出单行语句，批量语义归驱动覆盖）；`LAST_INSERT_ID()` 类「插入后再查一次」的回填（契约 §5.1 没有该入口，见 §11 Q-2）；无主键表的 UPDATE/DELETE（仍然禁止，本册只放开 INSERT）。

---

## 1. 目标与验收口径

**一句话目标**：让用户在数据网格里新增草稿行，并把草稿行作为 **INSERT** 语句与既有 UPDATE / DELETE 一起走「预览 → 指纹 → 确认」提交链路，落在**同一个事务**里，成功后被正确回填或明确降级，且**绝不会出现「界面显示成功、库里什么都没有」**。

### 1.1 验收标准（命令 + 期望输出）

| # | 命令 | 期望输出 / 期望结果 |
| --- | --- | --- |
| A1 | `cargo test -p datazen-feature-core --lib` + `cargo test -p datazen --lib` | 两个退出码均为 `0`；`test result: ok`；新增用例名含 `insert` 的全部出现在通过列表（〔契约审计修订 2026-10-21〕T0-2/T0-3：计划/执行逻辑在 `feature-core`，宿主只留命令壳用例） |
| A2 | `cargo test -p datazen-driver-api --lib` | 退出码 `0`；`sql_builder` / `traits` / `reuse` 的 `build_insert_statement` 默认实现、预算失败与 `ReuseDriver` 转发用例全部通过（〔契约审计修订 2026-10-21〕T2-3：原断言指向已废弃的字面量 builder，改为参数化 seam） |
| A3 | `cargo test -p datazen-driver-postgres --lib` / `-p datazen-driver-mysql` / `-p datazen-driver-sqlite` / `-p datazen-driver-sqlserver` | 退出码均为 `0`；每个 crate 至少有 1 个 INSERT 三态断言（`NULL` vs `DEFAULT` vs 空串） |
| A4 | `npx vitest run src/lib/__tests__/tableChanges.test.ts src/stores/__tests__/tableDataStore.test.ts src/stores/__tests__/draftInserts.journey.test.ts` | 退出码 `0`；`Test Files` 全通过；含草稿行计数与「草稿上线成 `insertMarked`」用例（〔契约审计修订 2026-10-21〕T2-4：不再有独立的 `inserts` 请求字段） |
| A5 | `pnpm typecheck` | 退出码 `0`，**无 `error TS`**（测试文件也在检查范围内） |
| A6 | `pnpm e2e:minimal` | 退出码 `0`；`e2e/specs/table-insert.ts` 用例全部 `passed` |

### 1.2 可验证的行为口径

1. 在无主键表上点「+ 行」→ 可以编辑、可以提交；同表的既有行双击仍然不进入编辑，`UPDATE`/`DELETE` 仍被拒绝（`tableData.noPrimaryKey`）。
2. 只在自增主键列留空（该列**不进** `changed_columns`）、其余列填值 → 提交后新行出现且主键由数据库分配；语句里**不出现**该自增列。
3. 提交一批「2 插入 + 1 更新」→ 后端 `affected_rows == 3`、`statements` 顺序为 `insert, insert, update`；任一条失败时**整批回滚**，插入的行不残留。
4. **本批不做 RETURNING 回填**〔契约审计修订 2026-10-21〕裁定 7：`insert_returning_clause` 只是**预留缝隙**（已实装默认 `None` + `ReuseDriver` / `MockDriver` 转发，**宿主尚未调用**），因此所有驱动在本批的行为一致——提交成功后**刷新当前页**，再按「`row_identity` 非空 → 用它定位；否则尝试用最大主键定位」的顺序找新行；**定位不到就明确降级为「不定位」**并附 `grid.insert.returningUnavailable` warning，界面**不得**声称能精确定位新行、也不承诺高亮。
5. 该 `dbSessionId` 上已有用户手动事务时提交（即调用方传入 `manual_handle: Some(handle)`）→ 后端**不** `begin_transaction`/`commit`（`MockDriver::open_transaction_count()` 不增长），插入与更新并入那个手动事务。〔契约审计修订 2026-10-21〕裁定 1：旧稿写「已有用户事务」，用词对应一张已不存在的会话事务表；真实入口是 `commit_pending_changes` 的可选 `manual_handle` 形参。
6. 回归证明：`inserts` 非空、`updates`/`deletes` 均为空时，`execute_row_change_plan_impl` **必须真的执行语句**（`MockDriver::execute_calls() == 1`），不得提前返回成功。

---

## 2. 现状代码事实

> 本节只列**我亲自读过**的符号。**不含行号**，不做推测。凡「没有」的结论都由 grep 给出。

### 2.1 `src-tauri/src/commands/data.rs`（〔合并后校正〕现状 **640 行**，按 `#[path]` 挂载子模块）

> ## 〔合并后校正〕写路径已经不是「executor 直连 driver」
>
> 下文第 1–14 条是**合并前**的事实，其中 2 / 3 / 11 / 12 / 14 已被主干推翻。今天的真实链路：
>
> 1. **`data.rs` 只生产计划**：`build_row_change_plan`（`commands/data.rs`）调 `build_update_statement` / `build_delete_statement`，产出 `RowChangePlan`；该结构已**新增 `config_revision: Option<Counter>`**，作为提交时的配置版本门。`validate_legacy_pk_columns` **已随遗留入口删除**（见第 3 条）。
> 2. **执行归 `packages/feature-core/src/data_browsing/write_exec.rs::execute_plan`**〔契约审计修订 2026-10-21〕原 `src-tauri/src/commands/data_session.rs` 已整文件删除、逻辑等价迁入 feature-core；`src-tauri` 只做 command 适配：它先 `if writes.is_empty()` 早退，再用 `bound_statement_for_plan` 把计划编译成 **`BoundSqlStatement`**（参数化，`driver.supports_bound_writes` / `parameter_placeholder` / `max_bound_parameters`），然后**要么复用调用方给的 `manual_handle`，要么自己 `runtime.open_session` 开一个短命会话**并在其上 `platform.beginTransaction{defersCommit:true}` / `commitTransaction`。**`AppState` 上那张会话事务表已不存在**（没有可查询的「会话是否已开事务」状态），也没有 `driver.execute(handle, "BEGIN")` 的裸 SQL 退化路径。
> 3. **遗留写入口仍然注册但恒失败**：`commit_row_updates` / `commit_row_deletes` 两个 Tauri 命令还在 `generate_handler!` 里，但实现直接返回 `"Legacy row writes are disabled; preview and commit a target-bound ChangeSet"`。**它们不再是可用的写路径**，04 的任何设计都不得挂回它们；C-1「删除遗留入口」的动作因此降级为「删掉这两个恒拒绝的桩」。
> 4. **行身份护栏在 `execute_plan` 里**：每条语句执行后要求 `affected == 1`，否则 `Row identity affected {affected} rows; refusing ambiguous write`。第 4 条描述的位置变了，语义不变。
> 5. **`build_delete_statement` 目前没有空主键守卫**（`build_row_change_plan` 侧也没有 `validate_legacy_pk_columns` 兜底）——这是本册必须补的第一件事，且要补在 `commands/data.rs` 的计划构建处，**不是**在 `execute_plan` 里补。
> 6. 测试现在挂在 `#[path = "data/tests.rs"] mod tests;`，现有 19 例，含 `commit_row_updates_does_not_join_open_session_transaction`——它证明的正是第 3 条：遗留入口**不再**并入用户会话事务。
>
> **对本册各步的净影响**：Step 1「目录化 + 迁移 `validate_legacy_pk_columns`」→ 改为「在现有子模块布局里新增 `row_change_plan.rs` / `row_change_insert.rs`，并给 `build_delete_statement` 补守卫」；Step 4「扩展空计划早退」→ 改 `feature_core::data_browsing::write_exec::execute_plan` 的 `if writes.is_empty()`；Step 5「事务归属」→ 改为「随 `execute_plan` 的 `defersCommit` 语义，INSERT 与 UPDATE/DELETE 必须落在**同一次** `beginTransaction`/`commitTransaction` 内」；Step 6 的「不自己 begin/commit」结论仍然成立，但依据换成 `execute_plan` 的会话所有权。

**DTO（全部已存在）**：`CellUpdate`、`RowUpdateBatch`、`RowDeleteBatch`、`RowChangeTableContext`、`PendingRowChange`、`PlannedStatement`、`ChangeWarning`、`RowChangePlan`、`CommitPendingChangesRequest`、`RowCommitStatementResult`、`CommitPendingChangesResponse`。

**关键事实（逐条已核）**：

1. `RowChangePlan` 的字段只有 `plan_id` / `fingerprint` / `table` / `updates` / `deletes` / `warnings`，**没有 `inserts`**。
2. **〔合并后校正：本条已失效，见上方校正块〕** 是 `if plan.updates.is_empty() && plan.deletes.is_empty()` → 直接 `Ok(... affected_rows: 0 ...)`，**不开启事务、不执行任何语句**。加 `inserts` 后这里必须同步扩展（§5 Step 4）。
3. **〔合并后校正：本条已失效，见上方校正块；〔契约审计修订 2026-10-21〕裁定 1 已改成实测形态〕** 事务归属**只有两条路径，入口是一个可选的手动句柄**，不存在会话事务表：
   - **路径 A（有手动句柄）**：`commit_pending_changes` 收到 `manual_handle: Option<SessionHandle> = Some(h)` → 校验目标（`database` / `schema` / `connection` 与计划一致）且该句柄处于 `TransactionState::Active` → 把语句**并入**该事务（`platform.beginTransaction` 的 `defersCommit: true` 语义），**不自行 begin、不自行 commit**，错误上抛给用户决定回滚；
   - **路径 B（无手动句柄）**：`manual_handle == None` → 自行开一次**会话级**事务（`runtime.open_session` + `platform.beginTransaction { defersCommit: true }`），全部语句成功后 `commitTransaction`，任一条失败则回滚后返回错误；
   - 两条路径共用同一段 `async` 块，因此 INSERT 与 UPDATE/DELETE 天然落在**同一次** `beginTransaction` / `commitTransaction` 内；
   - **没有** `driver.begin_transaction` / 裸 `"BEGIN"` 的退化路径，也**没有** `AppState` 上的会话事务表（`src-tauri/src/platform/handles.rs` 里那条「生产代码不得出现会话事务表」的守卫用例就在钉这件事）；调用方要参与事务只能传 `manual_handle`。
4. 该函数对 `updates` 与 `deletes` 各要求 `affected == 1`，否则 `CommandError::Validation`，文案为 `UPDATE for one row identity affected {affected} rows; refusing ambiguous write`（DELETE 同构）。语句收集容量是 `plan.updates.len() + plan.deletes.len()`。
5. `build_row_change_plan(driver, table, changes)` 只遍历 `canonicalize_changes(changes)` 的结果；对每条变更按 `delete_marked` 只调用 `driver.build_delete_sql` 或 `driver.build_update_sql`；行内以 `current_values` + `changed_columns` 组装 `set_columns: Vec<(&str, Option<Value>)>`；`parameter_summary` 由 `SET <列>=<值摘要>` 与 `PK <列>=<值摘要>` 拼接；随后追加 `delete-rows` 之类的 `ChangeWarning`。
6. `changes_fingerprint(table, changes: &[PendingRowChange])` 用**函数内局部结构体 `FingerprintPayload { table, changes }`** 做 `serde_json::to_vec` 再 `Sha256`。指纹**只覆盖该 payload**，正文 SQL 与 `parameter_summary` 不参与。
7. `validate_immutable_plan(driver, db_session_id, plan, fingerprint)`：校验 `plan_id` 非空、`table.db_session_id` 一致、`plan.fingerprint == fingerprint`，然后**用 `plan_changes(plan)` 从不可变的变更元数据重建整张计划**，要求 `rebuilt.fingerprint == fingerprint`，最后把 `plan_id` 还原。**重建是信任边界**：客户端给的 `sql_template` / `parameter_summary` 只是展示字段。
8. `plan_changes(plan: &RowChangePlan) -> Vec<PendingRowChange>` **只链 `updates` 与 `deletes`**。
9. `canonicalize_changes` 会：校验 `row_identity` 非空且非 NULL（`identity_key`）、排序并去重 `changed_columns`、**丢掉 `changed_columns` 为空且非删除的行**、校验 `effective_identity` 无重复/碰撞、最后按 `identity_key` 排序。→ **插入行没有身份，不能走这个函数**。
10. `identity_key` 对空 map 直接报 `Row changes require primary-key identity`。
11. **〔合并后校正：本条已失效，见上方校正块〕**  `state.connection_manager.get_session(&table.db_session_id)` 拿到 `(driver, _handle)`，然后调用 `build_row_change_plan`；注释明确「preview 对数据库是纯的：不开事务、不执行语句」。**它目前拿不到任何列元数据。**
12. **〔合并后校正：本条已失效，见上方校正块〕** ：读 session/driver/handle、`get_session_config`、`owner_connection_id` 与 `table.connection_id` 比对、`driver_type` 双比对、`database` 比对、显式 `schema` 比对、`read_only` 拒绝，然后 `validate_immutable_plan`。**只读拒绝的文案是 `Connection is read-only; row changes are not allowed`（没有 `grid.` 前缀）。**
13. `#[cfg(test)] mod tests` 使用 `crate::testing::app_state::TestAppState`，现有用例包括 `preview_builds_driver_sql_without_opening_a_transaction`、`commit_reuses_plan_fingerprint_and_returns_plan_id`、`commit_rejects_stale_fingerprint_before_execution`、`preview_rejects_changes_without_primary_key_identity`、`canonicalize_rejects_null_and_duplicate_composite_identities`、`canonicalize_rejects_composite_current_identity_collision`、`commit_row_updates_joins_open_session_transaction` 等。`commit_reuses_plan_fingerprint_and_returns_plan_id` 的写法是 `options.execute_rows_affected = 1` 后断言 `statements.len() == 2`、`affected_rows == 2`。
14. **〔合并后校正：本条已失效，见上方校正块〕** ——本册要给它加第 4 个参数，这些调用点必须同步改（§5 Step 3）。

### 2.2 `packages/driver-api`（契约层）

- `packages/driver-api/src/traits.rs`：`DatabaseDriver` trait 已声明 `quote_char`、`quote_ident`、`skip_count_query`、`supports_offset`、`pagination_syntax`、`supports_explain`、`has_schema_level`、`has_multi_database`、`default_schema`、`ddl_atomicity`、`format_sql_literal`、`build_update_sql`、`build_delete_sql`、`query`、`query_multi`、`execute`、`begin_transaction`、`commit`、`rollback`；文件顶部有 `mod sql_text;`。**本册新增的插入 builder 与回填子句在基线中均不存在**（〔契约审计修订 2026-10-21〕T2-3：旧稿在这里点名了一个已废弃的字面量方法，现只描述「缺失」这一事实）。
- `packages/driver-api/src/traits/sql_text.rs`：`quote_ident`（按 `quote_char` 选 `` ` `` 或 `"`，并把引号字符翻倍）、`format_sql_literal`（`None | Some(Null)` → `NULL`，字符串/时间戳/JSON 都先 `replace('\\', "\\\\")` 再 `replace('\'', "''")`，Bytes → `X'…hex…'`）、`build_update_sql`（`UPDATE {q(table)} SET {…} WHERE {…}`，列名与值分别经 `driver.quote_ident` / `driver.format_sql_literal`）、`build_delete_sql`（同构 `DELETE FROM {q(table)} WHERE {…}`）、`pagination_syntax`、`qualified_sql`、`split_restore_sql`，以及私有 `bytes_to_hex`。
- `packages/driver-api/src/reuse.rs`：`ReuseDriver`（MySQL 协议族 / PostgreSQL wire 的兼容包装器）**逐方法转发**给 `inner`，其中就有 `pagination_syntax`、`format_sql_literal`、`build_update_sql`、`build_delete_sql`。→ **新增 trait 方法如果不在 `ReuseDriver` 里补一行转发，包装驱动就会丢掉内层驱动的覆盖。**
- `packages/driver-api/src/types.rs`：`Value`（`#[serde(untagged)]`，`Null`/`Bool`/`Integer`/`Float`/`String`/`Bytes`/`Timestamp`/`Json`，`Default = Null`）、`ColumnSchema { name, data_type, nullable, default_value: Option<String>, comment, is_primary_key, is_auto_increment }`、`TableSchema::effective_primary_keys()`、`TableDataResult`、`PaginationSyntax`。**没有任何三态写入类型，本册也不引入（R-D）。**
- `packages/driver-api/src/lib.rs`：`PROTOCOL_VERSION: u32 = 6`（旧稿写 5）、`MIN_PROTOCOL_VERSION: u32 = 1`。〔合并后校正〕判定已不在 `capabilities.rs` 的成对函数里：注册期在 `packages/runtime/src/driver_registry/registry.rs`——**低于 MIN → 拒绝**，**高于 `PROTOCOL_VERSION` → 只 warn 并继续加载**，低于当前版本 → warn「degraded mode」并继续加载。协议自 2026-10-21 冻结于 6（[driver-capability-migration.md](../../architecture/platform/driver-capability-migration.md)）。
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

- `src/lib/tableChanges.ts`：`RowIdentity`、`TableChangeContext`、`PendingRowChange`（`rowIndex?` / `rowIdentity` / `originalValues` / `currentValues` / `changedColumns` / `deleteMarked`；本册加 `insertMarked?`）、`PlannedStatement`、`ChangeWarning`、`RowChangePlan`、`CommitStatementResult`（`operation: 'update' | 'delete'`）、`CommitPendingChangesResponse`、`PendingStatus`、`CommitPendingChangesResult`、`isCompleteTableChangeContext`、`tableChangeContextKey`、`buildRowIdentity`、`rowIdentityKey`、`duplicateRowIdentityKeys`、`valuesEqual`、`clonePendingRowChange`（私有 `valueForColumn` / `stableSerialize` / `isStableIdentityValue`）。**没有三态写入类型，也没有草稿行概念；`PendingRowChange` 尚无 `insertMarked` 字段。**
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
- `src/lib/gridErrors.ts` **在基线中不存在**；其**创建归属已由契约 §8.1 归属总则冻结给 01**（01 是提交顺序里最先合并的分册）。〔合并后校正〕**但 01 尚未落地，该文件今天仍不存在**——若本册先合并则由本册按契约 §8 建骨架。无论谁先，本册都**只有「追加」这一种合法动作**：按 §8.1 三步追加 `grid.insert.*` 码，**禁止整文件重写**（整文件覆盖不产生编译错误，只会静默抹掉其他分册的码）。原 §11 Q-5 的归属冲突**已关闭**，见该条。

### 2.6 权威契约与 PRD 原文

- `00-contracts.md` §1.4 的五个事实、§3.5「插入 builder 默认实现的 4 条语义要求」（〔契约审计修订 2026-10-21〕T2-3：方法名现为 `build_insert_statement`）、§3.6「宿主必须拒绝的组合」、**§4.5 写路径硬约束 W-1…W-6（本册的主契约，Step 1 的迁移清单与 §7 的第 17–20 条都引它）**、§5.1 冻结签名、§8.2 错误码清单与文案纪律、§9.1 共用文件归属与追加纪律、§10 本册模板。
- `docs/prd/data-browsing-optimization-prd.md` DB-04 原文：入口为「工具条 `+ 行` / `⌘/Ctrl+I` / 双击最后一行下方空白区」；草稿行可编辑所有非自增列、自增与有默认值的列显示 `DEFAULT`；支持多行草稿、`⌘+D` 复制、`Delete` 删除；提交与 UPDATE/DELETE 同事务同流程、INSERT 影响行数同样要求为 1；**顺序固定 inserts → updates → deletes**；**无主键表允许插入**；`VirtualBody` 渲染 `data-testid="draft-row-N"`；驱动专属测试落 postgres / mysql / sqlite / sqlserver。
- **签名冲突提示**〔契约审计修订 2026-10-21〕T2-3 / R-D：PRD 原文写的是一个「列 + 三态值」的拼字符串 builder（返回 `String`），而 `00-contracts.md` §5.1 冻结的是 `fn build_insert_statement(&self, table: &str, columns: &[(&str, Option<Value>)]) -> Result<BoundSqlStatement, DriverError>`。**以 00 为准**（00 明确「以下方法签名是最终形态，分册不得改动」）；拼字符串的旧形态已废弃，本册任何位置都不得再引用它。

---

## 3. 数据结构与接口设计

### 3.1 写入意图的载体：**没有新增类型**（取代原三态枚举）

> **〔契约审计修订 2026-10-21〕T2-3 / R-D**：本节原文定义了一个三态枚举并把它放进 `packages/driver-api/src/types.rs`，还给了 `#[serde(tag, content)]` 的线上形态。该类型**不落地**：真实写缝是 `build_update_statement` / `build_delete_statement` / `build_insert_statement`，入参都是 `&[(&str, Option<Value>)]`，**三态由「列名是否在这个切片里」+ `Option<Value>` 承载**；再造一个类型等于给同一件事开第二条通路。`packages/driver-api/src/types.rs` 因此**本册零改动**（原稿的 `+45` 行一并作废）。

三态在两个方向上的一致编码（这是本册唯一的权威表）：

| 意图 | 请求 / 计划（`PendingRowChange`） | 驱动入参（`columns` 切片） | 渲染与执行 |
| --- | --- | --- | --- |
| 写具体值 | `changed_columns` 含该列，且 `current_values[col] = Some(v)` | `(col, Some(v))` | 列出现在 `INSERT INTO t (…)`，值经 `BoundSqlStatement.parameters` 绑定 |
| 写 SQL `NULL` | `changed_columns` 含该列，且 `current_values[col] = None` | `(col, None)` | 列出现在语句里，绑定 `Value::Null` |
| 不写（用库默认） | `changed_columns` **不含**该列（`current_values` 里的残留不参与） | 列名**不出现**在切片里 | 列不出现在语句里，由数据库填默认值 / 自增 / 生成列 / 触发器 |

三条必须写进代码注释的约束（原枚举的文档注释内容，**不得丢失**）：

1. `None` 与「列不在清单里」在 SQL 里是两件不同的事：前者是 `NULL`，后者是 `DEFAULT`。把它们混成一件事，就是区域粘贴把用户没选中的格**清空**、以及自增列失效的根因。
2. **禁止「凑齐列」**：把不写的列补成 `None` 会让自增主键、`DEFAULT now()`、生成列全部失效——这正是本册存在的原始动机。
3. `PlannedStatement.current_values` 的键集必须**恰好等于** `changed_columns`（`canonicalize_insert` 用 `retain` 强制）；否则「有没有这一列」这个三态判别就失去了唯一权威。

### 3.2 `build_insert_statement` / `insert_returning_clause`（冻结签名 + 默认实现）

> **〔契约审计修订 2026-10-21〕T2-3**：本节原文是一个「把值拼进 SQL 文本」的 builder（返回 `String`，值经 `format_sql_literal` 变成字面量）。该路径**已废弃**：值与 SQL 文本必须分离，否则同一条语句在「值里带引号/反斜杠/换行」时既难测又不安全，并且与已落地的 update/delete 参数化 seam 分叉。下面是与 `build_update_statement` 同构的最终形态。

```rust
// packages/driver-api/src/traits.rs —— 签名照抄 00-contracts §5.1，不改一个字符
/// 构造一条插入语句，值与 SQL 文本分离（与 build_update_statement / build_delete_statement 同构）。
/// `columns` 是**精确的写入列清单**：要保留数据库默认值的列（自增 / 生成列 / `DEFAULT now()` / 触发器）
/// 直接不出现；`None` 写 SQL NULL。空切片是合法的一行「全默认值」插入。
fn build_insert_statement(
    &self,
    table: &str,
    columns: &[(&str, Option<Value>)],
) -> Result<BoundSqlStatement, DriverError> {
    if !self.supports_bound_writes() {
        return Err(DriverError::Unsupported(
            "parameterized row writes are not supported".into(),
        ));
    }
    crate::sql_builder::build_insert_statement(
        &LegacySqlDialect::new(self),
        self.max_bound_parameters(),
        table,
        columns,
    )
}

/// 方言的新增行主键回填子句；`None` 表示该方言无法回填。
fn insert_returning_clause(&self, columns: &[&str]) -> Option<String> {
    None
}
```

```rust
// packages/driver-api/src/sql_builder.rs（新增函数，与 build_update_statement 同址同风格）
pub fn build_insert_statement(
    dialect: &dyn SqlDialect,
    max_parameters: usize,
    table: &str,
    columns: &[(&str, Option<Value>)],
) -> Result<BoundSqlStatement, DriverError> {
    let mut parameters = Vec::new();
    let mut names = Vec::with_capacity(columns.len());
    let mut placeholders = Vec::with_capacity(columns.len());
    for (column, value) in columns {
        names.push(dialect.quote_ident(column));
        placeholders.push(bind_parameter(dialect, max_parameters, &mut parameters, value)?);
    }
    if names.is_empty() {
        // 不通用：MySQL 不支持 DEFAULT VALUES，由该驱动覆盖（契约 §3.5 第 2 条）。
        return Ok(BoundSqlStatement {
            sql: format!("INSERT INTO {} DEFAULT VALUES", dialect.quote_ident(table)),
            parameters,
        });
    }
    Ok(BoundSqlStatement {
        sql: format!(
            "INSERT INTO {} ({}) VALUES ({})",
            dialect.quote_ident(table),
            names.join(", "),
            placeholders.join(", "),
        ),
        parameters,
    })
}
```

**与 `build_update_statement` 的风格一致性（硬要求）**：表名与列名**只**经 `dialect.quote_ident`；值**只**经 `bind_parameter` 变成占位符 + `parameters`，**绝不**经 `format_sql_literal` 进 SQL 文本；不拼接分号；不做 schema 限定（目标限定由既有会话/调用方负责）；超过 `max_bound_parameters()` 时**在返回 SQL 之前**报 `DriverError::Unsupported`；不引入任何按 `driver_type()` 的分支（零硬编码）。

| 方法 | 语义 | `None` / 空语义 |
| --- | --- | --- |
| `build_insert_statement` | 返回 `BoundSqlStatement { sql, parameters }`；空切片 → `INSERT INTO t DEFAULT VALUES` | 超预算 → `Unsupported`，不返回半成品；MySQL 的 `()` 形式由驱动覆盖 |
| `insert_returning_clause(columns)` | 返回**追加在插入语句尾部**的方言子句（例：`RETURNING "id"`） | `None` = 该方言不能回填 → 宿主走降级路径 |
| `insert_returning_clause` 返回 `Some("")` | **已内联信号**：子句已被驱动的 `build_insert_statement` 写进语句内部（SQL Server 的 `OUTPUT INSERTED.*` 只能出现在列清单与 `VALUES` 之间），宿主**原样**执行语句且**必须**用查询路径读取返回行 | 宿主不得追加任何字符（含空格） |
| `ReuseDriver` | 必须显式转发 `build_insert_statement` 与 `insert_returning_clause` 到 `inner` | 漏转发 = 包装驱动退回默认实现，丢掉内层覆盖 |

**SQL Server 的位置矛盾必须正面处理**：契约规定「宿主把子句追加到插入语句」，而 T-SQL 的 `OUTPUT` 子句**不能追加在 `VALUES` 之后**。因此 SQL Server 驱动的正确实现是：覆盖 `build_insert_statement` 自行内联 `OUTPUT INSERTED.[col]`，并让 `insert_returning_clause` 返回 `Some("")` 表示「已内联、用查询路径读」。这条语义写在 `traits.rs` 的文档注释里，并由 §9 的驱动用例锁死。更干净的替代方案（新增 `InsertReturningPlacement` 枚举或第二个 trait 方法）见 §11 Q-1。

### 3.3 计划 DTO 变更：**新行用 `PendingRowChange.insert_marked` 承载，不新增 DTO**

> **〔契约审计修订 2026-10-21〕T2-4（P2）**：本节原文新增了一个独立的「插入草稿」DTO（含 `draft_id` 与三态写入 map），并给 `preview_pending_changes` 加了第 4 个参数（该 DTO 的数组）。**两者都删掉**：实装采用的是「给既有的 `PendingRowChange` 加一个布尔标记」，插入行与既有行的改动走同一个数组、同一个指纹、同一次重建。理由有三条，都不是风格问题：
> ① 多一条 IPC 形参就多一份「旧客户端不发送」的兼容分支，而 `PendingRowChange` 本来就能装下新行（空身份 + 写入列清单）；
> ② 指纹必须覆盖插入内容，而指纹只吃 `changes`；另开一个字段就要再加一个 `InsertFingerprintEntry` 投影、再处理一轮「重建拿不回 `draft_id`」的坑（旧稿为此写了整整一节）；
> ③ `draft_id` 是纯 UI 关联键，**根本不该上线**（见 §3.4.1）。

```rust
pub struct PendingRowChange {
    pub row_identity: BTreeMap<String, Option<Value>>,   // 新行：空 map
    pub original_values: BTreeMap<String, Option<Value>>, // 新行：空 map
    pub current_values: BTreeMap<String, Option<Value>>, // 新行：**只含实际写入的列**；None = SQL NULL
    pub changed_columns: Vec<String>,                    // 新行：**精确的 INSERT 列清单**（排序去重）
    pub delete_marked: bool,                             // 新行：恒 false
    /// 新增：标记这一条是「新增行草稿」而不是对既有行的改动。
    /// - 只有 `true` 才进 JSON（`skip_serializing_if = "is_false"`），
    ///   因此**不含插入的旧计划指纹逐字节不变**；
    /// - 新行没有身份，跳过 UPDATE/DELETE 的主键校验；`insert_marked` 与 `delete_marked`
    ///   同时为真属矛盾意图，预览期报 `grid.commit.conflictingIntents`。
    #[serde(default, skip_serializing_if = "is_false")]
    pub insert_marked: bool,
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

**分区与校验（`canonicalize_changes` 内部，插入分支落在共享层）**：

| 符号 | 职责 | 位置 |
| --- | --- | --- |
| `canonicalize_changes` | 遍历 `changes`：`insert_marked` → 交给 `canonicalize_insert` 并按**请求顺序**追加到末尾；其余按身份排序归并 | `change_set` 模块 |
| `canonicalize_insert(change)` | 新行的规范化：`row_identity` / `original_values` / `delete_marked` 归零；`changed_columns` 排序去重；`changed_columns` 里的每一列都必须能在 `current_values` 里找到（否则计划不变式被破坏，直接拒绝）；`current_values.retain(changed_columns)`（把用户填过又清空的格剔掉）；`delete_marked == true` → `grid.commit.conflictingIntents` | 同上 |
| `planned_insert_statement(driver, table, change)` | 调 `driver.build_insert_statement(table, &insert_values(..)?)` 得到 `BoundSqlStatement`，用它的 `sql` 当 `sql_template`；`parameter_summary` 逐列写 `INSERT <列>=<值摘要>` | 同上 |
| `bound_insert_statement_for_plan(driver, table, planned)` | **从计划元数据重建**可执行语句（绝不执行 `sql_template`） | 同上 |
| `insert_values(columns, values) -> Vec<(&str, Option<Value>)>` | 按 `changed_columns` 的顺序取出 `(列名, Option<Value>)`；缺值即计划不变式被破坏，报 `Validation`，**不得**静默补 `NULL` | 同上 |

**`PlannedStatement` 是否够用？——够用，且不新增字段。** 插入语句复用其全部字段：

| `PlannedStatement` 字段 | INSERT 语义 | 不变量 |
| --- | --- | --- |
| `row_identity` | **空 map**（插入没有行身份）；回填成功后由宿主在**执行结果**里填真实主键 | 插入语句的 `row_identity` 恒为空，前端不得据此定位草稿 |
| `original_values` | **空 map**（新行没有原值） | — |
| `current_values` | 该行**实际写入**的列 → 写值（`None` 表示 SQL `NULL`） | 键集必须**恰好等于** `changed_columns`（`canonicalize_insert` 用 `retain` 强制） |
| `changed_columns` | 出现在 `INSERT INTO t (…)` 中的列，**按列名排序** | 「不在其中」= 不写（库默认）。**NULL 与「不写」的区分由「是否在 `changed_columns` 里」承载**——这是复用 `PlannedStatement` 而不丢信息的关键 |
| `sql_template` | 驱动渲染的预览 SQL（**展示用**，执行前一律重建） | 由 `build_insert_statement(..).sql` 得到；回填子句另行追加（§5 Step 6） |
| `parameter_summary` | 形如 `INSERT <列>=<值摘要>` | 仅展示；截断 ≠ 脱敏（§7 第 22 条） |

由此可从计划**逐字重建**提交时的入参：`changed_columns` 的每一列 + `current_values[col]` → `(&str, Option<Value>)` 切片。**不需要任何额外字段**：`draft_id` 不上线（§3.4.1），`sql_template` 不参与执行。

### 3.4 指纹覆盖插入（`changes_fingerprint` 的**签名不变**）

#### 3.4.1 可重建性铁律（本节最重要的一条）

`validate_immutable_plan` 的最后一步是「用 `plan_changes` 从 `RowChangePlan` **重建整张计划**，要求 `rebuilt.fingerprint == fingerprint`」。

由此得到一条**必须先于实现写下**的约束：

> **指纹 payload 里的每一个字节，都必须能从 `RowChangePlan` 重建出来。**
> 否则每一次带插入的提交都会得到 `rebuilt.fingerprint != fingerprint`，被拒为 `grid.commit.stalePlan`——
> 表现是「插入功能完全不可用」，而报错文案指向指纹，极易被误诊成前端签名 bug。

> **〔契约审计修订 2026-10-21〕T2-4 / R-C 附带结论**：旧稿为满足这条铁律做了两件专门的修补（把 `draft_id` 结构性排除出 `InsertFingerprintEntry`、把 `Value(Value::Null)` 在请求侧规范化）。**在 `insert_marked` 方案下两件都不需要**：
> - `draft_id` **从来不上线**（它只存在于前端 store 的草稿 Map 里），因此不存在「计划里拿不回来」的问题——排除是**结构性的**，不是靠 `skip_serializing_if` 顺手漏掉；
> - `Some(Value::Null)` 与 `None` 在线上都序列化成 `null`（`Option<Value>` 的 serde 形态），也都绑定 SQL `NULL`，执行与指纹上都等价。**不需要**规范化，旧的 V-7 规则随之作废。
>
> 剩下唯一必须保住的性质是：**不含插入的计划指纹与今天逐字节相同**（`insert_marked == false` 时不进 JSON）。

#### 3.4.2 实现：`changes_fingerprint(table, changes)` —— **不加参数**

```rust
fn changes_fingerprint(
    table: &RowChangeTableContext,
    changes: &[PendingRowChange],
) -> Result<String, CommandError> {
    #[derive(Serialize)]
    struct FingerprintPayload<'a> {
        table: &'a RowChangeTableContext,
        changes: &'a [PendingRowChange],
    }
    let bytes = serde_json::to_vec(&FingerprintPayload { table, changes })
        .map_err(CommandError::Json)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
```

**仍然成立的部分**：指纹覆盖插入内容（`changed_columns` 的成员关系 + `current_values` 的值，逐条按请求顺序），否则「预览后又在同一条草稿上改了一个列值」不会被察觉，重建校验会放过一个与用户确认内容不同的计划——这是契约 §1.4 事实 5 明确禁止的「计划开裂」。多条草稿的**相对顺序**也被覆盖（数组按序序列化），所以「把两条内容不同的草稿对调」会改变指纹。

**不被覆盖的部分（是有意的）**：草稿的 UI 关联键（`draft_id`）。两条**内容完全相同**的草稿互换位置，指纹不变，而这在执行上是等价的（SQL 逐字节相同），不是漏洞。提交结果与草稿的顺序对齐由 §11 Q-4 的顺序契约保证。

### 3.5 执行结果（`RowCommitStatementResult`）

`operation` 是 `String`，**新增取值 `"insert"`**（不改字段名与类型）。`row_identity` 在插入场景：回填成功 → 真实主键；回填降级（`None` 或表无主键）→ **空 map**；用户显式写满全部主键列 → 即使不给回填子句，宿主也用用户写入的值构造该行的身份。

### 3.6 前端镜像（TS，`src/lib/tableChanges.ts`）

> **〔契约审计修订 2026-10-21〕T2-4 / R-D**：本节原文定义了三态写入类型 + 三个构造器 + 草稿值的三态 map + `PreviewPendingChangesRequest.inserts`。**全部删掉**：前端镜像必须与线上形态一致——三态由「`values` 里有没有这个键」+ `Value | null` 承载，新行经 `insertMarked` 混在 `changes` 里。

```ts
/** 一条新增行草稿（**前端 UI 形状**，不上线；上线时转成 `PendingRowChange` + `insertMarked`）。 */
export interface PendingInsert {
  /** UI 关联键，同时是 React key 与 `data-testid` 后缀的一部分。 */
  draftId: string;
  /** 草稿值。**键存在** = 该列进语句；`null` = 写 SQL NULL；键不存在 = 不写（库默认）。 */
  values: Record<string, Value | null>;
}

/** 既有类型，新增一个可选标记：`true` 才进 JSON（Rust 侧 `skip_serializing_if = "is_false"`）。 */
export interface PendingRowChange {
  // …既有字段全部不动…
  insertMarked?: boolean;
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
// src/commands/database.ts —— **请求形状不变**（T2-4）
export interface PreviewPendingChangesRequest {
  context: TableChangeContext;
  /** 既有行的改动 + 新行草稿（`insertMarked: true`）都在这一个数组里，按草稿顺序追加在后面。 */
  changes: PendingRowChange[];
}
```

**逐字段说明与缺省**

| TS 符号 | 用途 | 缺省 / 空值语义 |
| --- | --- | --- |
| `PendingInsert.values` | 逐列写入意图 | 缺省 `{}` = 一列都不写 = `DEFAULT VALUES`（是否合法由宿主按表结构裁决）。`undefined` 不能作为值出现：要表达「不写」就**删掉这个键** |
| `PendingRowChange.insertMarked` | 标记「这条是新行草稿」 | 缺省 `false`（不序列化）；为 `true` 时 `rowIdentity` / `originalValues` 必须是空对象、`deleteMarked` 必须为 `false` |
| `PendingInsert.draftId` | 草稿身份，只服务 UI；提交后据此清理与高亮 | 必填、非空、批内唯一（前端生成，`crypto.randomUUID()` 或递增计数器均可，**不得**用行下标当 id：翻页/排序会变）。**不上线**，因此不参与指纹（§3.4.1） |
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
2. 后端能复用同一个数组，**不是**因为放宽了身份校验，而是因为它在 `identity_key` **之前**就把 `insertMarked` 的行分拣出来（§3.3 的 `canonicalize_changes`）；前端 store 的 `pendingChanges` 是**按身份做键**的 Map，没有这个分拣层，草稿行放进去必然撞上「先求身份再入表」的既有路径。**线上形状与 store 形状不同不是不一致，而是分层**：store 用 `draftInserts` 管 UI 生命周期，`draftsForWire()` 在提交边界转成 `insertMarked` 的 `PendingRowChange`。
3. 两者生命周期不同：`pendingChanges` 的键是**内容派生**的（改了主键值键会变），草稿行的键是**创建时固定**的 `draftId`（还没有主键可改）。

**共存规则（必须逐条实现）**：

| 规则 | 内容 |
| --- | --- |
| R-1 | 两个 Map 互不读写；`draftInserts` 不参与 `overlayPendingRows` / `rebuildEditBuffer` / `findPendingForRow` / `rowIdentityAnchors`。 |
| R-2 | 「有未提交工作」= `pendingChanges.size > 0 \|\| draftInserts.size > 0`。`previewPendingChanges` 与 `commitPendingChanges` 的早退判据、`TableView` 的三处按钮门闸、待提交计数文案全部改用它。 |
| R-3 | 预览一致性签名从 `pendingChangesSignature(pendingChanges)` 变成 `pendingWorkSignature({ pendingChanges, draftInserts, draftInsertOrder })`；否则「预览期间拖入/删掉一条草稿」不会被察觉（沿用现有 `pendingChangesSignature` 的思路，只扩一个入参）。服务端指纹另有一层：草稿内容进了 `changes`，改了列值同样会被 `grid.commit.stalePlan` 拦住。 |
| R-4 | 提交成功后**同时**清空两者；`invalidateCachedData` 的「有暂存改动的面板不失效」判据同样改用 R-2 的口径。 |
| R-5 | 草稿行不写入 `ts.rows`（否则会被 `buildRowIdentity` / 去重 / 导出 / 排序当成真实行）；渲染层用 `rows.concat(draftRows)` 之类的**派生**方式拼出显示数据，草稿行下标永远落在真实行之后。**上线时**（`draftsForWire`）才转成 `PendingRowChange`，且转换是纯函数、不写 store。 |
| R-6 | `selectedRows` / `rowIdentityAnchors` 不包含草稿行；行级操作（删除、导出）忽略草稿行。 |

### 3.8 兼容性结论

**是否提升 `PROTOCOL_VERSION`：不需要，也不可能——版本已是 `6` 且自 2026-10-21 冻结；`MIN_PROTOCOL_VERSION` 保持 1。**〔合并后校正〕旧稿写「保持 5」。理由：

1. 只**新增** trait 方法（`build_insert_statement` / `insert_returning_clause`，〔契约审计修订 2026-10-21〕T2-3：方法名按冻结签名校正），两者都有默认实现，默认行为等于今天（不产生任何 INSERT 语句）；注册期判定因此对旧驱动照旧加载（高于宿主版本也只 warn）。
2. 不改任何既有方法签名（`build_update_sql` / `build_delete_sql` / `format_sql_literal` / `pagination_syntax` / `execute` / `query` / `begin_transaction` / `commit` / `rollback` 全部逐字不变）。
3. 不改既有 DTO 字段名与类型：`RowChangePlan` 只**新增可选**字段、`Value` / `ColumnSchema` / `PlannedStatement` 不动、`RowCommitStatementResult.operation` 只是多一个字符串取值。前端 TS 侧的 `CommitStatementResult.operation` 由 `'update' | 'delete'` 拓宽为三元联合，属类型拓宽，运行时收到的仍是字符串。
4. **没有新增 DTO**：`RowChangePlan.inserts` 是带 `#[serde(default)]` 的新字段，`PendingRowChange.insertMarked` 是 `skip_serializing_if = "is_false"` 的新字段——两者都只增不改，旧客户端/旧计划照旧可用。（原稿此处写「全新类型无旧数据需要反序列化」，随 R-D 删类型而作废。）
5. **残留风险（不构成版本提升理由）**：钉在旧 `ref` 的 git 驱动不会覆盖 `build_insert_statement`，MySQL 协议族会遇到「一列都不写 → `DEFAULT VALUES`」的语法错误。这不是静默错数据，且 MySQL 协议族的 path 驱动与 `ReuseDriver` 内层都在本仓、同一次提交内即可覆盖。**动作**：随本册在 `drivers-registry.json` 的说明/发布说明里登记「建议刷新 git 驱动 `ref`」，**不要**为此提升协议版本（提升会强制所有插件同步发版，收益为零）。

---

## 4. 交互与状态机

### 4.1 草稿行生命周期（三要素齐全）

| 状态 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `idle` | 面板初始；提交成功并刷新完成；`Esc` 在无单元格编辑时清空全部空草稿 | 工具条「+ 行」可用（只读连接除外）；既有行照旧双击编辑 | 满足任一进入条件 → `drafting` |
| `drafting` | ① 点工具条「+ 行」；② `⌘/Ctrl+I`；③ 双击**最后一行下方空白区**；④ `⌘/Ctrl+D` 复制当前草稿行；⑤ 03 的 `Paste rows` 无区域路径回填 | 新草稿行追加到 `draftInsertOrder` 末尾并滚动到可见；每个单元格的空状态显示 `DEFAULT` 占位；编辑草稿格 → `values[col] = v`；把草稿格清空 → **删掉** `values` 里的这个键（即「不写」= 库默认，**不是**写 `NULL`）；`Set NULL` 显式动作 → `values[col] = null`；`⌘/Ctrl+D` → 复制本行 `values` 成一条新草稿；`Delete`（焦点在草稿行且无格在编辑）→ 移除该草稿；待提交计数 = `pendingChanges.size + draftInserts.size` | 点「预览/提交」→ `previewing`；在任何草稿格里输入但未提交 → 仍 `drafting`；`Esc` 且**所有**草稿行 `values` 为空 → 逐条移除空草稿并回 `idle`（有内容的草稿行**不**被 `Esc` 丢掉，必须显式删除） |
| `previewing` | 点「预览」或「提交」触发 `previewPendingChanges` | 预览对话框同时列出 `inserts` / `updates` / `deletes` 三段与总影响行数；确认文案形如「提交 2 插入、1 更新、0 删除」；指纹可见 | 取消 → `drafting`（草稿与既有改动都保留）；确认 → `committing`；预览报错 → `drafting` 并把 `classifyGridError` 结果落到 `ts.error` |
| `committing` | 预览计划指纹校验通过后调 `commitPendingChanges` | 两个集合都冻结（输入被忽略或在途改动触发签名不一致保护）；按钮 `pendingBusy` 置灰 | 成功 → 清空 `pendingChanges` 与 `draftInserts`、`loadTableData` 刷新当前页 → `highlighting` → `idle`；失败 → 保留两套暂存、`ts.error` 显示分类后文案 → `drafting` |
| `highlighting` | 提交返回且**能定位到新行**（`row_identity` 非空，或「最大主键」候补命中；见裁定 7） | 命中行加 1.5s 高亮（复用既有 `highlightedRow` 机制）；`highlightedNewRowKey` 超时清 `null` | 超时 → `idle`；用户翻页/改筛选/改排序 → 立即清空并 → `idle` |

**降级分支（定位不到新行：无主键 / 复合主键 / 类型不可比较 / 新行不在当前页 / 驱动未提供回填）**：〔契约审计修订 2026-10-21〕裁定 7——提交成功后**不进入 `highlighting`**，直接刷新当前页回 `idle`，并在提交结果提示里显示「新行主键无法自动定位，已为你刷新当前页」（`grid.insert.returningUnavailable` → 文案 key 见 §8）。**这是本批的常规路径而非异常**：宿主没有调用 `insert_returning_clause`，所以绝大多数驱动都会走到这里，文案必须中性（不是错误），且**不得**暗示新行已高亮。

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
| IME | 复用既有 `EditableCell` 的组合输入保护：composition 期间不落草稿值、不触发签名重算、不响应 `Enter`；composition 结束后一次落值。**禁止**在草稿格里新建一套 `input` 事件处理 |
| 只读 | `isEditable === false` 时「+ 行」不渲染（而不是渲染后报错）；若后端仍返回 `grid.cell.readOnly: …`，前端必须先 `classifyGridError()` 再渲染，未命中前缀则**原样显示**原文（契约 §8.1） |

### 4.3 与既有「待提交」UI 的关系

- 待提交计数文案由 `tableData.pendingChanges`（`{count} unsaved changes`）扩展为 `pendingChanges` + `draftInserts` 之和；插入条数在预览区单独显示（`tableData.pendingInserts`）。
- 既有 `confirmCommit`（`Commit {updates} updates and {deletes} deletes?`）**不改其含义**，另加 `tableData.confirmCommitWithInserts`（`Commit {inserts} inserts, {updates} updates and {deletes} deletes?`），插入为 0 时继续走旧 key，避免动到其他语言已有词条。
- 草稿行在虚拟列表里必须用 `data-testid="draft-row-N"`（PRD 明文要求）与 `data-row-kind="draft"`，供 E2E 与单测稳定定位（禁止用几何坐标反查）。

---

## 5. 实现步骤

> 每步都写「改哪个文件 / 加什么符号 / 为什么 / 怎么自测」。顺序即推荐提交顺序。

### Step 1 · 把计划/执行逻辑搬进共享层 `packages/feature-core/src/data_browsing/`（T0-2 / T0-3，**先于任何插入代码**）

> **〔契约审计修订 2026-10-21〕T0-2 / T0-3**：本节原稿把 `row_change_plan.rs`（≈620 行）与 `row_change_execute.rs`（≈350 行）落在 `src-tauri/src/commands/data/`。**落点错了**：这两块是纯业务逻辑（计划构造、指纹、事务内逐条执行、回填判定），一条都不依赖 Tauri 运行时。按 **R-A 分层规则**改落共享层；`src-tauri` 只保留 command 适配壳。原「先拆 1416 行的 data.rs / 再迁移守卫」的叙述一并作废（见本节末的 R-F 说明）。

- **目标布局**：
  - `packages/feature-core/src/data_browsing/change_set.rs`（新增）：`RowChangeTableContext` / `PendingRowChange` / `PlannedStatement` / `ChangeWarning` / `RowChangePlan` / `CommitPendingChangesRequest` / `RowCommitStatementResult` / `CommitPendingChangesResponse`（**DTO 留 host 也合法**：它们是 IPC 载荷，`feature-core` 侧只要求 `serde` 可序列化；本册按「DTO 与逻辑同址」放入 `change_set.rs`，宿主 `use` 即可，避免两处定义同一结构）+ `canonicalize_changes` / `identity_key` / `effective_identity` / `validate_table_context` / `changes_fingerprint` / `value_summary` / `build_row_change_plan` / `plan_changes` / `validate_immutable_plan`，以及本册新增的 `canonicalize_insert` / `planned_insert_statement` / `bound_insert_statement_for_plan` / `insert_values`。
  - `packages/feature-core/src/data_browsing/write_exec.rs`（新增）：`execute_row_change_plan_impl` / `commit_pending_changes_impl` + 回填判定 `insert_row_identity`。
  - `packages/feature-core/src/data_browsing/mod.rs`：`pub mod change_set; pub mod write_exec;`（+ 必要的 re-export，保持宿主调用点一行 `use`）。
  - `src-tauri/src/commands/data/mod.rs`（改造）：只留 `#[tauri::command] async fn preview_pending_changes(...)` / `commit_pending_changes(...)` 等**适配壳**——取参数 → 必要校验（`dbSessionId` 是否存在、请求是否完整）→ 转发共享层 → 把 `CommandError` / `String` 映射成 `CommandError`。**壳里不得有 `canonicalize_*`、指纹、SQL 构造、事务控制**。
- **为什么是 `feature-core` 而不是 `application` 或 `src-tauri`（R-A 原文）**：`feature-core` 已依赖 `datazen-runtime` / `driver-api` / `platform-api` / `application`，既能拿 driver 句柄又能拿 runtime 的 session/事务；`packages/application` **不依赖 runtime**，放不了执行层；`src-tauri` 只做适配是 web 与 desktop 同源的前提（否则 web 端永远没有这条写路径）。
- **driver 句柄怎么注入（不存在「必须吃 `AppState`」的例外）**：`feature-core::services::ports::DatabaseToolHost` 已经提供 `driver_for_connection(id)` 与 `execute_target(id, command, input)`（`packages/feature-core/src/services/ports.rs`，既有 `db_tools.rs` / `ai/tools.rs` 就在用）。写路径所需 driver 句柄经这个端口注入，宿主适配壳实现该 trait（转发到 `AppState` 的 session 表）即可；事务由 runtime 的 session handle 承担，**没有**把 `AppState` 塞进 `feature-core` 的必要。
- **与已落地分支的关系（必须写明，避免重复搬迁）**：`feature/insert-backend` 分支已把 INSERT 计划落在 `src-tauri/src/commands/data/insert_plan.rs`（host）。**那次搬迁与本节是同一件事**——不是第三种布局，而是把它并入共享层：`insert_plan.rs` 的 `canonicalize_insert` / `planned_insert_statement` / `bound_insert_statement_for_plan` / `insert_values` 一并迁入 `change_set.rs`，宿主只留参数解包。
- **R-F 事实校正（原稿三条全部作废）**：
  1. `validate_legacy_pk_columns` 与 `commit_row_deletes_rejects_empty_pk` **全仓不存在**，没有「迁移它们」这个动作；本册的守卫是**新建**的（W-1/W-2/W-3，落地在 `change_set.rs` 的计划构建处），不是搬迁。
  2. `commit_row_updates` / `commit_row_deletes` **仍然注册**在 `generate_handler!`（`bootstrap/run.rs`），但实现**恒拒绝**。本册不改这两个命令的注册状态，**不得**写「注销注册项 / 删除 impl / 断言 `generate_handler!` 里不再出现」——那与今天的代码相反。
  3. 与 `tableDataStore.test.ts` 的 mock / `not.toHaveBeenCalled()` 断言无关，本册不要求删它们。
- **怎么自测**：`cargo test -p datazen-feature-core --lib` 退出码 `0`，用例数与搬迁前 `cargo test -p datazen --lib` 中相关用例**逐个不变**（先记下旧用例清单再比对）；`cargo test -p datazen --lib` 仍然全绿（适配壳无行为变化）；`wc -l` 每个新文件 ≤ 800。

### Step 2 · `driver-api`：两个新 trait 方法 + 默认实现 + `ReuseDriver` 转发（**不新增类型**）

> **〔契约审计修订 2026-10-21〕T2-3 / R-D**：原 Step 2 的第一步是「在 `types.rs` 新增三态写入类型、在 `traits/sql_text.rs` 新增拼字符串的插入 builder」。**两步都删掉**：`types.rs` 零改动，`build_insert_statement` 落在 `sql_builder.rs`（与 `build_update_statement` 同址）。

- **改哪个文件 / 加什么符号**：
  - `packages/driver-api/src/traits.rs`：新增 `build_insert_statement(&self, table: &str, columns: &[(&str, Option<Value>)]) -> Result<BoundSqlStatement, DriverError>`（默认实现：`!self.supports_bound_writes()` 直接 `Unsupported`，否则委托 `sql_builder::build_insert_statement(.., self.max_bound_parameters(), ..)`）与 `insert_returning_clause(&self, columns: &[&str]) -> Option<String>`（默认 `None`）。文档注释写清「列不出现 = 默认值 / `None` = NULL / `Some("")` = 已内联」三条（§3.2）。
  - `packages/driver-api/src/sql_builder.rs`：新增 `pub fn build_insert_statement(dialect, max_parameters, table, columns) -> Result<BoundSqlStatement, DriverError>`（§3.2 函数体；复用既有私有 `bind_parameter`，空切片 → `DEFAULT VALUES`）。
  - `packages/driver-api/src/reuse.rs`：`ReuseDriver` 的 impl 里**补两行转发**到 `self.inner`。
  - `packages/driver-api/src/types.rs`：**不改**。
- **为什么**：这是契约层唯一的改动点；`ReuseDriver` 转发是必须的，否则 MySQL 协议族（doris / starrocks / oceanbase 等包装驱动）会退回默认 `DEFAULT VALUES` 而在 MySQL 上报语法错误。
- **怎么自测**：`cargo test -p datazen-driver-api --lib`；新增单测见 §9 的 `insert_*` 用例（含「预算超限在返回 SQL 之前失败」「值不进 SQL 文本」「空切片 → `DEFAULT VALUES`」）；另加一条「`ReuseDriver` 转发」用例：用一个覆盖了 `build_insert_statement` 的测试内层驱动，断言包装后**命中内层**（返回内层哨兵，而不是默认实现产出的 `DEFAULT VALUES`）。

### Step 3 · 共享层：计划侧 `inserts` + 新行规范化 + 指纹（签名不变）

> **〔契约审计修订 2026-10-21〕T2-4**：原 Step 3 的新增请求侧 DTO、第 4 个 IPC 参数、`changes_fingerprint` 加参数、`canonicalize_inserts` 全部作废；改为下面的形态（落点见 Step 1 的 `change_set.rs`）。

- **改哪个文件 / 加什么符号**（`packages/feature-core/src/data_browsing/change_set.rs`）：
  - `PendingRowChange` 加 `insert_marked: bool`（`#[serde(default, skip_serializing_if = "is_false")]`）；`RowChangePlan` 加 `inserts: Vec<PlannedStatement>`（`#[serde(default)]`，序列化始终输出）。
  - `canonicalize_changes`：先按 `insert_marked` 分区（新行 → `canonicalize_insert`，**按请求顺序**追加到末尾；其余照旧按身份归并）。
  - `canonicalize_insert(change)`：`row_identity` / `original_values` 清空、`delete_marked = false`、`changed_columns` 排序去重、逐列校验 `current_values` 里必须有值、`current_values.retain(changed_columns)`；`delete_marked == true` → `grid.commit.conflictingIntents`。
  - `build_row_change_plan`：新行分支调 `planned_insert_statement`（`driver.build_insert_statement` → `PlannedStatement`，`sql_template` 用产出的 `sql`），**按请求顺序 `push` 到 `inserts`（不排序）**；`validate_table_context` 不变。
  - `changes_fingerprint`：**签名与 payload 都不变**（插入行已在 `changes` 里，`insert_marked` 只在为 `true` 时序列化，故不含插入的旧计划指纹逐字节相同）。
  - **V-1…V-6 的校验点不变**（未知列 / NOT NULL 无默认 / 全默认值合法性 / schema 元数据缺失 / 自增列 / 无主键表允许插入），只是它们现在读 `changed_columns` + `current_values`，不再读三态枚举。
- **为什么**：让「新行」与「既有行改动」共用同一个数组、同一个指纹、同一次重建，从而**不可能**出现「指纹没覆盖插入内容」或「重建拿不回草稿」这类结构性问题（旧稿为此付出了 `InsertFingerprintEntry` 投影与 `draft_id` 排除两处补丁）。
- **怎么自测**：`cargo test -p datazen-feature-core --lib`；用见 §9.1，重点是「只有 `inserts` 时真的执行」「`insert_marked` 为 `false` 时指纹与旧实现逐字节相同」。

### Step 4 · **陷阱 1**：扩展「空计划提前返回」，并修好 `plan_changes` 的回填

> **〔契约审计修订 2026-10-21〕T2-4**：原 Step 4 的第 (2)(3) 两点建立在那个独立的草稿 DTO 之上（`plan_changes` 返回两个数组、新增 `plan_inserts`、`InsertFingerprintEntry` 排除 `draft_id`、V-7 规范化 `Value(Value::Null)`）。**全部作废**：在 `insert_marked` 方案下，插入行本来就在 `changes` 里，`plan_changes` 只需**照原样**把 `insert_marked` 还原（它已经是 `PendingRowChange` 的字段），不存在「装不下」「反推丢信息」「指纹拿不回 `draft_id`」三个坑。剩下的只有一个坑，就是空计划早退。

- **改哪个文件 / 加什么符号**（`packages/feature-core/src/data_browsing/write_exec.rs`）：
  1. `execute_row_change_plan_impl`：把首个空计划判断改成
     `if plan.inserts.is_empty() && plan.updates.is_empty() && plan.deletes.is_empty() { … }`。
  2. `plan_changes`（`change_set.rs`）：插入行按原顺序还原成 `PendingRowChange { insert_marked: true, .. }`，与 `updates` / `deletes` 的还原并列；**不需要**新函数、不需要第二返回值。
- **为什么**：漏改第 (1) 点 → `updates`/`deletes` 都为空、只有插入时函数在第一行就 `return Ok(affected_rows: 0)`：**不报错、不开事务、不执行任何语句**，用户在界面上看到「提交成功」，数据库里什么都没有（契约 §1.4 事实 3）。这是本册最隐蔽的坑：它把「数据丢失」伪装成「功能正常」。
  - **指纹为什么不再有第二个坑**：`changes` 数组里已经带了 `insert_marked` 与全部写入列值，序列化即覆盖；`insert_marked == false` 时该字段**不出现**，所以旧计划指纹逐字节不变（§3.4.1）。重建只需把同一份 `changes` 喂回去。
- **怎么自测**（`cargo test -p datazen-feature-core --lib`）：
  - 新增回归测试 `commit_executes_insert_only_plan_instead_of_returning_empty_success`：`options.columns` 配好列、`options.execute_rows_affected = 1`，只传一条 `insert_marked: true` 的 change（`updates`/`deletes` 为空）→ 断言 `response.affected_rows == 1`、`response.statements.len() == 1`、`response.statements[0].operation == "insert"`、`mock.execute_calls() == 1`、`mock.commit_calls() == 1`。
  - 新增 `commit_rebuilds_plan_with_inserts_without_fingerprint_mismatch`：预览拿到 plan 后立刻提交 → 必须 `Ok`。
  - 手动反向验证（一次性）：临时删掉 `plan.inserts.is_empty() &&`，确认回归测试**变红**——测试必须真的能抓住这个坑。
  - 新增 `fingerprint_covers_insert_values_and_stays_stable_without_inserts`（**纯函数级，不需要驱动**）：同一组带 `insert_marked` 的 `changes`，改一个写入列值 → 指纹变化；把 `insert_marked` 全设为 `false` 后与旧实现比对 → **逐字节相同**；把两条内容不同的草稿对调 → 指纹变化。**注意**：不再有「只改 `draft_id` → 指纹不变」这条用例——`draft_id` 不上线，无从「只改」。

### Step 5 · **陷阱 2**：把插入放进**同一个**事务执行块

- **改哪个文件 / 加什么符号**：`packages/feature-core/src/data_browsing/write_exec.rs::execute_row_change_plan_impl`（〔契约审计修订 2026-10-21〕T0-2：落共享层，宿主只留命令壳）。
- **具体做法（顺序即语义）**：
  1. `let mut statements = Vec::with_capacity(plan.inserts.len() + plan.updates.len() + plan.deletes.len());`
  2. 在**既有的** `let result: Result<…> = async { … }.await;` 块内部，**最前面**加 `for planned in &plan.inserts { … }`，然后才是既有的 `updates` 循环、`deletes` 循环。顺序固定 **inserts → updates → deletes**（PRD 明文：先建父行再改子行，避免同批内瞬时外键违约）。
  3. 事务的 begin/commit/rollback **一行都不改**：`manual_handle == Some(h)` 时照旧不自行 begin/commit（插入并入该手动事务），`None` 时照旧自行开一次会话级事务并 commit/rollback。〔契约审计修订 2026-10-21〕裁定 1：旧稿依赖的那个「会话是否已开事务」的布尔变量、以及 `driver.begin_transaction` / 裸 `BEGIN` 的退化路径，都不存在。
  4. 插入语句的**执行通道**本批**恒为** `driver.execute`（无回填；〔契约审计修订 2026-10-21〕裁定 7：宿主不调用 `insert_returning_clause`，所以 `driver.query` 通道本批不启用）。它与 UPDATE/DELETE 在**同一个** `async` 块里，因此共享同一事务与同一 `handle`。
- **为什么**：函数有**两条**事务归属路径（`manual_handle: Some(h)` → 并入该句柄的手动事务、不自己 begin/commit；`None` → 自行开会话级事务，结束时 commit/rollback）。插入如果写在 `async` 块之外（例如在 `begin_transaction` 之前跑，或在 `commit` 之后补跑），就会落在**另一个**事务里，破坏「一次提交要么全成、要么全不成」；而且落在 commit 之后的插入**无法回滚**，是真正的脏数据。
- **怎么自测**：
  - `insert_and_update_share_one_transaction_and_one_commit`：`options.execute_rows_affected = 1`，一批含 1 插入 + 1 更新 → `mock.execute_calls() == 2`（真实执行两次，而不是 1 次）、`mock.commit_calls() == 1`、`open_transaction_count()` 归零、`affected_rows == 2`。
  - `insert_joins_manual_handle_transaction_instead_of_committing`：构造一个 `TransactionState::Active` 的 `SessionHandle`（测试里用 `TestAppState` 的手动事务夹具开一个），把它作为 `manual_handle: Some(h)` 传入 → 断言提交后 `mock.commit_calls()` **不增长**、`open_transaction_count()` 仍为 1，但插入语句**已执行**（`execute_calls()` 增长）。〔契约审计修订 2026-10-21〕裁定 1：搭法从「往宿主那张会话事务表里塞一条登记」改为「传一个手动句柄」——那张表已不存在，测试里也不得引用它。
  - 事务原子性用例（§9 `insert_rolls_back_whole_batch_on_failure`）：让更新语句的 `affected != 1`（把 `execute_rows_affected` 设成 2 或在第二次调用上制造错误）→ 断言整批 `Err`、且**回滚被调用**（`rollback_calls` 增长，若 mock 未暴露该访问器则断言 `open_transaction_count()` 归零 + 无 `commit_calls` 增长）。
  - 手动句柄路径失败时**不**回滚用户事务（保持既有语义）：断言 `rollback_calls` 不增长。

### Step 6 · 主键回填与降级（两条路径的用户可见差异）

> **〔契约审计修订 2026-10-21〕裁定 7：本批不实现回填，本节降级为「预留缝隙的设计说明 + 本批的定位策略」**
>
> - **代码现状**：`insert_returning_clause` 已有契约默认实现（`None`）与 `ReuseDriver` / `MockDriver` 的转发，但**宿主一行都没调用**，所以今天插入结果的 `row_identity` **恒为空 map**，也没有任何 RETURNING / `OUTPUT INSERTED` 回填。
> - **本批的定位策略（唯一要实现的）**：插入成功后 ① 若结果里的 `row_identity` 非空 → 用它定位并高亮；② 否则尝试「按最大主键定位」——**仅当**表有单列主键且该列类型可比较时，刷新后取当前页里主键最大的那一行作为候补；③ 仍定位不到（无主键 / 复合主键 / 类型不可比较 / 新行不在当前页）→ **不定位**，附 `grid.insert.returningUnavailable` warning，只刷新不高亮。**任何一步都不许猜主键值**（例如「当前最大主键 + 1」）。
> - **下一节那张条件表保留为「回填能力落地时的设计」**，不是本批验收项：表里 `Some(clause)` / `Some("")` 两行属于后续改造；本批实际走的只有「`None` + 用户写满主键」与「`None` + 没写满」这两条。
> - **因此 §9 里 `insert_uses_query_path_when_returning_clause_is_present` / `insert_identity_falls_back_to_written_primary_key_when_returning_is_none` / `insert_returning_clause_*` 这些驱动级用例，本批只保留在 `driver-api` 的默认实现与转发层面**（证明缝隙存在、不影响任何驱动），**宿主侧的回填用例本批不写**。

- **改哪个文件 / 加什么符号**〔裁定 7：本批只实现「定位」，`insert_row_identity` 回填函数随回填能力落地再建〕：`packages/feature-core/src/data_browsing/write_exec.rs` 新增私有函数 `insert_row_identity(driver, handle, sql, pk_columns) -> Result<Option<RowIdentity>, CommandError>`（或等价的内联逻辑）；`packages/feature-core/src/data_browsing/change_set.rs::build_row_change_plan` 在生成插入 `sql_template` 时调用 `driver.insert_returning_clause(&pk_column_names)`。（〔契约审计修订 2026-10-21〕T0-2/T0-3：路径改落 feature-core。）
- **符号与规则**：

| 条件 | 宿主行为 | 用户可见差异 |
| --- | --- | --- |
| 表有主键（`CachedColumns::primary_keys` 非空，或由 `ColumnSchema::is_primary_key` 回退得到）且 `insert_returning_clause(&pk_names) == Some(clause)` 且 `clause` 非空 | `sql = format!("{} {}", sql_template, clause)`；用 `driver.query(handle, &sql)` 取 `QueryResult`，按 `QueryResult.columns[].name` 把第一行的主键列值映射成 `RowIdentity`；非 NULL 且齐全才接受，否则 `CommandError::Validation` 前缀 `grid.insert.returningUnavailable: …`；`RowCommitStatementResult.row_identity` = 回填身份 | 提交后**高亮新行 1.5s**（PRD 第 5 条） |
| `insert_returning_clause(...) == Some("")` | **不追加任何字符**；`driver.query(handle, &sql_template)` 读返回行（SQL Server 的 `OUTPUT INSERTED.*` 已由驱动的 `build_insert_statement` 内联） | 同上一行 |
| `insert_returning_clause(...) == None`，**但**该草稿把全部主键列都显式写了值 | 用用户写入的 PK 值构造 `row_identity`（不再向数据库索取） | 仍高亮新行；**不**提示降级 |
| `insert_returning_clause(...) == None`，且草稿没写满主键（**本批的常规路径**，见上方裁定 7） | 不猜测主键值；`row_identity` 留空 map；刷新当前页后尝试「最大主键候补」（单列可比较主键才试）→ 命中则高亮，未命中则附 `ChangeWarning { code: "insert-remaining-unavailable", severity: "info" }` | 命中：高亮新行；未命中：**不定位 + 刷新**，提示「新行主键无法自动定位，已为你刷新当前页」（`tableData.insertReturningUnavailable`）；**不承诺**高亮 |
| 表**无主键** | 不调用回填（`pk_names` 为空）；同「无法回填」路径 | 插入成功、刷新当前页、不高亮；`+ 行` 可用而既有行编辑/删除仍禁用 |

- **为什么**：契约 §5.1 明文「返回 `Some` 时宿主追加子句并读取返回行以回填主键；返回 `None` 时宿主**不得**猜测主键，而是走『提交后重新拉取当前页』的降级路径，并在 UI 上不承诺『新行会保持高亮』」。回填通道必须用 `query` 而非 `execute`（`execute` 只回 `u64`），且**必须**在同一事务块内。
- **怎么自测**：驱动用例（§9）断言 postgres/sqlite 的 `insert_returning_clause(&["id"])` 返回 `Some("RETURNING \"id\"")` 且 `build_insert_statement` 产出的 `sql` 里**没有** `RETURNING`（保证「追加」语义）；SQL Server 断言 `build_insert_statement` 产出的 `sql` 内含 `OUTPUT INSERTED.` 且 `insert_returning_clause` 返回 `Some("")`；MySQL 断言 `insert_returning_clause` 返回 `None`。宿主侧用 `MockDriverOptions.query_rows` 造返回行，断言回填后的 `row_identity`。

### Step 7 · 前端 store：草稿状态、动作、计数聚合、预览/提交接线

> **〔契约审计修订 2026-10-21〕T2-4 / R-D**：原 Step 7 的三态写入类型 + 三个构造器、值→三态转换函数、以三态类型为形参的草稿写动作、`PreviewPendingChangesRequest.inserts` 全部作废；草稿值用 `Record<string, Value | null>`（键存在 = 写，`null` = NULL），上线时转成 `insertMarked`。

- **改哪个文件 / 加什么符号**：
  - `src/lib/tableChanges.ts`：`PendingInsert`（`values: Record<string, Value | null>`）、`PendingRowChange.insertMarked?: boolean`、`RowChangePlan.inserts`、`CommitStatementResult.operation` 拓宽；新增纯函数 `draftWriteColumns(values: Record<string, Value | null>): string[]`（排序后的写列，即 `changed_columns`）、`draftInsertCount(ts)`。
  - `src/stores/tableData/types.ts`：`TableState` 加 `draftInserts`、`draftInsertOrder`、`highlightedNewRowKey`（§3.7）。
  - `src/stores/tableData/pendingChanges.ts`：新增 `pendingWorkSignature({ pendingChanges, draftInserts, draftInsertOrder })`（在既有 `pendingChangesSignature` 的基础上扩一个入参，不改既有函数签名）、`hasPendingWork(ts)`、`draftsForWire(ts): PendingRowChange[]`——按 `draftInsertOrder` 展开成 `{ rowIdentity: {}, originalValues: {}, currentValues: draft.values, changedColumns: draftWriteColumns(draft.values), deleteMarked: false, insertMarked: true }`。
  - `src/stores/tableDataStore.ts`：`TableDataStore` 接口新增 `addDraftRow(panelId)`、`setDraftValue(panelId, draftId, column, value: Value | null | undefined)`（`undefined` = 删掉该键 = 不写）、`removeDraftRow(panelId, draftId)`、`duplicateDraftRow(panelId, draftId)`；`previewPendingChanges` / `commitPendingChanges` 的早退判据与签名改用 `hasPendingWork` / `pendingWorkSignature`；请求体改为 `changes: [...既有改动, ...draftsForWire(ts)]`（**不加新字段**）；`commitPendingChanges` 成功分支同时清空 `draftInserts` / `draftInsertOrder` 并设置 `highlightedNewRowKey`。
  - `src/commands/database.ts`：**不改请求接口**（`PreviewPendingChangesRequest` 仍只有 `context` / `changes`）；只是调用方把草稿并进 `changes`。
- **为什么**：R-2/R-3 不做，就会出现「只有草稿行时按钮永远置灰」或「预览后改草稿仍能提交过期计划」两类回归；`draftsForWire` 必须按 `draftInsertOrder`（不能按 `Map` 迭代序的隐式约定），因为**插入顺序影响自增主键分配**，后端也刻意不排序。
- **怎么自测**：`npx vitest run src/stores/__tests__/tableDataStore.test.ts src/lib/__tests__/tableChanges.test.ts`；新增用例见 §9，重点是「仅草稿时请求 `changes` 非空且带 `insertMarked: true`」「草稿值清空后该列从 `changedColumns` 消失」。

### Step 8 · 前端 UI：入口、草稿行渲染、预览与确认文案

- **改哪个文件 / 加什么符号**：
  - `src/components/DataTable/VirtualBody.tsx`：`VirtualBodyProps` 加 `draftRows?: DraftRowViewModel[]`（`{ draftId, cells: Record<string, Value | null> }`，**键不存在 = 不写**）与 `onDraftCellEdit` / `onDraftRowRemove`；在真实行之后渲染草稿行，节点带 `data-testid="draft-row-N"`、`data-row-kind="draft"`，键不存在的格显示 `DEFAULT` 占位（`tableData.insertDefault`）。〔契约审计修订 2026-10-21〕T2-4/R-D：`cells` 不再用三态枚举。
  - `src/windows/connection/TableView.tsx`：工具条「+ 行」按钮（`tableData.insertRow`，`title` 带 `⌘/Ctrl+I`）、`⌘/Ctrl+I` 与 `⌘/Ctrl+D` 快捷键（与 02 分册的键盘表对齐，避免重复注册）、最后一行下方空白区双击处理、`Delete` 移除草稿；三处 `pendingChanges.size === 0` 门闸改用 `hasPendingWork(ts)`；预览对话框增加 `inserts` 段落（复用既有 `tableData.previewTitle` / `previewSql` / `previewParameters` / `previewWarnings`）；确认文案在 `inserts.length > 0` 时改走 `tableData.confirmCommitWithInserts`。
  - `src/lib/gridErrors.ts`（**01 已建骨架**，本册按契约 §8.1 三步只补码表，**禁止整文件重写**）：见 §8。
- **为什么**：草稿行不能写进 `ts.rows`（§3.7 R-5），所以必须由 `VirtualBody` 的**派生渲染**承接；「+ 行」在只读连接下**不渲染**（契约 R4：能力缺失时隐藏入口，而不是渲染一个点了报错的按钮）。
- **怎么自测**：`pnpm typecheck` + `npx vitest run`（组件级用例见 §9）+ `pnpm e2e:minimal`（`e2e/specs/table-insert.ts`）。

### Step 9 · 驱动覆盖与驱动专属测试（postgres / mysql / sqlite / sqlserver）

- **改哪个文件 / 加什么符号**：
  - `packages/drivers/mysql/src/mysql.rs`：覆盖 `build_insert_statement`——`columns` 为空（一列都不写）时产出 `INSERT INTO `t` () VALUES ()`（MySQL 不支持 `DEFAULT VALUES`），其余情形委托默认实现（`dialect.quote_ident` 走自己的 `quote_char`）。〔契约审计修订 2026-10-21〕T2-3：方法名与入参形态按冻结签名校正。
  - `packages/drivers/postgres/src/postgres.rs`：覆盖 `insert_returning_clause` → `Some(format!("RETURNING {}", cols.map(quote_ident).join(", ")))`。
  - `packages/drivers/sqlite/src/sqlite.rs`：同上（SQLite ≥ 3.35 支持 `RETURNING`；低版本策略见 §11 Q-3）。
  - `packages/drivers/sqlserver/src/sqlserver.rs`：覆盖 `build_insert_statement` 内联 `OUTPUT INSERTED.[col]`（仍返回 `BoundSqlStatement`，值照样走 `parameters`），`insert_returning_clause` 返回 `Some(String::new())`。
  - 各驱动 `src/tests.rs`（或该 crate 的既有测试文件）新增三态断言：**不写的列不出现**、`None` 绑定 `NULL`、`Some(v)` 绑定值。
- **为什么**：零硬编码——方言差异只能经 `DatabaseDriver` 表达，宿主绝不按库名分支；而这些差异（`DEFAULT VALUES` 的可用性、回填子句的位置与语法）正是驱动该拥有的知识。
- **怎么自测**：`cargo test -p datazen-driver-postgres --lib`、`-p datazen-driver-mysql --lib`、`-p datazen-driver-sqlite --lib`、`-p datazen-driver-sqlserver --lib` 全部退出码 `0`；每个 crate 至少断言「不写（列不出现）」与「`NULL`」与「空串」三者在产出的 `sql` / `parameters` 上互不相同。

---

## 6. 文件级改动清单

> **〔契约审计修订 2026-10-21〕T0-2 / T0-3 / T2-3 / T2-4 / N5 / R-G**：本表已按「计划/执行落共享层、驱动侧无新增类型」重写；既有文件的实测行数一律标 `wc -l` 日期，`预估` 列只表示**本册预计新增/变动的行数**（不是任何文件的现况）。

| 文件 | 新增/修改 | 职责 | 预估新增/变动 | 触及 800 行上限？ |
| --- | --- | --- | --- | --- |
| `packages/feature-core/src/data_browsing/change_set.rs` | 新增 | DTO（`PendingRowChange.insert_marked`、`RowChangePlan.inserts` 等）、`canonicalize_changes`、`canonicalize_insert`、指纹、计划构建、`plan_changes`、本册 V-1…V-6 校验 | ≈ 620（迁移 ≈ 440 + 插入 ≈ 180） | 否 |
| `packages/feature-core/src/data_browsing/write_exec.rs` | 新增 | `execute_row_change_plan_impl`、`commit_pending_changes_impl`、**新行定位**（`row_identity` 优先 → 最大主键候补）；回填判定 `insert_row_identity` 属**后续批次**（裁定 7） | ≈ 320（迁移 ≈ 240 + 插入 ≈ 60 + 定位 ≈ 20；原估 350 含未实现回填的 ≈ 30） | 否 |
| `packages/feature-core/src/data_browsing/mod.rs` | 新增 | `pub mod change_set; pub mod write_exec;` + re-export | ≈ 10 | 否 |
| `src-tauri/src/commands/data/mod.rs` | 改造 | **只留 `#[tauri::command]` 适配壳**（取值 → 必要校验 → 转发 → 映射 `CommandError`）；遗留入口 `commit_row_updates*` / `commit_row_deletes*` **仍注册且恒拒绝**（本册不改其注册状态） | −≈ 560（逻辑迁出；〔合并后校正〕现 **640** 行 → ≈ 80 + 既有子模块） | 否（本册不再新增子模块） |
| `src-tauri/src/commands/data/tests.rs` | 修改 | 只留**命令壳级**用例（IPC 形状与错误映射）；计划/执行用例随逻辑迁到 `feature-core` 的 `#[cfg(test)] mod tests` | −≈ 460 | 否 |
| `packages/driver-api/src/sql_builder.rs` | 修改 | `pub fn build_insert_statement(..)`（复用私有 `bind_parameter`） | +70 | 否（现 161 行级） |
| `packages/driver-api/src/traits.rs` | 修改 | 两个新 trait 方法 + 文档注释 | +45 | 否（实测 **1160** 行，`wc -l` 2026-10-21 → **已超限**，属既有技术债，本册只做加法） |
| `packages/driver-api/src/reuse.rs` | 修改 | `ReuseDriver` 转发两个方法 | +12 | 否（实测 **1344** 行，`wc -l` 2026-10-21 → **已超限**，同上） |
| `packages/driver-api/src/types.rs` | **不改** | —（三态类型不落地，R-D） | 0 | 否（实测 **963** 行，`wc -l` 2026-10-21；已超限属既有技术债） |
| `packages/drivers/{mysql,postgres,sqlite,sqlserver}/src/*.rs` | 修改 | 方言覆盖（`build_insert_statement` / `insert_returning_clause`） | 每驱动 +15~35 | 否 |
| `packages/drivers/*/src/tests.rs`（4 个） | 修改 | 三态断言（不写 / NULL / 值） | 每驱动 +40 | 否 |
| `src/lib/tableChanges.ts` | 修改 | `PendingInsert`、`PendingRowChange.insertMarked`、`RowChangePlan.inserts`、`CommitStatementResult.operation` 拓宽、`draftWriteColumns` / `draftInsertCount` | +70 | 否（现 228 行，`wc -l` 2026-10-21） |
| `src/lib/gridErrors.ts` | **修改（追加）** | **01 已建骨架**（契约 §8.1 归属总则）；本册只追加 `grid.insert.*` 码，**禁止整文件覆盖** | +15 | 否 |
| `src/stores/tableData/types.ts` | 修改 | `TableState` 三个新字段 | +12 | 否 |
| `src/stores/tableData/pendingChanges.ts` | 修改 | `pendingWorkSignature`、`hasPendingWork`、`draftsForWire` | +50 | 否（现 127 行级） |
| `src/stores/tableDataStore.ts` | 修改 | 草稿动作薄接线 + 早退判据 + 请求体合并 | +30（草稿主体按下方拆分方案落到 `draftInserts.ts`） | **否**（〔合并后校正〕现 **729** 行 → 加后 ≈ 760，**仍须**按拆分方案执行） |
| `src/stores/tableData/draftInserts.ts` | 新增 | 草稿行的纯函数：增删改、复制、`draftsForWire`、`pendingWorkSignature` 的草稿部分 | ≈ 160 | 否 |
| `src/commands/database.ts` | **不改** | —（`changes` 里带草稿，请求形状不变，T2-4） | 0 | 否 |
| `src/components/DataTable/VirtualBody.tsx` | 修改 | 草稿行渲染（`cells: Record<string, Value \| null>`） | +120 | 否（现 ≈ 250 行级） |
| `src/windows/connection/TableView.tsx` | 修改 | 入口、快捷键、门闸、预览与确认 | +160 | 否（现 ≈ 400 行级） |
| `src/locales/en/query.ts` | 修改 | 新增词条（§8） | +25 | 否 |
| `src/lib/__tests__/tableChanges.test.ts` / `src/stores/__tests__/tableDataStore.test.ts` | 修改 | 单测 | +220 | 否 |
| `e2e/specs/table-insert.ts` | 新增 | E2E | ≈ 180 | 否 |

### 拆分方案（`tableDataStore.ts` 逼近红线；Rust 侧不再是本册的拆分对象）

1. **Rust（Step 1）**：计划/执行主体**搬出 `src-tauri`**，落 `packages/feature-core/src/data_browsing/{change_set.rs, write_exec.rs}`。`src-tauri` 侧只剩命令壳，规模问题随之消失；这也是 web 端拿到同一条写路径的前提（R-A）。
2. **Rust 既有超限（登记，不在本册强改）**：`packages/driver-api/src/types.rs` **963**、`traits.rs` **1160**、`reuse.rs` **1344**（均 `wc -l` 实测，2026-10-21；〔契约审计修订 2026-10-21〕N5：旧稿写 958 / 1062 / 1159）**今天已超 800**。本册只做加法，不做无关重构；建议在 `docs/development/` 的规模纪律条目里登记为技术债（拆法示例：`types.rs` → `types/value.rs` + `types/schema.rs`；`traits.rs` → `traits/` 目录下按域拆并保留 re-export）。
3. **前端（强制）**：`src/stores/tableDataStore.ts` 现 **729** 行，草稿行的增删改/复制/上线转换/签名如果全写进 store 必然越线 → 放进新文件 `src/stores/tableData/draftInserts.ts`（纯函数），`tableDataStore.ts` 只保留薄接线（每个动作 3~6 行），加后 ≈ 760 行，仍在 800 以内。

---

## 7. 边界与异常清单

| # | 场景 | 期望行为 | 相关符号 / 前缀 |
| --- | --- | --- | --- |
| 1 | **空表** | 可以新增行；`VirtualBody` 在没有真实行时也能渲染草稿行；提交后刷新仍显示新行 | `VirtualBody.draftRows` |
| 2 | **一列都不写（草稿行一个格都没填）** | 合法，产出 `INSERT INTO t DEFAULT VALUES`（或驱动覆盖的等价形式，如 MySQL 的 `() VALUES ()`），条件是没有「NOT NULL 且无默认值且非自增」的列；否则预览期拒绝 | `build_insert_statement` 默认实现、`grid.insert.noWritableColumn` |
| 3 | **仅自增主键、其余列都有默认值** | 草稿行全留空即可提交；语句里**不出现**自增列；提交后新行可见且主键由数据库分配 | V-3 排除 `is_auto_increment` |
| 4 | **复合主键** | 插入不需要身份；回填子句一次带上全部主键列（`RETURNING "tenant_id", "id"`）；用户写满全部主键列时即使无回填也能高亮 | `insert_returning_clause(&["tenant_id","id"])` |
| 5 | **无主键表** | **允许插入**（结论）。理由：INSERT 不需要行身份，`RowChangePlan.inserts` 的语句不含 `WHERE`，`affected` 校验也不依赖身份；代价是无法回填、不高亮。同表的既有行编辑/删除仍禁用（`tableData.noPrimaryKey`），UI 必须明确区分「+ 行可用、双击不可编辑」 | PRD DB-04 第 6 条 |
| 6 | **NOT NULL 且无默认值的列被留空** | 预览期拒绝：`grid.insert.noWritableColumn: column "email" requires a value` | V-3 |
| 7 | **唯一约束冲突** | 由数据库抛错 → 驱动 `Err` → `cmd_err("commit_pending_changes")` → 整批回滚；错误文本含列名（既有脱敏管线）；**不得**被 `affected` 校验误吞 | `CommandError` 序列化为脱敏字符串 |
| 8 | **批量部分失败** | 「一次提交要么全成、要么全不成」：`manual_handle == None` 时全部成功后 `COMMIT`，任一条失败 `ROLLBACK` 后返回错误；`manual_handle == Some(h)` 时不回滚那个手动事务，只把错误上抛 | `execute_row_change_plan_impl` |
| 9 | **调用方传入手动句柄（用户事务已开启）** | 校验目标库与 `TransactionState::Active` 通过 → 不自行 `beginTransaction`、不 `commitTransaction`；插入与其他改动并入该事务，由用户决定提交/回滚；`statements` 照常返回 | 陷阱 2 / Step 5 |
| 10 | **只读连接** | 预览前就被拒（`Connection is read-only; row changes are not allowed`，**无 `grid.` 前缀**）；前端不渲染「+ 行」 | `commit_pending_changes_impl` |
| 11 | **表结构未知（列清单为空）** | 不做 V-2/V-3，附 `insert-schema-unavailable` warning，把约束判定交给数据库；**不得**因此拒绝插入，也**不得**因此把「不写」的列补成 `NULL` | V-4 |
| 12 | **`affected == 0`（驱动不上报计数，如 ClickHouse）** | **不豁免**：INSERT 与其他语句一样，`affected != 1` 即整批拒绝（文案 `INSERT affected {n} rows; refusing ambiguous write`）。〔契约审计修订 2026-10-21〕R-C：`0` 行是否可按「幂等成功」放行，作为**独立分支**讨论（需先有可判定的幂等判据：明确的唯一键 + 驱动确实上报计数），不得在本册静默放宽护栏 | `execute_row_change_plan_impl`、`grid.commit.affectedMismatch` |
| 13 | **定位不到新行**（含回填返回空行 / 主键为 NULL 的将来形态） | **不回滚**：整批已提交成功，只是无法定位新行 → 刷新当前页 + `grid.insert.returningUnavailable` warning（`severity: "info"`，中性文案）〔契约审计修订 2026-10-21〕裁定 7：旧稿写「整批回滚」，与「插入已成功」矛盾 | Step 6 |
| 14 | **草稿数量很大（如 200 条）** | 预览对话框仍一次展示（虚拟滚动），提交仍是 200 条单行语句；超过前端阈值时提示「将执行 200 条语句」但**不**静默截断（拒绝静默丢失） | `tableData.confirmCommitWithInserts` |
| 15 | **多字节字符 / 超长值 / 引号与反斜杠** | **值不进 SQL 文本**：`build_insert_statement` 只产出占位符 + `BoundSqlStatement.parameters`，值原样绑定，因此引号/反斜杠/换行既不需要转义也不会被改写（`format_sql_literal` 仍在基线里服务预览类场景，本册写路径不使用它） | `BoundSqlStatement` |
| 16 | **草稿行在预览后被删除/修改** | `pendingWorkSignature` 不一致 → 预览计划作废、要求重新预览（现有纪律，不得降级为「按新数据重算」） | R-3 |
| 17 | **翻页 / 改筛选 / 改排序 / 关闭面板** | `highlightedNewRowKey` 立即清空；面板关闭丢弃草稿（与既有 `pendingChanges` 的丢弃语义一致）；有暂存工作时 `invalidateCachedData` 不触碰该面板（判据改用 R-2） | `invalidateCachedData`、R-4 |
| 18 | **自增列被显式写 `NULL`** | 允许提交并交数据库报错，附 warning `insert-null-on-auto-increment`；宿主**不猜**、不静默改写成「不写」（把该列从 `changed_columns` 里摘掉） | V-5 |
| 19 | **复合主键只提供了部分列** | **W-1/W-3**：在**生成 SQL 之前**就拒绝（`CommandError::Validation`），不是发出去以后再看影响行数。既有 `identity_key` 只要求 map 非空且值非 NULL，**不校验键集相等**——这条由本册**新建**在计划构建处（`change_set.rs`，见 Step 1） | `identity_key` + 新增列集相等断言 |
| 20 | **两个客户端同时改同一行的不同列** | **W-5**：不检测、不阻断，last-writer-wins。`WHERE` **只带主键谓词**，**禁止**把被编辑列的旧值塞进 `WHERE`（那会让「我改这一列」反过来变成「我只能改这一列」）。文档与 UI 文案**不得**出现「只影响这一行」「不会覆盖别人的修改」 | `build_update_statement` |
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
| `tableData.insertDefault` | `DEFAULT` | 草稿格里「不写」的占位符（该列不在 `changed_columns` 里） |
| `tableData.insertSetNull` | `Set NULL` | 右键菜单项（与 05 共用，本册先落 key） |
| `tableData.pendingInserts` | `{count} inserts` | 待提交计数分段 |
| `tableData.confirmCommitWithInserts` | `Commit {inserts} inserts, {updates} updates and {deletes} deletes?` | 确认文案（插入为 0 时继续用既有 `tableData.confirmCommit`） |
| `tableData.insertFailed` | `Failed to insert rows` | 提交失败兜底 |
| `tableData.insertNoWritableColumn` | `Fill at least one column: {columns} requires a value.` | 对应 `grid.insert.noWritableColumn` |
| `tableData.insertUnknownColumn` | `Column {column} is not part of this table.` | 对应 `grid.insert.unknownColumn` |
| ~~`tableData.insertDuplicateDraft`~~ | — | 〔契约审计修订 2026-10-21〕T2-4：**删除**。草稿的 `draftId` 不上线，宿主无从判定「同一条草稿出现两次」；两条内容相同的草稿是**两次合法的插入**，不再有重复草稿校验 |
| `tableData.insertReturningUnavailable` | `The new row's primary key could not be located automatically; the page was refreshed.` | 对应 `grid.insert.returningUnavailable` 与降级 warning |
| ~~`tableData.insertCountUnavailable`~~ | — | 〔契约审计修订 2026-10-21〕R-C：**删除**。INSERT 不豁免 `affected == 1`，「计数不可用」这条容忍分支不存在 |
| `tableData.insertSchemaUnavailable` | `Column metadata is unavailable; the database will validate this row.` | 对应 `insert-schema-unavailable` warning |
| `tableData.highlightNewRow` | `New row` | 高亮行提示 |

**后端 `CommandError` 文案前缀（英文，不走 i18n，前端按前缀分类后再渲染本地化文案）**：

| 前缀 | 触发点 | 前端映射 |
| --- | --- | --- |
| `grid.insert.noWritableColumn: ` | V-3 | `tableData.insertNoWritableColumn`〔契约审计修订 2026-10-21〕**前端登记推迟到 V-3 实装**：`src/lib/gridErrors.ts` 的登记规则是「只登记**有生产者**的码」（登记了却无生产者 = 前端分支到一个永不到达的前缀）。本码目前**零生产者**（V-3 需要 `ColumnSchema` 的 generated/identity 标记），故本批**不追加**到前端注册表；V-3 实装时同步补登记与 §9.4 的分类用例。**i18n 词条 `tableData.insertNoWritableColumn` 照常落地**（未使用的词条无害，届时即用上） |
| `grid.insert.unknownColumn: ` | V-2 | `tableData.insertUnknownColumn` |
| ~~`grid.insert.duplicateDraft: `~~ | — | 〔契约审计修订 2026-10-21〕T2-4：**删除**（同 `tableData.insertDuplicateDraft` 的理由） |
| `grid.insert.returningUnavailable: ` | Step 6「定位不到新行」（**本批常规路径**，非异常；裁定 7） | `tableData.insertReturningUnavailable` |
| `grid.commit.affectedMismatch: ` | `affected != 1`，**三类语句统一、INSERT 不豁免**（R-C） | 既有 `tableData.commitFailed` + 明细 |
| `grid.commit.stalePlan: ` | 指纹/重建不一致 | 提示重新预览 |
| `grid.commit.conflictingIntents: ` | 同一行既删除又有列写入 | 指出冲突行 |
| `grid.cell.readOnly: ` | 只读/生成列 | 说明具体原因 |

**前端分类器**：`src/lib/gridErrors.ts`（**01 已创建骨架**；本册按契约 §8.1 三步**只追加**上述 `grid.insert.*` 码值，**不新建第二份骨架、也不整文件重写**）：

```ts
export type GridErrorCode =
  | 'grid.insert.noWritableColumn'
  | 'grid.insert.unknownColumn'
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

> **〔契约审计修订 2026-10-21〕T2-3 / T2-4 / R-C / R-D**：9.1 的「指纹可重建」与「`Value(Value::Null)` 规范化」两条用例随独立的草稿 DTO / 三态枚举一起删除；9.2 改为参数化 seam 的断言（值在 `parameters` 里、不在 `sql` 里）；9.3 的方法名与「不写」措辞同步；新增 R-C 的 `affected == 0` 拒绝用例。

### 9.1 Rust 单元（共享层：`packages/feature-core/src/data_browsing/{change_set,write_exec}.rs` 的 `#[cfg(test)]`；命令壳级用例仍在 `src-tauri/src/commands/data/tests.rs`）

| 用例名 | 断言要点 |
| --- | --- |
| `commit_executes_insert_only_plan_instead_of_returning_empty_success` | **陷阱 1 回归**：`changes` 只有一条 `insert_marked` 时 `affected_rows == 1`、`statements.len() == 1`、`operation == "insert"`、`mock.execute_calls() == 1`、`mock.commit_calls() == 1`（把提前返回的判断写错时此用例必须变红） |
| `commit_accepts_insert_with_explicit_null_value_and_two_drafts` | 一条草稿把某列写成 `Some(Value::Null)`、一条写成 `None`，两条都提交 → 都绑定 `Value::Null`、都被 `changed_columns` 覆盖 → 预览后立刻提交 `Ok` |
| `commit_rebuilds_plan_with_inserts_without_fingerprint_mismatch` | 预览拿到 plan 后立刻提交 → 必须 `Ok`；`plan_changes` 把 `insert_marked` 弄丢时会拿到 `Row change plan fingerprint is stale or was modified` |
| `insert_canonicalization_sorts_columns_and_keeps_order_of_rows` | 同一行内 `changed_columns` 排序去重；**多行之间不排序**（顺序 = 请求顺序） |
| `insert_canonicalization_retains_only_written_columns` | 用户填过又清空的格（键已删）不会留在 `current_values` 里；`current_values` 键集 == `changed_columns` |
| `insert_canonicalization_rejects_delete_and_insert_together` | `insert_marked && delete_marked` → `grid.commit.conflictingIntents` |
| `canonicalize_insert_rejects_missing_not_null_without_default` | V-3 → `grid.insert.noWritableColumn`；错误文本含列名 |
| `canonicalize_insert_rejects_unknown_column` | V-2 → `grid.insert.unknownColumn` |
| `canonicalize_insert_allows_no_written_column_when_defaults_exist` | V-6：一列都不写且每个 NOT NULL 列都有 `default_value` 或 `is_auto_increment` → 计划里出现一条 `DEFAULT VALUES` 语句（**不是**被当作「无改动」丢掉） |
| `canonicalize_insert_skips_schema_checks_when_columns_are_unknown` | V-4：`options.columns` 为空（如 `rich_mock_options()`）→ 不拒绝，附 `insert-schema-unavailable` warning |
| `fingerprint_covers_insert_values_and_stays_stable_without_inserts` | 全部 `insert_marked == false` 时指纹与旧实现**逐字节相同**（保护 `commit_reuses_plan_fingerprint_and_returns_plan_id`）；改一个写入列值 → 指纹变化；对调两条内容不同的草稿 → 指纹变化 |
| `insert_statement_values_never_reach_the_sql_text` | 写入值含 `'`、`\`、`\n` → `sql_template` 只有占位符，值出现在 `parameters`；`sql_template` 内不含该值的任何字面量 |
| `insert_and_update_share_one_transaction_and_one_commit` | **陷阱 2**：`execute_calls() == 2`（真执行两次）、`commit_calls() == 1`、`open_transaction_count() == 0`、`affected_rows == 2` |
| `insert_joins_manual_handle_transaction_instead_of_committing` | 传入手动句柄（`manual_handle: Some(h)`，`TransactionState::Active`）时 `commit_calls()` 不增长、`open_transaction_count()` 不归零、但插入已执行（**注意**：搭法是传入 `manual_handle: Some(h)`（`TransactionState::Active`），断言不得引用已不存在的 `AppState` 会话表，见 §11 Q-8 的裁定 1 已改口径） |
| `insert_rolls_back_whole_batch_on_failure` | **事务原子性**：让某条语句失败（`execute_rows_affected = 2` 触发 `grid.commit.affectedMismatch`，或 `MockDriverOptions.execute_error`）→ 整批 `Err`、`commit_calls()` 不增长、`open_transaction_count()` 归零。**注意**：`MockDriver` 只把 `rollback_calls` 记在内部、无公开访问器，因此要么补一个 `rollback_calls()`（测试基建纯新增），要么用「无 commit + 事务计数归零」间接断言；不要为断言在**生产路径**加钩子 |
| `insert_joins_user_transaction_and_does_not_roll_it_back_on_failure` | 手动句柄事务内失败 → 不回滚那个手动事务（`open_transaction_count()` 保持）且错误上抛 |
| ~~`insert_uses_query_path_when_returning_clause_is_present`~~〔裁定 7：**本批不写**，随回填能力落地再启用〕 | `insert_returning_clause` 非 `None` 时走 `driver.query`（`mock.query_calls()` 增长、`execute_calls()` 不增长），回填后的 `row_identity` 等于 `MockDriverOptions.query_rows` 给出的主键值 |
| `insert_identity_falls_back_to_written_primary_key_when_returning_is_none` | 〔契约审计修订 2026-10-21〕**后端未实装**：新行的 `row_identity` 被 `canonicalize_insert` 清空，且 `write_exec` 原样回传该空身份 ⇒ 「用户写满主键即可精确定位」这条路径**当前走不通**，前端只能靠「最大主键候补」（且要求候补**比提交前更大**，否则宁可不定位）。**不在本批补**（改 `canonicalize_insert` 会触及 Wave 1 已验收的指纹/规范化逻辑，收益不抵风险）；由 Wave 3 前端按**降级**口径实现（定位不到就明说），后端补回填另立条目 | **本批唯一启用的回填相关用例**（不依赖 `insert_returning_clause`）：驱动返回 `None`、草稿写满全部主键列 → `row_identity` 非空且等于用户写入值，**不**出现降级 warning |
| `insert_without_returning_reports_degradation_warning` | **本批核心用例**：驱动返回 `None` 且未写满主键 → `row_identity` 为空 map + warning `insert-returning-unavailable`；并断言**没有**任何 `driver.query` 调用（证明宿主真的没走回填）、`driver.execute` 恰好 1 次 |
| `insert_locate_new_row_falls_back_to_max_primary_key_when_row_identity_empty` | **本批定位策略**：`row_identity` 为空、表为单列可比较主键 → 刷新后取当前页主键最大行为候补并高亮；表为复合主键 → 不取候补、直接走降级 warning |
| `insert_affected_zero_is_rejected_like_every_other_statement` | **R-C 回归**：`execute_rows_affected = 0` → `Err` + `grid.commit.affectedMismatch`（文案含 `INSERT affected 0 rows`）；`= 2` → 同样 `Err`。**不存在**「0 行容忍」分支 |

### 9.2 Rust 单元（`driver-api`：`packages/driver-api/src/sql_builder.rs`、`traits.rs`、`reuse.rs` 的 `#[cfg(test)]`）

| 用例名 | 断言要点 |
| --- | --- |
| `insert_statement_lists_written_columns_and_binds_each_value` | **必含**：入参 `[("name", Some(Value::String("a"))), ("note", None)]`（`id` 缺席）→ `sql == "INSERT INTO \"t\" (\"name\", \"note\") VALUES (?1, ?2)"`、`parameters == [String("a"), Null]`、`sql` **不含** `id` |
| `insert_statement_with_no_written_columns_is_default_values` | **必含**：空切片 → 恰好等于 `INSERT INTO "t" DEFAULT VALUES`、`parameters` 为空 |
| `insert_statement_binds_values_instead_of_escaping_them` | **必含**：`Value::String("O'Brien\\x")` → `sql` 里**没有**该值（只有占位符），`parameters[0]` 逐字等于原值；再断言 `Value::Bytes` / `Value::Bool(true)` 同样只出现在 `parameters` |
| `insert_statement_matches_update_and_delete_quoting` | **必含（风格一致性）**：同一驱动、同一表名与列名，`build_insert_statement(..).sql` 与 `build_update_statement(..).sql` / `build_delete_statement(..).sql` 使用**同一个**引号形态（由 `quote_char` 决定）：显式断言三处表名渲染**逐字相同**、列名渲染相同（把 `quote_ident` 换掉时三条同时变红） |
| `insert_statement_exceeding_the_parameter_budget_fails` | **必含**：列数超过 `max_bound_parameters()` → `Err(DriverError::Unsupported(_))`，且**不返回**任何半成品 `sql` |
| `insert_statement_is_refused_when_bound_writes_are_unsupported` | `supports_bound_writes() == false` → `Err(Unsupported("parameterized row writes are not supported"))` |
| `insert_returning_clause_default_is_none` | 默认实现返回 `None`（保证未声明驱动走降级路径） |
| `reuse_driver_forwards_insert_builders` | 内层驱动覆盖 `build_insert_statement` 返回哨兵 SQL → `ReuseDriver` 包装后输出等于哨兵（不转发时退回默认实现，用例变红） |
| `reuse_driver_forwards_insert_returning_clause` | 同上，覆盖回填子句 |

### 9.3 驱动 crate 测试（`packages/drivers/<id>/`）

| crate | 用例名 | 断言要点 |
| --- | --- | --- |
| `datazen-driver-postgres` | `insert_statement_distinguishes_null_from_an_unwritten_column` | 不写的列**不出现**；`None` → 绑定 `Value::Null`；`Some(String(""))` → 绑定空串（三者互不相同）；`insert_returning_clause(&["id"])` → `Some("RETURNING \"id\"")` |
| `datazen-driver-mysql` | `insert_statement_with_no_written_columns_uses_empty_values_list` | 空切片 → `INSERT INTO `t` () VALUES ()`（**不是** `DEFAULT VALUES`）；反引号引号形态；`insert_returning_clause` → `None` |
| `datazen-driver-sqlite` | `insert_statement_returns_returning_clause` | 同 postgres（`RETURNING`）；空串参数 ≠ `NULL` 参数 |
| `datazen-driver-sqlserver` | `insert_statement_inlines_output_inserted_and_signals_inlined_returning` | `build_insert_statement(..).sql` 含 `OUTPUT INSERTED.` 且位置在列清单与 `VALUES` **之间**；`insert_returning_clause` → `Some(s)` 且 `s.is_empty()`；值仍只在 `parameters` 里 |

### 9.4 前端单测（Vitest）

| 文件 / 用例名 | 断言要点 |
| --- | --- |
| `src/lib/__tests__/tableChanges.test.ts::draft_values_round_trip_the_wire_shape` | `draftsForWire` 产出的条目序列化后含 `insertMarked: true`、`changedColumns` 为排序后的键集、`currentValues` 里 `null` 原样保留；`insertMarked: false` 的既有改动序列化后**不含**该键（保证旧指纹逐字节不变） |
| `…::draft_write_columns_excludes_deleted_keys` | 键存在即入列（含 `null`）；键被删（`undefined`）→ 从 `changedColumns` 消失；`0` / `false` / `''` 不得被误判成「未填」 |
| `…::draft_insert_count_and_has_pending_work_cover_both_collections` | `pendingChanges` 空 + `draftInserts` 非空 → 仍算「有待提交工作」 |
| `src/stores/__tests__/tableDataStore.test.ts::preview_sends_drafts_in_creation_order` | `previewPendingChanges` 收到的 `changes` 末尾按 `draftInsertOrder` 追加了草稿条目，且请求体**没有** `inserts` 字段 |
| `…::commit_clears_drafts_and_marks_highlighted_row` | 提交成功后 `draftInserts.size === 0`、`draftInsertOrder` 为空、`highlightedNewRowKey` 被设置 |
| `…::preview_is_invalidated_when_a_draft_changes` | 预览返回前修改一条草稿 → `previewPlan` **不**被写入（沿用既有签名保护思想） |
| `…::no_primary_key_table_allows_drafts_but_not_cell_edits` | 无主键表：`addDraftRow` 生效；`stageCellChange` 仍报 `tableData.noPrimaryKey` |
| `…::new_locale_keys_exist_in_en_query_pack` | 遍历 §8 的新 key（**排除**已被 R-C / T2-4 删除的两条），断言都能在 `src/locales/en/query.ts` 的默认导出里取到非空字符串（防 `I18nKey` 的 `string & {}` 逃逸） |
| `src/lib/__tests__/gridErrors.test.ts::classify_maps_insert_prefixes_and_falls_back_to_raw` | 〔契约审计修订 2026-10-21〕**用例改写**：`grid.insert.noWritableColumn` **本批不在前端注册表**（零生产者，见 §8 的裁定），所以这条断言**不能**用它做例子；改用**有生产者的** insert 域码（`grid.insert.unknownColumn` / `grid.insert.returningUnavailable`）做分类断言，并额外断言「未登记的码分类为 `unknown` 且**回退到原始消息**」——即 `classifyGridError('grid.insert.noWritableColumn: …')` → `'unknown'`（该断言的**用意**是「不把未登记的码当已登记」）。V-3 实装后再加一条 `noWritableColumn` 的分类断言 |rt.noWritableColumn'`；无关消息 → `'unknown'` 且调用方拿到**原始字符串** |

### 9.5 连续旅程测试（journey）

`src/stores/__tests__/draftInserts.journey.test.ts`：模拟完整击键序列并断言每一步的状态跃迁（进入条件 / 状态内行为 / 退出跃迁，含残缺中间态）：

1. 打开无主键表 → 点「+ 行」→ 断言 `draftInserts.size === 1`、`ts.rows` **未变**（R-5）。
2. 在 `name` 格输入 `ab` → 断言 `values['name'] === 'ab'`、`draftWriteColumns(values)` 为 `['name']`。
3. 再输入 `c`（模拟 IME composition 中间态）→ 断言 composition 期间草稿值不变、不触发 `previewPendingChanges`。
4. `Esc` → 断言**有内容**的草稿行**不**被移除、回到 `drafting` 而不是 `idle`。
5. `⌘+D` → 断言两条草稿、`draftId` 不同、值深拷贝（改一条不影响另一条）。
6. `Delete`（焦点在草稿行号槽）→ 删一条，剩一条。
7. 把 `name` 格清空 → 断言键被删掉（回到「不写」），`changedColumns` 为空。
8. 点「提交」→ 断言请求体 `changes` 里恰好一条 `insertMarked: true`、顺序 = `draftInsertOrder`，且没有 `inserts` 字段。
9. 提交失败 → 断言草稿**全部保留**、`ts.error` 是 `classifyGridError` 分类后的文案。

## 10. 自查清单

| # | 错误做法 | 正确做法 | 后果 |
| --- | --- | --- | --- |
| 1 | 给 `RowChangePlan` 加上 `inserts` 就算完，忘了改 `execute_row_change_plan_impl` 的**首个空计划判断** | 三个集合一起判空：`inserts.is_empty() && updates.is_empty() && deletes.is_empty()` | **界面显示提交成功、数据库里什么都没有，而且不报错**（契约 §1.4 事实 3） |
| 2 | 忘了让 `plan_changes` / `validate_immutable_plan` 把 `inserts` 反推回来 | 重建计划时必须带上插入（`plan_changes` 返回二元组或新增 `plan_inserts`） | 每次带插入的提交都被判「指纹已过期/计划被动过」，功能完全不可用，且错误文案会把人引向错误方向 |
| 3 | 把插入语句写在第 `async` 块之外（`begin_transaction` 之前，或 `commit` 之后补跑） | 插入循环放在**同一个** `async {}` 执行块内，顺序 `inserts → updates → deletes` | 插入落在另一个事务（或落在已提交之后无法回滚）：用户事务路径与自动事务路径都破坏原子性，产生**真正没法回滚的脏数据** |
| 4 | 复用 `canonicalize_changes` 处理插入，于是「一列都不写」的行被当成「无改动」丢掉 | 独立的 `canonicalize_insert` 分支（由 `insert_marked` 分拣）；「一列都不写」在 INSERT 语义下是合法的 `DEFAULT VALUES`，只能由 V-3 的 NOT NULL 规则否决 | 用户点「提交」后没有错误、也没有插入（静默丢行），高发在「只想插一行全默认值」的场景 |
| 5 | 给「不写」的列补 `NULL` 以「凑齐列」 | 这些列完全不出现在 `changed_columns` 与语句里 | 自增列、`DEFAULT now()`、生成列全部失效或直接报错（正是契约 §3.1 要消灭的动机） |
| 6 | 让插入也走 `affected == 1` 的严格校验，并把 0 当失败 | 插入的判据是「语句执行未报错」+ 拒绝 `affected > 1`；`affected == 0` 记 warning 不作失败 | 不上报行数的驱动（如 ClickHouse）上所有插入提交都被拒；或者反过来，把真正的失败当成 0 放过去 |
| 7 | `insert_returning_clause` 返回 `None` 时「猜」主键（例如取当前页最大主键 + 1） | `None` 时**绝不猜**：走「提交后重新拉当前页」的降级路径，UI 明确不承诺高亮 | 并发插入下高亮/身份指向错误的行，后续基于该身份的编辑会改错行 |
| 8 | 把 SQL Server 的 `OUTPUT INSERTED.*` 当普通子句**追加**在 `VALUES` 之后 | 由驱动的 `build_insert_statement` 内联到列清单与 `VALUES` 之间，`insert_returning_clause` 返回 `Some("")` 表示「已内联、请用查询路径读」 | T-SQL 语法错误，或（更糟）回填读不到行导致每次提交都报 `returningUnavailable` |
| 9 | 新增 trait 方法后忘了在 `ReuseDriver` 里补转发 | 在 `ReuseDriver` 的 impl 里显式转发 `build_insert_statement` / `insert_returning_clause` | 所有 MySQL 协议族包装驱动（doris / starrocks / oceanbase…）退回默认实现，`DEFAULT VALUES` 在 MySQL 上直接语法错误 |
| 10 | 把草稿行塞进 `pendingChanges`，或写进 `ts.rows` | 独立的 `draftInserts` + 派生渲染，永不写 `rows` | 无主键表上 `buildRowIdentity`/`rowIdentityKey` 抛错或静默失效；真实行集合被污染（导出、去重、排序、行数统计全部错） |
| 11 | 新增 trait 方法时顺手改了 `build_update_sql` 的签名或 `Value` 的序列化形态，却没提升 `PROTOCOL_VERSION` | 只新增不动既有签名；确需改既有形态时**必须**提升版本并同步所有 git 驱动 `ref` | 版本号与实际契约不符，宿主与插件在「接受的版本区间」内却互相误解协议 |
| 12 | 后端报错写成 `no writable column` 这类没有前缀的裸文本，前端用 `includes('column')` 判断 | 后端一律 `grid.<域>.<原因>: ` 前缀；前端用**前缀匹配**的 `classifyGridError()`，未知回退显示原文 | 文案里恰好出现同一个词就误判；错误分类在两侧都无法逐字断言 |

---

## 11. 未决问题

| # | 问题 | 建议选项 |
| --- | --- | --- |
| Q-1 | **回填子句的位置**：契约 §5.1 只冻结了 `insert_returning_clause()`，宿主「追加」的语义对 T-SQL 不成立（`OUTPUT` 必须在列清单与 `VALUES` 之间）。是否新增位置枚举/新 trait 方法？ | 建议**不新增**：维持「`Some("")` = 已内联、宿主用查询路径读」的约定，在 `traits.rs` 文档注释写清并由驱动测试锁死（零新增方法、不破冻结签名）。备选：新增 `insert_returning_placement()`（带默认实现，不破兼容）——需要人工裁定是否值得为此扩契约 **〔契约审计修订 2026-10-21〕裁定 7：本批不做 RETURNING 回填，Q-1 / Q-2 / Q-3 三条一并推迟**——它们都只在「宿主开始调用 `insert_returning_clause`」之后才成立；本批 `row_identity` 恒为空、插入成功后按最大主键候补定位或降级，因此这三条**不在本批解决**，登记为回填能力落地时的前置裁定。 |
| Q-2 | **MySQL 族的主键回填**：契约把 MySQL 归入「用 `LAST_INSERT_ID()`，由驱动另行处理」，但 §5.1 没有任何「插入后再查一次」的入口，因此 MySQL 只能返回 `None` 走降级（无高亮）。 | 建议本册先按 `None` 降级交付（不猜、不假装）；若产品要求 MySQL 也高亮，另开一条契约改动（新增 `post_insert_identity_query(&self) -> Option<String>` 类方法，带默认实现，不破兼容）——**不属于本册范围**，需人工确认优先级 |
| Q-3 | **SQLite 的 `RETURNING` 版本门槛**：SQLite 3.35 起支持 `RETURNING`，更早版本语法错误。驱动该在连接时探测版本，还是一律返回 `None`？ | 建议**在连接时读版本并缓存到驱动实例**，`insert_returning_clause` 按版本返回 `Some`/`None`；不引入新 trait 方法。备选：一律 `None`（最保守，退回降级路径） |
| Q-4 | **草稿行与提交结果的关联**：`PlannedStatement` 刻意不加 `draftId`，前端靠「插入结果与 `plan.inserts` 同序」对齐。是否要显式字段？ | 〔契约审计修订 2026-10-21〕T2-4：**已关闭**——`draftId` 连线上都不进（新行是 `PendingRowChange.insert_marked`），计划里更没有它的位置。关联靠**顺序契约**：`RowChangePlan.inserts` 与 `RowCommitStatementResult` 都按请求顺序回传，前端按 `draftInsertOrder` 逐条对齐。**不加字段**；为防止「后来加回去」，`fingerprint_covers_insert_values_and_stays_stable_without_inserts` 与 `draft_values_round_trip_the_wire_shape` 各锁一次。 |
| Q-5 | **`src/lib/gridErrors.ts` 的归属**：01 分册把它登记为「由 04/05 首次建立」，03 分册的文件清单里已把它列为新增（冲突）。 | **已裁定并关闭：归属冻结给 01**（契约 §8.1 归属总则）。理由不是「以先落地者为准」——那仍然依赖不可知的合并时序——而是 **01 是提交顺序里唯一最早的分册**，因此无论并行开发如何交错，它必然先落地。本册只需按三步追加 `grid.insert.*`，无需请求任何分册为它让路。**附带硬约束：禁止整文件重写该文件**（并行下会静默抹掉他人码值且无编译错误） |
| Q-6 | **执行顺序是否要驱动可声明**：PRD 提到「顺序固定 inserts → updates → deletes；需要别的顺序由驱动声明」，但 00-contracts §5 冻结的新增方法只有 5 个，其中没有顺序声明入口。 | **已裁定并冻结顺序声明**：① 顺序**恒为** `inserts → updates → deletes`，同一类内部按 `plan` 数组顺序（插入按请求顺序，即前端草稿视觉顺序）；② **不提供驱动声明入口**——契约 §5 不加方法，宿主**禁止**按 `driver_type()` 分支（违反零硬编码）；③ 确需别的顺序时，走**新的契约改动**（新增带默认实现的方法，默认实现即本顺序，所以未覆写的驱动零影响），不得在宿主里私开分支；④ 这条顺序与「同一事务内执行」是两件事：原子性见 Step 5 的陷阱 2，顺序见本条，两条都要有测试 |
| Q-7 | **`affected == 0` 的容忍度**：旧稿按「驱动不上报计数」容忍 0 并记 warning，是否要更严格？ | 〔契约审计修订 2026-10-21〕**R-C 已裁定：不豁免**。INSERT 与 UPDATE/DELETE 走同一条护栏——`affected != 1` 整批拒绝（`INSERT affected {n} rows; refusing ambiguous write`），`insert-count-unavailable` 这条 warning 与同名 i18n key 一并删除。理由：①「0 行影响」对单行 `VALUES` 不是可忽略的噪声，而是「没插进去」的最可能形态（触发器吞掉、权限过滤、上游误路由）；②容忍 0 会让「提交成功但页面没有新行」成为合法终态，用户无法区分这两者。**剩下的开放点**：若要支持幂等重放（网络超时后由客户端重试同一批），`0` 行需要一条**独立**的成功路径，前提是（a）有明确唯一键、（b）驱动确实上报计数、（c）请求带幂等标记；这三条本批都不具备，故**不实现**，登记为后续契约改动项。 |
| Q-8 | **事务归属的表述还停在旧模型**：本册 §4 / §7 第 8-9 条 / §9.1 的三条事务用例原先用「宿主会话事务表 + 一个『会话是否已开事务』的布尔量」描述「并入用户事务」。 | **〔契约审计修订 2026-10-21〕已裁定，并已按裁定改写**（协调者裁定 1）：真实形态只有「可选手动句柄」这一个入口——`commit_pending_changes` 接受 `manual_handle: Option<SessionHandle>`；`Some(h)` 时校验目标库与 `TransactionState::Active`，把语句并入该事务（`platform.beginTransaction` 的 `defersCommit: true`），**不自行提交**；`None` 时自行开一次会话级事务并在结束时 `commitTransaction` / 回滚。宿主上那张会话事务表已不存在，裸 `BEGIN` 退化路径也不存在。本册已按此改写 §4 事实第 3 条、§5 Step 5 的第 3 点与三条用例、§7 第 8-9 条、§9.1 的三条事务用例；**测试搭法改为构造 `TransactionState::Active` 的 `SessionHandle` 并从 `manual_handle` 传入**。 |
