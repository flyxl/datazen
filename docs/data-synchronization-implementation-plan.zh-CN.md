# DataZen 数据同步实施方案

**关联 PRD：** `docs/data-synchronization-prd.zh-CN.md`

**目标：** 将数据同步 PRD 落地为 DataZen 可维护、可扩展、可测试的实现方案。

**实施原则：**

> Driver 能力负责数据库差异；Host 负责同步编排、Diff、Change Set、SQL Preview 和执行生命周期；Frontend 负责 Diff Workspace 与用户交互。

---

## 1. 实施范围

本方案对应 PRD V1/P0，第一阶段只实现同类型数据库的数据同步：

- MySQL → MySQL
- PostgreSQL → PostgreSQL

核心闭环：

```text
Source / Target
      ↓
Configuration
      ↓
Table Mapping
      ↓
Schema & Capability Check
      ↓
Streaming Comparison
      ↓
Comparison Result
      ↓
Change Set
      ↓
SQL Preview
      ↓
Revalidate
      ↓
Transaction Execute
      ↓
Execution Result
```

V1 不实现：

- CDC / Replication
- 定时同步
- Schema Structure Sync
- ETL / 字段转换
- 不同数据库类型之间的数据迁移
- MongoDB Document Sync

---

## 2. 当前代码基础与复用策略

当前仓库已经存在数据库 Driver 抽象，可以直接作为数据同步的基础设施：

- `packages/driver-api/src/traits.rs`
- `packages/driver-api/src/types.rs`
- `packages/driver-api/src/query_stream/`
- `packages/driver-api/src/sync/`
- `src-tauri/src/`
- `src/commands/`
- `src/components/`
- `src/stores/`

`DatabaseDriver` 已提供：

- `connect`
- `disconnect`
- `get_databases`
- `get_tables`
- `get_table_schema`
- `query`
- `query_with_params`
- `query_stream`
- `execute`
- `begin_transaction`
- `commit`
- `rollback`
- `cancel_query`
- `quote_ident`
- `format_sql_literal`
- `build_update_sql`
- `build_delete_sql`

因此数据同步**不应该重新设计一套数据库连接和 SQL 执行体系**。

### 2.1 `packages/driver-api/src/sync/` 的定位

当前 `sync` 模块主要提供跨数据库的 Schema/DDL Intermediate Representation：

```text
IRTable
IRColumn
IRType
IRDefault
SyncSourceAdapter
SyncTargetAdapter
```

它可以复用其中的类型和 Driver Adapter 思路，但不建议直接把当前 `IRTable` 当成数据同步的 Row Model。

原因：

```text
Schema Sync
    → Table / Column / Type

Data Sync
    → Row / Cell / Key / Operation
```

两者生命周期、数据量和执行模型完全不同。

因此建议新增独立的 `data_sync` 模块，避免把 Schema Sync 和 Data Sync 强耦合。

---

## 3. 总体架构

```text
┌──────────────────────────────────────────────┐
│                  React UI                    │
│                                              │
│ Sync Config → Table Mapping → Diff Workspace │
│                         → SQL Preview         │
│                         → Execution Result    │
└──────────────────────┬───────────────────────┘
                       │ Tauri Commands / Events
┌──────────────────────▼───────────────────────┐
│                Sync Application              │
│                                              │
│ SyncTaskService                              │
│ ComparisonService                            │
│ MappingService                                │
│ ChangeSetService                              │
│ SqlGenerationService                          │
│ ExecutionService                              │
│ HistoryService                                │
└──────────────────────┬───────────────────────┘
                       │
┌──────────────────────▼───────────────────────┐
│               Driver Abstraction              │
│                                              │
│ DatabaseDriver                               │
│ Query / QueryStream / Transaction            │
│ TableSchema / ColumnSchema / Value           │
└──────────────────────┬───────────────────────┘
                       │
             ┌─────────┴─────────┐
             ▼                   ▼
        Source Driver        Target Driver
             │                   │
             ▼                   ▼
          Source DB            Target DB
```

核心原则：**比较阶段和执行阶段解耦。**

```text
ComparisonResult
      ↓
ChangeSet
      ↓
SQL
      ↓
Execution
```

用户可以在 Compare 完成后长时间 Review，而不需要保持数据库连接或重新执行 Compare。

