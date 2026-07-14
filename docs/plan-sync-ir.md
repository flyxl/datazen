# 数据同步中间类型表示（Sync IR）实施计划

> **目标**：用 LLVM-style 中间码（IR）替代 N×N 直接类型映射，将跨库同步的扩展成本从 O(N²) 降到 O(N)。

---

## 一、现状分析

### 1.1 当前架构

```
src-tauri/src/commands/sync.rs
├── pg_type_to_mysql()          # PG → MySQL 列类型映射 (~60行)
├── mysql_type_to_pg()          # MySQL → PG 列类型映射 (~40行)
├── map_default_for_target()    # 默认值跨库转换
├── pg_full_column_types()      # PG 专用：获取精确类型（含精度）
├── format_value()              # 值字面量格式化（硬编码 is_mysql_target 分支）
├── build_create_table_ddl()    # DDL 生成（match src_type/tgt_type）
├── sync_table()                # 单表同步
└── sync_tables()               # 多表同步（带进度/断点续传）
```

**问题**：

| 当前 DB 数量 | 需要的映射函数对 | 已实现 | 缺失 |
|-------------|-----------------|--------|------|
| 6 (PG, MySQL, MariaDB, SQLite, Presto, Trino) | 30 | 2 (PG↔MySQL) | 28 |

新加第 7 种库需要写 12 个映射函数，维护和测试成本指数增长。

### 1.2 已有的"半 IR"

`db::Value` enum 已经是**运行时数据层**的 IR：

```rust
pub enum Value {
    Null, Bool(bool), Integer(i64), Float(f64),
    String(String), Bytes(Vec<u8>), Timestamp(String), Json(serde_json::Value),
}
```

所有驱动查出的数据行统一为 `Vec<Vec<Option<Value>>>`，同步时直接透传。
缺的只是**列类型元数据层**的 IR。

---

## 二、目标架构

```
                    ┌──────────────────────────────┐
                    │   src-tauri/src/sync/ir.rs    │
                    │   IRType / IRColumn / IRDefault│
                    └──────────────────────────────┘
                           ▲              │
           to_ir()         │              │  from_ir()
    ┌──────────────────────┘              └──────────────────────┐
    │                                                            │
┌───┴────────┐ ┌───────────┐ ┌──────────┐ ┌──────────┐ ┌────────┴──┐
│  PG        │ │  MySQL    │ │  SQLite  │ │  Trino   │ │  未来新库  │
│  Adapter   │ │  Adapter  │ │  Adapter │ │  Adapter │ │  Adapter   │
└────────────┘ └───────────┘ └──────────┘ └──────────┘ └────────────┘
  ~120行         ~100行        ~80行        ~80行         ~80行
```

同步流程：

```
源库 ColumnSchema.data_type
        │ src_adapter.to_ir()
        ▼
    IRColumn { ir_type: Varchar { length: Some(255) }, ... }
        │ tgt_adapter.from_ir()
        ▼
目标库 DDL 类型字符串 ("VARCHAR(255)" / "character varying(255)" / ...)
```

---

## 三、IR 类型系统设计

### 3.1 IRType 枚举

```rust
/// 文件：src-tauri/src/sync/ir.rs

/// 与具体数据库无关的标准列类型。
#[derive(Debug, Clone, PartialEq)]
pub enum IRType {
    // ── 布尔 ──
    Bool,

    // ── 整数 ──
    Int8,           // tinyint
    Int16,          // smallint
    Int32,          // int
    Int64,          // bigint

    // ── 浮点 ──
    Float32,        // real / float
    Float64,        // double precision

    // ── 精确小数 ──
    Decimal {
        precision: u8,  // 0 表示无限制
        scale: u8,
    },

    // ── 字符 ──
    Char { length: u32 },
    Varchar { length: Option<u32> },  // None = unbounded
    Text,

    // ── 二进制 ──
    Binary { length: Option<u32> },
    Blob,

    // ── 日期时间 ──
    Date,
    Time { with_timezone: bool },
    Timestamp { with_timezone: bool },

    // ── 结构化 ──
    Json,
    Uuid,

    // ── 位 ──
    Bit { length: u32 },

    // ── 透传/降级 ──
    /// 无法映射到标准 IR 的类型，保留原文。
    /// from_ir 遇到 Other 时应降级为 Text。
    Other(String),
}
```

### 3.2 IRColumn 结构体

