# 08 · 筛选能力与命名视图（DB-08）

> **状态**：目标设计，**未实现**。本文件描述的是要落地的形态；仓库里今天不存在 `SHARED_FILTER_OPERATORS`、`supported_filter_operators`、`filter_operator_sql`、`like_is_case_insensitive`、`dataGrid` 能力位、`gridErrors.ts` 与任何命名视图代码。文中凡新增符号一律标注「（新增）」。
>
> **依赖**：[`00-contracts.md`](./00-contracts.md)。本册直接消费它的第 1.5 节（宿主 SQL 生成的现状）、第 1.6 节（`FilterOperator` 真实定义与前端两份重复定义）、第 5.2 节（**算子集与三个 trait 方法的最终签名，不得改动**）、第 6.1 / 6.2 节（IPC 命令与新增可选字段）、第 7 节（`DataGridCapabilities`）、第 8 节（错误码与 i18n 落点）。契约与 PRD 冲突时以契约为准，冲突处一律进第 11 节。
>
> **被谁依赖**：DB-15（筛选 SQL 预览与「按当前筛选导出」——预览文本必须与本册生成的语句逐字节同源）；DB-07（外键跳转预置筛选必须走 `setFilters` 的**可见结构化条件**，不得退化成隐形 raw SQL）；DB-11（多列排序写入本册 `NamedView.sorts`，所以 DTO 用数组）；DB-09（筛选与行数三态的组合规则，见 §7「筛选后行数统计」）；分册 12（门禁）。
>
> **与已落地的相邻分册对齐（撰写期间 05 / 09 已出现在同目录，本册已读并按其事实改写）**：
> - **分册 05（类型化编辑器）** 已定义 `src/lib/cellTypes.ts` 的 `CellType` / `resolveCellType` / `cellTypeToDataTypeFamily` / `CellTypeHints`，并把 `classifyDataType` 改为 `cellTypeToDataTypeFamily(resolveCellType(...))`。因此本册的**算子默认集按 `CellType` 分派**（§3.5），不再自行按 `DataTypeFamily` 猜；驱动经 `DatabaseTypeMeta.cellTypes` 声明的类型提示同时约束编辑控件与筛选算子集。
> - **分册 09（行数三态与默认排序）** 已把「有生效筛选时禁止用估算行数」写进它的验收点 A7，并在其 §7 用 `has_effective_filters` + `filter_is_complete` 落地。本册 §7 只做**引用与耦合声明**，不再自行归口（原 Q-11 已改写）。
> - **共用文件的去重（重要）**：`src/lib/gridErrors.ts`、`src/lib/databaseMeta.ts` 的 `DataGridCapabilities`、`src-tauri/src/services/query_executor.rs` 的拆分、`src/stores/tableDataStore.ts`、`src/windows/connection/TableView.tsx` 这五处 **08 与 09 都会动**。本册在 §6 逐条标注「只建一次 / 净增预算 / 已超限」。其中 `TableView.tsx` 是本册发现的**真实超限冲突**（770 + 09 的约 60 + 本册约 40 > 800），见 Q-13。
>
> **预估人日**：约 10.5 人日（不含跨驱动 E2E 全矩阵跑批）。Rust 契约与默认实现 1；`filter_sql` 拆分与 19 算子渲染 2；宿主校验与错误码 0.5；命名视图 store + IPC 1.5；前端算子元数据（按 `CellType`，与 05 对齐）与值控件 2；命名视图 UI 1；`TableView` 拆分（与 09 的 800 行冲突，Q-13）0.5；测试（Rust 单测 / 前端单测 / 连续旅程 / E2E）2。
>
> **本文档不包含什么**：
> - **`Raw SQL` 自由条件**（PRD DB-08 目标行为第 5 条）。契约第 5.2 节的算子分组表是「最终定义，禁止分册自行增删」，其中没有 `RawSql`；且 raw SQL 是 WHERE **片段**而非算子，与算子模型正交，必须和 `sql_guard` 的注释剥离 / 危险语句扫描一起设计。冲突记入第 11 节 Q-9。
> - **`Any column` 全列搜索**（PRD 目标行为第 4 条）：它生成 `col1 ILIKE x OR col2 ILIKE x …`，本质是「动态展开成 N 条 OR 条件」，依赖逐条连接符；本册不实现逐条连接符（见下条）。
> - **逐条 AND/OR 与嵌套布尔分组**：契约第 1.5 节确认 `filter_join` 今天只有全局单一逻辑，且契约未要求本册改它。本册对嵌套分组**只做类型预留、不做实现**，理由与接口见 §3.6 与 Q-2。
> - **按表记忆上次筛选状态**（PRD DB-08 目标行为第 7 条前半：`tableViewPrefs` + LRU）：见 Q-6，建议拆为独立可回滚单元。
> - **筛选 SQL 预览 / 按当前筛选导出直达**：DB-15。
> - **`count` 三态与默认排序的具体实现**：DB-09；本册只声明它与筛选的**组合规则**（§7）。
> - **AI 自然语言筛选**（`NlFilterInput` / `ai_parse_filter`）：已存在，本册不改。
> - **多列排序 UI**：DB-11。

---

## 1. 目标与验收口径

**一句话目标**：把筛选从「10 个算子 + 全局单一 AND/OR + 关标签即丢」提升到「19 个宿主可渲染算子 + 3 个按驱动能力开放的方言算子 + 按列类型给默认算子 + 可命名、可持久化、可导入导出的筛选视图」，同时保证**既有 10 个算子的 SQL 输出逐字节不变**，并且把「不完整筛选被静默丢弃」改成**显式拒绝**。

### 1.1 验收口径（命令 + 期望）

| 编号 | 命令 | 期望 |
| --- | --- | --- |
| A1 | `cargo test -p datazen --lib` | 全绿；其中 `services::filter_sql` 与 `services::query_executor` 的筛选用例必须包含 legacy 10 算子的 `assert_eq!` 全串断言（不是 `contains`） |
| A2 | `cargo test -p datazen-driver-api --lib` | 全绿；含「三个新 trait 方法的默认实现 == 今天的行为」用例 |
| A3 | `npx vitest run` | 全绿；`src/lib/__tests__/filterOperators.test.ts`、`src/components/__tests__/FilterEditor.test.tsx`、`src/components/__tests__/FilterEditorJourney.test.tsx`、`src/lib/__tests__/namedViews.test.ts`、`src/lib/__tests__/gridErrors.test.ts`、`src/stores/__tests__/tableDataStore.test.ts` 全部通过 |
| A4 | `pnpm typecheck` | 干净。**测试文件参与类型检查**，所以新测试里的 mock 必须用精确类型（`satisfies` / `Pick<>` / `as unknown as X`），禁止 `any` |
| A5 | `pnpm e2e:minimal` | `e2e/specs/table-filter.ts`（含改写后的 TF-008）与新增 `e2e/specs/table-saved-views.ts` 全绿。DB-08 的验收驱动集要求 PG / MySQL / SQLite，三者都在 basic 集合内，故用 `:minimal` 即可 |
| A6 | 人工 | 关标签再开、重启应用后：命名视图仍可切换到完全相同的筛选 + 排序 + 可见列 |

> 门禁纪律（照抄仓库 `AGENTS.md`）：测试类命令的输出动辄上千行，结论在末尾，一律重定向到系统临时目录再 `tail` / `grep` 取片段，退出码单独打印；**临时文件不得落在仓库内**。本册所有命令与结论均为**设计期望**，撰写时未运行任何测试或构建。

### 1.2 兼容性验收（本册最重要的一条）

| 编号 | 断言 |
| --- | --- |
| B1 | 对既有 10 个算子，`QueryExecutor::build_select_sql` / `build_count_sql` 产出的 SQL 与今天**逐字节相等**（`assert_eq!` 全串，见 §9 的 golden 用例） |
| B2 | 对既有 10 个算子，前端算子在选项列表里的**相对顺序与显示文案逐位不变**（`eq ne gt lt gte lte like in isNull isNotNull`） |
| B3 | 不做任何改动、也不覆盖任何新方法的驱动（含全部 path 驱动与 git 驱动）：`supported_filter_operators()` 返回共享算子集，`filter_operator_sql()` 返回 `None`，`like_is_case_insensitive()` 返回 `false` —— 行为与今天一致 |
| B4 | 不提升 `PROTOCOL_VERSION`（只新增带默认实现的方法），不修改任何既有方法签名 |

---

## 2. 现状代码事实

只列**亲自读过**的符号；位置一律「文件路径 + 符号名」，全文无行号。

### 2.1 驱动契约（`packages/driver-api`）

- `packages/driver-api/src/filters.rs`：`FilterCondition { column, operator, #[serde(default)] value }`；`FilterOperator` 恰好 10 个变体 `Eq / Ne / Gt / Lt / Gte / Lte / Like / In / IsNull / IsNotNull`，带 `#[serde(rename_all = "camelCase")]`。**没有** `PartialEq` / `Eq` / `Copy` derive。
- `packages/driver-api/src/traits.rs`：`DatabaseDriver` trait 本体；同目录 `packages/driver-api/src/traits/` 下已有 `sql_text` / `streaming` / `command_api` / `key_value` / `schema_target` 子模块，`traits.rs` 用 `mod` + `pub use` 再导出。既有的引用与字面量入口是 `quote_ident` / `format_sql_literal` / `pagination_syntax` / `skip_count_query` / `supports_offset` / `has_schema_level`。
- `packages/driver-api/src/mock_driver.rs`：`MockDriver`、`MockDriverOptions`。**它没有覆盖任何筛选相关方法**——这正是「默认实现 == 今天行为」的可测基座。宿主通过 `src-tauri/src/testing/mod.rs` 的 `pub use datazen_driver_api::mock_driver;` 以 `crate::testing::mock_driver` 使用它。

### 2.2 宿主 SQL 生成（`src-tauri/src/services/query_executor.rs`，797 行）

- `SortCondition { column, #[serde(default)] descending }`、`OrderBy { column, descending }`。
- `pub use datazen_driver_api::filters::{FilterCondition, FilterOperator};` —— 宿主与驱动看到**同一个类型**。
- `QueryExecutor::get_table_data(...)`（12 个参数，含 `filters`、`order_by`、`skip_count`、`filter_logic`）：先取 `schema_cache.get_columns`，再 `build_select_sql`，`skip_count` 为真时只发 SELECT，否则 `tokio::try_join!` 同时发 count 与 data。
- `QueryExecutor::build_count_sql(table_name, columns, filters, qi, format_lit, filter_logic) -> String`：方法体第一行 `let _ = columns;`（**有意忽略 columns**），产出 `SELECT COUNT(*) FROM t WHERE …`，WHERE 的拼接是 `map(format_condition).filter(|s| !s.is_empty()).join(filter_join(logic))`。
- `QueryExecutor::build_select_sql(table_name, columns, filters, order_by, qi, format_lit, pagination, filter_logic) -> String`：没有显式排序时注入 `ORDER BY <主键列…> ASC`，无主键退化为**第一列**；`pagination.requires_order_by` 为真且无 ORDER BY 时补 `order_by_fallback`；分页子句原样追加。
- `QueryExecutor::format_condition(condition, qi, format_lit) -> String`：先 `filter_is_complete` 判定，**不完整直接 `return String::new()`**（注释写明原因：驱动会把 `''` 强转到整型主键导致整表加载失败）；`In` 分支支持 `Value::Json(Array)` 与逗号分隔 `Value::String` 两种形态，两者都逐元素走 `format_lit`；其余分支是 `col <op> <literal>` 直拼。
- `QueryExecutor::filter_is_complete(condition) -> bool`：`IsNull`/`IsNotNull` 只看列名；`In` 要求数组非空或逗号串至少有一段非空；其余算子在 `Value::Null` 或空字符串时为 `false`。
- `filter_join(logic: Option<&str>) -> &'static str`（模块级自由函数）：`or`（忽略大小写）→ `" OR "`，其余 → `" AND "`。
- `#[cfg(test)] mod tests` 内与筛选相关的既有用例：`filter_literals_use_formatter`、`build_count_sql_with_eq_filter`、`build_select_sql_joins_filters_with_or`、`build_count_sql_joins_filters_with_or`、`filter_join_defaults_to_and`、`format_condition_in_and_is_null`、`incomplete_eq_filter_is_skipped`、`complete_filters_kept_while_incomplete_skipped`、`get_table_data_uses_schema_cache_and_returns_total`。测试自带 `simple_qi` / `simple_lit` / `make_column` / `limit_syntax` / `limit_only_syntax` / `tsql_syntax` 夹具。

### 2.3 宿主 IPC 与服务层

- `src-tauri/src/commands/schema.rs`：`get_table_data_impl(state, db_session_id, table, page, page_size, filters, sorts, skip_count, filter_logic, database, schema) -> Result<TableDataResult, CommandError>`。关键事实：`sorts` 用 `and_then(|list| list.into_iter().next())` **只取第一个**；`effective_skip_count = skip_count.unwrap_or(false) || driver.skip_count_query()`；`filters` 未经任何校验直接透传给 `QueryExecutor::get_table_data`。同文件 `get_table_data` 是 `#[tauri::command]` 薄壳。
- `src-tauri/src/commands/error.rs`：`CommandError` 枚举（含 `Validation(String)` / `NotFound(String)` / `Internal(String)`）；`Serialize` 实现是「先 `redact_secrets_for_log` 再 `serialize_str`」，因此前端收到的是**字符串**、没有 `code` 字段；`From<DriverError>` 把 `DriverError::NotSupported(msg)` 映射为 `CommandError::Validation(msg)`；`CmdExt::cmd_err` 只记日志、**不改写错误文本**，所以消息前缀会原样保留在最前。
- `src-tauri/src/db/registry.rs`：`DriverCapabilities { supports_cancel_query, supports_query_execution_cancel, supports_explain, supports_streaming_results, supports_offset, has_schema_level }`、`DriverCapabilities::from_factory(...)`、`DriverRegistry::get_capabilities`。**没有**任何筛选算子能力位。
- `src-tauri/src/commands/schema/tests.rs` 存在（DB-08 的宿主命令测试落点之一）。
- `src-tauri/src/services/mod.rs`：`pub mod query_executor;` 与 `pub use query_executor::{FilterCondition, OrderBy, QueryExecutor, SortCondition};`。

### 2.4 持久化范式（`src-tauri/src/store`）

- `src-tauri/src/store/mod.rs`：`Store { data_dir, encryption_key, cache: Arc<RwLock<StoreCache>>, write_lock: Mutex<()>, history_db, app_db, favorites }`；`StoreError`（`InitError` / `ReadError` / `WriteError` / `ParseError` / `EncryptionError`）；`Store::load_json_file` / `save_json_file` / `write_file_atomic` / `unique_tmp_path`；`Store::init_with_path`；`Store::get_settings` / `save_settings`。
- `src-tauri/src/store/models.rs`：`StoreCache`（`connections` / `tunnels` / `groups` / `settings` / `sync_tasks` + `sync_tasks_loaded` / `sync_profiles` + `sync_profiles_loaded` / `transfer_profiles` / `schema_diff_profiles` / `ai_config` / `ai_settings_config` …）——**「懒加载 + `*_loaded` 标志」是本项目既有的第二个 store 范式**。同文件 `FavoriteQuery` / `SyncTask` / `QueryHistoryEntry` 展示了 `#[serde(rename_all = "camelCase")]` + `#[serde(default)]` 的 DTO 写法。
- `src-tauri/src/store/sync_profiles.rs`：`ensure_sync_profiles_loaded`（逐条 `serde_json::from_value` 解析，单条坏数据不拖垮整个文件）、`get_sync_profiles`、`save_sync_profile`（按 `id` upsert，然后整体 `save_json_file("sync_profiles.json", …)`）、`delete_sync_profile`。**这就是命名视图要复用的范式。**
- `src-tauri/src/store/favorites/ulid.rs`：`new_ulid()`、`Ulid::parse`、`Ulid::is_valid`。**id 由 store 侧铸造、与标题解耦**（`models.rs` 的 `FavoriteQuery` 注释明确写了这条纪律：改名绝不重写文件）。
- `src-tauri/src/store/tests/`：`fixtures.rs` 的 `init_store_for_test(dir)` / `use_file_key_backend()` / `env_lock()`，`domain_crud_tests.rs` 展示了 facade 级 CRUD 用例写法。
- `src-tauri/src/commands/sync/mod.rs`：`#[tauri::command] pub async fn get_sync_profiles(state)` → `get_sync_profiles_impl(&state)` 的两段式范式（命令薄壳 + 可单测的 `*_impl`）。
- `src-tauri/src/bootstrap/run.rs`：`tauri::generate_handler![...]` 注册表；`src-tauri/src/commands/ipc_surface_tests.rs` 用 `include_str!("../bootstrap/run.rs")` + `include_str!` 的 SOURCE 常量断言「命令已注册」。

