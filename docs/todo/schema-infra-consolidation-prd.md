# Schema Diff / Transfer / Sync 基础设施整合 PRD

> 状态：Draft  
> 优先级：P1  
> 影响范围：`packages/driver-api`、`src-tauri/src/schema_diff`、`src-tauri/src/data_sync`、`src-tauri/src/data_transfer`、`src-tauri/src/transfer`、`packages/drivers/*`

## 背景

DataZen 有三个涉及 schema 操作的产品模块：

| 模块 | 语义 | 代码路径 |
|------|------|---------|
| **Schema Diff** | 结构对比 → ALTER 计划 → DDL 部署 | `schema_diff/` + `commands/schema_diff.rs` |
| **Data Sync** | 同族、结构完全一致 → 行级 diff → DML | `data_sync/` + `commands/sync/` |
| **Data Transfer** | 异构/同族单向拷贝 → IR → CREATE + INSERT | `data_transfer/` + `transfer/` |

经架构 review 发现以下问题：

1. **Schema Diff 幽灵 Diff**：同族方言下 `int4` vs `integer` 等类型别名导致永远有差异
2. **`fetch_full_column_types`** 在 `commands/sync/compare.rs` 和 `data_transfer/structure.rs` 重复实现
3. **`effective_primary_key()`** 在 `data_sync/gate.rs` 和 `schema_diff/ir.rs` 各有一份
4. **事务管理**：Data Sync 和 Schema Diff Deploy 各自实现了事务包装，可抽统一抽象
5. **Job cancel**：仅 Data Sync 有 cancel 机制，Transfer/Schema Diff 长任务缺乏取消支持

## 目标

- 消除同族方言下的幽灵 Diff
- 消除代码重复，建立 三模块共用的基础设施层
- 提升长任务的可中断性
- 不改变任何模块的产品语义和用户行为

## 非目标

- 不合并三个模块的产品域逻辑
- 不改变 Data Sync 的"结构必须完全一致"门闸语义
- 不在第一版处理 `default_value` 的规范化（PG `'0'::integer` vs `0`）

---

## 任务 1：驱动级类型规范化（消除幽灵 Diff）

### 问题

Schema Diff `compare.rs` 用字符串精确相等比较 `data_type`：

```rust
if col.data_type != tgt_col.data_type {
    changes.push(ColumnChange::DataType);
}
```

PG 驱动的 `get_table_schema` 可能返回 `int4` 或 `integer`（取决于元数据查询和 PG 版本），导致同一种类型被报为"已变更"。

### 已知幽灵 Diff 清单

| 方言 | 别名对 | 等价 |
|------|--------|------|
| PostgreSQL | `int4` / `integer` | ✅ |
| PostgreSQL | `int8` / `bigint` | ✅ |
| PostgreSQL | `int2` / `smallint` | ✅ |
| PostgreSQL | `float8` / `double precision` | ✅ |
| PostgreSQL | `float4` / `real` | ✅ |
| PostgreSQL | `bool` / `boolean` | ✅ |
| PostgreSQL | `character varying(n)` / `varchar(n)` | ✅ |
| PostgreSQL | `character(n)` / `char(n)` | ✅ |
| PostgreSQL | `timestamptz` / `timestamp with time zone` | ✅ |
| PostgreSQL | `timestamp without time zone` / `timestamp` | ✅ |
| MySQL | `int(11)` / `int` | ✅ (display width 无意义) |
| MySQL | `integer` / `int` | ✅ |
| MySQL | `bool` / `boolean` | ✅ |
| MySQL | `dec` / `decimal` / `numeric` | ✅ |

### 设计方案（方案 B：驱动级 trait 方法）

#### 1. driver-api 新增 trait 方法

```rust
// packages/driver-api/src/schema_migration.rs（或 traits.rs）

/// Normalize a column type string for comparison purposes.
/// Used by Schema Diff to eliminate phantom diffs caused by dialect-specific
/// type aliases (e.g. PG int4 = integer, MySQL INT(11) = INT).
///
/// The normalized form should be the canonical representation that the driver
/// considers equivalent. Two types that are semantically identical MUST produce
/// the same normalized string.
///
/// Default: identity (returns input unchanged).
pub trait TypeNormalizer: Send + Sync {
    fn normalize_type(&self, data_type: &str) -> String;
}
```

在 `DatabaseDriver` trait 上添加：

```rust
pub trait DatabaseDriver: Send + Sync {
    // ... existing methods ...

    /// Optional type normalizer for eliminating phantom diffs in Schema Diff.
    fn type_normalizer(&self) -> Option<Arc<dyn TypeNormalizer>> {
        None
    }
}
```

#### 2. 各驱动实现