```rust
/// 中间表示的列定义（完整的同步单元）。
#[derive(Debug, Clone)]
pub struct IRColumn {
    pub name: String,
    pub ir_type: IRType,
    pub nullable: bool,
    pub default_expr: Option<IRDefault>,
    pub is_primary_key: bool,
    pub is_auto_increment: bool,
    pub comment: Option<String>,
}
```

### 3.3 IRDefault 枚举

```rust
/// 标准化的默认值表达式。
#[derive(Debug, Clone)]
pub enum IRDefault {
    /// CURRENT_TIMESTAMP / now() 等标准时间戳默认值
    CurrentTimestamp,
    /// 字面量值（已去除库专有语法，如 PG 的 `::text` 后缀）
    Literal(String),
    /// 无法标准化的表达式原文（如序列、存储过程调用等）
    RawExpression(String),
}
```

### 3.4 IRTable 结构体

```rust
/// 中间表示的完整表定义。
#[derive(Debug, Clone)]
pub struct IRTable {
    pub name: String,
    pub columns: Vec<IRColumn>,
    pub primary_keys: Vec<String>,
}
```

---

## 四、Adapter Trait 设计

### 4.1 核心 Trait

```rust
/// 文件：src-tauri/src/sync/adapter.rs

use super::ir::{IRColumn, IRDefault, IRTable, IRType};
use crate::db::{ColumnSchema, TableSchema, Value};

/// 源库适配器：原生类型 → IR
pub trait SyncSourceAdapter: Send + Sync {
    /// 将一列的原生类型字符串转换为 IR 列定义。
    ///
    /// `native_full_type` 为带精度的完整类型（如 PG 的 `character varying(255)`），
    /// 可能与 `column.data_type` 不同（后者可能是简写）。
    fn column_to_ir(
        &self,
        column: &ColumnSchema,
        native_full_type: Option<&str>,
    ) -> IRColumn;

    /// 将整个 TableSchema 转换为 IRTable。
    /// 默认实现逐列调用 column_to_ir。
    fn table_to_ir(
        &self,
        schema: &TableSchema,
        full_types: Option<&std::collections::HashMap<String, String>>,
    ) -> IRTable {
        let columns = schema.columns.iter().map(|c| {
            let ft = full_types.and_then(|m| m.get(&c.name)).map(|s| s.as_str());
            self.column_to_ir(c, ft)
        }).collect();
        IRTable {
            name: schema.table_name.clone(),
            columns,
            primary_keys: schema.primary_keys.clone(),
        }
    }
}

/// 目标库适配器：IR → 原生 DDL
pub trait SyncTargetAdapter: Send + Sync {
    /// 将 IR 类型渲染为本库的 DDL 类型字符串。
    fn ir_type_to_native(&self, ir_type: &IRType) -> String;

    /// 将 IR 默认值渲染为本库的 DEFAULT 子句内容。
    /// 返回 None 表示跳过默认值（如序列型由本库机制处理）。
    fn format_default(&self, default: &IRDefault) -> Option<String>;

    /// 将运行时 Value 格式化为本库的 SQL 字面量。
    fn format_literal(&self, value: &Option<Value>, ir_type: &IRType) -> String;

    /// 本库的标识符引用字符。
    fn quote_char(&self) -> char { '"' }

    /// 引用标识符。
    fn quote_ident(&self, name: &str) -> String {
        let q = self.quote_char();
        if q == '`' {
            format!("`{}`", name.replace('`', "``"))
        } else {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
    }

    /// 是否支持 PRIMARY KEY 约束。Trino 等 OLAP 库可能不支持。
    fn supports_primary_key(&self) -> bool { true }

    /// 自增语法。None 表示不支持 / 不使用。
    fn auto_increment_keyword(&self) -> Option<&str> { None }
}
```

### 4.2 Adapter Registry

```rust
/// 文件：src-tauri/src/sync/adapter_registry.rs

use super::adapter::{SyncSourceAdapter, SyncTargetAdapter};
use crate::db::DatabaseType;
use std::collections::HashMap;
use std::sync::Arc;

pub struct SyncAdapterRegistry {
    source_adapters: HashMap<DatabaseType, Arc<dyn SyncSourceAdapter>>,
    target_adapters: HashMap<DatabaseType, Arc<dyn SyncTargetAdapter>>,
}

impl SyncAdapterRegistry {
    pub fn new() -> Self { /* ... */ }

    pub fn register_source(&mut self, db_type: DatabaseType, adapter: Arc<dyn SyncSourceAdapter>) { /* ... */ }
    pub fn register_target(&mut self, db_type: DatabaseType, adapter: Arc<dyn SyncTargetAdapter>) { /* ... */ }

