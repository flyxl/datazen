# 11 · 键集分页（DB-18）

> **状态**：目标设计，**未实现**。本文描述的是「加完这些代码之后系统应该长什么样」，当前 `main` 上没有 `supports_keyset_pagination`、没有 `keyset_predicate_sql`、没有 `src/lib/keysetPaging.ts`、没有 `grid.paging.*` 错误码。凡本文出现的符号，都标注了「既有」或「新增」，未标注即视为读错。
>
> **依赖**：[00-contracts.md](00-contracts.md)（**契约源，尤其第 1.5 / 1.6 / 5.4 / 5.5 / 8 节**）、[09-count-and-order.md](09-count-and-order.md)（稳定全序与默认排序策略；该分册**已存在**，且它的「不包含」小节显式把键集分页划归本册，依赖方向是**单向**的：本册依赖它，它不依赖本册）。本册**不重新定义**契约；若要偏离，只能先改 00，再改本册，最后改代码。
>
> **被谁依赖**：本册处于依赖图叶子（[总纲](../data-browsing-design.md) §4.1），没有任何分册依赖它。但 PRD 的 C-47（手填 Offset 跳页）、C-48（深翻页性能）与 DB-09 的「不排序即翻页不稳定」提示都以本册结论为准；[12-testing.md](12-testing.md) 的 journey 模板可直接引用本册第 9 节的键集旅程用例。
>
> **预估**：5~6 人日（净编码 + 测试，不含评审与返工），拆成 4 个可独立合并的提交（见第 5 节）。
>
> **本文档不包含什么**：不讲「为什么该做 DB-18」（那是 [PRD](../data-browsing-optimization-prd.md) §4.12 / §7.4 的职责）；不含无限滚动（DataZen 明确选择了分页而非无限滚动）；不含游标在 SQL 编辑器 / 查询结果网格 / MCP 上的复用（结果网格走 `execute_driver_command`，见 [10-result-grid.md](10-result-grid.md)）；不实现反向 seek（`ORDER BY` 反向 + 结果倒序），本册用有界游标栈替代，理由见第 11 节 K-5；不修改任何既有方法签名。

---

## 1. 目标与验收口径

**一句话目标**：在**不改任何既有方法签名、不改任何既有驱动行为**的前提下，让「有稳定全序的表 + 支持该能力的驱动」翻页时用 `WHERE <keyset 条件>`（seek）替代 `OFFSET`，并把键集的**前置条件、降级路径、游标失效规则**写死成可测试的不变量。

### 1.1 验收标准（可执行）

| # | 命令 | 期望输出（逐字） |
| --- | --- | --- |
| A1 | `cargo test -p datazen --lib` | `test result: ok.`，且本文第 9 节列出的既有分页/排序用例（`tsql_pagination_emits_offset_fetch_and_never_limit`、`no_explicit_sort_uses_primary_key_order`、`no_explicit_sort_composite_pk_uses_all_pk_columns`、`explicit_sort_overrides_default_pk_order`、`no_pk_no_explicit_sort_still_has_order_by_first_column`）**一个都没被删、没被改名、仍然通过** |
| A2 | `cargo test -p datazen-driver-api --lib` | `test result: ok.`，含「默认实现全 false / 全 None」的兼容性用例 |
| A3 | `cargo test -p datazen-driver-postgres` / `-p datazen-driver-mysql` / `-p datazen-driver-sqlite` / `-p datazen-driver-sqlserver` | 各 `test result: ok.`；四个 crate 都在 workspace `members` 内，**即使本工作树的驱动集不含 sqlserver 也能 `-p` 指定测试** |
| A4 | `npx vitest run` | `Test Files` 全通过；新增 `keysetPaging` 纯函数用例与 store 旅程用例在其中 |
| A5 | `pnpm typecheck` | 无 `error TS`；测试文件参与检查，mock 只用精确断言（`satisfies` / `as unknown as X`），**不得出现 `any`** |
| A6 | `pnpm e2e:skip-build`（或 `pnpm e2e:minimal`） | `e2e/specs/table-keyset-paging.ts` 通过：连翻两页无重复行；按可空列排序时出现降级提示且仍能翻页 |
| A7 | `pnpm e2e:contract:matrix` | 分页 journey 在各驱动上顺序翻页不重复；**结论必须写明本次构建实际链接的驱动集** |

### 1.2 口径（不写清等于没验收）

1. **驱动集必须写进结论**：本工作树当前以 `postgres` / `mysql` / `sqlite` / `redis` 铺设（`.driver-features.json` 的 `variant` 为 `custom`），CI 常以 `--drivers=all` 跑。Host 侧的键集用例**不得依赖已注册的驱动**，必须像既有 `tsql_syntax` 那样手工构造 `PaginationSyntax` / 用 `datazen_driver_api::mock_driver::MockDriver` 注入能力位——这样结论与驱动集解耦。
2. **无键集能力的驱动行为不变（本册最硬的口径）**：任何不覆盖新方法的驱动，`supports_keyset_pagination()` 必须返回 `false`、`keyset_predicate_sql(...)` 必须返回 `None`，宿主因此**永远不构造游标条件**，其 `SELECT` 文本必须与今天**逐字节相同**。这条由第 9 节的黄金 SQL 回归用例逐字断言，不靠人工比对。
3. **降级不是错误**：能力缺失 / 排序不构成全序 / 可空键列 / 游标失效，一律**静默走 OFFSET 并在 UI 上给出可读原因**，不得抛错、不得显示半页数据、不得复用一个已失效的游标。
4. **不允许第三种分页语义**：只有「OFFSET 分页」与「键集分页」两种；「按页码推算游标」这种第三态在本册里被明确禁止（见第 4 节的决策 J）。
5. **`CommandError`**：只有真正需要前端分类的失败才升错误码，前缀 `grid.paging.<原因>: <说明>`；生产路径禁止裸 `unwrap()` / `expect()`（见 `docs/development/panic-policy.md`）。

---

## 2. 现状代码事实

> 本节只列**亲自读过**的符号，位置一律「文件路径 + 符号名」，不写行号。发现与 00 契约不一致处，以代码为准并记在本节末尾。

### 2.1 SQL 生成与分页子句的确切拼装逻辑（既有）

位置：`src-tauri/src/services/query_executor.rs`。

- `QueryExecutor::get_table_data` —— 真实签名比 00 契约第 1.5 节的摘录**多一个参数**：结尾还有 `filter_logic: Option<&str>`。它先经 `SchemaCache::get_columns` 取 `CachedColumns`，再调 `Self::build_select_sql`，分页子句来自 `driver.pagination_syntax(page_size as u64, (page as u64) * (page_size as u64))` —— **offset 就是「页码 × 每页条数」，这是 OFFSET 分页的定义式，也是键集要替换的那一项**。
- 计数与数据是两次查询：`skip_count == true` 时只发 `SELECT` 且 `total_rows: None`；否则 `tokio::try_join!` 并发发 `build_count_sql` 与 `build_select_sql`，`total_rows` 由 COUNT 结果首行首列取 `Value::Integer`。
- `QueryExecutor::build_select_sql` 的确切顺序是：`SELECT <引用过的列名 或 *>` → `FROM <表>` → 有筛选则 ` WHERE ` + 各条件（`format_condition` 产出空串的**被过滤掉**）→ **排序** → **分页子句**。排序分支：
  - `order_by: Some(OrderBy)` → `ORDER BY <col> ASC|DESC`，`has_order_by = true`；
  - `order_by: None` → 取 `columns` 中 `is_primary_key == true` 的列，**全 ASC** 拼 `ORDER BY a ASC, b ASC`；若无主键则退化为**第一列** ASC；若 `columns` 为空（`SELECT *` 路径）则不加排序、`has_order_by` 保持 `false`；
  - 仍无排序且 `pagination.requires_order_by` 为真时，才追加 `pagination.order_by_fallback`（SQL Server 给的是 `(SELECT NULL)`）；
  - 最后 `if !pagination.clause.is_empty()` 原样追加 `pagination.clause`——**宿主不自己拼 `LIMIT`**。
- `QueryExecutor::build_count_sql` 第一行就丢弃入参 `columns`，只发 `SELECT COUNT(*) FROM t WHERE ...`。
- `QueryExecutor::format_condition` 只渲染 10 个算子（`Eq` / `Ne` / `Gt` / `Lt` / `Gte` / `Lte` / `Like` / `In` / `IsNull` / `IsNotNull`），值经 `driver.format_sql_literal` **内联为字面量**；`filter_is_complete` 会静默丢弃不完整筛选；`filter_join` 只有全局单一 `AND` / `OR`。
- 文件末尾 `#[cfg(test)] mod tests` 里的既有语义锚点（**键集接入后必须继续成立**）：`tsql_pagination_emits_offset_fetch_and_never_limit`、`tsql_pagination_keeps_pk_order_without_fallback`、`no_explicit_sort_uses_primary_key_order`、`no_explicit_sort_composite_pk_uses_all_pk_columns`、`explicit_sort_overrides_default_pk_order`、`no_pk_no_explicit_sort_still_has_order_by_first_column`、`build_select_sql_without_offset_when_unsupported`。测试用的 `PaginationSyntax` 由本地 helper `limit_syntax` / `limit_only_syntax` / `tsql_syntax` 手工构造。
- 该文件**已经 797 行**（`wc -l`），逼近 800 行上限：键集逻辑**不能**直接加在这里（见第 6 节拆分方案）。

### 2.2 分页子句为什么属于驱动（既有）

- `packages/driver-api/src/traits/sql_text.rs::pagination_syntax`（默认实现体）：`supports_offset()` 为真 → `LIMIT {limit} OFFSET {offset}`，为假 → **只有 `LIMIT {limit}`**；`requires_order_by: false`、`order_by_fallback: None`。
- `packages/driver-api/src/traits.rs`：`fn supports_offset(&self) -> bool { true }`（注释明确「不支持 OFFSET 的方言（如 Presto/Hive via Superset）返回 false」）、`fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax`（注释明确「宿主对每条分页读都走这个方法，因此没有 `LIMIT`（SQL Server）或需要 `ORDER BY` 才能 `OFFSET` 的方言覆盖它，而不是宿主按驱动名特判」）。
- `packages/driver-api/src/types.rs::PaginationSyntax` 三个字段的**确切含义**（读注释得出）：`clause` 是「追加在排序子句之后、不带前导空格」的片段，为空表示该方言根本无法分页；`requires_order_by` 表示 `clause` **只在语句已有 `ORDER BY` 时合法**；`order_by_fallback` 是「调用方没有自然排序列时方言接受的表达式」，为 `None` 时**调用方不得自己发明一个**——因为凭空造的 `ORDER BY` 会改变一页返回哪些行。
- `packages/driver-api/src/reuse.rs` 的 `ReuseDriver` **显式转发**这两个方法（`supports_offset`、`pagination_syntax`），并有 `reuse_driver_forwards_*` 系列测试做「转发遗漏」守卫。**新增能力位必须在这里同步转发，否则线级复用驱动会永远拿到默认值。**

**我读了哪几个驱动对 `pagination_syntax` 的覆盖**（全仓 grep `fn pagination_syntax` / `fn supports_offset`）：

| 驱动 | `pagination_syntax` | `supports_offset` | 说明 |
| --- | --- | --- | --- |
| `packages/drivers/sqlserver/src/sqlserver.rs` | **唯一覆盖者**：`OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY`，`requires_order_by: true`，`order_by_fallback: Some("(SELECT NULL)")` | 未覆盖（默认 `true`） | T-SQL 无 `LIMIT`；`OFFSET … FETCH` 必须有 `ORDER BY` |
| `packages/drivers/sqlserver/src/sqlserver/tests.rs` | `pagination_syntax_is_offset_fetch_and_never_limit` + `assert!(driver.supports_offset())` | — | 驱动自己钉死了这条契约 |
| `packages/drivers/{postgres,mysql,sqlite,clickhouse,duckdb,rqlite,turso,mongodb,redis,elasticsearch,influxdb,vector,victoriametrics,hbase}/src` | **全部未覆盖**，走 `sql_text::pagination_syntax` 默认体 | 全部未覆盖（默认 `true`） | 即 SQL 表格浏览路径统一是 `LIMIT n OFFSET m` + 宿主注入的 `ORDER BY <主键> ASC` |
| `packages/drivers/http-support/tests/support/real_driver_contract_{probe,plumbing,refusal,free}.rs` | 契约探针：`flip(supports_offset)` 等变异守卫 | 覆盖仅出现在**测试支撑代码** | 「假驱动不转发能力位就必须红」的机器守卫 |

