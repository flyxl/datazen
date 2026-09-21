# 数据库 / Schema 维度契约改造方案（RFC）

> [返回架构总览](../README.md) · 状态：**已评审，待实施** · 关联缺陷：BUG-003

## 1. 方案摘要

当前驱动契约是**分裂**的：`get_tables`/`get_all_columns` 把 `database` 作为显式参数，而
`get_table_schema`/`get_columns` **完全没有 database 维度**，只能落在会话当前连接的那个库上。
宿主为了让后者能读到"用户选中的库"，发明了 `use_database` 会话切换（`ensure_session_database`），
于是产生了一个隐式的、池级的、跨请求共享的可变状态。BUG-003（表结构不显示列 / ER 图只有表名 /
数据网格 42 行全空）就是该状态漂移的直接后果。

本方案**消灭这个可变状态**，而不是继续给它打补丁：

1. 所有 schema 读取方法补上 `database` + `schema` 两个显式参数；
2. **删除 `use_database`** 及宿主侧全部会话切换逻辑；
3. PostgreSQL 建立 `(pool_id, database) → PgPool` 映射，把"跨库读"变成"选 pool"；
4. `schema` 由前端显式传入（含 `public`），后端只做校验，不再有隐式默认；
5. 数据库具备显式的"打开/关闭"状态，右键菜单提供"关闭数据库连接"。

## 2. 根因复盘

### 2.1 契约分裂

| 方法 | database 维度 | schema 维度 | 谁负责解析 |
| --- | --- | --- | --- |
| `get_tables(handle, database)` | 显式参数 | **无** | 驱动（PG 临时建 pool） |
| `get_all_columns(handle, database)` | 显式参数 | **无** | 驱动 |
| `get_table_schema(handle, table)` | **无** | **无** | **调用方必须先 pin** |
| `get_columns(handle, table)` | **无** | **无** | **调用方必须先 pin** |

### 2.2 后果链

```
用户选中 winamz_site_medusa → 打开 ER 图
  get_er_data(dbSessionId, database)        ← database 只用于 get_tables（140 张，正确）
    └─ get_table_schema(handle, "api_key")  ← 无 database 维度，落在会话当前库 datazen_demo
         └─ information_schema 查不到 → Ok(TableSchema { columns: [] })
              └─ SchemaCache 以 (conn, database, table) 为键写入空列，TTL 300s
                   ├─ 表结构视图：无列（DDL 视图正常，不走该缓存）
                   ├─ ER 图：只有表名
                   └─ 数据网格：build_select_sql 退化为 SELECT * → 42 行、columns=[] → rowsToRecords 得到 42 个空对象
```

**现象是顺序依赖的**：`datazen_demo` 能正常显示、`v0.2.1` 也"正常"，都是因为"读 schema 时
会话恰好已经 pin 在正确的库上"，与版本无关（`git diff v0.2.1..HEAD` 相关代码完全一致）。

### 2.3 其它同源缺陷（本次一并修）

| 位置 | 问题 |
| --- | --- |
| `packages/driver-api/src/sql_dump/dump.rs:202` | `dump_sql_database_with_progress` 已持有 `database`，但 `dump_one_object` 调 `get_table_schema(handle, tname)` 不传 → 导出别的库会产出空 DDL |
| `packages/driver-api/src/sql_dump/dump.rs:51` | `dump_table_ddl_from_schema` 签名里连 `database` 都没有 |
| `packages/driver-api/src/schema_catalog_commands.rs:85-97` | `get_table_schema` 命令的 `input_schema` 只声明 `table`，无 `database`/`schema` → Workflow / MCP 路径天然无维度 |
| `src-tauri/src/services/db_tools.rs:131,147` | MCP `get_schema` / `describe_table` 无 `database` 参数 |
| `packages/drivers/postgres/src/connection.rs:333-338` | `get_tables_impl` 的 schema 过滤取自 `connect_configs[pool_id].schema`，不是参数 |
| `packages/drivers/postgres/src/connection.rs:37` | 连接串注入 `search_path`，把 schema 变成隐式会话态 |
| `packages/drivers/postgres/src/schema.rs` | 只按 `table_name` 过滤，同名表会把多个 schema 的列**混在一起** |
| `packages/drivers/mysql/src/mysql.rs:618` | `get_all_columns` 在池内连接上 `USE` 后**不切回**，污染归还的连接 |

## 3. 设计决策（已确认）