    /// 同一类型可同时作为 source 和 target
    pub fn register_both(
        &mut self,
        db_type: DatabaseType,
        source: Arc<dyn SyncSourceAdapter>,
        target: Arc<dyn SyncTargetAdapter>,
    ) { /* ... */ }

    pub fn get_source(&self, db_type: &DatabaseType) -> Option<Arc<dyn SyncSourceAdapter>> { /* ... */ }
    pub fn get_target(&self, db_type: &DatabaseType) -> Option<Arc<dyn SyncTargetAdapter>> { /* ... */ }
}
```

---

## 五、各 Adapter 实现要点

### 5.1 PostgreSQL Adapter

```
文件：src-tauri/src/sync/adapters/postgresql.rs

SyncSourceAdapter::column_to_ir:
  - 优先使用 native_full_type（来自 format_type()，含精度）
  - "character varying(N)" → Varchar { length: Some(N) }
  - "numeric(P,S)" → Decimal { precision: P, scale: S }
  - "boolean" / "bool" → Bool
  - "integer" / "int4" → Int32
  - "bigint" / "int8" → Int64
  - "text" → Text
  - "jsonb" / "json" → Json
  - "uuid" → Uuid
  - "bytea" → Blob
  - "timestamp with time zone" → Timestamp { with_timezone: true }
  - "inet" → Varchar { length: Some(45) }  // 有损降级
  - "money" → Decimal { 19, 2 }
  - "xxx[]" → Json  // 数组降级
  - 其余 → Other(原文)

  默认值处理:
  - "nextval(...)" → 跳过（auto_increment）
  - "now()" / "CURRENT_TIMESTAMP" → IRDefault::CurrentTimestamp
  - "'value'::text" → IRDefault::Literal("'value'")  // 去 :: 后缀
  - 其余 → IRDefault::RawExpression

SyncTargetAdapter:
  - Bool → "boolean"
  - Int32 → "integer"
  - Int64 → "bigint"
  - Varchar { Some(n) } → "character varying(N)"
  - Text → "text"
  - Json → "jsonb"
  - Uuid → "uuid"
  - Blob → "bytea"
  - Other → "text" (降级)

  format_literal:
  - Bool: TRUE / FALSE
  - Bytes: '\x...' 格式
  - 其余: 标准 SQL 字面量

  format_default:
  - CurrentTimestamp → "now()"
  - Literal → 原样
  - RawExpression → Some(原文)

  auto_increment_keyword: None (PG 用 SERIAL / IDENTITY，但同步场景可忽略)
```

### 5.2 MySQL/MariaDB Adapter

```
文件：src-tauri/src/sync/adapters/mysql.rs

SyncSourceAdapter::column_to_ir:
  - "tinyint(1)" → Bool
  - "tinyint" → Int8
  - "smallint" → Int16
  - "int" / "mediumint" → Int32
  - "bigint" → Int64
  - "varchar(N)" → Varchar { length: Some(N) }
  - "char(N)" → Char { length: N }
  - "decimal(P,S)" → Decimal { P, S }
  - "float" → Float32
  - "double" → Float64
  - "datetime" / "timestamp" → Timestamp { with_timezone: false }
  - "json" → Json
  - "enum(...)" / "set(...)" → Text (有损)
  - "blob" 系列 → Blob
  - "binary" / "varbinary" → Binary / Blob
  - "year" → Int16
  - 去除 "unsigned" / "zerofill" 修饰后解析

SyncTargetAdapter:
  - Bool → "TINYINT(1)"
  - Int32 → "INT"
  - Uuid → "CHAR(36)"  // MySQL 无原生 UUID
  - Text → "TEXT"
  - Json → "JSON"
  - Blob → "LONGBLOB"
  - Other → "TEXT"

  format_literal:
  - Bool: "1" / "0"
  - Bytes: X'...' 格式

  format_default:
  - CurrentTimestamp → "CURRENT_TIMESTAMP"

  auto_increment_keyword: Some("AUTO_INCREMENT")
  quote_char: '`'
```

### 5.3 SQLite Adapter

```
文件：src-tauri/src/sync/adapters/sqlite.rs

SyncSourceAdapter::column_to_ir:
  - SQLite 的类型亲和性规则：
    "INTEGER" → Int64
    "REAL" → Float64
    "TEXT" → Text
    "BLOB" → Blob
    "NUMERIC" → Decimal { 0, 0 }
    含 "VARCHAR" / "CHAR" → Varchar
    含 "INT" → Int64
    其余 → Text (SQLite 默认 TEXT 亲和)

