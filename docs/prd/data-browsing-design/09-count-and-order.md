# 09 · 行数三态与默认排序可配（DB-09）

> **状态**：目标设计，**未实现**。本文档中凡未标注「既有」的符号一律是**新增**。
>
> **依赖**：[00-contracts](00-contracts.md) §5.3（`count_strategy` / `count_estimate_sql`）、§5.4（`default_order_columns` / `allow_first_column_fallback_order`）、§6.1（`estimate_table_rows`）、§6.2（`getTableData` 的两个新增可选入参）、§7（`DataGridCapabilities`）、§8（`grid.<域>.<原因>` 错误前缀与 `classifyGridError`）。**这些签名已冻结，本册不得改动**；本册只决定「怎么用、怎么回退、怎么测」。
>
> **被谁依赖**：分册 11（键集分页需要一个明确的、可复现的排序键来源，本册先把「默认排序从哪来、能不能关」定死）；PRD 的 DB-16（无主键显式化）、C-46（行数三态 + 阈值）、C-48（深翻页的前置条件）、D-6、R5、R7。
>
> **⚠ 本册包含一次有意的行为变更（契约 §5.4 要求显式声明）**：无显式排序时，排序列的**主键来源**从「遍历 `columns` 上的 `is_primary_key` 标志」改为「`CachedColumns.primary_keys`（已经过 `TableSchema::effective_primary_keys()` 归一化）」。当某个驱动的「列标志」与「`primary_keys` 字段」不一致时，新实现按**真实主键**排序，旧实现退化成**按第一列**排序。**这不是回归，是对既有隐患的修正**，但其影响面、合入条件与测试必须按契约 §5.4 的三条逐一交代（见 §1.1 / §2.1 / §3.2 / §5 / §7 / §9）。**除这一处之外，未改动驱动的 SQL 与结果保持不变。**
>
> **预估工作量**：**2.5 ~ 3.5 人日**。Rust 契约与默认实现（含 `ReuseDriver` 转发、mock 扩展）0.5；Host 执行器与 IPC 0.5；PostgreSQL 估算落地与驱动内测试 0.5；前端三态渲染 + 设置项 + 面板覆盖 1.0；测试与门禁 0.5。不含分册 11 的键集分页。
>
> **本文档不包含**：
>
> - 键集（seek）分页与 `keyset_predicate_sql` 的落地 —— 属分册 11（`supports_keyset_pagination` 的接口已在本册依赖的契约里冻结，本册不实现）。
> - 多列排序的 UI 与拖拽（C-41 / D-8）。本册只在「默认排序」这一条路径上用多列（默认实现天然产主键多列升序），不改用户手动排序的交互。
> - 筛选算子扩展与命名视图 —— 属分册 08。但「**count 与 select 必须共用同一份筛选条件**」这条纪律由本册第 7 节钉死。
> - `TableInfo.rowCount` 在对象树里的展示与刷新策略（既有能力，本册只把它当作 `auto` 模式的先验输入）。
> - 列宽 / 列顺序 / 排序状态的**按表持久化**（D-9 与 C-42）。本册只定义「默认表排序」这一个全局设置项的语义，不新建任何按表偏好存储。

---

## 1. 目标与验收口径

**一句话目标**：把「这一页有多少行」从一个布尔位升级为驱动可声明的三态（精确 / 估算 / 不支持），并把「无显式排序时宿主隐式注入的 `ORDER BY <主键 ASC>`」从写死的行为改造成「驱动可声明、用户可配置、可显式关闭」的能力。除 §1.1 显式声明的那一次行为变更外，未改动驱动的 SQL 与结果保持不变。

### 1.1 本次唯一的行为变更与影响面（契约 §5.4 合入条件 ①）

| 项 | 内容 |
| --- | --- |
| **变更** | 无显式排序时，主键列的来源从「`columns` 上的 `is_primary_key` 标志」改为「`CachedColumns.primary_keys`」 |
| **为什么** | `CachedColumns.primary_keys` 已在缓存层经过 `TableSchema::effective_primary_keys()` 归一化（优先 `primary_keys` 字段）；而今天的 `build_select_sql` **完全没读它**，只遍历列标志。于是「驱动填了 `primary_keys` 却没在列上打 `is_primary_key` 标志」的表会**找不到主键、退化成按第一列排序** —— 这是既有隐患，本册修正它 |
| **影响面（会变）** | ①「`primary_keys` 非空、但对应列未打 `is_primary_key` 标志」的表：从 `ORDER BY <第一列> ASC` 变为 `ORDER BY <真实主键> ASC`（结果集顺序与分页稳定性同时改变）；② 由 ① 派生：这类表不再出现「无主键、按首列排序」的提示（它现在真的有主键了） |
| **影响面（不会变）** | 列标志与 `primary_keys` 一致的驱动（**树内 15 个 path 驱动经代码核实均属此类**：postgres 的 `get_columns_impl` 用 `pk_names.contains(&name)` 同时产出列标志与主键表；其余驱动走 `traits::streaming::get_columns` 默认实现，其 `pks` 直接来自 `TableSchema::effective_primary_keys()`）；有主键且列标志齐全的表；显式排序路径；**全部行数路径**（`count_strategy` / `count_estimate_sql` 与排序无关） |
> ⚠ **驱动的计数口径**：`packages/drivers/` 下有 16 个目录，但其中 `http-support` **不是驱动**（`drivers-registry.json` 里没有它，crate 自述为「Shared HTTP helpers for DataZen path drivers」）。按注册表口径：path 驱动 **15** 个（postgres / mysql / sqlite / redis / mongodb / sqlserver / clickhouse / duckdb / elasticsearch / rqlite / turso / influxdb / victoriametrics / hbase / vector），git 驱动 **3** 个（kiwi / olap / superset），合计 **18**。**不要用目录数当驱动数。**
| **无法穷举的部分** | `drivers-registry.json` 里的 git 驱动（kiwi / olap / superset 等）源码在构建时才克隆，**不在本工作树内，其列标志与 `primary_keys` 是否一致无法核实**。因此合入时必须：在 PR 描述里列出树内核实结果，并在宿主加一条 `tracing::debug!`（当 `cached.primary_keys` 与「列标志推导结果」不一致时打印表名），让不一致的表可被发现与上报 |
| **合入条件** | ② 四条既有路径各一条测试 + 一条「列标志与 `primary_keys` 不一致时按 `primary_keys` 排序」的新语义测试（§9 A 组）；③ 已确认 `build_select_sql` 是**私有** `fn`（非 `pub`），给它加参数**不违反**契约 R1 —— R1 约束的是驱动可见的 trait 方法与 IPC DTO，不是宿主内部私有辅助函数 |

### 1.2 可观察的验收标准

| # | 验收点 | 判定方式 |
| --- | --- | --- |
| A1 | 有主键 / 无主键退化 / 复合主键 / 显式排序覆盖**四条路径**的 SQL 与今天一致（前提：该驱动的列标志与 `primary_keys` 一致） | 四条等价性单元测试断言完整 SQL 字符串相等 |
| A1b | **列标志与 `primary_keys` 不一致时，按 `primary_keys` 排序**（新语义，钉住 §1.1 的变更） | 单元测试：构造 `columns` 中无 `is_primary_key` 标志但 `primary_keys = ["id"]` 的输入，断言产出 `ORDER BY "id" ASC` 而**不是** `ORDER BY "<第一列>" ASC` |
| A2 | `count_strategy()` 默认实现由既有 `skip_count_query()` 推出，无驱动需要改代码 | `CountStrategy` 默认值单元测试 + `ReuseDriver` 转发测试 |
| A3 | 行数在 UI 上有且仅有三种形态：精确 `1-50 / 1234`、估算 `1-50 / ≈ 1.2M`、未知 `第 3 页`（无总页数） | `Pagination` 组件测试（三态各一条）+ 截图 |
| A4 | 估算态**必须**带 `≈` 与 tooltip，且提供一次性的「Count（精确统计）」入口 | 组件测试断言文本、`title`/`aria-label` 与按钮存在性；不允许估算值与精确值同形渲染 |
| A5 | 默认排序可配：`不排序 / 主键升 / 主键降`；选「不排序」时**必须**出现稳定性提示 | 设置项 + 面板覆盖的组件测试；E2E 断言提示文案可见 |
| A6 | 无主键表的**浏览路径不失败**（这是竞品 TablePlus 的硬失败点，PRD §4.12 / C-58） | E2E：打开无主键表 → 有数据、无错误横幅、出现「按首列排序」的提示 |
| A7 | 有生效筛选时**禁止**用估算行数（否则「筛选后 3 行」会显示成「约 120 万行」） | 单元测试：带完整筛选条件 + `countStrategy = 'estimated'` 时，实际下发 `SELECT COUNT(*)` |
| A8 | `unsupported`（含既有 `skip_count_query == true` 的驱动）下，分页器不得把「下一页」永久禁用 | 组件测试：`totalRowsKind='unsupported'` 且 `hasMore === true` 时 Next 可用 |
| A9 | 估算分支**不改变**「数据必须先于行数可用」的取舍：估算语句失败不得让整次加载失败 | 单元测试：估算 SQL 报错时仍返回行数据，`totalRowsKind === 'unsupported'`，且**没有**第二次往返（`query_calls() == 2`） |

### 1.3 命令与期望输出（门禁口径）

```bash
# ① Host 单元测试（本册改动的主要门禁；结论必须是 test result: ok）
cargo test -p datazen --lib 2>&1 | tail -n 5
#   期望：test result: ok. 0 failed（其中 query_executor / commands::schema 两个模块的用例全绿）

# ② 驱动契约与默认实现（新增方法的默认实现、ReuseDriver 转发、mock 扩展）
cargo test -p datazen-driver-api 2>&1 | tail -n 5
#   期望：test result: ok. 0 failed

# ③ 具体驱动（示例：PostgreSQL 的 reltuples 估算；驱动测试必须落在驱动 crate 内）
cargo test -p datazen-driver-postgres count_estimate 2>&1 | tail -n 5
#   期望：test result: ok. 且至少 1 passed（不得是 0 passed）

# ④ Host 前端单元测试（三态渲染 + 设置项 + store 写入）
npx vitest run src/components/DataTable/__tests__/Pagination.test.tsx \
               src/stores/__tests__/tableDataStore.test.ts \
               src/lib/__tests__/totalRowsDisplay.test.ts 2>&1 | tail -n 12
#   期望：Test Files  3 passed / Tests  0 failed

# ⑤ 类型门禁（测试文件参与类型检查，新增字段必须全套对齐）
pnpm typecheck 2>&1 | tail -n 5
#   期望：无 error TS 行

# ⑥ i18n 门禁：scripts 会扫描源码里所有字面量 t('…')，en 缺 key 即失败
npx vitest run src/locales/locales.test.ts 2>&1 | tail -n 12
#   期望：Test Files  1 passed（其中 "resolves every literal t() key used in host source" 必须绿）

# ⑦ E2E（构建方式必须走 pnpm 脚本，禁止裸 cargo build）
pnpm e2e:minimal -- --spec e2e/specs/table-count-and-order.ts
#   期望：spec 全绿；其中「无主键表打开不失败」是回归断言，不得 skip
```

> **本文档不新增门禁脚本**。以上全部复用既有命令；E2E 的 `--spec` 透传沿用 `node e2e/run.mjs` 的 `--` 后参数转发（`package.json` 里 `e2e:pro:sql-editor` 这类脚本已经是这个形态）。

---

## 2. 现状代码事实

本节只写**已经亲自读过**的符号，位置一律用「文件路径 + 符号名」。**全文不写行号。**

### 2.1 SQL 生成与执行（Host）

**`src-tauri/src/services/query_executor.rs`**（既有）：

- `QueryExecutor::get_table_data` —— 先 `self.schema_cache.get_columns(...)` 取 `CachedColumns`；用 `driver.quote_ident(name)` 作为 `qi`、用 `driver.format_sql_literal(&Some(v.clone()))` 作为 `format_lit`；分页子句来自 `driver.pagination_syntax(page_size, page * page_size)`（乘法用 `saturating_mul`）。`skip_count == true` 时**只**发 SELECT 并返回 `total_rows: None`；否则 `tokio::try_join!` 并发发 count 与 select，并把 count 结果的第一行第一格解析为 `total_rows`（只有 `Value::Integer` 会被接受，其他类型得到 `None`）。
- `QueryExecutor::build_select_sql` —— **本册的核心现状**：
  1. 列清单为空时发 `SELECT *`，否则逐列 `quote_ident` 后逗号连接；
  2. 有 `Some(order_by)` 时注入单列 `ORDER BY <col> ASC|DESC`，并置 `has_order_by = true`；
  3. **没有显式排序**时，主键列由 `columns.iter().filter(|c| c.is_primary_key).map(|c| c.name.as_str())` 得到（**按列顺序**）。**它完全没有读同一份缓存的 `primary_keys`**（`CachedColumns` 里就有，见下），也**不是** `TableSchema::effective_primary_keys()`；主键为空则退化为 `columns.first()`；两者都为空（`columns` 为空）时不注入任何 `ORDER BY`；
  4. 注入形式固定为每个列各一段 `<col> ASC` 再逗号连接，即复合主键是 `ORDER BY "a" ASC, "b" ASC`；
  5. 仍未 `has_order_by` 且 `pagination.requires_order_by` 为真时，才用 `pagination.order_by_fallback` 补一个驱动提供的中性排序（T-SQL 的 `(SELECT NULL)`）；
  6. 最后原样追加 `pagination.clause`。
- `QueryExecutor::build_count_sql` —— **方法体第一行就是 `let _ = columns;`，把传入的 `columns` 直接丢弃**，只产出 `SELECT COUNT(*) FROM <qi(table_name)>`，再按 `filters` 拼 `WHERE`。筛选渲染与 select 完全共用 `QueryExecutor::format_condition`、`QueryExecutor::filter_is_complete` 与自由函数 `filter_join`，因此两条语句的筛选条件是同一份（这条必须保持）。
- `QueryExecutor::format_condition`（既有，10 个算子）、`QueryExecutor::filter_is_complete`（既有，不完整条件返回 `String::new()` 被过滤掉）、`filter_join`（既有，只有 `" AND "` / `" OR "`）。
- `OrderBy { column, descending }`（既有，宿主机内结构，非 IPC DTO）与 `SortCondition { column, #[serde(default)] descending }`（既有，IPC DTO）。
- `#[cfg(test)] mod tests` —— **既有测试是本册新增测试的范本**，现有用例名（全部已读）：
  - `no_explicit_sort_uses_primary_key_order`：断言 `ORDER BY "id" ASC`（单列主键）。
  - `no_explicit_sort_composite_pk_uses_all_pk_columns`：断言 `ORDER BY "order_id" ASC, "product_id" ASC`。
  - `explicit_sort_overrides_default_pk_order`：显式 `name DESC` 时**不得**出现 `"id" ASC`。
  - `no_pk_no_explicit_sort_still_has_order_by_first_column`：断言 `ORDER BY "col_a" ASC`。
  - `tsql_pagination_emits_offset_fetch_and_never_limit` / `tsql_pagination_keeps_pk_order_without_fallback`：分页子句与中性 fallback 的边界。
  - `build_select_sql_without_offset_when_unsupported`、`build_count_sql_with_eq_filter`、`build_select_sql_joins_filters_with_or`、`build_count_sql_joins_filters_with_or`、`filter_join_defaults_to_and`、`format_condition_in_and_is_null`、`incomplete_eq_filter_is_skipped`、`complete_filters_kept_while_incomplete_skipped`、`filter_literals_use_formatter`。
  - `get_table_data_uses_schema_cache_and_returns_total`：用 `crate::testing::mock_driver::{MockDriver, MockDriverOptions}`，断言 `total_rows == Some(99)`、`rows.len() == 1`、`mock.query_calls() == 2`（count + data）。