**PostgreSQL**：
```rust
pub struct PostgresTypeNormalizer;

impl TypeNormalizer for PostgresTypeNormalizer {
    fn normalize_type(&self, data_type: &str) -> String {
        let upper = data_type.trim().to_ascii_uppercase();
        let (base, args, suffix) = parse_type_parts(&upper);
        
        let canonical_base = match base.as_str() {
            "INT" | "INT4" => "INTEGER",
            "INT8" => "BIGINT",
            "INT2" => "SMALLINT",
            "FLOAT8" | "DOUBLE PRECISION" => "DOUBLE PRECISION",
            "FLOAT4" => "REAL",
            "BOOL" => "BOOLEAN",
            "CHARACTER VARYING" => "VARCHAR",
            "CHARACTER" => "CHAR",
            "TIMESTAMPTZ" => "TIMESTAMP WITH TIME ZONE",
            "TIMESTAMP WITHOUT TIME ZONE" => "TIMESTAMP",
            other => other,
        };
        
        format_type(canonical_base, args.as_deref(), &suffix)
    }
}
```

**MySQL**：
```rust
pub struct MysqlTypeNormalizer;

impl TypeNormalizer for MysqlTypeNormalizer {
    fn normalize_type(&self, data_type: &str) -> String {
        let upper = data_type.trim().to_ascii_uppercase();
        let (base, args, suffix) = parse_type_parts(&upper);
        
        let canonical_base = match base.as_str() {
            "INTEGER" => "INT",
            "BOOL" => "BOOLEAN",
            "DEC" | "NUMERIC" => "DECIMAL",
            other => other,
        };
        
        // Strip display width for integer types (MySQL 8.0+ deprecated)
        let effective_args = if is_mysql_integer(canonical_base) {
            None
        } else {
            args
        };
        
        format_type(canonical_base, effective_args.as_deref(), &suffix)
    }
}
```

**SQLite**：基本只需 case normalization（`TEXT` = `text`）。

#### 3. driver-api 提供 `parse_type_parts` 公共辅助

```rust
// packages/driver-api/src/schema_migration.rs

/// Parse a type string into (base, args, suffix) components.
/// Example: "VARCHAR(255) UNSIGNED" → ("VARCHAR", Some("255"), "UNSIGNED")
pub fn parse_type_parts(raw: &str) -> (String, Option<String>, String) {
    // ... (从 data_sync/types_eq.rs 的 parse_type 逻辑移植)
}

/// Reassemble type from normalized parts.
pub fn format_type(base: &str, args: Option<&str>, suffix: &str) -> String {
    let mut out = base.to_string();
    if let Some(a) = args {
        if !a.is_empty() {
            out.push('(');
            out.push_str(a);
            out.push(')');
        }
    }
    if !suffix.is_empty() {
        out.push(' ');
        out.push_str(suffix);
    }
    out
}
```

#### 4. Schema Diff compare 层接入

```rust
// schema_diff/compare.rs

pub fn diff_table_schemas(
    table: &str,
    src: &TableSchema,
    tgt: &TableSchema,
    normalizer: Option<&dyn TypeNormalizer>,  // 同族时传入，跨族传 None
) -> TableColumnDiff {
    // ...
    let types_equal = match normalizer {
        Some(n) => n.normalize_type(&col.data_type)
                  == n.normalize_type(&tgt_col.data_type),
        None => col.data_type == tgt_col.data_type,
    };
    // ...
}
```

调用链传递：
```
commands/schema_diff.rs
  → compare_table_schemas_impl: 同族时从 target driver 获取 type_normalizer()
  → prepare_schema_diff_plan: PlanOptions 携带 normalizer
  → plan_single_table → diff_to_operations → diff_table_schemas(normalizer)
```

#### 5. Data Sync 的 types_eq.rs 迁移

将 `types_eq.rs` 中的规范化逻辑迁移到各驱动的 `TypeNormalizer` 实现中。
`data_sync/types_eq.rs` 改为调用 driver 的 `type_normalizer()` 而非自己维护别名表。

这样 **规范化逻辑从 Host 下沉到驱动**，符合 AGENTS.md 的驱动自治原则。

#### 6. PROTOCOL_VERSION

`TypeNormalizer` 是可选 trait，现有驱动不实现时 fallback 到字符串比较（向后兼容），无需立即 bump。
但建议在 PROTOCOL_VERSION 的 changelog 中记录此新能力。

### 测试

| 测试类型 | 位置 | 内容 |
|---------|------|------|
| 单元测试 | 各驱动 `migration.rs` 或新建 `type_normalizer.rs` | 每种别名对的 normalize 等价性 |
| 集成测试 | `src-tauri/src/schema_diff/compare.rs` | 传入 normalizer 后 int4/integer 不报 DataType changed |
| 现有 E2E | `e2e/specs/schema-diff-*.ts` | 回归验证（不应 break） |

### 验收标准