> **由此得到的两个事实**：
> 1. ClickHouse 正是 PRD §4.12 里 85.75 秒那条根因的主体形状——`packages/drivers/clickhouse/src` 没有任何分页覆盖，宿主给它的是 `SELECT ... ORDER BY <主键> ASC LIMIT n OFFSET m`。**DB-18 不是优化项，是在拆一条已确认会炸的负债。**
> 2. 今天**没有任何 in-tree path 驱动**覆盖 `supports_offset()`。一旦某个驱动返回 `false`，默认 `pagination_syntax` 会吐出不含 `OFFSET` 的 `LIMIT n`，于是 `page > 0` 的请求会**重复返回第一页**。键集恰好是这类驱动的正解（`OFFSET` 恒为 0），本册因此要求它们优先开启键集能力位。

### 2.3 宿主数据通路的事实

- `src-tauri/src/commands/schema.rs::get_table_data_impl` 的入参是 `sorts: Option<Vec<SortCondition>>`，但**只取第一个元素**：`sorts.and_then(|list| list.into_iter().next())` → `OrderBy { column, descending }`。**多列排序目前到不了 SQL**（DB-11 的职责），所以键集在 v1 只能看见「单列显式排序 或 默认主键排序」这两种序。
- `src-tauri/src/commands/schema.rs::get_table_data`（`#[tauri::command]`）是 `get_table_data_impl` 的唯一 IPC 入口；全仓没有 MCP 或其它调用方（`get_table_data_impl` 的引用只在 `commands/schema.rs` 与其测试里）。
- `src-tauri/src/cache/schema_cache.rs::SchemaCache::get_columns` 返回 `CachedColumns { columns, primary_keys, table_name, cached_at }`，注释明写「**Lightweight cached columns (no indexes / foreign keys)**」。`CachedColumns.primary_keys` 来自 `driver.get_columns(...)`（返回 `(columns, primary_keys)`）或全量 schema 缓存里的 `TableSchema::effective_primary_keys()`。
- ⚠ **同一份主键在浏览路径上有两个来源**：`build_select_sql` 用的是 `columns.iter().filter(|c| c.is_primary_key)`，而缓存里的 `cached.primary_keys` **根本没被它用**。00 契约第 1.6 节要求「凡需要主键列的地方复用 `TableSchema::effective_primary_keys()`，不要自己过滤 `is_primary_key`」。这是既有漂移，本册的稳定性判定必须**沿用产出 `ORDER BY` 的那一份**，不得引入第三种口径（见未决问题 K-9）。
- `packages/driver-api/src/types.rs::TableDataResult { columns, rows, total_rows, page, page_size }`：全仓**只有宿主**构造它（`query_executor.rs` 两处），驱动通过 `QueryResult` 返回行。
- `packages/driver-api/src/types.rs::Value` 是 `#[serde(untagged)]` 枚举（`Null` / `Bool` / `Integer` / `Float` / `String` / `Bytes` / `Timestamp` / `Json`）。**这条对游标至关重要**：untagged 下 `[1,2,3]` 这类数组既能反序列化成 `Bytes` 又能成为 `Json`，字符串既是 `String` 也可能是 `Timestamp`——**游标值不能靠 untagged 的 `Value` 过线**（见第 3 节的 `CursorValue` 与未决问题 K-6）。
- `packages/driver-api/src/traits.rs::query_at` 的默认实现是「`qualified_sql` 重写后调 `query`」——**浏览路径没有任何绑定参数**。筛选值今天也是内联字面量。键集条件因此必须内联（见未决问题 K-6）。
- `packages/driver-api/src/lib.rs`：`mod traits;` 是私有模块，`pub mod filters;` / `pub mod sync;` 是公开模块——**驱动要能调用共享实现，必须放在公开模块里**（这是新增 `pub mod keyset;` 的理由）。

### 2.4 既有键集实现（Data Sync，可参考但**不复用**）

- `packages/data-sync/src/keyset.rs`：`build_keyset_select_sql` / `_with_order` / `_with_order_and_filter` / `_with_order_filter_and_pagination` / `keyset_seek_parameter_count`。它已经把键集分页的坑踩明白了，**必须读**：
  - 组合键的 seek 用**可移植的字典序析取**（`(a > ?) OR (a = ? AND b > ?)`），源码注释直接写明理由：「T-SQL has no row-value comparison syntax」；
  - 占位符按 flat 顺序生成，参数顺序与 `ORDER BY` 顺序严格一致（`keyset_seek_parameter_count` 是 `n*(n+1)/2`）；
  - 键顺序表达式由驱动提供（`packages/driver-api/src/sync/adapter.rs::sync_key_order_expression`，注释：「用于 `ORDER BY` 与 seek 谓词的 SQL 表达式必须与 `normalize_sync_key` 的排序一致」，例如 SQLite 的 `CAST(text_key AS BLOB)` 处理文本排序）；
  - `pk_columns` 为空、`after_key` 长度与键数不符、分页子句为空，都是**显式错误**而不是降级。
- `src-tauri/src/commands/sync/keyset_source.rs::DriverKeysetSource` 是它的宿主调用方。
- **为什么不直接复用**：(1) Data Sync 的 key 全 ASC，键集分页要支持混合方向（`ORDER BY a ASC, b DESC`）；(2) Data Sync 全程**参数化**，而网格浏览走的是**无参数的 `query_at`**；(3) Data Sync 的错误是 `DataSyncError`，网格要的是「安静降级 + 可选错误码」。**可以复用的是「策略与算法」，不是函数**——本册把可移植算法以公开 helper 形式提供（第 3.4 节），并要求驱动侧行为与 Data Sync 保持一致（同一个方言，同一套 seek 语义）。

### 2.5 前端分页控件与状态（既有，**与常见描述不符，务必以本节为准**）

- `src/components/DataTable/Pagination.tsx` 今天**只有**：左侧 `from-to / totalRows` 文本、`perPage` 的 `Select`（`25/50/100/200/500`）、**两个按钮**（`ChevronLeft` / `ChevronRight`，即「上一页」「下一页」）、中间的 `Page X / Y` 标签。**没有首页、没有末页、没有手填页码/Offset 的跳页输入框。**这与 PRD 矩阵里 C-47「手填 `Offset` 跳页：TP ✅ / DS ❌」一致。
- `Pagination.tsx::paginationReducer` 是纯函数：`pageChanged` 把 `Math.trunc` 后的页号 `Math.max(0, …)`（**接受任意页码**）；`pageSizeChanged` 改页大小时把 `page` 归 0；`filterChanged` 只把 `page` 归 0。
- `Pagination.tsx` 声明了 `filterRevision` / `onPageReset` 两个 props 并在 `useEffect` 里做「筛选修订号变化 → `onPageReset()` + 回第 0 页」，但 **`src/components/DataTable/DataTable.tsx` 从不传这两个 prop**（DataTable 渲染 `<Pagination page pageSize totalRows onPageChange onPageSizeChange loading />`）——它们今天等价于死 prop；筛选变化导致的归零实际由 store 负责。
- `src/components/DataTable/DataTable.tsx` 只在 `page != null && pageSize != null && totalRows != null && onPageChange && onPageSizeChange` 时渲染 `Pagination`。
- `src/windows/connection/TableView.tsx` 把 `actions.setPage` / `actions.setPageSize` 接到 `Pagination`；`actions.setPage = (page) => store().setPage(panelId, page)`。
- `src/stores/tableDataStore.ts`：
  - `TableState`（`src/stores/tableData/types.ts`）持有 `page` / `pageSize` / `totalRows` / `sorts: SortCondition[]` / `filters` / `filterLogic` / `requestRevision` / `loadingRevision`；`emptyTableState` 的缺省是 `page: 0`、`pageSize: 50`、`sorts: []`、`totalRows: 0`。
  - `loadTableData` 发 `databaseCommands.getTableData({… page, pageSize, filters: filters.filter(isCompleteFilter), sorts, skipCount, filterLogic, database, schema })`；`pageSize` 首次取 `settings.defaultPageSize || DB_REGISTRY[driverType].defaultPageSize || 已有值`。
  - `commitFetchedPage` 有**唯一的写入口径**：只有 `current.requestRevision === requestRevision && current.loadingRevision === requestRevision` 才落库，落库时写 `totalRows: res.totalRows ?? current.totalRows`、`page: res.page`、`pageSize: res.pageSize`，清 `selectedRows`、重建 `editBuffer` 与 `rowIdentityAnchors`。**键集翻页必须复用同一套修订号判定，禁止绕开它直接写 `rows`**（00 契约第 1.1 节第 2 条）。
  - `setPage(panelId, page)` → `patchPanelForReload(page)` + `reloadPanel(panelId, { skipCount: true })`；`setPageSize` → `pageSize` 改、`page` 归 0、`skipCount: true`；`setSort` → `sorts: [sort]`、`page: 0`、`skipCount: true`；`applyFilters` / `setFilters` / `clearFilters` → `page: 0`（有筛选变化时不带 `skipCount`，因此会重跑 COUNT）。
  - `setPage` 的唯一调用方是 `Pagination` 的相邻翻页按钮与「筛选修订号」分支（后者未接线），**今天没有任何调用方请求非相邻页**。
- `src/commands/database.ts::databaseCommands.getTableData` 是 IPC 封装，逐个字段显式传参（`database` / `schema` 缺省补 `null`）。
- `src/types/index.ts::TableDataResult { columns, rows, totalRows?, page, pageSize }` 与 `src/types/index.ts::SortCondition { column, descending }` 是前端镜像；`src/types/settings.ts` 里还有一份重复的 `SortCondition`。
- `src/locales/en/query.ts` 已持有 `pagination.perPage` / `pagination.prev` / `pagination.next` / `pagination.page` / `pagination.pageOf`。
- `src/lib/gridErrors.ts` 在基线中不存在，但**归属已由 00 契约 §8.1 冻结给 01**：本册合并时它已存在，本册只按三步追加 `grid.paging.*` 分支。见第 8 节。
- `src/stores/__tests__/tableDataStore.test.ts`、`src/components/DataTable/__tests__/Pagination.test.tsx`、`src/stores/tableData/__tests__/` 是前端既有测试落点。

### 2.6 与 00 契约不一致/需回填的点（不藏）

1. 00 契约第 1.5 节摘录的 `get_table_data` 少了 `filter_logic: Option<&str>`；真实签名以 2.1 为准。
2. 00 契约把「首页/跳页」等操作留在想象里；真实控件只有上一页/下一页/页大小（2.5）。**本册所有「跳页」讨论都在现实约束下展开：今天没有跳页 UI，但 store 与 reducer 的页码语义接受任意页。**
3. `filters::SHARED_FILTER_OPERATORS`（契约第 5.2 节引用）在 `packages/driver-api/src/filters.rs` 中**尚不存在**，属 DB-08 的新增物；本册不引用它。
4. `supports_keyset_pagination` / `keyset_predicate_sql`（契约第 5.5 节）在全仓 grep 结果为空——**确认尚未实现，且签名以契约第 5.5 节为最终形态，本册不改一个字符**。

---

## 3. 数据结构与接口设计

### 3.1 契约冻结的 trait 方法（既有契约，**不得改动**）

以下两段逐字来自 00 契约第 5.5 节，本册只实现它们：

```rust
// packages/driver-api/src/traits.rs（本册把它们从「契约」变成「代码」）
/// 该驱动是否支持键集（seek）分页。默认 false = 继续走 OFFSET 分页。
fn supports_keyset_pagination(&self) -> bool { false }

/// 构造键集分页的 WHERE 片段（不含前导 `WHERE`/`AND`）。
/// 返回 None 表示该排序组合无法安全转成键集条件。
fn keyset_predicate_sql(
    &self,
    order_columns: &[(String, bool)],
    boundary: &[(String, Value)],
) -> Option<String> {
    None
}
```

