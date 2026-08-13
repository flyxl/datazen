# DataZen 数据同步功能 PRD

**产品：** DataZen  
**功能：** Data Synchronization / 数据同步  
**版本：** V1.0  
**状态：** Draft  
**参考：** Navicat Data Synchronization

## 1. 产品背景

数据库开发、测试和运维过程中，经常需要在两个数据库之间同步数据，例如开发环境到测试环境、本地数据库到远程数据库、测试数据库之间的数据复制。传统方式通常依赖手工 SQL、mysqldump 或临时脚本，难以直观看到差异，也难以精确控制同步范围，尤其是删除操作风险较高。

DataZen 数据同步的核心理念是：

> **Compare → Review → Generate Changes → Execute**

而不是简单的 Source → Copy → Target。

## 2. 产品目标

用户可以通过可视化界面完成：

1. 选择 Source Database
2. 选择 Target Database
3. 配置同步策略
4. 自动匹配 Table
5. 比较数据差异
6. 查看 Table / Row / Cell 级差异
7. 选择需要同步的变更
8. 生成 SQL
9. 预览 SQL
10. 执行同步
11. 查看执行结果

目标是让用户无需手工编写 SQL，即可安全完成两个数据库之间的数据同步。

## 3. 非目标

V1 不包含：

- 实时 CDC / Replication
- Schema / Structure Synchronization
- 复杂 ETL 与数据转换
- 不同数据库类型之间的迁移

V1 重点支持同数据库类型之间的数据同步，例如 MySQL → MySQL、PostgreSQL → PostgreSQL。

## 4. 核心概念

```text
Sync Task
├── Source
├── Target
├── Sync Options
├── Table Mapping
├── Comparison Result
├── Change Set
└── Execution Result
```

## 5. 用户流程

```text
进入 Data Synchronization
          ↓
选择 Source / Target
          ↓
配置 Synchronization Options
          ↓
Table Mapping
          ↓
Start Comparison
          ↓
Comparison Result
          ↓
Table Diff
          ↓
Row Diff
          ↓
选择需要同步的 Change
          ↓
Generate SQL
          ↓
SQL Preview
          ↓
Execute
          ↓
Execution Result
```

## 6. 功能入口

主导航可提供：

- Database → Data Synchronization
- Connection / Database / Table 右键 → Data Synchronization...

## 7. Source / Target

同步任务必须选择 Source 和 Target：

- Connection
- Database
- Schema（数据库支持时）

默认模型：

```text
Source DB  ─────────→  Target DB
```

提供 **Swap** 操作交换 Source 与 Target，并重新校验连接、数据库、Schema 和权限。

V1 要求 Source / Target Driver Type Compatible；不同数据库类型的迁移不属于本功能。

## 8. Synchronization Options

### 8.1 Insert

默认开启。Source 中存在、Target 中不存在的数据插入 Target。

### 8.2 Update

默认开启。Source 和 Target 均存在但数据不同的数据更新到 Target。

### 8.3 Delete

默认关闭。Target 中存在、Source 中不存在的数据从 Target 删除。

开启 Delete 时必须显示风险提示，并要求用户确认：

> Records that exist only in the target database will be deleted.

删除属于永久性数据变更，执行前必须再次确认。

## 9. Matching Strategy

数据比较需要明确记录匹配键。默认优先使用 Primary Key：

```text
● Primary Key
○ Unique Index
○ Custom Columns
```

如果 Table 没有 Primary Key，显示警告并提供 **Configure Matching Columns**。用户可以选择唯一字段作为匹配条件，例如 `email`。

## 10. Table Mapping

默认根据 Table Name 自动匹配：

```text
users      → users
orders     → orders
products   → products
```

支持手工映射不同名称的 Table，例如：

```text
customers  → clients
```

未匹配的 Table 显示为 Unmapped。用户可以取消某个 Table，使其不参与同步。

## 11. Comparison

点击 Compare 后开始比较。UI 应显示整体和单表进度，并允许取消比较。

比较完成后显示摘要：

```text
12 tables compared
35 inserts
171 updates
4 deletes
8 unchanged
```

## 12. Comparison Result

顶部显示：