### 2.5 前端

- **两份重复定义（契约第 1.6 节点名要求收敛）**：`src/types/index.ts` 的 `FilterOperator` / `FilterCondition` / `SortCondition` 与 `src/types/settings.ts` 的同名三处定义**逐字相同**（都是 10 个算子的字符串联合）。`src/types/settings.ts` 的模块头注释自证：「this module has no importers」，且我已 grep 确认**全仓库没有任何文件 import `types/settings`**。`Value` 定义在 `src/types/connection.ts`。
- `src/stores/tableData/types.ts`：`TableState` 的筛选字段 `filters` / `filterLogic: 'and' | 'or'` / `draftFilters` / `draftFilterLogic` / `filterPanelOpen`；同文件还声明了 `requestRevision` / `loadingRevision` 竞态防护。
- `src/stores/tableData/filterUtils.ts`：`isCompleteFilter(filter)`（**注意它比 Rust 侧多一条**：`Array.isArray(value) && length === 0` 判不完整）、`cloneFilters`、`filterDraftEqualsApplied`（用 `JSON.stringify` 比较）。
- `src/stores/tableDataStore.ts`（722 行）：`TableDataStore` 接口的筛选动作 `addFilter` / `setFilters` / `updateFilter` / `setFilterLogic` / `removeFilter` / `clearFilters` / `applyFilters` / `setFilterPanelOpen`；`patchPanel` / `patchPanelForReload`（后者 `requestRevision + 1`）；`applyFilters` 把 `draftFilters` 原样克隆进 `filters` 且 `page: 0`；**请求路径上存在静默剥离**：`loadTableData` 里 `filters: filters.filter(isCompleteFilter)`。
- `src/components/FilterEditor.tsx`（558 行）：`FilterEditorProps`；模块级 `OPERATORS: FilterOperator[]`（就是那 10 个）；`FILTER_VALUE_DEBOUNCE_MS = 350`；`FilterValueInput`（受控 draft + 防抖 + `composingRef` / `isComposing` / `onCompositionEnd` 的 IME 安全 + blur flush + Enter 提交）；`FilterConditionChip`；`FilterConditionEditor`（`needsValue` 只看 `isNull` / `isNotNull`；`Select` 换算子时**保留旧值**，切到 `isNull` / `isNotNull` 时直接 `onCollapse`）；`opLabel` 用 `t('filter.<op>')`；`conditionSummary`；内部 `editingIndex` 状态机（不完整 → 强制展开，完整 → 折叠成 chip）；`addEmpty` 恒定 `{ column: columns[0]?.name ?? '', operator: 'eq', value: '' }`。
- `src/components/FilterBar.tsx`：`FilterBar` 与 `labelFor`（`${column} ${operator} ${formatCell(value)}`，chips 只读展示 + 单个移除 + 全部清空）。
- `src/components/DataTable/DataTable.tsx`：`hasFilters`（`filters.length > 0 && onRemoveFilter && onClearFilters`）与 `hasFilterEditor`（要求 7 个回调齐备）决定渲染 `FilterEditor` 还是退化的 `FilterBar`。
- `src/windows/connection/TableView.tsx`（770 行）：`handleQuickFilter`（`parseFilterForApply` → 混用 AND/OR 时以 `t('filter.mixedLogic')` 拒绝 → `actions.setFilters(...)`）、`openManualFilter`、`quickFilter` / `quickFilterError` 状态。
- `src/lib/filterExpression.ts`：`FilterExpressionOperator`（8 个：`eq ne gt gte lt lte isNull isNotNull`，**不含 `like` / `in`**）、`FilterConditionExpression` / `FilterLogicalExpression` / `FilterExpression`、`parseFilterExpression`、`filterExpressionToConditions`、`parseFilterForApply`。
- `src/lib/dataTypeColors.ts`：`DataTypeFamily = 'null' | 'bool' | 'number' | 'datetime' | 'json' | 'binary' | 'text'` 与 **`classifyDataType(dataType: string | undefined): DataTypeFamily`**（大小写归一 + 关键字包含匹配）。**注意（分册 05 已落地）**：`classifyDataType` 已被改造为 `cellTypeToDataTypeFamily(resolveCellType(...))`；本册的算子白名单**不再**直接用它，而是消费 05 的 `CellType`（§3.5），族只用于渲染着色。
- `src/lib/databaseMeta.ts`：`KvWorkspaceCapabilities`（可选字段 + 缺省 `false` 的向后兼容范式）与 `DatabaseTypeMeta`。**没有** `dataGrid` 字段。
- `src/lib/driverCapabilities.ts`：`hasSchemaLevel`、`supportsOffset`、`capabilitiesForDbSession`、`relationSchemaFor`——运行时能力查询的既有范式（`get_connection_info` → `DriverCapabilities`）。
- `src/commands/database.ts`：`databaseCommands.getTableData(params)` 的入参形状（`filters` / `sorts` / `skipCount` / `filterLogic` / `database` / `schema`）。
- `src/locales/en/query.ts`：既有 `filter.*` 词条（`filter.filter` / `remove` / `clear` / `add` / `and` / `or` / `value` / `apply` / `collapse` / `activeCount` / `noActive` / `unapplied` / `editCondition` / `eq`…`isNotNull` / `quickPlaceholder` / `invalidExpression` / `mixedLogic`）；其中 **`'filter.like': 'contains'`**（与本册要新增的 `contains` 撞名，见 §8 与 Q-8）。
- `src/locales/index.ts`：`export type I18nKey = TranslationKey | (string & {})`；`src/locales/en/index.ts` 聚合 `core / connection / query / …`。
- 既有测试：`src/stores/tableData/__tests__/filterUtils.test.ts`、`src/components/__tests__/FilterEditor.test.tsx`（mock 掉 `useI18n` 与 `ui/Select`）、`src/components/__tests__/FilterBar.test.tsx`、`src/stores/__tests__/tableDataStore.test.ts`（含 `loadTableData omits incomplete applied filters from the request` —— **本册要显式改写它**）。连续旅程测试的命名先例：`src/components/__tests__/SqlEditorIntentionSettingsJourney.test.tsx`。
- `e2e/specs/table-filter.ts`：TF-001~TF-011 + TF-AI-001，其中 **TF-008「空值 Apply 不得报加载失败且表仍可见」正是今天静默丢弃行为的 E2E 固化**。

### 2.6 我确认**不存在**的东西（避免误以为已有）

`src/lib/gridErrors.ts`、`DataGridCapabilities`、`DatabaseTypeMeta.dataGrid`、任何 `namedView` / `savedView` 代码、任何 `dataType` + 算子组合的白名单逻辑（我 grep 过 `dataType` 与算子、`normalizeDataType` / `dataTypeKind` / `isNumericType` 等命名，均无命中）、任何 `supported_filter_operators` 实现。

---

## 3. 数据结构与接口设计

### 3.1 Rust：算子枚举扩展与常量（`packages/driver-api/src/filters.rs`，修改）

```rust
// 既有 derive 追加 PartialEq / Eq / Copy（仅加宽，不改变序列化形态；Copy 便于按值传进闭包）
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterOperator {
    // ── 既有 10 个：变体名、顺序、序列化拼写一律不动 ──
    Eq, Ne, Gt, Lt, Gte, Lte, Like, In, IsNull, IsNotNull,
    // ── 共享扩展 9 个（新增）：宿主 format_condition 负责渲染 ──
    NotIn, Between, NotBetween, Contains, NotContains, StartsWith, EndsWith, IsEmpty, IsNotEmpty,
    // ── 方言专有 3 个（新增）：只能经 filter_operator_sql 渲染 ──
    Regex, NotRegex, JsonContains,
}

/// 宿主可直接渲染的算子（既有 10 + 共享扩展 9）。契约 5.2 冻结，禁止分册增删。
/// 这同时是 `DatabaseDriver::supported_filter_operators()` 的默认返回值。
pub const SHARED_FILTER_OPERATORS: &[FilterOperator] = &[/* 上面 19 个，顺序同上 */];

/// 只能由驱动 `filter_operator_sql` 渲染的算子；**不在**共享白名单内。
pub const DIALECT_FILTER_OPERATORS: &[FilterOperator] = &[
    FilterOperator::Regex, FilterOperator::NotRegex, FilterOperator::JsonContains,
];

/// 既有 10 个。**只有一个用途**：作为「既有行为逐字节不变」的测试基线；
/// **运行时不被任何默认实现返回**（`supported_filter_operators()` 的默认值是 19 个的 SHARED）。
pub const EXISTING_FILTER_OPERATORS: &[FilterOperator] = &[/* 上面前 10 个 */];

impl FilterOperator {
    /// 稳定的小写驼峰拼写，与 `#[serde(rename_all = "camelCase")]` 的线上形态一致；
    /// 用于错误消息与测试断言，避免各处手写字符串。
    pub fn as_str(self) -> &'static str;              // 新增
    /// 是否属于方言专有组。
    pub fn is_dialect_specific(self) -> bool;         // 新增
    /// 该算子需要几个字面量：0（IsNull 家族）/ 1（多数）/ 2（Between 家族）/ N（In 家族，见 §3.4）。
    pub fn literal_arity(self) -> LiteralArity;       // 新增
}

/// 算子需要的字面量个数（新增）。渲染矩阵按它分派与校验元素数。
pub enum LiteralArity { None, One, Two, Many }       // 新增
```

**逐字段/逐变体说明**

| 项 | 用途 | 缺省 / 兼容 |
| --- | --- | --- |
| `PartialEq/Eq` derive | 让 `supported_filter_operators().contains(&op)`、`FilterRejection` 的比较成立 | 加宽，不影响 serde |
| `Copy` derive | 允许按值传入 `filter_operator_sql(op, …)` 与闭包，避免到处 `.clone()` | 加宽，旧代码继续编译 |
| `SHARED_FILTER_OPERATORS` | 宿主渲染白名单 + trait 默认返回值 | 契约 5.2 字面 |
| `DIALECT_FILTER_OPERATORS` | 供宿主判定「必须问驱动」 | 新增 |
| `EXISTING_FILTER_OPERATORS` | 既有 10 个，**仅作逐字节回归基线**；运行时不被任何默认实现返回 | 新增 |
| `as_str` | 错误消息 `grid.filter.unsupportedOperator: … 'regex' …` 里的稳定拼写 | 新增 |

### 3.2 Rust：`DatabaseDriver` 三个新方法（`packages/driver-api/src/traits.rs`，新增，签名照抄契约 5.2）

```rust
/// 本驱动支持的筛选算子白名单。默认返回共享算子集（契约 §5.2 字面）。
/// 注意：默认值让「没写一行代码的驱动」宣称支持 19 个宿主可渲染算子；
/// 这 19 个全部是 ANSI 可渲染的，且 UI 还受 `CellType` 白名单约束（§3.5）。
fn supported_filter_operators(&self) -> &'static [FilterOperator] {
    crate::filters::SHARED_FILTER_OPERATORS
}

/// 方言专有算子的渲染入口。默认 None：宿主遇到未实现的算子必须报
/// 「该数据库不支持此筛选算子」，禁止静默丢弃条件后照常查询。
/// `column_expr` 已由宿主引用，`literal` 已由本驱动 `format_sql_literal` 渲染。
fn filter_operator_sql(
    &self,
    operator: FilterOperator,
    column_expr: &str,
    literal: &str,
) -> Option<String> {
    None
}

/// 该驱动的 LIKE 在默认排序规则下是否大小写不敏感（MySQL 系默认是）。
/// 仅用于 UI 文案，不改变 SQL。默认 false。
fn like_is_case_insensitive(&self) -> bool {
    false
}
```

**为什么可以只加这三个**：既有 10 个算子由宿主渲染，9 个共享扩展由宿主渲染，3 个方言算子走 `filter_operator_sql`；签名与契约 5.2 完全一致，因此**不需要提升 `PROTOCOL_VERSION`**（契约 §1.7 / R2）。

**已确认的签名局限（诚实记录）**：`filter_operator_sql` 只有一个 `literal: &str`，**表达不了 `In`/`NotIn`（N 个字面量）与 `Between`/`NotBetween`（2 个字面量）**。因此本册的落地口径是：**只有单字面量算子会询问驱动**（Like 家族 + 3 个方言算子），多字面量算子恒由宿主渲染。若将来要驱动渲染多值算子，必须扩展签名并在提升 `PROTOCOL_VERSION` 时一并处理（记 Q-7）。

### 3.3 Rust：宿主渲染与校验模块（`src-tauri/src/services/query_executor/filter_sql.rs`，新增）

把筛选相关逻辑从 `query_executor` 的生产文件里抽出，既解决 797/800 行的上限问题，又让「纯字符串生成」可以脱离 `QueryExecutor` 单测。

**与分册 09 的拆分形态合并（重要，避免两条分支各拆一次同一个文件）**：分册 09 已规划把 `src-tauri/src/services/query_executor.rs` **目录化**为 `query_executor/mod.rs`（生产代码）+ `query_executor/tests.rs`（整块测试）。本册沿用同一形态，只在其下新增 `filter_sql.rs` 子模块：

- **谁先合并谁做目录化**：若 08 先合并，则 08 完成 `query_executor.rs → query_executor/{mod.rs,tests.rs}` 并同时新增 `filter_sql.rs`；若 09 先合并，08 只新增 `filter_sql.rs` 并在 `mod.rs` 里加 `pub(crate) mod filter_sql;`。两种顺序的最终目录结构完全一致，因此不会产生结构性冲突。
- **`QueryExecutor::filter_is_complete` 保留为一行转发**（`fn filter_is_complete(c: &FilterCondition) -> bool { filter_sql::filter_is_complete(c) }`）。理由：分册 09 已在文档里点名「复用 `QueryExecutor::filter_is_complete`」作为 `has_effective_filters` 的唯一判定，保留这个符号路径可以让 09 的引用继续成立，且保证两侧真的是**同一个谓词**。
- **`QueryExecutor::format_condition` / `filter_join` 的引用需要 09 同步改**：`format_condition` 的签名变了（新增回调参数、`String` → `Result`），`filter_join` 搬到了 `filter_sql`。这是本册与 09 之间唯一需要回写对方文档的符号路径变更。

```rust
use datazen_driver_api::filters::{FilterOperator, FilterCondition, SHARED_FILTER_OPERATORS};
use crate::db::Value;

/// 方言专有算子的渲染回调。与既有 `qi` / `format_lit` 同一种注入风格：
/// 纯函数不持有 `Arc<dyn DatabaseDriver>`，单测无需构造驱动实例。
pub(crate) type DialectOperatorRenderer<'a> =
    &'a dyn Fn(FilterOperator, &str, &str) -> Option<String>;

/// LIKE 家族的模式转义字符。**刻意不用反斜杠**：MySQL 默认把字符串字面量里的
/// `\` 当转义引导符，宿主写进去的 `\%` 会被再解释一次而退化成 `%`，
/// 于是「搜 50% 却命中所有含 50 的行」——静默返回错数据。
/// `!` 在四种主流方言的字符串字面量里都是普通字符。
pub(crate) const LIKE_ESCAPE_CHAR: char = '!';

/// 把用户输入的**字面量**转成 LIKE 模式体：转义转义符自身、`%`、`_`；其他字符（含多字节）原样。
pub(crate) fn escape_like_pattern(raw: &str) -> String;                        // 新增
/// 在模式体两侧加通配符。`%a%` / `a%` / `%a` 由调用方按算子选择。
fn like_pattern(body: &str, leading: bool, trailing: bool) -> String;          // 新增（私有）

/// 条件是否「值已填好」。**语义从「静默丢弃」升级为「入口闸门」**，见 §4.5。
/// `QueryExecutor::filter_is_complete` 保留为转发调用它（分册 09 依赖这个符号路径）。
pub(crate) fn filter_is_complete(condition: &FilterCondition) -> bool;         // 从 query_executor 搬移
/// 不完整时的可读原因，用于 grid.filter.incomplete 消息体。
pub(crate) fn incomplete_reason(condition: &FilterCondition) -> Option<&'static str>;  // 新增

/// 进入 SQL 前的**拒绝**原因。它不是「跳过」——契约 §8.2 明文禁止静默丢弃。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FilterRejection {
    UnsupportedOperator { column: String, operator: FilterOperator },
    Incomplete { column: String, operator: FilterOperator, reason: &'static str },
}
impl FilterRejection {
    /// 以契约前缀开头的消息，例如
    /// `grid.filter.unsupportedOperator: sqlite does not support the 'regex' filter on column "name"; try 'like' or 'contains'`
    pub(crate) fn contract_message(&self) -> String;                           // 新增
}

/// 白名单 + 完整性的一次性校验。宿主 IPC 层与服务层各调一次（§5 Step 4/5）。
pub(crate) fn validate_filters(
    supported: &[FilterOperator],
    filters: &[FilterCondition],
) -> Result<(), FilterRejection>;                                             // 新增

