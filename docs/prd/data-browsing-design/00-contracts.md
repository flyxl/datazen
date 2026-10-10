# 00 · 共享契约（所有分册的强制约束）

> **本册地位**：本目录下每一份分册都必须遵守本册定义的契约。分册里如需偏离，必须在分册的「未决问题」中列出并说明理由，不得自行改动契约后继续往下写。
>
> **目标读者**：不了解本项目历史、第一次接手数据浏览功能的开发者。本册假设你还没看过任何 DataZen 代码，因此第 1 节把「今天代码是什么样」逐条写清；看不懂业务背景时回读 [PRD](../data-browsing-optimization-prd.md)。
>
> **基线**：`main` = `e0f6ba28b`；本目录所有事实均按该提交核对。**本册及所有分册一律不写行号**（行号会随重构腐烂，且没有任何门禁检查它），引用位置统一用「文件路径 + 符号名」，例如 `QueryExecutor::build_select_sql`。

---

## 0. 术语与阅读约定

| 术语 | 含义 |
| --- | --- |
| **网格 / grid** | 数据浏览的表格组件族，入口是 `DataTable`，内部由 `TableHeader` + `VirtualBody` + `Pagination` 组成 |
| **表格态 / table state** | 一个标签页（panel）的表数据状态，落在 `TableState`；页、筛选、选择、暂存改动都属于它，不属于连接会话 |
| **暂存改动 / pending change** | 已经写在 UI 上、但还没提交到数据库的改动，落在 `TableState.pendingChanges` |
| **变更计划 / plan** | 提交前由后端生成的、带 `fingerprint` 的不可变计划，类型是 `RowChangePlan` |
| **行身份 / row identity** | 由原值快照构成的行主键标识（`RowIdentity`），是写操作的唯一定位依据；UI 行号只是临时锚点 |
| **单元坐标 / cell coord** | `(rowIndex, columnName)`。注意是**列名**不是列下标——本项目网格的行数据是位置数组，列下标会随 `visibleColumns` 变化而失去稳定含义 |
| **写意图 / CellWrite** | 本册第 3 节定义的三态写入意图：写 NULL、写具体值、不写（交给数据库默认值） |
| **能力声明 / capability** | 驱动通过元数据或 trait 方法声明「我支持什么」，宿主据此开合 UI 插槽，绝不在宿主里按驱动名硬编码 |

**两个必须分清的概念**（混用会导致严重缺陷）：

- `connectionId` = 持久化的连接配置 id，落盘；语义是「配置 / 归属」。
- `dbSessionId` = 运行时的数据库会话 id，只在内存，永不落盘；语义是「操作已建立的连接」。
  凡写操作、查询、缓存键，一律用 `dbSessionId`；凡保存配置、调度任务，一律用 `connectionId`。禁止双模回退。

---

## 1. 现状基线（已逐条核对，不是推测）

新手最容易犯的错是「以为某个能力已经有了」。这一节给出的是**今天真实存在**的东西，第 2 节给出的是**必须自己新增**的东西。

### 1.1 前端表格状态：`TableState`

位置：`src/stores/tableData/types.ts`。一个标签页一份，字段含义如下（这是设计分册要改动的核心状态对象）。

```ts
export interface CellEdit {
  rowIndex: number;
  columnName: string;
  originalValue: unknown;
  newValue: unknown;
  pkSnapshot: Record<string, unknown>;
}

export interface TableState {
  context: TableChangeContext | null;   // 写操作的路由信息（连接/会话/库/模式/表）
  columns: ColumnSchema[];
  rows: Record<string, unknown>[];      // 注意：store 里是「按列名索引的记录」，不是位置数组
  totalRows: number;
  page: number;
  pageSize: number;
  filters: FilterCondition[];           // 已应用的筛选
  filterLogic: 'and' | 'or';            // 单一全局逻辑连接词
  draftFilters: FilterCondition[];      // 编辑中、未应用的筛选
  draftFilterLogic: 'and' | 'or';
  filterPanelOpen: boolean;
  sorts: SortCondition[];               // 现状：只会有 0 或 1 个元素
  editBuffer: Map<string, CellEdit>;    // 逐格编辑缓冲（键由单元坐标拼成）
  pendingChanges: Map<string, PendingRowChange>;  // 暂存改动，键是稳定的行身份
  rowIdentityAnchors: Map<number, string>;        // 行号 → 行身份键；行号只是临时锚点
  previewPlan: RowChangePlan | null;
  pendingStatus: PendingStatus;         // 'idle' | 'previewing' | 'committing'
  selectedRows: Set<number>;            // 只支持整行选择
  lastSelectedIndex: number | null;     // 整行选择的锚点（Shift 连选用）
  editingCell: { row: number; col: string } | null;
  detailRowIndex: number | null;
  loading: boolean;
  requestRevision: number;              // 单调递增的请求版本号
  loadingRevision: number | null;       // 在途请求代表的版本号
  error: string | null;
  visibleColumns: string[] | null;      // 列显隐（null = 全部可见）
}
```

**由此可推出的关键结论（分册必须接受）**：

1. **没有单元格级选择模型**。只有 `selectedRows: Set<number>` 和 `lastSelectedIndex`，粒度是整行。区域选择是全新能力。
2. **`requestRevision` / `loadingRevision` 是既有的竞态防护机制**。任何新增的异步取数（如行数估计、外键候选、键集翻页）都必须复用同一套版本号判定，禁止绕开它直接写 `rows`。
3. **`sorts` 是数组，但写入端只放一个元素**。多列排序是「数据通路已就绪、只差 UI 与写入端」的状态。
4. **`rowIdentityAnchors` 已经建立了「行号 → 行身份」的映射纪律**。任何写路径都必须先解析出行身份，禁止直接用行号拼 `WHERE`。

### 1.2 前端表格组件与事件契约

位置：`src/components/DataTable/DataTable.tsx`（`DataTableProps`）、`VirtualBody.tsx`、`TableHeader.tsx`、`Pagination.tsx`。

已存在的 props（挑与本次相关者）：

```ts
export interface DataTableProps {
  columns: ColumnDef[];
  rows: unknown[][];                    // 组件层是「位置数组」，与 store 的记录形态不同，父级负责转换
  totalRows?: number;
  page?: number;
  pageSize?: number;
  sorts?: SortCondition[];
  onSort?: (sort: SortCondition) => void;
  filters?: FilterCondition[];
  filterLogic?: 'and' | 'or';
  editingCell?: { row: number; col: string } | null;
  editBuffer?: Map<string, CellEdit>;   // ⚠ 已声明但组件内部从未解构使用：等价于死 prop
  onCellDoubleClick?: (row: number, col: string) => void;
  onCellEdit?: (row: number, col: string, value: unknown) => void;
  onCellEditCancel?: () => void;
  enableSetNull?: boolean;
  selectedRows?: Set<number>;           // 只有整行选择
  onRowSelect?: (index: number, opts?: { multi?: boolean; range?: boolean }) => void;
  onSelectAll?: () => void;
  onRowClick?: (index: number) => void;
}
```

**DOM 数据属性约定（必须沿用，禁止另立一套）**：

| 属性 | 挂在哪 | 用途 |
| --- | --- | --- |
| `data-dt-row` | `VirtualBody` 的单元格 | 行下标 |
| `data-dt-col` | `VirtualBody` 的单元格 | **列名** |
| `data-col-header` | `TableHeader` 的单元格 | 列名 |

解析函数已存在：`resolveDataTableCellFromEvent`、`resolveDataTableHeaderColFromEvent`（位于 `src/lib/dataTableContextMenu.ts`）。它们用 `target.closest('[data-dt-row][data-dt-col]')` 反查，**这是本项目确认过的正确做法**：禁止用视口几何坐标反查单元格。

**右键菜单**：统一走 Web Context Menu（`showNativeContextMenu` + `buildDataTableContextMenuItems`），**禁止** Tauri 原生 `Menu.popup()`。

**测试 id**：`tid(id)` 来自 `@datazen/ui`（本项目在 `src/lib/tid.ts` 再导出），返回 `{ 'data-testid': id }` 或 `{}`（仅 E2E 构建注入）。新 UI 一律用 `tid(...)` 而不是手写 `data-testid`。

### 1.3 快捷键与键盘事件的现状

位置：`src/hooks/useKeyboardShortcuts.ts`。

```ts
export interface ShortcutDef {
  key: string;                 // 形如 'mod+c' / 'shift+enter' / 'delete' / 'escape' / 'space'
  scope: 'global' | 'editor' | 'table';
  action: () => void;
  description: string;
}
export function useKeyboardShortcuts(shortcuts: ShortcutDef[]): void;
```

**你必须知道的匹配语义**（读实现得出，写分册时必须考虑）：

1. 修饰键是**精确匹配**：`mod` / `shift` / `alt` 的期望值与实际值必须完全相等。因此 `mod+c` 不会在按下 `mod+shift+c` 时触发。
2. `key` 用 `e.key.toLowerCase()` 比较；`'escape'` / `'delete'` / `'enter'` / `'space'` 有专门分支，其中 `'delete'` 同时匹配 `Delete` 与 `Backspace`，`'space'` 匹配 `' '`。
3. `'table'` 作用域的快捷键在焦点位于 `input` / `textarea` / `contentEditable` 时**会被跳过**；`'global'` 作用域不受此限制。这一条决定了「编辑器打开时按 Delete 不应该删行」之类的行为。
4. 命中即 `preventDefault()` 并 `return`，**只执行第一个命中的快捷键**——因此注册顺序即优先级，重复定义同一组合会导致后面的永远不生效。

**现状**：网格自身的键盘处理只有一处，即删除键处理，没有方向键导航、没有 `Ctrl/Cmd+C/V`、没有 `Enter` 进入编辑、没有 `Tab` 移动。

### 1.4 提交链路（Rust 侧）

位置：`src-tauri/src/commands/data.rs`。链路是：

```
preview_pending_changes          // tauri command：生成计划
  └─ build_row_change_plan       // 组装 RowChangePlan，计算 fingerprint
       └─ changes_fingerprint    // 指纹（sha2）
commit_pending_changes           // tauri command：提交
  └─ commit_pending_changes_impl // pub(crate)：加锁、校验指纹、取连接
       └─ execute_row_change_plan_impl  // 真正逐条执行
（遗留入口 `commit_row_updates` / `commit_row_deletes` 按 C-1 已裁定删除，不再是本设计的链路一环）
```

相关 DTO（**都是已存在的，新增字段要遵守第 2 节铁律**）：

```rust
pub struct CellUpdate { pub column: String, pub value: Option<Value> }
pub struct RowUpdateBatch { pub set_columns: Vec<CellUpdate>, pub pk_columns: Vec<CellUpdate> }
pub struct RowDeleteBatch { pub pk_columns: Vec<CellUpdate> }

pub struct RowChangeTableContext {
    pub connection_id: String, pub db_session_id: String, pub driver_type: String,
    pub database: Option<String>, pub schema: Option<String>, pub table: String,
}
pub struct PendingRowChange {
    pub row_identity: BTreeMap<String, Option<Value>>,
    pub original_values: BTreeMap<String, Option<Value>>,
    pub current_values: BTreeMap<String, Option<Value>>,
    pub changed_columns: Vec<String>,
    pub delete_marked: bool,
}
pub struct PlannedStatement {
    pub row_identity: BTreeMap<String, Option<Value>>,
    pub original_values: BTreeMap<String, Option<Value>>,
    pub current_values: BTreeMap<String, Option<Value>>,
    pub changed_columns: Vec<String>,
    pub sql_template: String,          // 驱动渲染的预览，非线级审计记录
    pub parameter_summary: Vec<String>,
}
pub struct RowChangePlan {
    pub plan_id: String, pub fingerprint: String, pub table: RowChangeTableContext,
    pub updates: Vec<PlannedStatement>, pub deletes: Vec<PlannedStatement>,
    pub warnings: Vec<ChangeWarning>,
}
pub struct CommitPendingChangesRequest { pub db_session_id: String, pub plan: RowChangePlan, pub fingerprint: String }
pub struct RowCommitStatementResult { pub operation: String, pub row_identity: BTreeMap<String, Option<Value>>, pub affected_rows: u64 }
pub struct CommitPendingChangesResponse { pub plan_id: String, pub fingerprint: String, pub statements: Vec<RowCommitStatementResult>, pub affected_rows: u64 }
```

**五个必须记住的事实**：

1. **`RowChangePlan` 只有 `updates` 与 `deletes` 两个语句集合，没有 `inserts`**。新增行是空白能力，不是「开关没打开」。
2. **`execute_row_change_plan_impl` 对每条语句要求影响行数恰好为 1**，否则整批失败，错误是 `CommandError::Validation`，文案形如 `UPDATE for one row identity affected {n} rows; refusing ambiguous write`（DELETE 同构）。这条纪律本身是安全设计（`affected == 1` 意味着「恰好命中我定位的那一行」），但它与「插入」语义天然冲突：插入的影响行数是 1，却没有 `row_identity` 可校验。
3. **⚠ 插入计划会被静默吞掉（DB-04 必须处理的陷阱）**：该函数的第一个判断是「`plan.updates` 与 `plan.deletes` **都为空**时直接返回成功、`affected_rows` 为 0」。因此 DB-04 在给计划加入 `inserts` 之后，**必须同步把这条提前返回的判断扩展到 `inserts`**；漏改的后果是「用户点了提交、界面显示成功、数据库里什么都没有」，而且不报错。
4. **事务归属有两条路径，插入必须走同一条**：若该 `dbSessionId` 上**已有用户开启的事务**（`AppState.session_transactions` 命中），本函数**不**自己开启或提交事务，改动并入用户事务、由用户决定提交或回滚；否则它自行 `begin_transaction`（驱动不支持时退化为裸 `BEGIN`），全部语句成功后 `COMMIT`，任一条失败则 `ROLLBACK` 后返回错误。**新增的插入语句必须放在同一个执行块内**，否则插入会落在另一个事务里，破坏「一次提交要么全成、要么全不成」的语义。
5. **`fingerprint` 是防「计划开裂」的**：预览与提交之间若暂存改动变了，指纹不匹配必须拒绝提交，禁止降级为「按新数据重算」。