- 该文件当前**总计 797 行**（生产代码到 `filter_join` 结束，其后全部是 `#[cfg(test)] mod tests`）——这直接决定了第 6 节的拆分结论。

**三条约束本册设计、必须逐字承认的现状事实（契约 §1.5 事实 7 / 8 / 9）**：

1. **多列排序在 IPC 边界就被截断**：`get_table_data_impl` 对 `sorts` 只取第一个元素（`sorts.and_then(|list| list.into_iter().next())`），**后面的排序条件被丢弃**。因此「只支持单列排序」不只在前端 store（`setSort` 只写 `[sort]`），后端入口同样如此；要支持多列排序必须**同时**改这两处（见 §11 Q2）。
2. **行数查询与数据查询是并发的，不存在先后关系**：`get_table_data` 用 `tokio::try_join!(count, select)` 同时发出两条语句（所以不能用「先数一下再决定」的写法）。`total_rows` 的解析路径是「count 结果的第一行 → 第一个单元格 → `as_ref()` → 只接受 `Value::Integer(i64)`」，因此**字符串 / `NULL` / 大整数越界等非整数形态一律得到 `None`** —— 这正是「三态」要表达的现实；本册任何改动都必须重新交代这条并发结构的取舍（§3.3 给出结论）。
3. **列与主键同源于一次 `get_columns`，但今天只用了列**：见 §2.2 的 `SchemaCache::get_columns`；`build_select_sql` 不读 `primary_keys` 构成既有隐患（§1.1）。

**`src-tauri/src/commands/schema.rs`**（既有）：

- `get_table_data_impl` —— IPC 命令的实际入口。它：从 `state.connection_manager.get_session(db_session_id)` 取 `(driver, handle)`；解析 `database`（显式 target 优先，否则会话配置，最后 `"default"`）；用 `metadata_schema(driver, schema, embedded_schema_of(table), config.schema)` 解析 schema；**把 `sorts` 数组只取第一个元素**（`sorts.and_then(|list| list.into_iter().next())`）转成 `OrderBy`；计算 `effective_skip_count = skip_count.unwrap_or(false) || driver.skip_count_query()`；调用 `QueryExecutor::get_table_data`。
- `get_table_data`（`#[tauri::command]`，同文件）—— 参数与 `get_table_data_impl` 一一对应，`skip_count: Option<bool>`、`filter_logic: Option<String>`、`database: Option<String>`、`schema: Option<String>` 全是可选，缺省不改变行为。
- `metadata_schema`（`src-tauri/src/services/schema_scope.rs`，由 `services/mod.rs` 再导出）、`embedded_schema_of`（本文件内的私有函数）。
- `mod tests` 声明在本文件末尾，用例落在 **`src-tauri/src/commands/schema/tests.rs`**（既有）：`get_table_data_with_query_rows`、`get_table_data_joins_filters_with_or`、`get_table_data_reads_the_target_database_without_switching`，以及共用的 `schema_commands_with_connected_mock` 夹具。
- 注册点在 **`src-tauri/src/bootstrap/run.rs`** 的 `tauri::generate_handler!` 列表（「Schema 元数据」分组里现有 `crate::commands::get_table_data` 等）。

**`src-tauri/src/cache/schema_cache.rs`**（既有）：

- `SchemaCache::get_columns` 返回 `CachedColumns { columns, primary_keys, table_name, cached_at }`；**`primary_keys` 已经存在且已归一化**（缓存命中路径由 `cached.schema.effective_primary_keys()` 得到；未命中路径直接取 `driver.get_columns(...)` 的第二个返回值，而该方法的默认实现 `traits::streaming::get_columns` 返回的正是 `TableSchema::effective_primary_keys()`）。但今天的 `build_select_sql` **只读 `cached.columns`，完全没读 `cached.primary_keys`**，而是从列上的 `is_primary_key` 标志重新推导 —— 两者不一致时就会漏掉主键（§1.1）。
- `CachedColumns.table_name` 就是调用方传入的 `table` 字符串，`build_select_sql` 的 `FROM` 用的也是它——本册新增的估算语句必须用**同一个标识符**，否则两条语句可能指向不同对象。

### 2.2 驱动契约（`packages/driver-api`）

**`packages/driver-api/src/traits.rs`**（既有，声明体）：

- `DatabaseDriver::skip_count_query(&self) -> bool`（默认 `false`）—— **今天唯一的「不数行数」表达能力**。
- `DatabaseDriver::supports_offset(&self) -> bool`（默认 `true`）、`pagination_syntax(&self, limit, offset) -> PaginationSyntax`（默认转发到 `traits::sql_text::pagination_syntax`）。
- `DatabaseDriver::quote_char` / `quote_ident`（默认转发 `traits::sql_text::quote_ident`）、`format_sql_literal`（默认转发 `traits::sql_text::format_sql_literal`）、`has_schema_level`（默认 `false`）、`has_multi_database`（默认 `false`）、`default_schema`（默认 `None`）。
- `DatabaseDriver::get_columns` 的默认实现转发到 `traits::streaming::get_columns`，后者用 `get_table_schema` + `TableSchema::effective_primary_keys()` 产出 `(columns, pks)`。
- 文件顶部自带说明：trait 是**故意**保持单文件的（「the interface driver authors read」），非契约的实现细节一律放到 `traits/` 子模块（`sql_text` / `streaming` / `command_api` / `key_value` / `schema_target`）。
- 该文件当前约 1020 行，**已经超过 800 行的单文件纪律**（既有负债）。

**`packages/driver-api/src/types.rs`**（既有）：

- `PaginationSyntax { clause: String, requires_order_by: bool, order_by_fallback: Option<&'static str> }`，文档明确「callers must not invent an `ORDER BY`」当 `order_by_fallback` 为 `None`。
- `TableDataResult { columns, rows, total_rows: Option<i64>, page, page_size }`（`#[serde(rename_all = "camelCase")]`，无 `#[serde(default)]` 的必填字段）。
- `ColumnSchema { name, data_type, nullable, default_value, comment, is_primary_key, is_auto_increment }`。
- `TableSchema { table_name, columns, primary_keys, indexes, foreign_keys, check_constraints, table_options }` 与 `TableSchema::effective_primary_keys()`（优先 `primary_keys` 字段，回退到列上的 `is_primary_key`）。
- `TableInfo { name, schema, table_type, row_count: Option<i64> }` —— `auto` 阈值模式的先验输入来源。
- 该文件当前约 939 行，同样**已经超过 800 行**（既有负债）。

**`packages/driver-api/src/reuse.rs`**（既有）：

- `ReuseDriver { inner: Arc<dyn DatabaseDriver>, .. }` + `impl DatabaseDriver for ReuseDriver`，其中**逐方法显式转发**：`quote_char`、`skip_count_query`、`supports_offset`、`pagination_syntax`、`supports_explain`、`has_schema_level`、`default_schema`、`type_normalizer`、`structure_*` 等；构造入口是 `ReuseDriver::new` 与 `ReuseDriver::new_with_precise_cancel`。
- 使用方：**`packages/drivers/postgres/src/lib.rs`** 与 **`packages/drivers/mysql/src/lib.rs`** 的工厂把驱动包进 `ReuseDriver::new_with_precise_cancel(...)`（questdb / cloudberry 与多种 MySQL 系别名都走这条路）。
- 既有测试里有 `reuse_driver_forwards_sync_taxonomy`、`reuse_driver_forwards_structure_methods` 这类「转发必须生效」的用例范式。
- **结论（本册最重要的兼容性风险）**：新的 trait 方法如果在 `ReuseDriver` 里没有转发，那么被包裹的驱动（questdb / cloudberry / MySQL 别名）会**静默丢掉**自己的覆盖，退回默认实现——而这些驱动恰好是「整表统计行数最贵」的那一类。

**`packages/driver-api/src/mock_driver.rs`**（既有）：

- `MockDriverOptions`（手工 `impl Default`）字段包含 `columns`、`primary_keys`、`count_total`、`query_rows`、`table_schema`、`query_error` 等，**没有任何 `skip_count_query` / 估算相关字段**。
- `MockDriver` 提供 `query_calls()`、`get_columns_calls()`、`get_schema_calls()`、`set_table_schema_for_test()` 等测试钩子；以 `register_test_driver` 注册进 `DriverRegistry`（`get_table_data_uses_schema_cache_and_returns_total` 就是这个用法）。
- 该模块是 `pub mod mock_driver`，**没有 feature 门**，宿主测试可以直接用。

**`skip_count_query` 的覆盖现状（本册的兼容性事实基础）**：

- 全仓 grep `fn skip_count_query` 只命中两处：`packages/driver-api/src/traits.rs`（默认实现）与 `packages/driver-api/src/reuse.rs`（纯转发）。**树内 15 个 path 驱动（postgres / mysql / sqlite / sqlserver / clickhouse / duckdb / redis / mongodb / rqlite / turso / elasticsearch / influxdb / hbase / vector / victoriametrics + http-support）没有一个覆盖它。**
- PRD §7 DB-09 提到的 OLAP / Kiwi / Superset 在 `drivers-registry.json` 里是 `source: "git"` 的驱动，源码在构建时才克隆，**本工作树中不存在，因此其覆盖行为无法核实**。本册的兼容性论证只能采用「默认实现恒等于今天的行为」这一形式，**不得**依赖任何具体 git 驱动的实现细节。

### 2.3 前端

**`src/commands/database.ts`**（既有）：`databaseCommands.getTableData` 的入参含 `skipCount?: boolean`，透传为 `skipCount`；`databaseCommands.listTables` 已把 catalog 的 `relation.rowCount` 映射成 `TableInfo.rowCount`。

**`src/types/index.ts`**（既有）：`TableDataResult { columns, rows, totalRows?: number, page, pageSize }`（**`totalRows` 可选，且只有数字，没有可信度信息**）；`SortCondition { column, descending }`；`AppSettings`（含 `defaultPageSize: number`）。另有一份历史重复的 `SortCondition`/`FilterCondition` 定义在 `src/types/settings.ts`（分册 08 负责收敛，本册不动）。

**`src/components/DataTable/Pagination.tsx`**（既有）：`PaginationProps { page, pageSize, totalRows: number, onPageChange, onPageSizeChange, filterRevision?, onPageReset?, loading? }`；导出纯函数 `paginationReducer` 与 `resetPageOnFilterChange`；渲染 `{from}-{to} / {totalRows}` 与 `t('pagination.page') + " x / y " + t('pagination.pageOf')`；`totalPages = Math.max(1, Math.ceil(totalRows / pageSize))`，`current = Math.min(page, totalPages - 1)`，`from = totalRows === 0 ? 0 : current * pageSize + 1`。

**`src/components/DataTable/DataTable.tsx`**（既有）：`DataTableProps.totalRows?: number`；`hasPagination = page != null && pageSize != null && totalRows != null && onPageChange && onPageSizeChange`；把 `totalRows` 同时传给 `Pagination` 与 `DataExportDialog`。

**`src/windows/connection/ContentStatusBar.tsx`**（既有）：`ContentStatusBarProps.totalRows: number`，在 `totalRows > 0` 时渲染 `{totalRows} {t('connWin.rowCount')}`（`connWin.rowCount` = `'rows'`，在 `src/locales/en/connection.ts`）。

**`src/stores/tableDataStore.ts`**（既有，722 行）：

- `LoadTableDataParams` 含 `skipCount?: boolean`；`reloadPanel: (panelId, opts?: { skipCount?: boolean }) => void`。
- `commitFetchedPage` 是**唯一**写 `totalRows` 的地方：`totalRows: res.totalRows ?? current.totalRows`（缺省时保留上一次的值）。
- `loadTableData` 组装请求：`filters: filters.filter(isCompleteFilter)`、`sorts`、`skipCount`、`filterLogic`。
- `setPage` / `setPageSize` / `setSort` 三个 action 全部 `patchPanelForReload(...)` 后 `reloadPanel(panelId, { skipCount: true })` —— 即「翻页/改页大小/改排序时复用已缓存的 total，不重跑 COUNT」。
- `invalidateCachedData` 把 `totalRows` 归 0 并 `requestRevision + 1`。
- `src/stores/tableData/types.ts` 的 `TableState.totalRows: number`（无 kind 字段）；`sorts: SortCondition[]`。
- 既有测试 `src/stores/__tests__/tableDataStore.test.ts` 有 `setPage triggers reload with skipCount`、`setSort triggers reload` 等用例。

**`src/windows/connection/TableView.tsx`**（既有）：`const totalRows = ts?.totalRows ?? 0; const sorts = ts?.sorts ?? [];`，把 `totalRows` / `page` / `pageSize` / `sorts` / `onSort={actions.setSort}` 等传给 `DataTable`；只读判定链是 `readOnlyProp ?? (savedConnection?.readOnly || driverReadOnly)`，最终 `isEditable = !isConnectionReadOnly`（**注意：只读只影响写入入口，不影响读路径**）。

**`src/lib/databaseMeta.ts`**（既有）：`KvWorkspaceCapabilities`（全字段可选、缺省 `false`、声明写在驱动自己的 meta 文件里、宿主只判能力不判驱动 id）是本项目**已确立的向后兼容范式**；`DatabaseTypeMeta` 上**没有** `DataGridCapabilities`（契约 §7 要求新增）。

**`src/lib/loadBatchExportTable.ts`**（既有）：批导路径也调 `getTableData`，用 `skipCount: page > 0` —— 新增可选入参不能破坏它。

**locales**（既有）：`src/locales/en.ts` **只有一行** `export { default } from './en/index'`，改它无效；真正的词条在领域包 `src/locales/en/<域名>.ts`，分页与网格词条在 **`src/locales/en/query.ts`**（既有 `pagination.*`、`dataTable.*`、`tableView.*`），设置页词条在 **`src/locales/en/settings.ts`**（既有 `settings.defaultPageSize`）。`src/locales/en/index.ts` 自动合并各领域包，**新增 key 不需要改 index.ts**。`src/locales/index.ts` 把 `I18nKey` 定义为 `TranslationKey | (string & {})`，所以只加 en 不会产生类型错误；但 `src/locales/locales.test.ts` 的用例 `resolves every literal t() key used in host source (en dictionary)` 会扫描 `src/` 下所有非测试 `.ts/.tsx` 里的字面量 `t('…')`，**en 里缺 key 直接失败**——因此第 8 节的清单必须完整。

**设置持久化**（既有）：`src/stores/settingsStore.ts` 的 `DEFAULT_SETTINGS` + `loadSettings`（用 `{...DEFAULT_SETTINGS, ...loaded}` 合并，因此新增字段对老用户自动取默认值）+ `updateSettings`；落盘走 `src/commands/settings.ts` 的 `getSettings` / `saveSettings`。**全仓 grep `tablePrefs` / `perTable` / `tablePreferences` 无命中 —— 今天不存在任何「按表偏好」的存储设施。**

---

## 3. 数据结构与接口设计

### 3.1 Rust：契约层（新增，位置由契约规定，本册不改签名）

```rust
// packages/driver-api/src/types.rs（新增类型，位置与定义照抄 00-contracts §5.3）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CountStrategy {
    Exact,        // COUNT(*) 可信且开销可接受
    Estimated,    // 只能给估计值，UI 必须显示「约 N 行」
    Unsupported,  // 不提供行数
}
```

```rust
// packages/driver-api/src/traits.rs（新增方法声明；方法体转发到新增的 traits/browsing.rs）
fn count_strategy(&self) -> CountStrategy {
    if self.skip_count_query() { CountStrategy::Unsupported } else { CountStrategy::Exact }
}
fn count_estimate_sql(&self, database: &str, schema: Option<&str>, table: &str) -> Option<String> { None }
fn default_order_columns(&self, columns: &[ColumnSchema], primary_keys: &[String]) -> Vec<String> {
    if !primary_keys.is_empty() { return primary_keys.to_vec(); }
    columns.first().map(|c| c.name.clone()).into_iter().collect()
}
fn allow_first_column_fallback_order(&self) -> bool { true }
```