/// 单条条件 → SQL 片段。返回 Err 表示**拒绝**，调用方必须把 Err 变成用户可见错误，
/// 绝不允许把它降级成空串（这正是本册要修掉的历史行为）。
pub(crate) fn format_condition(
    condition: &FilterCondition,
    qi: &dyn Fn(&str) -> String,
    format_lit: &dyn Fn(&Value) -> String,
    dialect_op: DialectOperatorRenderer<'_>,
) -> Result<String, FilterRejection>;   // 既有符号，**签名变更**：新增回调参数，返回类型由 String 改为 Result

pub(crate) fn filter_join(logic: Option<&str>) -> &'static str;               // 从 query_executor 搬移，实现一字不改
```

**渲染矩阵（本册的最终 SQL 形态，逐条锁定）**

| 算子 | 分组 | 值形态（wire） | 生成的 SQL（`col` 已引用，`L` 为 `format_lit`） | 转义 |
| --- | --- | --- | --- | --- |
| `Eq` | 既有 | 单值 | `col = L(v)` | 无（驱动负责字面量） |
| `Ne` | 既有 | 单值 | `col != L(v)` | 无 |
| `Gt` `Lt` `Gte` `Lte` | 既有 | 单值 | `col > / < / >= / <= L(v)` | 无 |
| `Like` | 既有 | 单值 | `col LIKE L(v)` | **不转义**：用户自己写 `%`/`_`（今天就是这样） |
| `In` | 既有 | `Value::Json(Array)` 或逗号串 | `col IN (L(e1), L(e2), …)` | 无 |
| `IsNull` `IsNotNull` | 既有 | 无 | `col IS NULL` / `col IS NOT NULL` | — |
| `NotIn` | 共享扩展 | 同 `In` | `col NOT IN (L(e1), …)` | 无（与 `In` 共用同一取值/格式化路径） |
| `Between` | 共享扩展 | `Value::Json(Array)` 恰好 2 元素；逗号串按**首个逗号**切成 2 段 | `col BETWEEN L(lo) AND L(hi)` | 无 |
| `NotBetween` | 共享扩展 | 同 `Between` | `col NOT BETWEEN L(lo) AND L(hi)` | 无 |
| `Contains` | 共享扩展 | 单值 | `col LIKE L('%' + esc(v) + '%') ESCAPE L("!")` | `!`→`!!`，`%`→`!%`，`_`→`!_` |
| `NotContains` | 共享扩展 | 单值 | `col NOT LIKE L('%' + esc(v) + '%') ESCAPE L("!")` | 同上 |
| `StartsWith` | 共享扩展 | 单值 | `col LIKE L(esc(v) + '%') ESCAPE L("!")` | 同上 |
| `EndsWith` | 共享扩展 | 单值 | `col LIKE L('%' + esc(v)) ESCAPE L("!")` | 同上 |
| `IsEmpty` | 共享扩展 | 无 | `col = L("")` | — |
| `IsNotEmpty` | 共享扩展 | 无 | `col <> L("")` | — |
| `Regex` `NotRegex` `JsonContains` | 方言专有 | 单值 | **完全由驱动决定**：`dialect_op(op, col, L(v))` 为 `Some` 时原样使用；为 `None` → 拒绝 | 驱动负责 |

**两条必须写进代码注释的纪律**：

1. **`Like` 与 `Contains` 是两种东西**。`Like` 是「模式算子」，用户输入 `%` 就是通配符（今天的行为，逐字节不变）；`Contains` 是「字面量子串算子」，用户输入的 `%` 只是一个百分号。二者共用 `LIKE` 关键字但**转义策略相反**，这是本册最容易搞错的地方（§10 第 3 条）。
2. **`IsEmpty` 只表达「等于空字符串」，不包含 `NULL`**。`col = ''` 在 PG/MySQL/SQLite 上对 `NULL` 行一律为 `UNKNOWN`（不为真），这正是我们要的：`NULL` 有自己的算子（`IsNull`），把两者合并会让「数据库里到底是 NULL 还是空串」这个用户最关心的问题在 UI 上不可分辨。文案必须写成「空字符串」，不能写「为空」（§8）。

### 3.4 值形态：为什么 `Between` 不改 DTO

`FilterCondition.value` 的类型是 `Value`，契约 R1 禁止改既有字段类型/名字。三种做法里：

- **（选定）用 `Value::Json(serde_json::Value::Array([lo, hi]))`**：`In` 分支今天已经走这条路径（`Value::Json(Array)`），宿主与驱动的字面量通道已被验证；**零 DTO 改动**；算子本身消歧（`Between` 只接受 2 元素）。
- （否决）新增 `value2: Option<Value>` 字段：可读性略好，但给一个「19 个算子里有 17 个永远用不到第二个值」的 DTO 加常驻字段，且要为老数据补 `#[serde(default)]` 语义。
- （否决）把上下界拼成 `"lo,hi"` 字符串：值里含逗号时不可解析（与 `In` 的既有逗号切分是同一个坑）。

**逗号串兼容**：`Between` 若收到 `Value::String`，按**第一个**逗号切成上下界（而不是 `split(',')` 全长切分）——因为日期时间字面量本身可能含逗号（如 `"2024-01-01 10:00,2024-01-02 10:00"` 这种写法虽罕见，但 `split(',')` 一旦切出 3 段就无法判断谁是上界）。切不出 2 段 → 不完整 → 拒绝。

### 3.5 前端：算子元数据表（`src/lib/filterOperators.ts`，新增）

```ts
import type { FilterOperator } from '../types';
import type { CellType, CellTypeHints } from './cellTypes';
import type { DatabaseTypeMeta } from './databaseMeta';

/** 值控件形态，直接决定 FilterConditionEditor 渲染哪种输入。 */
export type FilterValueInputKind =
  | 'none'      // IsNull / IsNotNull / IsEmpty / IsNotEmpty
  | 'single'    // 其余单字面量算子
  | 'pair'      // Between / NotBetween：两个输入框
  | 'tokens';   // In / NotIn：token 输入

export interface FilterOperatorMeta {
  id: FilterOperator;
  /** 值控件形态。 */
  valueInput: FilterValueInputKind;
  /**
   * 适用的**单元格类型**（`CellType`，来自分册 05 的 `./cellTypes`）。
   * 本文件按 `CellType` 分派，**不再**自行按 `DataTypeFamily` 猜：
   * `classifyDataType` 已改为 `cellTypeToDataTypeFamily(resolveCellType(...))`，
   * 若这里再用族，会把 `enum` / `uuid` / `unknown` 三类揉回 `text`，从而放出
   * 在 PG 上必然报错的 `enum = ''`（`IsEmpty`）这类算子。
   */
  cellTypes: readonly CellType[];
  /** 是否方言专有（只能由驱动渲染）。 */
  dialect: boolean;
  /** 分组，用于 UI 分组显示与文档/埋点归类。 */
  group: 'compare' | 'pattern' | 'set' | 'null' | 'empty' | 'dialect';
  /** 该算子的值是否按 LIKE 模式体转义（只有 pattern 组的字面量语义为真）。 */
  escapesLikePattern: boolean;
}

export const FILTER_OPERATOR_META: Record<FilterOperator, FilterOperatorMeta>;   // 新增

/**
 * 从驱动元数据读出「本驱动声明支持哪些算子」。
 * 缺省 = `SHARED_FILTER_OPERATORS`（19 个，契约 §7 冻结）；**方言 3 个必须显式声明**
 * （`meta.dataGrid?.filterOperators` 里不出现就不进 UI）。
 * 注意：这只是「提前知道」，后端 trait 方法才是权威（契约 §7 消费规则第 3 条）。
 */
export function supportedOperatorsForMeta(meta: DatabaseTypeMeta | undefined): readonly FilterOperator[];  // 新增
/** 读 `meta.dataGrid?.likeCaseInsensitive`，缺省 false（仅影响文案）。 */
export function likeCaseInsensitiveForMeta(meta: DatabaseTypeMeta | undefined): boolean;                  // 新增

/**
 * 该列（已归一化的 `CellType`）+ 驱动白名单下可选的算子，顺序稳定（见下方顺序纪律）。
 * 调用方负责先做一次归一化：`resolveCellType(dataType, meta?.cellTypes)`。
 */
export function operatorsForColumn(
  cellType: CellType,
  supported: readonly FilterOperator[],
): FilterOperator[];                                                             // 新增
export function valueInputFor(operator: FilterOperator): FilterValueInputKind;   // 新增
/** 换列/换取到的驱动白名单后，把当前算子收敛到合法集合内；converter 见下。 */
export function normalizeOperatorForColumn(
  current: FilterOperator,
  cellType: CellType,
  supported: readonly FilterOperator[],
): FilterOperator;                                                               // 新增
/** 算子切换时的值形态转换（single ↔ tokens ↔ pair），保证切换不丢用户输入。 */
export function convertFilterValue(
  from: FilterValueInputKind,
  to: FilterValueInputKind,
  value: unknown,
): unknown;                                                                      // 新增
/** 该列的默认算子（新加条件时用）。恒定 'eq'，见下方纪律。 */
export function defaultOperatorForColumn(cellType: CellType): FilterOperator;    // 新增
```

**按 `CellType` 的默认算子集（`cellTypes` 的来源与结果）**

| `CellType`（分册 05） | 可选算子（显示顺序即此顺序） |
| --- | --- |
| `text` | `eq ne gt lt gte lte like contains notContains startsWith endsWith in notIn isNull isNotNull isEmpty isNotEmpty` |
| `enum` | `eq ne in notIn isNull isNotNull`（**不放 LIKE 家族与 `IsEmpty`**：PG 的枚举列与字面量做模式匹配/空串比较需要显式 cast，属必然报错的入口） |
| `number` | `eq ne gt lt gte lte between notBetween in notIn isNull isNotNull` |
| `bool` | `eq ne isNull isNotNull` |
| `date` / `time` / `datetime` | `eq ne gt lt gte lte between notBetween isNull isNotNull` |
| `json` | `eq ne isNull isNotNull`（驱动白名单含 `jsonContains` 时追加，排在 `isNotNull` 之后） |
| `bytes` | `eq ne isNull isNotNull` |
| `uuid` | `eq ne in notIn isNull isNotNull` |
| `unknown` | 同 `text`（05 的兜底语义：未知类型按文本处理、绝不报错） |

**顺序纪律（兼容性 B2 的落点）**：每个 `CellType` 的数组必须让既有 10 个算子的**相对顺序**保持 `eq ne gt lt gte lte like in isNull isNotNull`。上表中 `text` 把 `in` 放在 `notIn` 前、`isNull` 放在 `isEmpty` 前，正是为了满足这一点；`number` / `datetime` 去掉 `like` 与 `in` 后，剩下的相对顺序仍然一致。§9 有专门用例断言这条。

**两个刻意的决定**：

- **默认算子恒为 `eq`**，不按类型变化。理由：今天 `addEmpty` 与 `openManualFilter` 都写死 `{ operator: 'eq', value: '' }`，既有测试（`FilterEditor.test.tsx` 的 onAdd 断言）固化在这个值上；且 `eq` 在**所有** `CellType` 的集合里都存在，按类型改默认值的收益为零、回归面为正。
- **方言算子不进任何 `cellTypes` 数组**：它们只通过驱动白名单追加，且只有 `text` 接 `regex` / `notRegex`、`json` 接 `jsonContains`。这是 R4 的落点——能力缺失时**隐藏入口**而不是渲染一个点了报错的按钮。

**类型归一化的唯一来源（与分册 05 对齐，05 已落地）**：`src/lib/cellTypes.ts` 的 `resolveCellType(dataType, meta?.cellTypes)` → `CellType`。分册 05 已把 `classifyDataType` 改造为 `cellTypeToDataTypeFamily(resolveCellType(...))`，因此：
- 本册**不新增**任何类型判断逻辑，只消费 `CellType`；
- 调用点必须先归一化一次：`operatorsForColumn(resolveCellType(column.dataType, meta?.cellTypes), supportedOperatorsForMeta(meta))`；
- 驱动在 `DatabaseTypeMeta.cellTypes`（05 的 `CellTypeHints`）里声明的覆盖规则**同时**约束编辑控件与筛选算子集——这是「一处声明、两处生效」，避免出现「编辑器按 enum 处理、筛选器按 text 处理」的双重标准；
- 05 已知的分类缺陷修正（`point` 由 `number` 改判 `text`）会**自动**改变该列的筛选算子集（多出 LIKE 家族），这是正确的连带效果，需在 CHANGELOG 标注（与 05 的标注同批）。

### 3.6 嵌套布尔分组：只预留，不实现

**数据结构（新增，TS 侧先落地类型，Rust 侧仅在文档中冻结形状）**

```ts
/** 未来用于表达 (A AND B) OR C 的递归节点。本册只定义、不接线。 */
export type FilterNode =
  | { kind: 'condition'; condition: FilterCondition }
  | { kind: 'group'; op: 'and' | 'or'; children: FilterNode[] };

/** `(A AND B) OR C` 的线上形态（本册不发送，仅冻结形状）。 */
export const EXAMPLE_FILTER_TREE: FilterNode = {
  kind: 'group',
  op: 'or',
  children: [
    { kind: 'group', op: 'and', children: [
      { kind: 'condition', condition: { column: 'status', operator: 'eq', value: 'active' } },
      { kind: 'condition', condition: { column: 'score', operator: 'gte', value: 60 } },
    ] },
    { kind: 'condition', condition: { column: 'role', operator: 'eq', value: 'admin' } },
  ],
};
```

**结论：本册只做「类型预留 + 不做实现」**。理由（三条，缺一不可）：

1. PRD 把 C-33 列为 **P2**，并在「不做」表里写明「待 DB-08 的逐条连接符上线后，用埋点判断是否真有需求」；而逐条连接符本身也不在本册（契约 `filter_join` 是全局单一逻辑，未授权改动）。
2. 契约 §5.2 冻结的算子集与 `filter_join` 语义决定了「分组」不是本册的改动面；把 `filter_join` 换成递归树 = 改一个被三处调用的既有契约点，属于独立可回滚提交。
3. 分组必须配套 UI（缩进 / 括号 / 键盘导航 / IME 与拖拽），这是新的一套交互状态机，塞进本册会把 `FilterEditor` 的改动面翻倍。

**预留的接线点（写清楚，方便将来接手）**：`getTableData` 的新增**可选**入参 `filterTree?: FilterNode`（契约 §6.2 允许，`Option` + `#[serde(default)]`）；缺省时 `filters` + `filterLogic` 仍是唯一权威，**线上形态与今天完全一致**。当 `filterTree` 出现时，宿主走新的递归渲染 `format_filter_node`，`filter_is_complete` 变成递归版本 `filter_node_is_complete`，并在 UI 里禁用「全局 AND/OR 开关」以防两套真相。本册交付 `FilterNode` 类型 + `filterNodeToLegacyConditions(node): FilterCondition[]`（**仅用于「扁平列表可表达」的降级**，遇到真嵌套时返回 `null` 让调用方显式拒绝，绝不静默拍平）。

### 3.7 命名视图 DTO（TS：`src/lib/namedViews.ts`（新增）；Rust：`src-tauri/src/store/named_views.rs`（新增））

```ts
/** 视图作用域：持久化连接 id + 库 + 模式 + 表。 */
export interface NamedViewScope {
  /** 必须是持久化连接 id（connectionId），**绝不是** dbSessionId。 */
  connectionId: string;
  database: string | null;
  schema: string | null;
  table: string;
}

export interface NamedView {
  /** ULID，由后端铸造；改名绝不改 id。 */
  id: string;
  name: string;
  scope: NamedViewScope;
  /** 保存的是**已应用**的筛选，不是草稿。 */
  filters: FilterCondition[];
  filterLogic: 'and' | 'or';
  /** 数组：DB-11 落地多列排序后无需改 DTO。 */
  sorts: SortCondition[];
  /** 列显隐；null = 全部可见（与 TableState.visibleColumns 同义）。 */
  visibleColumns: string[] | null;
  /** 列顺序；null = 交给驱动/默认顺序。 */
  columnOrder: string[] | null;
  /** 每个 scope 内至多一个为真。 */
  isDefault: boolean;
  /** RFC3339。 */
  createdAt: string;
  updatedAt: string;
}

/** 导入导出的文件载荷（版本化，便于以后加字段）。 */
export interface NamedViewExportFile {
  version: 1;
  exportedAt: string;
  views: NamedView[];
}

export function namedViewScopeKey(scope: NamedViewScope): string;               // 新增
export function viewFromTableState(...): NamedView;                             // 新增（取自 applied）
export function normalizeViewName(name: string): string;                        // 新增（trim + 折叠空白）
export function viewsForScope(views: NamedView[], scope: NamedViewScope): NamedView[];  // 新增
export function resolveImportName(name: string, taken: Set<string>): string;    // 新增（"name (2)"）
```