**本册对这两个签名补充三条实现约定**（不改签名，只把语义写死）：

| 约定 | 内容 |
| --- | --- |
| P1 | `order_columns` 的顺序**就是** `ORDER BY` 的顺序，`bool` 为 `true` 表示 `DESC`；`boundary` 的顺序与它**逐项对应**（`boundary[i]` 是 `order_columns[i]` 的值） |
| P2 | 返回的片段是**完整可拼的 SQL 文本**（值由驱动用**自己的** `format_sql_literal` 内联，理由见 2.3 与 K-6），宿主只负责加括号与 `WHERE`/`AND` |
| P3 | 只要出现以下任一情况就**必须**返回 `None`：`order_columns` 为空；两者长度不等；任一 `boundary` 是 `Value::Null`；该组合无法安全转换 |

### 3.2 新增 Rust 类型（新增）

位置：`packages/driver-api/src/keyset.rs`（**新增文件**，`lib.rs` 里加 `pub mod keyset;`，与 `pub mod filters;` 同样的公开性）。

```rust
use serde::{Deserialize, Serialize};
use crate::traits::DatabaseDriver;
use crate::types::Value;

/// 一次分页读实际使用的分页策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PagingMode {
    /// 页码 × 每页条数 = OFFSET。今天的唯一形态。
    Offset,
    /// 用键集条件 seek；请求里的 page 只是「访问过的第几页」这个序数。
    Keyset,
}

/// 一个排序列及其方向，用于向宿主与前端披露「本页生效的稳定全序」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderColumn {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

/// 键集不可用的原因。宿主据此选文案，前端据此决定是否提示；`None` 表示可用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeysetUnavailableReason {
    /// `supports_keyset_pagination()` 为 false。
    DriverUnsupported,
    /// 排序不构成稳定全序（无主键列、或排序列集合不含任何唯一键）。
    NoStableOrder,
    /// 排序键里含可空列（NULL 不参与大小比较，会静默漏行）。
    NullableKeyColumn,
    /// 游标值里出现 NULL。
    NullBoundary,
    /// `keyset_predicate_sql()` 返回 None（驱动判断该组合不安全）。
    PredicateUnavailable,
    /// 驱动给不出分页子句（`PaginationSyntax.clause` 为空，方言无法分页）。
    NoPaginationClause,
    /// 请求自带的游标与当前排序/筛选签名不符，已被丢弃。
    StaleCursor,
    /// 游标栈超过上限，或用户请求了非相邻页。
    NotAdjacent,
}

/// 一页的分页元信息，随 `TableDataResult` 返回给调用方。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagePagingInfo {
    /// 本次实际使用的策略。**前端只信这个字段，不信自己的猜测。**
    pub mode: PagingMode,
    /// 本页 `ORDER BY` 的真实列序（含为凑成全序而追加的主键列）。
    pub order_columns: Vec<OrderColumn>,
    /// 本页最后一行的键值，供下一次「下一页」原样回传；`None` 表示本页无后继
    /// （未启用键集、页为空、或驱动给不出边界）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_after_key: Option<Vec<CursorValue>>,
    /// `mode == Offset` 且曾尝试键集时，说明为什么没走键集。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<KeysetUnavailableReason>,
    /// 键集翻页时「本页行数 < 每页条数」但按 COUNT 仍应有后续页——说明翻页期间数据被删。
    /// 前端据此提示「行数已刷新」并触发一次重新计数。
    #[serde(default)]
    pub drift_suspected: bool,
}

/// 游标值的**线上形态**。与 `Value` 同构但带显式标签。
///
/// 为什么不能直接用 `Value`：`Value` 是 `#[serde(untagged)]`，`[1,2,3]` 既能读成
/// `Bytes` 又能读成 `Json`，字符串既可能是 `String` 也可能是 `Timestamp`。游标要经过
/// 「Rust → 前端 → Rust」两跳，untagged 会在第二跳把 `Json` 读成 `Bytes`，于是谓词从
/// `'{"a":1}'` 变成 `X'7b226131...'`，**键集边界悄悄算错**。带标签的形态与 00 契约
/// 第 3.2 节 `CellWrite` 的 `tag = "kind", content = "value"` 是同一手法。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum CursorValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Timestamp(String),
    Json(serde_json::Value),
}

impl CursorValue {
    /// 无损降级到驱动契约用的 `Value`。
    pub fn to_value(&self) -> Value { /* 新增：逐变体映射，不做字符串化 */ }
    /// 从查询结果里的 `Value` 提升。`None` 表示该行该列是 SQL NULL。
    pub fn from_value(value: &Value) -> Self { /* 新增 */ }
}
```

**`TableDataResult` 的唯一改动（新增字段，不改既有字段）**：位置 `packages/driver-api/src/types.rs`。

```rust
pub struct TableDataResult {
    pub columns: Vec<ColumnSchema>,
    pub rows: Vec<Vec<Option<Value>>>,
    pub total_rows: Option<i64>,
    pub page: u32,
    pub page_size: u32,
    /// 本页的分页元信息；老调用方/老构造点不填即 `None`，语义等同 OFFSET 分页。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paging: Option<crate::keyset::PagePagingInfo>,
}
```

> 字段加 `#[serde(default)]` 是为了「老前端 / MCP / 第三方调用方读到 `null` 也不炸」；但 Rust 侧结构体字面量**必须**补这个字段。全仓只有 `query_executor.rs` 的两个构造点（2.3），加字段的爆炸半径由此确定；外部 git 驱动的风险与替代方案见 K-4。

### 3.3 宿主内部类型（新增，位置见第 6 节）

```rust
// src-tauri/src/services/table_paging.rs（新增）
/// 一次读请求生效的排序规格——**它就是最终写进 ORDER BY 的东西**，
/// 不允许任何下游再自己算一遍（否则文档一改、代码一改就漂移）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OrderSpec {
    pub columns: Vec<OrderColumn>,
    /// 本规格的来源：显式排序 / 驱动默认排序 / 主键注入 / 第一列兜底 / 无排序。
    pub source: OrderSource,
    /// 判定全序时使用的唯一键列（今天只可能是主键；见 K-9）。
    pub unique_key: Vec<String>,
}

pub(crate) enum OrderSource { ExplicitSort, DriverDefault, PrimaryKeyInjection, FirstColumnFallback, None }

/// 键集可用性判定结果。`Eligible` 之外一律走 OFFSET，并把原因带到 UI。
pub(crate) enum KeysetDecision {
    /// 可用，并给出「补齐后的稳定全序」（含追加的主键列）。
    Eligible { order: OrderSpec },
    /// 不可用 + 原因 + 用于展示的排序规格。
    Unavailable { reason: KeysetUnavailableReason, order: OrderSpec },
}

/// 判定当前排序是否构成**稳定全序**，并在可行时把主键补进排序。
///
/// 算法（与 09 分册的默认排序策略必须一致）：
/// 1. 先由宿主产出「将要写进 SQL 的排序规格」（显式排序 → 驱动 `default_order_columns`
///    → 主键注入 → 第一列兜底），**不许另算一份**；
/// 2. 取唯一键集合（今天 = `CachedColumns.primary_keys`，即产出 ORDER BY 的那一份）；
/// 3. 若唯一键为空 → `NoStableOrder`（无主键表：任何排序都不是全序）；
/// 4. 若排序列已包含唯一键的全部列 → 稳定；
/// 5. 否则把缺失的唯一键列**按主键声明顺序追加**到排序末尾（沿用原方向：追加列一律 ASC），
///    得到 `Eligible` 的 `order`——因为「排序元组包含唯一键」等价于「元组能唯一确定一行」，
///    这才是键集条件成立的前提；
/// 6. 逐列检查 `ColumnSchema.nullable`：任一排序列可空 → `NullableKeyColumn`（见 3.5）。
pub(crate) fn keyset_decision(
    order: OrderSpec,
    columns: &[ColumnSchema],
) -> KeysetDecision;
```

**为什么「排序元组包含唯一键」就够**：键集条件是「按同一个排序元组做字典序比较」，若两行的元组值可能相同（存在并列），引擎对并列行的先后是任意的，于是翻页时这些行可能被跳过或重复。当元组包含某个唯一键的全部列时，两行的元组必然不同，比较才有全序意义。**只把主键放在「最后一列做 tie-breaker」是不够的——必须保证它真的出现在排序里**，所以第 5 步是「追加」而不是「假设」。
> 注意方向：追加的主键列用 `ASC`（与今天主键注入的方向一致），不反转用户的方向。混合方向的字典序展开见 3.4。

```rust
// src-tauri/src/services/query_executor/mod.rs（08 已目录化）；键集逻辑另落 table_read_sql.rs
/// 组装一次分页读的 SELECT。新增符号；`build_select_sql` 保留原名、原签名、原语义，
/// 作为「无游标」适配器转调它，从而既有的 5 个语义锚点用例一个都不动。
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_paged_select_sql(
    table_name: &str,
    columns: &[ColumnSchema],
    filters: Option<Vec<FilterCondition>>,
    order_spec: &OrderSpec,        // 取代 Option<OrderBy>
    keyset: Option<&[(String, Value)]>, // Some = 本次请求带游标
    qi: &dyn Fn(&str) -> String,
    format_lit: &dyn Fn(&Value) -> String,
    pagination: &PaginationSyntax,
    filter_logic: Option<&str>,
    predicate_sql: Option<&str>,   // 驱动给的片段，None 即降级
) -> PagedSelectSql;

pub(crate) struct PagedSelectSql { pub sql: String, pub mode: PagingMode }
```

### 3.4 键集条件的构造策略与驱动分工（新增）

**规范形态 = 逐列展开的字典序析取**（可移植，T-SQL 也吃）：

对 `ORDER BY k1 d1, k2 d2, …, kn dn`（`di ∈ {ASC, DESC}`）与边界 `(b1…bn)`，「边界之后」的条件是：

```
(k1 ⋈1 b1)
OR (k1 = b1 AND k2 ⋈2 b2)
OR (k1 = b1 AND k2 = b2 AND k3 ⋈3 b3)
…
OR (k1 = b1 AND … AND k(n-1) = b(n-1) AND kn ⋈n bn)
```

其中 `⋈i` = `>`（`di = ASC`）或 `<`（`di = DESC`）。等式项与方向无关。

- **单列升/降序**：退化为 `(k1 > b1)` 或 `(k1 < b1)`。
- **多列同向**：`(a, b) > (b1, b2)` 这种**行值比较**在 PostgreSQL / MySQL / SQLite(≥3.15) / ClickHouse 上合法，**在 T-SQL 上不合法**；同向时行值比较与展开式语义等价。
- **多列混合方向**：行值比较**无法表达**（`(a,b) > (x,y)` 隐含两列同向），必须用展开式。
- **展开式的代价**：占位/字面量数量是 `n(n+1)/2`（与 Data Sync 的 `keyset_seek_parameter_count` 同形），但本路径是字面量内联，因此只是 SQL 变长；`n` 在实践中 ≤ 4（排序列 + 主键），可接受。

**驱动分工**（零硬编码的唯一正确姿势）：

| 层 | 职责 |
| --- | --- |
| `driver-api` 公开 helper | `packages/driver-api/src/keyset.rs::lexicographic_seek_predicate(driver, order_columns, boundary) -> Option<String>`：实现上面这段展开式，引用/字面量分别走 `driver.quote_ident` / `driver.format_sql_literal`；出现 3.1 的 P3 情况返回 `None`。**它不是默认实现**——trait 默认仍是 `None`，只是给驱动一个「一行接入」的参考实现 |
| 驱动 crate | 决定用哪种形态：`fn keyset_predicate_sql(...)` 里调 helper（展开式），或自己发行值比较。**这个决定属于驱动**，因为「方言支不支持行值比较」是方言知识 |
| 宿主 | 只做三件事：拿到片段 → 加括号 → 用 `AND` 拼到既有 `WHERE` 之后；**永远不自己拼键集条件、永远不按驱动名分支** |

各驱动的**建议**形态（写在驱动自己的注释与测试里）：