| 类型 | 数量 |
|---|---:|
| Inserts | 35 |
| Updates | 171 |
| Deletes | 4 |
| Unchanged | 8 |

Table 列表显示：

| Table | Insert | Update | Delete |
|---|---:|---:|---:|
| users | 12 | 35 | 0 |
| orders | 23 | 128 | 4 |
| products | 0 | 8 | 0 |
| categories | 0 | 0 | 0 |

提供 All / Insert / Update / Delete 筛选和 Row Search。

## 13. Table / Row / Cell Diff

点击 Table 后进入详细 Diff。

左右显示 Source 与 Target 数据，至少支持 Row 级和 Cell 级差异识别。

例如：

```text
Source       Target
age = 20     age = 21   ← changed
```

不同 Cell 必须有明确的视觉高亮。

用户可以逐 Row 选择是否同步：

```text
☑ Row 1  UPDATE
☑ Row 2  SAME
☐ Row 3  INSERT
☐ Row 4  DELETE
```

同步选择支持三级粒度：

```text
Table
  ↓
Operation
  ↓
Row
```

## 14. SQL Preview

用户点击 **Generate SQL** 后，根据 Change Set 生成最终 SQL。

示例：

```sql
INSERT INTO users (id, name, age)
VALUES (3, 'Tom', 25);

UPDATE users
SET age = 20
WHERE id = 1;

DELETE FROM users
WHERE id = 4;
```

SQL Preview 支持：

- 查看
- 搜索
- 编辑
- 复制
- 保存 SQL
- Execute

SQL 是最终执行层，产品内部流程为：

```text
Diff
 ↓
Change Set
 ↓
SQL
 ↓
Execution
```

## 15. Execute 前安全检查

执行前必须重新检查：

- Target Connection 是否有效
- Database 是否存在
- Table 是否存在
- Matching Key 是否发生变化
- Target 数据是否在 Compare 后发生变化
- 当前用户是否拥有所需权限

如果 Target 自比较完成后发生变化，应提示：

> The target database has changed since the comparison was performed. Re-comparison is recommended.

提供 **Recompare** 和 **Execute Anyway**。

## 16. Transaction

如果数据库支持 Transaction，默认开启：

```sql
BEGIN;

INSERT ...
UPDATE ...
DELETE ...

COMMIT;
```

发生错误时执行 Rollback。

不支持 Transaction 的数据库必须明确提示用户可能发生 Partial Apply。

## 17. 权限检查

根据 Change Set 检查 Target 权限：

- Insert → INSERT
- Update → UPDATE
- Delete → DELETE

权限不足时阻止对应操作，并显示具体 Table、Operation 和错误信息。

## 18. Execution

执行过程中显示：

```text
Synchronizing...

users       ████████████████████ 100%
orders      ████████████░░░░░░░░  62%

Processed: 1,285
Succeeded: 1,281
Failed: 4
```

完成后显示：

```text
Synchronization Completed

✓ 35 records inserted
✓ 171 records updated
✓ 4 records deleted

Duration: 12.8s
```

存在失败时显示 Completed with Errors，并提供错误详情、复制错误和查看 SQL 的能力。

## 19. Sync History

建议在本地保存同步历史：

```text
2026-08-13 15:20
DEV → TEST
35 inserts
171 updates
4 deletes
Success
```

历史记录应包含：

- Source
- Target
- Options
- Table Mapping
- SQL
- Execution Result
- 时间与耗时

## 20. Saved Sync Task

允许保存同步任务，例如：

```text
DEV → TEST Data Sync
```

保存：

- Source / Target 标识
- Database / Schema
- Sync Options
- Table Mapping
- Matching Strategy

**不得保存数据库密码。**

## 21. 大数据量策略

不能将整个 Source Table 一次性加载到客户端内存。应采用分页 / Chunk 处理：

```text
Source
 ↓
Chunk
 ↓
Compare
 ↓
Change Set
 ↓
Target
```

V1 优先支持基于 Primary Key 的分页：

```sql
WHERE id > ?
ORDER BY id
LIMIT 1000
```

对于超大表，后续可增加 Hash / Checksum → Changed Range → Row Comparison 的优化路径。