| # | 决策 |
| --- | --- |
| D1 | 前端显式传 `schema`，**PG 的 `public` 也必须显式传**；后端不做隐式默认 |
| D2 | 有 schema 层级的驱动收到 `None` → 报错；无 schema 层级的驱动收到 `Some` → **报错** |
| D3 | PG 建立 `(pool_id, database) → PgPool` 映射 |
| D4 | 库的打开状态采用**混合方案**：主会话走独立 session，其他库走驱动内 pool 映射 |
| D5 | "关闭数据库连接"前后端都清（后端 pool + 前端该库缓存/tab） |
| D6 | `PROTOCOL_VERSION` **硬切** 3 → 4，所有受影响驱动（含 git 驱动）同步修改 |
| D7 | **删除 `use_database`**；`search_path` 仅保留给用户手写 SQL 的解析默认 |

## 4. 契约定义（`packages/driver-api`）

### 4.1 trait 签名

```rust
async fn get_tables(
    &self,
    handle: &ConnectionHandle,
    database: &str,
    schema: Option<&str>,
) -> Result<Vec<TableInfo>, DriverError>;

async fn get_table_schema(
    &self,
    handle: &ConnectionHandle,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<TableSchema, DriverError>;

async fn get_columns(
    &self,
    handle: &ConnectionHandle,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError>;

async fn get_all_columns(
    &self,
    handle: &ConnectionHandle,
    database: &str,
    schema: Option<&str>,
) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError>;

async fn dump_table_ddl(
    &self,
    handle: &ConnectionHandle,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<String, DriverError>;

// 删除：
// async fn use_database(&self, handle: &ConnectionHandle, database: &str) -> Result<(), DriverError>;
```

### 4.2 能力声明与校验

```rust
/// 该驱动是否存在独立的 schema 层级（PG / SQL Server / DuckDB = true）。
fn has_schema_level(&self) -> bool {
    false
}
```

校验规则（**双向强制**，host 与驱动各校验一次）：

| 驱动能力 | 传入 `schema` | 结果 |
| --- | --- | --- |
| `has_schema_level() == true` | `Some(s)`，`s` 非空 | ✅ 正常 |
| `has_schema_level() == true` | `None` / 空串 | ❌ `InvalidConfig("schema is required for <driver>")` |
| `has_schema_level() == false` | `None` | ✅ 正常 |
| `has_schema_level() == false` | `Some(_)` | ❌ `InvalidConfig("schema is not supported by <driver>")` |

**校验位置：驱动侧强制，host 侧只是提前反馈。** 原因见 §4.4——trait 方法是被所有路径共享的
唯一收口点，MCP / Workflow / driver-api 内部（`sql_dump`）/ Wrapper 插件都不经过
`src-tauri/src/commands/*`。规则实现为 driver-api 的**单一函数**，两边都调它：

```rust
// packages/driver-api/src/traits.rs
pub fn validate_schema_target(
    driver: &(impl DatabaseDriver + ?Sized),
    database: &str,
    schema: Option<&str>,
) -> Result<(), DriverError>;
```

- 驱动实现在方法首行调用 → 覆盖全部调用路径（强制保证）；
- host 调用点在发 IPC 前调用 → 更早、更友好的报错，避免无谓往返（锦上添花）；
- 规则只有一份实现，不存在两边漂移。

`ConnectionConfig.schema` 降级为"连接表单默认值回填 + 手写 SQL 的解析默认"，**不参与工具链**。

### 4.3 为什么保留四个粒度方法（而非合并）

`get_tables` / `get_columns` / `get_table_schema` / `get_all_columns` 是**三档粒度 + 一个批量优化**，
以 PostgreSQL 驱动实测成本为例：

| 方法 | 返回 | PG 查询数 | 用途 |
| --- | --- | --- | --- |
| `get_tables(handle, database)` | `Vec<TableInfo>`（name / schema / table_type / row_count） | **1**（`connection.rs:141` 的 UNION） | 连接树、自动补全、表选择器、导出清单 |
| `get_columns(handle, table)` | 列 + 主键 | **2** | 数据网格列元数据、SchemaCache 快路径 |
| `get_table_schema(handle, table)` | 列 + PK + 索引 + 外键 | **4** | 表结构视图、ER 图、DDL 生成 |
| `get_all_columns(handle, database)` | `HashMap<table,(cols,pks)>` | 批量 1 条（仅 5 驱动实现，其余回退） | 批量场景 N+1 优化 |

成本差达 3 个数量级：列出 140 张表 = 1 条查询；对 140 张表取完整结构 = 560 条查询。
该分层同时映射缓存的两层（`cache.md`：tables 层 + columns 层）。