### 1.5 SQL 生成（Host 侧）

位置：`src-tauri/src/services/query_executor.rs`。

```rust
pub struct SortCondition { pub column: String, #[serde(default)] pub descending: bool }
pub struct OrderBy { pub column: String, pub descending: bool }

impl QueryExecutor {
    pub async fn get_table_data(
        &self, driver: &Arc<dyn DatabaseDriver>, handle: &ConnectionHandle,
        db_session_id: &str, database: &str, schema: Option<&str>, table: &str,
        page: u32, page_size: u32, filters: Option<Vec<FilterCondition>>,
        order_by: Option<OrderBy>, skip_count: bool,
    ) -> ...;

    fn build_count_sql(table_name, columns, filters, qi, format_lit, filter_logic) -> String;
    fn build_select_sql(table_name, columns, filters, order_by, qi, format_lit, pagination, filter_logic) -> String;
    fn format_condition(condition, qi, format_lit) -> String;
    fn filter_is_complete(condition) -> bool;
}
fn filter_join(logic: Option<&str>) -> &'static str;
```

**现状行为（分册设计的出发点，务必逐条读）**：

1. `build_select_sql` 在**没有显式排序**时，会注入 `ORDER BY <主键列...> ASC`；没有主键时退化为**第一列**。这就是「无主键表打开即慢」和「按第一列排序看起来像乱序」的根因（对应 PRD 的 D-9 / DB-09 / DB-18）。
2. `format_condition` 只处理 **10 个算子**，且 `IsNull` / `IsNotNull` 之外都要字面量。
3. `filter_is_complete` 会**静默丢弃**不完整筛选（刚添加还没填值的条件不进 SQL）。这是刻意的：否则驱动会把 `''` 强转到整型主键上，导致整表加载失败。
4. `filter_join` 只有 `AND` / `OR` 两态，且是**全局单一逻辑**——没有 `(A AND B) OR C` 这种嵌套分组能力。
5. `build_count_sql` 忽略传入的 `columns`（方法体第一行是丢弃），只发 `SELECT COUNT(*) FROM t WHERE ...`。
6. 分页子句来自驱动：`PaginationSyntax { clause, requires_order_by, order_by_fallback }`，`clause` 形如 `LIMIT 25 OFFSET 50` 或 `OFFSET 50 ROWS FETCH NEXT 25 ROWS ONLY`；`requires_order_by` 为真且当前语句没有 `ORDER BY` 时，才用 `order_by_fallback` 补一个中性排序。
7. **多列排序在 IPC 边界就被截断**：`get_table_data_impl` 对 `sorts` 只取第一个元素（`list.into_iter().next()`），后面的排序条件被丢弃。所以「只支持单列排序」不只在前端 store 里，后端入口同样如此——分册要实现多列排序，**必须同时改这两处**。
8. **行数查询与数据查询是并发的**：`get_table_data` 用 `tokio::try_join!` 同时发出 count 与 select；不存在「先数行数再取数据」的先后关系。`total_rows` 从 count 结果第一个单元格按 `Value::Integer` 解析，**非整数形态（字符串、`NULL`、大整数越界）会得到 `None`**——这正是「三态」要表达的现实。任何改成「估计行数」或「不数行数」的方案都要重新说明这条并发结构的取舍。
9. **列与主键的来源**：两者都来自 `SchemaCache::get_columns`，返回 `CachedColumns { columns: Vec<ColumnSchema>, primary_keys: Vec<String>, table_name: String }`，其 `primary_keys` 在缓存内部已经过 `TableSchema::effective_primary_keys()` 归一化。但 **`build_select_sql` 目前只读 `cached.columns`，完全没读 `cached.primary_keys`**，它是靠遍历 `columns` 上的 `is_primary_key` 标志来挑主键的。这构成一个既有隐患：驱动若填了 `primary_keys` 却没在列上打 `is_primary_key` 标志，今天的默认排序会**找不到主键而退化成按第一列排序**。分册 09 改成读 `cached.primary_keys` 是对这个隐患的修正，但它**是一次行为变更**，必须显式声明并配测试（见第 5.4 节）。

### 1.6 驱动契约（`driver-api`）现状

位置：`packages/driver-api/`。

**`DatabaseDriver` trait 里与数据读写相关的既有方法**（`packages/driver-api/src/traits.rs`）：

```rust
fn quote_char(&self) -> char;                             // 默认 '"'
fn quote_ident(&self, name: &str) -> String;
fn skip_count_query(&self) -> bool;                       // 默认 false
fn supports_offset(&self) -> bool;                        // 默认 true
fn pagination_syntax(&self, limit: u64, offset: u64) -> PaginationSyntax;
fn format_sql_literal(&self, value: &Option<Value>) -> String;
fn build_update_sql(&self, table: &str, set_columns: &[(&str, Option<Value>)], pk_columns: &[(&str, Option<Value>)]) -> String;
fn build_delete_sql(&self, table: &str, pk_columns: &[(&str, Option<Value>)]) -> String;
fn has_schema_level(&self) -> bool;                       // 默认 false
fn has_multi_database(&self) -> bool;                     // 默认 false
fn default_schema(&self) -> Option<&'static str>;         // 默认 None
```

> **`build_insert_sql` 不存在。** 插入语句目前没有任何驱动侧构造入口，DB-04 必须新增（见第 5 节）。

**数据模型**（`packages/driver-api/src/types.rs`）：

```rust
pub enum Value { Null, Bool(bool), Integer(i64), Float(f64), String(String), Bytes(Vec<u8>), Timestamp(String), Json(serde_json::Value) }

pub struct ColumnSchema {
    pub name: String, pub data_type: String, pub nullable: bool,
    pub default_value: Option<String>, pub comment: Option<String>,
    pub is_primary_key: bool, pub is_auto_increment: bool,
}
pub struct TableSchema {
    pub table_name: String, pub columns: Vec<ColumnSchema>, pub primary_keys: Vec<String>,
    pub indexes: Vec<IndexInfo>, pub foreign_keys: Vec<ForeignKeyInfo>,
    pub check_constraints: Vec<CheckConstraint>, pub table_options: TableOptions,
}
impl TableSchema { pub fn effective_primary_keys(&self) -> Vec<String>; }  // 优先 primary_keys 字段，回退到列上的 is_primary_key

pub struct ForeignKeyInfo {
    pub name: String, pub columns: Vec<String>,
    pub referenced_table: String, pub referenced_columns: Vec<String>,
    pub on_update: String, pub on_delete: String,
    pub deferrability: ForeignKeyDeferrability,
}
pub struct TableDataResult {
    pub columns: Vec<ColumnSchema>, pub rows: Vec<Vec<Option<Value>>>,
    pub total_rows: Option<i64>, pub page: u32, pub page_size: u32,
}
pub struct PaginationSyntax { pub clause: String, pub requires_order_by: bool, pub order_by_fallback: Option<&'static str> }
```

`TableSchema::effective_primary_keys()` 是既有便利方法，凡是需要主键列的地方都应复用它，**不要**自己过滤 `is_primary_key`（双重来源会漂移）。

**筛选 DTO**（`src/filters.rs`，注意不在 `types.rs`）：

```rust
pub struct FilterCondition { pub column: String, pub operator: FilterOperator, #[serde(default)] pub value: Value }
pub enum FilterOperator { Eq, Ne, Gt, Lt, Gte, Lte, Like, In, IsNull, IsNotNull }   // 恰好 10 个
```

宿主在 `query_executor.rs` 里以 `pub use datazen_driver_api::filters::{FilterCondition, FilterOperator};` 再导出，所以宿主与驱动看到的是**同一个类型**。

**前端镜像（两份，都要改）**：`src/types/index.ts` 与 `src/types/settings.ts` 各自定义了同一套小写字符串算子：

```ts
export type FilterOperator = 'eq' | 'ne' | 'gt' | 'lt' | 'gte' | 'lte' | 'like' | 'in' | 'isNull' | 'isNotNull';
export interface FilterCondition { column: string; operator: FilterOperator; value?: Value }
export type Value = string | number | boolean | null | Record<string, unknown> | unknown[];
```

> 两份定义是历史遗留的重复。改算子集合时**必须同时改这两处并保持一致**，否则会出现「筛选面板能选、执行时报类型错」的漂移。分册 08 负责收敛这件事。

### 1.7 协议版本与能力协商

位置：`packages/driver-api/src/lib.rs` 与 `capabilities.rs`。

```rust
pub const PROTOCOL_VERSION: u32 = 4;
pub const MIN_PROTOCOL_VERSION: u32 = 1;
```

`capabilities.rs` 的 `classify_protocol_version` / `check_protocol_compatibility` 的判定语义是：

- 驱动声明版本 **低于 `MIN_PROTOCOL_VERSION`** → 必须**拒绝**，绝不降级运行。
- 驱动声明版本 **高于 `PROTOCOL_VERSION`** → 宿主不认识，同样拒绝。
- 在 `[MIN, PROTOCOL_VERSION]` 区间内 → 接受。

**结论（第 5 节展开）**：按第 2 节铁律新增「带默认实现」的 trait 方法，**不需要**提升 `PROTOCOL_VERSION`；一旦你修改了任何**既有**方法的签名或既有 `Value` / DTO 的序列化形态，就必须提升版本号并同步所有 git 驱动的 `ref` 钉定。

### 1.8 前端能力声明的既有范式

位置：`src/lib/databaseMeta.ts`。`DatabaseTypeMeta` 里的 `KvWorkspaceCapabilities` 是本项目**已确立的向后兼容范式**，本次所有新能力声明都应照抄它的写法：

- 每个字段**可选**，缺省 `false`；
- 字段是**能力而非驱动 id**（宿主只判断能力，不判断「这是不是 Redis」）；
- 声明写在**驱动自己的元数据里**（绝大多数驱动是 `packages/drivers/<id>/ui/meta.ts`；redis 因结构不同放在 `packages/drivers/redis/ui/shared/meta.ts`），**绝不写在宿主**；
- 未声明该字段的驱动行为**逐位保持与今天一致**。

---

## 2. 兼容性铁律（违反即返工）

### R1 · 只新增，不改签名

不得修改 `build_update_sql` / `build_delete_sql` / `format_sql_literal` / `pagination_syntax` / `get_table_data` 等既有方法的签名，也不得改既有 DTO 字段的类型或名字。需要新语义时**新增**方法或字段。

### R2 · 新增 trait 方法必须有默认实现

所有 `DatabaseDriver` 新增方法都必须给出默认实现。这样任何驱动（含本项目 50+ 个 path 驱动与 git 驱动）不改一行代码仍然编译通过。

默认实现的**约束**是**单向的**：允许**扩大**能力面，不得**移除或改变**今天已有的能力。

- **允许**：默认实现可以返回比今天更多的结果（例如 `supported_filter_operators` 默认返回 19 个算子，而今天只有 10 个）——宿主侧的渲染器能处理全部 19 个，扩大能力面对旧驱动只会更宽松。
- **禁止**：默认实现不得让今天能用的路径变得不可用、不得改变今天既有方法的返回值语义、不得要求驱动必须实现才正确。

⚠ 本条**不是**「默认行为逐字等于今天」。凡默认实现刻意扩大能力面的，必须在方法注释里写明「今天的返回值是什么、为什么默认改成这样」，以便驱动作者知道覆盖点在哪。参见 §5.2 的 `supported_filter_operators` 与 §5.4 的 `default_order_columns`。

### R3 · 前端与 Rust 的 JSON 契约靠 serde 自动映射

Rust 侧结构体已标 `#[serde(rename_all = "camelCase")]`，前端用 camelCase；**不要**手写转换函数，也不要给前端预留 snake_case 别名。新增 DTO 必须带上同名属性宏。

### R4 · 默认值必须等于今天的语义

- 新增能力字段的缺省值必须等于**今天该能力缺失时的实际行为**，且在字段注释里写明这个「今天的行为」是什么。**不是无条件填 `false`**：
  - 今天默认**不可用**的能力（如 `insertReturning`）填 `false`；
  - 今天默认**可用**的能力填 `true`（如 `DataGridCapabilities.editable`——今天所有驱动都没声明过该字段，前端一律按可编辑渲染，缺省必须是 `true`；填 `false` 会让所有未声明 `dataGrid` 的驱动一夜之间变只读，这正是本条要防的回归）；
  - 枚举类（如 `countStrategy`）缺省填**今天走的那条分支**，不是「最严格的那个」。
- 新增枚举分支必须在旧数据上反序列化成功（能用 `#[serde(default)]` 就用）；
- 新增 UI 交互在能力缺失时应**隐藏入口**，而不是渲染一个点了报错的按钮。