- [ ] PG `int4` ↔ `integer` 同表不再产生 AlterColumnType 操作
- [ ] MySQL `int(11)` ↔ `int` 同表不再产生 AlterColumnType 操作  
- [ ] 不实现 `type_normalizer` 的驱动行为不变
- [ ] Data Sync `types_eq.rs` 改为委托到驱动 normalizer
- [ ] 所有 schema-diff E2E 通过

---

## 任务 2：`fetch_full_column_types` 去重

### 问题

`fetch_full_column_types` 在两处几乎相同的实现：
- `data_transfer/structure.rs::fetch_full_column_types`
- `commands/sync/compare.rs::fetch_full_column_types`

两者都是：取 adapter 的 `full_column_types_query(table)` → 执行 → 返回 `HashMap<String, String>`。

### 方案

将函数移到 `transfer/` 公共模块（新建 `transfer/full_types.rs` 或放入 `transfer/adapter.rs`）：

```rust
// transfer/full_types.rs
pub async fn fetch_full_column_types(
    adapter: &dyn SyncSourceAdapter,
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    table: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error + Send + Sync>> {
    let Some(sql) = adapter.full_column_types_query(table) else {
        return Ok(HashMap::new());
    };
    let result = driver.query(handle, &sql).await?;
    let mut map = HashMap::new();
    for row in &result.rows {
        if let (Some(Some(Value::String(name))), Some(Some(Value::String(ft)))) =
            (row.get(0), row.get(1))
        {
            map.insert(name.clone(), ft.clone());
        }
    }
    Ok(map)
}
```

`data_transfer/structure.rs` 和 `commands/sync/compare.rs` 改为 `use crate::transfer::full_types::fetch_full_column_types;`。

### 验收标准

- [ ] `data_transfer/structure.rs` 中的 `fetch_full_column_types` 删除
- [ ] `commands/sync/compare.rs` 中的 `fetch_full_column_types` 删除
- [ ] 两处调用改为共用 `transfer::full_types::fetch_full_column_types`
- [ ] 现有测试通过

---

## 任务 3：`effective_primary_key()` 去重

### 问题

- `data_sync/gate.rs`：从 `TableSchema` 提取有效 PK（优先 `primary_keys`，回退 `columns[].is_primary_key`）
- `schema_diff/ir.rs::effective_primary_keys`：逻辑完全相同

### 方案

在 `packages/driver-api/src/types.rs` 的 `TableSchema` 上添加方法（或旁边加自由函数）：

```rust
impl TableSchema {
    /// Effective primary key columns. Prefers `primary_keys` field;
    /// falls back to columns marked `is_primary_key`.
    pub fn effective_primary_keys(&self) -> Vec<String> {
        if !self.primary_keys.is_empty() {
            return self.primary_keys.clone();
        }
        self.columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect()
    }
}
```

`data_sync/gate.rs` 和 `schema_diff/ir.rs` 改为调用 `schema.effective_primary_keys()`。

### 验收标准

- [ ] `data_sync/gate.rs` 的 effective PK 逻辑删除，改用 `TableSchema::effective_primary_keys()`
- [ ] `schema_diff/ir.rs` 的 `effective_primary_keys` 函数删除，改用 `TableSchema::effective_primary_keys()`
- [ ] `cargo test -p datazen-driver-api` 通过
- [ ] `cargo test -p datazen` 通过

---

## 任务 4：统一 `TransactionScope` 抽象

### 问题

Data Sync (`data_sync/execute.rs`) 和 Schema Diff Deploy (`schema_diff/deploy.rs`) 各自实现了事务包装：

| 组件 | 事务 API | 特点 |
|------|---------|------|
| Data Sync | `StatementExecutor::begin/commit/rollback` + `execute_with_params` | 参数化 DML，单事务 |
| Schema Diff | `DriverStatementExecutor::exec` 拦截 `BEGIN/COMMIT/ROLLBACK` 字符串 | 纯 DDL，方言感知 atomicity |

### 方案

抽取共用的事务生命周期管理：