**逐字段/逐方法说明与缺省值**：

| 符号 | 缺省值 | 为什么是这个缺省 |
| --- | --- | --- |
| `count_strategy()` | 由 `skip_count_query()` 推出 | 树内 15 个 path 驱动与所有未改动的 git 驱动都得到 `Exact`，与今天一致；`skip_count_query == true` 的驱动得到 `Unsupported`，也与今天一致。**这是全册最重要的兼容性技巧** |
| `count_estimate_sql()` | `None` | 「我没有估算手段」= 退回精确计数，绝不猜 |
| `default_order_columns()` | 主键列 → 否则第一列 → 否则空 | **选择逻辑**必须复刻今天 `build_select_sql` 的三段规则（主键 → 无主键退化首列 → 空），但这**不等于**行为等价：⚠ **输入变了**：宿主传的是 `CachedColumns.primary_keys`（归一化后的真实主键），而不是从列标志现推 —— 这是 §1.1 声明的行为变更，不能写成「完全等价」 |
| `allow_first_column_fallback_order()` | `true` | `false` 会让无主键表从「按首列排序」变成「不排序」，是行为变化，只能由驱动显式声明 |

> **参数语义（契约 §5.3 已冻结，与本文档早期草稿不同）**：`count_estimate_sql(database, schema, table)` 收到的是**未引用的原始表名 + 显式库/schema**，与 `get_columns` / `get_table_schema` 的既有惯例完全一致；**标识符引用由驱动自己做**（用 `self.quote_ident` / `self.quote_char`），宿主不代劳。
>
> 早期草稿传的是「已经引用好的表名」（`quoted_table`），**已废弃**。原因是它让两个主流方言的估算无法安全实现：MySQL 必须走 `information_schema.TABLES WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?`，SQL Server 必须用三段名（`db.schema.table`）或 `sys.dm_db_partition_stats` —— 两者都需要**未引用**的库名与表名。而「靠驱动会话语境自洽（`DATABASE()` / `DB_NAME()`）」是不安全的：宿主是**以显式目标取数**的，会话默认库未必等于本次请求的库，那样会算出**另一张表**的行数。因此本次改为显式传入 `database` / `schema`。

### 3.2 Rust：Host 侧新增 / 变更

```rust
// packages/driver-api/src/types.rs（修改：只新增字段，既有字段名与类型一个字都不动）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableDataResult {
    pub columns: Vec<ColumnSchema>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub total_rows: Option<i64>,
    /// 新增。本次响应所带行数的可信度；`None` = 本次**没有**重新取行数
    /// （调用方要求复用缓存，或旧调用方/旧驱动不填），前端必须保留上一次的值。
    #[serde(default)]
    pub total_rows_kind: Option<CountStrategy>,
    pub page: u32,
    pub page_size: u32,
}
```

**为什么用「新增字段」而不是把 `total_rows` 改成带 kind 的结构**：`TableDataResult` 是既有 IPC DTO，契约 §2 R1 禁止改既有字段的类型与名字；而 `total_rows_kind` 缺省 `None` 恰好表达「本次没取行数」，与今天 `total_rows: None` 的语义严格对齐。构造点只有两处（`QueryExecutor::get_table_data` 的两个 `Ok(TableDataResult { .. })`），改动可控。

```rust
// src-tauri/src/services/table_browsing.rs（新增文件：纯函数，便于单测，避免撑爆 query_executor.rs）
use datazen_driver_api::{ColumnSchema, CountStrategy, DatabaseDriver, FilterCondition};

/// 本次请求实际采用的行数策略。优先级：调用方显式要求 > 驱动声明 > 安全降级。
pub(crate) fn resolve_count_strategy(
    driver: &dyn DatabaseDriver,
    requested: Option<CountStrategy>,
    skip_count: bool,
    has_effective_filters: bool,
) -> CountStrategy;

/// 无显式排序时生效的排序列（**升序**）。返回空数组 = 本次不加任何默认排序。
/// `override_columns` 来自 IPC 的 `defaultOrderColumns`（`Some(vec![])` = 显式要求不排序）。
pub(crate) fn resolve_default_order(
    driver: &dyn DatabaseDriver,
    columns: &[ColumnSchema],
    primary_keys: &[String],
    override_columns: Option<&[String]>,
) -> Vec<String>;
```

**`resolve_count_strategy` 的判定规则（必须逐条实现，且每条都有测试）**：

1. `skip_count == true` → `Unsupported`（调用方说「复用缓存」，一个计数都不许发；这是今天的语义）。
2. 否则若调用方**显式**给了 `requested`：
   - `requested = Exact`：驱动声明 `Unsupported` 时仍返回 `Unsupported`（绝不违反驱动「我不数」的声明）；否则 `Exact`。
   - `requested = Estimated`：驱动声明 `Unsupported` → `Unsupported`；驱动 `count_estimate_sql(database, schema, table)` 为 `None` → **降级为 `Exact`**（宁可慢不可错）；否则 `Estimated`。
   - `requested = Unsupported` → `Unsupported`。
3. 否则用驱动声明 `driver.count_strategy()`，并施加同上的安全降级（`Estimated` 但没有估算语句 → `Exact`）。
4. **最后一道闸**：`has_effective_filters == true` 且结果是 `Estimated` → 降级为 `Exact`。理由：估算语句是**表级目录统计**，套不上 `WHERE`；一旦有生效筛选，估算值与筛选结果毫无关系（A7 验收点）。

**`has_effective_filters` 的判定**：复用既有语义 —— 只有当 `filters` 为空，或**所有**条件都被 `QueryExecutor::filter_is_complete` 判为不完整（即最终不会产生 `WHERE`）时才为 `false`。**禁止**另写一套「有没有筛选」的判断，否则会和 `build_count_sql` 的实际 SQL 漂移。

**`resolve_default_order` 的判定规则**：

1. `override_columns = Some(cols)` → 直接用 `cols`（含空数组，表示「显式要求不排序」）。
2. `override_columns = None` → 调 `driver.default_order_columns(columns, primary_keys)`；若结果非空且 `primary_keys` 为空且 `driver.allow_first_column_fallback_order()` 为 `false` → 返回空数组（把「第一列兜底」这个开关在**宿主侧**落地，使驱动无论是否覆盖过 `default_order_columns`，该开关都真实生效）。
3. `primary_keys` **必须传 `CachedColumns.primary_keys`**（即契约 §5.4 要求的新输入），**不得**再从 `columns` 上的 `is_primary_key` 标志重新推导。

> **⚠ 规则 3 就是 §1.1 声明的那次行为变更**。为什么必须这么传：`CachedColumns.primary_keys` 已经过 `TableSchema::effective_primary_keys()` 归一化（优先 `primary_keys` 字段、回退列标志），是「真实的键」；而列标志只是它的一个可能残缺的副本。契约 §1.6 也明确要求「凡是需要主键列的地方都应复用 `effective_primary_keys()`，不要自己过滤 `is_primary_key`（双重来源会漂移）」。**今天是宿主自己过滤列标志，属于该纪律的违例**，本册顺手改正。
>
> **实现时必须同时放一条可观测的告警**：当 `cached.primary_keys` 与「列标志推导结果」不一致时输出 `tracing::debug!`（带表名与两者内容）。理由：不一致的表正是本次变更的影响面，而 git 驱动不在树内、无法穷举；有了这条日志，用户上报「排序变了」时能一眼定位。**禁止**把它写成 `warn!` 级（正常的驱动差异不该刷屏），也**禁止**在 PR 描述之外不留下任何可观测痕迹。
>
> **门禁未覆盖到的风险要主动说明**：契约 §5.4 的合入条件 ③ 已确认 `build_select_sql` 是私有 `fn`（非 `pub`），给它加参数不违反 R1 —— R1 约束的是驱动可见的 trait 方法与 IPC DTO。但这次改的是**输入来源**，不是签名，因此「编译通过」不能证明行为没变；必须靠 §9 A 组的五条测试。

```rust
// src-tauri/src/services/query_executor.rs（修改：私有方法，非契约方法）
fn build_select_sql(
    table_name: &str,
    columns: &[ColumnSchema],
    filters: Option<Vec<FilterCondition>>,
    order_by: Option<OrderBy>,
    default_order: &[String],   // 新增：由 resolve_default_order 预先算好，空数组 = 不注入
    qi: &dyn Fn(&str) -> String,
    format_lit: &dyn Fn(&Value) -> String,
    pagination: &PaginationSyntax,
    filter_logic: Option<&str>,
) -> String;
```

`build_select_sql` 内部分支顺序**原样保留**：显式 `order_by` → 否则 `default_order` 非空则逐列注入 `<col> ASC` → 否则走既有的 `pagination.requires_order_by` + `order_by_fallback` 兜底。删掉的是「宿主自己从 `is_primary_key` 推导主键」的那段（它搬进了 `resolve_default_order` 的调用方）。

```rust
// src-tauri/src/services/query_executor.rs（修改 get_table_data 的参数列表）
pub async fn get_table_data(
    &self, driver: &Arc<dyn DatabaseDriver>, handle: &ConnectionHandle,
    db_session_id: &str, database: &str, schema: Option<&str>, table: &str,
    page: u32, page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    order_by: Option<OrderBy>,
    skip_count: bool,
    filter_logic: Option<&str>,
    count_strategy: Option<CountStrategy>,          // 新增
    default_order_columns: Option<Vec<String>>,     // 新增
) -> Result<TableDataResult, DriverError>;

/// 新增：取估计行数。`Ok(None)` = 该驱动/该表没有估算手段（**不是错误**）。
pub async fn estimate_row_count(
    &self, driver: &Arc<dyn DatabaseDriver>, handle: &ConnectionHandle,
    db_session_id: &str, database: &str, schema: Option<&str>, table: &str,
) -> Result<Option<i64>, DriverError>;
```

```rust
// src-tauri/src/commands/schema.rs（新增 IPC 命令）
#[tauri::command]
pub async fn estimate_table_rows(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    database: Option<String>,
    schema: Option<String>,
) -> Result<Option<i64>, CommandError>;
// 另有 pub(crate) async fn estimate_table_rows_impl(...)，参数同构，
// 供集成测试直接调用（与 get_table_data_impl / get_table_data 的既有分层一致）。
```

**`estimate_table_rows` 的返回语义**：`Ok(None)` = 无法估算（驱动没实现 `count_estimate_sql`、语句为空、**或返回了 NULL/非整数/负数这类不可用值**）；`Err` 只在会话不存在、目标解析失败等宿主层问题出现。**「估不出来」绝不是一个错误**，也**不是**「在命令内部偷偷改跑 COUNT」——那是调用方的决定（前端拿到 `None` 要么保持「未知」，要么显式改用 `countStrategy: 'exact'` 重取）。

### 3.3 行数解析路径与并发结构的取舍（契约 §1.5 事实 8）

**今天的解析路径（必须原样保留在精确分支里）**：

```
count 结果的第一行
  → 第一个单元格
  → as_ref()（Option<&Value>）
  → 只接受 Value::Integer(i64) ⇒ Some(n)
  → 其余（字符串 / NULL / 大整数越界 / 首行缺失）⇒ None
```

这条路径是「三态」的现实依据：`total_rows: None` 在今天的语义里**歧义**——可能是「调用方说别数」，也可能是「数了但解析不出整数」。本册用 `total_rows_kind` 把这两件事分开（下表），并**不允许**顺手把解析放宽成「字符串也 `parse()` 一下」：那会改变既有行为，且会把驱动的异常值悄悄变成看似可信的数字。

| 本次响应 | `total_rows` | `total_rows_kind` | 前端行为 |
| --- | --- | --- | --- |
| 精确计数成功，但解析不出整数（非整数形态） | `None` | `Some(Exact)` | 显示「未知」——**数过了但没有可信数字**，比假装有值诚实 |
| 精确计数成功且解析出整数 | `Some(n)` | `Some(Exact)` | 显示 `from-to / n` |
| 估算成功且解析出整数 | `Some(n)` | `Some(Estimated)` | 显示 `from-to / ≈ <紧凑格式>` |
| 估算执行失败 / 值不可用 | `None` | `Some(Unsupported)` | 显示「未知」+ 可选提示；数据照常渲染 |
| 调用方要求复用缓存（`skip_count`） | `None` | `None` | **保留上一次的 kind 与数字**（今天的 `res.totalRows ?? current.totalRows` 语义） |
| 驱动声明 `Unsupported` 且不是「复用缓存」 | `None` | `Some(Unsupported)` | 显示「未知」；Next 由 `canPageForward` 决定 |

**并发结构：分支不同，结构不同，必须逐条落地**

| 分支 | 语句与并发结构 | 与今天的取舍关系 |
| --- | --- | --- |
| `Exact`（含所有未改动驱动的默认路径） | **`tokio::try_join!(count_sql, data_sql)`**，与今天**完全相同** | 完全不变：count 失败 ⇒ 整次加载失败（这是今天的行为，是否要改由 §11 Q5 裁定） |
| 估算前置条件不满足（驱动声明 `Unsupported` / 没有 `count_estimate_sql` / 有生效筛选 / 调用方要求跳过） | 走 `Exact` 或直接只发数据（不允许把估算硬塞进去） | 无额外往返：判定全部发生在**发语句之前** |
| `Estimated` 且前置条件满足 | **`tokio::join!(estimate_sql, data_sql)`** —— 仍然并发（保持「数据不因行数而等待」的既有优点），但**两条语句的结果各自独立处理**：数据失败照旧传播；估算失败只把 kind 降级为 `Unsupported` | **这是一处有意的取舍**：`try_join!` 会「估算失败 ⇒ 整页数据也没了」，对本册要修的问题（行数绑架可见性）恰好是最坏结果。因此估算分支**拆开 join**，且**不重跑 COUNT**（绝不引入第二次顺序往返）；用户想拿精确值就点一次 `Count`（一次显式请求） |
| 显式要求精确（用户点 `Count`） | 走 `Exact` 分支的 `try_join!` | 与今天一致；失败时如实报错并保留当前渲染 |

> **禁止**为了让估算「先看看值可不可用」而写成「先查估算、再查数据」的顺序结构：那会平白多一次往返，且直接违反契约 §1.5 事实 8 要求的「重新说明并发取舍」——本册的取舍就是**继续并发**。

### 3.4 TypeScript

```ts
// src/types/index.ts（新增 / 修改）
export type CountStrategy = 'exact' | 'estimated' | 'unsupported';   // 与 Rust CountStrategy 的 serde camelCase 形态一一对应

export interface TableDataResult {
  columns: ColumnSchema[];
  rows: (Value | null)[][];
  totalRows?: number;
  /** 新增：本次响应行数的可信度。缺省 = 本次没取行数，调用方保留上一次的值。 */
  totalRowsKind?: CountStrategy;
  page: number;
  pageSize: number;
}

export interface AppSettings {
  // …既有字段不动
  /** 新增。行数统计模式。默认 'auto'。 */
  rowCountMode: 'auto' | 'exact' | 'estimated' | 'none';
  /** 新增。'auto' 模式下超过此值改用估算。默认 200000。 */
  rowCountEstimateThreshold: number;
  /** 新增。默认表排序。默认 'primaryKeyAsc'（= 今天的行为）。 */
  defaultTableSort: 'primaryKeyAsc' | 'primaryKeyDesc' | 'none';
}
```