---

## 4. 推荐代码目录

最终建议形成如下结构：

```text
packages/driver-api/
└── src/
    ├── traits.rs
    ├── types.rs
    └── data_sync/
        ├── mod.rs
        ├── capability.rs
        ├── dialect.rs
        └── types.rs

src-tauri/src/
├── data_sync/
│   ├── mod.rs
│   ├── service.rs
│   ├── task.rs
│   ├── mapping.rs
│   ├── comparison.rs
│   ├── matcher.rs
│   ├── changeset.rs
│   ├── sql.rs
│   ├── execution.rs
│   ├── validation.rs
│   ├── pagination.rs
│   ├── history.rs
│   └── error.rs
├── commands/
│   └── data_sync.rs
└── ...

src/
├── components/
│   └── data-sync/
│       ├── DataSyncWorkspace.tsx
│       ├── SyncSourceTarget.tsx
│       ├── SyncOptions.tsx
│       ├── TableMapping.tsx
│       ├── ComparisonSummary.tsx
│       ├── TableDiff.tsx
│       ├── RowDiff.tsx
│       ├── ChangeSetToolbar.tsx
│       ├── SqlPreview.tsx
│       ├── ExecutionProgress.tsx
│       └── ExecutionResult.tsx
├── stores/
│   └── data-sync.ts
├── types/
│   └── data-sync.ts
└── commands/
    └── data-sync.ts
```

具体路径以现有模块组织方式为准，但职责边界应保持不变。

---

## 5. Driver API 设计

### 5.1 第一阶段尽量不修改 `DatabaseDriver`

现有 `DatabaseDriver` 已经可以完成 P0：

```text
get_table_schema
query_with_params
query_stream
execute
begin_transaction
commit
rollback
quote_ident
format_sql_literal
```

因此第一阶段不应为了数据同步而向 `DatabaseDriver` 增加大量同步专用方法。

同步 Host 可以通过标准 SQL 完成：

- Row 查询
- COUNT
- Primary Key 查询
- Insert
- Update
- Delete

这样可以降低插件 API breaking change 风险。

### 5.2 后续可增加可选能力

当 MySQL/PostgreSQL 实现成熟后，再评估增加：

```rust
trait DataSyncDialect {
    fn build_keyset_query(...);
    fn build_insert_sql(...);
    fn build_update_sql(...);
    fn build_delete_sql(...);
    fn normalize_value(...);
    fn compare_value(...);
}
```

这些能力应该作为**可选扩展能力**，而不是把所有同步逻辑塞进 Driver Trait。

---

## 6. 核心数据模型

### 6.1 SyncTask

```rust
struct SyncTask {
    id: String,
    source: Endpoint,
    target: Endpoint,
    options: SyncOptions,
    mappings: Vec<TableMapping>,
}
```

### 6.2 Endpoint

```rust
struct Endpoint {
    connection_id: String,
    database: String,
    schema: Option<String>,
}
```

不保存密码。密码始终从现有连接凭据系统获取。

### 6.3 SyncOptions

```rust
struct SyncOptions {
    insert: bool,
    update: bool,
    delete: bool,
    matching_strategy: MatchingStrategy,
    batch_size: u32,
    large_value_mode: LargeValueMode,
}
```

默认：

```text
insert = true
update = true
delete = false
batch_size = 1000
```

### 6.4 MatchingStrategy

```rust
enum MatchingStrategy {
    PrimaryKey,
    UniqueIndex,
    CustomColumns(Vec<String>),
}
```

P0 只实现 `PrimaryKey`。

### 6.5 TableMapping

```rust
struct TableMapping {
    source_table: String,
    target_table: String,
    enabled: bool,
    matching_columns: Vec<ColumnMapping>,
}
```

### 6.6 RowChange

```rust
enum ChangeOperation {
    Insert,
    Update,
    Delete,
    Unchanged,
}

struct RowChange {
    operation: ChangeOperation,
    key: Vec<CellValue>,
    source_row: Option<Row>,
    target_row: Option<Row>,
    changed_columns: Vec<String>,
    selected: bool,
}
```

`source_row` 和 `target_row` 可以在内存中保存，也可以针对大表改为按需加载。

---

## 7. Comparison 算法

### 7.1 基本算法

对于有 Primary Key 的表：