```rust
// services/transaction.rs（新文件）

/// Dialect-aware transaction scope.
pub struct TransactionScope<'a> {
    driver: &'a dyn DatabaseDriver,
    handle: &'a ConnectionHandle,
    atomicity: DdlAtomicity,
    active: bool,
}

pub enum DdlAtomicity {
    Transactional,          // PG: BEGIN → COMMIT/ROLLBACK
    AutoCommitPerStatement, // MySQL: 每条 DDL 自动提交
    Unknown,                // 不确定
}

impl<'a> TransactionScope<'a> {
    pub async fn begin(
        driver: &'a dyn DatabaseDriver,
        handle: &'a ConnectionHandle,
        dialect: &str,
    ) -> Result<Self, String> {
        let atomicity = ddl_atomicity(dialect);
        let active = matches!(atomicity, DdlAtomicity::Transactional);
        if active {
            driver.begin_transaction(handle).await.map_err(|e| e.to_string())?;
        }
        Ok(Self { driver, handle, atomicity, active })
    }

    pub async fn commit(mut self) -> Result<(), String> {
        if self.active {
            self.driver.commit(self.handle).await.map_err(|e| e.to_string())?;
            self.active = false;
        }
        Ok(())
    }

    pub async fn rollback(mut self) -> Result<(), String> {
        if self.active {
            self.driver.rollback(self.handle).await.map_err(|e| e.to_string())?;
            self.active = false;
        }
        Ok(())
    }

    pub fn is_transactional(&self) -> bool {
        matches!(self.atomicity, DdlAtomicity::Transactional)
    }
}

impl Drop for TransactionScope<'_> {
    fn drop(&mut self) {
        if self.active {
            tracing::warn!("TransactionScope dropped without commit/rollback");
        }
    }
}
```

`schema_diff/deploy.rs` 改用 `TransactionScope`；`data_sync/execute.rs` 可选择改用或保持现有实现（因其 `execute_with_params` 需求不同）。

### 验收标准

- [ ] `services/transaction.rs` 新增 `TransactionScope`
- [ ] `schema_diff/deploy.rs` 用 `TransactionScope` 替代手动 `BEGIN/COMMIT/ROLLBACK` 拦截
- [ ] `ddl_atomicity` 函数移到 `services/transaction.rs`
- [ ] Data Sync 的 PG 事务路径可选改用（或保持独立，仅共用 `DdlAtomicity` 判断）
- [ ] 所有 schema-diff E2E 通过

---

## 任务 5：Job Cancel 模式推广

### 问题

Data Sync 在 `commands/sync/jobs.rs` 有完整的 job 注册/取消机制：

```rust
pub struct SyncJobRegistry {
    active: DashMap<String, Arc<AtomicBool>>,  // job_id → cancel_flag
}
```

Data Transfer 在 `data_transfer/execute.rs` 接收 `cancelled: Option<Arc<AtomicBool>>` 但没有统一的 job registry。
Schema Diff Deploy 完全没有取消支持。

### 方案

抽取通用 Job Registry：

```rust
// services/job_registry.rs

pub struct JobRegistry {
    active: DashMap<String, Arc<AtomicBool>>,
}

impl JobRegistry {
    pub fn new() -> Self { ... }

    /// Register a new job, return cancel token.
    pub fn register(&self, job_id: &str) -> Arc<AtomicBool> { ... }

    /// Cancel a job by ID.
    pub fn cancel(&self, job_id: &str) -> bool { ... }

    /// Remove a completed/cancelled job.
    pub fn remove(&self, job_id: &str) { ... }

    /// Check if cancelled.
    pub fn is_cancelled(&self, job_id: &str) -> bool { ... }
}
```

在 `AppState` 中挂载共用 `JobRegistry`（或保留独立的 `SyncJobRegistry` + 新增 `TransferJobRegistry` / `SchemaDiffJobRegistry`）。

### IPC 命令

```
cancel_data_sync(job_id)       → 已有
cancel_data_transfer(job_id)   → 新增
cancel_schema_diff_deploy(job_id) → 新增
```

### 验收标准

- [ ] `services/job_registry.rs` 提供通用 `JobRegistry`
- [ ] Data Sync 的 `SyncJobRegistry` 改用或包装 `JobRegistry`
- [ ] Data Transfer 添加 `cancel_data_transfer` IPC
- [ ] Schema Diff Deploy 添加 `cancel_schema_diff_deploy` IPC，长任务可中断
- [ ] 前端 Data Transfer 和 Schema Diff 向导添加"取消"按钮

---

## 实现顺序

建议按依赖关系分两批：

### Wave 1（无依赖，可并行）

| Track | 任务 | 改动文件数 | 风险 |
|-------|------|-----------|------|
| A | 任务 2: `fetch_full_column_types` 去重 | ~3 | 低 |
| B | 任务 3: `effective_primary_key` 去重 | ~3 | 低 |

### Wave 2（依赖 Wave 1 完成）

| Track | 任务 | 改动文件数 | 风险 |
|-------|------|-----------|------|
| C | 任务 1: 驱动级类型规范化 | ~10 | 中（涉及 driver-api） |
| D | 任务 4: TransactionScope | ~4 | 中 |
| E | 任务 5: Job Cancel 推广 | ~8 | 中（涉及前后端） |

### 总测试要求

- `cargo test -p datazen-driver-api`
- `cargo test -p datazen`
- `cargo test -p datazen-driver-postgres`
- `cargo test -p datazen-driver-mysql`
- `cargo test -p datazen-driver-sqlite`
- `npx vitest run`
- `pnpm e2e --suite schema-diff`（E2E 回归）
- `pnpm e2e --suite data-sync`（E2E 回归）