### R5 · 生产路径禁止裸 `unwrap()` / `expect()`

见 `docs/development/panic-policy.md`。Rust 新代码用 `?` + `CommandError` / `DriverError`；确需 panic 必须写注释说明为什么不可能失败。

---

## 3. `CellWrite` 三态契约（本次架构改动的核心）

### 3.1 为什么必须引入

今天的写路径用 `Option<Value>` 表达「要不要写这一列」：

```rust
pub struct CellUpdate { pub column: String, pub value: Option<Value> }
```

`Option<Value>` 只有两态，而 SQL 写入真实需要**三态**，因此今天存在无法表达的语义空洞：

| 用户意图 | SQL 语义 | 今天能否表达 |
| --- | --- | --- |
| 把这一列写成 NULL | `SET c = NULL` | ✅ `None` 或 `Some(Value::Null)`（两者等价，本身也是歧义） |
| 把这一列写成某个值 | `SET c = 'x'` | ✅ `Some(Value::String("x"))` |
| **不动这一列**（UPDATE）/ **让数据库填默认值**（INSERT） | 该列不出现在语句里 | ❌ **无法表达** |

后果（都是今天可复现的）：

1. **INSERT 无法省略列**：想插一行、让自增主键与带 `DEFAULT` 的列自己填值，做不到——只能被迫给所有列显式赋值，于是自增列、生成列、`DEFAULT now()` 全部失效或报错。
2. **`None` 与 `Some(Value::Null)` 语义重合**：读代码的人无法判断「没写」还是「写 NULL」，`changed_columns` 与实际写入列在边界情况下可能不一致。
3. **UPDATE 的空值歧义**：把一列从有值改为 NULL 与「这一列没被改动」在数据结构上只差一个 `Option` 包装，任何一处丢包都会静默变成「不改这一列」。

### 3.2 Rust 定义（新增，放在 `packages/driver-api/src/types.rs`）

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

**线上形态**（前端与 Rust 之间、宿主与驱动之间相同）：

```json
{ "kind": "null" }
{ "kind": "value", "value": 42 }
{ "kind": "unset" }
```

> `tag = "kind"` + `content = "value"` 生成的就是上面这种 `{ kind, value }` 形状。**不要**改用 `untagged`：`Value` 本身已经是 untagged 枚举，两层 untagged 会让 `{"value": null}` 这类输入无法判定，反序列化会静默选错分支。

### 3.3 前端镜像定义（新增，放在 `src/lib/tableChanges.ts`）

```ts
export type CellWrite =
  | { kind: 'null' }
  | { kind: 'value'; value: Value }
  | { kind: 'unset' };

/** 便捷构造器，避免各处手写字面量（也方便以后加字段）。 */
export const cellWriteNull = (): CellWrite => ({ kind: 'null' });
export const cellWriteValue = (value: Value): CellWrite => ({ kind: 'value', value });
export const cellWriteUnset = (): CellWrite => ({ kind: 'unset' });
```

### 3.4 从 `CellWrite` 派生 `changed_columns` 的规则

`changed_columns` 是「本次语句要写的列」的权威列表，**必须**由写意图派生，而不是反过来：

| `CellWrite` | 是否进入语句列清单 | 是否计入 `changed_columns` | 是否算「有改动」 |
| --- | --- | --- | --- |
| `Unset` | 否 | **否** | 否 |
| `Null` | 是 | 是 | 是（且当前值与 `null` 不等时才真正是改动） |
| `Value(v)` | 是 | 是 | 是（且 `v` 与原值不相等时才真正是改动） |

判定「是否真的改动」一律复用既有的 `valuesEqual`（`src/lib/tableChanges.ts`）与后端的等价比较逻辑，**禁止**用 `===` 或 `JSON.stringify` 另写一份（`valuesEqual` 内部就是稳定的 JSON 序列化比较，专为 `NaN` / 对象键序这类情况设计）。

### 3.5 各驱动默认实现的语义要求

新增的 `build_insert_sql`（第 5 节）默认实现必须遵守：

1. **只列 `Null` 与 `Value` 的列**，`Unset` 的列完全不出现在列清单与 `VALUES` 中。
2. **一列都没有时**（全部 `Unset`）默认产出 `INSERT INTO t DEFAULT VALUES`。
   > 这是**不通用**的写法，因此这是「默认实现」而非「唯一实现」：MySQL 不支持 `DEFAULT VALUES`，须由 MySQL 驱动覆盖为 `INSERT INTO t () VALUES ()`。这类方言差异**属于驱动**，不得在宿主里按驱动名分支。
3. **不得**为了「凑齐列」而给 `Unset` 列补 `NULL` —— 那正是今天自增列失效的原因。
4. 自增 / 生成列在 INSERT 时**必须**是 `Unset`，除非用户显式填了值。

### 3.6 校验规则（宿主必须拒绝的组合）

| 组合 | 期望行为 |
| --- | --- |
| `Value(v)`，而该列是生成列（`is_auto_increment` 或有 `default_value` 的只读列）且用户未显式改 | 应保持 `Unset`；若确实显式改了则允许，交由数据库报错 |
| 一行的所有列都是 `Unset` 且 `delete_marked == false` | 该行无实际改动，**不得**进入计划（空语句会污染影响行数校验） |
| `delete_marked == true` 同时又给出非 `Unset` 列写入 | 拒绝，报 `grid.commit.conflictingIntents` |
| 主键列出现在 `Unset` 中，而该行是 UPDATE | 合法：主键列本来就不该改；行定位走 `row_identity`，与写入列无关 |

---

## 4. 单元格坐标系与选择模型契约

所有涉及「选中哪些格」的分册（01/02/03/05/06）必须使用本节定义的模型，不得各自造一套。

### 4.1 坐标与区域

```ts
// 位置：src/stores/tableData/gridSelection.ts（新增）
export interface CellCoord {
  /** 当前页内的行下标（0 基）。仅用于定位渲染，禁止用于拼 SQL。 */
  rowIndex: number;
  /** 列名，而不是列下标：列下标会随 visibleColumns / 列拖拽失去稳定含义。 */
  columnName: string;
}

export interface CellRange {
  /** 区域的两个角，顺序无意义（由工具函数规范化）。 */
  start: CellCoord;
  end: CellCoord;
}

export interface GridSelection {
  /** 区域选择的锚点（按下鼠标 / 开始 Shift 连选时确定）。 */
  anchor: CellCoord | null;
  /** 区域的另一端，随鼠标拖拽或 Shift+方向键移动。 */
  focus: CellCoord | null;
  /** 额外的非连续区域（Cmd/Ctrl+点击产生）。 */
  extraRanges: CellRange[];
  /** 该选择的交互模式，决定渲染与后续操作语义。 */
  mode: 'none' | 'cell' | 'row';
}
```

**代码落点（两个文件，缺一不可，后写的分册不要另建一份）**：

| 文件 | 职责 | 是否依赖 React |
| --- | --- | --- |
| `src/stores/tableData/gridSelection.ts`（新增） | **纯逻辑**：类型定义、`normalizeCellRange`、区域求交/合并、行集推导、键的构造 | 否（可独立单测） |
| `src/hooks/useGridSelection.ts`（新增） | **React 接线**：把纯逻辑接到 `TableState`、订阅鼠标与键盘事件、产出记忆化选择器 | 是 |

拆成两个文件是刻意的：纯逻辑可以脱离组件单测（本项目要求交互逻辑有完整状态机测试），React 层只做接线。02/03/05/06/07/10 都必须复用这两处，**不得**各自再实现一遍区域运算。

**规范化规则（必须有单元测试）**：任何消费 `CellRange` 的代码都必须先经过 `normalizeCellRange(range)`，它保证 `start` 的行下标 ≤ `end` 的行下标、且列在 `visibleColumns` 顺序上不晚于 `end`。区域比较用的是**列序**而不是列名字典序（`id` < `name` 是字典序，但显示顺序可能相反，两者混用会导致区域漏格或多格）。

### 4.2 三种选择模式的关系（互斥且可跃迁）

| 模式 | 进入条件 | 状态内行为 | 退出跃迁 |
| --- | --- | --- | --- |
| `none` | 初始；按 `Escape`；点击网格空白处 | 无高亮 | 单击单元格 → `cell`；点击行号槽 → `row` |
| `cell` | 单击单元格；方向键 | `anchor`/`focus` 跟踪；Shift+方向键扩区；Cmd/Ctrl+点击加非连续区 | `Escape` → `none`；点行号槽 → `row` |
| `row` | 点击行号槽；点击已选中行号槽 | 整行高亮，继续沿用既有 `selectedRows` / `lastSelectedIndex` | `Escape` → `none`；点单元格 → `cell` |

**必须遵守的两条**：

1. `row` 模式**继续使用既有的 `selectedRows: Set<number>` 与 `lastSelectedIndex`**，不要新建一套整行选择状态——否则「全选 / 反选 / Shift 连选」会出现两套真相。
2. `cell` 模式下 `selectedRows` 必须**清空**，避免出现「单元格区域高亮 + 整行也高亮」的视觉歧义与语义冲突。
3. **行级操作的取值域用派生选择器，不要把派生结果写回 `selectedRows`。** 「删除行 / 导出 / 批量复制」这类行级操作需要知道「作用于哪些行」，而 `cell` 模式下 `selectedRows` 是空的。正确做法是定义一个**只读派生选择器** `rowsCoveredBySelection(state): ReadonlySet<number>`：
   - `mode === 'row'` → 返回 `selectedRows`；
   - `mode === 'cell'` → 返回区域（含 `extraRanges`）覆盖到的行下标集合，**派生得到，不落库**；
   - `mode === 'none'` → 空集。

   **禁止**把派生行集写回 `selectedRows`。写了就等于同时存在「区域」和「整行选中」两套真相：用户接着 Shift 连选、或翻页后重算，两者必然打架。这条与 PRD 里「`selectedRows` 一律由区域派生」的早期草稿**方向相反**，以本节为准（理由见上）。

   行级操作在 `mode === 'cell'` 且用户未显式选择整行时，其作用域**必须**在 UI 上说清（例如「将删除区域覆盖的 12 行」），不能让用户以为只删了当前行。

   #### 4.2.1 `rowsCoveredBySelection` 的冻结签名（**唯一合法形状**）

   ```ts
   // 落在 src/stores/tableData/gridSelection.ts（01 创建），纯函数
   export function rowsCoveredBySelection(state: GridSelection): ReadonlySet<number>;
   ```

   **本节是唯一权威形状，已冻结。** 各分册此前出现过三种互斥写法，全部以本节为准：

   | 曾出现的写法 | 出现位置 | 裁定 |
   | --- | --- | --- |
   | `(state) => ReadonlySet<number>` | 本契约、03、06 | ✅ **唯一合法** |
   | `(selection, columnOrder) => number[]` | 01、05 | ❌ **作废** |
   | `rowsCoveredBySelection(...).size` | 03 | ✅ 仅当返回 `ReadonlySet` 才成立 |

   **两条禁止项，违反其一即 `pnpm typecheck` 失败或语义反了**：

   1. **不得加第二参数。** `columnOrder` 是**列**的顺序，而本函数返回**行**下标集合——区域求覆盖行集只需要行坐标与 `extraRanges`，列顺序对它没有影响。加一个用不上的参数会诱使实现去读列序，从而把「列」和「行」两种下标混进同一个 `number[]`/`Set<number>`，是静默数据错误的入口。
   2. **不得返回 `number[]`。** 返回值必须是 `ReadonlySet<number>`，理由有三：调用方需要的是**成员判定**（`has(i)`）而不是**有序列表**；`ReadonlySet` 在类型层面就禁止调用方误写回去；并且 03 的既有断言写作 `rowsCoveredBySelection(state).size > 0`——`number[]` 没有 `.size`，`pnpm typecheck`（含测试文件）会当场失败。

   **判定口径**：`mode === 'row'` 时必须**直接返回 `selectedRows` 本身**（不拷贝），这样 02 的 `Cmd+A` 路径零派生、零歧义；`mode === 'cell'` 时用 `selectionRanges(anchor, focus, extraRanges)` 逐个区间 `[Math.min, Math.max]` 取并集去重后构造新 `Set`。

   **本函数是纯读取**：`pnpm typecheck` 覆盖测试文件，因此**任何分册若写了 `.add(...)` / `.delete(...)` / 遍历中改写，都必须改成重新调用本函数**；禁止把返回的 `Set` 转成数组后传给 `buildRowsMatrix`，除非那个函数签名本身就是 `Iterable<number>`（03 的 `rowIndices` 参数按此执行）。

### 4.3 与行身份的边界（最重要的一条）

**选择状态只存在于 UI 层，永远不参与写操作定位。** 任何写路径的转换链条固定为：

```
CellCoord.rowIndex
  → TableState.rowIdentityAnchors.get(rowIndex)   // 得到行身份键
  → TableState.pendingChanges.get(identityKey)    // 得到 PendingRowChange
  → PendingRowChange.rowIdentity                  // 唯一可用于 WHERE 的原值快照
```

禁止出现 `WHERE ... = <rowIndex>` 形式的任何代码路径。翻页、筛选、排序变化后 `rowIndex` 的含义会整体平移，`rowIdentityAnchors` 是唯一能把「用户当时看到的那一行」映射回数据库行的桥。这条纪律对应 PRD 的 R5 根因。