| 字段 | 缺省值 | 用途 |
| --- | --- | --- |
| `rowCountMode` | `'auto'` | `'auto'`：先看目录里已知的行数（`TableInfo.rowCount`），超过阈值才要估算；取不到该值就当 `'exact'`。`'exact'` / `'estimated'` / `'none'` 分别映射为 IPC 的 `countStrategy: exact / estimated / unsupported` |
| `rowCountEstimateThreshold` | `200000` | 只在 `'auto'` 下生效；`<= 0` 视为「一律精确」（防御脏数据） |
| `defaultTableSort` | `'primaryKeyAsc'` | `'primaryKeyAsc'` = **不传**任何排序入参，走驱动默认实现（对「列标志与 `primary_keys` 一致」的表与今天逐字节相同，见 §1.1）；`'none'` = 传 `defaultOrderColumns: []`；`'primaryKeyDesc'` = 走既有的显式排序通道（见 §4.4） |

```ts
// src/stores/tableData/types.ts（修改 TableState，只新增字段）
export interface TableState {
  // …既有字段（含 totalRows: number、sorts: SortCondition[]）一个字不动
  /** 新增。totalRows 的可信度；'unsupported' 时 totalRows 恒为 0 且不渲染总页数。 */
  totalRowsKind: CountStrategy;
  /** 新增。本面板对默认排序的临时覆盖（不持久化）。默认 'inherit'。 */
  defaultOrderOverride: 'inherit' | 'primaryKeyAsc' | 'primaryKeyDesc' | 'none';
}
```

```ts
// src/lib/totalRowsDisplay.ts（新增，纯函数，全部可单测）
import type { CountStrategy } from '../types';

export type TotalRowsView =
  | { kind: 'exact'; value: number }
  | { kind: 'estimated'; value: number }
  | { kind: 'unknown' };

/** 把 (kind, number) 归一成展示三态。kind 缺省按 'exact'（老后端行为不变）。 */
export function totalRowsView(kind: CountStrategy | undefined, totalRows: number): TotalRowsView;

/** 紧凑格式：1234 → '1,234'；1_200_000 → '1.2M'；0 → '0'。 */
export function formatRowCount(value: number): string;

/** 未知态是否允许翻页：只有「本页取满了」才说明后面还可能有行。 */
export function canPageForward(view: TotalRowsView, rowCount: number, pageSize: number): boolean;
```

```ts
// src/lib/databaseMeta.ts（新增接口 + DatabaseTypeMeta 新增可选字段）
// ⚠ 本块与契约 §7 逐字对齐；`editable` 的缺省值是 true（不是 false），
//   理由见契约 §7 的字段注释：R4 要求缺省值等于今天的语义，而今天是可编辑的。
export interface DataGridCapabilities {
  /**
   * 表级可写入。**默认 true**（等于今天的 `isEditable = !isConnectionReadOnly`）。
   * 只有显式写 `editable: false` 才关闭写入入口。
   *
   * ⚠ 本字段的缺省值曾经误写为 false，已更正：契约第 2 节 R4 规定「默认值必须等于今天的语义」，
   * 而今天在没有任何该声明时网格是**可编辑**的。若缺省为 false，所有未声明该能力的驱动
   * 会在一夜之间变成只读——那是严重回归，不是保守。
   */
  editable?: boolean;
  /** 插入后数据库能回填新行主键（对应 insert_returning_clause）。默认 false（今天没有插入能力）。 */
  insertReturning?: boolean;
  /** 行数策略；缺省视为 'exact'（与既有 `skip_count_query` 的缺省语义对齐）。 */
  countStrategy?: 'exact' | 'estimated' | 'unsupported';
  /**
   * 支持的筛选算子；缺省为**共享算子集的 19 个**（见第 5.2 节：既有 10 + 共享扩展 9）。
   * 方言专有的 3 个（Regex / NotRegex / JsonContains）**必须**由驱动显式声明才会出现在 UI。
   */
  filterOperators?: FilterOperator[];
  /** LIKE 在默认排序规则下大小写不敏感（仅影响文案，不改变 SQL）。默认 false。 */
  likeCaseInsensitive?: boolean;
}
// 注：`countStrategy` 上面写的是字面量联合（与契约 §7 逐字一致），而本册 §3 已导出 `export type CountStrategy = 'exact' | 'estimated' | 'unsupported'`。两者同形。**落地时二选一**：要么在 `databaseMeta.ts` 里 `import type { CountStrategy }` 后写 `countStrategy?: CountStrategy`，要么删掉 §3 的别名。**不要两处各定义一份**——契约冻结的是字段名与取值集合，不是「必须写成内联联合」。
export interface DatabaseTypeMeta {
  // …既有字段不动
  /** 新增。**字段名已由契约 §7 冻结为 `dataGrid`**；缺省时宿主按契约 §7 的缺省值行事（= 今天的体验）。 */
  dataGrid?: DataGridCapabilities;
}
```

```ts
// src/commands/database.ts（修改 getTableData 入参 + 新增一个命令）
getTableData: (params: {
  // …既有入参不动
  /** 新增。缺省 = 由后端按驱动声明决定。 */
  countStrategy?: CountStrategy;
  /** 新增。缺省 = 走后端驱动的 default_order_columns；空数组 = 显式要求不注入默认排序。 */
  defaultOrderColumns?: string[];
}) => Promise<TableDataResult>;

estimateTableRows: (
  dbSessionId: string,
  table: string,
  database?: string | null,
  schema?: string | null,
) => Promise<number | null>;
```

> **两层能力声明的关系**（契约 §7 已定，此处复述以免误用）：驱动 trait 方法是**权威**，前端 `dataGrid` 只是「提前知道」。二者不一致时以后端返回值与错误为准，并且**不允许**因为前端声明 `countStrategy: 'estimated'` 就跳过驱动检查。

---

## 4. 交互与状态机

### 4.1 行数三态的展示

| 态 | 触发条件 | 分页器左区 | 分页器右区 | 状态栏 |
| --- | --- | --- | --- | --- |
| **精确** | `totalRowsKind === 'exact'`（含缺省） | `1-50 / 1,234`（与今天同形） | `Page 1 / 25` + `Count` 按钮**不显示** | `1,234 rows` |
| **约 N 行** | `totalRowsKind === 'estimated'` | `1-50 / ≈ 1.2M`，整串带 `title`/`aria-label` = `t('pagination.estimateTooltip')` | `Page 1 / 24,000`（估算总页数）+ **`Count` 一次性按钮** | `≈ 1.2M rows` |
| **未知** | `totalRowsKind === 'unsupported'` | `1-50 / —` | `第 N 页`（**不渲染总页数**），Next 仅在 `canPageForward` 为真时可用 | 不渲染行数 |

三条硬性约束：

1. **估算值绝不允许与精确值同形渲染**：`≈` 是文案的一部分，不是 CSS 装饰；`formatRowCount` 的返回值**不含** `≈`，`≈` 由渲染层拼（这样测试可以精确断言数字格式化与符号两件事）。
2. **未知态不允许出现 `NaN` / `Infinity` / `Page 1 / 1`**：`totalPages` 相关计算在未知态一律不参与渲染（这是 PRD 明确的验收点）。
3. **未知态的 Next 按钮**以 `canPageForward(view, rows.length, pageSize)` 为准。今天的实现用 `totalPages = Math.max(1, ceil(totalRows / pageSize))` 且 `totalRows` 恒为 0，会让 Next **永久禁用** —— 对 `skip_count_query == true` 的驱动（今天树内没有，但 git 驱动里可能有）这是既有缺陷，本册必须一并修掉（A8）。

### 4.2 三态的状态跃迁

| 当前态 | 事件 | 下一个态 | 状态内行为 / 退出条件 |
| --- | --- | --- | --- |
| 未加载 | 面板首次加载 | 精确 / 估算 / 未知 | 由 `rowCountMode` + 驱动 `count_strategy()` 决定；`skip_count` 为真时响应不带 `totalRowsKind`，故**保持在未加载前记录的值**（首帧即 `'exact'`） |
| 精确 | 翻页 / 改页大小 / 点列头排序 | 精确（不变） | `skipCount: true`，响应不带 `totalRowsKind`，`commitFetchedPage` 保留旧值（今天的行为） |
| 未知 | 翻页 | 未知 | 不重跑计数；Next 由 `canPageForward` 决定 |
| 估算 | 点 `Count` 按钮 | 精确 | 发一次 `countStrategy: 'exact'` 的完整重载（**不是** `skipCount`）；成功后 `totalRowsKind = 'exact'`，按钮消失；失败则保持估算态并显示原始错误（走 `classifyGridError` 回退） |
| 估算 | 用户改筛选条件（Apply） | 精确 | 有生效筛选 ⇒ 后端按 §3.2 规则 4 强制降级为 `Exact`；前端不需要特判，**但要断言** |
| 估算 / 精确 | 用户切设置项到「不统计」 | 未知 | 下一次加载带 `countStrategy: 'unsupported'`；当前已渲染的数字**立即**停止参与总页数计算 |
| 任意 | 面板关闭 / 连接断开 | 随面板 slice 一起销毁 | `removePanel` 既有逻辑，本册不动 |

> **竞态**：所有三态写入都走既有的 `requestRevision` / `loadingRevision` 判定（`commitFetchedPage` 已经实现），**禁止**绕开它直接改 `totalRowsKind`。这是契约 §1.1 结论 2 的强制要求。

### 4.3 默认排序配置的入口与作用域

**建议（本册采用）：全局设置 + 面板临时覆盖，两层。**

| 层 | 位置 | 作用域 | 是否持久化 | 理由 |
| --- | --- | --- | --- | --- |
| 全局 | 设置页「表数据」分区：`Default table sort`（`primaryKeyAsc` / `primaryKeyDesc` / `none`）、`Row count mode`、`Estimate above` | 所有连接、所有表 | 是（走既有 `settingsCommands.saveSettings`） | 与既有的 `defaultPageSize` 完全同构，用户预期一致；零新增存储设施 |
| 面板临时 | 分页器旁的排序下拉：`Follow setting` / `Primary key ↑` / `Primary key ↓` / `No sort` | 当前标签页（`TableState.defaultOrderOverride`） | 否 | 「同一张表这次不想排序」是高频一次性诉求；持久化它需要按表偏好存储，而**今天不存在该设施**（§2.3 已核实），新建存储属于 D-9/C-42 的范围，不在本册 |

**为什么不选「连接级」**：同一连接下的表差异极大（日志表 vs 字典表），连接级会把「A 表要排序稳定、B 表要秒开」这两个诉求绑死；而且连接级作用域在今天的设置模型里只能落进 `connectionId` 的配置，会让「设置项」与「连接配置」两套持久化语义纠缠。

**为什么不做「表级持久化（本期）」**：没有存储设施，且它属于列宽/列顺序同一个「按表偏好」议题（D-9）。本册只留出 `defaultOrderOverride` 这个**内存态**钩子，将来接入持久化时只需把它的初值从 `'inherit'` 改为从偏好存储读。

### 4.4 与用户手动点击列头排序的优先级

```
显式排序（sorts 非空，来自点击列头）
  > 面板临时覆盖（defaultOrderOverride → 必要时转成 defaultOrderColumns / sorts）
    > 全局设置 defaultTableSort
      > 驱动 default_order_columns()（无主键时可能再退化到第一列）
        > pagination.order_by_fallback（仅当方言要求 ORDER BY 才用，如 T-SQL 的 (SELECT NULL)）
```

三条必须写进测试的行为：

1. **显式排序只取第一个元素**：`get_table_data_impl` 今天就是 `sorts.into_iter().next()`（§2.1）。因此「点列头」永远只发一列；`defaultOrderColumns` **只在 `sorts` 为空时**由前端发送（前端保证，后端不猜）。
2. **取消排序后必须回到默认排序**：`setSort` 的三态（升 / 降 / 取消）里，「取消」= `sorts` 变为空 ⇒ 下次加载重新走默认排序链。今天的 `setSort` 只会写入 `[sort]`，取消路径由调用方传空数组，本册不改变这条既有语义，但要补一条测试固定它。
3. **`primaryKeyDesc` 的实现方式**：契约给的是 `defaultOrderColumns: string[]`（**只有列名，没有方向**，与 `default_order_columns() -> Vec<String>` 逐字对应）。因此「主键降」**不复用** `defaultOrderColumns`，而是由前端在 `sorts` 为空时发一个显式的降序 `sorts: [{ column: <第一主键列>, descending: true }]`：
   - 单列主键：语义精确，`ORDER BY "id" DESC`，与今天显式排序走同一条 SQL 路径。
   - **复合主键：`sorts` 通道只能表达一列**（后端只取第一个元素），所以本册对复合主键的「主键降」**置灰**并给出说明文案，而不是静默降级成「只降第一列」。
   - 该限制的替代方案（让宿主消费全部 `sorts`）已列入第 11 节未决问题，请裁定。

### 4.5 「不排序」时的稳定性提示（不得静默）

`defaultTableSort === 'none'` 或面板覆盖为 `No sort` 时，分页器右侧必须常驻一条提示：`t('pagination.unstablePaging')`（悬停 / 直接可见均可），文案必须说清「未指定排序，翻页结果可能重复或遗漏」——这正是竞品厂商自己承认的权衡（PRD §4.12）。同时：

- T-SQL 这类**语法上必须**带 `ORDER BY` 的方言，仍会注入驱动给的 `order_by_fallback`（`(SELECT NULL)`，中性排序）。这**不违背**用户「不排序」的意图（它不指定任何业务顺序），但稳定性提示照常显示。
- **不得**因为「不排序」就跳过 `requires_order_by` 的 fallback —— 那会让 T-SQL 直接语法报错。

### 4.6 无主键 / 视图的显式化

无主键表今天**不失败**（PRD 把它列为 DataZen 的优势 C-58），本册**只加提示、不改成功路径**：

- 主键为空时 `default_order_columns()` 默认实现给出第一列 ⇒ 对「列标志与 `primary_keys` 一致」的表，SQL 与今天逐字节相同；
- 前端在 `columns.every(c => !c.isPrimaryKey) && defaultOrderOverride !== 'none'` 时显示 `t('defaultSort.noPrimaryKey')`，并提供两个出路：**按首列排序**（= 保持现状）与**自定义排序**（引导用户点列头）。两条出路都是既有能力，不新增数据通路；
- **已知的残留不精确（必须写进实现注释）**：前端只有 `columns` 上的 `is_primary_key` 标志，因此对「列标志缺失但 `primary_keys` 非空」的驱动（正是 §1.1 影响面里的那类表），会**误显示**「无主键、按首列排序」的提示，而实际 SQL 已按真实主键排序。本册**不为此新增 IPC 字段**（那会扩大 DTO 改动面），保留偏差并列入 §11 Q14 裁定；受影响范围与 §1.1 完全一致（树内 15 个 path 驱动不受影响）。

---

## 5. 实现步骤

> 每一步都写「改哪个文件 / 加什么符号 / 为什么 / 怎么自测」。**顺序不可调换**：Step 1~2 先把新契约的**选择逻辑**与今天对齐（有主键用主键、无主键退化第一列）并用测试钉住，Step 3 才允许宿主改**输入来源**（唯一的行为变更，§1.1），Step 4 之后才动行数通路。这是契约 §2 R2/R4 与 §5.4 合入条件的要求。

### Step 0（**已由 08 完成，本册只补用例**）：`query_executor/` 的测试模块

> **本 Step 在本册合并时已经不存在了。** `query_executor.rs → query_executor/{mod.rs,tests.rs}` 的目录化**归属冻结给 08**（契约 §9.1；提交序 01 → 02 → 03 → 05 → 06 → 04 → **08 → 09** → 10 → 07 → 11，08 必然先落地）。本册合并时该目录必然已存在，因此**不需要 Step 0**，只需确认 `mod.rs` 末尾的 `#[cfg(test)] mod tests;` 仍在，并往 `tests.rs` 补本册用例。下面保留搬迁形态的描述，仅供回读 08 的产出结构。