SyncTargetAdapter:
  - 所有整型 → "INTEGER"
  - 所有浮点/小数 → "REAL"
  - 所有字符 → "TEXT"
  - Bool → "INTEGER"
  - Blob/Binary → "BLOB"
  - Json → "TEXT"
  - Uuid → "TEXT"
  - Date/Time/Timestamp → "TEXT"

  auto_increment_keyword: Some("AUTOINCREMENT")
  supports_primary_key: true
```

### 5.4 Trino/Presto Adapter

```
文件：src-tauri/src/sync/adapters/trino.rs

SyncSourceAdapter::column_to_ir:
  - "boolean" → Bool
  - "tinyint" → Int8
  - "smallint" → Int16
  - "integer" → Int32
  - "bigint" → Int64
  - "real" → Float32
  - "double" → Float64
  - "decimal(P,S)" → Decimal { P, S }
  - "varchar" / "varchar(N)" → Varchar { length }
  - "char(N)" → Char { length }
  - "varbinary" → Blob
  - "date" → Date
  - "time" / "time with time zone" → Time
  - "timestamp" / "timestamp with time zone" → Timestamp
  - "json" → Json
  - "uuid" → Uuid
  - "array(...)" / "map(...)" / "row(...)" → Json (降级)

SyncTargetAdapter:
  - Bool → "boolean"
  - Int32 → "integer"
  - Text → "varchar"  (Trino 无 TEXT，用 unbounded varchar)
  - Blob → "varbinary"
  - Uuid → "uuid" (Trino 原生支持)
  - Other → "varchar"

  supports_primary_key: false (大多数 Trino connector 不支持)
  auto_increment_keyword: None
```

---

## 六、sync.rs 重构要点

### 6.1 需要修改的函数

| 现有函数 | 改动 | 详情 |
|---------|------|------|
| `pg_type_to_mysql()` | **删除** | 逻辑拆入 PG 和 MySQL adapter |
| `mysql_type_to_pg()` | **删除** | 同上 |
| `map_default_for_target()` | **删除** | 合入各 adapter 的 `column_to_ir` 和 `format_default` |
| `pg_full_column_types()` | **保留** | PG adapter 的 `to_ir` 需要 full types，此函数移至 PG adapter 或保留为辅助 |
| `format_value()` | **删除** | 替换为 `tgt_adapter.format_literal()` |
| `build_create_table_ddl()` | **重构** | 接收 `&IRTable` + `&dyn SyncTargetAdapter`，不再需要 src_type/tgt_type |
| `sync_table()` | **修改** | 调用 adapter registry 获取适配器 |
| `sync_tables()` | **修改** | 同上 |

### 6.2 新的 build_create_table_ddl 签名

```rust
fn build_create_table_ddl(
    ir_table: &IRTable,
    tgt: &dyn SyncTargetAdapter,
) -> String {
    let q = |name: &str| tgt.quote_ident(name);
    let cols: Vec<String> = ir_table.columns.iter().map(|c| {
        let mut def = format!("  {} {}", q(&c.name), tgt.ir_type_to_native(&c.ir_type));
        if !c.nullable { def.push_str(" NOT NULL"); }
        if c.is_auto_increment {
            if let Some(kw) = tgt.auto_increment_keyword() {
                def.push_str(&format!(" {kw}"));
            }
        }
        if let Some(ref d) = c.default_expr {
            if let Some(s) = tgt.format_default(d) {
                def.push_str(&format!(" DEFAULT {s}"));
            }
        }
        def
    }).collect();

    let mut ddl = format!("CREATE TABLE {} (\n{}", q(&ir_table.name), cols.join(",\n"));
    if tgt.supports_primary_key() && !ir_table.primary_keys.is_empty() {
        let pk_cols: Vec<String> = ir_table.primary_keys.iter().map(|k| q(k)).collect();
        ddl.push_str(&format!(",\n  PRIMARY KEY ({})", pk_cols.join(", ")));
    }
    ddl.push_str("\n)");
    ddl
}
```

### 6.3 新的同步主流程（伪码）

```rust
// 在 sync_table / sync_tables 中
let adapter_registry = &state.sync_adapters;  // SyncAdapterRegistry 挂到 AppState
let src_adapter = adapter_registry.get_source(&src_type)
    .ok_or("Source database type does not support sync")?;
let tgt_adapter = adapter_registry.get_target(&tgt_type)
    .ok_or("Target database type does not support sync")?;