```rust
// src-tauri/src/store/named_views.rs（新增）
use crate::services::{FilterCondition, SortCondition};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedViewScope {
    pub connection_id: String,
    #[serde(default)] pub database: Option<String>,
    #[serde(default)] pub schema: Option<String>,
    pub table: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedView {
    /// 新记录由 `ensure` 之外的保存路径用 `favorites::ulid::new_ulid()` 铸造；
    /// 空串 = 由 store 生成。
    #[serde(default)] pub id: String,
    pub name: String,
    pub scope: NamedViewScope,
    #[serde(default)] pub filters: Vec<FilterCondition>,
    /// 沿用既有线上形态的字符串（与 `get_table_data` 的 `filter_logic: Option<String>` 一致），
    /// 由 `validate()` 收敛为 "and" / "or"。
    #[serde(default = "default_filter_logic")] pub filter_logic: String,
    #[serde(default)] pub sorts: Vec<SortCondition>,
    #[serde(default)] pub visible_columns: Option<Vec<String>>,
    #[serde(default)] pub column_order: Option<Vec<String>>,
    #[serde(default)] pub is_default: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)] pub updated_at: Option<DateTime<Utc>>,
}

impl NamedView {
    /// 校验：名字非空（trim 后）、scope.connection_id / table 非空、filter_logic ∈ {and, or}、
    /// 每个 filter 都有非空列名。返回人类可读原因（不含 SQL 片段）。
    pub fn validate(&self) -> Result<(), String>;                                // 新增
    /// 规范化：trim 名字、折叠连续空白、filter_logic 小写。
    pub fn normalize(&mut self);                                                 // 新增
}
```

**store 门面（新增于 `impl Store`）**

```rust
impl Store {
    async fn ensure_named_views_loaded(&self);                                   // 新增
    pub async fn get_named_views(&self) -> Vec<NamedView>;                        // 新增
    /// 按 id upsert（id 为空则铸造）；返回落盘后的记录（含 id 与时间戳）。
    /// 同 scope 内**异 id 同名** → Err(NamedViewError::NameExists)。
    pub async fn save_named_view(&self, view: NamedView) -> Result<NamedView, NamedViewError>;   // 新增
    /// 幂等：删不存在的 id 返回 Ok，并返回剩余列表。
    pub async fn delete_named_view(&self, id: &str) -> Result<Vec<NamedView>, NamedViewError>;   // 新增
}

#[derive(Debug, thiserror::Error)]
pub enum NamedViewError {                                                        // 新增
    #[error("grid.view.nameExists: a saved view named '{0}' already exists for this table")]
    NameExists(String),
    #[error("grid.view.invalid: {0}")]
    Invalid(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}
```

**逐字段说明与缺省**

| 字段 | 用途 | 缺省 |
| --- | --- | --- |
| `id` | 稳定身份；改名只改 `name` | 缺省空串 → store 铸造 ULID |
| `scope.connectionId` | 持久化归属，跨重启有效 | 必填；**dbSessionId 永不落盘**（`AGENTS.md` 的 ID 术语规范） |
| `filters` + `filterLogic` | 视图的核心 | `[]` + `"and"` |
| `sorts` | 排序恢复；数组以兼容 DB-11 | `[]` |
| `visibleColumns` / `columnOrder` | PRD DB-08 §7 要求的列状态 | `null` = 不干预 |
| `isDefault` | 每个 scope 至多一个 | `false`；保存 `true` 时把同 scope 其他记录置 `false`（同一次写盘） |
| `createdAt` / `updatedAt` | 显示与排序 | `createdAt` 必填；`updatedAt` 缺省 `None`，改名/覆盖时更新 |

**持久化位置与格式**：`{appData}/named_views.json`，内容为 `NamedView[]` 的 pretty JSON，走 `Store::save_json_file`（`write_file_atomic` + `unique_tmp_path` + `write_lock`，与 `sync_profiles.json` 同一套原子写与并发保护）。加载走 `ensure_named_views_loaded` 的**逐条容错解析**（照抄 `sync_profiles.rs`：单条坏记录不拖垮整个文件）。**不新建存储引擎、不新建加密格式**：这个文件与 `settings.json` / `sync_profiles.json` 同族。

### 3.8 IPC 命令（契约 §6.1 的三个）

```rust
// src-tauri/src/commands/named_views.rs（新增）
#[tauri::command]
pub async fn list_named_views(
    state: tauri::State<'_, AppState>,
    scope: Option<NamedViewScope>,
) -> Result<Vec<NamedView>, CommandError>;

#[tauri::command]
pub async fn save_named_view(
    state: tauri::State<'_, AppState>,
    view: NamedView,
) -> Result<NamedView, CommandError>;

#[tauri::command]
pub async fn delete_named_view(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<Vec<NamedView>, CommandError>;

// 与 commands/sync/mod.rs 同构的两段式：命令薄壳 + 可单测的 *_impl
pub(crate) async fn list_named_views_impl(state: &AppState, scope: Option<NamedViewScope>) -> Result<Vec<NamedView>, CommandError>;
pub(crate) async fn save_named_view_impl(state: &AppState, view: NamedView) -> Result<NamedView, CommandError>;
pub(crate) async fn delete_named_view_impl(state: &AppState, id: &str) -> Result<Vec<NamedView>, CommandError>;
```

| 命令 | 入参 | 出参 | 缺省与语义 |
| --- | --- | --- | --- |
| `list_named_views` | `scope?: NamedViewScope`（`Option` + `#[serde(default)]`） | 全部（或该 scope 的）视图，按 `isDefault` 优先、再按 `name` 字典序 | 省略 scope = 返回全部（给「视图管理 / 导入导出」用） |
| `save_named_view` | `view`（`id` 可空串） | 落盘后的记录（含铸造的 id 与时间戳） | 返回记录而不是 `()`：前端必须用返回值替换本地临时对象，否则改名/新建后本地 id 与后端不一致 |
| `delete_named_view` | `id` | 删除后的剩余列表 | 幂等，删不存在 = `Ok`（与 `delete_sync_profile` 的「retain 后整体写盘」一致） |

**前端封装（`src/commands/database.ts`，修改）**：`databaseCommands.listNamedViews(scope?)` / `saveNamedView(view)` / `deleteNamedView(id)`，用 `invoke('list_named_views', { scope })` 等（IPC 命名：前端 camelCase、Rust snake_case，Tauri 自动映射）。

**注册**：三个命令必须加入 `src-tauri/src/bootstrap/run.rs` 的 `tauri::generate_handler![...]`，并在 `commands/named_views.rs` 内用 `include_str!("../bootstrap/run.rs")` 加一条「已注册」断言（照抄 `ipc_surface_tests.rs` 的纪律；那个测试的 SOURCE 常量只 include 了 config/connection_import/app_archive/encryption_key/tunnel，本册的命令文件不在其中，所以断言写在自己的模块里）。

**导入导出**：不做新命令，走既有的原生文件对话框 + 读写路径。导出 = 把 `NamedViewExportFile` 序列化为 pretty JSON 写文件；导入 = 解析 → `validate()` 逐条 → scope 归属到**当前表**（跨表导入时用当前 scope 覆盖记录里的 scope，并在 UI 明说）→ 重名走 `resolveImportName` 加 `(2)` 后缀 → 逐条 `save_named_view`。**不共享跨连接**：视图作用域含 `connectionId`，因为筛选条件里的列名是表/连接特有的；跨连接搬运的唯一途径是导出/导入。

---

## 4. 交互与状态机

### 4.1 面板状态机

| 状态 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `closed`（`filterPanelOpen === false` 且无已应用筛选） | 初始；点摘要头收起；`Escape` | 组件整体返回 `null`（`FilterEditor` 既有行为） | 点摘要头 / 点 `filter-add` / `⌘`/`Ctrl+F`（键位归 DB-02）→ `open`；外部 `setFilters`（快速筛选、FK 跳转、视图切换）→ `open` |
| `collapsed-summary`（有已应用筛选但面板收起） | 点摘要头；`Escape` | 摘要行显示 `{count} active` + `AND`/`OR` 连接后的条件文本；`dirty` 时显示「未使用」徽标 | 点摘要头 → `open` |
| `open` | 见上 | 逻辑开关 + `Add` + 条件列表 + `Apply` / `Hide` / `Clear` | `Escape` → `collapsed-summary`（若已应用）或 `closed`（若无已应用且草稿为空）；`Clear` → `open` 且两个列表清空 |

**焦点契约**：进 `open` 后焦点落到**第一个条件**的值输入框（没有条件则落到 `filter-add`）。`Escape` 在输入框内先清空「正在编辑的条件」再关面板（两级退出，避免一次按键丢两层上下文）。这两条要在 E2E 里断言。

### 4.2 单条条件的状态机

| 状态 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `incomplete-expanded` | 新增条件（`eq` + 空值）；把已有条件的值清空；`Between` 只填了上界；`In` token 为空 | 强制展开（`isCompleteFilter` 为假），行内显示「未填值」标记；`Apply` 被阻断 | 值填完 → `complete-chip` |
| `complete-chip` | 条件完整且不是当前 `editingIndex` | 折叠为 chip，显示 `column op value` | 点 chip → `editing-expanded` |
| `editing-expanded` | 点 chip；新加条件且已完整 | 完整控件可见，可直接改列/算子/值 | 失焦且完整 → `complete-chip`；`Enter`（值非空）→ `complete-chip`；`Escape` → `complete-chip`；删除 → 从列表移除 |

**算子切换时的值控件跃迁（本册新增的核心交互）**

| 从 → 到 | 值转换（`convertFilterValue`） | 控件 |
| --- | --- | --- |
| 单值 → 单值 | 原值不变 | 一个输入框 |
| 单值 → `none`（`isNull`/`isNotNull`/`isEmpty`/`isNotEmpty`） | **保留旧值不动**（与今天一致：切回 `eq` 时用户输入还在）；`isEmpty`/`isNotEmpty` 与 `isNull` 一样把值视为「不参与 SQL」 | 值输入框消失 |
| `none` → 单值 | 沿用保留的旧值 | 一个输入框 |
| 单值 → `tokens`（`in`/`notIn`） | 字符串按逗号切分、`trim`、丢空段 → `string[]` | token 输入框 |
| `tokens` → 单值 | `string[]` 用 `', '` 连接 → `string`（空数组 → `''`） | 一个输入框 |
| 单值 → `pair`（`between`/`notBetween`） | `[原值, '']`：上半沿用，下半留空 → 立即是「不完整」，把用户视线引到缺失的下界 | 两个输入框 |
| `pair` → 单值 | 取**下界**（上界丢弃，因为单值控件表达不了） | 一个输入框 |
| `tokens` → `pair` | `[t0 ?? '', t1 ?? '']` | 两个输入框 |
| `pair` → `tokens` | `[lo, hi]` 去掉空段 | token 输入框 |
| 任意 → `dialect`（`regex`/`jsonContains`） | 原值按字符串保留 | 一个输入框 |

**换列**：列的 `CellType` 变了以后，若当前算子不在新类型的集合里，`normalizeOperatorForColumn` 收敛到 `eq`（默认算子），值**保留**（用户换列常常是想复用同一个值再改列）。`CellType` 不变时算子不动。

**不完整提示（取代静默丢弃）**：不完整条件在 UI 上是**三处可见**——行内「未填值」标记、`Apply` 按钮被阻断并显示原因（`filter.errorIncomplete`，指明是第几个条件/哪一列）、面板展开时把该条件滚入视野。用户想「先看全表」的唯一路径是 `Clear` 或删除该条件，**不是**留一个空条件按 Apply。

### 4.3 token 输入（`in` / `notIn`）

- 控件：一个带 chips 的输入框（`FilterTokenInput`）。`,` 或 `Enter` 成 token；`Backspace` 在输入框为空时删最后一个 token；点 token 上的 `×` 删该 token；失焦 `flush` 未提交的文本。
- **IME 纪律**（照抄既有 `FilterValueInput`）：`composingRef` + `e.nativeEvent.isComposing` + `onCompositionStart` / `onCompositionEnd`；**合成期间 `Enter` 不得成 token**（中文输入法确认候选词会发 Enter，如果不拦就会切出半个词）。合成结束时用 `requestAnimationFrame` 复位标志（与今天同构）。
- 值形态：token 数组原样作为 `Value::Json(Array([...]))` 发给后端；**不拼接成逗号串**（避免值里含逗号时后端切错）。后端对逗号串的兼容保留，仅为老调用方与既有测试服务。

### 4.4 `pair` 输入（`between` / `notBetween`）

- 控件：两个输入框，中间一个 `filter.and` 文案的连接符（复用既有 `'filter.and'` = `AND`，不新增词条）。
- 上下界颠倒（`lo > hi`）：**不自动交换**。当两值都是数字、或都是可被 `Date.parse` 解析的日期时，显示 `filter.betweenInverted` 警告（「上界小于下界，结果必为空」），但**允许 Apply**——数据库会返回 0 行，这与用户输入一致；自动交换会把打错的区间静默改成「有结果」，属于最坏的一类静默行为。

### 4.5 「不完整」政策：变更与替代保护

**今天的行为（两层静默）**：`tableDataStore.applyFilters` 把草稿原样放进 `filters`；`loadTableData` 在请求前 `filters.filter(isCompleteFilter)` 剥掉不完整的；Rust 侧 `format_condition` 再剥一次。E2E TF-008 与前端 store 测试 `loadTableData omits incomplete applied filters from the request` 把这条行为固化了。

**本册的行为（变更为显式拒绝）**：

| 层 | 今天 | 本册 | 为什么 |
| --- | --- | --- | --- |
| UI（`FilterEditor`） | 不完整条件静默不参与，`Apply` 变灰只是因为在 `dirty` 判定上无变化 | 不完整条件**可见标记**，`Apply` 被阻断并显示原因 | 用户必须知道「我有一项筛选没填完，未生效」 |
| 前端请求（`tableDataStore.loadTableData`） | `filters.filter(isCompleteFilter)` 静默剥离 | **移除该剥离**，请求体 = 已应用状态 | 否则「UI 显示的条件」与「实际查询条件」长期不一致，且宿主永远看不到问题（契约 §8.2 要求不得静默丢弃后照常查询） |
| 宿主 IPC（`get_table_data_impl`） | 无校验 | `validate_filters` 前置校验 → `CommandError::Validation("grid.filter.incomplete: …")` | 快速失败、错误类型明确、不必先建 schema 缓存与 COUNT |
| 宿主服务（`QueryExecutor::get_table_data`） | `format_condition` 返回空串 → 被 `.filter(!is_empty)` 丢掉 | `format_condition` 返回 `Err(FilterRejection)` → `DriverError::NotSupported(contract_message())` → `CommandError::Validation` | 非 IPC 调用方（MCP / workflow 直接走服务层）也不能漏；两层都拒 |

**替代保护（原保护的原因必须仍然成立）**。原静默丢弃针对的是「`''` 被驱动强转到整型主键 → 整表加载失败」。新方案下这个失败模式**不可能再发生**，因为 `''` 在**进入 SQL 之前**就被拒绝：

1. `filter_is_complete` 保留为**入口闸门**（语义从「跳过」改为「拒绝」），`Value::Null` / 空字符串 / 空数组仍然被判为不完整——**判定逻辑本身不变**，变的只是「不完整时返回什么」。
2. 拒绝时 `cmd_err` 不改写消息，`grid.filter.*` 前缀稳定落在最前（§2.3 已核实）；前端 `classifyGridError` 前缀匹配成功即显示本地化文案，失败则**回退显示原始消息**（契约 §8.1），绝不吞错。
3. 错误消息必须**带上列名与原因**（`grid.filter.incomplete: column "id" needs a value before it can be filtered`），UI 据此定位并聚焦到那一条条件，同时给出一键删除 / 一键清空。
4. 想「看全表」的用户走 `Clear`，而不是留空条件 Apply。这条写进 E2E（TF-008 改写）。

**代价（诚实记录）**：任何今天依赖「空条件 ⇒ 得到部分/全表结果」的第三方调用方（MCP 客户端、脚本）行为会改变，从「静默少一个条件」变成「收到明确错误」。这是**有意的**：静默少一个条件意味着返回的数据集大于用户以为的范围，属于数据正确性问题，不是体验问题。此项仍需人工确认（Q-1）。

### 4.6 命名视图的交互状态机