**不合并的理由**：合并为 `get_table(handle, table, detail)` 会把"列表"退化为"必须指定单表"，
而列表本质是集合操作；且把编译期类型换成运行期枚举开关，收益为负。

**语义差异（决定 schema 参数的强制程度）**：`get_tables` 是**集合**语义（"该库该 schema 下有哪些表"），
`get_table_schema`/`get_columns` 是**单表**语义（必须唯一定位一张表）。后者若 schema 不精确，
就会出现同名表跨 schema **混列**（`postgres/src/schema.rs` 现状），因此 `schema` 对单表方法是硬要求。

### 4.4 校验为什么必须在驱动侧

驱动以 `Arc<dyn DatabaseDriver>` 形式被多个子系统直接持有，Tauri IPC 命令层只是**其中一条**路径：

| 路径 | 调用链 | 过 `commands/*.rs`？ |
| --- | --- | --- |
| GUI IPC | 前端 `invoke` → `commands/schema.rs:533` → `SchemaCache` → `driver.get_table_schema` | ✅ |
| MCP Server | `mcp/tools.rs:168` → `services/db_tools.rs:123` → `resolve_session_for_connection` 自行取 handle → `driver.get_table_schema` | ❌ |
| Workflow | `workflow/command_runtime.rs` → `driver.execute_command` → `driver-api/src/schema_catalog_commands.rs:152` | ❌（在 driver-api 内，位置低于 host） |
| 驱动内部 | `driver-api/src/sql_dump/dump.rs:51,202` | ❌ |
| Wrapper / 插件 | `reuse.rs` 的 `ReuseDriver` 委派给 inner driver | ❌ |
| 其它 host 子系统 | `data_transfer` / `sync` / `schema_diff` / `backup` / `ai/context` 各自 `get_session` 后直接调 | ❌ |

因此 **trait 方法（及 driver-api 共享 helper）是唯一收口点**。

### 4.5 版本策略

`PROTOCOL_VERSION: 3 → 4`（`packages/driver-api/src/lib.rs:73`），硬切，无过渡窗口。
`MIN_PROTOCOL_VERSION` 保持 1，但 v3 驱动会因方法签名不匹配而**编译失败**——这是刻意的：
所有驱动必须与 driver-api 同批升级。

## 5. 标识与生命周期

| 标识 | 生成方 | 生命周期 | 作用 |
| --- | --- | --- | --- |
| `connectionId` | 用户 / Store | 落盘，永久 | 配置、归属、调度 |
| `dbSessionId` | **= `handle.id`**（驱动 `connect()` 生成 UUID） | 内存态；idle 淘汰；**reconnect 后不变** | 宿主会话键、IPC 参数 |
| `pool_id` | **驱动 `connect()` 内生成 UUID** | 驱动内部；**reconnect 后换新值** | 驱动内 pool 表的键 |

链路：`connect(connectionId)` → `establish_connection` → `driver.connect(&config)` →
`ConnectionHandle { id, pool_id }` → `db_session_id = handle.id` →
`ActiveSession { handle }` 存入 `connections`。

后续每次调用 `get_session(dbSessionId)` 都把**同一个 handle 原样回传**驱动，
因此 `(pool_id, database)` 在每次调用时天然可得，**无需新增任何管线**。

> ⚠️ `pool_id` 在 reconnect 后会变（`connection_manager.rs:424-426`）。
> 驱动内缓存用 `(pool_id, database)` 并随 pool 替换清理；
> 宿主侧"库是否已打开"的状态用 `(db_session_id, database)`（跨 reconnect 稳定）。

## 6. PostgreSQL：库 → pool 映射

```rust
/// 库 → pool 映射。key = (pool_id, database)
database_pools: RwLock<HashMap<(String, String), PgPool>>,
```

| 维度 | 方案 |
| --- | --- |
| key | `(pool_id, database)`；`database` 等于连接默认库时**复用主 pool** |
| 获取 | `pool_for(handle, database)`：命中即用；未命中则用 `pool_for_named_database` 建立并缓存 |
| 上限 | 每 handle 最多 **8** 个库 pool；每 pool `max_connections = 2`、`min = 0`；LRU 淘汰 |
| 销毁 | `disconnect` 时按 `pool_id` 清空并 `close()`；`close_database(db_session_id, database)` 单独销毁 |
| 事务 | PG 事务绑定在 pool 的某条连接上 → **跨库事务不支持**，事务中请求其他库须返回明确错误 |
| 顺带收益 | 修掉 `get_tables_impl` 现有"临时 pool 建完即关"的抖动（ER 图 140 张表 = 140 次建连） |

## 7. 库的打开 / 关闭状态（混合方案）