// 获取源表 schema + full types（如适用）
let src_schema = src_driver.get_table_schema(&src_handle, &table_name).await?;
let full_types = /* PG adapter 内部处理，或外部统一获取 */;

// 源 → IR
let ir_table = src_adapter.table_to_ir(&src_schema, full_types.as_ref());

// IR → 目标 DDL
let create_ddl = build_create_table_ddl(&ir_table, tgt_adapter.as_ref());

// 数据迁移（Value 已是 IR，只需格式化字面量）
for batch in result.rows.chunks(BATCH_SIZE) {
    let value_sets: Vec<String> = batch.iter().map(|row| {
        let vals: Vec<String> = row.iter().enumerate().map(|(i, v)| {
            tgt_adapter.format_literal(v, &ir_table.columns[i].ir_type)
        }).collect();
        format!("({})", vals.join(", "))
    }).collect();
    // INSERT ...
}
```

---

## 七、文件结构

```
src-tauri/src/
├── sync/                       # 新增模块
│   ├── mod.rs                  # pub mod 声明
│   ├── ir.rs                   # IRType, IRColumn, IRDefault, IRTable
│   ├── adapter.rs              # SyncSourceAdapter, SyncTargetAdapter traits
│   ├── adapter_registry.rs     # SyncAdapterRegistry
│   ├── ddl.rs                  # build_create_table_ddl（通用 DDL 生成）
│   └── adapters/
│       ├── mod.rs              # pub mod + init_sync_adapters()
│       ├── postgresql.rs       # PgSyncAdapter
│       ├── mysql.rs            # MysqlSyncAdapter
│       ├── sqlite.rs           # SqliteSyncAdapter
│       └── trino.rs            # TrinoSyncAdapter
├── commands/
│   └── sync.rs                 # 删除映射函数，调用 adapter registry
├── db/
│   └── mod.rs                  # 不变
└── main.rs                     # AppState 加入 SyncAdapterRegistry
```

---

## 八、分步实施

### Step 1：IR 定义 + Trait（纯新增，零影响）

**文件**：`sync/ir.rs`、`sync/adapter.rs`、`sync/adapter_registry.rs`、`sync/mod.rs`

**内容**：
- 定义 `IRType`、`IRColumn`、`IRDefault`、`IRTable`
- 定义 `SyncSourceAdapter`、`SyncTargetAdapter` traits
- 定义 `SyncAdapterRegistry`

**验证**：`cargo check` 通过

**预计工作量**：~200 行代码

---

### Step 2：PG + MySQL Adapter（从现有代码提取）

**文件**：`sync/adapters/postgresql.rs`、`sync/adapters/mysql.rs`

**内容**：
- 将 `pg_type_to_mysql()` 的解析逻辑提取到 `PgSyncAdapter::column_to_ir()`
- 将 `pg_type_to_mysql()` 的输出逻辑提取到 `MysqlSyncAdapter::ir_type_to_native()`
- 将 `mysql_type_to_pg()` 反向处理
- 将 `map_default_for_target()` 拆分到两个 adapter
- 将 `format_value()` 拆分到两个 adapter 的 `format_literal()`

**验证**：
- 单元测试：对照现有映射规则，验证 `PG原生 → IR → MySQL原生` 和 `MySQL原生 → IR → PG原生` 的结果与当前 `pg_type_to_mysql` / `mysql_type_to_pg` 一致
- 编写 `sync/adapters/tests.rs`，用参数化测试覆盖全部映射

**预计工作量**：~250 行代码 + ~150 行测试

---

### Step 3：重构 sync.rs 使用 Adapter

**文件**：`commands/sync.rs`、`main.rs`

**内容**：
- `AppState` 加入 `sync_adapters: SyncAdapterRegistry`
- `main.rs` 初始化时注册 PG / MySQL adapter
- 重构 `build_create_table_ddl()` 使用 IR
- 重构 `sync_table()` / `sync_tables()` 使用 adapter
- 删除 `pg_type_to_mysql()`、`mysql_type_to_pg()`、`map_default_for_target()`、`format_value()`

**验证**：
- 现有 e2e 测试通过（`e2e:core` 中的 sync 相关测试）
- PG ↔ MySQL 同步行为不变

**预计工作量**：~100 行修改

---

### Step 4：新增 SQLite + Trino Adapter

**文件**：`sync/adapters/sqlite.rs`、`sync/adapters/trino.rs`

**内容**：
- SQLite adapter：基于 SQLite 类型亲和性规则
- Trino adapter：覆盖 Trino/Presto 的标准类型
- `init_sync_adapters()` 中注册新 adapter

**验证**：
- 单元测试覆盖关键类型映射
- 集成测试（如有本地 Trino 环境）

**预计工作量**：~200 行代码 + ~100 行测试

---

### Step 5：通用 DDL 生成器 + pg_full_column_types 改造

**文件**：`sync/ddl.rs`

**内容**：
- 将 `build_create_table_ddl` 从 `sync.rs` 移至 `sync/ddl.rs`
- `pg_full_column_types()` 改为 PG adapter 内部的辅助方法，或移至 `sync/adapters/postgresql.rs`

**验证**：编译通过 + 现有测试通过

**预计工作量**：~50 行

---

## 九、测试策略

### 9.1 单元测试（新增）

```
src-tauri/src/sync/adapters/tests.rs