```text
Source rows
     ↓
按 PK 排序/分页
     ↓
读取 Target 对应范围
     ↓
PK Map
     ↓
逐 Row 比较
     ↓
INSERT / UPDATE / UNCHANGED / DELETE
```

逻辑：

```text
Source ∩ Target
    → compare cells

Source - Target
    → INSERT

Target - Source
    → DELETE
```

### 7.2 不建议全表加载

禁止：

```rust
let source_rows = query_all(...);
let target_rows = query_all(...);
```

大表会导致：

- 客户端内存暴涨
- IPC 数据量过大
- UI 卡顿
- Compare 时间不可控

必须采用 Chunk：

```text
1000 rows / batch
```

### 7.3 第一阶段分页

优先使用 Keyset Pagination：

```sql
SELECT ...
FROM users
WHERE id > ?
ORDER BY id
LIMIT 1000
```

不优先使用：

```sql
OFFSET 1000000 LIMIT 1000
```

原因是大 OFFSET 在很多数据库上性能较差。

### 7.4 Composite Primary Key

必须支持：

```text
PRIMARY KEY (tenant_id, user_id)
```

比较 Key 使用有序 Tuple：

```text
(tenant_id, user_id)
```

生成 WHERE：

```sql
WHERE tenant_id = ?
  AND user_id = ?
```

### 7.5 NULL 比较

不能使用简单的字符串比较。

必须区分：

```text
NULL
""
0
false
```

逻辑上：

```text
NULL == NULL       true
NULL == empty      false
0 == "0"           false
false == 0         false
```

除非 Driver 明确提供类型归一化规则。

---

## 8. Row Compare 策略

V1 推荐采用两阶段比较：

```text
第一阶段：Key + Lightweight Values
第二阶段：只对疑似变化 Row 做完整比较
```

对于普通字段，可以直接比较。

对于 TEXT / JSON / BLOB 等大字段：

```text
Full
Hash
Ignore
```

V1 可先实现 Full，P1 再增加 Hash。

### 8.1 Hash 不应在客户端计算

对于大字段，不应该：

```text
DB → full data → client → hash
```

优先：

```text
DB → server-side hash
```

例如 PostgreSQL 可以利用数据库函数；MySQL 也可使用对应 hash 函数。

具体函数由 Driver Dialect 决定。

---

## 9. Table Mapping 实现

### 9.1 自动匹配

Source / Target 获取：

```text
get_tables()
```

建立：

```text
Map<table_name, TableInfo>
```

按标准化名称匹配：

```text
users → users
orders → orders
```

### 9.2 Mapping 状态

```text
MATCHED
UNMAPPED_SOURCE
UNMAPPED_TARGET
DISABLED
INCOMPATIBLE
```

### 9.3 Schema 校验

进入 Compare 前检查：

- Source table 存在
- Target table 存在
- Column 数量/名称
- Matching columns 存在
- Matching columns 类型兼容
- Source/Target Driver Type 相同

V1 如果列结构存在不兼容，默认阻止 Compare，而不是尝试隐式转换。

---

## 10. Change Set

ComparisonResult 不直接进入 Execute。

中间增加：

```text
ComparisonResult
       ↓
ChangeSet
```

ChangeSet 是用户选择后的最终变更集合。

例如：

```text
Comparison Result

users
  INSERT 12
  UPDATE 35
  DELETE 0

orders
  INSERT 23
  UPDATE 128
  DELETE 4
```

用户取消：

```text
orders DELETE
```

最终：

```text
ChangeSet
  users: INSERT + UPDATE
  orders: INSERT + UPDATE
```

执行阶段只接受 ChangeSet，不直接读取 ComparisonResult。

---

## 11. SQL Generation

SQL Generation 必须是独立服务：

```text
SqlGenerationService
```

输入：

```text
TableMapping
RowChange
Target Driver
```

输出：

```text
SqlStatement[]
```

建议模型：

```rust
struct SqlStatement {
    table: String,
    operation: ChangeOperation,
    sql: String,
    parameters: Vec<Value>,
    row_key: Vec<Value>,
}
```

### 11.1 不建议直接拼接用户数据

优先生成：

```sql
UPDATE users
SET age = ?
WHERE id = ?
```

然后：

```text
parameters = [20, 1]
```