| 状态 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `idle` | 面板关闭或无视图 | 工具条只有 `Saved views` 入口 | 点入口 → `loading` |
| `loading` | 打开菜单（`list_named_views` 在途） | 菜单显示骨架，条目不可点 | 返回 → `list`；失败 → `error`（菜单内错误 + 重试） |
| `list` | 已加载 | 列出当前 scope 的视图（默认视图带 `defaultBadge`）；条目动作：应用 / 重命名（行内）/ 删除（确认）/ 设为默认；底部 `Save as…` / `Export` / `Import` | 点条目 → 应用并回到 `idle`；`Save as…` → `naming`；`Import` → `importing` |
| `naming` | 点 `Save as…`（未应用草稿时额外提示） | 输入名称；名称占用时禁用保存并提示存在同名 | 确定 → 保存 → `list`；取消 → `list` |
| `importing` | 点 `Import` | 选文件 → 校验 → 逐条导入，结果 toast（`{count} views imported` / 失败原因） | 完成 → `list` |
| `error` | 任一命令失败 | 显示 `classifyGridError` 后的文案 | 重试 → `loading` |

**与草稿筛选的关系（重要）**：保存的是**已应用**的 `filters` + `filterLogic`，**不是** `draftFilters`。若 `dirty`（草稿 ≠ 已应用）时点 `Save as…`，先显示 `filter.view.draftNotAppliedHint`（「将保存已应用的筛选；你还有未应用的改动」）并给「先应用再保存」，**不**阻止保存（用户可能就是想先存住当前已生效的状态）。应用视图 = 一次 `setFilters(filters, logic)` + `setSort`（`sorts[0]`，直到 DB-11）+ `setVisibleColumns`，全部走既有动作，因此自动获得 `requestRevision` 竞态防护与 `page: 0` 重置。

**重名与改名**：同 scope 内名称唯一（比较前 `normalizeViewName` = trim + 折叠空白，**大小写不敏感**以免出现 `Users` / `users` 两条用户分不清的视图）。改名 = 带原 id 的 `save_named_view`；若新名与**同 scope 的异 id** 冲突 → 宿主返回 `grid.view.nameExists`，UI 弹出「覆盖」确认；确认后以**被覆盖者**的 id 保存（丢弃旧记录），保证列表不出现两条同名。

**删除**：二次确认（视图是用户资产）；后端幂等。

**跨连接**：**不共享**。视图的 scope 含 `connectionId`；换连接后菜单只显示该连接的视图。导出/导入是唯一的搬运方式，导入时 scope 被重写为目标表。

---

## 5. 实现步骤

每步都能独立编译、独立自测、独立回滚。

**Step 1 — 驱动契约扩展（`packages/driver-api/src/filters.rs`）**
改：加 12 个变体、3 个常量、`PartialEq/Eq/Copy` derive、`as_str` / `is_dialect_specific` / `literal_arity`。
为什么：这是所有下游的类型基座，必须先落地；加变体会让穷尽匹配的驱动**编译失败**——这是契约明确要的响亮失败。
自测：`cargo test -p datazen-driver-api --lib`；另加 `#[cfg(test)]` 用例断言「既有 10 个的 `as_str()` 与 serde 拼写逐字相等」「`SHARED.len() == 19`」「`LEGACY.len() == 10`」「`DIALECT.len() == 3`」「三组两两不重叠」。

**Step 2 — 三个 trait 方法（`packages/driver-api/src/traits.rs`）**
改：加 `supported_filter_operators` / `filter_operator_sql` / `like_is_case_insensitive`，签名照抄契约 §5.2。
为什么：把「支持什么」与「怎么渲染」变成驱动声明，宿主不按库名分支（零硬编码）。
自测：用 `MockDriver`（未覆盖这三个方法）断言默认值 = `SHARED_FILTER_OPERATORS` / `None` / `false`；这条用例就是「不写代码的驱动行为不变」的证明。

**Step 3 — 抽出 `filter_sql` 子模块（`src-tauri/src/services/query_executor/filter_sql.rs` 新增；`query_executor` 目录化，`query_executor/mod.rs`、`services/mod.rs` 修改）**
改：把 `format_condition` / `filter_is_complete` / `filter_join` 搬到新子模块，加上 §3.3 的 19 算子渲染与 `Result` 化；`build_select_sql` / `build_count_sql` 改为 `Result<String, FilterRejection>`，删掉 `filter(|s| !s.is_empty())`；`get_table_data` 把 `FilterRejection` 映射为 `DriverError::NotSupported(contract_message())`；`services/mod.rs` 增 `pub(crate) use query_executor::filter_sql::{validate_filters, FilterRejection};`。
为什么：`query_executor.rs` 今天 797 行，本册的 match 分支与校验逻辑必然突破 800 行上限（§6 有拆分方案）；同时让纯字符串生成可以脱离数据库单测。**必须先与分册 09 对齐拆分形态**（§3.3：谁先合并谁做 `query_executor.rs → query_executor/` 目录化，后合并者只加子模块），否则两条分支会各拆一次同一个文件。
自测：把 §2.2 里与筛选直接相关的既有用例**连断言文本一起**搬进 `filter_sql` 的测试模块，再补 §9 的 19 算子用例；保留 `QueryExecutor::filter_is_complete` 转发并加一条「转发与 `filter_sql::filter_is_complete` 同结果」的用例（分册 09 依赖这个谓词唯一）；`cargo test -p datazen --lib`。

**Step 4 — IPC 前置校验（`src-tauri/src/commands/schema.rs`）**
改：`get_table_data_impl` 在取会话之后、建 SQL 之前插入
`filter_sql::validate_filters(driver.supported_filter_operators(), filters.as_deref().unwrap_or_default()).map_err(|r| CommandError::Validation(r.contract_message()))?;`
为什么：失败在 COST 之前；错误类型是明确的 `Validation`，与契约 §8.2 前缀一致。
自测：在 `src-tauri/src/commands/schema/tests.rs` 加「不完整筛选返回 `grid.filter.incomplete` 前缀」「白名单外算子返回 `grid.filter.unsupportedOperator` 前缀」，断言消息**以该前缀开头**（用 `starts_with`，不写行号）。

**Step 5 — 服务层兜底校验（`src-tauri/src/services/query_executor.rs`）**
改：`get_table_data` 开头用 `driver.supported_filter_operators()` 调一次 `validate_filters`，把结果映射成 `DriverError::NotSupported`。
为什么：MCP / workflow 等非 IPC 调用方绕过 `*_impl`，缺少这一层就会出现「服务层拿到不支持算子」的路径；两层都拒 = 无死角。
自测：直接调 `QueryExecutor::get_table_data` 并断言 `Err`，消息前缀一致。

**Step 6 — 前端类型收敛（`src/types/index.ts`、`src/types/settings.ts`）**
改：`index.ts` 的 `FilterOperator` 扩到 22 个（顺序与 Rust 一致），加 `EXISTING_FILTER_OPERATORS` / `SHARED_FILTER_OPERATORS` / `DIALECT_FILTER_OPERATORS` 三个 TS 常量便于测试与 UI 判定；`settings.ts` 删掉重复的 `FilterOperator` / `FilterCondition` / `SortCondition` 三处定义（全仓库无 importer，已在 §2.5 核实）。
为什么：契约 §1.6 点名要求收敛；不收敛的后果是「面板能选、执行时报类型错」。
自测：`pnpm typecheck` 干净（删除后若有隐藏 importer，tsc 会立刻报出来——这就是最好的验证）。

**Step 7 — 算子元数据表（`src/lib/filterOperators.ts` 新增）**
改：落 §3.5 的 `FILTER_OPERATOR_META` 与四个工具函数 + `convertFilterValue`。
为什么：算子 → 值控件 / `CellType` / 转义语义必须是**数据**而不是散在 JSX 里的 `if`，否则每加一个算子都要改三处。
自测：`src/lib/__tests__/filterOperators.test.ts`（§9）。

**Step 8 — 错误码分类器（`src/lib/gridErrors.ts`，与分册 09 **共用，只建一次**）**
改：`GridErrorCode` 联合类型 + `classifyGridError(message)` 前缀匹配（`grid.filter.unsupportedOperator` / `grid.filter.incomplete` / `grid.view.nameExists` / 契约 §8.2 的其余码），未知 → `'unknown'`，UI 回退显示原始消息。
**去重纪律**：分册 09 也已规划创建该文件（它用 `grid.count.*` 前缀）。若 09 先合并，本册**只追加** `grid.filter.*` / `grid.view.*` 的成员与前缀，**不重建文件**；若 08 先合并，则由 08 建文件并把 `grid.count.*` 的成员位置留好（或同时写入，由 09 复核）。对应地，`src/lib/databaseMeta.ts` 的 `DataGridCapabilities`（契约 §7）同样**只建一次**：09 补 `countStrategy`，本册补 `filterOperators` / `likeCaseInsensitive`，`editable` / `insertReturning` 由契约冻结缺省。
为什么：契约 §8.1 指定了机制与落点；不做的后果是前端只能显示原始英文消息。两个分册各建一份同名文件会在合并时产生「后提交者整文件覆盖」的静默丢失。
自测：`src/lib/__tests__/gridErrors.test.ts`。

**Step 9 — 前端值控件与条件编辑器拆分（`src/components/filter/` 新增目录 + `src/components/FilterEditor.tsx` 修改）**
改：把 `FilterValueInput` 搬进 `src/components/filter/FilterValueInput.tsx`（IME 逻辑一字不改），新增 `FilterTokenInput.tsx`、`FilterBetweenInput.tsx`、`FilterConditionEditor.tsx`、`FilterConditionChip.tsx`；`FilterEditor.tsx` 只留面板壳与状态机，并**继续 re-export `FILTER_VALUE_DEBOUNCE_MS`**（既有测试从 `'../FilterEditor'` import 它）。
为什么：`FilterEditor.tsx` 已 558 行，直接加三种值控件 + 视图菜单会突破 800 行；先拆分再改功能，diff 可读、可回滚。
自测：既有 `FilterEditor.test.tsx` 与 `FilterBar.test.tsx` **不改断言**先跑绿（证明拆分是纯搬家），再在新文件上做 Step 10。

**Step 10 — 面板行为落地（`FilterEditor.tsx` + `tableDataStore.ts`）**
改：`OPERATORS` 换成 `operatorsForColumn(columnDataType, supportedOperators)`；不完整条件加标记 + `Apply` 阻断；移除 `loadTableData` 里的 `filters.filter(isCompleteFilter)`；`setFilters` 增加可选 `visibleColumns` / `columnOrder` 承载（给视图应用用）。
为什么：这是 §4.5 的政策变更落点，也是本册唯一的**行为破坏性**改动。
自测：改写 `tableDataStore.test.ts` 的 `loadTableData omits incomplete applied filters from the request` → 新用例断言「请求体保留不完整条件、由宿主拒绝」，并新增「`applyFilters` 在不完整时不改变已应用状态、不发请求」。

**Step 11 — 命名视图 store（`src-tauri/src/store/named_views.rs` 新增；`models.rs`、`mod.rs` 修改）**
改：`NamedViewScope` / `NamedView` / `NamedViewError` + 门面四个方法 + `StoreCache` 的 `named_views` / `named_views_loaded`。
为什么：契约 §6.1 要求这批数据走既有 store 范式；复用 `sync_profiles.rs` 的懒加载 + 整体原子写，避免新造一套持久化。
自测：`src-tauri/src/store/tests/named_views_tests.rs`（§9）。

**Step 12 — 命令与注册（`src-tauri/src/commands/named_views.rs` 新增；`commands/mod.rs`、`bootstrap/run.rs` 修改）**
改：三个 `#[tauri::command]` + `*_impl`，加进 `generate_handler!`，加注册断言。
为什么：Tauri 命令不注册就永远收不到调用，而**没有任何编译错误**——必须用测试固化。
自测：`cargo test -p datazen --lib`，含注册断言。

**Step 13 — 前端命令封装与视图 UI（`src/commands/database.ts` + `src/components/filter/NamedViewMenu.tsx` 新增 + `TableView.tsx` 修改）**
改：三个 `invoke` 封装；菜单状态机（§4.6）；`TableView` 把 `context.connectionId` / `database` / `schema` / `table` 组成 scope 传下去；**同时把筛选工具条（筛选面板入口 + 视图菜单 + 快筛输入）抽成 `src/windows/connection/TableFilterToolbar.tsx`**，把 `TableView` 的净增压到接近 0（Q-13：不抽则 08+09 合计约 870 行，必超 800）。
为什么：scope 必须用**持久化连接 id**；`TableView` 是唯一持有 `TableState.context` 的地方。
自测：`NamedViewJourney.test.tsx`（§9）。

**Step 14 — i18n（`src/locales/en/query.ts` 修改）**
改：加 §8 的词条；把 `'filter.like'` 从 `'contains'` 改为 `'matches pattern'`。
为什么：新增了真正的 `contains`，两个不同语义的算子都叫 "contains" 会直接误导用户。
自测：`pnpm typecheck`（`I18nKey` 是 `TranslationKey | (string & {})`，不会因新 key 报错，所以**必须**靠关键字面检查）：用 `node scripts/i18n-sync-check.mjs` 在发布前统一补齐其他语言（开发期只改英文）。

**Step 15 — E2E（`e2e/specs/table-filter.ts` 修改 + `e2e/specs/table-saved-views.ts` 新增）**
改：TF-008 改写为「空条件 Apply 显示显式不完整提示且不发查询」；新增 BETWEEN / IN token / `%` 字面量 / 不支持算子隐藏用例；新增视图往返用例。
为什么：TF-008 固化的正是本册要改掉的旧行为；不改它就是「文档说了新行为、门禁还在守旧行为」。
自测：`pnpm e2e:minimal`（basic 驱动集含 PG / MySQL / SQLite，满足 DB-08 的驱动验收要求）。

---

## 6. 文件级改动清单

