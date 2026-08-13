# DataZen 数据同步实施方案

**关联 PRD：** `docs/data-synchronization-prd.zh-CN.md`（V1.2）  
**相关：** [Data Transfer PRD](./data-transfer-prd.zh-CN.md)（本次不实施）· Schema Diff（已有）

**目标：** 对标 Navicat **Data Synchronization**。现有「整表覆盖拷贝」按 Transfer 处理：Sync 引擎替换为 Compare → Change Set → Execute；旧 DROP+INSERT 从 Sync 拆除，留给未来 Transfer。

**实施原则：**

> Driver 负责连接、查询流、事务、标识符与参数化 DML；Host 负责同步编排、Diff、Change Set、SQL Preview 与执行生命周期；Frontend 负责 Diff Workspace。方言细节测在驱动 crate，Host 只测编排。

**硬门闸（Compare 前）：** 同 dialect family · **表结构完全一致** · **双方相同 Primary Key**。不满足 → `INCOMPATIBLE`，提示 Structure Sync 或 Transfer，不得比较。

---

## 0. 与 Transfer / Structure Sync 的实施边界

| 产品 | 本次 | 引擎 |
|---|---|---|
| Data Synchronization | **做** | 新 `data_sync`：PK Diff + Change Set |
| Structure Synchronization | 已有，不改范围 | Schema Diff + Deploy |
| Data Transfer | **不做** | 未来独立窗口；可吸收 IR + 旧 `table_sync` |

禁止把字段映射、类型转换、目标建表、无 PK 灌数做进 Sync。INCOMPATIBLE UI 只引导，不在 Sync 里做半套 Transfer。

---

## 1. 实施范围

对应 PRD V1 / P0。同 dialect family **且通过结构+PK 门闸** 的增量同步：

- MySQL ↔ MySQL / MariaDB
- PostgreSQL ↔ PostgreSQL

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
Streaming Comparison（query_stream + keyset）
      ↓
Comparison Result
      ↓
Change Set
      ↓
SQL Preview（只读）
      ↓
Revalidate（read_only / 表 / PK）
      ↓
Transaction Execute
      ↓
Execution Result
      ↓
