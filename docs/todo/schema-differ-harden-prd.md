下面按当前 `feat/schema-diff-hardening` 的实现状态，把**剩余工作收敛成一份可直接执行的 PRD**。目标不是继续扩大 Schema Diff，而是把当前已经搭好的 Diff → Operation → Driver → Plan 链路补完整。

# Schema Migration Hardening — 剩余工作 PRD

## 1. 产品目标

完善 DataZen Schema Diff / Schema Migration，使其能够：

1. 安全识别 Schema 差异
2. 生成**可执行且可靠**的 Migration Plan
3. 对无法自动迁移的场景明确告知用户
4. 对高风险操作提供人工确认
5. 支持必要的 Backfill 流程
6. 正确处理 MySQL / PostgreSQL / SQLite 的方言差异
7. 提供可追溯的 rollback 信息

核心原则：

> **宁可生成 Unsupported / Requirement，也不能生成错误 SQL。**

---

# 2. 功能范围

本阶段只处理以下 6 个模块：

```text
Schema Diff
    ↓
Migration Operation
    ↓
Dependency Resolution
    ↓
Driver Capability
    ↓
Migration Plan
    ↓
Review / Execute
```

不包含：

- Schema Editor
- 完整 Data Sync
- 数据迁移工具
- 新数据库驱动
- UI 大规模重构

---

# 3. P0：完善 Migration Operation → DDL

## 3.1 Create Table

### 需求

`CreateTable` 必须完整表达 source schema。

支持：


| 属性             | 要求           |
| -------------- | ------------ |
| column name    | 必须           |
| data type      | 必须           |
| nullable       | 必须           |
| default        | 必须           |
| comment        | 尽可能支持        |
| auto increment | 尽可能支持        |
| primary key    | 必须           |
| index          | 单独 operation |


### 示例

Source：

```sql
CREATE TABLE users (
    id BIGINT NOT NULL AUTO_INCREMENT,
    name VARCHAR(100) NOT NULL DEFAULT '',
    email VARCHAR(255),
    PRIMARY KEY (id)
);
```

Migration Plan 应能够表达对应结构，而不是只生成：

```sql
CREATE TABLE users (
    id BIGINT,
    name VARCHAR(100),
    email VARCHAR(255)
);
```

---

# 4. P0：Cross-Dialect Type Mapping

## 4.1 Mapper 成功

例如：

```text
PostgreSQL jsonb
        ↓
MySQL json
```

正常生成 target SQL。

## 4.2 Mapper 失败

例如：

```text
PostgreSQL custom_type
        ↓
MySQL
```

Mapper 返回错误时：

```text
PlanRequirement::Unsupported
```

而不是继续使用 source type。

### 禁止行为

```rust
if let Ok(ty) = mapper(...) {
    ...
}
```

失败后继续 migration。

### 正确行为

```text
Type Mapping Failed
        ↓
Unsupported
        ↓
该 operation 不进入 executable statements
```

---

# 5. P0：NOT NULL Backfill

这是本阶段最重要的业务安全流程。

## 5.1 场景

Source：

```sql
email VARCHAR(255) NOT NULL
```

Target：

```sql
email 不存在
```

且：

```text
没有 default
```

不能直接：

```sql
ADD COLUMN email VARCHAR(255) NOT NULL;
```

因为已有数据可能无法满足 NOT NULL。

---

## 5.2 Migration Plan

自动生成：

```sql
ADD COLUMN email VARCHAR(255) NULL;
```

同时：

```text
PlanRequirement::Backfill
```

内容：

```text
table
column
reason
```

---

## 5.3 UI / Workflow

用户看到：

```text
⚠ Backfill required

users.email

Existing rows must be populated before enforcing NOT NULL.

[Configure Backfill]
```

用户填写：

```sql
UPDATE users
SET email = '';
```

然后：

```text
Run Backfill
      ↓
Check NULL count
      ↓
NULL = 0
      ↓
SET NOT NULL
```

---

## 5.4 验证

执行 `SET NOT NULL` 前必须检查：

```sql
SELECT COUNT(*)
FROM users
WHERE email IS NULL;
```

如果：

```text
count > 0
```

则：

```text
Migration blocked
```

---

# 6. P1：Primary Key Migration

## 6.1 Create PK

支持：

```text
CreatePrimaryKey
```

生成：

```sql
ALTER TABLE users
ADD PRIMARY KEY (id);
```

---

## 6.2 Drop PK

支持：