**主会话**（连接默认库）：沿用现有形态——`connect(connectionId)` 建立的 session。

**其他库**（连接树里展开/打开）：走驱动内 `database_pools` 映射，**不新增 session**。

| 动作 | 后端 | 前端 |
| --- | --- | --- |
| 打开库 | 首次 schema/表读取时惰性建 pool 并缓存 | 标记该库为已打开；`dbTablesMap[dbSessionId::dbName]` 缓存 |
| 关闭库（右键"关闭数据库连接"） | 新 IPC `close_database(db_session_id, database)` → 销毁该库 pool + 清相关缓存条目 | 清 `dbTablesMap` / schemaStore 该库条目 / 该库已打开 tab（有未保存编辑态需提示） |
| 断开连接 | 现有 `disconnect` / `release_connection`，清空全部库 pool | 清全部 |

现状说明：`connect_dedicated(connectionId, database)` + `ref_counts` + `release_connection`
已经提供"按库开独立 session"的能力（前端 `src/lib/dedicatedDbSession.ts` 正用于
Transfer / Sync / Schema-Diff 端点），本方案**保留**该能力，但连接树浏览不再为每个库开 session。

## 8. 各驱动路由矩阵

| 驱动 | database 维度 | schema 维度 | 元数据读 | 残留会话态 |
| --- | --- | --- | --- | --- |
| **PostgreSQL** | pool 映射 `(pool_id, db)` | 显式内联 `"schema"."t"` | 选 pool + `WHERE table_schema = $n` | **无** |
| **DuckDB** | 同 PG（catalog / ATTACH） | 显式内联 | 同上 | 无 |
| **MySQL / MariaDB** | 内联 `` `db`.`t` `` | 不支持（`None`） | `information_schema` + `WHERE TABLE_SCHEMA = ?`，**去掉 `USE`** | 无 |
| **ClickHouse** | 内联 `` `db`.`t` `` | 不支持（`None`） | 系统表带库名过滤；**不再把库名当 schema 回填** | 无 |
| **SQL Server** | 内联 `[db].[schema].[t]`（DML） | 显式内联 `[schema].t` | 三段式 + 目录视图显式过滤 | **仅可编程对象 DDL** 走专用连接 |
| **SQLite** | ATTACH 别名 `alias.t` | 不支持（`None`） | `sqlite_master` / `PRAGMA` 带库名 | 无 |
| **Redis / MongoDB / HBase / InfluxDB / ES / Vector / VictoriaMetrics / rqlite / Turso** | 连接级（handle 已绑定） | 不支持 | 不适用 | 无 |

**必须保留 `USE` 的例外**（已确认）：T-SQL 的 `CREATE VIEW / PROCEDURE / FUNCTION / TRIGGER`
对象名不允许带库名前缀，必须在目标库上下文执行。做法：使用**专用连接**（不归还主池，或归还前恢复），
禁止在主池连接上 `USE`。

## 9. 删除清单

| 对象 | 位置 | 处理 |
| --- | --- | --- |
| `use_database` trait 方法 | `driver-api/src/traits.rs:392` | 删除 |
| `use_database` 实现 | 8 个驱动（postgres / mysql / sqlserver / mongodb / clickhouse / kiwi / olap / superset） | 删除 |
| `use_database` 委派 | `driver-api/src/reuse.rs:305` | 删除 |
| `use_database_impl` / `build_use_database_sql` | postgres `connection.rs:368`、mysql `connection.rs:47`、sqlserver `sqlserver.rs:40` | 删除（SQL Server DDL 例外改为专用连接实现） |
| `ensure_session_database` + 12 处调用点 | `commands/query.rs:22` 及 query/driver_command/schema/structure/ai 各处 | 删除 |
| `ensure_active_database` / `set_active_database` | `services/connection_manager.rs:459,484` | 删除 |
| `active_databases` 映射 | postgres / mysql 驱动 | 删除（由 `database_pools` 取代） |
| `search_path` 连接串注入 | `postgres/src/connection.rs:37` | 降级为"手写 SQL 默认"，不参与工具链 |
| `connect_configs[pool_id].schema` 读取 | `postgres/src/connection.rs:333-338` | 改为参数 |
| 库名当 schema 回填 | `clickhouse/src/clickhouse.rs:285` | 改为 `None` |
| `pg_relation_exists` 守卫 | `postgres/src/schema.rs`（上一轮新增） | **保留**——重构期的安全网 |

## 10. 前端改造