- **改哪个文件**：`src-tauri/src/services/query_executor.rs` → 目录模块 `src-tauri/src/services/query_executor/mod.rs` + 新增 `src-tauri/src/services/query_executor/tests.rs`；`src-tauri/src/services/mod.rs` 的 `mod query_executor;` 声明不变（目录模块对声明透明）。
- **加什么符号**：`mod.rs` 末尾保留 `#[cfg(test)] mod tests;`；测试体原样搬到 `tests.rs`，只把 `use super::*;` 改为 `use super::*;`（同层级引用，符号路径不变）。
- **为什么**：该文件当前 797 行，其中约 460 行是测试；本册在 `get_table_data` / `build_select_sql` 上净增约 30~40 行生产代码，不拆分**必然越过 800 行红线**（第 6 节有账）。
- **怎么自测**：`cargo test -p datazen --lib query_executor` 必须与原样全绿、用例数不变（这是纯搬运，任何一条由红转绿或消失都说明搬错了）。

### Step 1：契约层——类型 + 4 个带默认实现的方法 + `ReuseDriver` 转发 + mock 扩展

- **改哪个文件**：`packages/driver-api/src/types.rs`（加 `CountStrategy`、`TableDataResult.total_rows_kind`）、`packages/driver-api/src/traits.rs`（只加 4 个方法的声明与文档）、新增 `packages/driver-api/src/traits/browsing.rs`（4 个方法体的可复用实现）、`packages/driver-api/src/reuse.rs`（补 4 条转发）、`packages/driver-api/src/mock_driver.rs`（`MockDriverOptions` 新增 `skip_count_query: bool`、`count_estimate_rows: Option<i64>`，并在 `impl Default` 与 `impl DatabaseDriver for MockDriver` 里接通）。
- **加什么符号**：`CountStrategy`、`count_strategy`、`count_estimate_sql`、`default_order_columns`、`allow_first_column_fallback_order`、`traits::browsing::{count_strategy, default_order_columns}`、`MockDriverOptions::skip_count_query` / `::count_estimate_rows`。
- **为什么**：把「默认实现 = 今天」这件事**先**固化在契约层，后面宿主怎么改都不会破坏旧驱动；`ReuseDriver` 的转发是刚需（§2.2 结论：不转发会让 questdb / cloudberry / MySQL 别名的覆盖静默失效）；mock 扩展是为了让 `skip_count_query == true` 这条分支真的能被测到（今天没有任何树内驱动覆盖它，没有钩子就写不出测试）。
- **怎么自测**：
  - `cargo test -p datazen-driver-api`：新增 `count_strategy_defaults_to_exact_when_skip_count_query_is_false`、`count_strategy_defaults_to_unsupported_when_skip_count_query_is_true`、`reuse_driver_forwards_browsing_strategy_methods`。
  - `cargo build --workspace`：**所有驱动不改一行也必须编译通过**（这是 R2 的验收动作，比单测更有说服力）。

### Step 2：等价性 + 新语义测试（先写测试，允许先红）

- **改哪个文件**：`src-tauri/src/services/query_executor/tests.rs`（Step 0 拆出来的那份）。
- **加什么符号**：四条等价性用例 + 一条新语义用例（见 §9 A 组），外加一条「不一致时输出 `tracing::debug!`」的可观测性用例（可用既有 `tracing` 测试工具或直接断言纯函数返回的告警标记，避免为测试引入新依赖）。
- **为什么**：契约 §5.4 的合入条件 ② 明确要求「四条既有路径各一条测试 + 一条『列标志与 `primary_keys` 不一致时按 `primary_keys` 排序』的测试来钉住新语义」。先把**旧行为的期望 SQL 字符串**（`ORDER BY "id" ASC` / `ORDER BY "order_id" ASC, "product_id" ASC` / `ORDER BY "col_a" ASC`）写成断言，再在 Step 3 改造输入来源，才能证明「不是改完再补一个迎合实现的测试」。
- **怎么自测**：
  - Step 2 结束时：**四条等价性用例全绿**（Step 3 还没做，默认实现与旧逻辑都在），**新语义用例应当是红的**——它红着才说明它真的在钉新行为，而不是恒真。
  - Step 3 之后再跑：五条**全部绿**。任何一条由绿转红都说明改错了。

### Step 3：宿主把主键输入来源切换为 `CachedColumns.primary_keys`（**行为变更步骤**）

- **改哪个文件**：`src-tauri/src/services/query_executor/mod.rs`（`build_select_sql` 新增 `default_order: &[String]` 参数并删掉内部的 `is_primary_key` 推导；`get_table_data` 新增 `count_strategy`、`default_order_columns` 两个参数，并在开头用 `cached.primary_keys` 调 `resolve_default_order` 与 `resolve_count_strategy`，同时输出 §3.2 要求的不一致告警）、新增 `src-tauri/src/services/table_browsing.rs`、`src-tauri/src/services/mod.rs`（导出新模块）。
- **加什么符号**：`resolve_default_order`、`resolve_count_strategy`、`QueryExecutor::estimate_row_count`、`table_browsing::pk_sources_disagree`（或等价的告警判定函数）。
- **为什么**：这是本册的行为中枢——「谁来排」（宿主写死 → 驱动声明）与「主键从哪来」（列标志 → 归一化主键表）两件事都在这一步落地。把纯决策逻辑放在新文件里，`query_executor/mod.rs` 只留调用点（第 6 节有行数账）。
- **提交与说明（契约 §5.4 合入条件 ①，不可省略）**：提交信息必须显式写出这是一次行为变更，并在 PR 描述里附上影响面表（= §1.1 的表），包含「树内 15 个 path 驱动经核实列标志与 `primary_keys` 一致」「git 驱动不在树内、依靠 `tracing::debug!` 告警发现不一致」两条结论。**禁止**把这次变更描述成「纯重构，行为不变」。
- **怎么自测**：
  - Step 2 的五条用例全绿（**最重要**）；
  - `explicit_sort_overrides_default_pk_order` 仍绿，证明显式排序优先级没被打破；
  - `get_table_data_uses_schema_cache_and_returns_total` 仍断言 `query_calls() == 2`，证明没有多发/少发查询；
  - 额外自查：对「列标志齐全 + `primary_keys` 一致」的输入，Step 2 记录的四条旧 SQL 串**逐字未变**（这是「除声明外的行为不变」的机器证明）。

### Step 4：行数三态在 Host 落地（含 `skip_count` 与驱动 `Unsupported` 的分离、并发结构取舍）

- **改哪个文件**：`src-tauri/src/services/query_executor/mod.rs`（两个 `Ok(TableDataResult { .. })` 构造点补 `total_rows_kind`；精确分支保持 `tokio::try_join!` 不动，新增的估算分支按 §3.3 用 `tokio::join!` 并各自处理结果）、`src-tauri/src/commands/schema.rs`（`get_table_data_impl` 新增 `count_strategy: Option<CountStrategy>`、`default_order_columns: Option<Vec<String>>` 两个可选入参；`effective_skip_count` 拆成「调用方要求跳过」与「驱动不支持」两件事）、`src-tauri/src/bootstrap/run.rs`（注册新命令）。
- **加什么符号**：`get_table_data_impl` 的两个新参数、`TableDataResult.total_rows_kind` 的三个赋值点（精确 / 估算 / 不支持）、`estimate_table_rows_impl` + `estimate_table_rows`。
- **为什么**：契约 §6.2 要求 `getTableData` 新增这两个可选入参；`skip_count` 与 `Unsupported` 必须分开，否则「翻页复用缓存」会把三态错误地翻成「未知」（§4.2 的跃迁表要求二者语义不同）。并发结构的取舍必须在这里一次做对：**估算分支拆开 join、且估算失败不重跑 COUNT**（§3.3 的表）。
- **怎么自测**：
  - `cargo test -p datazen --lib` 全绿，且 `src-tauri/src/services/query_executor/tests.rs` 与 `src-tauri/src/commands/schema/tests.rs` 新增的用例覆盖三态与并发取舍；
  - **兼容性自查**：把 `count_strategy` 与 `default_order_columns` 都传 `None`、`skip_count` 传 `false`，SQL、响应形状与 `query_calls()` 计数必须与今天完全一致（用既有 `get_table_data_with_query_rows` 与 `get_table_data_uses_schema_cache_and_returns_total` 的断言兜住）。

### Step 5：驱动侧落地估算（先声明，再逐驱动覆盖）

- **改哪个文件**：`packages/drivers/postgres/src/postgres.rs`（覆盖 `count_strategy` → `Estimated`，实现 `count_estimate_sql`）、驱动内测试放 `packages/drivers/postgres/src/sql_text.rs` 或 `packages/drivers/postgres/tests/`（**驱动测试必须落在驱动 crate 内，禁止写进 Host**）、可选地在 `packages/drivers/postgres/ui/meta.ts` 声明 `dataGrid.countStrategy = 'estimated'`。
- **加什么符号**：`PostgresDriver::count_strategy`、`PostgresDriver::count_estimate_sql`。
- **为什么**：PG 的 `pg_class.reltuples` 是最标准的估算来源，且 PRD 明确点名。**顺序上必须最后做**：只有 Step 1~4 全部落地并把默认行为锁死之后，才允许有驱动真正改变行为；这样任何一个驱动覆盖出问题，回滚面只有一个 crate。
- **怎么自测**：
  - `cargo test -p datazen-driver-postgres` 全绿（新增用例断言估算 SQL 里出现了被引用的表名、`::regclass` 形态正确、单引号转义走 `format_sql_literal` 而不是手拼）；
  - 单独跑 `cargo test -p datazen --lib` 确认 Host 在没有 mock 覆盖声明时行为不变。
- **SQLite / 其他驱动本轮不动**：`count_strategy()` 默认已是 `Exact`，而 SQLite 的 `COUNT(*)` 本来就便宜，**保持零改动**就是正确实现。

### Step 6：前端类型、命令封装与驱动能力声明

- **改哪个文件**：`src/types/index.ts`（`CountStrategy`、`TableDataResult.totalRowsKind`、`AppSettings` 三个新字段）、`src/commands/database.ts`（`getTableData` 两个新入参 + `estimateTableRows`）、`src/lib/databaseMeta.ts`（`DataGridCapabilities` + `dataGrid?`）、`src/stores/settingsStore.ts`（`DEFAULT_SETTINGS` 三个默认值）。
- **加什么符号**：`CountStrategy`、`DataGridCapabilities`、`estimateTableRows`。
- **为什么**：先把类型与通道铺好，UI 才有东西可渲染；`DEFAULT_SETTINGS` 决定老用户升级后的行为是「与今天一致」。
- **怎么自测**：`pnpm typecheck` 干净；`npx vitest run src/stores/__tests__/settingsStore.test.ts` 绿（既有用例含 `defaultPageSize` 断言，能证明设置合并没被破坏）。

### Step 7：store 的三态写入（单一写入点）

- **改哪个文件**：`src/stores/tableData/types.ts`（`TableState` 两个新字段）、`src/stores/tableData/connectionState.ts`（`emptyTableState` 补 `totalRowsKind: 'exact'`、`defaultOrderOverride: 'inherit'`）、`src/stores/tableDataStore.ts`（`commitFetchedPage` 补 `totalRowsKind: res.totalRowsKind ?? current.totalRowsKind`；`loadTableData` 按 §3.3 的映射组装 `countStrategy` / `defaultOrderColumns`；`reloadPanel` 的 opts 增加 `countStrategy?: CountStrategy`）、新增 `src/lib/totalRowsDisplay.ts`。
- **加什么符号**：`TableState.totalRowsKind`、`TableState.defaultOrderOverride`、`reloadPanel` 的新可选参数、`totalRowsView` / `formatRowCount` / `canPageForward`。
- **为什么**：`commitFetchedPage` 是唯一写 `totalRows` 的地方（§2.3 已核实），把 `totalRowsKind` 也放在同一处，就天然满足「禁止两套真相」；而 `?? current` 的写法正是「响应不带 kind 时保留旧值」的实现。
- **怎么自测**：`npx vitest run src/stores/__tests__/tableDataStore.test.ts`，新增用例断言：估算响应写入 `'estimated'`、不带 kind 的响应保留旧值、`defaultOrderOverride === 'none'` 时请求体里 `defaultOrderColumns` 为 `[]`。

### Step 8：UI 三态 + 稳定性提示 + 精确统计入口

- **改哪个文件**：`src/components/DataTable/Pagination.tsx`（新增可选 props `totalRowsKind`、`hasMore`、`onCountExact`；未知态不渲染总页数；估算态渲染 `≈` + tooltip + `Count` 按钮；`none` 排序时渲染稳定性提示）、`src/components/DataTable/DataTable.tsx`（透传 `totalRowsKind` / `hasMore={rows.length === pageSize}` / `onCountExact`）、`src/windows/connection/TableView.tsx`（把 kind 与 `onCountExact` 接上；无主键提示；面板排序下拉）、`src/windows/connection/ContentStatusBar.tsx`（`totalRows` 之外新增可选 `totalRowsKind`，估算加 `≈`，未知不渲染行数）。
- **加什么符号**：`PaginationProps.totalRowsKind` / `.hasMore` / `.onCountExact`、`ContentStatusBarProps.totalRowsKind`、`TableView` 的面板覆盖下拉。
- **为什么**：能力声明（Step 6）与状态（Step 7）都就位后，UI 才可能「按能力渲染」而不是「按驱动名渲染」；`hasMore` 必须由 `rows.length === pageSize` 推出，这是未知态唯一可靠的翻页依据。
- **怎么自测**：`npx vitest run src/components/DataTable/__tests__/Pagination.test.tsx`（既有 9 条用例必须全绿，证明精确态的渲染没被改坏）+ 新增三态用例。

### Step 9：i18n 与最终门禁

- **改哪个文件**：`src/locales/en/query.ts`、`src/locales/en/settings.ts`（清单见第 8 节）。**不要**改 `src/locales/en.ts`，也不要新建域名。
- **加什么符号**：无（纯词条）。
- **为什么**：`locales.test.ts` 的字面量扫描会把「代码里 `t('x')` 而 en 没有 `x`」判为失败；先把 Step 8 的 `t()` 调用写出来，再按第 8 节清单补 key，一次补齐。
- **怎么自测**：`npx vitest run src/locales/locales.test.ts` 全绿；`node scripts/i18n-sync-check.mjs` **只在发布前**跑（开发期不动其他语言）。

---

## 6. 文件级改动清单