```text
DropPrimaryKey
```

但是必须能够确定 constraint / PK 名称。

不能依赖：

```text
ALTER TABLE ... DROP CONSTRAINT undefined
```

### 要求

Snapshot 必须提供足够信息，或者 driver 根据数据库 metadata 获取实际名称。

---

## 6.3 PK 变更

例如：

```text
PRIMARY KEY(id)
        ↓
PRIMARY KEY(user_id)
```

应该拆成：

```text
DropPrimaryKey
        ↓
CreatePrimaryKey
```

并通过 dependency resolver 保证顺序。

---

# 7. P1：Index Migration

## 7.1 Create Index

支持：

```text
CreateIndex
```

包括：

- index name
- columns
- unique
- index type

例如：

```sql
CREATE UNIQUE INDEX idx_email
ON users(email);
```

---

## 7.2 Drop Index

支持：

```sql
DROP INDEX idx_email;
```

不同数据库使用 driver renderer 处理。

---

## 7.3 Index 差异

以下变化必须被识别：

```text
idx_email(email)
        ↓
idx_email(email, tenant_id)
```

应当：

```text
DropIndex
    ↓
CreateIndex
```

---

# 8. P1：Rollback

每个 migration statement 尽可能提供：

```text
Forward SQL
Rollback SQL
```

例如：

```text
ADD COLUMN email VARCHAR(255)
```

对应：

```text
DROP COLUMN email
```

---

## 8.1 Rollback Completeness

继续保留：

```rust
RollbackCompleteness {
    complete,
    missing
}
```

UI 显示：

### 完整

```text
Rollback: Available
```

### 不完整

```text
Rollback: Partial

2 statements cannot be automatically rolled back.
```

---

## 8.2 不允许伪造 rollback

无法安全生成 rollback 时：

```text
rollback_sql = None
```

而不是生成可能错误的 SQL。

---

# 9. P1：SQLite Table Rebuild

SQLite 特殊处理。

对于：

```text
ALTER COLUMN TYPE
ALTER COLUMN NULLABILITY
```

等 SQLite 不直接支持的操作：

```text
CREATE new table
        ↓
COPY data
        ↓
DROP old table
        ↓
RENAME new table
        ↓
RECREATE indexes
```

---

## 本阶段要求

如果不实现完整 rebuild：

> 必须明确标记 `Unsupported`。

不能出现：

```text
Capability = supported
Renderer = unsupported
```

这种语义不一致。

---

# 10. Driver Capability 一致性

三个 driver：

```text
MySQL
PostgreSQL
SQLite
```

必须满足：

```text
supports(operation)
        ⇅
renderer.render(operation)
```

原则：

### supports = false

一定不能生成 SQL。

### supports = true

renderer 必须能够处理。

---

## Capability Matrix

最终需要形成类似：


| Operation       | MySQL | PostgreSQL | SQLite    |
| --------------- | ----- | ---------- | --------- |
| CreateTable     | ✅     | ✅          | ✅         |
| AddColumn       | ✅     | ✅          | ✅         |
| DropColumn      | ✅     | ✅          | ⚠         |
| AlterColumnType | ✅     | ✅          | ⚠ rebuild |
| SetNullable     | ⚠     | ✅          | ⚠         |
| SetDefault      | ✅     | ✅          | ⚠         |
| SetComment      | ✅     | ✅          | ❌         |
| AutoIncrement   | ✅     | ⚠          | ❌         |
| CreatePK        | ✅     | ✅          | ⚠         |
| DropPK          | ✅     | ✅          | ⚠         |
| CreateIndex     | ✅     | ✅          | ✅         |
| DropIndex       | ✅     | ✅          | ✅         |


实际矩阵以各 driver 当前真实能力为准。

---

# 11. Dependency Resolver

Migration Operation 必须保证执行顺序。

例如：

```text
CreateTable
    ↓
AddColumn
    ↓
CreatePrimaryKey
    ↓
CreateIndex
```

删除则反向：

```text
DropIndex
    ↓
DropPrimaryKey
    ↓
DropColumn
    ↓
DropTable
```

最终要求：

> Migration Plan 中 operation 顺序必须是 deterministic 的。

相同输入 Schema：

```text
第一次生成 Plan
=
第二次生成 Plan
```

避免 SQL 顺序随机变化。

---

# 12. Destructive Change Safety

默认：

```text
allow_destructive = false
```

以下属于 destructive：