只有 SQL Preview 展示时再生成可读 SQL。

这样可以降低：

- SQL Injection
- 字符串转义错误
- JSON 转义错误
- BLOB 转义错误

### 11.2 SQL Preview 与实际执行 SQL 必须一致

必须保证：

```text
Preview SQL
      =
Execution SQL Template + Parameters
```

不能 Preview 一套 SQL，Execute 时重新生成另一套逻辑。

---

## 12. INSERT

生成：

```sql
INSERT INTO "users" ("id", "name", "age")
VALUES (?, ?, ?)
```

必须按照目标表 Column 顺序稳定生成。

如果目标存在自动生成字段，应根据 schema 决定是否写入该字段。

---

## 13. UPDATE

只更新发生变化的字段，不建议整行覆盖：

```sql
UPDATE users
SET age = ?
WHERE id = ?
```

而不是：

```sql
UPDATE users
SET id = ?, name = ?, age = ?, ...
WHERE id = ?
```

原因：

- 减少锁定范围
- 减少日志量
- 避免覆盖 Target 独立字段
- SQL 更容易 Review

Primary Key 默认禁止进入 SET。

---

## 14. DELETE

DELETE 默认关闭。

生成：

```sql
DELETE FROM users
WHERE id = ?
```

如果是 Composite PK：

```sql
DELETE FROM users
WHERE tenant_id = ?
  AND user_id = ?
```

执行前必须再次确认 Delete ChangeSet。

---

## 15. Execute 前 Revalidation

这是整个功能最重要的安全机制之一。

Compare 完成后，Target 可能被其他用户修改。

因此 Execute 前重新验证：

```text
Connection
Database
Schema
Table
Matching Key
Target Row
```

### 15.1 Row-level Conflict Detection

对于 UPDATE / DELETE，ChangeSet 中保存 Compare 时的 Target Row 快照。

执行前重新读取对应 Key：

```text
Compare Target Row
        ↓
Current Target Row
        ↓
Compare
```

如果不同：

```text
CONFLICT
```

默认停止执行该 Row。

### 15.2 冲突 UI

显示：

```text
Target changed since comparison

Compared:
age = 20

Current:
age = 25

[Recompare] [Skip] [Execute Anyway]
```

P0 可以先实现 Table/Task 级 revalidation；Row-level conflict detection 可作为 P1，但架构必须预留。

---

## 16. Transaction Execution

ExecutionService：

```text
begin_transaction()
        ↓
execute ChangeSet
        ↓
commit()
```

失败：

```text
rollback()
```

### 16.1 执行顺序

建议：

```text
INSERT
UPDATE
DELETE
```

原因是减少因外键依赖导致的失败概率。

后续可以根据 FK 图进行拓扑排序。

### 16.2 Batch Execution

不要每条 Row 创建一次连接。

连接模型：

```text
Source Connection
Target Connection
```

整个 Task 复用连接。

在事务中批量执行。

---

## 17. Transaction 不支持时

如果 Driver 不支持 Transaction：

```text
Warning:
Target database does not support transaction rollback.
Partial changes may remain if execution fails.
```

用户必须明确确认。

Execution Result 状态：

```text
COMPLETED
COMPLETED_WITH_ERRORS
FAILED
ROLLED_BACK
```

---

## 18. Backend State Machine

实现：

```text
Draft
 ↓
Configured
 ↓
Comparing
 ↓
Compared
 ↓
Reviewing
 ↓
GeneratingSql
 ↓
ReadyToExecute
 ↓
Revalidating
 ↓
Executing
 ↓
Completed
```

错误：

```text
CompareFailed
ValidationFailed
ExecutionFailed
Cancelled
```

不允许非法跳转，例如：

```text
Comparing → Execute
```

必须经过：

```text
Compared → Reviewing → ReadyToExecute → Revalidating
```

---

## 19. Tauri Command API

建议暴露以下 Command：

```text
sync_create_task
sync_validate_task
sync_start_compare
sync_cancel_compare
sync_get_comparison
sync_update_selection
sync_generate_sql
sync_validate_execution
sync_execute
sync_cancel_execution
sync_get_history
sync_save_task
sync_delete_task
```

### 19.1 `sync_create_task`

输入：