| 文件 | 新增 / 修改 | 职责 | 预估行数 | 是否触及 800 行上限 |
| --- | --- | --- | --- | --- |
| `packages/driver-api/src/types.rs` | 修改 | 新增 `CountStrategy`；`TableDataResult` 新增 `total_rows_kind` | +18 / -0 | **已超限（既有约 939 行）**；本次净增 18，不再往里加逻辑，且列为第 11 节待裁定 |
| `packages/driver-api/src/traits.rs` | 修改 | 4 个新方法的声明与文档（方法体不写在这里） | +40 / -0 | **已超限（既有约 1020 行）**；新方法体一律放 `traits/browsing.rs`，本文件只加签名（与既有 `pagination_syntax` 转发 `sql_text` 的做法一致） |
| `packages/driver-api/src/traits/browsing.rs` | **新增** | 4 个新方法的可复用实现（默认实现体） | ~50 | 否 |
| `packages/driver-api/src/traits.rs` 的 `mod` 声明 | 修改 | 声明 `mod browsing;` | +1 | — |
| `packages/driver-api/src/reuse.rs` | 修改 | 4 条转发（`count_strategy` / `count_estimate_sql` / `default_order_columns` / `allow_first_column_fallback_order`） | +24 | 否（既有约 1159 行，**已超限**；本次只补转发，属最小必要改动） |
| `packages/driver-api/src/mock_driver.rs` | 修改 | `MockDriverOptions` 新增 `skip_count_query`、`count_estimate_rows`，并在 `Default` 与 `impl DatabaseDriver` 接通 | +20 | 否 |
| ~~`src-tauri/src/services/query_executor.rs`~~ | **目录化归属冻结给 08**（契约 §9.1；提交序 08 早于 09） | 本册合并时 `query_executor/` 目录**必然已存在**，本册**不做搬迁**，只改 `mod.rs` 与 `tests.rs` | 0（搬迁由 08 完成） | **拆除红线风险**：现状 797 行，**由 08 完成**目录化后生产文件约 340 行，本册净增 30~40 后仍在 380 行以内 |
| `src-tauri/src/services/query_executor/mod.rs` | 修改 | `build_select_sql` 新增 `default_order` 参数并删除内部主键推导；`get_table_data` 新增两个参数、改用 `cached.primary_keys` 调 `resolve_default_order`（**行为变更，见 §1.1**）、接 `resolve_count_strategy`、补 `total_rows_kind`；新增 `estimate_row_count` 与估算分支的并发结构 | +55 / -25 | 否（约 370 行） |
| `src-tauri/src/services/query_executor/tests.rs` | 修改（**搬迁已由 08 完成**，本册只补用例） | 四条等价性用例 + 一条新语义用例 + 三态与并发用例 | 新增约 150（不含 08 搬出的约 460） | 否 |
| `src-tauri/src/services/table_browsing.rs` | **新增** | `resolve_count_strategy` / `resolve_default_order` 两个纯函数 + 单测 | ~120（含测试） | 否 |
| `src-tauri/src/services/mod.rs` | 修改 | 导出新模块 | +2 | 否 |
| `src-tauri/src/commands/schema.rs` | 修改 | `get_table_data_impl` 两个新可选入参；新增 `estimate_table_rows_impl` / `estimate_table_rows` | +70 | 否（约 455 行） |
| `src-tauri/src/commands/schema/tests.rs` | 修改 | 新增三态与估算命令的用例 | +120 | 否 |
| `src-tauri/src/bootstrap/run.rs` | 修改 | `generate_handler!` 注册 `estimate_table_rows` | +1 | 否 |
| `packages/drivers/postgres/src/postgres.rs` | 修改 | 覆盖 `count_strategy` / `count_estimate_sql` | +30 | 否（约 800+？**执行前必须 `wc -l` 确认**；若已接近上限，实现放 `packages/drivers/postgres/src/sql_text.rs` 并只留转发） |
| `packages/drivers/postgres/src/sql_text.rs`（或 `tests/`） | 修改 / 新增 | 估算 SQL 的拼装与转义测试 | +60 | 否 |
| `packages/drivers/postgres/ui/meta.ts` | 修改 | 声明 `dataGrid.countStrategy = 'estimated'` | +5 | 否 |
| `src/types/index.ts` | 修改 | `CountStrategy`、`TableDataResult.totalRowsKind`、`AppSettings` 三字段 | +25 | 否 |
| `src/commands/database.ts` | 修改 | `getTableData` 两个新入参 + `estimateTableRows` | +20 | 否（约 180 行） |
| `src/lib/databaseMeta.ts` | **修改（追加）** | `DataGridCapabilities` 与 `dataGrid?` 的**创建归属已冻结给 08**（提交序 08 早于 09，契约 §9.1）；本册合并时它们必然已存在，只追加 `countStrategy?` / `allowFirstColumnFallbackOrder?` 两项，**禁止整文件重写** | +22 | 否（合并时约 255 行） |
| `src/lib/totalRowsDisplay.ts` | **新增** | 三态归一、紧凑格式化、未知态翻页判定 | ~60 | 否 |
| `src/lib/gridErrors.ts` | **修改（追加）**（本册用到 `grid.count.*` 前缀） | 01 已建骨架；本册按契约 §8.1 三步追加 `grid.count.*` 码，**禁止整文件覆盖** | +10 | 否 |
| `src/stores/settingsStore.ts` | 修改 | `DEFAULT_SETTINGS` 三个默认值 | +4 | 否 |
| `src/stores/tableData/types.ts` | 修改 | `TableState` 两个新字段 | +6 | 否 |
| `src/stores/tableData/connectionState.ts` | 修改 | `emptyTableState` 两个新字段初值 | +2 | 否 |
| `src/stores/tableDataStore.ts` | 修改 | `commitFetchedPage` 补 kind；`loadTableData` 组装新入参；`reloadPanel` opts | +25 | **接近上限（现状 722 行，改后约 747）**：本次净增必须 ≤ 30 行；若超出，把 `commitFetchedPage` 的字段合成抽到 `src/stores/tableData/fetchedPage.ts` |
| `src/components/DataTable/Pagination.tsx` | 修改 | 三态渲染、稳定性提示、`Count` 按钮、未知态翻页 | +70 | 否（约 210 行） |
| `src/components/DataTable/DataTable.tsx` | 修改 | 透传 `totalRowsKind` / `hasMore` / `onCountExact` | +12 | **需 `wc -l` 确认**（该文件本身较大，若已接近 800 则把三态渲染完全留在 `Pagination` 内，只加 3 个 prop） |
| `src/windows/connection/TableView.tsx` | 修改 | 接线 + 无主键提示 + 面板排序下拉 | +60 | 否 |
| `src/windows/connection/ContentStatusBar.tsx` | 修改 | 可选 `totalRowsKind`，估算加 `≈` | +15 | 否 |
| `src/windows/settings/SettingsContent.tsx` | 修改 | 三个新设置项（复用既有 `SettingRow`） | +45 | 否 |
| `src/locales/en/query.ts` | 修改 | 第 8 节 `pagination.*` / `defaultSort.*` | +18 | 否 |
| `src/locales/en/settings.ts` | 修改 | 第 8 节 `settings.rowCount*` / `settings.defaultTableSort*` | +10 | 否 |
| `e2e/specs/table-count-and-order.ts` | **新增** | 无主键不失败、三态渲染、不排序提示 | ~120 | 否 |
| `src/lib/__tests__/totalRowsDisplay.test.ts` | **新增** | 纯函数用例 | ~70 | 否 |
| `src/components/DataTable/__tests__/Pagination.countKind.test.tsx` | **新增** | 三态组件用例（避免把既有 `Pagination.test.tsx` 撑大） | ~110 | 否 |

> **800 行红线的结论**：本册有 **两处**必须处理的既有超限/临界文件 —— `src-tauri/src/services/query_executor.rs`（797，**目录化拆测试已由 08 完成**，本册合并时生产文件约 340 行，净增 30~40 后仍有余量）与 `src/stores/tableDataStore.ts`（722，净增需节制）。`packages/driver-api/src/traits.rs`（1020）与 `types.rs`（939）**已经超限**，本册按契约强制位置只做最小净增，并把「是否拆分这两个文件」列入第 11 节。

---

## 7. 边界与异常清单

| # | 场景 | 期望行为 |
| --- | --- | --- |
| B1 | **无主键表** | 排序：`primary_keys` 为空 ⇒ `default_order_columns()` 默认实现给第一列 ⇒ 对「两来源一致」的表 SQL 与今天逐字节相同（`ORDER BY "col_a" ASC`）；**浏览路径不得失败**（竞品 #651 的硬失败点）；UI 显示 `defaultSort.noPrimaryKey` 并提供两条出路（按首列 / 自定义）。驱动可覆盖 `allow_first_column_fallback_order() -> false` 得到「无排序 + 稳定性提示」 |
| B2 | **复合主键表** | 排序：按 `CachedColumns.primary_keys`（= 归一化后的真实主键）的顺序全部主键列升序（`ORDER BY "a" ASC, "b" ASC`）。**注意这就是 §1.1 的行为变更点**：如果该驱动的列标志与 `primary_keys` 不一致，列序可能随之变化（今天按列定义序、新语义按主键表顺序）——这类表必须在 PR 描述里点名，并靠 §3.2 的 `tracing::debug!` 告警可发现。设置项「主键降」在复合主键上置灰（§4.4） |
| B2b | **`primary_keys` 非空但列上没有 `is_primary_key` 标志** | 新语义：按真实主键排序（`ORDER BY <primary_keys...> ASC`）；旧行为：**找不到主键 ⇒ 退化成第一列**。这是本次**唯一会改变 SQL** 的场景，必须有专门测试（§9 A 组第五条）。修正后这类表也不再展示「无主键、按首列排序」的提示 |
| B3 | **视图 / 外部表** | `get_columns` 对视图通常没有主键标记 ⇒ 落 B1/B2b 的路径（此时 `primary_keys` 也为空，两条来源一致，无行为变更）。估算：`count_estimate_sql` 对视图可能返回的语句执行失败 ⇒ 按 §3.3 的估算分支处理：**不重跑 COUNT**、返回数据 + `totalRowsKind = 'unsupported'`，并 `tracing::warn!` 记录降级原因 |
| B4 | **估算值为 `NULL` / 非整数 / 负数** | 一律视为「不可用」，但**两条路径的处理不同**：① `get_table_data` 的估算分支（执行期）⇒ 返回数据 + `totalRowsKind = 'unsupported'`，**不重跑 COUNT**（§3.3）；② 独立命令 `estimate_table_rows` ⇒ `Ok(None)`，由调用方决定是否改走精确。`0` 是**合法**估计（空表），必须显示 `≈ 0` 而不是当成不可用。两种情况下都**绝不**渲染 `≈ -1 行` |
| B5 | **估算值小于实际行数** | UI 必须带 `≈`；末页可能为空 ⇒ 该页显示空表 + `pagination.estimatedLessThanActual` 提示，且**不得**报错、不得把用户自动弹回第一页。注意 `Pagination` 既有的 `current = Math.min(page, totalPages - 1)` 会在估算变小后的下一次渲染把页码钳回，这条既有行为不得引入新的死循环（用「按用户请求的 page 拉数据、只用 kind 决定渲染」的方式绕开） |
| B6 | **`skip_count_query() == true` 的驱动** | `count_strategy()` 默认推出 `Unsupported` ⇒ 不发任何计数查询、`total_rows: None`、前端显示「未知」；**Next 按钮必须依据 `rows.length === pageSize` 判断**（今天因 `totalPages = max(1, ceil(0/pageSize)) = 1` 会永久禁用，这是必须一并修掉的既有缺陷，对应 A8） |
| B7 | **只读连接**（`savedConnection.readOnly` / 驱动 `readOnly`） | 行数统计与排序都是**读路径**，一律照常工作；`isEditable` 只控制写入入口，**不得**用它隐藏行数、排序下拉或 `Count` 按钮 |
| B8 | **超大数据表** | `'exact'` 模式仍会跑 `COUNT(*)`（用户显式选择，必须尊重）；`'estimated'` 模式首屏不跑 COUNT；`'auto'` 模式的阈值判断只能依据**已经拿到的** `TableInfo.rowCount`，取不到（`null`）就当 `'exact'`，**禁止**为了判断阈值而先跑一次 COUNT（那就是本末倒置） |
| B9 | **count 查询超时 / 失败** | 今天 `tokio::try_join!` 任一失败 ⇒ 整次加载失败（页面上是错误）。**精确分支本册保持这一行为不变**（是否整体降级见 §11 Q5）。**估算分支则明确不同**：估算失败不得带走数据（§3.3），返回数据 + `totalRowsKind = 'unsupported'` + `tracing::warn!` |
| B9b | **行数解析得到非整数形态**（字符串 / `NULL` / 大整数越界 / 首行缺失） | 即使计数查询执行成功，也把 `total_rows` 置为 `None`、kind 保持 `Some(Exact)`（「数过了但没有可信数字」）；**禁止**用 `parse()` 之类的宽松转换把它变成看似可信的数字（那会改变既有解析语义） |
| B10 | **用户翻到超过实际行数的页** | 请求照发（后端返回空行集）；UI 显示空表 + `pagination.estimatedLessThanActual`（估算态）或既有的 `dataTable.empty`（精确态），并把「精确统计」按钮放在同一视野内；**不得**报错或自动跳页 |
| B11 | **有生效筛选** | 估算**不可用** ⇒ 强制 `Exact`（§3.2 规则 4）。同时 count 与 select 必须共用同一份 `filters` 与 `filter_logic`（今天已经如此：`build_count_sql` 与 `build_select_sql` 共用 `format_condition` / `filter_is_complete` / `filter_join`），本册不得引入第二套筛选归一 |
| B12 | **显式排序与默认排序同时存在** | 显式排序（`sorts` 非空）胜出，且不得同时出现两段 `ORDER BY`（`build_select_sql` 的分支结构保证） |
| B13 | **`pagination.requires_order_by` 为真的方言（T-SQL）** | 「不排序」时仍必须注入驱动给的 `order_by_fallback`（`(SELECT NULL)`），否则语法报错；稳定性提示照常显示 |
| B14 | **`countStrategy: 'estimated'` 但驱动没有估算语句** | 宿主静默降级为 `Exact` 并正常返回；**不得**报错、不得返回 `unsupported`（用户要的是行数，不是错误） |
| B15 | **老后端 / 老驱动不返回 `totalRowsKind`** | 前端按 `'exact'` 处理，渲染与今天完全一致（R4：缺省 = 今天） |
| B16 | **`rowCountEstimateThreshold <= 0` 或非有限数** | 视为「一律精确」，不让脏设置把界面推进估算态 |

---

## 8. i18n key 清单

**落点（已按契约 §8 的修正核实）**：

- 分页器、网格内提示、排序稳定性提示 → **`src/locales/en/query.ts`**（既有 `pagination.*` / `dataTable.*` / `tableView.*` 都在这里）；
- 设置页的三个新设置项 → **`src/locales/en/settings.ts`**（既有 `settings.defaultPageSize` 就在这里，同构就近）；
- **不要**改 `src/locales/en.ts`（只有一行再导出）、**不要**新建域名、**不要**动其他语言文件；
- `src/locales/en/index.ts` 自动合并各领域包，新增 key 不需要改它。