// 参数化测试示例
#[test]
fn pg_to_ir_varchar() {
    let adapter = PgSyncAdapter;
    let col = ColumnSchema { name: "name".into(), data_type: "character varying".into(), ... };
    let ir = adapter.column_to_ir(&col, Some("character varying(255)"));
    assert_eq!(ir.ir_type, IRType::Varchar { length: Some(255) });
}

#[test]
fn ir_roundtrip_pg_mysql() {
    // PG 原生 → IR → MySQL 原生，对照旧函数结果
    let pg = PgSyncAdapter;
    let mysql = MysqlSyncAdapter;
    let ir = pg.column_to_ir(&col, Some("character varying(100)"));
    let mysql_type = mysql.ir_type_to_native(&ir.ir_type);
    assert_eq!(mysql_type, "VARCHAR(100)");
}
```

测试覆盖的关键映射路径：

| PG | IR | MySQL | SQLite | Trino |
|----|-----|-------|--------|-------|
| `character varying(N)` | `Varchar{N}` | `VARCHAR(N)` | `TEXT` | `varchar(N)` |
| `boolean` | `Bool` | `TINYINT(1)` | `INTEGER` | `boolean` |
| `integer` | `Int32` | `INT` | `INTEGER` | `integer` |
| `bigint` | `Int64` | `BIGINT` | `INTEGER` | `bigint` |
| `numeric(P,S)` | `Decimal{P,S}` | `DECIMAL(P,S)` | `REAL` | `decimal(P,S)` |
| `text` | `Text` | `TEXT` | `TEXT` | `varchar` |
| `jsonb` | `Json` | `JSON` | `TEXT` | `json` |
| `uuid` | `Uuid` | `CHAR(36)` | `TEXT` | `uuid` |
| `bytea` | `Blob` | `LONGBLOB` | `BLOB` | `varbinary` |
| `timestamp with time zone` | `Timestamp{tz:true}` | `DATETIME` | `TEXT` | `timestamp with time zone` |

### 9.2 回归测试

- 现有 `e2e:core` sync 相关用例必须通过
- 手动验证 PG → MySQL、MySQL → PG 同步结果与重构前一致

### 9.3 未来扩展测试

- 添加新库类型时，只需在 `adapters/tests.rs` 中增加该库的映射断言

---

## 十、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| IR 表达力不足 | 某些专有类型降级后数据语义丢失 | `Other(String)` 作为 escape hatch；日志告警降级事件 |
| 精度解析出错 | DDL 类型字符串格式多变 | 参数化测试覆盖所有已知格式变体 |
| DDL 语法差异 | 不同库的 CREATE TABLE 语法不同 | `SyncTargetAdapter` 提供 `supports_primary_key` 等能力标志 |
| 回归风险 | 重构后行为与原有映射不一致 | Step 2 用 roundtrip 测试对齐旧函数；Step 3 前后 e2e 对比 |
| full_types 获取时机 | PG 需要额外查询获取精确类型 | 保留 `pg_full_column_types` 辅助函数，PG adapter 使用 |

---

## 十一、后续演进方向

1. **Schema diff + 增量同步**：IR 层可扩展支持 `ALTER TABLE` 语句生成（对比两个 IRTable 的 diff）
2. **值转换层**：部分类型可能需要值层面的转换（如 PG `JSONB` 的二进制格式 vs MySQL `JSON` 的文本格式），可在 `SyncTargetAdapter` 中增加 `transform_value()` 方法
3. **可视化映射配置**：前端可展示 IR 映射表，允许用户自定义覆盖某些映射规则
4. **并行同步**：IR 层与数据传输层解耦后，更容易实现表级别的并行同步