```json
{
  "source": {
    "connectionId": "...",
    "database": "db1",
    "schema": null
  },
  "target": {
    "connectionId": "...",
    "database": "db2",
    "schema": null
  },
  "options": {
    "insert": true,
    "update": true,
    "delete": false
  }
}
```

返回：

```json
{
  "taskId": "sync-..."
}
```

### 19.2 `sync_start_compare`

返回 taskId，并通过 Tauri Event 推送进度。

### 19.3 Progress Event

```json
{
  "taskId": "...",
  "table": "orders",
  "processed": 10000,
  "total": 50000,
  "inserts": 10,
  "updates": 30,
  "deletes": 2
}
```

### 19.4 Execution Event

```json
{
  "taskId": "...",
  "processed": 1285,
  "succeeded": 1281,
  "failed": 4
}
```

---

## 20. Frontend Store

新增：

```text
src/stores/data-sync.ts
```

Store 状态：

```text
task
source
 target
options
mappings
comparison
changeSet
sqlPreview
execution
error
```

状态不要直接等同 Backend State Machine，可以保留 UI 状态：

```text
configuring
comparing
reviewing
previewing
executing
completed
```

### 20.1 Store 原则

不要把全部 Row Diff 永久塞进 React global store。

对于大表：

```text
Table list → Store
Current table → Local state / paged cache
```

避免一次性渲染几十万 Row。

---

## 21. UI 实现

### 21.1 Source / Target

```text
┌──────────────────────────────────────┐
│ Source               Target          │
│ Connection ▼         Connection ▼    │
│ Database ▼           Database ▼      │
│                                      │
│              [Swap]                  │
└──────────────────────────────────────┘
```

### 21.2 Options

```text
☑ Insert
☑ Update
☐ Delete

Matching:
● Primary Key
○ Unique Index
○ Custom Columns
```

### 21.3 Table Mapping

```text
Source             Target            Status
users       →      users             ✓
orders      →      orders            ✓
customers   →      clients           ⚠
products    →      —                 —
```

### 21.4 Comparison Workspace

顶部 Summary：

```text
35 Insert   171 Update   4 Delete   8 Unchanged
```

左侧：

```text
Tables
├── users
├── orders
├── products
└── categories
```

中间：

```text
Source              Target
--------------------------------
age 20              age 21
name Tom             name Tom
```

底部：

```text
[Generate SQL] [Recompare] [Sync]
```

### 21.5 Row Selection

支持：

```text
Table
  ↓
Operation
  ↓
Row
```

默认所有 INSERT/UPDATE 被选中，DELETE 未开启时不可选。

---

## 22. Diff Rendering

Diff UI 必须区分：

```text
INSERT  → Target 不存在
UPDATE  → Cell changed
DELETE  → Source 不存在
UNCHANGED
```

Update Cell 显示：

```text
Source: 20
Target: 21
```

而不是只显示：

```text
age changed
```

用户必须能看到实际变化。

---

## 23. SQL Preview UI

SQL Preview 分三层：

```text
Summary
   ↓
SQL List
   ↓
Selected SQL Detail
```

支持：

- Search
- Filter INSERT / UPDATE / DELETE
- Copy
- Save
- Execute

SQL 编辑必须明确标识：

> Editing SQL changes the execution payload.

如果 V1 暂不支持编辑 SQL，建议先提供只读 Preview，避免出现“用户编辑了 SQL，但 ChangeSet 与 SQL 不一致”的复杂状态。

---

## 24. Execution UI

```text
Synchronizing...

users      ████████████████ 100%
orders     ██████████░░░░░░  62%

Processed  1,285
Succeeded  1,281
Failed     4
```

允许：

```text
Cancel
```

Cancel 后：

- 如果在事务中：Rollback
- 如果不支持事务：停止后标记 Partial Apply

---

## 25. History

建议使用现有本地持久化机制，不新增独立数据库。

History 保存：

```text
Task ID
Source
Target
Options
Mapping Summary
Comparison Summary
Execution Summary
Started At
Finished At
Duration
Status
```

不要保存：

- 密码
- 完整敏感数据快照

SQL 是否持久化可以做成设置项；默认建议不保存完整 SQL，以降低敏感数据泄漏风险。

---

## 26. Cancel / Error Handling

### Compare Cancel

```text
Running
  ↓
Cancel Requested
  ↓
Cancel Query
  ↓
Cancelled
```