| 文件 | 新增/修改 | 职责 | 预估行数 | 是否触及 800 行上限 |
| --- | --- | --- | --- | --- |
| `packages/driver-api/src/filters.rs` | 修改 | 22 变体 + 3 常量 + 辅助方法 | +95 / −2 | 否（< 150） |
| `packages/driver-api/src/traits.rs` | 修改 | 三个带默认实现的新方法 | +35 | 否（trait 文件本就很大，但 +35 不改变量级） |
| `src-tauri/src/services/query_executor/filter_sql.rs` | **新增** | 19 算子渲染 + 转义 + 校验 + 拒绝类型（含单测） | ~360 | 否 |
| `src-tauri/src/services/query_executor.rs` → `query_executor/{mod.rs,tests.rs}` | **目录化 + 修改** | **拆出**筛选渲染到 `filter_sql.rs` 子模块；`build_select_sql` / `build_count_sql` 变 fallible；`get_table_data` 兜底校验；`filter_is_complete` 保留一行转发 | 搬运 + −150 / +60 | **是**：现状 797 行。目录化（同时搬走整块测试）后生产文件约 340 行，本册与 09 的净增都装得下。若不拆，本册**必然**突破 800。**拆分形态必须与 09 一致**（§3.3） |
| `src-tauri/src/services/mod.rs` | 修改 | `pub(crate) use query_executor::filter_sql::{validate_filters, FilterRejection};` | +2 | 否 |
| `src-tauri/src/commands/schema.rs` | 修改 | `get_table_data_impl` 前置 `validate_filters` | +18 | 否 |
| `src-tauri/src/commands/schema/tests.rs` | 修改 | 前缀断言用例 | +45 | 否 |
| `src-tauri/src/store/named_views.rs` | **新增** | DTO + 校验 + 门面 CRUD | ~190 | 否 |
| `src-tauri/src/store/models.rs` | 修改 | `StoreCache` 加 `named_views` / `named_views_loaded` | +6 | 否 |
| `src-tauri/src/store/mod.rs` | 修改 | `mod named_views;` + re-export DTO | +4 | 否（434 行，安全） |
| `src-tauri/src/store/tests/named_views_tests.rs` | **新增** | store 级 CRUD / 重名 / 幂等 / 作用域隔离 | ~170 | 否 |
| `src-tauri/src/store/tests/mod.rs` | 修改 | 挂载新测试模块 | +1 | 否 |
| `src-tauri/src/commands/named_views.rs` | **新增** | 三个命令 + `*_impl` + 注册断言 | ~150 | 否 |
| `src-tauri/src/commands/mod.rs` | 修改 | `pub mod named_views;` + re-export | +2 | 否 |
| `src-tauri/src/bootstrap/run.rs` | 修改 | `generate_handler!` 注册三个命令 | +3 | 否 |
| `src/types/index.ts` | 修改 | 22 算子联合 + 三个分组常量 | +30 | 否（852 行为整文件，本册只加常量） |
| `src/types/settings.ts` | 修改 | **删除**三处重复定义 | −20 | 否 |
| `src/lib/filterOperators.ts` | **新增** | 算子元数据表 + `CellType` 白名单 + 值形态转换 + `dataGrid` 缺省读取 | ~210 | 否 |
| `src/lib/databaseMeta.ts` | 修改 | `DataGridCapabilities`（契约 §7）**与 09 共用，只建一次**：本册补 `filterOperators?` / `likeCaseInsensitive?` | +8（若 09 先建）~ +30（若 08 先建） | 否（约 233 行） |
| `src/lib/namedViews.ts` | **新增** | 视图 DTO + 纯函数（scope key / 命名归一 / 导入命名） | ~140 | 否 |
| `src/lib/gridErrors.ts` | **新增（与 09 共用，只建一次）** | `GridErrorCode` + `classifyGridError`；本册负责 `grid.filter.*` / `grid.view.*` 成员 | ~90（若 08 先建）/ +25（若 09 先建） | 否 |
| `src/lib/__tests__/filterOperators.test.ts` | **新增** | 元数据 / 顺序 / 转换用例 | ~180 | 否 |
| `src/lib/__tests__/namedViews.test.ts` | **新增** | DTO 纯函数用例 | ~120 | 否 |
| `src/lib/__tests__/gridErrors.test.ts` | **新增** | 前缀匹配 + 回退用例 | ~70 | 否 |
| `src/components/filter/FilterValueInput.tsx` | **新增**（从 `FilterEditor.tsx` 搬出） | 单值输入 + IME + 防抖 | ~110 | 否 |
| `src/components/filter/FilterTokenInput.tsx` | **新增** | token 输入 + IME | ~150 | 否 |
| `src/components/filter/FilterBetweenInput.tsx` | **新增** | 上下界两输入 + 颠倒提示 | ~90 | 否 |
| `src/components/filter/FilterConditionEditor.tsx` | **新增**（搬出 + 扩展） | 列/算子/值控件分派 | ~180 | 否 |
| `src/components/filter/FilterConditionChip.tsx` | **新增**（搬出） | 折叠 chip + 不完整标记 | ~90 | 否 |
| `src/components/filter/NamedViewMenu.tsx` | **新增** | 视图菜单状态机 | ~230 | 否 |
| `src/components/FilterEditor.tsx` | 修改 | 只留面板壳 + 状态机 + re-export 常量 | 558 → ~330 | **是**：拆分前 558，若把三种值控件与视图菜单直接塞进去会突破 800 |
| `src/components/__tests__/FilterEditor.test.tsx` | 修改 | 追加新算子/断言；**既有断言不改** | +180 | 否 |
| `src/components/__tests__/FilterEditorJourney.test.tsx` | **新增** | 连续旅程（击键全过程） | ~220 | 否 |
| `src/components/__tests__/NamedViewJourney.test.tsx` | **新增** | 视图旅程 | ~180 | 否 |
| `src/stores/tableData/filterUtils.ts` | 修改 | `isCompleteFilter` 支持 22 算子（含 `pair` / `tokens`） | +25 | 否 |
| `src/stores/tableDataStore.ts` | 修改 | 移除静默剥离；`setFilters` 扩展可选入参 | +20 / −3 | **临界（与 09 共用，必须并账）**：现状 722 行，09 净增约 25 → 约 747，本册再净增约 17 → 约 764，仍在 800 以内但余量只剩约 36 行。**若任一册超预算，先按 09 的建议把 `commitFetchedPage` 抽到 `src/stores/tableData/fetchedPage.ts`** |
| `src/stores/__tests__/tableDataStore.test.ts` | 修改 | 改写 incomplete 用例 + 新增阻断用例 | +60 / −15 | 否 |
| `src/stores/tableData/types.ts` | 修改 | 加可选的 `columnOrder`（视图承载用） | +5 | 否 |
| `src/commands/database.ts` | 修改 | 三个视图命令封装 | +30 | 否 |
| `src/windows/connection/TableView.tsx` | 修改 | 组装 scope + 挂视图菜单 | +40 | **是（与 09 冲突，见 Q-13）**：现状 770 行，09 约 +60、本册约 +40 → **合计约 870，必超 800**。必须在合并前把筛选工具条抽成 `src/windows/connection/TableFilterToolbar.tsx`（筛选面板入口 + 视图菜单 + 快筛输入三块整体搬走，`TableView` 只留 props 接线） |
| `src/locales/en/query.ts` | 修改 | 新增筛选/视图/错误词条；改 `filter.like` 文案 | +45 / −1 | 否 |
| `e2e/specs/table-filter.ts` | 修改 | TF-008 改写 + 新算子用例 | +120 / −25 | 否 |
| `e2e/specs/table-saved-views.ts` | **新增** | 视图往返 E2E | ~180 | 否 |

| `src/windows/connection/TableFilterToolbar.tsx` | **新增（为拆 800 行而抽，见上）** | 筛选面板入口 + 命名视图菜单 + 快筛输入三块的整体搬移 | ~200 | 否 |

**四个必须盯 800 行的文件（其中两个是与分册 09 的共用热文件）**：

1. `src-tauri/src/services/query_executor.rs`（797）：**必须目录化**，拆出 `filter_sql.rs` 子模块与整块测试；形态与 09 一致（§3.3）。
2. `src/components/FilterEditor.tsx`（558）：拆出 `src/components/filter/*` 子组件后约 330。
3. `src/windows/connection/TableView.tsx`（770）：**本册与 09 合计必超 800**，必须在合并前抽 `TableFilterToolbar.tsx`（Q-13）。
4. `src/stores/tableDataStore.ts`（722）：本册与 09 合计约 764，余量仅约 36 行；再叠加 DB-02 的快捷键或 DB-11 的多列排序就必须先抽 `fetchedPage.ts`。

---

## 7. 边界与异常清单

| 场景 | 期望行为 |
| --- | --- |
| 空值（`Value::Null`）配 `Eq`/`Like`/`Contains` | 不完整 → 拒绝（`grid.filter.incomplete`）。**不能用 `= NULL` 表达「找 NULL 行」**，那是 `IsNull`；`format_lit(&Value::Null)` 产出 `NULL` 而 `col = NULL` 永远不为真，静默返回 0 行是最坏结果 |
| 空字符串 `''` 配 `Eq` | 不完整 → 拒绝（今天也是不完整，只是今天静默跳过）。想查空串用 `IsEmpty`，想查非空串用 `IsNotEmpty` |
| 空字符串配 `Contains` | 不完整 → 拒绝。语义上 `LIKE '%%'` 会匹配所有非 NULL 行，但用户点「包含空」几乎必然是想查空串，拒绝比返回全表更安全 |
| `NULL` 与非 `NULL` 行 | `IsNull` → `col IS NULL`；`IsNotNull` → `col IS NOT NULL`；`IsEmpty` → 只命中 `''`，**不命中 `NULL`**；文本族的 `Contains` 类**不命中 `NULL`**（`LIKE` 对 `NULL` 为 `UNKNOWN`）。这三条要在 UI 文案与文档里一致，避免「为什么搜索没搜到那行」的困惑 |
| `In` / `NotIn` 空列表 | 不完整 → 拒绝。**不允许**把 `IN ()` 发到数据库（语法错误），也不允许降级成「无条件」 |
| `In` 的 token 里含逗号 | token 输入不被逗号截断（`,` 是分隔符，要在值里表达逗号需粘贴后整体成一个 token —— UI 明确提示）。后端收到的是数组，不重新切分，所以值里的逗号安全 |
| `Between` 上下界顺序颠倒 | **不自动交换**；两值同类型可比较时显示「结果必为空」警告，允许 Apply（数据库返回 0 行，与输入一致）。见 §4.4 |
| `Between` 只填一个界 / 超过 2 个元素 | 不完整 → 拒绝（`literal_arity == Two`，元素数必须恰好 2） |
| 值里含 `%` 或 `_`（`Contains` / `StartsWith` / `EndsWith` / `NotContains`） | 转义为 `!%` / `!_`，并显式加 `ESCAPE '!'`。`50%` 只匹配「含字面量 `50%` 的行」。**与 `Like` 相反**：`Like` 不转义，用户写 `%` 就是通配符 |
| 值里含转义符 `!` 自身 | 转义为 `!!`。否则用户搜 `a!b` 会与转义序列混淆 |
| 值里含单引号 `'` | **宿主不处理**，交给驱动的 `format_sql_literal`（今天它负责把 `'` 变成 `''`）。宿主只在**模式体**里替换 `!`/`%`/`_`，绝不碰引号，避免出现「两处都转义」导致的双写 |
| 值里含反斜杠 `\` | 原样保留（宿主不把 `\` 当转义符）。这也是选 `!` 而不是 `\` 的收益：MySQL 默认把字面量里的 `\` 再解释一次，选 `\` 会让 `\%` 退化成 `%` |
| 多字节字符（CJK、emoji） | 转义按 `char` 遍历，不按字节切片，不做任何长度截断；`%`/`_`/`!` 都是 ASCII，多字节值原样通过 |
| 超长值 | 宿主不截断（截断会造成「筛选条件与用户输入不符」）；由数据库的长度语义决定（多数方言对字符串字面量无硬上限）。UI 不设 `maxLength` |
| 日期时间 | **宿主与 UI 都不做时区换算、不补 `T00:00:00`**；值以用户输入的字符串（或驱动的 `Value::Timestamp`）进入 `format_sql_literal`，解释权在数据库。UI 给格式示例提示，不给日期选择器（避免引入一个「本地时间 vs 库时区」的隐性换算）。`Between` 的两个界各自独立，闭区间语义由 SQL `BETWEEN` 保证 |
| 驱动不支持的算子 | 宿主 `validate_filters` 拒绝，前置在校验阶段：`grid.filter.unsupportedOperator` + **给出可替代算子**（`regex`/`notRegex` → `like`/`contains`；`jsonContains` → `eq`/`contains`）。UI 侧同时按驱动白名单**隐藏**该算子入口（双保险：入口不出现，绕过入口也被拒） |
| 驱动白名单与实际能力不一致（驱动声明支持但 SQL 报错，或声明不支持但实际可用） | 以后端错误为准（契约 §7）：报错时展示 `grid.filter.unsupportedOperator` 或原始驱动消息；不静默改写用户条件 |
| 筛选后无结果（`rows.length === 0` 且 `filters.length > 0`） | 空状态文案「没有匹配的行」+ 一键 `Clear`，**不得**渲染成加载失败或错误页（`TableView` 已刻意在失败时保留筛选条，方向一致） |
| 筛选后行数统计 | `build_count_sql` 的 WHERE 与 SELECT 完全同源，所以精确计数就是筛选后的行数（**新增用例** `build_count_sql_where_matches_select_where` 锁死这条）。**与 DB-09 的配合**：分册 09 已把「有生效筛选时禁止用估算行数」写进它的验收点 **A7**，并在其 §7 用 `has_effective_filters`（复用 `filter_is_complete`）+ 「`Estimated` → 降级 `Exact`」最后一道闸落地；本册**不重复归口**，只声明两条纪律：(1) 估算语句是表级目录统计、套不上 `WHERE`，有生效筛选时显示估算值等于撒谎；(2) `has_effective_filters` 的定义建立在「不完整条件不产生 `WHERE`」之上——本册 §4.5 若把不完整条件改成**入口拒绝**（Q-1），则该分支在 IPC 路径上不再可达（服务层直连调用方仍可达），**09 的实现不需要改，但不得依赖该分支来产生「无生效筛选」的结论**。这条耦合写进 Q-1 的裁定说明 |
| 筛选与排序组合 | 同一条 SELECT 里 WHERE 在 ORDER BY 前；筛选项与排序项互不影响。排序默认仍注入主键（DB-09 会改这条，本册不动）。筛选列的 `NULL` 排序位置由数据库决定，不做客户端补偿 |
| 筛选后当前页超界 | 应用/清空筛选/切换视图时一律 `page: 0`（`patchPanelForReload` 已保证）。其它路径（如从第 5 页直接改条件）若返回 `rows.length === 0 && totalRows > 0 && page > 0`，UI 记一次 `resetPageOnce` 标志 → `setPage(0)` 重取一次，**用 `requestRevision` 防止循环**；不重复触发（每次筛选变更新增一个 revision，标志随之复位） |
| 快筛表达式（`parseFilterForApply`） | 既有 8 算子解析器本册不动（`==` / `<>` 的接受范围属表达式语法演进）；它产出的条件如果含不完整值，走同一条拒绝路径 |
| 快速筛选与人工面板混用 | 快筛成功即 `setFilters`，会覆盖人工草稿（今天如此）；本册不改这条语义，但要求在 UI 上可见（应用后摘要行变化），不静默 |
| 视图恢复出不存在的列（表结构变了） | 恢复时把 `filters` 里列名不在当前 `columns` 的条件标为「列已不存在」并**拒绝应用**（提示用户改条件），不静默丢弃、不拼出会报错的 SQL |
| 视图重名 / 改名冲突 | 同 scope 内名称唯一（大小写不敏感）；冲突 → `grid.view.nameExists` + 覆盖确认；覆盖以被覆盖者 id 保存，列表不留两条同名 |
| 删除不存在的视图 | 幂等 `Ok`，返回剩余列表 |
| `named_views.json` 单条损坏 | 逐条容错解析，坏记录跳过（照抄 `sync_profiles.rs`），其余照常可用；不因一条坏数据让整个菜单为空 |
| 视图文件里的 `connectionId` 指向已删除的连接 | 该视图不出现在任何表的菜单里（scope 不匹配），也不主动删除（用户可能恢复连接）；在「视图管理」里标记为孤儿 |

---

## 8. i18n key 清单

**落点**：`src/locales/en/query.ts`（现有 `filter.*` 词条已在该文件）。**不改** `src/locales/en.ts`（它只有一行再导出，改它无效）；**不改**其他语言文件（发布前由 `scripts/i18n-sync-check.mjs` + i18n 同步流程统一补齐）；**不新建域名**。

**修改 1 条（有意变更既有英文文案）**

| key | 现值 | 新值 | 原因 |
| --- | --- | --- | --- |
| `filter.like` | `'contains'` | `'matches pattern'` | 本册新增了真正的 `contains`（字面量子串，`%` 被转义）。两个语义相反的算子都叫 "contains" 会让用户以为 `like` 也会转义 `%`，直接导致错数据。改文案不是改渲染行为（B1/B2 只约束 SQL 与相对顺序） |

**新增（算子标签，延续既有 `filter.<op>` 命名）**

`filter.notIn`、`filter.between`、`filter.notBetween`、`filter.contains`、`filter.notContains`、`filter.startsWith`、`filter.endsWith`、`filter.isEmpty`、`filter.isNotEmpty`、`filter.regex`、`filter.notRegex`、`filter.jsonContains`

**新增（值控件与不完整提示）**

| key | 建议英文 | 用途 |
| --- | --- | --- |
| `filter.valueTo` | `'and'` | `Between` 两个输入框之间的连接符（若不想复用 `filter.and`） |
| `filter.tokensPlaceholder` | `'Type a value, press Enter'` | token 输入占位符 |
| `filter.tokenRemove` | `'Remove value'` | token 上的 `×` 的 `aria-label` |
| `filter.incomplete` | `'Needs a value, not applied'` | 条件行内标记 |
| `filter.errorIncomplete` | `'Filter not applied: one condition has no value yet (column "{column}")'` | `Apply` 被阻断时的原因块 |
| `filter.errorUnsupportedOperator` | `'This database does not support the "{operator}" filter. Try {alternatives}.'` | `grid.filter.unsupportedOperator` 的本地化文案 |
| `filter.betweenInverted` | `'The lower bound is greater than the upper bound — this returns no rows'` | `pair` 颠倒警告（不阻断） |
| `filter.likeCaseInsensitiveHint` | `'LIKE is case-insensitive under this database's default collation'` | 只在 `like_is_case_insensitive() == true` 时显示 |
| `filter.noMatches` | `'No rows match the current filters'` | 筛选后无结果的空状态 |
| `filter.scanCostWarning` | `'"{operator}" cannot use an index and may scan the whole table (~{count} rows)'` | `contains` 类前缀通配符的代价提示（`count` 来自 DB-09 的行数或该表的估算值；`count` 取不到时不显示括号部分） |

**新增（命名视图，统一 `filter.view.` 前缀）**

`filter.view.savedViews`（`'Saved views'`）、`filter.view.save`（`'Save'`）、`filter.view.saveAs`（`'Save as…'`）、`filter.view.rename`（`'Rename'`）、`filter.view.delete`（`'Delete'`）、`filter.view.deleteConfirm`（`'Delete the saved view "{name}"?'`）、`filter.view.nameLabel`（`'View name'`）、`filter.view.namePlaceholder`（`'e.g. Open orders this week'`）、`filter.view.empty`（`'No saved views for this table'`）、`filter.view.nameExists`（`'A view with this name already exists for this table'`）、`filter.view.overwrite`（`'Overwrite'`）、`filter.view.setDefault`（`'Set as default'`）、`filter.view.defaultBadge`（`'Default'`）、`filter.view.draftNotAppliedHint`（`'Saving the applied filters — you still have unapplied changes'`）、`filter.view.applyAnyway`（`'Apply first'`）、`filter.view.export`（`'Export…'`）、`filter.view.import`（`'Import…'`）、`filter.view.importDone`（`'{count} views imported'`）、`filter.view.importFailed`（`'Import failed: {error}'`）、`filter.view.missingColumn`（`'Column "{column}" no longer exists on this table'`）、`filter.view.errorNameExists`（`'A saved view with this name already exists'`）