| 项 | 内容 |
| --- | --- |
| 能力位来源 | **从后端运行时获取，不在前端重复定义**。`hasSchemaLevel` 加入既有 `DriverCapabilities`（Rust `src-tauri/src/db/registry.rs:19`、TS `src/types/index.ts:56`），由 `DriverCapabilities::from_factory` 读取 `driver.has_schema_level()` |
| 懒加载 | **复用现有机制，无需新 IPC**：`activeConnectionStore.ts:82-98` 已在连接建立时调 `getConnectionInfo(dbSessionId).capabilities`，失败仅 `console.warn`，未知时为 `undefined`，按 `connectionId` 缓存 |
| 顺带清理 | `DatabaseTypeMeta.supportsOffset`（`databaseMeta.ts:117`）注释自承 "mirrors the driver's Rust `supports_offset()`"，属手工镜像 → 一并迁入 `DriverCapabilities` |
| 边界 | 前端保留 **UI 专属**字段（图标、颜色、`connectionForm`、`quoteChar`、`defaultPort`、树渲染语义）；**行为契约**必须由后端拥有（驱动实现就在后端） |
| 连接表单 | PG / SQLServer / DuckDB 的 `connectionIncludesSchema` 改为 `true`（提供默认 schema 回填，PG 默认 `public`）。注意它与 `hasSchemaLevel` **语义不同**：前者是"连接配置是否含 schema 字段"（现仅 olap/Presto/Trino 为 true），后者是"表标识是否为两级命名空间" |
| 传参 | 所有 schema/表读取 IPC 显式带 `schema`（有层级必传，含 `public`；无层级传 `null`） |
| 关闭库 | 连接树右键新增"关闭数据库连接"→ `close_database` + 清 `dbTablesMap` / schemaStore / 该库 tab |
| 分组 | `isSchemaGroupingSchema` 的字符串哨兵判定改为由运行时 `hasSchemaLevel` 驱动 |

> 原"前后端能力位一致性测试"已删除：能力值只由后端定义一次，前端通过 IPC 获取，
> 不存在两份真相。取而代之的是**后端契约测试**（PG / SQLServer / DuckDB = `true`，其余 `false`）。

## 11. 分阶段实施与验收

| 阶段 | 内容 | 规模 |
| --- | --- | --- |
| ① | driver-api 契约（4 改 1 删 + `has_schema_level` + `validate_schema_target` 单一校验函数 + `PROTOCOL_VERSION` 4 + reuse 委派 + `sql_dump` 默认实现 + catalog command input_schema） | 1 crate |
| ② | 参考实现：PostgreSQL（`database_pools` + 显式 schema + 拔掉 search_path/config 依赖）、MySQL（限定名 + `WHERE TABLE_SCHEMA`，零 `USE`） | 2 驱动 |
| ③ | 其余 16 个驱动（18/18 均 override `get_tables` 与 `get_table_schema`，必须逐个改） | 16 驱动 |
| ④ | host：29 处 direct 调用点 + `SchemaCache` 键加 schema + 删除 `ensure_session_database`/`set_active_database` 及 12 处调用点 | `src-tauri` |
| ⑤ | 库打开/关闭：`close_database` IPC + 驱动 `close_database_pool` + 前端树状态与右键菜单 | 前后端 |
| ⑥ | `DriverCapabilities` 扩展（`has_schema_level` + 迁移 `supports_offset`）+ 前端改用运行时能力位 | 前后端 |
| ⑦ | git 驱动同步：`kiwi` / `olap`（钉 ref）/ `superset` 三个独立仓库 + `drivers-registry.json` ref 更新 | 3 仓库 |

**验收标准**：

1. `winamz_site_medusa` 全流程：先开 ER 图 → 表结构显示列 → `cart_address` 42 行且单元格有值（16 列）；
2. 日志中不再出现 `cols=0 (cache)`，也不再有 `session active database switched`；
3. 同一连接下 `datazen_demo` / `winamz_site_medusa` 交替访问结果稳定，与访问顺序无关；
4. 同名表跨 schema（`public.orders` / `other.orders`）列不串；
5. 导出另一个库的 DDL 非空；
6. `cargo test -p datazen --lib`、各驱动 crate 测试、`npx vitest run` 全绿。

## 12. 风险