必须调用 Driver 的 `cancel_query`，而不是仅停止 UI 等待。

### Execution Error

每条错误至少记录：

```text
Table
Operation
Row Key
SQL / SQL Template
Database Error
```

例如：

```text
orders / UPDATE / id=1024
ERROR: duplicate key value violates unique constraint
```

---

## 27. 并发模型

一个 Sync Task：

```text
1 Source connection
1 Target connection
```

Table 级并发不建议 V1 默认开启。

原因：

- Target lock 竞争
- Transaction 行为复杂
- 外键依赖
- 数据库负载不可控

V1：

```text
Table sequential
Row batch execution
```

后续可以增加：

```text
max_concurrency = 2/4/8
```

---

## 28. 内存与性能目标

P0 目标：

| 指标 | 目标 |
|---|---:|
| Default batch | 1000 rows |
| UI Row rendering | ≤ 500 rows/page |
| Compare memory | 不随全表行数线性增长 |
| Cancel response | ≤ 2s（正常网络） |
| SQL preview | 支持分页/懒加载 |

对于百万级数据：

```text
不要求一次加载百万行
```

而是：

```text
Chunk → Compare → Persist/Stream Result
```

---

## 29. 大表优化路线

### P0

Keyset Pagination：

```sql
WHERE pk > ?
ORDER BY pk
LIMIT ?
```

### P1

增加 Server-side Hash：

```text
Table
 ↓
PK + Row Hash
 ↓
Changed Keys
 ↓
Full Row Compare
```

### P2

增加：

- Parallel Chunk Compare
- Adaptive Batch Size
- Sampling / Fast Compare
- Database-native checksum

---

## 30. 安全设计

### 30.1 Delete Protection

三层保护：

```text
Delete option 默认关闭
        ↓
开启 Delete 风险确认
        ↓
Execute 前再次确认
```

### 30.2 权限

Execute 前检查：

```text
INSERT
UPDATE
DELETE
```

Driver 如果无法提前判断权限，则在执行前通过轻量权限验证或实际错误反馈。

### 30.3 Credential

SyncTask 只保存：

```text
connection_id
```

不保存：

```text
password
access_token
private_key
```

### 30.4 SQL Injection

所有真实执行 SQL 使用参数绑定。

Identifier 使用：

```rust
driver.quote_ident()
```

绝不把用户输入直接拼入：

```text
WHERE
VALUES
SET
```

---

## 31. 测试方案

### 31.1 Unit Test

重点测试：

```text
PK matching
Composite PK
NULL comparison
INSERT detection
UPDATE detection
DELETE detection
UNCHANGED detection
Column diff
SQL generation
Identifier quoting
Value formatting
```

### 31.2 Property Test

对于相同数据：

```text
compare(A, A) = unchanged
```

对于 Apply 后：

```text
apply(A → B)
then
compare(A, B)

must produce no changes
```

这是数据同步最重要的闭环测试。

### 31.3 Integration Test

至少：

```text
MySQL source → MySQL target
PostgreSQL source → PostgreSQL target
```

测试：

- Insert
- Update
- Delete
- Transaction rollback
- Composite PK
- NULL
- Unicode
- JSON
- Large TEXT
- Timestamp
- Empty table
- Empty source
- Empty target

### 31.4 Failure Test

模拟：

```text
Source disconnect
Target disconnect
Query timeout
Permission denied
Duplicate key
Foreign key violation
Transaction failure
Cancel
Target changed after comparison
```

---

## 32. 测试数据库

建议提供 Docker Compose：

```text
mysql-source
mysql-target
postgres-source
postgres-target
```

初始化：

```text
users
orders
products
```

并提供固定 fixture：

```text
source.sql
 target.sql
```

方便 CI 做端到端同步测试。

---

## 33. 实施阶段

### Phase 0：架构确认

目标：明确现有 Driver/Command/Connection 复用边界。

任务：

- 确认 connection manager
- 确认 Tauri command 注册方式
- 确认 Event 推送机制
- 确认现有 TableSchema / Value 模型
- 确认 MySQL/PostgreSQL Driver 实现
- 确认现有测试基础设施

产物：

```text
Architecture Decision Record
```

### Phase 1：Backend Domain Model

实现：