**两条文案纪律**

1. **`like_is_case_insensitive` 只影响文案，不影响 SQL**。写法上必须遵守：`true` 时显示 `filter.likeCaseInsensitiveHint`（「本数据库默认排序规则下 LIKE 不区分大小写」）；`false` 时**什么都不显示**——绝不写「区分大小写」。原因：`false` 只是驱动级默认（默认实现返回 `false`），列级 `COLLATE` 仍可能让某列不区分大小写，写成「区分大小写」就是把一个驱动级缺省说成了列级事实，属于误导。
2. **`IsEmpty` 的文案必须写「空字符串」**，不能写「为空」。`filter.isEmpty` 建议 `'is empty string'`、`filter.isNotEmpty` 建议 `'is not empty string'`；`is null` / `is not null` 已有独立词条，两者不可混用。

---

## 9. 测试清单

### 9.1 Rust 单测（`cargo test -p datazen --lib`、`cargo test -p datazen-driver-api --lib`）

**`packages/driver-api/src/filters.rs`（`#[cfg(test)] mod tests`）**

| 用例名 | 断言要点 |
| --- | --- |
| `serde_spelling_is_stable_for_all_operators` | 22 个变体逐个 `serde_json::to_string` 等于 `"eq"` / `"notIn"` / `"jsonContains"` …（锁死线上拼写；前端字符串联合与之逐字对应） |
| `operator_groups_are_disjoint_and_complete` | `EXISTING_FILTER_OPERATORS.len() == 10`、`SHARED_FILTER_OPERATORS.len() == 19`、`DIALECT_FILTER_OPERATORS.len() == 3`；`EXISTING` 的 `as_str()` 序列**逐元素等于** `["eq","ne","gt","lt","gte","lte","like","in","isNull","isNotNull"]`；三组两两无交集、并集 22 |
| `literal_arity_matches_render_matrix` | `IsNull`家族 0、`Between`家族 2、`In`家族 `Many`、其余 1 |

**`packages/driver-api`（trait 默认实现，放在 `traits.rs` 的 `#[cfg(test)]` 内，用 `MockDriver`）**

| 用例名 | 断言要点 |
| --- | --- |
| `default_supported_filter_operators_is_shared_set` | `mock.supported_filter_operators() == SHARED_FILTER_OPERATORS` |
| `default_filter_operator_sql_is_none` | 对 `Regex`/`NotRegex`/`JsonContains` 逐个断言 `is_none()` |
| `default_like_is_case_sensitive_flag_is_false` | `!mock.like_is_case_insensitive()` |

**`src-tauri/src/services/query_executor/filter_sql.rs`（`#[cfg(test)] mod tests`）**

| 用例名 | 断言要点 |
| --- | --- |
| `legacy_ten_operators_render_byte_identical` | 逐个算子 `assert_eq!` **整串**：`"name" = 'x'`、`"name" != 'x'`、`"name" > 'x'`、`"name" < 'x'`、`"name" >= 'x'`、`"name" <= 'x'`、`"name" LIKE 'x%'`、`"id" IN ('1', '2', '3')`（Json 数组）、`"name" IN ('a', 'b')`（逗号串）、`"deleted_at" IS NULL`、`"deleted_at" IS NOT NULL`。**这是 B1 的核心证据**（沿用既有 `simple_qi` / `simple_lit` 夹具，断言文本与既有用例一字不差） |
| `in_comma_string_with_only_blank_parts_is_rejected` | 逗号串全是空白 → `Err(Incomplete)`（**替代**既有的静默 `String::new()`；这是政策变更的显式固化） |
| `not_in_mirrors_in_element_formatting` | `NotIn` 的元素格式化路径与 `In` 完全同构（同输入下把 ` NOT IN ` 换成 ` IN ` 即得 `In` 结果） |
| `between_renders_two_literals_in_order` | `Value::Json([1,5])` → `"score" BETWEEN '1' AND '5'`；颠倒输入 `[5,1]` → 仍按输入顺序渲染 `BETWEEN '5' AND '1'`（宿主不交换） |
| `between_comma_string_splits_on_first_comma_only` | `"2024-01-01,2024-02-01"` → 两界正确；`"a,b,c"` → `Err(Incomplete)`（元素数 ≠ 2） |
| `contains_escapes_wildcards_and_escape_char` | `Contains('50%')` → `"name" LIKE '%50!%%' ESCAPE '!'`；`Contains('a_b')` → `'%a!_b%'`；`Contains('a!b')` → `'%a!!b%'` |
| `starts_with_and_ends_with_put_wildcard_on_one_side` | `StartsWith('a_')` → `'a!_%'`；`EndsWith('a_')` → `'%a!_'` |
| `not_contains_uses_not_like_with_same_escaping` | 前缀 `NOT LIKE`，模式与 `Contains` 同 |
| `is_empty_compares_to_empty_string_only` | `"name" = ''` / `"name" <> ''`（**不含** `IS NULL`） |
| `regex_without_driver_support_is_rejected_with_contract_prefix` | `Err(UnsupportedOperator)`，且 `contract_message().starts_with("grid.filter.unsupportedOperator: ")` |
| `regex_with_driver_support_uses_driver_sql_verbatim` | 注入 `|_, col, lit| Some(format!("{col} ~ {lit}"))` 的假回调 → 结果逐字等于 `"name" ~ '^a'`（宿主不加工） |
| `incomplete_condition_is_rejected_not_silently_dropped` | `Eq` + `''` → `Err(Incomplete)`；消息前缀 `grid.filter.incomplete: `；**这是「既有行为显式变更」的固化石**（对应被改写的 `incomplete_eq_filter_is_skipped`） |
| `complete_and_incomplete_mixed_list_reports_the_offending_column` | 一条完整 + 一条不完整 → `Err`，且消息里出现不完整那一条的列名；**不再**出现「完整那条被照常执行」的结果 |
| `filter_join_defaults_to_and` | 逐字搬移既有断言（`None`/`"and"` → `" AND "`，`"or"` → `" OR "`） |
| `multibyte_pattern_is_not_byte_sliced` | `Contains('中文🙂%')` → 模式体里 CJK/emoji 原样、`%` 被转义，且输出逐字等于预期全串 |
| `single_quote_in_value_is_left_to_driver_formatter` | 值 `O'Brien` 交给夹具 `simple_lit` → `'O''Brien'`，宿主没有二次转义（输出里不出现 `''''`） |
| `literal_arity_mismatch_is_rejected_for_between_and_in` | 1 元素 / 3 元素 → `Err(Incomplete)` |

**`src-tauri/src/services/query_executor/mod.rs` 与 `query_executor/tests.rs`（既有 tests 模块，搬迁 + 修改 + 新增）**

> 搬迁形态与分册 09 一致：整块 `#[cfg(test)] mod tests` 移入 `query_executor/tests.rs`，`mod.rs` 只留生产代码。若 09 先合并，本册只做「补用例」，不做搬迁。

| 用例名 | 断言要点 |
| --- | --- |
| `legacy_filter_full_select_sql_is_golden`（新增） | 对一条 `Eq` 条件，`assert_eq!` 整条 SQL：`SELECT "id" FROM "users" WHERE "name" = 'alice' ORDER BY "id" ASC LIMIT 25 OFFSET 0`（含默认主键排序与分页子句，锁死端到端串） |
| `build_count_sql_with_eq_filter` / `build_select_sql_joins_filters_with_or` / `build_count_sql_joins_filters_with_or` / `filter_literals_use_formatter`（既有，改调用形状） | 断言文本一字不改；只补新的 `dialect_op` 参数（`&no_dialect_ops`）与 `.expect(...)` |
| `build_count_sql_where_matches_select_where`（新增） | 同输入下 count 与 select 的 WHERE 子串逐字相等（保证「筛选后行数」与行集一致） |
| `unsupported_dialect_operator_fails_the_whole_query`（新增） | `Err(DriverError::NotSupported)`，消息前缀是 `grid.filter.unsupportedOperator: ` |
| `tsql_pagination_*` / `no_explicit_sort_*` / `no_pk_no_explicit_sort_*`（既有） | 全部保持绿（本册不动排序注入路径，这组是「没误伤」的证据） |

**`src-tauri/src/commands/schema/tests.rs`**

`get_table_data_rejects_incomplete_filter_with_prefix`、`get_table_data_rejects_unsupported_operator_with_prefix`：断言 `Err(CommandError::Validation(msg))` 且 `msg.starts_with("grid.filter.incomplete: ")` / `"grid.filter.unsupportedOperator: "`。

**`src-tauri/src/store/tests/named_views_tests.rs`**

| 用例名 | 断言要点 |
| --- | --- |
| `named_view_round_trip_survives_reopen` | 保存 → 用 `init_store_for_test` 重新打开同一目录 → `get_named_views()` 得到逐字段相等的记录（证明真的落盘） |
| `save_with_empty_id_mints_ulid_and_created_at` | 返回记录的 `id` 通过 `Ulid::is_valid`，`created_at` 非空，原对象不被就地修改之外无副作用 |
| `duplicate_name_in_same_scope_is_rejected` | `NamedViewError::NameExists`，消息前缀 `grid.view.nameExists: ` |
| `same_name_in_different_scope_is_allowed` | 不同 `table` 或不同 `connectionId` → 都能保存 |
| `rename_keeps_id_and_updates_updated_at` | 同 id + 新名 → 列表长度不变、id 不变、`updated_at` 更新 |
| `delete_is_idempotent` | 删两次都 `Ok`，第二次剩余列表不变 |
| `only_one_default_per_scope` | 连续把两条设为默认 → 该 scope 内恰有一条 `is_default` |
| `corrupt_record_does_not_hide_healthy_records` | 手写一个坏 JSON 数组（一条坏 + 一条好）→ 加载后好记录仍在 |
| `view_with_invalid_filter_logic_is_rejected` | `filter_logic: "xor"` → `NamedViewError::Invalid` |

### 9.2 Host 前端单测（`npx vitest run`，须过 `pnpm typecheck`）

| 文件 | 用例名 | 断言要点 |
| --- | --- | --- |
| `src/lib/__tests__/filterOperators.test.ts` | `existing_operators_keep_relative_order_for_every_cell_type` | 每个 `CellType` 的 `operatorsForColumn` 结果过滤出既有算子子序列后，**逐元素等于** `['eq','ne','gt','lt','gte','lte','like','in','isNull','isNotNull']` 的相应子序列（B2 的证明） |
| 同上 | `cell_types_match_the_designed_matrix` | 九个 `CellType`（`text` / `enum` / `number` / `bool` / `date` / `time` / `datetime` / `json` / `bytes` / `uuid` / `unknown`）的集合与 §3.5 表逐项相等 |
| 同上 | `supported_operators_defaults_to_the_shared_nineteen` | `supportedOperatorsForMeta(undefined)` 与 `supportedOperatorsForMeta({})` 都等于 19 个共享算子；`dataGrid.filterOperators` 里不含 `regex` 时结果里没有它 |
| 同上 | `dialect_operator_only_visible_when_whitelisted` | `supported` 不含 `regex` 时 `text` 的结果里没有 `regex`；含则出现在 `isNotNull` 之后 |
| 同上 | `like_case_insensitive_flag_defaults_false` | `likeCaseInsensitiveForMeta(undefined)` 为 `false`；驱动显式 `true` 时为 `true`（仅影响文案） |
| 同上 | `default_operator_is_eq_for_every_family` | `defaultOperatorForColumn(undefined|'int4'|'text'|'bool'|'jsonb'|'bytea') === 'eq'` |
| 同上 | `convert_filter_value_*` | 六条转换（single↔tokens、single↔pair、tokens↔pair、任意→none 保值）逐条断言；`pair → single` 取下界；`pair` 从 single 生成时下半界为空串 |
| 同上 | `value_input_kind_per_operator` | 22 个算子的 `valueInput` 与矩阵一致（`none` 恰好 4 个：`isNull`/`isNotNull`/`isEmpty`/`isNotEmpty`） |
| `src/lib/__tests__/gridErrors.test.ts` | `classify_prefix_matching` | 契约 §8.2 的每个前缀各一例；`grid.filter.unsupportedOperator: …` → `'unsupportedOperator'`（或联合同名成员）；大小写与首尾空白鲁棒 |
| 同上 | `unknown_message_falls_back_to_raw` | 任意无关消息 → `'unknown'`（UI 必须原样显示，不吞错） |
| 同上 | `prefix_only_matches_at_start` | `'error: grid.filter.incomplete'`（前缀不在最前）→ `'unknown'`（证明是前缀匹配而不是包含匹配） |
| `src/lib/__tests__/namedViews.test.ts` | `scope_key_is_connection_scoped_and_stable` | 同输入同输出；换 `connectionId` 则不同；`null` 库/模式与空串区分 |
| 同上 | `view_from_table_state_captures_applied_not_draft` | 传入 draft ≠ applied 的状态 → 产出的 `filters` 等于 applied |
| 同上 | `export_import_round_trip` | 序列化再反序列化后逐字段相等；`version` 字段存在 |
| 同上 | `import_name_conflict_gets_suffix` | `resolveImportName('Users', {'users'})` → `'Users (2)'`（大小写不敏感比较） |
| `src/stores/tableData/__tests__/filterUtils.test.ts`（修改） | `isCompleteFilter_for_new_operators` | `between` 需 2 个非空；`in`/`notIn` 需 ≥1 个非空 token（数组或逗号串）；`isEmpty`/`isNotEmpty`/`isNull`/`isNotNull` 无值也算完整；`regex` 需值；`between` 收 `Value::Json` 数组时判完整 |
| 同上（既有） | `isCompleteFilter rejects incomplete filters` / `accepts null-check and complete value filters` | **断言不改**，全部保持绿 |
| `src/stores/__tests__/tableDataStore.test.ts`（修改） | `loadTableData_keeps_incomplete_applied_filters_on_the_wire` | **改写**原 `omits incomplete applied filters from the request`：请求体里**保留**不完整条件（`filters` 长度 2），由宿主拒绝（证明前端不再静默剥离） |
| 同上（新增） | `applyFilters_with_incomplete_draft_does_not_change_applied_state` | `filters` 不变、`requestRevision` 不变、`getTableData` 未被调用（阻断发生在 UI 层） |
| 同上（新增） | `setFilters_from_named_view_sets_page_zero_and_applied_state` | `page === 0`、`filters`/`filterLogic`/`visibleColumns` 同步为视图值、`draftFilters` 跟随 |
| 同上（新增） | `filtered_empty_page_resets_page_once` | 模拟「第 3 页 + 筛选后 0 行 + totalRows > 0」→ 只触发一次回第 0 页，且不会无限循环 |
| `src/components/__tests__/FilterEditor.test.tsx`（修改） | `renders_nineteen_host_operators_for_text_column_in_stable_order` | 文本列 + 共享白名单 → 选项的 `value` 序列与 `operatorsForColumn` 结果逐元素相等 |
| 同上（新增） | `hides_dialect_operator_when_driver_does_not_support_it` | 白名单无 `regex` → 选项里没有 `regex`（R4：隐藏入口而不是禁用/报错） |
| 同上（新增） | `between_renders_two_value_inputs` | `between` 条件展开后有两个 `data-testid="filter-value"`，上下界分别绑定 |
| 同上（新增） | `in_renders_token_input_and_enter_creates_token` | 输入 `a` + Enter → 出现一个 token chip；`,` 同样成 token |
| 同上（新增） | `ime_composition_enter_does_not_create_token` | `compositionstart` → `keydown Enter`（`isComposing: true`）→ **不**新增 token；`compositionend` 后才可 |
| 同上（新增） | `switching_eq_to_between_seeds_lower_bound` | 上界 = 原值、下界为空、条件立即被判不完整 |
| 同上（新增） | `switching_to_isnull_hides_value_and_keeps_draft_value` | 值输入消失但草稿里的值保留（切回 `eq` 复原） |
| 同上（新增） | `apply_is_blocked_with_incomplete_reason` | `filter-error` 出现、文案由 mock `t` 返回 `filter.errorIncomplete`、`onApply` 未被调用 |
| 同上（新增） | `escape_closes_expanded_condition_before_panel` | 第一次 Escape 收起正在编辑的条件、第二次才收起面板 |
| 同上（新增） | `named_view_menu_save_uses_applied_filters` | dirty 状态下点 `Save as…` → 提交的 `filters` 是 applied，且出现 `filter.view.draftNotAppliedHint` |