| 风险 | 缓解 |
| --- | --- |
| PG 每库一个 pool 导致连接数膨胀 | 每 handle ≤ 8 库、每 pool `max=2`、LRU 淘汰、`disconnect` 全清 |
| 事务中请求其他库 | 明确报错（PG 事务绑定 pool 内连接，跨库事务物理不支持） |
| SQL Server DDL 必须 `USE` | 专用连接，不污染主池 |
| git 驱动跟版窗口 | 硬切前先发布各驱动仓库，再更新 `drivers-registry.json` 的 `ref` |
| 重构期出现新的静默空 | 保留 `pg_relation_exists` 守卫 + `SchemaCache` 空列不缓存（视为 miss） |
| 能力位在非 IPC 路径缺失 | 校验实现为 driver-api 单一函数，驱动实现首行调用；不依赖 host 命令层 |

## 13. 实施状态与偏离记录

> 本节记录**实际落地结果**与原方案的差异。原方案正文保留不改，差异以本节为准。

### 13.1 已落地

| 阶段 | 状态 | 证据 |
| --- | --- | --- |
| ① driver-api | ✅ | `PROTOCOL_VERSION = 4`；4 方法加 `database` + `schema`；`use_database` 删除；新增 `has_schema_level` / `default_schema` / `SchemaScope` / `validate_schema_target` / `close_database` |
| ② PG + MySQL | ✅ | PG 105 测试、MySQL 86 测试；PG `database_pools` + LRU；MySQL 全走限定名，零 `USE` |
| ③ 其余 16 驱动 | ✅ | 4 批子代理完成，逐 crate 测试通过（sqlite 46 / redis 131 / mongodb 16 / influxdb 10 / hbase 13 / clickhouse 33 / sqlserver 48 / kiwi 15 / olap 14 / superset 16 …） |
| ④ host | ✅ | `cargo test -p datazen --lib` 1463 passed；`ensure_session_database` / `set_active_database` / `ensure_active_database` 全部删除 |
| ⑤ 库关闭 | ✅ | `close_database` IPC（`commands/connection.rs`）+ 前端 `connectionCommands.closeDatabase` + 连接树右键"关闭数据库连接"（同时清后端 pool 与前端 `dbTablesCache`） |
| ⑥ 能力位 | ✅ | `DriverCapabilities` 增加 `supports_offset` + `has_schema_level`（Rust `db/registry.rs` / TS `types/index.ts`）；前端 `src/lib/driverCapabilities.ts` 消费运行时值；`DatabaseTypeMeta.supportsOffset` 手工镜像删除 |
| ⑦ git 驱动 | ✅ | 三仓库已 push（kiwi `7e927cf` / superset `9690044` / olap `7096c87`）；`drivers-registry.json` 的 `olap` `ref` 已钉到 `7096c873` |
| ⑧ 目标感知查询路径 | ✅ | 新增 `qualified_sql` + `query_at` / `query_multi_at` / `execute_at` / `query_with_params_at` / `query_stream_at`；PG 覆写为按库选池；host 数据网格 / 编辑器 / 导出 / 同步 / 传输 / 备份 / 结构比对全部透传目标 |
| ⑨ 库打开状态 | ✅ | 新增 `open_databases`（driver-api 默认空）+ `get_open_databases` IPC；PG / MySQL 实现；连接树数据库节点显示打开标记 |

### 13.2 与原方案的偏离

1. **`has_schema_level() == true` 只有 PostgreSQL 与 SQL Server**（原方案 §4.2 写作 "PG / SQL Server / DuckDB = true"）。
   DuckDB 的 schema 支持（`ATTACH` 别名 + catalog/schema 两级）作为**显式后续项**保留，
   当前 `false`：把它标成 `true` 会让所有 DuckDB 调用点被迫传 schema，而其元数据读取尚未按两级命名空间改造。
   `default_schema`：PG = `public`，SQL Server = `dbo`。

2. **新增 `fn default_schema(&self) -> Option<&'static str>`**（原方案无此方法）。
   目的：让 `public` / `dbo` 这类"约定"留在拥有它的驱动里，而不是硬编码在 host 或前端。
   它是**最后兜底**（显式 > 表内嵌 > 连接配置 > 驱动约定），只有拿不到任何 schema 的调用方才会用到它。

3. **`validate_schema_target` 增加 `SchemaScope` 参数**（原方案签名只有 3 个参数）。
   列表类方法（`get_tables` / `get_all_columns`）用 `AnySchema`（允许 `None` = 列出全部 schema），
   解析类方法（`get_table_schema` / `get_columns` / `dump_*`）用 `ExactSchema`（必须显式给出）。
   不区分会导致"列表也要 schema"，与"打开库先列全部 schema"的交互冲突。

4. **MySQL 的 `active_databases` 保留但只读**。原方案 §9 要求删掉连接层的 `USE` 助手；
   实际只删了 `current_database` 与 `use_database` 覆写，`execute_text_on_conn` /
   `apply_active_database` 等保留（`active_databases` 在 `connect` 后不再被写入）。
   理由：限制改动半径，且**没有任何读取路径再依赖它**——所有元数据读取都走限定名。