```text
SyncTask
Endpoint
SyncOptions
TableMapping
ComparisonResult
RowChange
ChangeSet
ExecutionResult
```

同时实现状态机。

### Phase 2：Comparison Engine

实现：

```text
Table discovery
Schema validation
PK matching
Keyset pagination
Row comparison
Change detection
Progress events
Cancel
```

先完成 MySQL。

### Phase 3：SQL Generation

实现：

```text
Insert SQL
Update SQL
Delete SQL
Parameter binding
SQL Preview
```

重点完成 SQL 与 ChangeSet 的一一对应关系。

### Phase 4：Execution Engine

实现：

```text
Revalidation
Transaction
Batch execute
Rollback
Cancel
Execution result
```

### Phase 5：Frontend Diff Workspace

实现：

```text
Source / Target
Options
Table Mapping
Summary
Table Diff
Row Diff
Change Selection
SQL Preview
Execution Progress
Result
```

### Phase 6：PostgreSQL

在 MySQL 跑通后，将数据库相关差异限制在 Dialect/Driver 层。

目标：

```text
同一 Comparison Engine
+
MySQL Driver
PostgreSQL Driver
```

避免复制两套同步代码。

### Phase 7：History / Saved Task

最后增加：

- History
- Saved Task
- SQL export
- Sync task duplication

---

## 34. 推荐开发顺序

不要一次实现完整 UI。

推荐 Vertical Slice：

```text
Step 1
MySQL → MySQL

Source / Target
      ↓
Compare
      ↓
Detect INSERT / UPDATE / DELETE
      ↓
Generate SQL
      ↓
Execute
      ↓
Compare Again
      ↓
No Difference
```

这条链路跑通后再逐步增加：

```text
Table Mapping
Composite PK
Diff UI
Selection
Transaction
Progress
PostgreSQL
History
```

这样可以尽早验证最核心的技术风险。

---

## 35. MVP 拆分为开发任务

### Backend

- [ ] `data_sync` domain model
- [ ] Task state machine
- [ ] Endpoint validation
- [ ] Table mapping
- [ ] Schema compatibility
- [ ] Primary key matcher
- [ ] Keyset pagination
- [ ] Row comparator
- [ ] ChangeSet
- [ ] SQL generator
- [ ] Revalidation
- [ ] Transaction executor
- [ ] Progress events
- [ ] Cancel
- [ ] Error model

### Driver

- [ ] MySQL sync compatibility
- [ ] PostgreSQL sync compatibility
- [ ] Identifier quoting validation
- [ ] Parameter binding validation
- [ ] Transaction validation
- [ ] Large value handling

### Frontend

- [ ] Data Sync entry
- [ ] Source/Target selector
- [ ] Options
- [ ] Table Mapping
- [ ] Compare progress
- [ ] Summary
- [ ] Table Diff
- [ ] Row Diff
- [ ] Selection
- [ ] SQL Preview
- [ ] Execution Progress
- [ ] Execution Result

### Test

- [ ] Unit tests
- [ ] SQL generation tests
- [ ] MySQL integration tests
- [ ] PostgreSQL integration tests
- [ ] Rollback tests
- [ ] Cancel tests
- [ ] Recompare tests
- [ ] Large data tests

---

## 36. Acceptance Criteria

### Functional

1. Source/Target 可以选择连接、数据库和 Schema。
2. Source/Target 可以 Swap。
3. 可以自动匹配同名 Table。
4. 可以查看 Table Mapping。
5. 可以使用 Primary Key 比较数据。
6. 能正确识别 Insert / Update / Delete / Unchanged。
7. 可以按 Table、Operation、Row 选择 Change。
8. 可以生成 SQL Preview。
9. Execute 前进行 Target Validation。
10. Transaction 成功时 Commit。
11. Transaction 失败时 Rollback。
12. 可以显示执行结果。
13. Compare 和 Execute 都可以 Cancel。

### Safety

1. Delete 默认关闭。
2. 未选择的 Change 不得执行。
3. Preview SQL 与实际执行 SQL 一致。
4. 不保存数据库密码。
5. Target 发生明显变化时提示重新 Compare。

### Performance

1. 不全量加载大表。
2. 使用 Batch / Keyset Pagination。
3. UI 不直接渲染全部 Diff Row。
4. Compare 支持进度反馈。
5. Cancel 不应只停止前端等待，而应尝试取消数据库查询。