## 22. 大字段与特殊数据类型

对于 BLOB / CLOB / TEXT / JSON，比较策略可抽象为：

```text
● Full
○ Hash
○ Ignore
```

默认可使用 Hash 以减少客户端数据传输。

必须正确区分：

- NULL
- 空字符串
- 0

时间类型必须考虑 DATETIME / TIMESTAMP / Timezone，避免因时区转换产生错误 Diff。

## 23. 数据库支持

V1：

| Database | Data Sync |
|---|---|
| MySQL | ✓ |
| PostgreSQL | ✓ |
| MariaDB | ✓ |
| SQLite | ✓ |
| SQL Server | 后续 |
| Oracle | 后续 |
| ClickHouse | 后续 |
| MongoDB | 后续 |

MongoDB 后续应以 Collection / Document 模型实现，不强行套用 SQL Table 模型。

## 24. 核心数据模型

### SyncTask

```text
SyncTask
├── sourceConnection
├── targetConnection
├── sourceDatabase
├── targetDatabase
├── sourceSchema
├── targetSchema
├── options
└── tableMappings
```

### ComparisonResult

```text
ComparisonResult
├── tableResults[]
├── insertCount
├── updateCount
├── deleteCount
└── unchangedCount
```

### TableResult

```text
TableResult
├── sourceTable
├── targetTable
├── mapping
├── insertCount
├── updateCount
├── deleteCount
└── rowChanges[]
```

### RowChange

```text
RowChange
├── operation
├── primaryKey
├── sourceRow
├── targetRow
└── selected
```

Operation：`INSERT` / `UPDATE` / `DELETE` / `UNCHANGED`。

## 25. 状态机

```text
DRAFT
  ↓
CONFIGURED
  ↓
COMPARING
  ↓
COMPARED
  ↓
REVIEWING
  ↓
GENERATING_SQL
  ↓
READY_TO_EXECUTE
  ↓
EXECUTING
  ↓
COMPLETED
```

异常状态：`COMPARE_FAILED`、`EXECUTION_FAILED`。

## 26. UX 原则

### 26.1 不直接修改 Target

默认流程必须先 Compare，再 Review，再 Apply。

### 26.2 Delete 显式开启

Delete 默认关闭，开启和执行前均提供风险提示。

### 26.3 始终展示 Change

用户在执行前必须能回答：**What will change?**

### 26.4 SQL 是最终执行层

用户可以审查最终 SQL，便于理解、复制和排查问题。

## 27. DataZen 差异化 UX

Navicat 的能力可以保留，但 DataZen 不建议简单复制传统 Wizard：

```text
Next → Next → Next
```

推荐使用现代 Diff Workspace：

```text
Source → Target
       ↓
Summary
       ↓
Table Diff
       ↓
Row / Cell Diff
       ↓
Change Set
       ↓
SQL Preview
       ↓
Sync
```

核心设计思想是：

> **Navicat 的数据同步能力 + Git Diff 的审查模型 + DataZen 的现代数据库 IDE 体验。**

## 28. MVP 范围

### P0

- Source / Target
- MySQL → MySQL
- PostgreSQL → PostgreSQL
- Table Mapping
- Primary Key Matching
- Insert
- Update
- Delete
- Table Diff
- Row Diff
- SQL Preview
- Execute
- Transaction
- Execution Result

### P1

- Custom Matching Columns
- Saved Sync Task
- Sync History
- 大表优化
- BLOB / CLOB Hash
- Diff Filter
- Row-level Selection

### P2

- Cross Database
- Schema Transformation
- Data Transformation
- Scheduled Synchronization
- CLI
- Sync Task Automation
- MongoDB Collection Synchronization

## 29. 产品定位

DataZen 数据同步不应定位为：

> 把 A 数据库复制到 B 数据库。

而应定位为：

> **可视化比较两个数据库的数据差异，并让用户安全地将选定的差异应用到目标数据库。**

核心闭环：

```text
Source DB
   ↓
Compare
   ↓
Data Diff
   ↓
Table / Row Diff
   ↓
Change Set
   ↓
SQL Preview
   ↓
Apply
   ↓
Target DB
```