- Drop table
- Drop column
- Drop index
- Drop primary key
- 数据截断类变更
- 潜在数据丢失的 type change

默认行为：

```text
Detect
 ↓
Plan warning
 ↓
不进入 executable statements
```

用户显式开启：

```text
Allow destructive changes
```

才允许执行。

---

# 13. Type Change Risk

类型变化不能简单认为都是安全的。

例如：

```text
VARCHAR(255)
      ↓
VARCHAR(100)
```

属于潜在数据截断。

应该：

```text
StatementRisk::Rewrite
```

或者：

```text
Destructive
```

并要求用户确认。

---

# 14. Plan Review

最终 Migration Plan 应能向用户展示：

```text
Migration Plan

Target: PostgreSQL
Tables: 5

────────────────────────

ADD COLUMN
users.email VARCHAR(255)
Risk: Additive

ALTER COLUMN
users.name VARCHAR(255) → VARCHAR(100)
Risk: Rewrite ⚠

DROP COLUMN
users.legacy
Risk: Destructive ⚠

────────────────────────

Requirements

⚠ Backfill required: users.email

Rollback

Partial
2 statements have no automatic rollback

[Review SQL]
[Execute]
```

---

# 15. Test Requirements

## Unit Test

至少覆盖：

### Schema Diff

- column added
- column removed
- type changed
- nullable changed
- default changed
- comment changed
- auto increment changed
- PK changed
- index changed

### Safety

- destructive disabled
- destructive enabled
- NOT NULL without default
- type narrowing

### Cross Dialect

- PostgreSQL → MySQL
- MySQL → PostgreSQL
- PostgreSQL → SQLite
- type mapper success
- type mapper failure

### Driver

每个 driver：

- supported operation
- unsupported operation
- generated SQL
- rollback SQL
- risk

### Dependency

- PK before index
- index before column drop
- create table before indexes
- deterministic ordering

---

# 16. E2E

至少保留以下场景：

```text
SD-001 Basic Schema Diff
SD-002 Add Column
SD-003 Drop Column Safety
SD-004 Type Change
SD-005 Cross Dialect
SD-006 NOT NULL Backfill
SD-007 Index Diff
SD-008 Primary Key Diff
SD-009 SQLite Unsupported / Rebuild
```

每个场景验证：

```text
Schema
 ↓
Diff
 ↓
Plan
 ↓
SQL
 ↓
Execute
 ↓
Target Schema
```

而不是只测试 SQL 字符串。

---

# 17. 非目标

本 feature 暂时不做：

- 自动生成业务 Backfill SQL
- 自动推断业务默认值
- 任意数据库之间的自动类型转换
- SQLite 所有 ALTER TABLE 能力
- 在线 Schema Migration Scheduler
- Migration History / Versioning
- Migration 文件管理
- 多人 Migration Lock

这些可以作为后续 feature。

---

# 18. 最终验收标准

这个 feature 完成的标准不是“CI 通过”，而是：

### 架构

```text
Schema
 → IR
 → Operation
 → Dependency
 → Capability
 → Renderer
 → Plan
```

完整闭环。

### 安全

> **任何 DataZen 无法确定安全执行的 migration，都必须停在 Plan 阶段，而不是生成错误 SQL。**

### Driver

> Dialect-specific SQL 全部由 driver 负责。

### Migration

> Plan 必须能够明确区分 `executable / destructive / rewrite / backfill / unsupported`。

### Rollback

> 能回滚就提供 rollback SQL，不能回滚就明确标记。

### 测试

> 三个核心 driver + cross-dialect + destructive + backfill + dependency 均有覆盖。

---

## 优先级最终收敛


| 优先级    | 工作                                |
| ------ | --------------------------------- |
| **P0** | CreateTable 完整 metadata           |
| **P0** | Type mapper error handling        |
| **P0** | NOT NULL Backfill workflow        |
| **P1** | Primary Key migration             |
| **P1** | Index migration                   |
| **P1** | Rollback 完整性                      |
| **P1** | SQLite rebuild / 明确 Unsupported   |
| **P1** | Capability / Renderer 一致性         |
| **P1** | Dependency deterministic ordering |
| **P1** | Type change risk                  |
| **P2** | Migration History                 |
| **P2** | Migration 文件/版本管理                 |


**建议这个 feature 到 P1 全部完成就收口。**P2 的 Migration History / Versioning 不应该继续塞进 `schema-diff-hardening`，否则这个 PR 会从“Schema Diff 安全性”膨胀成完整 Migration System。