| 驱动 | 建议 | 理由 |
| --- | --- | --- |
| `postgres` / `mysql` | 全部同向且无 NULL → 行值比较 `(a, b) > (lit, lit)`；否则回退展开式 | 优化器对行值比较的索引利用更好；两库都支持 |
| `sqlite` | 一律展开式 | 行值比较需 ≥3.15，宿主无法探测内嵌版本，展开式无版本风险 |
| `sqlserver` | **必须**展开式 | T-SQL 没有行值比较语法（`packages/data-sync/src/keyset.rs` 的注释已确认） |
| `clickhouse` | v1 先用展开式 | `ORDER BY` 主键注入正是它的性能地狱；先把「能用键集」落地，形态优化留后续 |
| 其它 | 不覆盖 = 继续 `false` / `None` = 走 OFFSET | 兼容性铁律 R2 |

### 3.5 NULL 语义与可空键列（**必须写死的规则**）

SQL 里 `NULL` 不参与大小比较：`NULL > 5` 与 `NULL < 5` 都是 `UNKNOWN`，`WHERE` 直接不成立。因此排序键里只要有**一个**可空列，键集条件会在跨过 NULL 区块时**静默漏掉整段行**——这比慢更严重（用户会以为数据不存在）。而 NULL 在 `ORDER BY` 里排在最前还是最后是**方言行为**（PostgreSQL 默认 `NULLS LAST`（ASC）/`NULLS FIRST`（DESC），MySQL / SQLite / SQL Server 默认 NULL 在前），00 冻结的契约里**没有任何方法**能让宿主表达 NULL 位置。

**规则 N（本册的最终裁定）**：

1. 排序规格里**任一列** `ColumnSchema.nullable == true` → 键集**不可用**（`NullableKeyColumn`），该请求走 OFFSET，UI 给出可读原因。不区分「领头列」与「尾随列」——`(a, b)` 的字典序比较里 `a` 为 NULL 时整个析取式都不成立，尾随列同样致命。
2. 主键列在 SQL 语义上恒为 NOT NULL，因此**默认排序（主键注入）天然满足规则 N**——这正是「有主键的表开箱即用键集」的原因。
3. 兜底：即使前端/缓存把某个可空列误标为 NOT NULL，驱动在 `keyset_predicate_sql` 里看到 `Value::Null` 边界**必须返回 `None`**，宿主拿到 `None` 立即降级到 OFFSET，并在**同一个请求**里放弃游标（不得「用一半谓词」）。
4. 不允许「附加 `IS NULL` 分支」的写法进入 v1：它需要知道 NULL 排在升序的前还是后，而契约里没有这个能力位。要让这条可行，得先给 `DatabaseDriver` 加 `null_ordering()`——**那是改契约，不在本册授权范围**（K-2）。

### 3.6 前端类型与状态（新增）

`src/types/index.ts`：

```ts
export type PagingMode = 'offset' | 'keyset';

export type KeysetUnavailableReason =
  | 'driverUnsupported' | 'noStableOrder' | 'nullableKeyColumn' | 'nullBoundary'
  | 'predicateUnavailable' | 'noPaginationClause' | 'staleCursor' | 'notAdjacent';

/** 游标值的线上形态，与 Rust `CursorValue` 同构（带 kind 标签，不用裸 Value）。 */
export type CursorValue =
  | { kind: 'null' } | { kind: 'bool'; value: boolean } | { kind: 'integer'; value: number }
  | { kind: 'float'; value: number } | { kind: 'string'; value: string }
  | { kind: 'bytes'; value: number[] } | { kind: 'timestamp'; value: string }
  | { kind: 'json'; value: unknown };

export interface OrderColumn { column: string; descending: boolean }

export interface PagePagingInfo {
  mode: PagingMode;
  orderColumns: OrderColumn[];
  nextAfterKey?: CursorValue[];
  unavailableReason?: KeysetUnavailableReason;
  driftSuspected: boolean;
}

export interface TableDataResult {
  columns: ColumnSchema[];
  rows: (Value | null)[][];
  totalRows?: number;
  page: number;
  pageSize: number;
  /** 老后端不返回该字段 → 视为 OFFSET 分页。 */
  paging?: PagePagingInfo;
}
```

`src/lib/keysetPaging.ts`（**新增，纯函数，无 React 依赖**）：

```ts
/** 页大小与游标栈的硬上限：超过即不再增长栈，改用 OFFSET 并重新锚定。 */
export const KEYSET_STACK_LIMIT = 200;

export interface KeysetCursor {
  /** 该页起始边界；第 1 页为 null。 */
  boundary: CursorValue[] | null;
  /** 生成该边界时的排序签名，用于失效判定。 */
  orderSignature: string;
  /** 生成该边界时的筛选签名，用于失效判定。 */
  filterSignature: string;
}

export interface KeysetPagingState {
  /** 当前策略。默认 'offset'。 */
  mode: PagingMode;
  /** 后端披露的稳定全序；默认 []。 */
  orderColumns: OrderColumn[];
  /** 游标栈：`stack[i]` 是第 `anchorPage + i` 页的起始边界；默认 []。 */
  stack: KeysetCursor[];
  /** 栈顶对应的页码（非相邻跳页后重新锚定用）；默认 0。 */
  anchorPage: number;
  /** 最近一次降级原因；默认 null。 */
  unavailable: KeysetUnavailableReason | null;
  /** 是否怀疑翻页期间数据被删（由后端 driftSuspected 驱动）；默认 false。 */
  driftSuspected: boolean;
}

export const emptyKeysetPagingState: KeysetPagingState = {
  mode: 'offset', orderColumns: [], stack: [], anchorPage: 0,
  unavailable: null, driftSuspected: false,
};

/** 排序 + 筛选的稳定签名（列名与方向、算子与值的规范化序列化），用于游标失效判定。 */
export function pagingSignature(sorts: SortCondition[], filters: FilterCondition[], logic: 'and' | 'or'): string;

/** 目标页能否用栈里的边界命中；返回 null 表示必须走 OFFSET 并重新锚定。 */
export function cursorForPage(state: KeysetPagingState, targetPage: number): KeysetCursor | null;

/** 一次「下一页」成功后，把新页的起始边界压栈（带上限与签名）。 */
export function pushCursor(
  state: KeysetPagingState, page: number, info: PagePagingInfo,
  signature: string,
): KeysetPagingState;

/** 非相邻跳页 / 页大小变化 / 筛选排序变化：清栈并重新锚定到 `anchorPage`。 */
export function reanchor(state: KeysetPagingState, anchorPage: number): KeysetPagingState;

/** 依据后端 `paging` 元信息与本地状态决定下一次请求是否携带游标。 */
export function nextRequestCursor(
  state: KeysetPagingState, targetPage: number, signature: string,
): { afterKey: CursorValue[] | null; fallbackToOffset: boolean };
```

`src/stores/tableData/types.ts::TableState` 追加一个字段：

```ts
  /** 键集分页状态；缺省见 emptyKeysetPagingState（mode 'offset'、空栈、无原因）。 */
  keyset: KeysetPagingState;
```

缺省值一律「等于今天」：`mode: 'offset'`、`stack: []`、`unavailable: null`、`driftSuspected: false`；`emptyTableState` 里补 `keyset: emptyKeysetPagingState`。**不新增第二个 `page` 语义**：`TableState.page` 仍然是「第几页」这个序数，键集只改变「这一页怎么取」。

---

## 4. 交互与状态机

### 4.1 决策 J：与「随机跳页」的根本冲突（本册最关键的取舍）

**冲突的本质**：OFFSET 分页里「页码」是**位置**，`OFFSET = page × pageSize` 是定义式，所以任意页码可解；键集的「游标」是**值边界**，第 N 页的边界是「前 N-1 页所有行之后的那一行」，**与 N 之间没有任何函数关系**——不真正读过前面的行，就不知道第 N 页从哪里开始。所以：

> **键集分页下不存在「直接跳到第 N 页」；任何声称能从页码推出游标的实现都是错的**（它会返回一页内容正确性无法保证的数据，比慢得多的问题严重得多）。

三种候选方案的成本与结论：

| 方案 | 做法 | 成本 | 结论 |
| --- | --- | --- | --- |
| A · 禁用跳页 | 键集模式下不提供任何跳页入口，只保留上一页/下一页 | 最低（今天本来就没有跳页 UI，见 2.5） | **v1 的 UI 决策：采纳**。PRD 的 C-47 是 P2，不在本批范围；未落地之前不给入口，符合 00 契约 R4「能力缺失时隐藏入口」 |
| B · 跳页降级 OFFSET | 请求非相邻页时，这一次读走 OFFSET（`page × pageSize`），随后把该页设为新的游标锚点 | 一条额外的代码路径 + 用户跳页时付一次深翻页代价 | **v1 的 store 决策：采纳**。它让「页码语义」在任何未来入口下都诚实：跳页慢，但结果正确；且不需要服务端状态 |
| C · 记录游标栈 | 把访问过的每页边界存下来，`prev` 用栈顶边界重取 | 内存 O(访问页数)（本册用 `KEYSET_STACK_LIMIT = 200` 封顶） | **采纳，但只用于「上一页」**。它**不能**替代 B：栈里只有**访问过**的页，跳到一个从未访问的页（如第 500 页）仍无边界可用 |

**最终裁定（三合一，不许含糊）**：

1. **上一页/下一页**：键集。下一页用「本页末行键值」作边界；上一页用栈里该页的起始边界（值边界，天然抗漂移）。
2. **非相邻页请求**（`|target - page| > 1`，或目标页不在栈的可达范围内）：**这一次**走 OFFSET，随后 `reanchor(target)`（`stack` 截断为 `[null]`、`anchorPage = target`）。UI 上显示「快速翻页已从第 N 页重新开始」。
3. **UI 上不提供跳页入口**（A），但 store 层的语义必须已经支持 B——因为 `setPage(panelId, n)` 与 `paginationReducer` 今天**接受任意页码**，收紧语义的责任在 store，不在控件。
4. **`setPage` 语义收紧（新增符号）**：保留 `setPage(panelId, page)` 为「移动到指定页」但**在键集模式下按 2 处理**；新增 `setPageExact(panelId, page)`（新增）供将来的跳页 UI 显式表达「我知道这是一次跳转」。今天唯一调用方只发相邻页，因此收紧不改变现有行为。

### 4.2 状态与跃迁

| 状态 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `offset`（缺省） | 面板首次加载；能力位为 false；任何降级原因 | 完全等于今天：`page × pageSize` 作 OFFSET，`skipCount: true` 翻页 | 一次请求返回 `paging.mode === 'keyset'` 且 `unavailableReason` 为空 → `keyset` |
| `keyset` | 后端报告键集可用（3.3 的 `Eligible`），且本次请求未降级 | 下一页：把栈顶页的 `nextAfterKey` 压栈后请求；上一页：取 `stack` 中该页边界请求；两页都沿用 `requestRevision` 竞态保护 | 排序/筛选/页大小变化 → `invalidating`；能力位变假/`unavailableReason` 出现 → `degraded`；非相邻页 → `jump`；面板关闭 → 状态随 `removePanel` 销毁 |
| `degraded` | 收到 `unavailableReason`（任一原因） | 与 `offset` 同行为，但分页器常驻一条可读原因；`unavailable` 字段被写入 | 原因消失（如换成按主键排序）→ 下一次请求回到 `keyset`；用户点「按主键排序」等出路 → `keyset` |
| `jump` | `setPage` 收到非相邻页 | 这一次用 OFFSET；响应回来后 `reanchor` 到目标页并提示 | 立即回落到 `keyset`（新锚点）或 `degraded`（若本次请求本身不可用） |
| `invalidating` | `setSort` / `applyFilters` / `setFilters` / `clearFilters` / `setPageSize` 被调用 | 清空 `stack`、`anchorPage = 0`、`page = 0`（`setPageSize` 保留 `pageSize` 改动）、重新发请求（`skipCount` 按既有规则：排序/页大小带 `skipCount: true`，筛选变化不带） | 请求完成 → `keyset` 或 `degraded`；面板关闭 → 销毁 |