| key（完整路径） | 文件 | 英文文案 | 用途 |
| --- | --- | --- | --- |
| `pagination.rowCountEstimated` | `src/locales/en/query.ts` | `≈ {count}` | 分页器左区的估算行数（`≈` 必须在这里，不能由 CSS 伪装） |
| `pagination.rowCountUnknown` | `src/locales/en/query.ts` | `—` | 未知态的占位符（不显示总页数、不显示 `NaN`） |
| `pagination.estimateTooltip` | `src/locales/en/query.ts` | `Estimated row count — the real number may differ. Use Count for the exact value.` | 估算值的 `title` / `aria-label`，必须说清「可能不精确」 |
| `pagination.countExact` | `src/locales/en/query.ts` | `Count` | 估算态下的一次性「精确统计」按钮（对齐竞品点击即精确的交互） |
| `pagination.counting` | `src/locales/en/query.ts` | `Counting…` | 精确统计进行中的按钮文案（同时驱动 `loading` 禁用态） |
| `pagination.pageOnly` | `src/locales/en/query.ts` | `Page {page}` | 未知态的分页标签（无总页数），`{page}` 沿用既有 `{count}` 式占位符风格 |
| `pagination.unstablePaging` | `src/locales/en/query.ts` | `No sort order is set — rows across pages may repeat or be skipped.` | 「不排序」时的常驻稳定性提示（PRD §4.12 明确要求不得静默） |
| `pagination.estimatedLessThanActual` | `src/locales/en/query.ts` | `This page is empty. The real row count may be lower than the estimate — use Count to check.` | 估算小于实际、末页为空时的提示 |
| `pagination.rowsUnknown` | `src/locales/en/query.ts` | `Row count unavailable` | 状态栏未知态文案（`ContentStatusBar` 在 `totalRows > 0` 之外的新分支） |
| `defaultSort.noPrimaryKey` | `src/locales/en/query.ts` | `No primary key — ordering by the first column.` | 无主键表的显式化（DB-16），把今天的隐式行为讲出来 |
| `defaultSort.custom` | `src/locales/en/query.ts` | `Custom order` | 无主键提示里的第二条出路（引导用户点列头） |
| `defaultSort.panelLabel` | `src/locales/en/query.ts` | `Sort` | 分页器旁的面板级排序下拉标签 |
| `defaultSort.inherit` | `src/locales/en/query.ts` | `Follow setting` | 面板下拉的默认项（`defaultOrderOverride === 'inherit'`） |
| `defaultSort.primaryKeyAsc` | `src/locales/en/query.ts` | `Primary key ↑` | 面板下拉项 |
| `defaultSort.primaryKeyDesc` | `src/locales/en/query.ts` | `Primary key ↓` | 面板下拉项；复合主键时置灰 |
| `defaultSort.primaryKeyDescUnavailable` | `src/locales/en/query.ts` | `Descending by a composite primary key is not supported yet.` | 复合主键上「主键降」置灰的原因说明（禁止只置灰不解释） |
| `defaultSort.none` | `src/locales/en/query.ts` | `Do not sort` | 面板下拉项 |
| `settings.rowCountMode` | `src/locales/en/settings.ts` | `Row count mode` | 设置项标签 |
| `settings.rowCountMode.auto` | `src/locales/en/settings.ts` | `Automatic` | 选项：超过阈值才估算 |
| `settings.rowCountMode.exact` | `src/locales/en/settings.ts` | `Always exact` | 选项：恒定 `COUNT(*)` |
| `settings.rowCountMode.estimated` | `src/locales/en/settings.ts` | `Estimate when possible` | 选项：能估算就估算 |
| `settings.rowCountMode.none` | `src/locales/en/settings.ts` | `Do not count` | 选项：完全不数（分页器显示页码） |
| `settings.rowCountEstimateThreshold` | `src/locales/en/settings.ts` | `Estimate above this row count` | 阈值输入框标签（仅 `auto` 生效） |
| `settings.defaultTableSort` | `src/locales/en/settings.ts` | `Default table sort` | 设置项标签 |
| `settings.defaultTableSort.primaryKeyAsc` | `src/locales/en/settings.ts` | `Primary key, ascending` | 默认值（= 今天的行为） |
| `settings.defaultTableSort.primaryKeyDesc` | `src/locales/en/settings.ts` | `Primary key, descending` | 选项 |
| `settings.defaultTableSort.none` | `src/locales/en/settings.ts` | `Do not sort` | 选项；选中后网格必须出现稳定性提示 |
| `settings.defaultTableSort.hint` | `src/locales/en/settings.ts` | `“Do not sort” makes paging faster on large tables, but rows across pages may repeat or be skipped.` | 设置项说明（把权衡写在设置里，而不是只写在网格上） |
| `grid.count.estimateUnavailable` | `src/locales/en/query.ts` | `This database cannot estimate row counts.` | 契约 §8 的 `grid.<域>.<原因>` 前缀前缀族；用于「用户显式要求估算但驱动明确不支持」时的一次性提示（后端返回的错误消息以同名前缀开头） |

**两条纪律**：

1. **每一条 `t('…')` 调用都必须在 en 里存在**，否则 `src/locales/locales.test.ts` 的 `resolves every literal t() key used in host source (en dictionary)` 直接失败。新增 key 后**必须**跑一次 `npx vitest run src/locales/locales.test.ts`。
2. 契约 §8 要求「新增的、需前端精确识别的错误，消息以 `grid.<域>.<原因>: ` 开头」。本册涉及的只有 `grid.count.estimateUnavailable` 一条；前端用新增的 `classifyGridError` 做**前缀**匹配（`src/lib/gridErrors.ts`，新增），匹配不到必须**回退显示原始消息**。

---

## 9. 测试清单

### 9.1 Rust 单元测试

**A 组：排序路径（本册最重要；前五条 = 契约 §5.4 合入条件 ② 的硬性要求）— `src-tauri/src/services/query_executor/tests.rs`**

> 「等价」的**前提**是「该驱动的列标志与 `primary_keys` 一致」；下面前四条等价性用例全部按这个前提构造输入（两来源一致），第五条专门构造不一致的输入来钉新语义。

| 用例名 | 断言要点 |
| --- | --- |
| `default_order_columns_equals_legacy_single_pk_injection`（合入条件②-1） | 输入：`columns = [id(pk 标志 + primary_keys = ["id"]), name]` ⇒ `resolve_default_order` + `build_select_sql(..., default_order = <结果>)` 产出的 SQL **逐字包含** `ORDER BY "id" ASC`，与旧行为期望串完全相等 |
| `default_order_columns_equals_legacy_no_pk_first_column_fallback`（合入条件②-2） | 输入：无主键（`primary_keys` 为空且列上无标志）⇒ `ORDER BY "col_a" ASC`（第一列兜底不退化、不消失） |
| `default_order_columns_equals_legacy_composite_pk_injection`（合入条件②-3） | 输入：`columns = [order_id(pk), product_id(pk), quantity]`、`primary_keys = ["order_id","product_id"]` ⇒ `ORDER BY "order_id" ASC, "product_id" ASC` |
| `explicit_sort_still_overrides_default_order`（合入条件②-4） | 显式 `name DESC` 时**不得**出现 `"id" ASC`；也不得出现两段 `ORDER BY`（对齐既有 `explicit_sort_overrides_default_pk_order`） |
| **`default_order_uses_primary_keys_when_column_flags_disagree`**（合入条件②-**新增**，钉住 §1.1） | 输入：`columns = [col_a(无 pk 标志), col_b(无 pk 标志)]`、`primary_keys = ["id"]`（列里甚至没有叫 `id` 的列）⇒ 产出 `ORDER BY "id" ASC`，且**不含** `ORDER BY "col_a" ASC`；反向再构造一次（列上有标志、`primary_keys` 为空 ⇒ 走第一列兜底）也要各有一条断言 |
| `pk_source_disagreement_is_reported` | 构造不一致输入时，`table_browsing::pk_sources_disagree` 返回真（纯函数断言），保证 §3.2 要求的可观测告警**真的会被触发**，而不是写了没人调 |
| `default_order_columns_equals_legacy_empty_columns_emits_no_order` | `columns` 为空且 `primary_keys` 为空 ⇒ 不含 `ORDER BY`；若 `pagination.requires_order_by` 为真则出现驱动的 `order_by_fallback` |
| `build_select_sql_is_unchanged_when_no_new_inputs_are_passed` | 用同一组「两来源一致」的输入跑新代码，SQL 与 Step 2 记录的旧串完全相等（**「除声明外行为不变」的机器证明**） |

**B 组：契约默认实现（A2）— `packages/driver-api/src/traits/browsing.rs` 与 `reuse.rs`**

| 用例名 | 断言要点 |
| --- | --- |
| `count_strategy_defaults_to_exact_when_skip_count_query_is_false` | 默认实现返回 `CountStrategy::Exact` |
| `count_strategy_defaults_to_unsupported_when_skip_count_query_is_true` | `MockDriverOptions { skip_count_query: true, .. }` ⇒ `Unsupported`（**这条用例是全册兼容性论证的核心**） |
| `count_estimate_sql_defaults_to_none` | 默认返回 `None` |
| `default_order_columns_defaults_prefer_the_primary_keys_argument` | 默认实现：传入 `columns = [a(无标志), b(无标志)]` 与 `primary_keys = ["id"]` ⇒ 返回 `["id"]`（**在契约层钉住「`primary_keys` 参数优先」**，与宿主侧 A 组第五条互为双保险） |
| `default_order_is_empty_when_columns_are_empty` | `default_order_columns(&[], &[])` 为空数组（不 panic、不造列名） |
| `allow_first_column_fallback_order_defaults_to_true` | 默认 `true` |
| `reuse_driver_forwards_browsing_strategy_methods` | 包裹一个「估算 + 主键降 + 有估算语句」的假驱动，断言 4 个方法**全部**转发到 inner（照既有 `reuse_driver_forwards_structure_methods` 的写法） |

**C 组：三态执行与降级 — `src-tauri/src/services/table_browsing.rs` + `query_executor/tests.rs`**

| 用例名 | 断言要点 |
| --- | --- |
| `estimated_strategy_runs_estimate_sql_and_reports_kind` | 驱动声明 `Estimated` + `count_estimate_sql` 返回 SQL ⇒ `total_rows = Some(<mock 估计值>)`、`total_rows_kind = Some(Estimated)` |
| `estimated_branch_keeps_data_when_estimate_query_fails`（A9） | 估算 SQL 执行报错 ⇒ **仍返回行数据**、kind = `Some(Unsupported)`、`query_calls() == 2`（证明没有第二次往返、没有重跑 COUNT） |
| `estimated_branch_runs_both_statements_concurrently`（A9） | 断言两条语句在 `tokio::join!` 形态下都被下发（用 mock 的调用计数与「两条 SQL 都到达」来证明；**不得**退化成「先估算再取数据」的顺序形态） |
| `exact_branch_still_propagates_count_failure` | 精确分支沿用今天的 `try_join!`：count 报错 ⇒ 整次加载返回 `Err`（固定「本册不改精确分支失败语义」这条边界） |
| `total_rows_is_none_when_count_cell_is_not_an_integer`（B9b） | count 结果首格是字符串 / `NULL` / 越界整数 ⇒ `total_rows = None` **且** `total_rows_kind = Some(Exact)`（「数过了但没有可信数字」） |
| `estimated_downgrades_to_exact_when_effective_filters_present` | 有完整筛选条件 ⇒ 下发的是 `SELECT COUNT(*)`（断言 mock 收到的 SQL 前缀）且 kind = `Exact` |
| `estimated_downgrades_to_exact_when_driver_has_no_estimate_sql` | 声明 `Estimated` 但 `count_estimate_sql` 为 `None` ⇒ 走 count，kind = `Exact`，**不报错** |
| `estimated_branch_reports_unsupported_when_estimate_value_is_unusable` | 估计语句返回 `NULL` / `-1` / 非整数 ⇒ **执行期**降级：kind = `Some(Unsupported)`、数据照常、`query_calls() == 2`（**不重跑 COUNT**，与 §3.3 一致；返回 `0` 时**接受** `0` 并给 `Estimated`） |
| `unsupported_driver_returns_no_total_and_unsupported_kind` | `skip_count_query == true` ⇒ `total_rows = None`、kind = `Some(Unsupported)`、`query_calls() == 1` |
| `caller_skip_count_returns_absent_kind_and_no_extra_query` | `skip_count == true` ⇒ kind = `None`、`query_calls() == 1`（响应形状与查询次数与今天同形；`MockDriverOptions.count_total` 不得被读到） |
| `exact_strategy_still_issues_count_and_select_concurrently` | 默认驱动 ⇒ `query_calls() == 2`、`total_rows = Some(count_total)`（对齐既有 `get_table_data_uses_schema_cache_and_returns_total`） |
| `default_order_override_empty_array_emits_no_order_by` | `default_order_columns = Some(vec![])` ⇒ SQL 无 `ORDER BY`（LIMIT/OFFSET 方言） |
| `default_order_override_columns_are_emitted_ascending` | `Some(vec!["a","b"])` ⇒ `ORDER BY "a" ASC, "b" ASC` |
| `first_column_fallback_gate_is_honoured`（`table_browsing` 内） | 无主键 + `allow_first_column_fallback_order() == false` ⇒ 返回空数组；为 `true` ⇒ 返回第一列 |

**D 组：IPC 层 — `src-tauri/src/commands/schema/tests.rs`**

| 用例名 | 断言要点 |
| --- | --- |
| `get_table_data_forwards_count_strategy_and_default_order_columns` | 显式传 `count_strategy = Some(Unsupported)` ⇒ 响应 `total_rows_kind == Some(Unsupported)`；传 `default_order_columns = Some(vec![])` ⇒ mock 收到的数据 SQL 不含 `ORDER BY` |
| `get_table_data_defaults_keep_legacy_shape` | 两个新入参都传 `None` ⇒ 与既有 `get_table_data_with_query_rows` 的断言完全一致（**兼容性回归**） |
| `estimate_table_rows_returns_none_when_driver_cannot_estimate` | `count_estimate_sql` 为 `None` ⇒ `Ok(None)`；`query_calls()` 不增加 |
| `estimate_table_rows_returns_value_when_driver_estimates` | mock 估计值 ⇒ `Ok(Some(n))` |
| `estimate_table_rows_returns_none_for_unusable_estimate_value` | 估计结果 `NULL` / 负数 / 非整数 ⇒ `Ok(None)`（**不是** `Err`，也**不**在命令内部改跑 COUNT —— 那会让「取估计值」这个命令偷偷做重活） |
| `estimate_table_rows_requires_live_session` | 未连接 ⇒ `Err(CommandError::…)`（对齐既有 `schema_commands_error_when_not_connected` 的写法） |

**E 组：驱动内（PostgreSQL，测试必须落在 `packages/drivers/postgres/`）**

| 用例名 | 断言要点 |
| --- | --- |
| `count_estimate_sql_quotes_the_table_itself` | 传入原始表名 `orders` ⇒ 驱动产出的 SQL 里出现 `pg_class` 与经 `format_sql_literal` 转义的字面量；**表名的引用由驱动自行完成**，**不得**出现裸的未转义 `'` |
| `count_estimate_sql_escapes_single_quotes` | 传入含单引号的表名（恶意输入）⇒ 断言单引号被转义（`''`），不产生注入面 |
| `count_estimate_sql_targets_the_requested_database` | 传入 `database = "other_db"` 而会话默认库不是它 ⇒ 断言估算语句绑定的是**本次请求的库**，而不是 `DATABASE()` / `DB_NAME()` 所在的库。这是本次签名变更要解决的核心问题，**必须有这条用例** |
| `count_estimate_sql_default_returns_none_for_any_target` | 默认实现无论传什么 `database` / `schema` / `table` 都返回 `None`（即"我没有估算手段"），保证未实现的驱动行为不变 |
| `postgres_declares_estimated_strategy` | `driver.count_strategy() == CountStrategy::Estimated`（覆盖后仍满足契约形态） |

### 9.2 Host 前端单元测试