### 9.3 连续旅程测试（Journey：模拟击键全过程，含残缺中间态）

`src/components/__tests__/FilterEditorJourney.test.tsx`：**一条**长旅程，逐步骤断言状态跃迁（不是多个孤立合法态）。

1. 打开面板 → 断言 `filter-editor` 出现、焦点落到第一个条件（无条件时落到 `filter-add`）。
2. 点 `filter-add` → 断言自动生成 `{ column: 首列, operator: 'eq', value: '' }`。
3. **不填值**直接点 `filter-apply` → 断言 `onApply` 未被调用 + `filter.incomplete` 标记 + 原因块（这是「不含蓄」的关键一步）。
4. 在值输入框逐字键入 `a`、`l`、`i`（每次 `fireEvent.change`）→ 断言 350ms 防抖内**不**提交、超过后只提交一次（`onChange` 调用计数）。
5. 按 Enter → 断言条件折叠成 chip 且 show chip 文本 `id = ali`。
6. 点 chip 重新展开 → 算子切到 `contains` → 键入 `50%` → 断言草稿值是**原始** `50%`（转义发生在 Rust 侧，UI 不做任何改写）。
7. 算子切到 `between` → 断言出现两个输入框、上界是 `50%`、下界空 → 断言条件返回「不完整」态。
8. 算子切到 `in` → 断言 token 模态、输入 `x` + `,` 成 token、再输入 `中文` 并模拟 IME 合成 Enter（**不**成 token）、`compositionend` 后再 Enter 成 token。
9. 点 `filter-apply` → 断言 `onApply` 被调用一次、`page` 归零（经 store 断言）。
10. 点 `filter-clear` → 断言 applied 与 draft 同时清空、面板保持打开（既有语义）。
11. 按 Escape（面板无已应用筛选）→ 断言组件返回 `null`。

`src/components/__tests__/NamedViewJourney.test.tsx`：打开视图菜单（`loading` → `list`）→ `Save as…` → 输入名 → 保存（`saveNamedView` 收到 applied filters）→ 菜单列表出现新条目 → 行内重命名（同 id）→ 设为默认（`defaultBadge` 出现）→ 导入一个含同名条目的文件（断言出现 `(2)` 后缀）→ 删除（确认对话 → 条目消失）→ 命令失败注入 → 断言 `error` 态显示 `classifyGridError` 后的文案。

### 9.4 E2E（`pnpm e2e:minimal`）

| 文件 | 用例 | 断言要点 |
| --- | --- | --- |
| `e2e/specs/table-filter.ts`（修改） | TF-008「空条件 Apply 显示未填值提示且不发起查询」 | **改写**：断言出现 `filter-error`（`grid.filter.incomplete` 的本地化文本）与行内标记；表内容不变；**不**出现「加载失败」 |
| 同上（新增） | TF-012「BETWEEN 两个输入框生成闭区间结果」 | 种子数据下界/上界各含边界行；`SQL` 预览（DB-15 落地后）或结果集能区分开区间 |
| 同上（新增） | TF-013「IN token 输入多值」 | 输入 `a,b` 与 `a`+Enter 得到同一结果集；值里含逗号的行仍能被单独选中 |
| 同上（新增） | TF-014「Contains 把 `%` 当字面量」 | 种子数据含 `50%` 与 `5000` 两行；`contains '50%'` **只**返回 `50%` 那行（这条同时证明转义与 `ESCAPE '!'` 在真实驱动上生效；`like '50%'` 则返回两行，二者对照） |
| 同上（新增） | TF-015「SQLite 不出现 regex 算子」 | 切到 SQLite 连接的表，算子下拉里没有 `regex`；若通过快筛表达式构造 `regex`，得到明确错误而不是原始 SQL 报错 |
| `e2e/specs/table-saved-views.ts`（新增） | SV-001 保存并切换 | 保存 → Clear → 从菜单应用 → 行集恢复一致、`filter-summary-toggle` 文本一致 |
| 同上 | SV-002 重启后仍在 | 关闭标签再开（或重连）→ 菜单里条目仍在且可应用（证明 `{appData}/named_views.json` 真落盘） |
| 同上 | SV-003 重命名 / 删除 / 设为默认 | 重命名后 id 不变（菜单不出现两条）；删除有二次确认；默认徽标唯一 |
| 同上 | SV-004 导出再导入 | 导出文件 → 删除 → 导入 → 条目回来（重名场景得到 `(2)` 后缀） |
| 同上 | SV-005 跨连接不共享 | 换到另一个连接的同名表 → 菜单为空（作用域隔离） |

---

## 10. 自查清单

| # | 错误做法 | 正确做法 | 后果 |
| --- | --- | --- | --- |
| 1 | 在 `format_condition` 里为 `Contains` 直接拼 `ILIKE`，或按 `driver_type()` 写 `if mysql { … }`；或者：给 `FilterOperator` 加了变体、只在 UI 加了选项，却没做 `supported_filter_operators` 运行时白名单 | 共享算子恒由宿主渲染并走标准 `LIKE … ESCAPE`，方言专有算子必须走 `filter_operator_sql`；同时宿主 `validate_filters` 前置拒绝 + `grid.filter.unsupportedOperator` + UI 按白名单隐藏（入口不出现、绕过入口也被拒） | 前者让宿主出现按库名分支（违反零硬编码）、方言错 SQL 在 basic 驱动集上看不出来；后者编译能过、点下去数据库报错，用户看到含表名列名的原始 SQL 错误 |
| 2 | 用反斜杠当 LIKE 转义符（`\%`） | 用 `!` 并显式加 `ESCAPE '!'` | MySQL 默认把字面量里的 `\` 再解释一次，`'\%'` 退化成 `%`：用户搜 `50%` 会命中所有含 `50` 的行，**静默返回错数据** |
| 3 | 把 `contains` 实现成「值两侧拼 `%`，不转义用户输入」 | 先 `escape_like_pattern` 再拼通配符；`Like` 与 `Contains` 的转义策略必须相反 | 搜 `a_b` 会匹配 `axb`；用户拿到看着合理但错误的结果集 |
| 4 | 让 `filter_is_complete` 恒返回 `true`（或删掉它）以「避免空条件报错」 | 保留为**入口闸门**（不完整 → 拒绝），并保证替代保护成立：拒绝发生在建 SQL 之前 | 回到 `''` 强转到整型主键 → **整表加载失败**；这正是今天刻意静默丢弃要防的事 |
| 5 | 只改 `src/types/index.ts` 的算子联合，忘了 `src/types/settings.ts` 的重复定义 | 两处一起改，并按 §5 Step 6 **删掉** settings.ts 的重复定义（已核实无 importer） | 两份定义漂移：「筛选面板能选新算子、执行时报类型错」——契约 §1.6 点名的隐患 |
| 6 | 用 `assert!(sql.contains("..."))` 当「逐字节不变」的证据 | 既有 10 个算子用 `assert_eq!` **全串**断言（§9.1 的 golden 用例） | `contains` 会放过前缀/后缀/分隔符变化，等于没测；本册最重要的一条验收形同虚设 |
| 7 | 命名视图保存 `draftFilters` | 保存 `filters`（已应用）+ 已应用 `filterLogic`；dirty 时提示但仍保存 applied | 恢复出未生效甚至不完整的条件，直接触发新错误码，用户以为视图坏了 |
| 8 | 命名视图用 `dbSessionId` 做作用域键或存进视图记录 | 用 `connectionId`（持久化连接 id）；`dbSessionId` 永不落盘 | 重启后视图全部失效或串到另一个连接（`AGENTS.md` 的 ID 术语规范明令禁止） |
| 9 | 改 `src/locales/en.ts` 加词条 | 改 `src/locales/en/query.ts`（领域包），`en.ts` 只有一行再导出 | 文案完全不生效，且排查半天以为是 i18n 加载问题 |
| 10 | 把算子语义写错：`Between` 检测到上下界颠倒就自动交换；`IsEmpty` 写成 `col IS NULL OR col = ''` | `Between` 不交换、只提示「结果必为空」；`IsEmpty` 就是 `col = ''` 且文案写「空字符串」，`NULL` 交给 `IsNull` | 前者：用户打错的区间被静默「修正」，看到有结果却与输入不符（最难排查的一类 bug）；后者：库里的 `NULL` 与空串在 UI 上不可分辨，用户无法回答「这列到底是没填还是填了空」 |

---

## 11. 未决问题

| # | 问题 | 建议选项与理由 |
| --- | --- | --- |
| **Q-1** | **`incomplete` 政策是否变更**（本册建议变更：入口拒绝，不再静默丢弃） | **建议：变更**。理由：契约 §8.2 明文「不得静默丢弃后照常查询」，且静默少一个条件会让返回的数据集大于用户以为的范围（数据正确性问题）。备选：保持静默丢弃 + 仅 UI 标注「未生效」——改动面最小且不动 MCP 调用方语义，但契约错误码 `grid.filter.incomplete` 就没有触发点。若选备选，§4.5 的表格需整段改写为「UI 提示 + 不剥离」 |
| **Q-2** | 嵌套布尔分组 `(A AND B) OR C` 是否本册实现 | **建议：本册只预留、不实现**（§3.6 已给出 `FilterNode` 形状与接线点）。理由：PRD 把 C-33 列为 P2 并明确「待逐条连接符上线后用埋点判断需求」；契约的 `filter_join` 是全局单一逻辑、未授权改动；分组必须配套新 UI 与键盘导航，属独立可回滚提交。备选：本册实现 → 需先改 `filter_join` 契约（跨分册影响 DB-15 的 SQL 预览） |
| **Q-3** | **默认算子白名单：契约冻结为 19 个共享算子，是否要偏离成「驱动显式开启」**（此前 §5.2 的代码注释与分组表自相矛盾，现已冻结：`EXISTING_FILTER_OPERATORS` = 既有 10 个且**只是测试基线**，`SHARED_FILTER_OPERATORS` = 19 个且**就是 `supported_filter_operators()` 的默认返回值**，方言 3 个必须驱动显式声明） | **建议：不偏离，按冻结值落地**。理由：9 个共享扩展全部是 ANSI 可渲染的（`NOT IN` / `BETWEEN` / `LIKE … ESCAPE` / `= ''`），且 UI 还叠一层 `CellType` 白名单（§3.5），误用面很小。若人工裁定要改成「默认只给既有 10 个」，只需把默认返回值换成 `EXISTING_FILTER_OPERATORS`（一处），但**这是改契约**，必须回写 §5.2 而不是在分册里私自偏离。**并请确认**：本册的「既有 10 算子逐字节不变」回归测试（§9.1 `legacy_ten_operators_render_byte_identical` 与 `legacy_filter_full_select_sql_is_golden`）针对的是**宿主 `format_condition` 的渲染路径**，与驱动白名单无关——白名单只决定「UI 里出现哪些算子」，不改变已选中算子的 SQL 文本 |
| **Q-4** | 新增错误码前缀 `grid.view.nameExists` 是否可接受（契约 §8.2 的清单未列它） | **建议：接受并回写契约 §8.2**。理由：重名是命名视图必须处理的冲突，且契约 §8.1 的机制本来就允许「新增需前端识别的错误用 `grid.<域>.<原因>` 前缀」。备选：不入清单，宿主只返回 `CommandError::Validation` 普通消息 → 前端 `classifyGridError` 返回 `'unknown'` 并回退显示原始英文，体验差但零契约改动 |
| **Q-5** | 命名视图的持久化落点：契约的 IPC + Rust store，还是 PRD §7 的 `settingsStore.namedViews` | **建议：契约（Rust store）**。理由：契约是权威；且 `sync_profiles.json` / `settings.json` 的既有范式已经把「原子写 + 锁 + 加密目录」解决了，放前端 settings 要额外同步 `AppSettings` 形状并重走一遍落盘。PRD 的 `settingsStore` 说法视为被契约覆盖 |
| **Q-6** | PRD DB-08 §7 前半「按表记忆上次筛选/排序/可见列（`tableViewPrefs` + LRU）」是否随本册交付 | **建议：拆出本册**，作为 DB-08b 独立提交。理由：它改的是设置存储的形状与淘汰策略（容量上限、LRU），与命名视图共享 DTO 但不共享 UI；拆开可独立回滚。备选：一并交付 → 本册人日 +2~3，且要额外决定「记忆状态 vs 命名视图默认视图」的优先级冲突 |
| **Q-7** | `filter_operator_sql` 的 `literal` 是单值，无法表达 `In`/`NotIn`/`Between` 的多字面量 | **建议：本册按「仅单字面量算子询问驱动」落地**（Like 家族 + 3 个方言算子），多值算子恒由宿主渲染。若将来要驱动渲染多值算子，需扩展签名（新方法 `filter_operator_sql_many(op, column_expr, literals: &[String])`），并在提升 `PROTOCOL_VERSION` 时一并处理。备选：本册就把 `literal` 约定成「已拼接片段」（如 `"1 AND 5"`）——**否决**，语义含糊且驱动需要反解析 |
| **Q-8** | 把既有 `filter.like` 英文文案从 `'contains'` 改为 `'matches pattern'` 是否接受 | **建议：接受**。理由：本册新增了真正转义的 `contains`，两个相反语义的算子同名会让用户以为 `like` 也转义 `%`。这只动英文文案，不动 SQL（B1/B2 不受影响）；其他语言由发布前同步流程补齐。备选：保留 `'contains'` → 必须在 UI 里额外解释差异，成本更高且更容易误导 |
| **Q-9** | PRD DB-08 目标行为第 5 条要求 `Raw SQL` 自由条件，但契约 §5.2 的算子集不含它 | **建议：从本册拆出为独立分册**（DB-08-raw-sql）。理由：它是 WHERE **片段**而非算子，与算子模型正交；必须和 `sql_guard` 的注释剥离、危险语句扫描、拒绝 `;` 与注释逃逸一起设计（PRD §12 已把它列为注入面）；且它是安全敏感面，混在本册会让审查重点失焦。请契约负责人确认：`RawSql` 是新增算子变体还是 `FilterCondition` 之外的新可选字段 |
| **Q-10** | 类型归一化的权威来源 | **已由分册 05 落地解决**：权威是 `src/lib/cellTypes.ts` 的 `resolveCellType(...)` → `CellType`；`classifyDataType` 已被 05 改造为 `cellTypeToDataTypeFamily(resolveCellType(...))`，只用于渲染着色。本册按 `CellType` 分派算子（§3.5），不再自行判断类型。**剩余确认项**：08 与 05 必须消费**同一份** `DatabaseTypeMeta.cellTypes`（05 的 `CellTypeHints`），否则会出现「编辑器按 enum 处理、筛选器按 text 处理」的双重标准（§3.5 已声明纪律，请 05 作者确认字段名与暴露路径） |
| **Q-11** | 「筛选后行数」与 DB-09 `CountStrategy` 的归口 | **已由分册 09 落地解决**：09 的验收点 A7「有生效筛选时禁止用估算行数」+ 其 §7 的 `has_effective_filters`（复用 `filter_is_complete`）+ `Estimated → Exact` 降级闸。本册只声明纪律，不重复归口。**剩余确认项（与 Q-1 强耦合）**：09 把 `has_effective_filters == false` 定义为「`filters` 为空**或所有条件都不完整**」，这条建立在「不完整条件不产生 `WHERE`」即**今天的静默丢弃语义**上。若 Q-1 裁定改为**入口拒绝**，则该分支在 IPC 路径上不可达（服务层直连调用方仍可达）——09 的实现无需改动，但**不得依赖该分支得出「无生效筛选」的结论**；若 Q-1 裁定**保持静默丢弃**，则该分支是活路径，09 的现状定义即为正确。请 09 作者确认接收这个耦合 |
| **Q-12** | `src/lib/gridErrors.ts` 与 `src/lib/databaseMeta.ts` 的 `DataGridCapabilities` 由 08 还是 09 创建 | **建议：谁先合并谁建，后合并者只追加成员**（本册 §5 Step 8 已写明）。理由：两份都会写这两个文件，且它们是**单文件、单接口**，各建一份会在合并时被后提交者整文件覆盖，静默丢掉对方的前缀/字段。请协调者按合并顺序指派，不要两边都「新建」 |
| **Q-13** | `src/windows/connection/TableView.tsx` 的 800 行超限冲突的归属 | **建议：本册负责先抽 `TableFilterToolbar.tsx`**（筛选面板入口 + 命名视图菜单 + 快筛输入整体搬走）。理由：现状 770 行，09 约 +60、本册约 +40，**合计约 870 > 800**，两册各自的 §6 都误判为「不触及上限」。若由 09 先合并，则 09 需在合并前完成同等拆分，或本册落地时承担；**不能两册都不做**。本册的人日已含这 0.5 天 |