**游标失效判定（签名法）**：每次请求前比较 `pagingSignature(sorts, filters, filterLogic)` 与本栈顶记录的 `orderSignature`/`filterSignature`；不一致即视为 `staleCursor`：**不得**把旧边界发给后端（否则会拿到「按新排序取值、按旧边界定位」的错页），而是清栈、回第 0 页、发一次无游标请求。后端也做同一层防御：收到游标但自身判定不可用 → 忽略游标、返回第 0 页 + `unavailableReason: StaleCursor`。

**为什么「上一页」也用值边界而不是 OFFSET 回退**：OFFSET 回退的上一页在漂移下会与刚看过的页错位（用户视角是「上一页内容变了」）；值边界重取的是同一个「从这一行开始的一页」，语义稳定。反向 seek（`ORDER BY` 全反向 + 结果倒序）能省内存但引入「方向取反 + 行倒序 + 空页判定」三处新逻辑，本册不做（K-5）。

---

## 5. 实现步骤

> **顺序是硬约束**：Step 1~2 只加**默认全 false / 全 None** 的契约与共享实现，此时**任何驱动行为逐字节不变**；Step 3~5 在宿主内建能力但**默认不开启**；Step 6~7 才让具体驱动与前端开启。任何一步都不允许把签名改动混进来。

**提交分组**（每条可独立合并、可独立回滚；最后一条对应总纲 §4.2 的提交序列第 12 条）：

| 提交 | 覆盖步骤 | 提交信息建议 | 门禁 |
| --- | --- | --- | --- |
| C1 | Step 1~2 | `feat(driver-api): add keyset pagination contract with default-off impls` | `cargo test -p datazen-driver-api --lib`、`cargo test -p datazen --lib` |
| C2 | Step 3~5 | `feat(query): build keyset paged reads behind the driver capability` | `cargo test -p datazen --lib` |
| C3 | Step 6、Step 8 | `feat(grid): wire keyset cursors and paging hints` | `npx vitest run`、`pnpm typecheck` |
| C4 | Step 7、Step 9 | `feat(grid): add keyset pagination` | 上述 + 各驱动 crate + `pnpm e2e:skip-build` + `pnpm e2e:contract:matrix` |

### Step 1 · `driver-api` 新增 `keyset` 公开模块（纯新增，不含 trait 方法）

改：`packages/driver-api/src/keyset.rs`（新增）、`packages/driver-api/src/lib.rs`（加 `pub mod keyset;`）。
加：`PagingMode`、`OrderColumn`、`KeysetUnavailableReason`、`PagePagingInfo`、`CursorValue`（`to_value` / `from_value`）、`lexicographic_seek_predicate`。
为什么：这些类型要被宿主、前端与所有驱动共用；共享算法集中一处，避免每个驱动各写一遍展开式（仓库当前注册 18 个：15 个 path + 3 个 git）；`CursorValue` 的带标签形态必须先定下来，否则前端游标回传会在 untagged `Value` 上静默变形。
自测：`cargo test -p datazen-driver-api --lib` —— 新增的表驱动用例（第 9 节 D1~D5）。

### Step 2 · 契约方法落地（默认实现 = 今天）

改：`packages/driver-api/src/traits.rs`（按 3.1 **逐字**加两个方法，默认体 `false` / `None`）、`packages/driver-api/src/reuse.rs`（转发两个方法）。
加：兼容性用例「什么都不覆盖的驱动 → `false` / `None`」（D6）；`reuse.rs` 的转发用例（D7）。
为什么：先让契约存在于代码里、且对既有驱动**零影响**，后续每一步都能被这道门禁保护；`ReuseDriver` 不转发的话，线级复用驱动永远拿到默认值，能力位形同虚设。
自测：`cargo test -p datazen-driver-api --lib`；**并**跑 `cargo test -p datazen --lib` 确认宿主未受影响（这两个方法还没被调用）。

### Step 3 · 宿主的排序规格与全序判定（仍未接线）

改：`src-tauri/src/services/table_paging.rs`（新增），`src-tauri/src/services/mod.rs`（导出）。
加：`OrderSpec` / `OrderSource` / `KeysetDecision` / `keyset_decision` / `cursor_from_row`（从结果集末行按 `order_columns` 取键值，返回 `Option<Vec<CursorValue>>`）。
为什么：把「什么算稳定全序」「哪里取边界」变成**纯函数**，可以脱离数据库与 IPC 单测；这是本册最容易写错、也最该被测死的部分。
自测：`cargo test -p datazen --lib`——`keyset_decision_*` 表驱动用例（第 9 节 H1~H6）。

### Step 4 · 宿主的 SQL 组装（仍不改变默认行为）

改：`src-tauri/src/services/table_read_sql.rs`（**新增**，把 `build_select_sql` / `build_count_sql` / `format_condition` / `filter_is_complete` / `filter_join` **机械搬迁**过来）、`src-tauri/src/services/query_executor/mod.rs`（调用新模块、只留编排）。**注意路径**：本册排在提交序末位，此时 `query_executor/` 目录**已由 08 完成目录化**，基线里的单文件 `query_executor.rs` 不复存在——凡本册仍写 `query_executor.rs` 的地方，一律读作 `query_executor/mod.rs`。
加：`build_paged_select_sql` + `PagedSelectSql`；`build_select_sql` 保留原签名、原语义（作为无游标适配器）。
为什么：`query_executor.rs` 已 797 行（2.1），键集逻辑必须落在新文件（第 6 节）；搬迁是纯位移，**搬迁前后 SQL 文本必须逐字节相同**。
自测：`cargo test -p datazen --lib`，**先确认既有的 5 个语义锚点用例（A1 列出的）在搬迁后原样通过**，再跑新增的黄金 SQL 用例（第 9 节 H7~H12）。

### Step 5 · 宿主读链路与 IPC 接线（默认不启用）

改：`src-tauri/src/services/query_executor/mod.rs`（新增 `get_table_data_page`，`get_table_data` 变成它的薄包装、签名不变）、`src-tauri/src/commands/schema.rs`（`get_table_data_impl` 接可选 `after_key`、回填 `paging`）。
加：`grid.paging.keysetUnavailable` / `grid.paging.cursorInvalidated` / `grid.paging.cursorTooDeep` 三个 `CommandError` 消息；`PagePagingInfo` 的组装。
为什么：能力的**裁决权在后端**——前端只信 `paging.mode`；游标值由后端从**它自己返回的行**里截取，前端只做原样回传，避免前端按列名重新推导边界（列名会随 `visibleColumns` 漂移）。
自测：`cargo test -p datazen --lib`（H13~H16：mock 驱动开/关能力各一条）；`commands/schema/tests.rs` 里既有的 `get_table_data_*` 用例必须原样通过。
> ⚠ 自测必须包含「**不传 `afterKey` 的老调用形状仍然成功**」：Tauri v2 命令的缺省参数行为要用一个显式 `invoke`（或临时用例）验证一次；若发现缺失参数会报 `invalid args`，则**不要**给既有命令加参数，改为新增 `get_table_data_keyset` 命令承载游标（见 K-4 的同类取舍）。

### Step 6 · 前端状态与请求接线

改：`src/types/index.ts`、`src/stores/tableData/types.ts`、`src/stores/tableData/connectionState.ts`（`emptyTableState`）、`src/stores/tableDataStore.ts`（`loadTableData` 带游标、`commitFetchedPage` 写 `keyset`、`setPage` 按 4.1 收紧）、`src/commands/database.ts`（可选 `afterKey`）。
加：`src/lib/keysetPaging.ts` + `src/lib/__tests__/keysetPaging.test.ts`。
为什么：竞态保护、选择清理、`rowIdentityAnchors` 重建全部走既有 `commitFetchedPage`，键集**不许**另开一条写 `rows` 的路。
自测：`npx vitest run`（F1~F6）+ `pnpm typecheck`。

### Step 7 · 让具体驱动开启（每个驱动一个独立小提交）

改：`packages/drivers/postgres`、`packages/drivers/mysql`、`packages/drivers/sqlite`、`packages/drivers/sqlserver` 各加两个方法的覆盖，测试写在各自 crate（`src/<driver>/tests.rs` 或 `tests/`）。
加：各驱动的 `keyset_predicate_sql` 用例（第 9 节 P/M/S/T 组）。
为什么：兼容性铁律要求「先有默认实现、再让驱动开启」；能力位一旦开启就改变了 SQL 生成路径，必须一个驱动一个提交，才能独立回滚。
自测：`cargo test -p datazen-driver-<id>`；**`packages/drivers/sqlserver` 的用例必须断言 SQL 里不出现行值比较**（`((a, b) > …)` 这种形状）。

### Step 8 · UI 提示与降级可见

改：`src/components/DataTable/Pagination.tsx`（新增可选 `pagingHint?: { reason: KeysetUnavailableReason | null; driftSuspected: boolean; mode: PagingMode }`，渲染一行提示）、`src/components/DataTable/DataTable.tsx`（透传）、`src/windows/connection/TableView.tsx`（从 store 取）。
为什么：降级必须**可见**，否则用户会把「慢」当成「坏」；同时 UI 不得渲染一个点了会跳错页的按钮。
自测：`npx vitest run`（F7~F8）+ `pnpm e2e:skip-build`。

### Step 9 · 文案、错误码与文档收尾

改：`src/locales/en/query.ts`（第 8 节 key 全表）、`src/lib/gridErrors.ts`（**01 已建骨架**，本册按契约 §8.1 三步追加 `grid.paging.*` 前缀分支，禁止整文件重写；该文件由 01 拥有，先建则只追加）。
加：`e2e/specs/table-keyset-paging.ts`；`e2e/contract/journeys/` 的分页 journey。
为什么：文案与错误码是「可理解」的一部分；E2E 是唯一能证明「顺序翻页不重复」的门禁。
自测：`npx vitest run`、`pnpm e2e:minimal`、`pnpm e2e:contract:matrix`。

---

## 6. 文件级改动清单

| 文件 | 新增/修改 | 职责 | 预估行数 | 触及 800 行上限？ |
| --- | --- | --- | --- | --- |
| `packages/driver-api/src/keyset.rs` | **新增** | 公开类型 + `lexicographic_seek_predicate` + 用例 | ~200（含 ~90 行测试） | 否 |
| `packages/driver-api/src/lib.rs` | 修改 | 加 `pub mod keyset;` | +1 | 否 |
| `packages/driver-api/src/traits.rs` | 修改 | 两个方法（逐字契约） | +22 | 否（现 1020 行，**已超限**，见下） |
| `packages/driver-api/src/reuse.rs` | 修改 | 转发两个能力位 + 转发用例 | +16 | **是（现 1159 行，已超限）**，见下 |
| `packages/driver-api/src/types.rs` | 修改 | `TableDataResult` 加可选 `paging` | +4 | 否（现 939 行，**已超限**，见下） |
| `packages/driver-api/src/mock_driver.rs` | 修改 | `MockDriverOptions` 加 `supports_keyset_pagination` / `keyset_predicate_returns_none`；`MockDriver` 覆盖两个方法 | +30 | 否（文件较大，需自查） |
| `src-tauri/src/services/table_paging.rs` | **新增** | `OrderSpec` / `keyset_decision` / `cursor_from_row` + 用例 | ~230（含 ~110 行测试） | 否 |
| `src-tauri/src/services/table_read_sql.rs` | **新增** | SQL 文本层（搬迁既有 5 个函数 + 新增 `build_paged_select_sql` + 既有/新增用例） | ~620（含 ~300 行测试） | **接近**：若超 800，再把 `format_condition` / `filter_is_complete` / `filter_join` 拆到 `table_filter_sql.rs` |
| `src-tauri/src/services/query_executor/mod.rs`（合并时已由 08 目录化） | 修改 | 只留编排：`get_table_data`（薄包装）+ 新增 `get_table_data_page` | **~340（08 目录化后）** → 搬迁 `table_read_sql` → **~150** → 加键集 **~240** | 拆分后**不再触及** |
| `src-tauri/src/services/mod.rs` | 修改 | 导出两个新模块 | +4 | 否 |
| `src-tauri/src/commands/schema.rs` | 修改 | IPC 接可选 `after_key`、回填 `paging` | +40 | 否（现 385 行） |
| `src/types/index.ts` | 修改 | 前端镜像类型 | +35 | 否 |
| `src/lib/keysetPaging.ts` | **新增** | 纯函数：栈、签名、失效、锚定 | ~180 | 否 |
| `src/lib/__tests__/keysetPaging.test.ts` | **新增** | 上述纯函数用例 | ~220 | 否 |
| `src/stores/tableData/types.ts` | 修改 | `TableState.keyset` | +3 | 否 |
| `src/stores/tableData/connectionState.ts` | 修改 | `emptyTableState` 缺省 | +2 | 否 |
| `src/stores/tableDataStore.ts` | 修改 | 请求带游标、落 `paging`、`setPage` 收紧 | +70 | 否（现 722 行 → ~790，**注意别越线**；若越线按「pendingChanges 相关动作拆 `tableData/actions.ts`」处理） |
| `src/commands/database.ts` | 修改 | `afterKey` 透传 | +6 | 否 |
| `src/components/DataTable/Pagination.tsx` | 修改 | `pagingHint` 渲染 | +35 | 否（现 137 行） |
| `src/components/DataTable/DataTable.tsx` | 修改 | 透传 `pagingHint` | +6 | 否 |
| `src/windows/connection/TableView.tsx` | 修改 | 从 store 组装 `pagingHint` | +15 | 否 |
| `src/lib/gridErrors.ts` | **修改（追加）** | 01 已建骨架；本册按契约 §8.1 三步追加 `grid.paging.*` 码，**禁止整文件覆盖** | +25 | 否 |
| `src/locales/en/query.ts` | 修改 | 第 8 节 key | +12 | 否（现 424 行） |
| `packages/drivers/{postgres,mysql,sqlite,sqlserver}/src/*.rs` | 修改 | 覆盖两个能力位 | 每驱动 +25 | 各驱动主文件需自查；sqlserver 主文件已较大，必要时把覆盖写进 `sqlserver/keyset.rs` 子模块 |
| `packages/drivers/{postgres,mysql,sqlite,sqlserver}/src/**/tests.rs` | 修改 | 驱动侧谓词用例 | 每驱动 +60 | 否 |
| `e2e/specs/table-keyset-paging.ts` | **新增** | 顺序翻页无重复、降级提示 | ~150 | 否 |
| `e2e/contract/journeys/run-extended.ts` | 修改 | 分页 journey | +30 | 否 |