**推论（必须落地，这是一个既有缺陷，不是新功能）**：既然 `rowIndex` 会整体平移，那么**重新取数的每一条路径都必须清空全部按 `rowIndex` 记账的选择状态**，否则界面会把「新页的第 3 行」当成「刚才选中的第 3 行」。

- 「全部选择状态」包括**两套**：新增的 `gridSelection`，以及既有的 `selectedRows` 与 `lastSelectedIndex`。只清前者而留下后者，等于保留了最危险的那一套。
- 「重新取数的路径」指：`setPage` / `setPageSize` / `setSort` / `setFilters` / `applyFilters` / `clearFilters`（它们都经 `patchPanelForReload`），以及 `commitFetchedPage` 与 `invalidateCachedData`。
- **实现要求**：重置必须写在 `patchPanelForReload` / `commitFetchedPage` / `invalidateCachedData` 这三处**收敛点**里，**不要**让六个 action 各自记得清——那正是缺陷当初产生的方式。
- **必须有的测试**：`setPage` / `setSort` / `applyFilters` 各一条用例，断言调用后 `selectedRows` 为空、`lastSelectedIndex` 为 `null`、`gridSelection.mode === 'none'`。
- 本项由 **DB-01（分册 01）** 负责，03 的 U-1 独立复现了同一结论，两册结论一致。

#### 现状事实（已逐个收敛点核实，分册不得另行改写）

上表的「三处收敛点」行为**并不一致**，此前多份分册曾各自给出不同说法，这里一次性核实并冻结。各分册引用本表，**不要**再写自己的版本：

| 收敛点 | `selectedRows` | `lastSelectedIndex` |
| --- | --- | --- |
| `commitFetchedPage`（取数成功提交页数据） | 已清空 | **未清空** |
| `invalidateCachedData`（缓存失效） | 已清空 | **未清空** |
| `patchPanelForReload`（`setPage` / `setPageSize` / `setSort` / `setFilters` / `applyFilters` / `clearFilters` 全部经此） | **未清空** | **未清空** |

据此可知三条各自为真的事实，请勿再当作互相矛盾：

1. 「`patchPanelForReload` 从不清 `selectedRows`」——**真的**，这是本项要修的主缺陷；
2. 「`commitFetchedPage` 清空 `selectedRows` 是既有行为」——**真的**，01 / 06 引用这句话指的就是这一处，不是矛盾；
3. 「`setPage` / `setPageSize` / `setSort` 都不清空 `selectedRows`」——**真的**，因为它们走的是 `patchPanelForReload` 而非 `commitFetchedPage`。

**顺带暴露的第二个缺陷**：`lastSelectedIndex` 在**三处都没有**被清空。`selectedRows` 被清空后它仍指向旧下标，Shift + 点击会以陈旧锚点拉出一个跨页区间——用户点一下 Shift 键就选中了跨越前后两页的一批行。修复必须**同时**清 `selectedRows` 与 `lastSelectedIndex`，只清前者等于缺陷还在。

### 4.4 DOM 属性扩展

在既有 `data-dt-row` / `data-dt-col` 基础上新增（供 E2E 与右键菜单使用，**不得**用几何坐标替代）：

| 属性 | 值 | 语义 |
| --- | --- | --- |
| `data-dt-selected` | `"true"` / 不存在 | 该单元格处于当前选择内 |
| `data-dt-dirty` | `"true"` / 不存在 | 该单元格有未提交改动 |
| `data-dt-editing` | `"true"` / 不存在 | 该单元格正被编辑器占用 |
| `data-dt-cell-type` | 归一化后的类型名（最终为 `CellType` 的 11 个值） | 供类型感知编辑器与测试使用 |

**全量 `data-dt-*` 属性总表（唯一权威，禁止任何分册另起名字）**

上一张表只列了 4 个「在既有 `data-dt-row` / `data-dt-col` 之上新增」的属性，但 12 个分册里实际用到了 16 个 `data-dt-*` 名字，且它们此前分散在各册自述、命名互相冲突（同一个滚动容器在 01 叫 `data-dt-surface`、02 叫 `data-dt-grid`、03 叫 `data-dt-grid-surface`）。E2E 与单元测试都靠这些选择器定位，**两处分册写同一个元素用了不同名字，测试只会有一边能跑通，且没有编译错误**。因此在下表固化全量命名与归属：

| 属性 | 落在哪个元素 | 值 | 语义 | **创建归属** |
| --- | --- | --- | --- | --- |
| `data-dt-row` | 行容器（既有） | 行索引 | 行标识 | 既有 |
| `data-dt-col` | 单元格（既有） | 列标识 | 列标识 | 既有 |
| `data-dt-selected` | 单元格 | `"true"` / 不存在 | 该单元格处于当前选择内 | **01** |
| `data-dt-surface` | **滚动容器**（网格可视区根） | 固定 `""` | 网格外层锚点；`role="grid"`、`tabIndex={0}`、`aria-rowcount`/`aria-colcount`/`aria-activedescendant` 都挂在这里 | **01**（统一取代 02 的 `data-dt-grid`、03 的 `data-dt-grid-surface`） |
| `data-dt-row-view` | 行容器 | 行索引 | **roving tabindex 的行级焦点宿主**（`tabIndex={-1}`）。与既有 `data-dt-row` 并存且值相同，**故意不同名**：`data-dt-row` 是数据语义，`data-dt-row-view` 是交互语义，改动其一不得波及另一个 | **01** |
| `data-dt-selection-rows` | 行容器 | `"true"` / 不存在 | 该行属于当前行选择（`mode === 'row'`） | **01** |
| `data-dt-selection-cols` | 列头容器 | `"true"` / 不存在 | 该列属于当前列选择 | **01** |
| `data-dt-selection-cells` | 单元格 | `"true"` / 不存在 | 该单元格属于当前 cell 选择范围 | **01** |
| `data-dt-cell-type` | 单元格 | 归一化类型名 | 类型感知编辑器与测试（见下方收敛说明） | 01 先用 `classifyDataType`，**05 换成 `resolveCellType`** |
| `data-dt-editing` | 单元格 | `"true"` / 不存在 | 该单元格正被编辑器占用 | **05**（01 只加样式 hook，不加此属性） |
| `data-dt-editor-owned` | 单元格 | `"true"` / 不存在 | 编辑器已接管该格，外部点击不得抢焦点（IME / 组合态） | **05** |
| `data-dt-readonly` | 行容器或网格根 | `"true"` / 不存在 | 该范围只读，来自 `DataGridCapabilities.editable` | **05** |
| `data-dt-dirty` | 单元格 | `"true"` / 不存在 | 该单元格有未提交改动 | **06** |
| `data-dt-row-state` | 行容器 | `"dirty"` / `"error"` / 不存在 | 行级聚合态（该行存在脏格或行级错误） | **06** |
| `data-dt-null` | 单元格 | `"true"` / 不存在 | 该格值为 SQL `NULL` | **05**（按 01 的 Q9 裁定：NULL 是**值**的属性，不得污染 `data-dt-cell-type` 的列类型语义） |
| `data-dt-cell-error` | 单元格 | `"true"` / 不存在 | 该格最后一次写入被后端拒绝 | **03** |
| `data-dt-fk` | 单元格 | 引用目标标识 | 该格是外键值，值即 `data-dt-fk` 的目标 | **07** |
| `data-dt-fk-click` | 单元格 | `"true"` / 不存在 | 该外键格可点击跳转（**必须**同时具备 `data-dt-fk`，否则 E2E 会点到不可导航的格） | **07** |

**四条使用纪律（违反了测试与实现会各说各话）**：

1. **禁止新起名字**。要表达新语义，先在本表加行（含创建归属），再在各册引用。禁止出现 `data-dt-grid` / `data-dt-grid-surface` 这类同义异名。
2. **禁止 `data-dt-row-view` 与 `data-dt-row` 合并**。二者值相同是刻意的：前者是 roving tabindex 的选择器契约（02 的 6 条 `[tabindex="0"]` 迁移断言依赖它），后者是数据行标识（既有 E2E 依赖它）。合并会让其中一批测试静默失配。
3. **一册只创建自己那一行的属性**，需要别的属性时等该册合并。例外仅两处：`data-dt-cell-type`（01 建、05 换来源，见下）与 `data-dt-row-view`（01 建，02 只用不改）。
4. **断言用属性存在性，不写行号、不用几何坐标**（`AGENTS.md` 「数据属性解耦」）。`data-dt-*` 出现在测试里时，**同名必须同值**；禁止 `getAttribute('data-dt-row')` 取到 `null` 却在 `expect(...).toBe('3')` 上失败这类跨册失配。

**`data-dt-cell-type` 的取值收敛（按提交顺序分段，避免依赖倒置）**：类型分类的**唯一权威**是 `CellType`（11 个值，由 05 在 `src/lib/cellTypes.ts` 落地 `resolveCellType`）；既有的 `classifyDataType` → `DataTypeFamily`（7 族）降级为**颜色投影**（`dataTypeColors.ts` 的 token 查表继续用它，行为不变）。但 `cellTypes.ts` 由 05 创建，而 01 先合并，因此该属性分两步收敛：

1. **01** 先用**既有的** `classifyDataType(col.type)`（7 族，该函数今天已存在于 `src/lib/dataTypeColors.ts`，01 可直接调用，不需要等待 05）；
2. **05** 合并时把**同一属性的来源**换成 `resolveCellType(col.type)`——这是 `GridCell` 里一行的替换，**01 无需为它改任何代码**，也就不存在「01 依赖一个尚未创建的文件」的倒置。

禁止任何分册为同一个属性维护第二套类型归一逻辑；08 的算子分派、05 的编辑器分派与这个 DOM 属性必须同源。

**ARIA 网格角色必须原子添加（不可拆）**：本方案在滚动容器上与单元格上引入 `role="grid"` 与 `role="gridcell"`。按 ARIA 规范，`gridcell` 必须位于 `role="row"` 之内，`grid` 必须包含 `row`。因此这一组角色（`role="grid"` + 行容器 `role="row"` + 单元格 `role="gridcell"`，连同 `aria-rowcount` / `aria-colcount` / `aria-activedescendant`）**必须由同一个分册（01）一次性加齐**：

- **只加 `role="grid"` + `role="gridcell"` 而漏掉 `role="row"` 是无效 ARIA**，屏幕阅读器会宣布"这是一个网格"却找不到任何行——**比完全不加角色更糟**；
- 02 只在此之上改焦点（roving `tabIndex`）与键盘行为，**不新增也不删除任何 ARIA 角色**；
- 任何分册若只想引入其中一部分，必须先改本契约，**禁止半套**。这一点曾出现分歧（02 主张先不加、01 已规划要加），最终按"要么完整、要么不加"裁定为**完整**：单元格虚拟化的网格需要 `aria-activedescendant` 承担焦点，容器有 `tabIndex` 却没有角色同样是不完整的。

**随之而来的前置改造（01 必须一并完成，否则角色加了也是错的）**：行容器一旦冠上 `role="row"`，其直接子元素的**内容模型就被限定为 `gridcell` / `rowheader` / `columnheader` 或它们在 `grid` 中的替代角色**。今天行号槽渲染的是 `<button>`，`<button>` 不在上述集合内——一旦行容器声明 `role="row"`，屏幕阅读器会把整个行号槽**当作无效内容直接丢弃**，等于「行号槽既不可聚焦也不可朗读」。

因此 **01 的范围里增加一项**：行号槽从 `<button>` 改为 `<div role="rowheader">` + 内部**独立可聚焦**的 `<button>`（`tabIndex={-1}`，焦点由 roving tabindex 在行容器上统一管理），从而满足 `rowheader` 的内容模型并保住键盘可达性。**02 不得**因为「K-7 只加 tabIndex 不加 role」的理由新增或删除任何角色——它只改焦点；它的测试选择器需跟随 01 的改造后的 DOM，但**角色集合本身归 01**。

### 4.5 写路径硬约束（覆盖**全部**写路径，不是某一册的局部要求）

> 此前本约束只以章节局部要求的形式散落在各分册里，**没有任何一条覆盖所有写路径**。任何一条新链路只要不自查就会绕开它。以下四条是**全仓库级**铁律，对本设计集之外将来新增的任何写路径同样生效。

**W-1 · 每条 UPDATE / DELETE 必须携带至少一个非 NULL 的行身份谓词。**

即：拼给 `build_update_sql` / `build_delete_sql` 的 WHERE 片段，必须包含**该表主键列全集**且每个值都非 NULL 的等值条件。少一列、多一列、或任一主键值为 NULL，都必须在**生成 SQL 之前**拒绝，返回 `CommandError::Validation`。

**W-2 · 拒绝发生在 SQL 生成之前，不是之后。**

顺序固定为：`校验行身份 → 生成 SQL → 执行`。**禁止**先生成 SQL 再回头检查 WHERE 是否为空——`build_update_sql` 拿到空的 WHERE 片段时不会报错，它会照常产出一条无条件的语句。

**W-3 · 行身份列集必须与 `CachedColumns.primary_keys` 逐字相等。**

仅校验「非空」不够。既有 `canonicalize_changes` / `identity_key` 只要求主键 map **非空且值非 NULL**，**从不校验键集是否等于 `CachedColumns.primary_keys`**。后果：一张 `(tenant_id, order_no)` 复合主键表，若前端只传了 `{tenant_id: 7}`，就会生成 `WHERE "tenant_id" = 7` 并**静默影响成千上万行**。**复合主键必须全部提供，一列都不能少。**