5. **kiwi / superset 的 `get_tables` 仍调用各自的 `set_active_db` / `set_active_context`**。
   这不是元数据读取依赖会话状态（读取一律用显式参数），而是这两个驱动的**查询 API**
   没有 `database` 参数，只能靠会话上下文。已在驱动内注释说明。

6. **前端 `relationSchemaFor` 采用"未知即保留"语义**：
   只有 `hasSchemaLevel === false` 时才丢弃 schema。若能力位尚未加载（`unknown`）就丢弃，
   schema-aware 引擎会退回默认 schema 而**静默读到错误结构**——这正是本次要修的缺陷类型；
   反之，给无层级驱动多传一个 schema 只会**显式报错**。两害相权取轻。

7. **host 侧补了一层兜底解析**：`commands/schema.rs::resolve_metadata_schema`。
   前端对 schema-aware 引擎总会显式传 schema，但 MCP / AI / Workflow 等调用方拿不到，
   该兜底把 `None` 解析为"连接配置 schema → 驱动约定"，并识别 `sales.orders` 这类表内嵌 schema。

8. **`close_database` 的语义**：只释放该库的驱动侧资源 + 清该库全部 schema 缓存，
   **不动会话**；后续读取按需重新建池。驱动无 per-database 资源时返回 `false`。

### 13.3 未验证项

- 三个 git 驱动（kiwi / olap / superset）的**真实 HTTP / SQL 文本路径**无实库可验，仅编译 + 单测通过。
- SQL Server 的三段式 catalog 名、`INFORMATION_SCHEMA.COLUMNS` + `sys.*` 联表、schema 过滤未在实库验证。
- 三个 git 驱动均**已可编译**：`Cargo.toml` 统一改为 `path = "../../driver-api"`（仅克隆进
  `packages/drivers/<id>/` 时可解析，与 kiwi / superset 一致），`drivers-registry.json` 的 `ref`
  已钉到契约修订版。
- `datazen-driver-api` 的**发布通道**：`publish-driver-api.yml` 只发 crates.io，
  `github.com/flyxl/datazen-driver-api` 仓库（API 0.0.8 / `0b12d0a`）无人维护 —— 后续若要恢复
  独立发布，需要先决定该仓库的归属。

### 13.4 第三轮：目标感知查询路径与库打开状态

第一轮（①–④）把**元数据**读取改成了显式 `(database, schema)`，但**数据**路径仍调用
`driver.query(handle, sql)` 这类无目标签名，于是：

- PostgreSQL 永远用 `Self::get_pool(&pools, handle)`（句柄自己的库）→ 只能看到第一个库的数据；
- MySQL 不把库名内联进未限定表名 → 连接未选库时报 `1046 (3D000): No database selected`。

两者同根：**目标没有到达驱动的查询路径**。修复方式是在 `DatabaseDriver` 上补一组
目标感知方法，默认实现"改写后委托"（MySQL 因此免费修好），需要按库选池的驱动覆写：

| 方法 | 默认实现 | PostgreSQL 覆写 |
| --- | --- | --- |
| `qualified_sql(sql, target)` | 无目标时原样返回，否则 `qualify_sql_target` | 继承（只内联 schema） |
| `query_at` | `qualified_sql` → `query` | `pool_for_target(database)` 选池 |
| `query_multi_at` | 同上 | 同上 |
| `execute_at` | 同上 | 同上 |
| `query_with_params_at` | 同上 | 同上 |
| `query_stream_at` | 同上 | 同上 |

配套语义（**新增，原方案未定义**）：

1. **事务与目标的一致性**：PG 事务绑定单条连接（即句柄自己的库）。在事务内请求**其他库**
   返回 `DriverError::TransactionError`，而不是静默读到错误的 catalog。无事务时同一条语句
   走目标库自己的池，**不报错**。
2. **`SqlTarget::new(database, schema)`**：空串 / 纯空白一律视为"未给出"，避免"空选择"
   被解释成"会话默认"。
3. **`open_databases`**：驱动报告"当前仍持有打开资源"的库集合（PG = 句柄自己的库 + 缓存的外库池；
   MySQL = `active_databases`；其余默认空）。前端据此在连接树数据库节点上渲染打开标记；
   `null`（驱动不上报）**不渲染任何标记**，而不是谎称"已关闭"。
