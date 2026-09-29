# 数据库驱动层

> Source of truth: `packages/driver-api/` 与 `packages/drivers/*`。

DataZen 的 Driver 是**编译期集成**的数据库实现，不通过运行时 Rust 动态库 ABI 加载。

## 1. Driver API

`packages/driver-api/src/` 提供 Host 与 Driver 之间的公共契约。

核心接口：

- `DatabaseDriver`
- `DatabaseDriverFactory`
- `DriverCommandDefinition`
- `MigrationRenderer`
- `MigrationCapabilities`
- `TypeNormalizer`
- `SyncSourceAdapter` / `SyncTargetAdapter`

公共 API 不暴露 sqlx、mongodb、redis 等数据库实现类型；具体连接池、Row、事务实现由 Driver 自己管理。

当前 API：

- `PROTOCOL_VERSION = 3`
- `MIN_PROTOCOL_VERSION = 1`

## 2. DatabaseDriver

Driver 基础能力包括：

- connect / test_connection / disconnect
- get_databases / get_tables / get_table_schema
- query / query_multi / query_stream
- query_with_params / execute
- transaction
- EXPLAIN
- Driver Commands

查询流支持 opaque `QueryExecutionId` 生命周期：

```text
prepare_query_execution
       ↓
query_stream_with_execution
       ↓
cancel_query_with_execution
       ↓
cleanup_query_execution
```

只有实际声明精确取消能力的 Driver 才会被 Host 当作 cancellable；兼容默认实现不会自动获得取消能力。

### 2.1 database 维度的两套契约（重要）

`database` 在两个方法族里语义不同，调用方必须区分：

| 方法 | database 维度 | 谁负责解析 |
| --- | --- | --- |
| `get_tables(handle, database)` / `get_all_columns(handle, database)` | **显式参数** | 驱动自己（PG 对非 active 库临时开 pool 后关闭） |
| `get_table_schema(handle, table)` / `get_columns(handle, table)` | **无参数**，隐含依赖会话 active 库 | **调用方必须先 pin** |

Host 侧统一用 `ConnectionManager::ensure_active_database`（命令层包装为 `ensure_session_database`）在读取前 pin 会话；漏掉就会从别的库拿到答案（详见 [cache.md](cache.md) §1.8）。彻底解法是给后两个方法补上 database 参数，但那属于 Driver API 契约变更（`PROTOCOL_VERSION` + 所有驱动同步），目前以"调用方 pin + 驱动不静默"为约定。

### 2.2 分页契约（PaginationSyntax）

Host **不拼任何方言分页子句**：每次读取都向驱动要 `pagination_syntax(limit, offset)`。

| 字段 | 含义 |
| --- | --- |
| `clause` | 追加在 `ORDER BY` 之后的分页子句 |
| `requires_order_by` | 该子句是否要求语句已有 `ORDER BY` |
| `order_by_fallback` | 调用方未指定排序时用于占位的排序表达式 |

- 默认实现输出 `LIMIT {n} OFFSET {m}`；驱动声明 `supports_offset() == false`（`olap`、`superset`）时自动退化为 `LIMIT {n}`。
- `requires_order_by == true` 时，Host 仅在**调用方没有排序**时补 `order_by_fallback`；用户显式排序优先，不叠加兜底排序。
- 调用点：`services/query_executor.rs::build_select_sql`、`data_transfer/execute.rs`、`data_sync/keyset.rs`、`commands/sync/keyset_source.rs`。`data_transfer` 仍以 `supports_offset()` 作为翻页循环的终止条件（不能 OFFSET 的引擎不继续翻页），不参与 SQL 拼接。
- 设置项 `limitSelectResults` 不生成 SQL：它把 `limit` 传给驱动命令，由驱动截断结果集（SQL Server 的做法见 §2.3）。

### 2.3 方言要点：SQL Server