**W-4 · `affected == 1` 是最后一道兜底，不是唯一防线。**

它能挡住「多行被影响」，挡不住「影响 1 行但改的是错的行」。W-1/W-2/W-3 任一缺失都必须靠它兜底，所以它必须**始终存在**；但**不得**因为有它就放松 W-1~W-3。

#### 既有守卫登记：`validate_legacy_pk_columns` 是今天唯一的空 WHERE 防线，且必须随 C-1 迁移

既有函数 `validate_legacy_pk_columns(columns, operation)`（位于 `src-tauri/src/commands/data.rs`，由 `build_row_change_plan` 链路在「Row update」/「Row delete」两处调用，配套测试 `commit_row_deletes_rejects_empty_pk`）是**当前仓库中唯一**真正阻止空 WHERE 落到 `build_delete_sql` / `build_update_sql` 的检查。

它此前**在 14 份文档的盲区里无人提及**，而它所在的 `src-tauri/src/commands/data.rs`（1331 行）正是 DB-04 Step 1 要目录化为 `src-tauri/src/commands/data/{mod.rs,plan.rs,tests.rs}` 的那个文件——一次目录化就可能把它连同测试一起遗漏。

**必须落地**：

- 04 的目录化 Step **必须把 `validate_legacy_pk_columns` 与 `commit_row_deletes_rejects_empty_pk` 列为显式的迁移动作**，写进该 Step 的变更表，不得只写「拆分文件」；
- 它的校验语义并入 W-1（注意：它只校验非空，**不含 W-3 的列集相等**，因此不能直接当作 W-1 的实现，它需要被加强）；
- C-1 删除遗留入口后，该函数**不得随之删除**，其逻辑迁入新链路的 `row_change_plan.rs`；
- `12-testing.md` 的门禁必须包含「`validate_legacy_pk_columns` 及其测试在拆分后仍然存在」这一条断言。

**W-5 · 并发覆盖必须显式承认，不得声称"只影响刚选中的那一行"。**

既有实现里，行身份快照只**采集**被编辑列的原值，**不含**并发校验。若两个客户端同时编辑同一行的**不同**列，后提交的一方会**静默覆盖**先提交的一方（last-writer-wins）。这不是本次引入的缺陷，但分册文档**不得**把「用了行身份快照」表述成「不会覆盖别人的修改」——`10-result-grid.md` 曾写「原值快照」，属误导性措辞。

本设计的取舍：**不实现乐观并发控制**（那是独立需求，需引入版本列或 `WHERE` 上的旧值谓词），但要求：

- 文档与 UI 措辞**必须**如实描述为「以行身份定位并写入，不保证检测并发冲突」，禁止出现「只影响这一行」「不会覆盖」之类暗示隔离性的表述；
- WHERE 只用**行身份**（主键）谓词，**不得**把非主键列的原值也塞进 WHERE——那会让「我改了这一列」反过来变成「我只能改这一列」；
- 未来若引入乐观锁，另立需求，不得在本次实现中半途引入。

**W-6 · 用户可见的写入拒绝原因不得回显数据库原始错误文本。**

`CommandError` 的序列化只调用凭据脱敏（`redact_secrets_for_log`），而该脱敏器**只覆盖 4 类凭据模式，不处理绝对路径**。因此像 `grid.result.writeDenied` 这类码若直接携带数据库原文，会把本机目录结构暴露到界面与日志中。凡是写入被拒的文案，必须由宿主**重写成自己的话**，原始错误只进日志且同样受日志脱敏约束。

同理，DB-04 的 `parameter_summary` / `value_summary` 是**截断**（120 字符），**不等于脱敏**——截断后仍可能保留可识别的业务数据。摘要字段只用于展示与日志，**不得**进入 IPC 返回体的持久化路径。

---

## 5. `driver-api` 新增方法契约（共 13 个方法 / 分 5 组，全部带默认实现）

以下方法签名是**最终形态**，分册不得改动；实现细节写在对应分册。

### 5.1 插入语句构造（DB-04）

```rust
// packages/driver-api/src/traits.rs
/// 构造一条插入语句。`Unset` 的列不进语句（见第 3.5 节语义要求）。
fn build_insert_sql(&self, table: &str, columns: &[(&str, CellWrite)]) -> String {
    sql_text::build_insert_sql(self, table, columns)
}

/// 方言的新增行主键回填子句；`None` 表示该方言无法回填。
/// 例：PostgreSQL/SQLite → `RETURNING <cols>`；MySQL → 用 `LAST_INSERT_ID()`，由驱动另行处理。
fn insert_returning_clause(&self, columns: &[&str]) -> Option<String> {
    None
}
```

**宿主行为约定**：`insert_returning_clause` 返回 `Some` 时，宿主把子句追加到插入语句并读取返回行以回填主键；返回 `None` 时，宿主**不得**猜测主键，而是走「提交后重新拉取当前页」的降级路径，并在 UI 上不承诺「新行会保持高亮」。

### 5.2 筛选算子能力与方言渲染（DB-08）

```rust
// packages/driver-api/src/traits.rs
/// 本驱动支持的筛选算子白名单。默认返回共享算子集 = 既有 10 个 + 共享扩展 9 个 = 19 个。
/// **不是**「既有 10 个」——若只给 10 个，新算子在任何未显式声明的驱动上都不可用。
/// 共享算子的渲染由宿主负责（见下表），因此对每个驱动都安全；方言专有的 3 个不在其中。
fn supported_filter_operators(&self) -> &'static [FilterOperator] {
    filters::SHARED_FILTER_OPERATORS
}

/// 方言专有算子的渲染入口。默认 `None`：宿主遇到不支持/未实现的算子时必须
/// 向用户报「该数据库不支持此筛选算子」，禁止静默丢弃条件后照常查询。
fn filter_operator_sql(
    &self,
    operator: FilterOperator,
    column_expr: &str,
    literal: &str,
) -> Option<String> {
    None
}

/// 该驱动的 `LIKE` 在默认排序规则下是否大小写不敏感（MySQL 系默认是）。
/// 仅用于 UI 提示与文案，不改变 SQL。默认 false。
fn like_is_case_insensitive(&self) -> bool {
    false
}

/// 本驱动 `LIKE` 的**模式串转义配置**。默认 `None` = **完全不转义**。
/// 今天的 `format_condition` 就是 `{col} LIKE {literal}`（原样透传，用户输入的
/// `%` / `_` 保持通配语义），因此未覆盖本方法的驱动**行为逐字节不变**（R4）。
/// 返回 `Some(c)` 表示：宿主先把模式串里的 `c` 以及 `%` `_` 各转义成 `c` + 自身，
/// 再交给下面的 `like_escape_clause` 决定是否附加 `ESCAPE` 子句。
fn like_pattern_escape(&self) -> Option<char> {
    None
}

/// 本驱动 `LIKE` 是否接受 `... ESCAPE 'x'` 子句。默认 `None` = 不发该子句。
///
/// **必须与 `like_pattern_escape` 成对判定**，只覆盖其一是错误配置：
/// 转义符生效但不发出子句，意味着该方言把那个字符当普通字符，模式串里的转义标记
/// 反而变成字面量——比不转义更糟。
fn like_escape_clause(&self, escape_char: char) -> Option<&'static str> {
    let _ = escape_char;
    None
}

```

**算子集划分（最终定义，禁止分册自行增删）**：

| 分组 | 算子 | 归属 |
| --- | --- | --- |
| 既有 10 个 | `Eq` `Ne` `Gt` `Lt` `Gte` `Lte` `Like` `In` `IsNull` `IsNotNull` | 宿主 `format_condition` 继续渲染，**行为必须逐字节不变** |
| 共享扩展 | `NotIn` `Between` `NotBetween` `Contains` `NotContains` `StartsWith` `EndsWith` `IsEmpty` `IsNotEmpty` | 宿主渲染，进 `SHARED_FILTER_OPERATORS` |
| 方言专有 | `Regex` `NotRegex` `JsonContains` | 只能经 `filter_operator_sql` 渲染；不在共享白名单内 |

**两个常量的精确定义（都放 `packages/driver-api/src/filters.rs`）**：

- `EXISTING_FILTER_OPERATORS`：既有 10 个（`Eq` `Ne` `Gt` `Lt` `Gte` `Lte` `Like` `In` `IsNull` `IsNotNull`）。它只有一个用途：作为「既有行为逐字节不变」的**测试基线**，运行时不被任何默认实现返回。
- `SHARED_FILTER_OPERATORS`：既有 10 个 + 共享扩展 9 个 = **19 个**，即 `supported_filter_operators()` 的默认返回值。方言专有的 3 个（`Regex` / `NotRegex` / `JsonContains`）**不在**其中。

**兼容性说明**：给 `FilterOperator` 加变体会让「对枚举做穷尽匹配」的驱动代码**编译失败**——这是我们要的（响亮失败而非静默走错分支）。但**运行时**必须靠 `supported_filter_operators()` 兜底，因为「编译通过」不等于「数据库真的支持这个算子」。


#### 5.2.1 `FilterCondition` 新增 `connector` 字段（DB-08，PRD `C-32`，P0）

逐条 AND/OR 的唯一数据面改动，落在既有 DTO 上：

```rust
// packages/driver-api/src/filters.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FilterConnector { And, Or }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    pub value: Option<Value>,
    /// 本条件与**前一条**条件的连接符。第 0 条的值**无意义，渲染时必须忽略**。
    /// 缺省（`None`）= 回落到全局 `filterLogic`，即今天的行为（R4）。
    #[serde(default)]
    pub connector: Option<FilterConnector>,
}
```

- **R1 合规**：只新增字段，不改任何既有字段的类型或名字。`#[serde(default)]` 保证老调用方（不发该字段）反序列化后行为不变；R3 的 camelCase 自动映射使前端 `connector?: 'and' | 'or'` 自动对上，无需手写映射。
- **前端镜像**：`src/types/index.ts` 的 `FilterCondition` 加 `connector?: 'and' | 'or'`（可选，与 Rust 的 `Option` 对称）。
- **为什么是「与前一条的连接符」而不是「与后一条的」**：拼接是左到右的；挂在后一条上会让第 0 条永远没有连接符，渲染时反而要多判一次首项。挂在**当前条**上时，第 0 条的无意义值在渲染入口被显式丢弃，判断只有一处。
- **与命名视图**：`NamedView.filters` 存的是同一结构，存盘形态随之带 `connector`（缺省不写）。老视图文件读出来全是 `None` ⇒ 全部回落到全局逻辑 ⇒ 行为不变。

### 5.3 行数策略（DB-09）

```rust
// packages/driver-api/src/types.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CountStrategy {
    /// `SELECT COUNT(*)` 结果可信且开销可接受。
    Exact,
    /// 只能给出估计值（如目录表里的统计行数），UI 必须显示为「约 N 行」。
    Estimated,
    /// 不提供行数（驱动声明 `skip_count_query`、无权限或引擎不支持）。
    Unsupported,
}

// packages/driver-api/src/traits.rs
/// 默认由既有的 `skip_count_query()` 推出，因此所有现有驱动的行为不变。
fn count_strategy(&self) -> CountStrategy {
    if self.skip_count_query() { CountStrategy::Unsupported } else { CountStrategy::Exact }
}

/// 当 `count_strategy() == Estimated` 时，构造估计行数的语句。默认 `None`。
///
/// 参数与 `get_columns` / `get_table_schema` 的既有惯例**保持一致**：传**未引用**的原始表名，
/// 由驱动自行引用；`database` / `schema` 显式传入。
///
/// 为什么不传「已引用好的表名」（本契约的早期草稿如此，已废弃）：MySQL 的估算必须走
/// `information_schema.TABLES WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?`，SQL Server 必须用
/// 三段名（`db.schema.table`）或 `sys.dm_db_partition_stats`——两者都需要**未引用**的库名与表名。
/// 只给一个已引用字符串，会让这两个方言的估算无法安全实现（也无法把 `DATABASE()` 当默认库用，
/// 因为宿主是以显式目标取数的，会话默认库未必等于目标库）。
fn count_estimate_sql(&self, database: &str, schema: Option<&str>, table: &str) -> Option<String> {
    None
}
```

**这条默认实现是本契约里最重要的兼容性技巧**：`count_strategy()` 由既有 `skip_count_query()` 推出，于是「不写任何代码的驱动」得到的策略与今天**完全一致**；想声明 `Estimated` 的驱动才需要覆盖它。分册 09 必须把这一点写成测试。

### 5.4 默认排序策略（DB-09）

```rust
// packages/driver-api/src/traits.rs
/// 无显式排序时优先使用的排序列。默认实现 = 今天的行为：
/// 先取主键列，无主键则取第一列。返回空数组表示「不加默认排序」。
fn default_order_columns(&self, columns: &[ColumnSchema], primary_keys: &[String]) -> Vec<String> {
    if !primary_keys.is_empty() {
        return primary_keys.to_vec();
    }
    columns.first().map(|c| c.name.clone()).into_iter().collect()
}

/// 无主键表上是否允许用「第一列」兜底排序。
/// 默认 `true` 以保持今天的行为；设为 `false` 的驱动在无主键表上得到无排序查询。
fn allow_first_column_fallback_order(&self) -> bool {
    true
}
```