---

## 37. 关键技术风险

### 风险 1：Driver 能力不一致

解决：

```text
Capability Detection
        ↓
Feature Gate
```

例如：

```text
Transaction supported = false
→ Warning
```

### 风险 2：数据量过大

解决：

```text
Keyset Pagination
Streaming
Chunk
Lazy Diff
```

### 风险 3：Compare 与 Execute 之间数据变化

解决：

```text
Revalidation
Conflict Detection
Recompare
```

### 风险 4：SQL Dialect

解决：

```text
DatabaseDriver
 + optional DataSyncDialect
```

不要在 Sync Service 中写：

```rust
if mysql { ... }
else if postgres { ... }
```

### 风险 5：外键依赖

V1 采用：

```text
INSERT → UPDATE → DELETE
```

遇到复杂 FK 场景先给出明确错误。

P1/P2 再根据 FK 图优化执行顺序。

---

## 38. 不建议的实现方式

### 38.1 不要直接复制 Navicat Wizard

产品 UX 应保持 Diff Workspace，而不是：

```text
Next
Next
Next
Next
```

### 38.2 不要让 Frontend 直接执行 SQL

错误架构：

```text
React
  ↓
SQL
  ↓
Driver
```

正确：

```text
React
  ↓
ChangeSet
  ↓
Tauri
  ↓
ExecutionService
  ↓
Driver
```

### 38.3 不要把整个 Comparison Result 放进 React State

大表会造成严重内存和渲染问题。

### 38.4 不要用字符串拼接生成实际执行 SQL

实际执行必须参数化。

### 38.5 不要把 Data Sync 和 Schema Sync 混成一个模块

两者可以共享 Driver 能力，但 Domain Model 应保持独立。

---

## 39. 后续扩展设计

### Cross Database

未来：

```text
Source Driver
    ↓
Data IR
    ↓
Target Driver
```

这里可以复用现有 `packages/driver-api/src/sync/` 的 IR 思路。

### Data Transformation

未来增加：

```text
Column Mapping
Value Transformer
Filter
```

例如：

```text
source.email
    ↓
lowercase()
    ↓
target.email
```

不应在 V1 提前实现。

### Scheduled Sync

后续可以把：

```text
SyncTask
```

作为可持久化执行单元，再增加：

```text
Cron
Schedule
CLI
```

### CLI

最终可以复用同一个 Backend：

```text
datazen sync --task xxx
```

因此 Backend 不应依赖 React UI。

---

## 40. 最终架构结论

数据同步的核心不是“生成几条 INSERT/UPDATE/DELETE SQL”，而是建立一个稳定的数据变更流水线：

```text
             ┌──────────────┐
             │ Source DB    │
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │ Comparison   │
             │ Engine       │
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │ Comparison   │
             │ Result       │
             └──────┬───────┘
                    │
             User Review
                    │
                    ▼
             ┌──────────────┐
             │ Change Set   │
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │ SQL Generator│
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │ Revalidation │
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │ Transaction  │
             │ Executor     │
             └──────┬───────┘
                    │
                    ▼
             ┌──────────────┐
             │ Target DB    │
             └──────────────┘
```

最终代码边界应满足：

```text
Driver
  = 数据库能力

Data Sync Backend
  = 比较、变更集合、执行编排

Frontend
  = Diff Review / UX

History
  = 本地任务与结果
```

这样既能满足当前 MySQL/PostgreSQL 同类型同步，又为后续跨数据库同步、数据转换、定时任务和 CLI 保留扩展空间。

---

## 41. 第一阶段建议直接开工的任务

按照风险优先级，第一批实现建议只做以下内容：

```text
1. data_sync domain model
2. MySQL source/target connection
3. Table schema discovery
4. Primary Key matcher
5. Keyset pagination
6. Row comparator
7. INSERT/UPDATE/DELETE ChangeSet
8. Parameterized SQL generator
9. Transaction executor
10. Compare → Apply → Recompare integration test
```

其中第 10 项作为第一阶段的 **Definition of Done**：

```text
Source
  ↓
Compare
  ↓
Review
  ↓
Apply
  ↓
Compare Again
  ↓
0 Changes
```

如果这条闭环稳定，再开始投入大量 UI 和高级优化工作。