### 6.1 必须处理的三个「已超限」文件（诚实记录）

1. `src-tauri/src/services/query_executor/`（**基线 797 行，但本册合并时 08 已完成目录化**，`mod.rs` 约 340 行、`tests.rs` 约 460 行）——本册**仍需**把 SQL 文本层整体搬出，否则加键集会越线。方案：把 SQL 文本层整体搬到 `table_read_sql.rs`，`QueryExecutor` 只留「取缓存 → 组装 → 发查询 → 组装结果」。搬迁后约 330 行，留出约 470 行余量。既有 `#[cfg(test)] mod tests` **随函数一起搬**，用例名不改（它们是 A1 的口径）。
2. `packages/driver-api/src/reuse.rs`（**现 1159 行，早于本册就超限**）——本册要加约 16 行。两个选择，**二选一但必须写进提交信息**：(a) 本册提交内先把 `impl DatabaseDriver for ReuseDriver` 整体搬到 `packages/driver-api/src/reuse/driver_impl.rs`（`reuse.rs` 变 `reuse/mod.rs` + `driver_impl.rs`，纯位移，转发用例跟着走），原始 1159 行降到约 550 + 约 620；(b) 记为既有技术债、本册只加 16 行，并在 `progress.md` 与提交信息里注明。**建议 (a)**：本册是「新增 trait 方法」的批次，转发块正是相关代码，顺手拆成本最低。
3. `packages/driver-api/src/types.rs`（**现 939 行**）、`packages/driver-api/src/traits.rs`（**现 1020 行**）——同样早于本册超限。本册只加 4 / 22 行，且都是契约要求的位置，**不建议在本册顺手拆**（拆 DTO 与 trait 的爆炸半径远大于收益）。记录为既有技术债（K-10），提交信息里注明「未新增超限文件，仅触碰既有超限文件」。

---

## 7. 边界与异常清单

> 逐条给出**期望行为**。带「测试」标记的条目必须在第 9 节有对应用例。

| # | 场景 | 期望行为 |
| --- | --- | --- |
| 1 | 第一页 | 不带游标；`paging.mode` 由后端裁决；若可用，`next_after_key` = 末行键值；`WHERE` 里没有键集片段 |
| 2 | 最后一页 | 键集请求返回行数 `< pageSize`（含 0）；`next_after_key` 仍按末行给出（下一頁为空即可判定终止）；分页器「下一页」在 `page + 1 >= totalPages` 时本来就禁用；**不得**因为空页把 `totalRows` 改成 0 |
| 3 | 只有一页 | 第一页即最后一页；`totalPages == 1`；不产生任何键集请求 |
| 4 | 空表 | `rows` 空、`total_rows == 0`；`paging` 仍可返回（`mode: 'offset'`、无 `next_after_key`）；分页器显示 `0-0 / 0`；**不得**出现 `NaN` / `Infinity`（`Pagination` 的 `Math.max(1, ceil(...))` 已兜底，回归时确认） |
| 5 | 单列键 | `(k1 > b1)` / `(k1 < b1)`；走 helper 或驱动的行值/单列实现 |
| 6 | 复合键 | 展开式（3.4）；宿主追加缺失主键列后 `order_columns.len() == boundary.len()`；长度不等 → 驱动返回 `None` → 降级 |
| 7 | 混合方向（`ORDER BY a ASC, b DESC`） | 展开式里第 1 项 `a > b1`、第 2 项 `a = b1 AND b < b2`；**驱动不得**使用行值比较（T-SQL 不支持，PG 语义也不等价）**测试** |
| 8 | 排序列可空 | `keyset_decision` 返回 `Unavailable { NullableKeyColumn }`；本次走 OFFSET；UI 提示「按可空列排序时无法快速翻页」；换成按主键排序后自动恢复**测试** |
| 9 | 排序列含 NULL（后端已误开能力） | 驱动见到 `Value::Null` 边界返回 `None` → 宿主降级 OFFSET 并丢弃游标；**绝不**发一个 `= NULL` / `> NULL` 的条件**测试** |
| 10 | 翻页中数据被删 | 下一页只是「更少的行」；若 `返回行数 < pageSize` 而按 COUNT 仍有后续页 → `drift_suspected = true`，UI 提示「行数已刷新」并触发一次重新计数（`skipCount: false`）；**不重复**已展示过的行 |
| 11 | 翻页中数据被插 | 插在游标之后的行的键更大 → 会在后续页出现（正确）；插在游标之前（键更小）→ 本轮永不出现（与 OFFSET 一致，不需要提示）；**不重复**已展示过的行**测试** |
| 12 | 每页条数变化 | `setPageSize` → 清栈、`anchorPage = 0`、`page = 0`、`skipCount: true`；下一次读重新裁决键集可用性（页大小不影响全序，因此通常仍可用） |
| 13 | 用户直接跳页 | `|target - page| > 1` → 本次 OFFSET + `reanchor(target)` + 提示「快速翻页已从第 N 页重新开始」；**不得**由页码推算游标**测试** |
| 14 | 筛选变化 | 签名变化 → `staleCursor` 处理：清栈、回第 0 页、重跑 COUNT（沿用既有 `applyFilters` 行为）；游标一律不发送 |
| 15 | 排序变化 | 同 14；另外重新裁决全序（换到可空列即降级，换到主键即恢复） |
| 16 | 驱动不支持 OFFSET（`supports_offset() == false`） | 默认 `pagination_syntax` 只给 `LIMIT n`。键集模式下 offset 恒为 0，**因此能正常翻页**（这是修正今天「page>0 重复第一页」的路径）；若该驱动未开键集 → 行为与今天一致（第 2 页仍是第一页），**本册不改这个既有缺陷**，只在 §2.2 记录 |
| 17 | 驱动不支持键集 | `supports_keyset_pagination() == false` → 宿主完全不构造游标，SQL 与今天逐字节相同；UI 不显示任何键集入口（可以在 tooltip 里解释「该数据库不支持快速翻页」） |
| 18 | 驱动给不出分页子句 | `PaginationSyntax.clause` 为空 → `NoPaginationClause` → 降级且**不**自己拼 `LIMIT`（宿主自拼等于把方言知识硬编码进宿主） |
| 19 | SQL Server（`requires_order_by: true`） | 键集模式永远有 `ORDER BY`，因此**不触发** `order_by_fallback`；断言 SQL 里**不出现** `(SELECT NULL)`，子句仍是驱动给的 `OFFSET 0 ROWS FETCH NEXT n ROWS ONLY`**测试** |
| 20 | 不可写表 / 视图 | 键集完全不依赖可写性；有稳定全序就能用（视图无主键 → 降级 `NoStableOrder`） |
| 21 | 面板在请求途中关闭 | `commitFetchedPage` 既有 `if (!current) return;` 生效；游标栈随 `removePanel` 销毁 |
| 22 | 会话失效 / 重新加载 | `invalidateCachedData` 会把 `columns`/`rows` 清空并 bump `requestRevision`；键集状态必须一并重置（否则残留游标会在新数据上生效） |
| 23 | 有暂存改动的行被键集翻页移出视野 | 暂存改动以行身份为键，翻页不丢；是否提示「有改动不在当前页」是 DB-06 的职责，本册不新增 |
| 24 | 超长值 / 多字节字符 / 二进制键 | 游标值经 `CursorValue` 带标签过线，**不做字符串化**；驱动用 `format_sql_literal` 内联（二进制 → 方言的十六进制字面量）；测试覆盖 `Bytes` 与含引号的字符串 |

---

## 8. i18n key 清单

**落点**：`src/locales/en/query.ts`（该文件已持有 `pagination.*`）。**不要**新建域名，**不要**改 `src/locales/en.ts`——它只有一行 `export { default } from './en/index'`，改它没有任何效果。其它语言文件开发期不动（发布前由 `scripts/i18n-sync-check.mjs` 与 i18n 同步流程补齐）。

| key（完整路径） | 英文文案 | 用途 |
| --- | --- | --- |
| `pagination.mode.keyset` | `Fast paging` | 键集生效时的分页器状态标签（可放 tooltip） |
| `pagination.mode.offset` | `Offset paging` | 降级时的状态标签 |
| `pagination.hint.driverUnsupported` | `This database does not support fast paging.` | `driverUnsupported` |
| `pagination.hint.noStableOrder` | `This table has no primary key, so pages cannot be read with a stable order.` | `noStableOrder` |
| `pagination.hint.nullableKeyColumn` | `Fast paging is unavailable while sorting by a column that can be NULL.` | `nullableKeyColumn` |
| `pagination.hint.nullBoundary` | `This row's sort key is NULL, so fast paging restarted from an exact position.` | `nullBoundary` |
| `pagination.hint.predicateUnavailable` | `Fast paging is unavailable for the current sort order.` | `predicateUnavailable` |
| `pagination.hint.noPaginationClause` | `This database cannot paginate this query.` | `noPaginationClause` |
| `pagination.hint.staleCursor` | `Sort or filter changed; paging restarted from the first page.` | `staleCursor` |
| `pagination.hint.jumpReanchored` | `Fast paging restarted from page {page}.` | `notAdjacent`（跳页后重新锚定） |
| `pagination.hint.drift` | `Rows changed while paging; the row count has been refreshed.` | `driftSuspected` |
| `pagination.hint.unstableOrderWarning` | `未指定排序，翻页结果不保证稳定` 的英文对应文案 | DB-09 的稳定性提示**共用**此 key（09 分册若不认领，本册代为定义） |

**错误码**（后端消息以契约前缀开头，前端 `classifyGridError` 前缀匹配，未知则回退显示原始消息）：