> **注意（这里有一个必须先讲清的行为变更）**：默认实现的**选择逻辑**与今天完全等价（有主键用主键、无主键退化为第一列），但**输入变了**——
>
> - 今天：`build_select_sql` 从传入的 `columns` 里挑 `is_primary_key == true` 的列（见第 1.5 节事实 9）。
> - 新设计：宿主把 `CachedColumns.primary_keys`（已经过 `effective_primary_keys()` 归一化）作为 `primary_keys` 参数传进去。
>
> 当驱动的「列标志」与「primary_keys 字段」不一致时（有驱动确实如此），两者结论不同：新设计会按真实主键排序，旧实现会退化成按第一列排序。**这是一次有意的修正，不是回归**，但必须满足三个条件才允许合入：
>
> 1. 在分册与提交信息里**显式声明**这是一次行为变更，并说明影响面（无主键表与「主键字段与列标志不一致」的表）；
> 2. 为「有主键 / 无主键退化 / 复合主键 / 显式排序覆盖」四条路径各写一条测试，并**额外**写一条「列标志与 primary_keys 不一致时按 primary_keys 排序」的测试来钉住新语义；
> 3. 确认 `build_select_sql` 是**私有**函数（`fn`，非 `pub`），因此给它加参数**不违反**第 2 节 R1——R1 约束的是驱动可见的 trait 方法与 IPC DTO，不是宿主内部私有辅助函数。若你发现它已被外部引用，先解决引用再改。

### 5.5 键集分页（DB-18，阶段二）

```rust
// packages/driver-api/src/traits.rs
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

DB-18 的完整落地（含降级与边界）写在分册 11；**接口在本册冻结**，实现可分阶段交付。

---

## 6. IPC 命令契约变更

### 6.1 新增命令

| 命令（Rust snake_case） | 前端封装（camelCase） | 用途 | 分册 |
| --- | --- | --- | --- |
| `estimate_table_rows` | `estimateTableRows` | 取估计行数（`CountStrategy::Estimated` 时） | 09 |
| `resolve_foreign_key_targets` | `resolveForeignKeyTargets` | 取外键候选值（跳转与选择器共用） | 07 |
| `list_named_views` / `save_named_view` / `delete_named_view` | 同名 camelCase | 命名筛选视图的持久化 | 08 |

### 6.2 既有命令的**新增可选**字段

- `getTableData`（宿主 `get_table_data_impl`）增加可选入参 `countStrategy`（缺省时按驱动 `count_strategy()` 决定），以及可选 `defaultOrderColumns`（缺省走 5.4 的默认实现）。
- `commitPendingChanges`（宿主 `commit_pending_changes_impl`）增加可选入参 `inserts: Vec<...>`，字段**可选且缺省为空**，因此老前端调用形状不变。

**铁律**：新增字段一律 `Option<T>` + `#[serde(default)]`；**禁止**把既有字段改成必填，否则老前端（或 MCP 这类第三方调用方）会直接反序列化失败。

### 6.3 两层错误的分工

- **`CommandError`**：宿主 IPC 层错误（校验失败、计划过期、会话不存在）。新错误码统一加 `grid.` 前缀，见第 8 节。
- **`DriverError`**：驱动/SQL 层错误。宿主**不得**把 `DriverError` 的原样文本直接透给用户（可能含 SQL 片段与库名），须经 `CmdExt` 统一脱敏。
- 全局 IPC 错误 Payload 会自动脱敏凭据与绝对路径，新代码不要绕过这层。

---

## 7. 前端能力声明：`DataGridCapabilities`

位置：`src/lib/databaseMeta.ts` 的 `DatabaseTypeMeta`，按 `KvWorkspaceCapabilities` 的既有范式新增。

**字段名冻结（一次裁定，各分册不得另起名字）**：宿主上的字段名是 **`dataGrid?: DataGridCapabilities`**，与既有 `kvWorkspace` 的「能力域 + Capabilities」命名风格一致。驱动在自己的 meta 文件里写 `dataGrid: { … }`，宿主读 `meta.dataGrid?.<字段>`。

```ts
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
```

**前端消费规则**：

- 读不到声明时**一律按缺省值**，等同于今天的体验；
- 入口按能力**隐藏**而非禁用（禁用按钮会让用户以为是 bug）；
- 后端 trait 方法是权威、前端声明只是「提前知道」，两者不一致时以后端返回值与错误为准并在 UI 上给出可理解提示。

---

## 8. 统一错误码与用户可见文案

### 8.1 机制：错误码是**前端侧的分类结果**，不是 IPC 的新字段

先看清现状，否则会设计出一个改动面极大的方案：

- `CommandError` 是一个 Rust 枚举，但它的 `Serialize` 实现**把整个错误序列化成一个脱敏后的字符串**（先脱敏再 `serialize_str`），前端收到的是 `string`，**没有** `code` 字段。
- 既有约定是「错误类别必须保留在文本里，供前端分类」（`commands/error.rs` 的测试明确断言了这一点）。
- 因此**给 IPC 错误加 `{ code, message }` 结构会改动所有命令共享的错误契约**，属于第 2 节 R1 禁止的「改既有契约」，不是本批方案该做的事。

**所以本节的错误码按下述两层实现**：

1. **后端**：新增的、需要被前端精确识别的错误，其 `CommandError` 消息**必须以契约前缀开头**，格式为 `grid.<域>.<原因>: <人类可读说明>`。例如：
   `grid.commit.affectedMismatch: expected 1 affected row, got 3`
   前缀是**契约的一部分**，改动它等同于改契约。
2. **前端**：定义 `GridErrorCode` 联合类型与 `classifyGridError(message: string): GridErrorCode`（**位置 `src/lib/gridErrors.ts`，由分册 01 创建，见下方「归属总则」**），用**前缀匹配**归一化。匹配不到任何前缀时返回 `'unknown'`，此时 **UI 必须回退到显示原始消息**，而不是吞掉错误或显示空白。

**为什么用前缀而不是正则/包含匹配**：前缀是唯一在两侧都能稳定断言的形态，测试可以逐字比对；包含匹配会因为消息里出现同样的词而误判。

#### 归属总则：`src/lib/gridErrors.ts` 只由 01 创建，其余分册一律「追加分支」

> 这是全文唯一一处**结构性**的文件归属裁定，其余分册不得自行声明创建该文件。

| 分册 | 动作 |
| --- | --- |
| **01** | **创建** `src/lib/gridErrors.ts`，含 `GridErrorCode` 联合类型、`classifyGridError`、`grid.<域>.*` 的**前缀注册表骨架**（本册只填 `grid.selection.*` / `grid.cell.*` / `grid.row.*` 三个域）。 |
| 03 / 04 / 05 / 06 / 07 / 08 / 09 / 10 / 11 | **复用 01 的骨架**，只往联合类型与注册表里**追加自己那一域**的码。**禁止**新建该文件、禁止整文件覆盖。 |

**为什么必须这样裁定**：本设计集的 12 份分册曾对同一文件给出 6 种互斥说法（01 建 / 7 册说「新增」/ 08 说「与 09 共用，只建一次」/ 06 与 07 与 11 说「该文件不存在」）。后果不是编译失败而是**静默全文件覆盖**——后合并的分册把自己那份 `classifyGridError` 整体写回去，01 刚加的分支与类型一并消失，`tsc` 全绿、测试在合并瞬间开始漏分支，直到线上某条错误落到 `'unknown'` 才被发现。

**追加的合规动作只有三步**（顺序固定）：① 在 `GridErrorCode` 联合类型里加成员；② 在前缀注册表里加本域条目；③ 在本册 §6 变更表登记文件为「修改（追加）」。**第 ④ 步禁止**：任何分册不得通过「重写整个 `gridErrors.ts`」来加自己的码。

**新码的登记门禁**：分册若在本册设计中新造了本表以外的 `grid.*` 码，必须**同时**在本表登记一行（码 / 触发条件 / 文案要求 / 归属分册），否则不得进入实现。分册无权自行扩充错误码空间。

### 8.2 错误码清单（全量 42 个）

前缀一律 `grid.`，其后按「域.原因」分段。**本表是全集**：本设计集在实现阶段只允许使用下面这 42 个码，一个不多一个不少。

| # | 错误码 | 归属分册 | 触发条件 | 用户可见文案要求 |
| --- | --- | --- | --- | --- |
| 1 | `grid.cell.readOnly` | 05 | 连接级只读 / 无主键表 / 生成列 | 说明不可编辑的**具体原因**，禁止只显示「不可编辑」 |
| 2 | `grid.cell.invalidValue` | 05 | 类型化编辑器解析失败 | 说明期望类型与收到的值 |
| 3 | `grid.cell.ambiguousIdentity` | 03 | 一页内出现重复行身份键 | 说明该页无法安全定位，请缩小范围 |
| 4 | `grid.cell.noRowIdentity` | 03 | 目标行没有可用行身份锚点 | 说明该行不可写入 |
| 5 | `grid.commit.stalePlan` | 04 / 06 | `fingerprint` 不匹配 | 说明计划已过期，要求重新预览 |
| 6 | `grid.commit.affectedMismatch` | 04 / 06 | 影响行数 ≠ 1（INSERT 除外） | 说明「预期影响 1 行、实际 N 行，已回滚/未提交」 |
| 7 | `grid.commit.conflictingIntents` | 04 | 同一行既标记删除又有列写入 | 说明冲突并指出是同一行 |
| 8 | `grid.insert.noWritableColumn` | 04 | 一行的所有列都是 `Unset`、且无默认值可依赖 | 提示至少填一列 |
| 9 | `grid.insert.returningUnavailable` | 04 | `insert_returning_clause` 为 `None` | 说明「新行主键无法自动定位，已为你刷新当前页」 |
| 10 | `grid.insert.duplicateDraft` | 04 | 同一 draftId 被提交两次 | 说明草稿已提交，请刷新 |
| 11 | `grid.insert.unknownColumn` | 04 | 草稿引用了不存在/不可写的列 | 指出具体列名 |
| 12 | `grid.filter.unsupportedOperator` | 08 | 算子不在驱动白名单 | 说清「该数据库不支持 X 筛选」，并给出可替代算子 |
| 13 | `grid.filter.incomplete` | 08 | 筛选条件缺值 | 提示补齐，**不得**静默丢弃后照常查询 |
| 14 | `grid.view.invalid` | 08 | 命名视图名非法/为空 | 说明命名规则 |
| 15 | `grid.view.nameExists` | 08 | 命名视图重名 | 提示换名或覆盖 |
| 16 | `grid.count.estimateUnavailable` | 09 | 请求估算但驱动无该能力 | 回退到 exact 并说明，不静默 |
| 17 | `grid.paste.tooLarge` | 03 | 粘贴数据超过上限 | 说明上限值与实际大小 |
| 18 | `grid.paste.raggedRow` | 03 | 各行列数不一致 | 说明不一致的行号 |
| 19 | `grid.paste.unclosedQuote` | 03 | TSV 内引号未闭合 | 说明无法解析的行列 |
| 20 | `grid.paste.outOfBounds` | 03 | 粘贴区域越出可见列 | 说明越界列名 |
| 21 | `grid.paste.noWritableCell` | 03 | 粘贴目标单元格均不可写 | 说明原因（只读/生成列） |
| 22 | `grid.paste.notNullEmpty` | 03 | 空值写入 NOT NULL 列 | 说明该列不接受空值 |
| 23 | `grid.paste.identityChanged` | 03 | 粘贴期间行身份已变 | 要求重新加载后再粘 |
| 24 | `grid.paste.empty` | 03 | 剪贴板无有效数据 | 说明未读到可粘贴内容 |
| 25 | `grid.clipboard.readFailed` | 03 | 读系统剪贴板失败 | 提示重试或手动复制 |
| 26 | `grid.clipboard.writeFailed` | 03 | 写系统剪贴板失败 | 提示重试 |
| 27 | `grid.clipboard.paste` | 03 | 粘贴流程级失败（无法归入上面各具体原因） | 给出可复制的上下文 |
| 28 | `grid.fk.targetUnresolved` | 07 | 外键目标表/列解析不出 | 说明哪个外键解析失败 |
| 29 | `grid.fk.unknownConstraint` | 07 | 找不到对应外键约束元数据 | 说明无法识别引用来源 |
| 30 | `grid.fk.valueNull` | 07 | 外键列值为 NULL，无从跳转 | 说明该行为空 |
| 31 | `grid.fk.shapeMismatch` | 07 | 目标列数/类型与源不匹配 | 说明不匹配之处 |
| 32 | `grid.fk.probeFailed` | 07 | 探测目标表元数据失败 | 说明探测失败原因 |
| 33 | `grid.result.originUnavailable` | 10 | `describe_result_origin` 未实现/不可判定 | 说明结果集来源不明，保持只读 |
| 34 | `grid.result.originAmbiguous` | 10 | 来源可判定但不唯一 | 说明存在多个候选来源，保持只读 |
| 35 | `grid.result.rowNotAddressable` | 10 | 结果行没有唯一可定位行身份 | 说明该结果集行不可写 |
| 36 | `grid.result.multiRowNotAllowed` | 10 | 结果行映射到多张源表行 | 说明存在歧义，保持只读 |
| 37 | `grid.result.writeDenied` | 10 | 目标列不可写（生成列/无 PK 等） | 说明具体原因 |
| 38 | `grid.paging.keysetUnavailable` | 11 | 表无稳定唯一键集 | 说明该表无法键集分页 |
| 39 | `grid.paging.cursorInvalidated` | 11 | 游标指向的数据已变更 | 说明需从头重新加载 |
| 40 | `grid.paging.cursorTooDeep` | 11 | 游标链超过上限 | 说明层数上限 |
| 41 | `grid.paste.crossTable` | 03 | 剪贴板来源表与目标表不是同一张表 | **必须**说明来源表与目标表全名，并要求用户确认后才写入 |
| 42 | `grid.paste.columnMismatch` | 03 | 按列名对齐时，来源列在目标表找不到同名列 | 说明被丢弃的来源列名；被丢弃的列**不得**按位置补写 |