Compare again → 0 changes
```

V1 不实现：CDC、定时、ETL、字段映射、类型转换、无 PK、目标建表、跨类型拷贝、行 WHERE、可编辑 SQL、Mongo/Redis、Workflow/MCP。这些归 [Transfer PRD](./data-transfer-prd.zh-CN.md) 或已有 Schema Diff。

**Definition of Done：** Apply 后再 Compare，行差异为 0；产品内不可再走旧 DROP+INSERT 覆盖路径。

---

## 2. 现有实现：抛弃 vs 复用

当前 Data Sync 是 **DROP TABLE → CREATE → 批量 INSERT** 的克隆工具，可跨库 IR。与 Navicat 增量 Diff **模型冲突**，执行引擎应替换而非打补丁。

### 2.1 抛弃（产品内删除或停用）

| 模块 | 原因 |
|---|---|
| `src-tauri/src/commands/sync/table_sync.rs` 的 DROP/CREATE/INSERT | 覆盖拷贝，无 Change Set |
| `sync_table` / `sync_tables` 现语义 | 与新 Execute 冲突 |
| 跨库 IR 行拷贝路径 | 属于 Transfer，不进 Sync |
| `sync_tasks.json` 断点（`current_table_offset` + 跳过已 DROP 的表） | 与 Change Set 不兼容，不迁移 |
| `compare_table_data` 抽样 1000 行当「同步依据」 | 仅能预览，不能当 Change Set |
| 前端「选表 → 直接 Start Sync」主路径 | 违反 Compare → Review → Apply |

旧 IPC 可在同一替换 PR 中删除或改成返回明确错误，避免两套同步并存。

### 2.2 复用

| 能力 | 位置 | 用法 |
|---|---|---|
| 单例窗口 / kind | `windowKind` `data-sync`、`openDataSyncWindow()`、`DataSyncWindow.tsx` 壳 | 换成 Diff Workspace，不新增 window kind |
| 菜单 / 侧栏 / i18n | `menu:data-sync`、`action.dataSync`、10 语言 `sync.*` | 改文案与流程，补齐新 key |
| 连接管理 | ConnectionManager + 已保存连接 | Endpoint 只存 `connection_id` |
| 同族判定 | `sync/pairing.rs`、`src/lib/syncPairing.ts` | Sync 只开 Direct same-family；IR 留给 Schema Diff / 未来 Transfer |
| `get_tables` / `get_table_schema` | Driver | 映射与兼容性检查 |
| `query_stream` / `query_with_params` | Driver | Compare 流式读，禁止整表 `query()` |
| `build_update_sql` / `build_delete_sql` | Driver（Delete Row 已用） | Execute 参数化 UPDATE/DELETE |
| `quote_ident` / `format_sql_literal` | Driver | 标识符 + Preview 字面量 |
| `begin_transaction` / `commit` / `rollback` / `cancel_query` | Driver | 执行与取消 |
| Schema Diff 面板 | `SchemaDiffPanel` | INCOMPATIBLE 时展示列差异，引导去 Schema Diff / Transfer |
| E2E 夹具库 | `e2e/setup-sync-dbs.sh` | 夹具表必须结构一致且有 PK；改断言为 Diff 闭环 |
| `packages/driver-api/src/sync/` IR | Schema/DDL IR | Sync **不调用**拷贝；Schema Diff 与未来 Transfer 用 |

### 2.3 新模块边界

```text
packages/driver-api/src/sync/     → Schema IR（保留给结构同步，Data Sync V1 不调用拷贝）
src-tauri/src/data_sync/          → 新：Compare / ChangeSet / SQL / Execute
src-tauri/src/commands/data_sync.rs 或替换 commands/sync/* 语义
src/windows/data-sync/            → 改造现有窗口，不新建 kind
```

不要把 Data Sync 行模型塞进 `IRTable`。

### 2.4 Driver Trait：P0 不新增大批方法

P0 使用已有：

```text
get_tables / get_table_schema
query_with_params / query_stream
execute / begin_transaction / commit / rollback / cancel_query
quote_ident / format_sql_literal
build_update_sql / build_delete_sql
```

INSERT SQL 由 Host 按 Target schema + `quote_ident` + 参数生成；与 Preview 共用同一生成器。

P1 再评估可选 `DataSyncDialect`（keyset SQL、服务端 hash、值归一化）。禁止在 Host 写 `if mysql { ... } else if postgres`。

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

用户可以在 Compare 完成后长时间 Review。行 Diff 可落盘或分页回读，不要求一直占着 DB 连接；Execute 前必须重新拿到 Target 连接并 Revalidate。

---

## 4. 推荐代码目录

在**现有窗口与 IPC 包名上替换语义**，避免长期双轨：

```text
src-tauri/src/data_sync/          # 新领域（Compare / ChangeSet / SQL / Execute）
src-tauri/src/commands/sync/      # 替换现有 compare/sync_tables 实现，不要长期保留 DROP 路径
src/windows/data-sync/            # 改造 DataSyncWindow，拆子组件
src/commands/sync.ts              # 换新 command 形状
src/stores/dataSyncStore.ts       # 新增；只存任务/摘要/当前表页
packages/driver-api/src/sync/     # 保留给 Schema IR，V1 Data Sync 不走拷贝
```

Frontend 子组件可以放在 `src/windows/data-sync/`（与现有窗口一致），不必强行新建 `src/components/data-sync/`。

`packages/driver-api/src/data_sync/` 仅在 P1 需要跨 Driver 的 dialect hook 时再加；P0 不必新增 crate 模块。

---

## 5. Driver API

见 §2.4。P0 必须用 `query_stream` 做比较读取；必须用 `build_update_sql` / `build_delete_sql` 做执行（与 Delete Row 同一套 PK WHERE）。INSERT 由 Host 生成参数化语句。

执行通道必须是同步专用 IPC，**不要**把生成 SQL 交给 `execute_query`（否则 Safe Mode 的「UPDATE 必须有 WHERE」与只读连接语义会对不上：要么误杀合法 PK UPDATE，要么绕过 `read_only`）。

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

V1 **只有 Primary Key**。无 PK 或不一致 → `INCOMPATIBLE`，引导 Transfer / Structure Sync。不实现 Unique Index / Custom Columns（那会削弱「必须有 PK」的前提）。

```rust
enum MatchingStrategy {
    PrimaryKey,
}
```

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

禁止整表 `query()`（旧 `table_sync.rs` 即如此）。必须 `query_stream` + keyset chunk。

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

### 9.3 Schema 校验（硬门闸，PRD §9）

进入 Compare 前，**每一对映射表**必须：

- 双方表存在，且为基表（非视图）
- 同 dialect family
- **列名集合相同**（按列名对齐，允许物理顺序不同）
- **列类型等价、可空性相同**
- **PRIMARY KEY 存在且列集合+顺序相同**
- 不是「同一连接 + 同一库 + 映射到自身」

任一项失败 → `INCOMPATIBLE`，列出差异，**禁止**只同步列交集。提示：结构问题走 Schema Diff；异构/改列名/无 PK 走 Transfer。

索引/外键/触发器不一致：警告，不阻断。

PK 为 IDENTITY/SERIAL：INSERT 必须写入 PK 值（MySQL `SET IDENTITY_INSERT` 类能力若缺失则该表 INCOMPATIBLE 或 Execute 失败并说明）。生成列非 PK：不写入 SET/INSERT。

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

P0：Table/Task 级 revalidation + Target `read_only` 检查。Row-level conflict detection 为 P1，架构预留快照字段。

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

**表间顺序比行操作顺序更重要。**

V1 默认：

```text
按用户勾选的表顺序执行
同一张表内：INSERT → UPDATE → DELETE
```

不在 V1 解析 FK 图。外键失败给出明确错误（表、操作、row key、数据库信息），不静默跳过。

P1/P2 再按 FK 拓扑：INSERT 父→子，DELETE 子→父。

同表内 `INSERT → UPDATE → DELETE` 仍建议保留，降低自引用/唯一约束冲突概率。

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

替换现有 `commands/sync/*` 语义（删除 `sync_tables` 覆盖拷贝）。建议：

```text
sync_create_task
sync_validate_task
sync_start_compare
sync_cancel_compare
sync_get_comparison          # 表摘要
sync_get_table_diff          # 分页行 Diff，禁止一次返回全表
sync_update_selection
sync_generate_sql
sync_validate_execution
sync_execute
sync_cancel_execution
sync_get_history             # P1
sync_save_task               # P1
sync_delete_task
```

事件名避免与旧 `sync:progress` 语义混用：新事件建议 `data-sync:compare-progress` / `data-sync:execute-progress`（或同一事件加 `phase: compare|execute` 且 payload 含 Change Set 计数）。替换完成后删除旧监听。

新 command 必须写入 `src-tauri/capabilities/default.json`。

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
src/stores/dataSyncStore.ts
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

改造现有 `src/windows/data-sync/DataSyncWindow.tsx`，去掉「选表后直接同步」主按钮路径。右键菜单只用 Tauri 原生 Menu。10 语言同步更新 `sync.*`。

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

Matching: Primary Key only（无 PK 则不可同步）
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
35 Insert   171 Update   4 Delete   ·   8 tables unchanged
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

默认所有 INSERT/UPDATE 被选中；DELETE 仅在选项开启后可选，且默认不选。支持逐行勾选（P0）。

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

V1 **只提供只读 Preview**。禁止用户编辑 SQL 后执行（避免文本与 ChangeSet 不一致）。编辑执行为 P1。

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
- 旧版覆盖拷贝的 `current_table_offset` 任务（启动时忽略或删除 `sync_tasks.json` 不兼容条目）

SQL 是否持久化做成设置项；默认不保存完整 SQL。

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

行 Diff 存储建议（P0 必须有一种，避免全进内存）：

```text
{appData}/sync-compare/{taskId}/
  manifest.json          # 表摘要、计数
  {table}.changes        # 仅 INSERT/UPDATE/DELETE 行（可分页文件）
```

不持久化 UNCHANGED 行。窗口关闭后 Compare 结果可丢弃（P0）或按任务保留至 Execute（推荐，便于 Review 后回来再执行）。旧 `sync_tasks.json` 不复用该目录。

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

### 30.2 权限与只读 / Safe Mode

```text
Target.read_only = true  → 禁止 Execute（Compare 仍允许）
Source.read_only = true  → 允许 Compare
```

同步执行走专用 IPC，**不**经 `execute_query` / `sql_guard` 的「无 WHERE 则拦 UPDATE」规则（生成语句已带 PK WHERE）。专用通道仍必须：

- 拒绝 `read_only` Target
- 参数绑定
- 仅执行 Change Set 内已勾选行

Delete 与设置 `confirmOnDelete` 对齐。

Driver 无法预检权限时，执行失败要带 Table / Operation / Row Key / DB 错误。

触发器 / `ON DELETE CASCADE`：V1 在开启 Delete 时警告「实际删除可能多于 Change Set」。

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

### 31.3 Integration Test（落点必须遵守 AGENTS.md）

**方言 / 类型 / PK 行为**写在驱动 crate，不要进 Host：

```text
packages/drivers/mysql/tests/     data_sync_*.rs
packages/drivers/postgres/tests/  data_sync_*.rs
```

MariaDB 与 MySQL 同 family，P0 用同一套用例或显式 MariaDB 夹具各跑一次。

**Host**（`src-tauri` / `e2e/specs/`）只测编排：状态机、选择、Preview 与 Execute 一致、只读拦截、Cancel、事务结果、窗口流程。可用 PG/MySQL 当夹具，不断言方言 SQL。

复用 `e2e/setup-sync-dbs.sh`，**改写** `e2e/specs/data-sync-real.ts`：断言 Diff 闭环，删除对 DROP+INSERT 覆盖拷贝的依赖。

覆盖：Insert / Update / Delete / Rollback / Composite PK / NULL / Unicode / JSON / TEXT / Timestamp / 空表 / 空源 / 空目标 / Target read_only 拒绝 Execute。

Host UI：`e2e/specs/` 必须走完 Compare → 勾选 → Preview → Execute → 结果（现 SYNC UI 仍为 Partial，本功能落地时补齐）。

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

### Phase 0：替换边界确认

目标：冻结「抛弃 vs 复用」清单（见 §2），避免新旧引擎并存。

任务：

- 列出将删除的 `table_sync` / 旧 `sync_tables` / IR 拷贝调用点
- 确认窗口 kind、菜单、pairing 同族门闸保留
- 确认 `query_stream`、`build_update_sql` / `build_delete_sql`、事务、`cancel_query`
- 确认专用 IPC 与 `sql_guard` / `read_only` 的分工
- 确认 Compare 结果落盘目录与旧 `sync_tasks.json` 互不混用
- 确认 E2E 夹具可改为 Diff 闭环

产物：短 ADR（可写在本文件 §2，不必另开长文）。

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

改造 `DataSyncWindow`，不要新开窗口 kind：

```text
Source / Target / Swap
Options
Table Mapping
Summary
Table Diff
Row Diff
Change Selection
只读 SQL Preview
Execution Progress
Result
拆除「直接 Sync」覆盖入口
```

### Phase 6：PostgreSQL / MariaDB

同一 Comparison Engine + 各 Driver。MariaDB 与 MySQL 同 family，用现有 pairing 验收一遍即可。避免复制两套同步代码。

### Phase 7：拆除旧引擎 + History

- 删除或永久禁用 DROP+INSERT `sync_tables` 路径（P0 结束前产品内不可达）
- 忽略/清理不兼容的旧 `sync_tasks.json`
- P1：History / Saved Task / SQL 导出

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
- [ ] Schema 完全一致校验（列名/类型/可空/PK）
- [ ] Primary key matcher（无 PK 即拒绝）
- [ ] Keyset pagination
- [ ] Row comparator
- [ ] ChangeSet
- [ ] SQL generator
- [ ] Revalidation
- [ ] Transaction executor
- [ ] Progress events
- [ ] Cancel
- [ ] Error model

### Driver（测在各 crate，不是 Host）

- [ ] MySQL / MariaDB family：stream、事务、PK DML
- [ ] PostgreSQL：同上
- [ ] `quote_ident` / 参数绑定 / 大字段
- [ ] 值比较：NULL / 时间 / JSON（写在 `packages/drivers/{mysql,postgres}/`）

### Frontend

- [ ] 改造现有 Data Sync 窗口（去掉覆盖拷贝主路径）
- [ ] Source/Target + Swap + 同族校验 + 自同步禁止
- [ ] Options（Delete 默认关）
- [ ] Table Mapping + 结构完全一致/PK 硬门闸（失败 INCOMPATIBLE，引导 Schema Diff / Transfer）
- [ ] Compare progress + Cancel
- [ ] Summary（表 unchanged 与行 insert 分列）
- [ ] Table / Row / Cell Diff（分页 ≤500）
- [ ] Table / Operation / Row 选择
- [ ] 只读 SQL Preview
- [ ] Execution Progress / Result
- [ ] 10 语言 i18n；右键仅原生 Menu
- [ ] Host E2E 走通主路径

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

1. Delete 默认关闭；开启与执行两道确认。
2. 未选择的 Change 不得执行。
3. Preview 与执行同一生成器（参数化执行，Preview 仅展示）。
4. 不保存数据库密码。
5. Target `read_only` 禁止 Execute。
6. Target 发生明显变化时提示重新 Compare。
7. 产品内不可再触发旧 DROP+INSERT 覆盖同步。
8. 同一连接同一库自同步被拒绝。

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

V1：按勾选表顺序执行；同表内 INSERT → UPDATE → DELETE。FK 失败明确报错。

P1/P2：表级拓扑（INSERT 父→子，DELETE 子→父）。

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

两者可以共享 Driver 能力，但 Domain Model 应保持独立。`packages/driver-api/src/sync/` IR 不用于 V1 行拷贝。

### 38.6 不要保留两套同步引擎

P0 交付后，UI 与 IPC 不得再进入覆盖拷贝。旧任务文件不升级。

### 38.7 不要在 Host 写驱动方言测试

`mysql_*` / `postgres_*` 同步语义测在对应 driver crate。

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

V1 已去掉整表 IR 拷贝。未来跨库应是 **行级 Data IR**（值转换后再 INSERT/UPDATE），不是 DROP+重建。可借鉴现有 type IR，但不要复活旧 `table_sync`。

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
1. data_sync domain model + 状态机
2. 停用旧 sync_tables 覆盖路径（或先藏入口）
3. 复用连接 + 同族 pairing 门闸
4. PK matcher + keyset + query_stream 比较（MySQL）
5. ChangeSet + 参数化 SQL（复用 build_update/delete_sql）
6. 专用 Execute IPC（read_only / 事务 / Cancel）
7. Compare → Apply → Recompare = 0 差异（crate 集成测 + Host 编排测）
8. 改造 DataSyncWindow 最小 Diff UI
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

如果这条闭环稳定，再补 PostgreSQL、完整 Diff UI、MariaDB 验收，并**删除**旧 DROP+INSERT 代码路径。然后才是 History / Hash / 自定义匹配列。