| 主题 | 已实现的行为 |
| --- | --- |
| 分页 | 无 `LIMIT`；`OFFSET {o} ROWS FETCH NEXT {l} ROWS ONLY`，`requires_order_by = true`，`order_by_fallback = (SELECT NULL)` |
| 行数上限 | 命令带入 `limit` 时改写为 `SELECT TOP {n+1}`（多取一行用于判定截断）；语句自带**顶层** `OFFSET` 时不注入 `TOP`——T-SQL 禁止两者同查询（10741），且该语句已由自身 `FETCH NEXT` 界定行数。判定逻辑跳过字符串/注释/标识符并跟踪括号深度，嵌套子查询里的 `OFFSET` 与名为 `offset` 的列都不触发 |
| 批处理专用语句 | `CREATE`/`ALTER` `SCHEMA`·`VIEW`·`PROCEDURE`·`FUNCTION`·`TRIGGER`、会话级 `SET`（`IDENTITY_INSERT`、`SHOWPLAN_TEXT`）与 `BEGIN TRAN`/`COMMIT`/`ROLLBACK` 必须经 `Client::simple_query` 作为真实 batch 发出；走 `sp_executesql` RPC 会分别报 156 / 544 / 266 |
| EXPLAIN | `SET SHOWPLAN_TEXT ON` + 语句 + `SET SHOWPLAN_TEXT OFF` 整批执行；经 RPC 传入会**实际执行**被分析的语句 |
| 时间值 | `date` / `time` / `datetime2` 输出精确文本，`datetimeoffset` 输出带偏移的 RFC3339；不发 Rust Debug 形式（`Date(…)` / `Time { increments: … }`） |
| 参数 | `query_with_params` / `execute_with_params` 通过 Tiberius TDS 参数绑定传值，使用 `@P1` 到 `@P2100`；批处理专用 SQL 不接受参数并显式返回 Unsupported，SQL 文本不会拼入参数值 |
| 结构元数据 | 目录读取保留声明类型长度/精度、主键/索引/外键列顺序、外键动作、CHECK 表达式和列注释；无法由公共模型等价表达的计算/生成列、特殊索引及禁用或不可信约束会明确拒绝，避免静默返回不完整结构。Data Transfer 当前仍拒绝二级索引、外键和 CHECK，避免这些对象在传输计划中丢失 |
| 事务与读取快照 | `begin_transaction`、`commit`、`rollback` 在同一 TDS session 上执行；`begin_read_snapshot` 保存原隔离级别，在 `ALLOW_SNAPSHOT_ISOLATION` 可用时开始 SNAPSHOT 事务，并在提交/回滚后恢复隔离级别。失败的事务收尾会丢弃状态不确定的 session |
| 语句切分 | 多语句按引号/注释感知的扫描器切分（`driver-api::sql_split`），不使用裸 `;` |
| 服务端拒绝的写法 | `FETCH NEXT 0 ROWS ONLY`（10744）、缺少 `ORDER BY` 的 `OFFSET/FETCH`（102）。分页控件对 0 行页给出诊断而不是丢弃子句 |
| 平台限制 | Azure SQL Database：`BACKUP`/`RESTORE` 40510、`msdb` 跨库名 40515、`CREATE LOGIN` 需 master（5001）、serverless 自动暂停首次登录 40613（需退避重试） |

## 3. Driver Commands

Driver 可以通过 `command_definitions()` 声明 Command，通过 `execute_command()` 执行。

Host / Workflow 只依赖 Command Definition 和 JSON input/output，不按 database type 复制 Driver-specific dispatch。

标准 SQL Driver 默认提供 query / execute，以及 schema catalog commands；Driver 可以扩展管理、KV、NoSQL 等 Command。

## 4. Schema Migration

Schema Diff 不直接在 Host 拼接数据库方言。

```text
schema_diff
  ↓
MigrationOperation
  ↓
MigrationRenderer
  ↓
MigrationStatement
```

Driver 同时可提供：

- `MigrationCapabilities`
- `TypeNormalizer`

因此 PostgreSQL/MySQL/SQLite 等差异位于 Driver 层，而不是 UI 或 Schema Diff domain 中。

## 5. Sync / Transfer Adapter

`SyncSourceAdapter` / `SyncTargetAdapter` 服务于异构 Data Transfer：

```text
source schema/value
      ↓
     IR
      ↓
target native type/value
      ↓
target SQL
```

Data Sync 的同族结构门闸和 Data Transfer 的 IR pairing 不应混为一个执行路径。

## 6. Driver Registry

`src-tauri/src/db/registry.rs` 负责 Host 侧 Driver Registry；`src-tauri/src/db/mod.rs` re-export Driver API。

Driver 通过 inventory 在编译期注册。构建时由 registry / build tooling 决定哪些 Driver 被编译进 DataZen。

## 7. Driver 开发

独立 Driver 推荐使用独立 Git repository，通过 DataZen registry 的 `source: "path"` 在本地宿主中调试，发布后可使用固定 commit 的 Git dependency。

详见 [独立驱动开发指南](../../development/independent-driver-development.zh-CN.md)。