| 文件 | 用例名 | 断言要点 |
| --- | --- | --- |
| `src/lib/__tests__/totalRowsDisplay.test.ts`（新增） | `totalRowsView_defaults_to_exact_when_kind_is_undefined` | 老后端（无 kind）⇒ `{ kind: 'exact', value }` |
| | `totalRowsView_maps_estimated_and_unsupported` | 三态映射；`unsupported` 时**不读** value |
| | `formatRowCount_compacts_large_numbers` | `1234 → '1,234'`、`1_200_000 → '1.2M'`、`0 → '0'`；**不得**返回 `NaN`/`Infinity` |
| | `canPageForward_only_when_page_is_full` | 未知态：`rows.length === pageSize` 才为真；精确态恒由总数决定 |
| `src/components/DataTable/__tests__/Pagination.countKind.test.tsx`（新增） | `renders_exact_range_without_tilde` | 精确态渲染 `1-50 / 1,234`，**不含** `≈` |
| | `renders_estimated_with_tilde_and_tooltip` | 估算态含 `≈`，且存在 `title`/`aria-label` = `estimateTooltip` |
| | `exposes_count_button_only_for_estimated` | 精确态无 `Count` 按钮；估算态点击触发 `onCountExact` |
| | `unknown_state_hides_total_pages_and_enables_next_when_has_more` | 未知态不出现 `Page 1 / 1`；`hasMore === true` 时 Next 可用（A8） |
| | `unknown_state_disables_next_when_page_is_not_full` | `hasMore === false` 时 Next 禁用 |
| | `shows_unstable_paging_notice_when_sort_is_none` | 「不排序」⇒ 提示文案可见 |
| `src/components/DataTable/__tests__/Pagination.test.tsx`（既有，**必须全绿不改断言**） | 既有 9 条 | 证明精确态渲染与分页行为零回归 |
| `src/stores/__tests__/tableDataStore.test.ts`（修改） | `commitFetchedPage_stores_total_rows_kind` | 估算响应写入 `'estimated'` |
| | `keeps_previous_total_rows_kind_when_response_omits_it` | `skipCount` 响应不覆盖旧 kind（`setPage triggers reload with skipCount` 的邻居） |
| | `none_override_sends_empty_default_order_columns` | `defaultOrderOverride = 'none'` ⇒ 请求体含 `defaultOrderColumns: []` |
| | `estimated_request_is_sent_only_for_estimated_mode` | `rowCountMode = 'estimated'` ⇒ `countStrategy: 'estimated'`；`'exact'` ⇒ `'exact'`；`'auto'` 且无目录行数 ⇒ 不下发该字段 |
| | `setSort_still_skips_count`（既有，保持） | 显式排序路径不重跑 COUNT |
| `src/stores/__tests__/settingsStore.test.ts`（修改） | `new_settings_default_to_legacy_behaviour` | `rowCountMode === 'auto'`、`defaultTableSort === 'primaryKeyAsc'`、阈值 `200000`；老配置加载后同样取默认 |
| `src/locales/locales.test.ts`（既有，**必须全绿**） | `resolves every literal t() key used in host source (en dictionary)` | 第 8 节的 key 全部存在 |

### 9.3 E2E（`e2e/specs/table-count-and-order.ts`，新增）

| 用例 | 步骤与断言 |
| --- | --- |
| 无主键表可浏览（**回归，不得 skip**） | 打开无主键表 → 断言行区域非空、无双击报错横幅、出现 `defaultSort.noPrimaryKey` 提示（对齐 PRD 的 `e2e/specs/table-no-pk.ts` 设想；本册把它并入本 spec，避免新增两条回归线） |
| 精确态 | 默认设置下打开小表 → 分页器显示 `a-b / N`，**不含** `≈`，无 `Count` 按钮 |
| 估算态（需要 PG 目录统计可用） | 把 `rowCountMode` 切成「Estimate when possible」→ 分页器出现 `≈`，`title` 存在；点 `Count` → 变为精确值且按钮消失 |
| 不排序 | 设置改成 `Do not sort` → 网格出现稳定性提示；翻到第 2 页仍能返回数据（不被禁用） |
| 行数未知 | `rowCountMode = 'Do not count'` → 分页器显示 `Page N`，不出现总页数，也不出现 `NaN` |
| 复合主键 | 打开复合主键表 → 表头排序指示器与默认排序一致；「主键降」下拉项为置灰态并可见原因文案 |

---

## 10. 自查清单

> 实习生最容易做错的 15 条。

1. **又从列标志里重新推导主键**：这是本册最容易犯、也最难被门禁发现的错。新的输入是 `CachedColumns.primary_keys`（已归一化），**不是** `columns.iter().filter(|c| c.is_primary_key)`（契约 §1.6 明确要求「凡是需要主键列的地方都应复用 `effective_primary_keys()`，不要自己过滤 `is_primary_key`」）。写错不会编译失败，只会让「填了 `primary_keys` 却没打列标志」的表继续退化成按第一列排序。
2. **把这次行为变更说成「纯重构」**：它**是**行为变更（§1.1）。必须显式声明、在 PR 描述里附影响面表、并加 `tracing::debug!` 告警；说成「逐字节等价」会让评审放过一个真实的行为改变。
3. **忘了「无主键退化为第一列」**：只写「有主键就排主键」，于是无主键表从 `ORDER BY col_a ASC` 变成无排序 —— 这是**行为回归**，且会让 T-SQL 直接语法报错。默认实现里那三行 `columns.first()` 是契约的一部分，不许删。
4. **忘了 `allow_first_column_fallback_order` 的门**：驱动把它设成 `false` 却在宿主侧没生效（或反过来，宿主把原本的兜底也一并关掉）。`true` 必须得到与今天一致的 SQL。
5. **把 `skip_count_query` 与新策略写成两套真相**：正确做法是「`count_strategy()` 默认实现由 `skip_count_query()` 推出」，**不是**在 Host 里同时判断两者、也不是把 `skip_count_query` 删掉。宿主只需要把「调用方要求跳过（`skip_count`）」与「驱动声明不支持」区分开（一个返回 `kind = None`，一个返回 `kind = Some(Unsupported)`）。
6. **把估算分支写成顺序结构或让它带走数据**：`try_join!(estimate, data)` 会让「估算失败 ⇒ 整页没数据」，正是本册要修的毛病；应当是 `join!` 且估算失败只降级 kind、**不重跑 COUNT**（§3.3）。同时**不得**把精确分支的 `try_join!` 也顺手改掉。
7. **count 与 select 的筛选条件不一致**：新增估算/计数路径时另写一套「有没有筛选」的判断，导致行数与实际数据对不上（`build_count_sql` 与 `build_select_sql` 必须共用 `format_condition` / `filter_is_complete` / `filter_join`）。
8. **有筛选时还去用估算**：估算语句是表级目录统计，套不上 `WHERE`，会把「筛选后 3 行」渲染成「约 120 万行」。必须强制降级 `Exact`（§3.2 规则 4 / A7）。
9. **忘了 `ReuseDriver` 转发**：新方法只加在 `DatabaseDriver` 上，于是 questdb / cloudberry / MySQL 别名的覆盖全部静默失效，且**编译不报错、测试也不红**。必须同时在 `reuse.rs` 补转发并写 `reuse_driver_forwards_*` 测试。
10. **在宿主里按数据库名分支**：比如 `if driver_type == "postgres" { 用 reltuples }`。这是本项目的头号红线。估算只能来自驱动的 `count_estimate_sql()`，方言差异只能通过 trait 方法或驱动元数据表达。
11. **改 DTO 的既有字段**：把 `TableDataResult.total_rows` 改成 `{kind, value}`、或把 `TableState.totalRows` 从 `number` 换成对象 —— 前者违反契约 R1，后者会一口气打断 `Pagination` / `ContentStatusBar` / `DataExportDialog` / `TableView` / 大量既有测试。正确做法是**只新增** `total_rows_kind` / `totalRowsKind`，数字字段原样保留。
12. **让未知态看起来像「1 / 1 页」**：`totalPages = Math.max(1, Math.ceil(0 / pageSize))` 会让未知态显示 `Page 1 / 1` 并把 Next 永久禁用（B6/A8）。未知态必须走独立的渲染分支与 `canPageForward`。
13. **在前端以为多列排序已经可用**：`sorts` 在 IPC 边界就被 `into_iter().next()` 截断（契约 §1.5 事实 7）。任何「发多列 `sorts` 就有多列排序」的假设都是错的。
14. **忘记 `en` 词条导致 i18n 门禁红**：`locales.test.ts` 会扫描 `src/` 下所有字面量 `t('…')`。新增 key 必须落在 `src/locales/en/query.ts` 或 `src/locales/en/settings.ts`，不能只写在组件里。
15. **为了「让测试过」而改既有断言**：`no_explicit_sort_composite_pk_uses_all_pk_columns`、`no_pk_no_explicit_sort_still_has_order_by_first_column`、`explicit_sort_overrides_default_pk_order`、`get_table_data_uses_schema_cache_and_returns_total` 这四条是**旧行为的证据**，只能把它们的输入改造成新签名（多传一个 `default_order` 参数），**断言字符串一个字都不许改**。改动断言等于把回归证据销毁。

---

## 11. 未决问题

| # | 问题 | 建议选项与理由 |
| --- | --- | --- |
| Q1 | **`defaultTableSort` 的持久化位置与作用域** | **建议：全局设置（`AppSettings.defaultTableSort`，走既有 `settingsCommands`）+ 面板内存态临时覆盖（`TableState.defaultOrderOverride`），本册不做按表持久化。** 理由：与既有 `defaultPageSize` 同构、零新增存储设施；而按表持久化需要新的偏好存储（今天完全不存在，已 grep 核实），属列宽/列顺序同一议题（D-9/C-42）。备选：连接级（会掩盖同连接下不同表的巨大差异，不推荐） |
| Q2 | **复合主键上的「主键降」怎么办** | 本册采用「置灰 + 说明」。（A）接受置灰，等 C-41 多列排序落地（**推荐**，本册零额外 API）；（B）本册顺手让宿主消费**全部** `sorts`（今天前端只写 0/1 个元素，因此对现有调用方是纯超集），代价是把 `QueryExecutor::get_table_data` 的 `order_by: Option<OrderBy>` 改成多列形态，需要确认这不违反契约 R1（该参数是宿主内部类型，但同名的 `get_table_data` 出现在 R1 的名单里，需裁定） |
| Q3 | **`DatabaseTypeMeta` 上新能力字段的字段名** | 契约 §7 给了 `DataGridCapabilities` 接口，但没定宿主上的字段名。**建议 `dataGrid?: DataGridCapabilities`**（与既有 `kvWorkspace` 的命名风格一致：能力域 + Capabilities）。其他分册（01~08、10）如果也要读这些能力，必须统一用同一个字段名，**请在此处一次裁定** |
| Q4 | **`DataGridCapabilities` 与后端 trait 冲突时的 UI 行为** | 契约 §7 已定「以后端返回值与错误为准」。建议**再补一条**：当驱动声明 `countStrategy: 'estimated'`（前端元数据）而后端返回 `Unsupported` 时，UI 必须静默按 `Unsupported` 渲染并**不弹错**（元数据只是「提前知道」）。请确认这不算「能力声明不一致需要报警」的场景 |
| Q5 | **精确分支的 count 失败是否也降级为「未知行数」**（估算分支已在 §3.3 定案，不再是问题） | （A）保持今天：`tokio::try_join!` 失败 ⇒ 整次加载失败（**最保守，本册默认**）；（B）**建议**：精确分支的 count 失败也降级 ⇒ `totalRowsKind = 'unsupported'` + 数据照常渲染 + `tracing::warn!`，理由是「三态」存在的意义就是让行数不再绑架数据可见性。这是明确的行为变更（**会改变今天的错误语义**），**需要人工裁定后才实现**（B9） |
| Q6 | **`estimate_table_rows` 是否真的需要独立 IPC** | 契约 §6.1 已定该命令存在。本册把它的定位收窄为「不重新取页地刷新估计值 / 供 `auto` 之外的显式刷新按钮使用」。（A）**建议**：保留命令，但首版只有「面板估算过期后刷新」一处调用，避免与 `getTableData` 的估算路径出现两份实现——两条路径必须共用 `QueryExecutor::estimate_row_count`；（B）若无人调用则推迟到有需求时再加（但这会与契约冲突，需先改契约）。请裁定是否接受「命令先落地、调用点暂只有一处」 |
| Q7 | ~~**估算语句缺 `database` 上下文**~~ **已裁定并已修契约** | 原签名 `count_estimate_sql(&self, quoted_table)` 确实缺库上下文（MySQL `information_schema.TABLES`、SQL Server `sys.dm_db_partition_stats` 都无法安全实现）。**契约册 §5.3 已改为 `count_estimate_sql(&self, database: &str, schema: Option<&str>, table: &str)`：传未引用的原始表名 + 显式库/schema，引用由驱动自己做**（与 `get_columns` / `get_table_schema` 的既有惯例一致）。**因此本册的落地范围不再限于 PostgreSQL**：MySQL / SQL Server 在有 `DATABASE()` 歧义顾虑时，现在可以用显式库名安全实现估算。落地顺序仍建议先只做 PostgreSQL（`pg_class.reltuples`），把方言估算作为后续增量；但**不再需要**把「估算能否实现」立为契约修订项 |
| Q8 | **`driver-api` 的 `types.rs`（约 939 行）与 `traits.rs`（约 1020 行）已超 800 行红线** | 本册按契约强制位置只做最小净增（`types.rs` +18；`traits.rs` 只加签名，方法体放新增 `traits/browsing.rs`）。**建议：本册不动它们的既有内容**，把「是否拆分这两个文件」立为独立的存量治理项。（A）**推荐**：本次豁免，仅记录；（B）借本册一并拆分 `types.rs` 为 `types/{core,ddl,transfer}.rs`——改动面覆盖全仓所有 `use datazen_driver_api::...`，风险与收益不成比例 |
| Q9 | **`grid.count.estimateUnavailable` 的触发时机** | （A）**建议**：只在「用户显式要求估算（`rowCountMode = 'estimated'` 或点了估算刷新）而驱动 `count_estimate_sql` 为 `None`」时提示一次；（B）`auto` 模式下驱动不支持估算是**预期内**的降级，静默退回精确即可，不提示。请确认这个二分 |
| Q10 | **未知态下 `Page N` 的 N 从哪来** | 未知态没有总数，`page` 只能来自面板自身的请求页码（`TableState.page + 1`）。**建议**：直接显示 `TableState.page + 1`，并在用户翻到空页时按 B10 处理。备选：显示「第 N 页（未知总数）」，但会与 `pagination.pageOnly` 的文案取舍绑定，请一并裁定用哪种措辞 |
| Q11 | **§1.1 的行为变更要不要给用户一个可见提示** | 变更只影响「`primary_keys` 非空但列未打标志」的表（树内 15 个 path 驱动均不受影响，见 §1.1）。（A）**建议**：只在 PR 描述与 `tracing::debug!` 里交代，UI 不提示 —— 因为对绝大多数用户它只是「排序终于对了」，提示反而制造困惑；（B）跟随更新日志给一句「无主键判定改为以真实主键为准」。请裁定是否需要面向用户披露 |
| Q12 | **估算分支与精确分支的失败语义不一致，是否接受** | 本册的取舍是「精确分支保持 `try_join!`（行数失败 ⇒ 整页失败，与今天一致），估算分支 `join!`（估算失败 ⇒ 降级为未知、数据照常）」（§3.3）。（A）**建议**：接受这个不对称，因为估算分支的全部意义就是「行数不得绑架数据可见性」，而精确分支改语义属于独立议题（见 Q5）；（B）要求两者一致 —— 那么要么把精确分支也改成降级（= Q5 选 B，行为变更面更大），要么把估算分支改成 `try_join!`（等于白做）。请裁定 |
| Q13 | **`primary_keys` 与列标志不一致的运行时告警级别与开关** | 本册定为 `tracing::debug!`（不刷屏、可上报时开启）。（A）**建议**：保持 `debug`，并在 PR 描述里给出查看方法；（B）改为 `warn!`（更易发现，但会在整类驱动上持续刷日志）。请裁定 |
| Q14 | **「无主键」提示对不一致驱动的误报怎么处理**（§4.6 的残留不精确） | （A）**建议**：本册接受误报，只写实现注释 + 本条目，等驱动侧补齐列标志后自然消失；（B）让宿主在返回的列上**按 `cached.primary_keys` 回填 `is_primary_key` 标志** —— 提示会变准，但同一份标志同时被写路径消费（`commitFetchedPage` 用它算 `pkColumns`、`TableView` 用它判编辑能力），等于顺手改变了「这些表能不能写」的行为，**风险显著大于收益，不建议在本册做**；（C）在 `TableDataResult` 上再加一个输出字段（如 `defaultOrderColumnsUsed: string[]`）让前端精确显示 —— 干净但扩大了 IPC 输出契约，需要先改契约册。请裁定选哪一个 |