**域 → 注册表前缀的映射**（`classifyGridError` 按此表最长前缀匹配）：

| 域前缀 | 码数 | 归属分册 |
| --- | --- | --- |
| `grid.cell.*` | 4 | 05（2）/ 03（2） |
| `grid.commit.*` | 3 | 04 / 06 |
| `grid.insert.*` | 4 | 04 |
| `grid.filter.*` | 2 | 08 |
| `grid.view.*` | 2 | 08 |
| `grid.count.*` | 1 | 09 |
| `grid.paste.*` | 10 | 03 |
| `grid.clipboard.*` | 3 | 03 |
| `grid.fk.*` | 5 | 07 |
| `grid.result.*` | 5 | 10 |
| `grid.paging.*` | 3 | 11 |

**不属错误码、不得混入本表的同名命名空间**（历史上已被误扫入）：PRD 的埋点事件名（`grid.edit.commit` / `grid.selection.created` / `grid.shortcut.used` / `grid.row.inserted` / `grid.count.mode` / `grid.filter.operator` / `grid.fk.follow` / `grid.default_sort` / `grid.saved_view.used`）、`settings.grid.pasteOverflow*` 这组 i18n key、以及文件名里的 `*-grid.md`。它们与 `GridErrorCode` 无任何关系，`classifyGridError` 不得为其返回任何分支。

**文案纪律**：所有新增用户可见文案**只改英文侧**。注意英文侧不是单个文件，而是**领域包**：真正的词条在 `src/locales/en/<域名>.ts`（现有域名有 `core` / `connection` / `query` / `schema` / `settings` / `sync` / `chart` / `dashboard` / `ai` / `backup` / `mcp` / `onboarding` / `workflows`）。数据浏览相关文案按语义就近放入 `query.ts` 或 `connection.ts`，**不要新建域名**。
>
> ⚠️ `src/locales/en.ts` **只是聚合再导出的兼容入口**（内容为 `export { default } from './en/index'`），改它没有任何效果。分册的 key 清单必须写清「哪个域名文件 + 完整 key 路径」。
>
> 其他语言文件（`de.ts` / `zh-CN.ts` / …）在开发期**不要动**，发布前由 `scripts/i18n-sync-check.mjs` 与 i18n 同步流程统一补齐。

---

## 9. 分册依赖图与实现顺序

```
00-contracts（本册：契约冻结，无代码）
  ├─ 01-selection      DB-01 单元格/区域选择模型   ← 02/03/05/06 的地基
  │    ├─ 02-keyboard  DB-02 键盘导航与快捷键
  │    ├─ 03-clipboard DB-03 剪贴板复制/粘贴
  │    ├─ 05-editors   DB-05 类型化编辑器与 Set Value
  │    └─ 06-dirty     DB-06 待提交单元格高亮
  ├─ 04-insert         DB-04 新增行 INSERT（依赖 3.2 的 CellWrite）
  ├─ 07-fk-navigation  DB-07 外键跳转与返回栈（依赖 01）
  ├─ 08-filters        DB-08 筛选能力与命名视图（依赖 5.2）
  ├─ 09-count-and-order DB-09 行数三态与默认排序（依赖 5.3/5.4）
  ├─ 10-result-grid    DB-14 查询结果网格可编辑（依赖 01/02/03/04/05/06）
  ├─ 11-keyset-paging  DB-18 键集分页（依赖 5.5，阶段二）
  └─ 12-testing        共享测试基建与门禁（贯穿所有分册）
```

**推荐提交切分**（每一条都是可独立合并、可独立回滚的原子提交）：

1. `feat(driver-api): add CellWrite and insert builders with default impls`（仅契约与默认实现 + 单测）
2. `feat(grid): add cell selection model`（01）
3. `feat(grid): add keyboard navigation`（02）
4. `feat(grid): add clipboard support`（03）
5. `feat(grid): add type-aware cell editors`（05）
6. `feat(grid): highlight pending cells`（06）
7. `feat(grid): support inserting rows`（04，前后端 + 驱动）
8. `feat(grid): add filter operators and named views`（08）
9. `feat(grid): add count strategy and configurable default order`（09）
10. `feat(grid): make query result grid editable`（10）
11. `feat(grid): add foreign key navigation`（07）
12. `feat(grid): add keyset pagination`（11）

**每一份分册都必须自己写清**：本册不变量是否被破坏、需要同步修改的既有测试有哪些、以及「做完了怎么证明」。

### 9.1 共用文件的归属与追加纪律（合并期唯一的裁决依据）

提交序（01 → 02 → 03 → 05 → 06 → 04 → **08 → 09** → 10 → 07 → 11）**同时就是共用文件的归属序**。多条分册要写同一个文件时，按下面三条执行，**不再逐对协商**：

| 规则 | 内容 |
| --- | --- |
| **G-1 唯一归属** | 一个新文件/新接口由**提交序里最早需要它的那一册**创建。后续分册一律标「修改（追加）」，**禁止**再写「新增」「新建」「与 X 共用，只建一次」「谁先合并谁建」。 |
| **G-2 禁止整文件重写** | 后续分册在共用文件上**只允许新增自己那几行**（枚举成员、注册表条目、导出）。整文件覆盖在并行合并下**不产生任何编译错误**，只会静默抹掉先到者的内容——这是本设计集最危险的一种失败模式。 |
| **G-3 显式先后边** | 若 B 依赖 A 的**产出结构**（不只是同一文件），依赖图上必须有一条实边，且 B 排在 A 之后。 |

**已按 G-1/G-2/G-3 冻结的全部分配**（这是唯一清单，各册不得再自行博弈）：

| 共用对象 | 创建归属 | 后续分册只允许做什么 |
| --- | --- | --- |
| `src/lib/gridErrors.ts` | **01** | 03/04/05/06/07/08/09/10/11 按契约 §8.1 三步追加本册前缀的码 |
| `src/stores/tableData/gridSelection.ts` + `src/hooks/useGridSelection.ts` | **01** | 02 只调，不改签名（签名已冻结于 §4.2.1） |
| `src/components/DataTable/GridCell.tsx` | **01** | 05 加 `editing`/`editorOwned`/`readonly`，06 加 `dirty`，07 加 `fk`；**只加 prop 与 `data-*`，不改既有渲染分支** |
| `data-dt-*` 全量属性总表 | **01**（表在 §4.4） | 各册只在自己那一行里加取值，**禁止另起名字** |
| `src/lib/databaseMeta.ts` 的 `DataGridCapabilities` | **08** | 09 追加 `countStrategy?` / `allowFirstColumnFallbackOrder?` |
| `src-tauri/src/services/query_executor/{mod.rs,tests.rs}`（目录化） | **08** | 09 只改 `mod.rs` 并往 `tests.rs` 补用例，**不做搬迁** |
| `src-tauri/src/commands/data/`（目录化） | **04** | 04 同时把 `validate_legacy_pk_columns` 及其测试迁入 `plan.rs` |
| `src/lib/cellTypes.ts` | **05** | 01 的 `data-dt-cell-type` 在 05 合并时把来源换成 `resolveCellType` |
| `src-tauri/src/services/traits/browsing.rs` | **09** | — |
| `src/stores/fkNavigationStore.ts` | **07** | — |
| `src/stores/panelResultEdit.ts` | **10** | — |

**G-3 的实边（依赖图上原来漏画的一条）**：**09 → 08**。09 依赖的不是 08 的筛选逻辑，而是 08 的**目录化产出**——09 合并时 `query_executor/` 必须已是目录，否则 09 的 `build_select_sql` / `get_table_data` 改动会落在一个已被搬走的文件上。提交序已保证 08 先于 09，此边**只为让依赖图本身完整**，不代表 09 可以引用 08 的筛选符号。

### 9.2 共享热文件总账（跨分册唯一账本）

**为什么需要它**：多个分册会往同一批文件里加接线。各册在 §6 里独立申报行数时**必然互相矛盾**——实际已经发生：08 判定「本册与 09 合计必超 800」，05 判定「由 01 前置抽取后不再触红线」，两者都对了一半（05 没算 03/08/09/11 的增量）。因此行数账目**只有一个权威来源：本小节**。

**强制性规则**：

1. 任何分册在 §6 里对下列热文件申报净增行数时，**必须**同时写明「在自己之前合并的分册已完成哪些抽取」；只写一个数字而不写基线的，视为**未申报**。
2. 若某分册的净增会让文件达到或超过 800 行，该分册**必须**在**同一次提交**里包含自己的抽取方案（新建文件 + 搬走哪些内容 + 预估行数），不得把超限留给下一个分册去撞。
3. 「创建归属」列给出的文件，**只有该分册可以创建**；其他分册只能复用、追加成员或加可选 props，**禁止另建平行文件**（同一职责三个文件名的教训已经出现过一次）。
4. 分册若发现本账本与自己实测不符，**先改本账本**（它是契约的一部分），再改自己的推算；禁止各册各算一套。
5. 所有行数在提交前用 `wc -l` 实测复核，**估算值不得当结论**。
6. **本账本里的每个行数都必须标注实测命令与行号区间**（形如 `wc -l src/…/DataTable.tsx` = 628；抽取量为 `sed -n '200,424p'` = 225 行），**不得只给一个裸数字或「~N」估算**。这条规则本身是被迫加上的：本账本初版三处估算互相矛盾（契约写 −~70、05 写 ~495、07 写 ~450），根因是 01 的抽取量被记成 70 而实测为 225——**三个各自「看起来合理」的估算，叠加后得到一个谁都过不了 800 行红线的结论**。估算值可以出现在分册的讨论里，但**不得进入本账本**。

**创建归属（一次性裁定）**：

| 文件 | 状态 | 创建 / 抽取归属 | 其他分册只能 |
| --- | --- | --- | --- |
| `src/windows/connection/TablePendingChangesBar.tsx` | 新建（**~120 行**） | **01**（从 `TableView.tsx` 抽出，**实测移除 93 行**：JSX 块 530–622，含 `table-tx-controls` 与 `pending-changes-bar` 两个 `data-testid` 容器，testid 逐字保留） | 复用或加可选 props |
| `src/windows/connection/TableFilterToolbar.tsx` | 新建（**~110 行**） | **08**（抽出筛选入口 + 视图菜单 + 快筛输入，**实测移除 ~90 行**：工具栏容器 JSX 449–508 = 60 行，加快筛 handler ~30 行） | 复用或加可选 props |
| `src/stores/tableData/gridSelection.ts` | 新建（纯逻辑，无 React） | **01** | 只复用 |
| `src/hooks/useGridSelection.ts` | 新建（React 接线） | **01** | 只复用 |
| `src/lib/gridErrors.ts` | 新建（骨架 ~60 行） | **01**（见 §8 归属总则与下方裁定说明） | **只允许按 §8 三步追加**，禁止整文件覆盖 |
| `src/lib/cellTypes.ts`（`resolveCellType` / `CellType`） | 新建 | **05** | 只消费 |
| `src/lib/databaseMeta.ts` 的 `DataGridCapabilities` + `dataGrid?` | 既有文件 | **08**（按合并序最早需要它） | 只加自己那一项，且必须已存在于本契约 §7 |
| `src-tauri/src/services/query_executor.rs` → `query_executor/{mod.rs,tests.rs}` | **既有 797 行** ⚠ | **08**（目录化） | 只新增子模块文件（如 `filter_sql.rs`） |
| `packages/driver-api/src/traits/browsing.rs` | 新建 | **09**（DB-09 的第一个新 trait 方法落在这里） | 后续新方法继续追加到同一文件 |
| `src-tauri/src/commands/data.rs` → `data/{mod.rs,row_change_plan.rs,row_change_execute.rs,tests.rs}` | **既有 1331 行** ⚠⚠ | **04**（Step 1，目录化；**必须显式迁移 `validate_legacy_pk_columns` 及其测试**，见 §4.5） | 只新增子模块文件；**任何分册要改这个文件，必须先 rebase 到 04 的目录化之后** |
| `src/stores/panelStore.ts` | **既有 728 行** ⚠ | 不新增归属；**10** 只允许在其上加 ≤15 行接线 | 07 把返回栈放进**新文件** `src/stores/fkNavigationStore.ts`；10 的判定逻辑放进 `src/stores/panelResultEdit.ts`。超出 15 行者必须把逻辑整体搬进新文件，**不得**放任它突破 800 |

#### 行数总账的读法（先读这段，否则下表会被误用）