| 错误码前缀 | 触发 | 前端处理 |
| --- | --- | --- |
| `grid.paging.keysetUnavailable: ` | 客户端显式要求键集但后端无法满足（如 `paging` 元信息与请求不一致） | 归一到 `staleCursor` 文案并清栈 |
| `grid.paging.cursorInvalidated: ` | 游标长度/列序与当前排序不符 | 清栈、回第 0 页、显示 `pagination.hint.staleCursor` |
| `grid.paging.cursorTooDeep: ` | 游标栈超过 `KEYSET_STACK_LIMIT` | 改用 OFFSET 跳转并重新锚定，显示 `pagination.hint.jumpReanchored` |

> 三个错误的**可见文案一律走上面的 `pagination.hint.*`**，不新增 `errors.*` 域名下的键（避免给 DB-04/08 正在收敛的错误码体系再开一套命名）。`classifyGridError` 若在 DB-04/08 中已落地，本册只**追加**分支；若尚未落地，本册创建该文件时**只**实现 `grid.paging.*` 与「未知回退」两条规则，其余前缀留给后续分册。

---

## 9. 测试清单

> 每个用例给「名 + 断言要点」。**H 组用例名里的既有 5 个锚点不许改**（A1 口径）。

### 9.1 Rust · 契约与共享算法（`cargo test -p datazen-driver-api --lib`）

| 组 | 用例名 | 断言要点 |
| --- | --- | --- |
| D1 | `lexicographic_seek_predicate_single_ascending` | `[("id",false)]` + `[(id, 42)]` → `("id" > 42)` |
| D2 | `lexicographic_seek_predicate_single_descending` | `[("id",true)]` → `("id" < 42)` |
| D3 | `lexicographic_seek_predicate_composite_uniform` | `[(a,false),(b,false)]` → `((a > 1) OR (a = 1 AND b > 2))`；字面量顺序与 `order_columns` 一致 |
| D4 | `lexicographic_seek_predicate_mixed_directions` | `[(a,false),(b,true)]` → `((a > 1) OR (a = 1 AND b < 2))` |
| D5 | `lexicographic_seek_predicate_rejects_null_and_length_mismatch` | `Value::Null` 边界 / 长度不等 / 空 `order_columns` → 全部 `None` |
| D5b | `cursor_value_round_trip_is_lossless_for_array_shaped_values` | `Json([1,2,3])` 与 `Bytes([1,2,3])` 过线后**各自仍是原变体**（untagged `Value` 会读混，这条用例是防它回来的钉子） |
| D6 | `default_keyset_capability_is_false_and_predicate_is_none` | 一个只实现必备方法的测试驱动 → `supports_keyset_pagination() == false`、`keyset_predicate_sql(任意) == None`（**兼容性门禁**） |
| D7 | `reuse_driver_forwards_keyset_capability` | 内层驱动返回 `true` / `Some(..)` 时，`ReuseDriver` 必须透出同样的值（转发遗漏守卫，与 `reuse_driver_forwards_*` 同风格） |

### 9.2 Rust · 宿主 SQL（`cargo test -p datazen --lib`）

| 组 | 用例名 | 断言要点 |
| --- | --- | --- |
| H0 | **既有 5 个锚点（不得改名/删除）** | `tsql_pagination_emits_offset_fetch_and_never_limit`、`no_explicit_sort_uses_primary_key_order`、`no_explicit_sort_composite_pk_uses_all_pk_columns`、`explicit_sort_overrides_default_pk_order`、`no_pk_no_explicit_sort_still_has_order_by_first_column`；另加 `tsql_pagination_keeps_pk_order_without_fallback`、`build_select_sql_without_offset_when_unsupported` |
| H7 | `offset_path_sql_is_byte_identical_to_pre_keyset_golden` | 对 5 种形状（PG `LIMIT/OFFSET`、T-SQL `OFFSET…FETCH`、无 OFFSET 的 `LIMIT n`、显式排序、无主键兜底）逐字比对**整条 SQL 字符串**；这是「默认实现全 false 时旧行为逐字节不变」的机器证明 |
| H8 | `keyset_predicate_is_joined_with_and_after_user_filters` | `filter_logic = "or"` 时输出 `WHERE (f1 OR f2) AND (<seek>)`——**绝不能**变成 `… OR <seek>` |
| H9 | `keyset_always_orders_by_appended_primary_key` | 显式 `sorts = [name ASC]` + 主键 `id` → `ORDER BY "name" ASC, "id" ASC`；`boundary` 长度 2 |
| H10 | `keyset_uses_pagination_clause_with_zero_offset` | 断言 SQL 里是驱动给的 `clause`（如 `LIMIT 50 OFFSET 0`）且**宿主未自拼 `LIMIT`**；`clause` 为空 → 返回降级 |
| H11 | `tsql_keyset_never_emits_select_null_fallback` | `requires_order_by: true` + 键集 → 有 `ORDER BY`、无 `(SELECT NULL)`、子句为 `OFFSET 0 ROWS FETCH NEXT 50 ROWS ONLY` |
| H12 | `keyset_predicate_none_degrades_to_offset_for_this_request` | 驱动返回 `None` → `mode == Offset`、`unavailable_reason == Some(PredicateUnavailable)`、SQL 与 H7 的黄金串相同 |
| H1 | `keyset_decision_uses_primary_keys_as_stable_total_order` | 主键 `[id]`、无显式排序 → `Eligible`，`order_columns == [id ASC]` |
| H2 | `keyset_decision_appends_missing_primary_key_columns` | 显式 `sorts = [created_at DESC]`、主键 `[id]` → `order_columns == [created_at DESC, id ASC]` |
| H3 | `keyset_decision_rejects_table_without_primary_key` | 无主键（`primary_keys` 空）→ `NoStableOrder`；`allow_first_column_fallback_order` 相关行为由 09 决定，本册只断言「第一列兜底**不构成**全序」 |
| H4 | `keyset_decision_rejects_nullable_sort_column` | `created_at` 的 `nullable == true` → `NullableKeyColumn` |
| H5 | `keyset_decision_composite_primary_key_all_columns_appended` | 主键 `[order_id, product_id]`、显式 `sorts = [quantity ASC]` → `order_columns == [quantity ASC, order_id ASC, product_id ASC]` |
| H6 | `cursor_from_row_reads_last_row_in_order_column_order` | 混合方向下取值仍按 `order_columns` 顺序；末行某列 `None` → 返回 `None`（触发降级） |
| H13 | `get_table_data_page_returns_keyset_metadata_when_mock_supports_it` | mock 开能力 → `paging.mode == Keyset`、`next_after_key.is_some()`、`order_columns` 与 H9 一致 |
| H14 | `get_table_data_page_reports_offset_when_mock_lacks_capability` | mock 关能力 → `paging.mode == Offset` + `unavailable_reason == DriverUnsupported`，且数据 SQL 等于今天的黄金串 |
| H15 | `get_table_data_page_ignores_cursor_when_decision_is_unavailable` | 传游标但表无主键 → 不把游标拼进 SQL，回 `StaleCursor` 或 `NoStableOrder`，并返回第 0 页 |
| H16 | `stale_cursor_signature_mismatch_returns_page_zero` | 游标 + 排序已变 → `page == 0`、`mode == Offset`、`unavailable_reason == StaleCursor` |
| H17 | `get_table_data_without_after_key_still_compiles_and_behaves` | 走**旧签名** `get_table_data`（无游标参数）→ 与 H7 黄金串一致（保护「签名不变」这条铁律） |

> H7/H14/H17 三条合起来就是「**无键集能力的驱动行为逐字节不变**」的完整证明：一条证明宿主没改生成逻辑，一条证明能力位为假时走旧路径，一条证明旧入口签名仍可用。
>
> `MockDriver` 需要两个新 option：`supports_keyset_pagination: bool`（默认 `false`）与 `keyset_predicate_returns_none: bool`（默认 `false`）。它们必须**同时**加进 `MockDriverOptions::default()`；全仓 70+ 处 `MockDriverOptions { … }` 字面量里若有未带 `..Default::default()` 的，会编译失败——**编译通过即证明没漏**。

### 9.3 Rust · 驱动 crate（`cargo test -p datazen-driver-<id>`）

| 组 | 用例名 | 断言要点 |
| --- | --- | --- |
| P1/M1 | `keyset_predicate_uses_row_value_comparison_for_uniform_ascending` | PostgreSQL / MySQL：`((a, b) > (1, 'x'))`（同向且无 NULL 时） |
| P2/M2 | `keyset_predicate_falls_back_to_or_chain_for_mixed_direction` | `[(a,false),(b,true)]` → 展开式；**不得**出现行值比较 |
| S1 | `keyset_predicate_is_or_chain` | SQLite 一律展开式（不依赖行值比较的版本门槛） |
| T1 | `sqlserver_keyset_predicate_never_uses_row_value_comparison` | 组合键 + 混合方向 → 展开式；正则/子串断言不出现 `(a, b) >` 形状 |
| T2 | `sqlserver_keyset_pagination_clause_is_offset_fetch_with_zero_offset` | 键集 SQL 里子句仍是 `OFFSET 0 ROWS FETCH NEXT n ROWS ONLY`、不出现 `LIMIT`、不出现 `(SELECT NULL)` |
| ALL | `keyset_predicate_returns_none_for_null_boundary` | 每个开启能力的驱动各一条：`Value::Null` → `None` |
| ALL | `capability_flag_matches_default` | 未开启能力的驱动（redis / mongodb / duckdb / clickhouse 等）`supports_keyset_pagination() == false` |

### 9.4 Host 前端单测（`npx vitest run`）

| 组 | 文件 / 用例名 | 断言要点 |
| --- | --- | --- |
| F1 | `src/lib/__tests__/keysetPaging.test.ts::cursor_for_page_returns_null_for_unvisited_page` | 未访问页 → `null`（调用方必须降级） |
| F2 | `…::push_cursor_stops_at_stack_limit` | 第 `KEYSET_STACK_LIMIT + 1` 次 push 后栈长不增、返回「需重新锚定」 |
| F3 | `…::reanchor_truncates_stack_and_moves_anchor` | `reanchor(500)` → `stack == [null]`、`anchorPage == 500` |
| F4 | `…::signature_change_invalidates_cursor` | 排序或筛选签名变化 → `nextRequestCursor` 返回 `fallbackToOffset: true` |
| F5 | `…::mixed_direction_order_columns_are_preserved` | 后端给的 `orderColumns` 原样保留，不在前端重排 |
| F6 | `…::null_boundary_is_never_sent` | 后端 `nextAfterKey` 缺失/含 `kind: 'null'` → 不压栈、下一次请求不带游标 |
| F7 | `src/stores/__tests__/tableDataStore.keyset.test.ts::next_page_sends_cursor_from_previous_page_last_row` | 旅程：第 1 页 → 下一页（断言 `getTableData` 收到的 `afterKey` 等于第 1 页 `nextAfterKey`）→ 再下一页（用第 2 页的边界） |
| F8 | `…::prev_page_reuses_stacked_boundary` | 上一页 → `afterKey` 等于该页起始边界；不得重新计算 |
| F9 | `…::sort_change_clears_stack_and_resets_to_page_zero` | `setSort` 后 `keyset.stack == []`、`anchorPage == 0`、`page == 0` |
| F10 | `…::non_adjacent_page_falls_back_to_offset_and_reanchors` | `setPage(panelId, 7)` → 本次请求无 `afterKey`、`anchorPage == 7`；**断言没有把页码换算成游标** |
| F11 | `…::degrade_reason_from_response_is_surfaced` | 响应带 `unavailableReason: 'nullableKeyColumn'` → `TableState.keyset.unavailable` 写入、`mode === 'offset'` |
| F12 | `…::stale_response_cannot_overwrite_newer_page` | 复用既有 `requestRevision` / `loadingRevision`：旧响应到达时不写 `rows`、不改 `keyset` |
| F13 | `src/components/DataTable/__tests__/Pagination.keyset.test.tsx::renders_degrade_hint` | `pagingHint.reason = 'noStableOrder'` → 渲染对应文案（用 i18n key 断言，不硬编码英文串） |
| F14 | `…::no_jump_affordance_is_rendered` | 控件里**没有**页码输入框/首页/末页按钮（与 C-47 未落地的现状一致） |
| F15 | `…::drift_hint_is_rendered` | `driftSuspected` → 提示存在；不阻塞翻页按钮 |

### 9.5 E2E