4. **未知库报错**：MySQL 的 `get_tables` 对不存在的库此前返回空列表，与 PG 的报错行为不一致；
   现在也在 `information_schema.TABLES` 为空时回查 `SCHEMATA`，未知库报 `Unknown database`。

实库验证（本地 PG 17.9 + MySQL 8，均为门控集成测试）：

- `packages/drivers/postgres/tests/postgres_cross_database.rs`：带目标读外库成功、
  无目标读同一语句失败、事务内跨库报 `TransactionError`、`open_databases` 随关闭收敛。
- `packages/drivers/mysql/tests/mysql_cross_database.rs`：新增
  `connection_without_a_default_database_needs_an_explicit_target` —— 无默认库连接下
  未限定语句**复现 `1046`**，带目标语句成功。

### 13.5 第四轮：前端读取路径丢 schema 与 ER 图取数

E2E 定位到第一轮修复的**漏洞面**：契约与宿主都支持 `(database, schema)`，但前端有若干读取
路径仍然只传 `(dbSessionId, table, database)`，**丢掉了 schema**。此时 `metadata_schema()`
的优先级会回落到 `config.schema`（E2E 里是 `e2e_worker_0`），于是查表报
`Table '...' does not exist`，而同一张表的 `get_table_data`（传了 `public`）却正常——
这正是用户报的"表结构里没有 columns / 数据行全是空"。

修复：**凡是读取路径都必须显式携带关系自身的 schema**（`public` 也要显式传，不做"省略即默认"
的推断）。共 3 处调用点被补上第 4 个参数（详见
`docs/todo/bug/002-target-aware-query-path.md` §6）。

ER 图是最后一个仍走无 schema 调用的读取路径：`getErData(dbSessionId, database)`。
它与其他读取路径**不同**——ER 图是**库级**的，`resolveTableSchema`（按关系解析）无法复用，
需要"整库用哪个 schema"。最终语义（用户确认）：**取"当前面板"的 schema**：

1. 新增 `panelSchema(panel)`（`src/stores/panelTypes.ts`），把每类面板映射到它所在的作用域
   schema（table / view / query / create-table / db-object）。**不返回 database 兜底**——
   schema 是库**内部**的命名空间，用库名顶替会让 schema 感知驱动解析到错误的命名空间，
   而 schema 无关驱动会直接拒绝该参数。
2. `handleOpenErDiagram` 在**打开时**把该 schema 存进 `ErDiagramPanel.schema`；若带 `focus`
   （连接树右键"ER 图"会传表名），优先取该关系自身的 schema——为此把 `ContentView` 的
   `resolveTableSchema` 透传进 `usePanelHandlers`（`schemaViews` 只覆盖视图，不含表）。
3. 已存在的 ER 面板被重新打开时**跟随当前面板**刷新 schema，不保留旧命名空间。
4. `ErDiagramView` 把它作为第 3 个参数传给 `get_er_data`，并加入 effect 依赖以便切换时重取。

测试：`panelSchema` 单元用例；`usePanelHandlers` 5 条（活动面板 / focus 关系 / 视图兜底 /
null / 重新打开跟随）；`ErDiagramView` 3 条断言 IPC 实参（做过变异校验：去掉第 3 个参数后
3 条全挂）。前端 4605 通过。

### 13.6 上游 `#37` 删除测试的恢复

`7fc55501`（隧道特性）在拆分 `config.rs` / `wapps.rs` / `backup.rs` 的同时把测试体换成了
占位符（`config.rs` 26 → 0、`wapps.rs` 11 → 0、`backup.rs` 15 → 4），提交说明称"后续补回"。
本次从合并前分支恢复，并适配拆分后的布局：

- `ipc_contract_guards` 保持**嵌套模块**：拍平后它的模块级 `const resolve` /
  `const override_path` 会让上游同名的 `let resolve` 变成可反驳的常量模式（E0005）。
- 这些守卫用 `include_str!` 扫描被测模块源码，因此 `SOURCE` 改为拼接两半——
  `backup.rs` + `backup_restore.rs`、`config.rs` + `config_import_and_archive.rs`
  （`restore_sql_file` 与应用数据命令已被拆走）。
- `include_str!("../bootstrap.rs")` → `../bootstrap/run.rs`（bootstrap 已改为目录模块）。
- `import_file_filters` / `export_app_data_to_dest` / `import_app_data_from_source` 改为
  `pub(super)`：`pub use config_import_and_archive::*` 只重导出 public 项，测试已看不到它们。
- `ConnectionConfig` fixture 补上 `#37` 新增的 4 个隧道字段。

宿主测试 1423 → **1466 通过 / 0 失败**（合并前分支为 1463）。