- 下表的「移除」列是 **`wc -l` 实测的 JSX/代码块行数**，不是拍脑袋的估值。原文里 `−110` / `−120` / `−~70` / `~300` 四种互不相容的数字并存，正是因为没人量过。
- 「前置抽取」列写的是**该分册合并时，账上已经发生的抽取**。凡写作「依赖 08」的，必须是排在 08 之后的提交；若它排在 08 之前（提交序 01 → 02 → 03 → 05 → 06 → 04 → 08 → 09 → 10 → 07 → 11），它就**看不到** 08 的抽取，不得预先扣除。
- **峰值以「该分册自己那次提交结束时」为准**，不是以最终值为准。

**`src/windows/connection/TableView.tsx` 行数总账**（基线 **770**，已实测）：

| 合并序 | 分册 | 本册净增 | 本册移除 | 前置抽取（该提交时已发生） | 提交后余额 |
| --- | --- | --- | --- | --- | --- |
| 1 | **01** | +10 | **−93**（`TablePendingChangesBar`） | 无 | **687** |
| 2 | 02 | +4（键盘逻辑全在 `gridKeyboard.ts` / `useGridKeyboardNav.ts`，本层仅挂 props） | 0 | 01 | **691** |
| 3 | 03 | +20 | 0 | 01 | **711** |
| 4 | 05 | +25 | 0 | 01 | **736** |
| 5 | 06 | ±0（改走 `TablePendingChangesBar`，不再内联计数块） | 0 | 01 | **736** |
| 6 | 04 | 0（只改 `commands/data.rs`，Rust 侧） | 0 | — | **736** |
| 7 | 08 | +40 | **−90**（`TableFilterToolbar`） | 01 | **686** |
| 8 | 09 | +60 | 0 | 01 + 08 | **746** ← **峰值** |
| 9 | 10 | 0（改的是 `ContentViewDrawers.tsx`，不是本文件） | 0 | — | **746** |
| 10 | 07 | +10 | **−30**（`TableDataToolbar` + `TableFkBreadcrumb` 合计，净 −20 取整后按 −30 记） | 01 + 08 | **726** |
| 11 | 11 | +15 | 0 | 01 + 08 | **741** |

**峰值 746（09 合并后），距 800 余量 54 行，全程不触红线。**

> **为什么 08 的抽取仍然是强制前置条件**：把 08 那次 −90 拿掉重算，其余各册一字不改，则第 8 行（09 合并后）变成 **836 > 800**，第 11 行（11 合并后）变成 **891**。也就是说触发点**不是**原文所说的「各册合计 824」，而是**逐笔叠加后的第一个越界提交——09**。结论不变（不做这次抽取，09 无法合并），但越界数字与触发点此前两处都算错了。
>
> **三次抽取的顺序不可颠倒**：01（`TablePendingChangesBar`，−93）→ 08（`TableFilterToolbar`，−90）→ 07（`TableDataToolbar` + `TableFkBreadcrumb`）。三次都由各自的分册在**自己那次提交里**完成。任何分册若在自己的提交里看到这个文件已超过 800 行，说明它没有 rebase 到前一次抽取，**必须先 rebase 而不是继续加**。

**`src/components/DataTable/DataTable.tsx` 行数总账**（基线 **628**，已实测）：

| 合并序 | 分册 | 本册净增 | 本册移除 | 前置抽取 | 提交后余额 |
| --- | --- | --- | --- | --- | --- |
| 1 | **01** | +10 | **−225**（`useDataTableContextMenu.ts`，实测块 200–424，即整个 `handleContextMenu` 回调） | 无 | **403** |
| 2 | 02 | +40 | 0 | 01 | **443** |
| 3 | 03 | +25 | 0 | 01 | **468** |
| 4 | 05 | +15 | **−40**（`CellEditorHost` 抽走，编辑器宿主移出） | 01 | **443** |
| 5 | 06 | +20 | 0 | 01 + 05 | **463** |
| 6 | 10 | +20 | 0 | 01 + 05 | **483** ← 峰值 |

**峰值 483（10 合并后），距 800 余量 317 行。**

> 原文三处数字（契约「−~70」、05 的「~495」、07 的「~450」）彼此不相容，根因是把 01 的抽取规模从 225 误记成 70。实测基线 **403** 而非 ~558 / ~500，因此该文件的余量远比原先估计的宽裕：**任何分册往它加超过 100 行才需要考虑抽取**（不是原文的 50 行）。若某分册仍按 628 这个旧基线做推算，一律以本表为准。

**其余热文件基线（已实测，分册引用前请先用 `wc -l` 复核）**：`FilterEditor.tsx` 558、`tableDataStore.ts` 722、`panelStore.ts` 728、`ContentViewDrawers.tsx` 228、`VirtualBody.tsx` 207、`query_executor.rs` 797、`commands/data.rs` 1331、`driver-api/src/types.rs` 939、`driver-api/src/traits.rs` 1020。

**说明：为什么 `gridErrors.ts` 由 01 创建而不是 04/05/08**。01 的原文建议「留给 04/05 首次建立」，理由是「DB-01 不产生后端错误」。但按提交顺序 **01 最先合并**，而 01 自己就要写 `classifyGridError` 的消费侧用例（前缀匹配 + `'unknown'` 回退必须保留原始消息）。若文件不存在，01 的用例无法编译，01 就被一个下游分册卡住。因此按「**提交顺序最靠前、且确实需要该文件的分册负责建骨架**」这一统一规则，改由 **01 建骨架**（~60 行，形状完全照抄本契约 §8），04/05/08/09/10 只追加前缀常量。这条规则同时消除了「两个分册各建一份骨架」的风险。

---

## 10. 分册写作模板（强制）

每份分册必须包含以下小节，顺序一致、标题一致。缺节的文档视为未完成：

1. **目标与验收口径** —— 一句话目标 + 可验证的验收标准（命令 + 期望输出）
2. **现状代码事实** —— 只列你**亲自读过**的符号，禁止行号，禁止推测
3. **数据结构与接口设计** —— TS 与 Rust 完整定义，逐字段说明用途与缺省
4. **交互与状态机** —— 进入条件 / 状态内行为 / 退出跃迁，含键盘、鼠标、焦点、输入法（IME）
5. **实现步骤** —— Step 1..N，每步写「改哪个文件、加什么符号、为什么、怎么自测」
6. **文件级改动清单** —— 新增/修改的文件、职责、预估行数、是否触及 800 行上限
7. **边界与异常清单** —— 逐条给出期望行为，含空表、单行、无主键、只读、NULL、超长值、多字节字符
8. **i18n key 清单** —— 只改英文侧领域包 `src/locales/en/<域名>.ts`（**不是** `en.ts`，它只是再导出入口）；写清完整 key 路径
9. **测试清单** —— 单测 / 组件测试 / 连续旅程测试（journey）/ E2E，给出用例名与断言要点
10. **自查清单** —— 实习生最容易做错的 5~10 条
11. **未决问题** —— 需要人工裁定的点，逐条给出建议选项

**引用副本的精度要求（分两档，写分册时必须遵守）**：分册可以在自己的正文里重述契约定义，但按用途分两档：

- **落地该契约的分册**（如 04 是 `CellWrite` 的落地册）：其正文里的定义必须与契约**逐字一致，包括文档注释**。注释里往往带着落地必需的细节——`CellWrite::Unset` 的注释写明了「INSERT 时由数据库自己填（DEFAULT / 自增 / 生成列 / 触发器）」，这正是 INSERT 分册必须让读者看到的信息，压掉它就等于丢掉了语义。
- **仅消费该契约的分册**：可以压掉注释，但**变体集合、声明顺序、字段类型必须完全一致**，并且必须标注「照抄契约 §X，本册不改」。

自查方式：对同名代码块做逐字比对（见总纲 §8 的一致性纪律）。

---

## 11. 未决问题（需人工裁定）

| # | 问题 | 建议 |
| --- | --- | --- |
| C-1 | `CellWrite` 是否也替换既有的 `CellUpdate.value: Option<Value>`（遗留批量入口 `commit_row_updates` / `commit_row_deletes`） | **已裁定：删除遗留入口**。原「建议不动」的口径已推翻，理由如下（复核后确认，属高危）：① 两个命令**仍注册在 Tauri IPC**（`bootstrap/run.rs` 的 `generate_handler!` 列表内），任何 IPC 调用方或注入的 JS 都能触达，它们**不只**是死代码；② 两者都直接迭代 `driver.execute(...)` 并**把 affected 计数整个丢弃**，因此**是全仓库唯一不走 `affected == 1` 护栏的写路径**——DB-04 建立的那道护栏它们完全不经过；③ 两者只有 `validate_legacy_pk_columns` 挡空 WHERE，而它只校验「pk_columns 非空」，不校验是否等于真实主键列集；④ 前端**零生产调用点**（仅 `tableDataStore` 的测试 mock 断言 `not.toHaveBeenCalled()`），删除的使用成本为零。**动作**：从 `generate_handler!` 注销两个命令，删除 `commit_row_updates_impl` / `commit_row_deletes_impl` 及其导出，保留并迁移 `validate_legacy_pk_columns` 的校验逻辑到新链路（见 §4.5），同步删除 `tableDataStore.test.ts` 中的 mock 与 `not.toHaveBeenCalled()` 断言。**回归防线**：新增一条契约测试，断言 `tauri::generate_handler!` 注册的命令清单中**不存在** `commit_row_updates` 与 `commit_row_deletes`——否则将来重新注册时无人察觉 |
| C-2 | ~~`supported_filter_operators()` 默认返回共享算子集，是否意味着所有驱动「宣称支持」新算子但实际 SQL 可能失败~~ **已裁定** | **裁定：默认返回共享的 19 个**（§5.2 与 §7 已按此冻结）。原先的顾虑不成立：共享的 19 个算子**全部由宿主 `format_condition` 渲染**，驱动不需要为它们写任何代码（驱动只提供 `quote_ident` / `format_sql_literal` 这两个已有方法）；只有**方言专有的 3 个**（`Regex` / `NotRegex` / `JsonContains`）必须由驱动显式声明并经 `filter_operator_sql` 渲染。因此缺省给 19 个不会让任何驱动"谎称支持"，也不会造出会报错的入口——真正无法渲染的组合由 `filter_operator_sql` / `supported_filter_operators()` 在**发语句之前显式拒绝**兜住，而不是靠把白名单缩小到 10 个。若按原建议只给 10 个，新算子在**任何未改动的驱动**上都不可用，DB-08 等于白做。残留的真实差异只有 `LIKE` 的大小写与转义（各驱动默认排序规则不同），它由 `like_is_case_insensitive()` 只影响**文案**，不改变 SQL |
| C-3 | ~~`PROTOCOL_VERSION` 是否随本次改动提升~~ **已裁定：不提升** | **不提升，保持 4**（`MIN_PROTOCOL_VERSION` 保持 1）。分册 04 独立复核后得出同一结论：只新增**带默认实现**的 trait 方法、默认行为等于今天、不动任何既有方法签名与 IPC DTO 字段。唯一残留是「旧 `ref` 的 git 驱动不会覆盖新方法」，那是**驱动覆盖缺口**（登记为刷新 git 驱动 `ref` 的事项），不构成提升协议版本的理由——提升版本会强制所有插件同步升级，代价远大于收益。若后续确需修改**既有**方法签名，再单独提升并同步 git 驱动 `ref` |
| C-4 | **W-3 与「预览 → 提交」之间的 TOCTOU：列集相等校验比对的是 `SchemaCache::get_columns` 的缓存快照，而计划指纹（`changes_fingerprint`）只覆盖列值、不覆盖表结构。若两次预览之间、或校验通过与语句发出之间，另一会话对表做了 DDL（加列、删列、改主键），本设计的三道防线都会基于过期的 `primary_keys` 判定 | **已裁定：登记为已知残留，不在本批实现**，理由与本批的最小成本缓解如下。理由：① 修彻底需要在 SQL 上附加 `WHERE` 的旧值谓词或引入版本列，那是 **DB-04 范围外**的乐观并发控制，与 W-5「本设计明确不实现乐观并发」同源；② SchemaCache 本来就是缓存，任何基于它的判定都天然带这个窗口，本批若单独给写路径补一层，会与只读路径的判定口径分叉；③ DDL 与本批的 P0 目标（选择、键盘、剪贴板、筛选）正交，把它塞进本批会让 04 的提交链路再增一条与 §4.5 W-1…W-4 平行的规则，增加实习生实现面却不降低真实风险。**本批的最小成本缓解（必须在 04 落地，不得省略）**：W-3 的列集相等校验必须在**紧邻生成 SQL 的那一刻**读取 `SchemaCache::get_columns`（**不得**复用预览阶段缓存下来的主键列表），这样至少把窗口从「整个面板生命周期」压缩到「单次提交调用内」；并在提交失败时把该次读取的主键列集与失败语句的类型一并写进日志，便于事后定位。**明确不做**：不做 schema 版本号、不做表结构指纹、不做「结构变了就禁用编辑」的额外门禁。**遗留影响**：在一个持续 DDL 的表上，仍可能出现「按旧主键列集生成的 WHERE 未命中」（落到 `affected == 0` → 被既有 `affected == 1` 校验拒绝，属于**失败而非误写**）这一可观测后果；真正危险的方向是「旧主键列已不再是主键」而语句仍按它拼 WHERE，此时 W-3 的兜底也失效——该情形需由 DB 的表结构变更纪律（不在 DDL 期间批量编辑数据）承担，不在本批验收范围内。**若将来要求覆盖，须另立需求并单独裁定** |