| 文件 | 旅程 | 断言 |
| --- | --- | --- |
| `e2e/specs/table-keyset-paging.ts` | 打开有主键的表 → 记录第 1 页首行主键 → 下一页 → 记录首行主键 → 再下一页 → 上一页 | 相邻页主键集合无交集（顺序翻页不重复）；回到第 1 页时内容与首次一致 |
| 同上 | 按可空列排序 → 翻页 | 出现降级提示文案；翻页仍可用；**没有**出现跳页入口 |
| 同上 | 通过 IPC 直接调 `get_table_data`（不带 `afterKey`） | 成功返回（老调用形状兼容） |
| `e2e/specs/table-data.ts`（扩展） | 既有分页旅程 | 仍通过（回归） |
| `e2e/contract/journeys/run-extended.ts` | 各驱动的顺序翻页 journey | 无重复行；结论中写明本次构建实际链接的驱动集 |

---

## 10. 自查清单

> 用法：开工前读一遍，提交前逐条打勾。每条的格式是「错误做法 → 正确做法 → 后果」。

| # | 错误做法 | 正确做法 | 后果（真实会发生什么） |
| --- | --- | --- | --- |
| 1 | **以为翻页不需要稳定全序**：`ORDER BY name` 就用 `name > :last` 翻页 | 先裁决全序：排序列集合必须包含唯一键的全部列；不含就把主键**追加**进 `ORDER BY`，追加不了就降级 | 有并列值的表在翻页时**静默跳行/重复行**；用户以为数据丢了，且没有任何报错 |
| 2 | **把 OFFSET 的页码语义直接套到键集上**：`boundary = f(page)`，或把 `page × pageSize` 当成键集参数 | 边界只能来自**真实读到的行**；页码只是「访问过的第几页」这个序数；非相邻页一律 OFFSET + 重新锚定 | 第 N 页返回一段无意义的行区间（既不是第 N 页也不是任何一页），而且看起来很「正常」，只有对账时才会发现 |
| 3 | 让前端从 `rows` 里自己按列名算边界 | 边界由**后端**从它自己返回的行里截取，前端原样回传 `CursorValue[]` | 列名随 `visibleColumns` / 列拖拽变化，前端算出的边界会错列；错列之后谓词合法但语义错，最难查 |
| 4 | 用裸 `Value` 承载游标过线 | 用带 `kind` 标签的 `CursorValue`（3.2） | `Value` 是 untagged：`Json([...])` 回传后被读成 `Bytes`，谓词从 `'{"a":1}'` 变成 `X'7b22...'`，边界悄悄错位 |
| 5 | 在宿主里按数据库名分支（`if driver_type == "sqlserver"` 就走展开式） | 形态差异写进驱动的 `keyset_predicate_sql`；宿主只调 trait 方法 | 违反零硬编码铁律，评审直接打回；且 git 驱动永远拿不到正确形态 |
| 6 | 拿到 `None` 就自己拼一个「兜底谓词」或直接忽略游标继续查 | `None` = 本次降级 OFFSET，并带 `unavailableReason` 给 UI | 「尽力而为」的谓词会返回**缺行的页**，比降级慢一点严重得多 |
| 7 | 用 `format!("... {} ...", value)` 自己插值 | 一律经 `driver.format_sql_literal`（helper 已内建） | SQL 注入面 + 二进制/时间值渲染错误；这就是本路径必须走驱动的原因 |
| 8 | 可空列也开键集（或加个 `IS NULL` 分支了事） | 规则 N：任一可空列 → `NullableKeyColumn` 降级 | NULL 不参与大小比较，跨过 NULL 区块时**整段行消失**；`IS NULL` 分支还要求知道方言的 NULL 排序位置，而契约里没有这个能力位 |
| 9 | 顺手把 `supports_keyset_pagination` 的默认值写成 `true`，或「先开能力再补实现」 | 默认 `false` / `None`，先落地默认实现与黄金 SQL 回归，再逐驱动开启（Step 1→2→7） | 所有驱动瞬间改走键集路径；没有全序的表开始漏行，且这是**默认行为变更**，违反 R2 与 A1 |
| 10 | 排序/筛选变化后继续用旧游标 | 签名不一致即 `staleCursor`：清栈、回第 0 页、不发游标 | 用旧边界在新排序上定位 → 页内容不可预测；用户会看到「换个排序，数据全乱了」 |
| 11 | 让分页器显示「第 N 页」却暗示可跳页 | 键集模式下明确「只能顺序前后翻」；跳页要么不给入口，要么走 OFFSET 并提示已重新开始 | 用户点跳页得到错页，会判定整个网格不可信 |
| 12 | 为了「省一次 COUNT」把 `totalRows` 在键集翻页时清零 | 沿用既有 `skipCount: true` 语义：翻页不重跑 COUNT，`totalRows` 保留 | 分页器变成 `NaN` / 总页数跳动，`Pagination` 的页标签与范围全乱 |

---

## 11. 未决问题

> 全部为**需人工裁定**项，逐条给建议选项；未裁定前按「建议」实现，并在提交信息里标注待裁定（[总纲](../data-browsing-design.md) §6 明确「未决问题不阻塞交付」）。

| # | 问题 | 建议选项 | 建议 |
| --- | --- | --- | --- |
| K-1 | **跳页的产品决策**：`setPage` 今天接受任意页码但 UI 没有跳页入口（C-47 为 P2）。键集落地后如何处置？ | (a) 禁用跳页入口 + 非相邻页走 OFFSET 并重新锚定；(b) 纯键集、非相邻页直接拒绝；(c) 只允许跳「访问过的页」 | **(a)**。它让今天不存在的入口在未来出现时也是诚实的（慢但正确），且不需要服务端状态；(b) 会让 `setPage` 在健壮性上倒退（今天能跳到第 N 页）；(c) 不能覆盖「跳到没访问过的第 500 页」 |
| K-2 | **可空排序列**：降级（本册规则 N）还是实现 `IS NULL` 分支？ | (a) 降级；(b) 给 `DatabaseDriver` 加 `null_ordering()` 能力位后支持 | **(a)**。00 契约第 5.5 节已冻结，「加 `null_ordering()`」是改契约、会连带所有 git 驱动；且 (b) 的方言矩阵（PG/MySQL/SQLite/SQL Server 的 NULL 前后各不相同）需要逐库 live 测试，成本远超收益 |
| K-3 | **本册交付范围**：「接口预留」还是「完整实现」？ | (a) 只做 Step 1~5（后端能力 + 前端状态，UI 不提示）；(b) Step 1~9 全做，PG/MySQL/SQLite/SQL Server 四个驱动开启 | **(b)**，但必须保留 C1（Step 1~2）作为**可独立合并的契约提交**：总纲 §4.2 的提交 12 据此可再拆成「契约」与「实现」两条，契约那条先独占主干。理由：PRD 已把 C-48 定为 P0，且本册是唯一能同时解决「深翻页慢」与「默认排序贵」的路径 |
| K-4 | **`TableDataResult` 加字段 vs 宿主包一层**：加字段会要求**外部 git 驱动**若要构造该结构体就补字段（本仓内只有宿主构造） | (a) 加可选字段（本册方案）；(b) 宿主新增 `TablePageResponse` 包一层，driver-api 的 DTO 一字不动；(c) 新增独立命令返回 `paging` | **(a)**。与仓内既有先例一致（`ForeignKeyInfo::deferrability` 就是给既有 DTO 加 `#[serde(default)]` 字段）；(b) 会改动 `get_table_data_impl` 的返回类型，波及既有宿主测试；(c) 需要前端发两次请求。**若 CI 在 `--drivers=all` 下暴露 git 驱动编译失败，则改走 (b)** |
| K-5 | **「上一页」实现**：有界游标栈 vs 反向 seek（`ORDER BY` 全反向 + 结果倒序） | (a) 游标栈（`KEYSET_STACK_LIMIT = 200`）；(b) 反向 seek | **(a)**。(b) 省内存但引入「方向取反 + 行倒序 + 空页边界」三处新逻辑，且混合方向下反向规则的测试矩阵会翻倍；(a) 的内存上限可算（200 × 键宽，可忽略），且栈正好也是 K-1 的锚点载体 |
| K-6 | **游标值内联 vs 参数化**：`query_at` 没有绑定参数，因此本册让驱动内联字面量；但 `packages/data-sync/src/keyset.rs` 已经证明参数化是可行的 | (a) 内联字面量（本册方案，与筛选路径一致）；(b) 把浏览读切到 `query_with_params_at`，并给驱动加「占位符渲染」约定 | **(a)**。00 契约第 5.5 节的签名只回传 `Option<String>`、不回收参数，参数化需要额外约定占位符顺序；筛选值今天也是内联的，改路径属另一批次。(b) 可作为后续「值与字面量彻底分离」的独立方案 |
| K-7 | **稳定全序的唯一键来源**：`CachedColumns` 没有唯一索引信息，今天只能用主键；契约第 1.6 节还要求用 `effective_primary_keys()` 而不是自己过滤 `is_primary_key`，而 `build_select_sql` 今天恰恰是自己过滤 | (a) v1 只用主键且沿用产出 `ORDER BY` 的那一份（本册方案）；(b) 扩展 `SchemaCache` 拿 `TableSchema.indexes`（`IndexInfo::is_unique`），让唯一索引也能支撑键集；(c) 顺手把 PK 来源收敛到 `cached.primary_keys` | **(a)** 先落地；**(c) 归 09 分册**（它拥有默认排序策略，且「收敛 PK 来源」会改变既有 SQL，必须有独立回归）；**(b)** 记为后续增强——它能让「按 `email` 排序」这类常见诉求也用上键集 |
| K-8 | **漂移提示策略**：`driftSuspected` 只在「本页行数 < 每页条数但 COUNT 说还有后页」时置真；是否要做更主动的漂移检测（翻页时重算 COUNT） | (a) 只做这个廉价信号 + 一次按需重算；(b) 每次键集翻页都重算 COUNT | **(a)**。翻页时重算 COUNT 会把「深翻页快」这件事重新拖慢（既有 `skipCount: true` 就是为省这次 COUNT 才存在的）；(b) 的收益只是提示更及时 |
| K-9 | **与 09 的落点一致性**（**已核对，非未决**）：稳定全序的唯一键来源由 **09 拥有**——本册 K-7 的方案 (c)「把 PK 来源收敛到 `cached.primary_keys`」已明确归 09，因为它会改变既有 `ORDER BY` SQL，必须有独立回归；同时 09 的「不包含」小节把键集分页整体划归本册。⇒ **依赖是单向的**（本册 → 09），没有环。实施约束：① 冻结提交顺序里 `09` 排在 `11` **之前**（见总纲第 4 节），所以 09 必须**只依赖列元数据、不引用任何键集类型**，否则两册任意颠倒顺序都会编不过；② 两册共用同一批「不稳定排序」i18n key，**以 09 的 key 为准**，本册不再另起名 |
| K-10 | **既有超限文件**：`reuse.rs`(1159) / `types.rs`(939) / `traits.rs`(1020) 早于本册就超过 800 行；本册必须往其中两三个文件加行 | (a) 本册顺手拆 `reuse.rs`（转发块位移）；(b) 全部记为既有技术债，本册只加行 | **(a) 仅对 `reuse.rs`**（本册改的正是转发块，位移成本最低、收益最直接）；`types.rs` / `traits.rs` **建议 (b)**（拆 DTO 与 trait 的爆炸半径远大于收益），但要在提交信息里明确「未新增超限文件，仅触碰既有超限文件」，避免被误判为本册引入 |
| K-11 | **`en.ts` 口径冲突**：仓库 `AGENTS.md` 的 i18n 规则与总纲 §6 DoD 第 9 条写「新增 key 只在 `en.ts`」，但已核实 `src/locales/en.ts` 只有 `export { default } from './en/index'`（两行），改它无效；00 契约第 8 节与总纲 §5.4 已改为「领域包」 | (a) 按领域包执行，并把两处旧表述按事实修正；(b) 按旧表述改 `en.ts` | **(a)**。本册只改 `src/locales/en/query.ts`（第 8 节）。修正旧表述不在本册授权范围（不得改其它文档），因此**在此登记**，请文档 owner 收口 |
