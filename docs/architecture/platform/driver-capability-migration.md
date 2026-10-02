# P2 阶段：path 驱动的资源能力现状清点、迁移批次与协议版本演进

> **状态：目标设计，尚未实现。**
> 本文清点的是当前代码事实，描述的是 P2 应当如何迁移到目标契约；两者之间尚未落地的部分全部标注为待办，不得当作已实现能力。
>
> - 基线：`8592b0fe1`（2026-09-30）。
> - 代码核对时的工作区 HEAD 为 `e544e0699`（2026-09-30 21:08）。`8592b0fe1` 是 HEAD 的祖先，两者相差 4 个提交；`git diff --stat 8592b0fe1..HEAD -- packages/drivers packages/driver-api` 输出为空，因此本文的全部代码结论对两个基线一致。
> - 配套文档：[连接与资源运行时设计](connection-management.md)（下称「连接管理」）、[平台开发计划](../../development/platform-development-plan.md)、[Driver API 依赖边界](../../development/driver-api-dependency-boundary.md)。
> - 独立开发与发布的约束另见 [可选驱动](../../development/optional-drivers.md) 与 [独立驱动开发指南](../../development/independent-driver-development.zh-CN.md)。

## 0. 阅读须知：本文「当前」列的证据来源

本文的每一格「当前」结论都来自对代码的逐项核对，不是推断。核对范围是 `packages/driver-api/**` 与 `packages/drivers/<id>/**` 在上述基线下的实现，宿主侧只读 `src-tauri/src/db/registry.rs`、`src-tauri/src/ai/registry.rs` 与 `data_transfer` 的版本闸门。

三条硬规则：

1. **不猜测。** 只要某个能力无法从已读代码直接推出，就在矩阵中写「待 P0 验证」，并在 [§9](#9-待-p0-验证清单) 集中列出，绝不用「大概率支持」「应当可以」代替。
2. **能力取驱动当前返回的行为，不是引擎上限。** §5.2 的 `sessionScopedHandles` 问的是 driver 是否返回事务/游标/服务端预处理句柄；Redis 引擎支持 MULTI、DuckDB 引擎支持快照，都不改变「驱动当前返回 `unsupported`」这个事实。引擎能力与驱动能力必须分别记录。
3. **枚举值必须与连接管理 §5.2 逐字一致**（`supported` / `unsupported` / `unknown` / `inPlace` / `requiresReplacement` / `full` / `partial` / `verified` / `perTable` / `perDatabase` / `coordinated`）。本文不新增、不合并、不改名任何取值。

矩阵列数说明：连接管理 §5.2 的表格列出 10 条具名能力，第 11 列 `namespaceShape` 来自 §5.1 对 `ResourceDescriptor` 的必填字段要求（`ResourceDescriptor` 必须包含 `namespaceShape`）。因此矩阵是 **15 个驱动 × 11 列 = 165 格**。§5.1 的 9 个契约操作与 §5.2 的能力枚举在 `packages/driver-api` 中**都还不存在**（`resource.rs` / `session.rs` / `capabilities.rs` 是拟新增文件），本文的所有「当前」值都是从现有 trait 方法的具体实现反推出来的，这也是 P2 的起点形态。

## 1. 驱动清单

### 1.1 path 驱动逐个清点

`drivers-registry.json` 中 `source: "path"` 的条目共 15 个，全部对应 `packages/drivers/` 下的 crate，全部是根 `Cargo.toml` 的 workspace member（`members` 含 `packages/drivers/*`），全部参与 `--drivers=basic|all|stub` 的编译期选型，生成产物 `generated.ts` / `driver_init.rs` / `.driver-features.json` 均 gitignored。

| # | 目录 | crate 名 | registry 条目 | 默认 basic 构建 | 链接期注入 |
| --- | --- | --- | --- | --- | --- |
| 1 | `postgres` | `datazen-driver-postgres` | `postgres` | 是 | inventory |
| 2 | `mysql` | `datazen-driver-mysql` | `mysql` | 是 | inventory |
| 3 | `sqlite` | `datazen-driver-sqlite` | `sqlite` | 是 | inventory |
| 4 | `redis` | `datazen-driver-redis` | `redis`（`tauriPlugin`） | 是 | inventory |
| 5 | `mongodb` | `datazen-driver-mongodb` | `mongodb` | 否（Akulaku SKU） | inventory |
| 6 | `sqlserver` | `datazen-driver-sqlserver` | `sqlserver` | 否 | inventory |
| 7 | `clickhouse` | `datazen-driver-clickhouse` | `clickhouse` | 否 | inventory |
| 8 | `duckdb` | `datazen-driver-duckdb` | `duckdb` | 否 | inventory |
| 9 | `elasticsearch` | `datazen-driver-elasticsearch` | `elasticsearch` | 否 | inventory |
| 10 | `rqlite` | `datazen-driver-rqlite` | `rqlite` | 否 | inventory |
| 11 | `turso` | `datazen-driver-turso` | `turso` | 否 | inventory |
| 12 | `influxdb` | `datazen-driver-influxdb` | `influxdb` | 否 | inventory |
| 13 | `victoriametrics` | `datazen-driver-victoriametrics` | `victoriametrics` | 否 | inventory |
| 14 | `hbase` | `datazen-driver-hbase` | `hbase` | 否 | inventory |
| 15 | `vector` | `datazen-driver-vector` | `vector` | 否 | inventory |

`scripts/resolve-drivers.mjs` 的 `BASIC_DRIVERS` 为 `['postgres', 'mysql', 'sqlite', 'redis']`，默认 `--drivers=basic`；`all` 取遍 registry 里**所有 path 条目**，不取 git 条目。因此 `--drivers=all` 构建出的驱动集合就是上表 15 个，与 §1.3 的出入讨论无关。

`register_driver!` 宏在每个 crate 的 `lib.rs` 里被调用，`inventory::collect!` 在 `packages/driver-api/src/factory.rs` 里收集 `DatabaseDriverFactory`；宿主 `DriverRegistry` 只经 factory 创建，不按驱动名分支。

### 1.2 `http-support`：共享 helper crate，不是驱动

`packages/drivers/http-support` 的 crate 名是 `datazen-driver-http-support`（路径依赖 `../http-support`），依赖 `datazen-driver-api`、`reqwest 0.13.3`、`base64`、`serde_json`，`src/lib.rs` 共 146 行。

- **它没有 `register_driver!`，没有 `DatabaseDriverFactory`，不出现在 `drivers-registry.json` 里**，因此不是驱动，也不参与选型、不产生 `driver_id`。
- 它被 10 个驱动消费：clickhouse、duckdb、elasticsearch、hbase、influxdb、vector、victoriametrics、rqlite、turso、sqlserver。它提供的是 HTTP 客户端构造、基础 URL 拼接、认证头与响应解码一类管道。
- **关于 P2 退出门槛「不同 driver 不共享实现库类型」**：本文采纳的读法是「驱动之间不得共享后端实现库类型」。`http-support` 只提供 `reqwest` 管道，不暴露、也不依赖任何数据库后端类型（ClickHouse 类型、ES document、TDS 协议类型、Influx line protocol 都在各自 crate 内）；`sqlserver` 消费它并不意味着它和 `turso` 共享 SQLite 类型——`turso` 用的是自己的 HTTP 路径。因此 `http-support` 满足该门槛。**但这条读法必须在 P0 写成明确规则并加边界测试**，否则「共享实现库」这条门槛无法判定，见 [§8](#8-依赖边界约束)。

### 1.3 「17 个 path 驱动」这一表述与代码事实的出入

任务口径说「17 个 path 驱动」，代码核对结果是 **15 个**。差的 2 个来源是：

- `packages/drivers/kiwi`（crate `datazen-plugin-kiwi`）与 `packages/drivers/superset`（`register_driver!`，`DriverCategory::Sql`）在工作区里存在，但 registry 里是 `source: "git"`，不是 path；它们被根 `Cargo.toml` 的 `exclude` 排除在 workspace 之外，是 `resolve-drivers.mjs` 克隆后注入的。
- `packages/drivers/olap` 同为 `source: "git"`（registry 钉 `ref` = `7096c8737dbe25ab4529e5c3997c9f67309154d9`），但**当前工作区里没有该目录**，未克隆。

所以「17」= 15 个 path 驱动 + 2 个恰好在本地可见的 git 驱动。本文后续所有矩阵、批次、协议章节都只覆盖 15 个 path 驱动；git 驱动在 [§5](#5-协议版本演进) 单独说明升级机制，但不在能力矩阵内——它们的 `describeResource` 只能在自己的仓库里实现。

## 2. 能力现状总矩阵

### 2.1 矩阵

下表是 P2 的起点形态。取值全部按连接管理 §5.2 的枚举字面量书写；「待 P0 验证」的格在 [§9](#9-待-p0-验证清单) 有对应条目。`transactions` 与 `ddlAtomicity` 在 §5.2 是自由文本范围，这里填的是从实现中读到的实际语义。

本表的语义是遗留 `DatabaseDriver` trait，而不是当前的 `CapabilitySet`：后者逐驱动按真实声明核对过的现状表尚未建立，因此 `data` / `backup` 这类只存在于 `CapabilitySet` 的字段在本表中没有对应列，也不应补列。

| 驱动 | statefulSession | namespaceSwitch | contextObservation | transactionObservation | sessionScopedHandles | resetForReuse | preciseCancel | snapshots | transactions（实际语义） | ddlAtomicity | namespaceShape |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| postgres | `unsupported` | `requiresReplacement` | `partial` | `partial` | `unsupported` | `unsupported` | `supported` | `perDatabase` | `BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY` 快照；无 savepoint；事务独占一个 `PoolConnection`，跨库语句直接拒绝 | `Transactional` | database + schema（默认 `public`） |
| mysql | `unsupported` | `inPlace` | `partial` | `partial` | `unsupported` | `unsupported` | `supported` | `perDatabase` | `SET TRANSACTION ISOLATION LEVEL REPEATABLE READ` + `START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY`；无 savepoint；DDL 隐式提交 | `AutoCommitPerStatement` | database（无 schema 层） |
| sqlite | `unsupported` | `unsupported` | `partial` | `partial` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 文本执行 `BEGIN`/`COMMIT`/`ROLLBACK`；无 savepoint；`begin_read_snapshot` 未实现 | `Transactional` | 单文件 = 单库（`ATTACH` 仅出现在 `#[cfg(test)]`） |
| sqlserver | `unknown` | `unsupported` | `partial` | `partial` | `unsupported` | `unsupported` | `unsupported` | `perDatabase` | `BEGIN TRANSACTION`/`COMMIT`/`ROLLBACK`；提交回滚后按 `restore_isolation` 复原隔离级别；失败即移除客户端；无 savepoint | `Unknown` | database + schema（默认 `dbo`） |
| redis | `unknown` | 待 P0 验证 | 待 P0 验证 | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无（三个事务方法走 trait 默认 `TransactionError`） | `Unknown` | db index（`db0`..`dbN-1`，由 `CONFIG GET databases` 决定） |
| mongodb | `unsupported` | 待 P0 验证 | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无（走 trait 默认 `TransactionError`） | `Unknown` | database 即命名空间（无 schema） |
| clickhouse | `unsupported` | 待 P0 验证 | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无（走 trait 默认 `TransactionError`） | `Unknown` | database（ClickHouse 的 "schema" 就是 database） |
| duckdb | `unknown` | 待 P0 验证 | 待 P0 验证 | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 驱动层未暴露；引擎自身事务能力待 P0 验证 | `Unknown` | 单文件 = 单 catalog；`get_databases` 返回的 `main` / `temp` 是库内 **schema** 名而非 catalog 名（`duckdb.rs:80-98` 的 `catalog_selector` 注释已明确） |
| rqlite | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无 | `Unknown` | 固定为 `main` |
| turso | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无 | `Unknown` | 固定为 `main` |
| elasticsearch | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无 | `Unknown` | 固定为 `default` |
| hbase | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无 | `Unknown` | 固定为 `default` |
| influxdb | `unsupported` | 待 P0 验证 | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无 | `Unknown` | database（`SHOW DATABASES` 实际列举） |
| victoriametrics | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无 | `Unknown` | 固定为 `default` |
| vector | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | `unsupported` | 无 | `Unknown` | 固定为 `default` |

统计：**165 格中 158 格已核实，7 格待 P0 验证**。另有 3 格填的是枚举自带的 `unknown`（sqlserver / duckdb / redis 的 `statefulSession`），它们是有效取值，含义是「不得据此开启需要保证的功能」，P0 需要给出把它收紧成 `supported` 或 `unsupported` 的证据。

### 2.2 读表之前必须知道的三条结构事实

这三条决定了整个矩阵的形状，也决定了 P2 的工作量集中在哪。

**事实一：trait 默认值是 fail-closed 的，但有一个例外。** `packages/driver-api/src/traits.rs` 的默认实现里，`has_schema_level` = `false`（:123）、`has_multi_database` = `false`（:133）、`default_schema` = `None`（:147）、`ddl_atomicity` = `DdlAtomicity::Unknown`（:157）、`begin_transaction`/`commit`/`rollback` 返回 `TransactionError`（:602/:743/:749）、`begin_read_snapshot` 返回 `Unsupported("stable read snapshots are not supported by this driver")`（:734）、`discard_connection` 返回 `Unsupported`（:653）、`cancel_query_with_execution` 返回 `Unsupported("precise query cancellation is not supported for execution {}")`（:794）。**唯一破坏 fail-closed 的是 `cancel_query`**：它没有默认体，是必实现方法，而 15 个 path 驱动里有 13 个把它实现成 `Ok(())`。这是 P2 退出门槛「新增能力缺失不会 no-op 成功」最直接的违反点，见 [§7](#7-unsupported-的拒绝断言要求)。

**事实二：当前没有任何驱动拥有一等固定资源会话。** 逐个读 `connect` 得到的连接模型是：

- postgres：`sqlx` 连接池按 handle 建，每库一个池（`MAX_DATABASE_POOLS_PER_HANDLE = 8`，`DATABASE_POOL_MAX_CONNECTIONS = 2`），外加一个独立的 `control_pool`（max 1 / min 0）供取消用；每次操作 `pool.acquire()`。
- mysql：池化，且**每条语句执行前**都要对刚 acquire 到的连接重跑 `apply_active_database`。
- sqlserver：每个 `pool_id` 恰好一个 `tiberius::Client`，`handle.id == handle.pool_id`，是真实物理 TCP 会话。
- sqlite：`SqlitePoolOptions::max_connections(1).min_connections(1)`，单文件。
- duckdb：`::duckdb::Connection` 包在 `Arc<Mutex<_>>` 里，一个 handle 一个。
- redis：handle 存一条 `RedisConn { plan, live }`；**大多数面向库的操作直接在这条常驻连接上发 `SELECT` 再执行**（`with_live_op!` / `with_live_op_topo!`，见 `mod.rs:33-64`，调用点如 `mod.rs:208`、`:242`、`:264`、`:357`、`:387`），只有元数据读取另开一次性连接（`open_pinned_conn`：`database.rs:122` 的 `get_tables`、`database.rs:146` 的 `get_table_schema`；`mod.rs:313-316` 的全库 `DBSIZE` 统计也另开 `open_live_conn`），以免扰动共享会话。因此「切换 db」在同一驱动里**就地切换与换连接两种机制并存**，矩阵取值待 P0 定案（[§9](#9-待-p0-验证清单) 第 7 条）。
- 其余 9 个（mongodb、clickhouse、rqlite、turso、elasticsearch、hbase、influxdb、victoriametrics、vector）：存 `(client, base)` 元组，**完全无会话**。

事务只在事务存续期间临时占用一条连接（postgres 是 `pool.acquire()` 后把 `PoolConnection` 存进 `self.transactions`；mysql 同理；sqlite 通过 `tx.connection_id` 重建 handle；sqlserver 直接就是那条常驻 client）。所以**不能**把任何驱动标成 `statefulSession: supported`。sqlserver / duckdb / redis 三格写 `unknown` 而不是 `unsupported`，是因为它们确实存在「物理连接在 handle 生命周期内不变」的事实，只是没有任何声明或校验机制把它变成可依赖的保证——见 [§6.1](#61-允许与禁止)。

**事实三：`unsupported` 的驱动大多不是「不支持」，而是「根本没接」。** 8 个驱动（duckdb、elasticsearch、hbase、influxdb、rqlite、turso、vector、victoriametrics）**一个能力方法都没覆盖**，全走 trait 默认。这在 P2 里要分两种处理：真正没有该能力的（HTTP 文档库的 `snapshots`、`transactions`）保留 `unsupported` 并补拒绝断言；引擎支持但驱动没接的（SQLite 快照、DuckDB 事务、Redis 事务）要进批次 1/2 补实现，否则 Host 会把「驱动没接」和「后端不支持」混成同一个错误，无法给出可执行的下一步。

### 2.3 读表方法：`grep` 命中什么 ≠ 这个契约做不到

[§2.2](#22-读表之前必须知道的三条结构事实) 讲矩阵的形状，这一节讲**怎么读出一格**。反复出现的是同一类错误：**看到某个符号、字段或方法名，就断言它的行为**。中间隔着两样东西——`pub`（能读到不代表会被调用）和 **trait 默认实现**（没有覆写的格子走的是通用实现，不是空的）。

判据按这个顺序走：

1. **先问「有没有覆写并拒绝」，再问别的方法。顺序颠倒会误分类。** [§5.6](#56-supportsbackup判据不是有没有覆写-dump) 里的 redis 就是反例：它显式覆写 `dump_database_with_progress` 返回 `NotSupported`，**同时**又有真实实现的 `get_tables` / `get_table_schema`（那两个是 schema 浏览用的）。先看后者会把它误判成「声明 `false` 但其实能做」。
2. **声明与实现之间隔着 `pub` 和默认实现。** 「没写代码」≠「做不到」——通用默认实现可能真能跑；反过来 `NotSupported` 桩也可能被上游接住并降级（[§5.6](#56-supportsbackup判据不是有没有覆写-dump) 的 `dump_view_ddl`）。两个方向都不能只凭「grep 不到 / grep 到」定论。
3. **两套并行的声明不要互推。** UI 的 `supportsBackup`（`packages/drivers/<id>/ui/meta.ts`）与 Rust 的 `CapabilitySet.backup: BackupSupport`（`packages/driver-api/src/capabilities.rs:298`）彼此独立：前者描述 UI 入口是否可达，后者是逐驱动声明的能力。任一方向的推断都是错的。

**这次的真实代价（记下来是因为它已经发生过一次）：** 读到 rqlite / turso 声明 `supportsBackup: true`，而在 Rust 侧「grep 不到 backup 代码」，于是差点落成一句「前端声明了不存在的操作」。**这句是错的。** `dump_database_with_progress` 有通用默认实现（`packages/driver-api/src/traits.rs:1000-1020`），往下经 `sql_dump` 真能跑通——缺的只是「零显式覆写」，不是「能力不存在」。完整推导见 [§5.6](#56-supportsbackup判据不是有没有覆写-dump)。

## 3. 分驱动说明

每节只写已读代码能证明的语义；与连接管理 §5.1 的 9 个契约操作（`describeResource` / `acquireResource` / `executeOnResource` / `observeSession` / `changeContext` / `begin|commit|rollback` / `requestCancel` / `resetResource` / `closeResource）逐条的差距在每节末尾的「与目标契约的差距」中给出。

### 3.1 PostgreSQL（含 questdb / cloudberry 复用）

`packages/drivers/postgres` 一个 crate 注册 3 个 factory：`PostgresFactory`（`driver_id = "postgresql"`）、`QuestDbFactory`（`"questdb"`）、`CloudberryFactory`（`"cloudberry"`）。后两个的 `create()` 返回 `ReuseDriver::new_with_precise_cancel(PostgresDriver, "questdb"|"cloudberry", true)`。三个 factory 的 `supports_explain()` 与 `supports_query_execution_cancel()` **都**返回 `true`（`lib.rs:36,42` / `:60,66` / `:86,92`），但 `supports_cancel_query()` **只有 `postgresql` 返回 `true`**（`lib.rs:39`），`questdb` / `cloudberry` 返回 `false`（`:63` / `:89`）。

- **多库/多 schema**：库级由多池承担，每库一个池，单 handle 最多 8 个；schema 级由 `search_path` 承担。`build_pg_options()` 在**建连时**写入 `.database(resolve_connect_database(config))`，并且只在 `config.schema` 非空时追加 `opts.options([("search_path", format!("\"{clean}\",public"))])`。`fetch_tables_from_pool` 读取时也固定补 `'public'`。这意味着 schema 是**连接建立时固化**的，驱动里没有对活会话发 `SET search_path` 的路径。
- **事务隔离**：`begin_transaction_impl` 为每个 `handle.id` 记一条事务，从该 handle 自己的池里 `pool.acquire()` 一条专用连接执行 `BEGIN`，并把这条连接存进 `self.transactions`。源码注释明确写：事务占用本库池中的一条连接，指向另一个库的语句被拒绝，绝不静默重定向。`begin_read_snapshot_impl` 发 `BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY`。
- **会话作用域句柄**：`prepare_query_execution` 存在，但 `QueryExecutionId` 在 API 文档里被要求保持 opaque，禁止暴露后端 PID/线程号。
- **取消**：`cancel_query` 返回 `Err(DriverError::Unsupported("legacy session-wide query cancellation is disabled; use an execution handle"))`；`cancel_query_with_execution` 走独立的 `control_pool`。这是 15 个驱动里**两个**正确拒绝 legacy cancel 的之一。
- **重置/丢弃**：`discard_connection` 未覆盖，走 trait 默认 `Err(Unsupported(...))`（fail-closed）。
- **多库标识**：`is_active_database` 是只读元数据查询，源码注释写明它**从不被改成用来切换会话**；`close_database_pool` 拒绝关闭 handle 自己的库。

**与目标契约的差距**：`changeContext` 必须返回 `RequiresReplacement` 而不是 `Unsupported`——库与 schema 都能换，只是要换一条连接，这是 `requiresReplacement` 的教科书场景。`resetResource` 目前只能整体丢弃（且本驱动连 `discard_connection` 都没实现），要补一个「回滚未决事务 + 恢复 `search_path` 到 baseline」的实现才能给 `resetForReuse: verified`。`executeOnResource` 要求的「不自行再从随机 pool 取连接」目前不成立：`begin_transaction` 之外的路径都是每次 `pool.acquire()`。

### 3.2 MySQL（含 mariadb / doris / starrocks / manticore / ob_oracle 复用）

`packages/drivers/mysql` 一个 crate 注册 **6 个 factory**：`mysql`、`mariadb`、`doris`、`starrocks`、`manticore`、`ob_oracle`（驱动 id 用的是下划线，见 `lib.rs:142`），全部 `supports_query_execution_cancel() -> true`。

- **切库语义（本矩阵里唯一的 `namespaceSwitch: inPlace`）**：`apply_active_database(handle, &mut conn)` 在**每条语句执行前**对新 acquire 到的连接执行 `USE \`db\``（`build_use_database_sql` 做标识符引用，拒绝空名与含 NUL 的名字），随后调用 `conn.clear_cached_statements()`——因为 MySQL 在 PREPARE 时就解析非限定名，不清缓存会打到错的库。这条路径的定义在 `connection.rs:130`（内部经 `execute_use_on_conn`，在 `connection.rs:103` 清 sqlx 语句缓存），在 `mysql.rs` 里有 9 个调用点（`1300`、`1410`、`1509`、`1603`、`1654`、`1680`、`1708`、`1740`、`1816`），另有一处在 `execution.rs:159`，结论是：**MySQL 的库上下文是每操作重放的，不是会话粘滞的**。
- **上下文观察**：`current_database_on_conn` 用**文本协议**发 `SELECT DATABASE()`，源码注释解释：走 prepared 会返回 PREPARE 时刻的 schema，因此必须文本协议。
- **协议限制**：`execute_text_on_conn` 注释记录 MySQL 会在 prepared 协议上拒绝 `BEGIN`/`COMMIT`/`ROLLBACK`/`USE`/`SET`/DDL/`CREATE PROCEDURE`，错误码 1295 (HY000)。`execute_use_on_conn` 因此固定走文本路径。
- **事务隔离**：`begin_transaction` 先 `apply_active_database` 再 `BEGIN`；`begin_read_snapshot` 发 `SET TRANSACTION ISOLATION LEVEL REPEATABLE READ` + `START TRANSACTION WITH CONSISTENT SNAPSHOT, READ ONLY`，且**失败时关闭该连接**，避免一次性隔离级别泄漏。
- **DDL 原子性**：`ddl_atomicity` = `DdlAtomicity::AutoCommitPerStatement`，与 PostgreSQL 的 `Transactional` 正好构成一对，宿主据此决定 schema-diff 部署是否包事务。
- **取消**：`cancel_query` 返回 `Err(DriverError::Unsupported(...))`（与 postgres 同文案）；`cancel_query_with_execution` 锁住注册表条目直到 `KILL QUERY <thread_id>` 完成，并处理了 thread id 在过早清理后被复用的风险；`cleanup_query_execution` 直接移除注册表条目（`execution.rs:25-38`），`execution.cancel_requested` 的复位则发生在 `cancel_query_with_execution` 的各条失败分支。这是 15 个驱动里**第二个**正确拒绝 legacy cancel 的。
- **重置/丢弃**：`discard_connection` 未覆盖，走默认 `Err(Unsupported(...))`。

**与目标契约的差距**：`inPlace` 只在「同一个活连接上」成立，而 `executeOnResource` 每次都可能拿到池里另一条连接。P2 要么让 `changeContext` 在 MySQL 上也返回 `requiresReplacement`（并把 `inPlace` 收窄到「同一 `ResourceHandle` 已固定的那条连接」），要么引入显式的资源固定。这一取舍必须在 P0 定案并写进契约测试，不能靠 `apply_active_database` 的存在就宣称 `inPlace` 成立。

### 3.3 SQLite

- **连接模型**：`SqlitePoolOptions::max_connections(1).min_connections(1)` 建在 `sqlite:{path}` 上，随后 `PRAGMA journal_mode=WAL`；`ConnectionHandle` 的 `id` 与 `pool_id` 取同一个值。
- **库即文件**：`get_databases` 恒返回 `vec!["main"]`。`physical_database_identity` 读 `PRAGMA database_list` 的 `file` 列得到 `sqlite_file_identity`。迁移路径里出现过 `ATTACH DATABASE '...' AS aux` 与 `"aux".sqlite_master` 限定名，但**该 `ATTACH` 只出现在 `#[cfg(test)]`**（`sqlite.rs:1177`），生产路径没有跨文件附加。因此矩阵里是「引擎能力存在，驱动不支持」。
- **DDL 原子性**：`DdlAtomicity::Transactional`。
- **事务**：`begin_transaction` 走 `execute(handle, "BEGIN")`，产出 `TransactionHandle { id: "sqlite_tx_<uuid>", connection_id: handle.id }`；`commit`/`rollback` 由 `tx.connection_id` 重建 `ConnectionHandle` 后续行，源码注释说明这就是因为 `connect()` 让 `handle.id` 与 `handle.pool_id` 取了同值。
- **未覆盖的方法**：`begin_read_snapshot`、`prepare_query_execution`、`discard_connection` 均未覆盖，分别落到 trait 默认的 `Unsupported`（fail-closed）。
- **取消**：`cancel_query` 只打一条 `tracing::debug!("sqlite: cancel_query is a no-op (single-connection, in-process)")` 然后返回 `Ok(())`。

**与目标契约的差距**：池大小为 1 是本文档最需要守住的一条红线。连接管理 §5.2 明确写「不能因为『池大小为 1』就声明固定会话：底层重建和不同 consumer 仍会破坏语义」。SQLite 的 `max_connections(1)` 完全符合这个反例——底层仍然是 sqlx pool，`handle` 仍然可以被 `disconnect` 之外的路径重建，Host 侧也没有任何基线校验。因此 `statefulSession` 必须保持 `unsupported`。`preciseCancel` 应从现在的「no-op 成功」改为显式 `Unsupported`（见 §7）。

### 3.4 SQL Server

- **连接模型**：`connect_client` 直接用 `TcpStream` + `tiberius::Client::connect`（超时包裹），不走 SQLx。`connect` 每个 `pool_id` 保存**恰好一个** `Client`，`handle.id == handle.pool_id`。
- **命名空间**：对象名一律全限定 `[db].[dbo].[obj]`。单测（`sqlserver.rs:2086`）显式断言没有任何读路径内嵌 `USE [`，并断言空库名时 SQL 保持本地。注释还记录了 T-SQL 没有 `LIMIT`、分页必须写 `OFFSET … ROWS FETCH NEXT … ROWS ONLY` 因而强制 `ORDER BY`。因此**驱动从不发 `USE`**，切库是寻址问题不是会话问题，`namespaceSwitch` 是 `unsupported`（就当前实现而言）。
- **会话级开关**：`explicit_identity_insert_requires_session_toggle` = `true`，`set_identity_insert` 的作用域是物理 SQL Server 会话——这是矩阵之外的一个真实会话级副作用，P0 需要决定它归入 `sessionScopedHandles` 还是 `initializationRequirements`。
- **事务**：`begin_transaction` 先 `ensure_no_open_transaction` 再 `BEGIN TRANSACTION`，失败时移除 client；`begin_read_snapshot` 读取并保存当前隔离级别（`restore_isolation`）；`commit`/`rollback` 校验 `tx.connection_id` 匹配后执行，必要时附 `SET TRANSACTION ISOLATION LEVEL {level}` 复原，**执行失败即 `clients.remove()`**（fail-closed 丢弃）。
- **丢弃**：`discard_connection` 是 15 个驱动里**唯一**实现了的：移除事务条目 + 丢弃 client，返回 `Ok`。
- **取消**：`cancel_query` 返回 `Ok(())` 空实现，且没有覆盖 `cancel_query_with_execution` / `supports_query_execution_cancel`（即默认 `false` + 默认 `Err(Unsupported(...))`）。这是最刺眼的一格：**`preciseCancel` 明确 `unsupported`，而 legacy cancel 却静默「成功」**。
- **DDL 原子性**：未覆盖 → `DdlAtomicity::Unknown`。
- **绑定参数**：`max_bound_parameters` = 2100。
- **测试基座**：`tests/` 下共 9 个 `.rs`——7 个 `live_*.rs`（`live_admin_and_sync`、`live_connection`、`live_pagination`、`live_query_types`、`live_schema_metadata`、`live_streaming`、`live_write_and_ddl`）+ `sync_adapter_smoke.rs` + `command_definitions.rs`，另有 `common/` 辅助模块。它是当前 15 个 path 驱动里真实服务端测试覆盖最厚的。

**与目标契约的差距**：这是最接近目标资源模型、也最需要谨慎的一个驱动。它有真实的单物理会话，有 `discard_connection`，有会话级开关，但**没有声明、没有基线校验、没有 reset 复原**。`resetForReuse` 的枚举只有 `verified` / `unsupported` 两值，本驱动拿不到 `verified`——`discard` 是丢弃不是复原。P2 的工作是把「回滚未决事务 → 复原 `restore_isolation` → 复原 `SHOWPLAN_TEXT`/IDENTITY_INSERT 等会话开关」做成显式 `resetResource`，再由 CM-26/CM-69 验证之后才能填 `verified`。

### 3.5 Redis

- **库即 db index**：`get_databases` 读 `CONFIG GET databases`（失败回退 16），返回 `(0..db_count).map(|i| format!("db{i}"))`。
- **切库**：`parse_db_name` 去掉可选的 `db` 前缀后 `u32::parse`，空串或不可解析直接 `DriverError::QueryFailed("invalid database name (expected e.g. db0)")`；`open_pinned_conn` 克隆 handle 的 plan，**重开**一条 `open_live_conn(&plan)`，再 `select_db(&mut live, db_index)`——这条路径用于元数据枚举，`mod.rs:140-146` 的注释写明这样做是为了让浏览某个 db 不会改动共享会话。但同一个驱动在 `with_live_op!` / `with_live_op_topo!` 里对常驻 `rc.live` 直接 `select_db`（`mod.rs:208`、`:242`、`:264`、`:357`、`:387`），即**就地切换**。两种机制并存，所以本文既不记 `inPlace` 也不记 `requiresReplacement`，改记「待 P0 验证」（[§9](#9-待-p0-验证清单) 第 7 条）。
- **单测**：`get_table_schema_rejects_unparsable_database` 覆盖了拒绝路径。
- **事务**：三个事务方法都未覆盖，走 trait 默认 `TransactionError`。注意 `ops/value_search.rs:264` 有一个 `commit`，它是任务/作业累计器（把完成的批次折进任务总数，单测 `commit_accumulates_until_task_replaced`），**不是数据库事务**，不要在迁移时误当成事务实现。
- **取消**：`cancel_query` 返回 `Ok(())`。
- **依赖**：`redis 0.27`（tokio-comp / aio / cluster-async / sentinel / tls）、`rmpv`、`flate2`、`urlencoding`、`futures-util`，可选 `tauri 2`；不依赖 `http-support`。

**与目标契约的差距**：`contextObservation` 与 `namespaceSwitch` 两格待 P0 验证（[§9](#9-待-p0-验证清单) 第 1、7 条）。`statefulSession` 写 `unknown` 是因为 handle 里确实常驻一条 `live` 连接（面向库的操作就在它上面发 `SELECT`），但没有任何声明或校验机制把「句柄生命周期内连接不变」变成可依赖的保证；P0 需要用 CM-09 判定这条常驻连接在并发下是否可被复用为固定资源。

### 3.6 MongoDB

- `connect` 存 `(client, config.database)`；`get_databases` 调 `client.list_database_names()`。
- `has_multi_database` = `true`；`driver_category` = `DriverCategory::Document`（15 个 path 驱动里只有它与 redis 显式声明分类：`Document` 与 `KeyValue`，其余 13 个走默认 `Sql`；git 驱动 kiwi / superset 另显式声明为 `Sql`）。
- `get_tables` 注释写明 MongoDB 没有 schema 层、**database 就是命名空间**，随后 `validate_schema_target(self, database, schema, SchemaScope::AnySchema)` 再 `client.database(database).list_collection_names()`。
- 寻址方式是 `client.database(name)`——**每次操作按名寻址，没有会话**。

**与目标契约的差距**：`namespaceSwitch` 待 P0 验证（[§9](#9-待-p0-验证清单) 第 2 条）。根因是 §5.2 的 `namespaceSwitch` 四个取值里**没有「无会话、按请求寻址」这一档**：`inPlace` 需要一条会被切换的会话，`requiresReplacement` 需要先有一条会被替换的会话，而 MongoDB 两者都没有。同样的缺口也出现在 clickhouse（库随每个 HTTP 查询下发）和 influxdb（`SHOW DATABASES` 真实枚举、库随请求下发）。**P0 必须先扩展枚举或明确定义映射规则**，否则这 3 个驱动无法给出合法的 `namespaceSwitch` 取值。

### 3.7 ClickHouse

- `PoolEntry { client, base, database }`：库在建连时取自 `config.database` 并随每次查询下发，因此**库不是会话状态**。
- `has_multi_database` = `true`；`get_databases` 查 `SELECT name FROM system.databases ORDER BY name`（真实枚举）。
- `get_tables` 注释：ClickHouse 的 "schema" **就是** database，所以 `namespaceShape` 只有一层。
- `cancel_query` 返回 `Ok(())`；事务三方法未覆盖；`ddl_atomicity` 未覆盖。

**与目标契约的差距**：同 §3.6 的枚举缺口。额外一条：ClickHouse 的 engine 变量、临时表、session 设置都是 HTTP query 参数级的，不构成会话上下文，`contextObservation: unsupported` 是准确的。

### 3.8 DuckDB

- `connect` 用 `::duckdb::Connection::open(path or ":memory:")` 包在 `Arc<Mutex<_>>` 里按 `pool_id` 存，`ConnectionHandle` 的 `id` 与 `pool_id` 同值。一个 handle 一条真连接，这一点与 sqlserver 同构。
- `get_databases` 返回 `vec!["main","temp"]`，但这两个名字**是 schema 名不是 catalog 名**——`duckdb.rs:80-98` 的 `catalog_selector` 注释明确写了这一点，所以 `namespaceShape` 不能按「单文件 = 单库（`main`/`temp`）」记。`ATTACH ':memory:' AS aux` 出现在 `duckdb.rs:831`，同样位于 `#[cfg(test)]` 内；生产路径无跨 catalog 附加。
- 已知方言细节：注释记录 DuckDB 的 `PRAGMA` 解析器拒绝 catalog 前缀（`PRAGMA aux.table_info`），必须写成 `PRAGMA table_info('aux.t')`。
- `cancel_query` 返回 `Ok(())`；事务三方法、`begin_read_snapshot`、`discard_connection`、`ddl_atomicity` 全部未覆盖。

**与目标契约的差距**：`namespaceSwitch` 与 `contextObservation` 两格待 P0 验证（[§9](#9-待-p0-验证清单) 第 3、4 条）。DuckDB 引擎本身的事务与快照能力驱动完全没有暴露，属于「引擎支持、驱动没接」，批次 2 要决定是接还是不接；不接就必须保证 `Unsupported` 是显式拒绝而非静默。

### 3.9 rqlite / turso

两者结构相同：`(client, base)` 无会话元组，`get_databases` 恒返回 `vec!["main"]`，事务/快照/重置全走 trait 默认，`cancel_query` 返回 `Ok(())`。两个 crate **都没有 `sync_adapter` 模块，也没有实现 `SyncAdapter`**；sqlite 兼容的元数据读取留在各自驱动文件内（`rqlite.rs:82-89` / `turso.rs:84-91` 的 `table_info_sql`，拼 `PRAGMA {db}.table_info('t')`）。这是 P2 里**最低成本的一档**：全部能力取 `unsupported`，工作量在于把 `cancel_query` 改成显式拒绝并补契约测试。

### 3.10 Elasticsearch / HBase

同样是无会话 `(client, base)`，`get_databases` 恒返回 `vec!["default"]`，`cancel_query` 返回 `Ok(())`。HBase 另有 namespace 概念（列族/表限定名），但驱动当前不暴露成可切换的命名空间。**elasticsearch 没有 `tests/` 集成测试目录**（真实服务端基线为 0），只有 `src/elasticsearch.rs:516` 与 `src/sync_adapter.rs:128` 两个 `#[cfg(test)]` 模块共 10 个单元测试。**没有 `tests/` 目录的 path 驱动共 7 个**：rqlite / turso / elasticsearch / hbase / influxdb / victoriametrics / vector——批次 3 必须先补最小可运行基线，否则这 7 个驱动的任何能力结论都没有真实协议回归保护。

### 3.11 InfluxDB / VictoriaMetrics

- influxdb：`get_databases` 解析 `SHOW DATABASES` 的 results/series/values，返回真实库列表；库随请求下发 → `namespaceSwitch` 待 P0 验证（§9 第 5 条）。`cancel_query` 返回 `Ok(())`。
- victoriametrics：`get_databases` 恒返回 `vec!["default"]`，`cancel_query` 返回 `Ok(())`。

### 3.12 Vector

`(client, base)` 无会话，`get_databases` 恒返回 `vec!["default"]`，`cancel_query` 返回 `Ok(())`，事务/快照/重置全走默认。与 §3.9 同一档处理。

## 4. 迁移批次

批次划分原则：**先用最少的驱动把契约测试模板跑通并冻结，再按语义相近度扩面**。每个批次内所有驱动的 D 层用例同时转绿才准进下一批；任一驱动未转绿则整批停在原地，不允许「大部分通过」。

**所有 D 层测试的落点固定在驱动 crate 内**（`AGENTS.md` 驱动测试落点表 + 独立驱动开发指南 §8）：Rust 单元写在 `packages/drivers/<id>/src/*.rs` 的 `#[cfg(test)]`，集成测试写在 `packages/drivers/<id>/tests/`，`cargo test -p datazen-driver-<id>`；驱动专属 UI 单测写在 `packages/drivers/<id>/ui/__tests__/`，`pnpm test:unit:drivers`；驱动专属 E2E 写在 `packages/drivers/<id>/e2e/`。**禁止**把这些用例加到 `src-tauri/`、`src/` 或 `e2e/specs/`。

### 批次 0：契约测试模板（不接任何真实驱动）

目标：把「能力断言 + 拒绝断言」写成一份可复制的模板，让后续每个驱动只需填表。

- **范围**：`packages/driver-api` 新增 `capabilities.rs`（能力枚举与 `describeResource` 的返回结构）、`session.rs`、`resource.rs` 三个模块（拟新增），以及驱动侧的能力自检 harness。
- **必做断言**：
  - 每个枚举取值都有一正一反两个用例：合法值原样返回；非法/未覆盖时返回 `Unknown` 而不是猜一个 `supported`。
  - 能力缺失必须走 `DriverError::Unsupported`，**不得**返回 `Ok(())`；已建立执行记录时记 `ExecutionErrorCode::hostRejected`，尚未建立记录时直接返回 `ApiError.code=CapabilityUnsupported`（`CapabilityUnsupported` 是 §13 的 `ApiError.code`，不是 `ExecutionErrorCode` 取值；这是把当前 13 处 no-op 变成回归网的关键断言）。
  - `describeResource` 的 `ResourceDescriptor` 必含 `providerId` / `resourceKey` / `sessionContinuity` / `reusePolicy` / `initializationRequirements` / `connectionCostPolicy` / `namespaceShape`（连接管理 §5.1）。
  - `SessionObservation` 的 unknown 字段不得填入 `initialTarget` 假充确认（§5.1）。
  - **CM-08（两个编辑器隔离，H/D/F）与 CM-30（idle 连接计入预算，H/D）必须进模板**：这两条描述的是宿主侧「同一个 driver 实例同时服务两个编辑器、上下文互不破坏」与「预算必须统计真实 idle 连接」，不是某个方言的语义，因此对全部 15 个驱动成立。模板里先把两条的用例骨架写出来（驱动只需提供自己的 `ResourceDescriptor` 与连接统计钩子），后续驱动行就不必再重复列它们。
- **进出门槛**：模板本身是一个可运行的 hostless harness（`cargo test -p datazen-driver-api`），且在至少一个「全部能力 unsupported」的桩驱动上能全绿。

### 批次 1：代表驱动（每种语义模型各一个）

选这 4 个是因为它们把 §2.2 的三条结构事实全部覆盖了一遍，且彼此没有共享实现库。

| 驱动 | 选它的原因 | 必做 D 层断言 |
| --- | --- | --- |
| postgres | `Transactional`（postgres 与 sqlite 同为 `Transactional`）+ 多池 + per-DB 控制通道 + 两个正确拒绝 legacy cancel 的驱动之一（另一个是 mysql） | CM-07、CM-08、CM-09～CM-14、CM-15、CM-16、CM-17～CM-19、CM-22～CM-24、CM-26、CM-30、CM-62、CM-67 |
| mysql | 唯一 `namespaceSwitch: inPlace`、唯一 `AutoCommitPerStatement`、DDL 隐式提交 | CM-07、CM-10～CM-13、CM-16、CM-17、CM-19、CM-22～CM-24、CM-26、CM-45、CM-48、CM-62 |
| sqlserver | 唯一真·单物理会话、唯一 `discard_connection`、唯一会话级开关 | CM-07、CM-09、CM-14、CM-17～CM-19、CM-22～CM-24、CM-26、CM-45；另需跑纯 H 的 `CM-69` 与 H/F 的 `CM-74`（二者无 D 层标记，**不能算作 D 层断言**） |
| sqlite | 唯一的硬编码单连接池反例（`sqlite.rs:288-291`；mysql 只有在 `max_pool_size` 配成 1 时才会同样降到 1，见 `types.rs:170-172` 的 `clamp(1, 100)`）、最小 dialect 面 | CM-07、CM-13、CM-14、CM-17～CM-19、CM-24、CM-26、CM-62 |

- **CM-19（driver 无状态能力）** 是批次 1 的核心用例：它直接检验「不把单连接池假装成固定资源」，sqlite 这一格就是它的标准靶子。
- **CM-15** 只对 postgres 成立（切 database 必须替换连接，是 `requiresReplacement` 的教科书实现）；**CM-16**（替换失败保留旧连接）适用于**一切** `requiresReplacement` 驱动——mysql 当前矩阵填 `inPlace`，若 §3.2 的 P0 定案改为 `requiresReplacement`，CM-16 立即对它必做；**CM-11 / CM-12** 只对 mysql 的 `USE` 路径成立。这三条都不允许被推广成「所有驱动都要发 `USE`」的断言。
- **进出门槛**：这 4 个驱动的 `cargo test -p datazen-driver-<id>` 全绿；`pnpm test:unit:drivers` 干净；`pnpm typecheck` 干净；`pnpm e2e:contract:matrix` 中这 4 行的 D 层用例转绿。任一项不过则批次 1 不完成。

### 批次 2：其余关系库与嵌入式数据库

| 驱动 | 必做 D 层断言 | 备注 |
| --- | --- | --- |
| clickhouse | CM-07、CM-10、CM-14、CM-17、CM-19、CM-24、CM-26、CM-62、CM-67 | 先解决 §5.2 的 `namespaceSwitch` 枚举缺口 |
| duckdb | CM-07、CM-13、CM-14、CM-17、CM-19、CM-24、CM-26、CM-62 | 需定案：接不接引擎事务 |
| rqlite | CM-07、CM-14、CM-17、CM-19、CM-24、CM-26 | 最低成本档 |
| turso | CM-07、CM-14、CM-17、CM-19、CM-24、CM-26 | 同上 |
| mongodb | CM-07、CM-09、CM-14、CM-17、CM-19、CM-24、CM-26、CM-62、CM-67 | 需先解决 `namespaceSwitch` 枚举缺口 |

- **批次 2 的前置条件**：§5.2 的 `namespaceSwitch` 必须先补齐「无会话按请求寻址」这一情形的表达方式（扩展枚举，或在 `ResourceDescriptor.namespaceShape` 里把它显式建模并由契约测试固定映射）。clickhouse / mongodb / influxdb 三者共用这一前置。
- **进出门门槛**：本批 5 个驱动 D 层全绿，且批次 1 的 4 个驱动仍然全绿（防止新增能力改坏旧驱动）。`pnpm test:boundaries` 的生产侧基线必须仍为 0 违规。

### 批次 3：非关系库与明确 unsupported 的固化

| 驱动 | 必做 D 层断言 | 备注 |
| --- | --- | --- |
| redis | CM-07、CM-09、CM-19、CM-24、CM-26、CM-62 | 补 `contextObservation` 与 `namespaceSwitch` 的 P0 结论；`value_search` 的 `commit` 不得被误认成事务 |
| influxdb | CM-07、CM-14、CM-19、CM-24、CM-26 | 需先解决 `namespaceSwitch` 枚举缺口 |
| victoriametrics | CM-07、CM-14、CM-19、CM-24、CM-26 | |
| hbase | CM-07、CM-14、CM-19、CM-24、CM-26 | |
| vector | CM-07、CM-14、CM-19、CM-24、CM-26 | |
| elasticsearch | CM-07、CM-14、CM-19、CM-24、CM-26 | **先补最小 Rust 集成测试基线**（当前无 `tests/` 目录，只有 `src/` 内 10 个单元测试） |

- 批次 3 的重点不是接能力，而是**把「不支持」变成可执行的显式拒绝**：`requestCancel` 返回 `Unsupported`、`begin` 返回 `Unsupported`、`resetResource` 返回 `Unsupported`，且每一条都有 CM-24（无取消能力）与 §7 的对应断言。
- **进出门槛**：13 处 `cancel_query -> Ok(())` 全部改为显式拒绝（见 §7）；这 6 个驱动的 D 层全绿；`pnpm e2e:contract:matrix` 全矩阵转绿。

### 4.5 批次顺序的硬性依赖

```text
批次 0（契约模板）
   └─> 批次 1（postgres / mysql / sqlserver / sqlite）
          └─> 批次 2（clickhouse / duckdb / mongodb / rqlite / turso）
                 │  需要：§5.2 namespaceSwitch 枚举缺口先补齐
                 └─> 批次 3（redis / influxdb / victoriametrics / hbase / vector / elasticsearch）
                        需要：13 处 no-op cancel 先改显式拒绝
```

## 5. 协议版本演进

### 5.1 当前版本与最低兼容版本

`packages/driver-api/src/lib.rs` 尾部定义两个常量：

```rust
pub const PROTOCOL_VERSION: u32 = 4;   // lib.rs:99
pub const MIN_PROTOCOL_VERSION: u32 = 1; // lib.rs:105
```

同处文档说明：`version < MIN` 的驱动被拒绝加载；`MIN <= version < PROTOCOL_VERSION` 的驱动以**降级模式**运行，缺失的能力一律按 `false` 处理。**这是 `driver-api` 侧写下的契约承诺；宿主代码当前只打印降级警告，并没有真的把任何能力改写为 `false`（见下方闸门现状）。**

宿主侧闸门有两处，结构相同，但**只拒绝对老版本，不拒绝更新版本**：

- `src-tauri/src/db/registry.rs:126-151`：`pv < MIN_PROTOCOL_VERSION` → `return Err(...)`，拒绝加载（:126-133）；`pv > PROTOCOL_VERSION` → **只有 `tracing::warn!`（:134-140），并不拒绝**，随后照常 `factory.create()` 加载（:154）；`MIN <= pv < PROTOCOL_VERSION` → `warn!` 提示降级（:141-151）后同样照常加载，代码里**没有**把任何 `supports_*` 强制改写为 `false`——降级模式只是打日志，驱动仍按 factory 自己声明的能力运行。
- `src-tauri/src/ai/registry.rs:170-198`：`pv < MIN_AI_PROTOCOL_VERSION` → `tracing::error!` + `continue`，跳过注册；`pv > AI_PROTOCOL_VERSION` → 同样只 `warn!` 后照常 `registry.register()`；`pv < AI_PROTOCOL_VERSION` → `warn!` 降级提示。

数据迁移另有独立校验：plan 里携带 `plan.source_driver_protocol` / `plan.target_driver_protocol`，执行时再次核对（`src-tauri/src/commands/data_transfer/exec.rs:125,150,487-488`，取值辅助函数 `plans::driver_protocol_version` 在 `plans.rs:156`）。

**闸门现状与 §5.4 目标之间的缺口**：`pv > PROTOCOL_VERSION` 分支今天是「警告后照常加载」，也就是说宿主面对一个声明了更高协议版本的驱动时会静默进入未知兼容态，而 §5.4 要求的是「不静默降级」。P2 必须先在**硬拒绝（返回 `Err`、不 `create()`）**与**加载但标记为不可用并禁止 `execute_driver_command`**之间**二选一**（无论选哪个都不得以「下调 `MIN` 让它落进降级分支」的方式绕过），再把它落成门禁；定案见 §5.4 第 5 条与 [§9](#9-待-p0-验证清单) 待定义 H。**在定案并落成门禁前，不能声称「版本更高的驱动会被拒绝」。**

### 5.2 版本是怎么声明的

`DatabaseDriverFactory` 的 `protocol_version() -> u32` 有默认体，返回 `crate::PROTOCOL_VERSION`。**全部 15 个 path 驱动都没有覆盖它**；唯一覆盖者是 git 驱动 `packages/drivers/superset/src/lib.rs:1197`。

这意味着：**驱动只要不显式声明就自动跟随 `driver-api` 的版本**，只要升级 `PROTOCOL_VERSION`，所有 path 驱动同步升版、闸门恒走「当前」分支。代价是**驱动无法为旧宿主做兼容**——这正是必须由 `MIN_PROTOCOL_VERSION` 承接的部分。

### 5.3 哪些改动是 breaking

| 改动 | 是否 breaking | 处理 |
| --- | --- | --- |
| 新增 trait 方法且带 fail-closed 默认体 | 否 | 不动版本；驱动不改也能编 |
| 新增 trait 方法**没有**默认体 | 是 | 升 `PROTOCOL_VERSION`（`MIN` 动不动见下） |
| 改能力枚举取值或语义 | 是 | 升 `PROTOCOL_VERSION`（`MIN` 动不动见下） |
| 改 DTO 字段的必填性 | 是 | 升 `PROTOCOL_VERSION`（`MIN` 动不动见下） |
| 删除或改写受治理的符号（trait 方法、`const`、`type`、DTO 字段、命令定义） | 是 | 升 `PROTOCOL_VERSION`（`MIN` 动不动见下） |
| 放宽已声明 `unsupported` 的行为 | 否 | 降级模式下按 `false` 处理即安全 |
| 收紧已声明 `supported` 的行为 | 是 | 视为 breaking，需按 breaking 发布 |

判定原则是**「老驱动在新宿主上是否仍能安全运行」**。`capabilities.rs` 里所有新枚举都按 fail-closed 默认（`unknown` / `unsupported`）设计，正是为了让「新增能力」本身不成为 breaking。

**这条原则有一个它回答不了的问法。** 上表的六行都在问「老驱动还能不能跑」，但 `#[non_exhaustive]` 这类改动问的是「新的还编不编得出来」——已编好的老驱动照跑不误，答案不在这张表的任何一格里。所以门禁另有第四档 `source-breaking`，它不是本表的补充行，而是不回答本表问题的另一类改动，见 [§5.5](#55-source-breaking老驱动跑得好好的但新的编不出来)。

**升 `PROTOCOL_VERSION` 与升 `MIN` 是两件事，不要混用。** `PROTOCOL_VERSION` 是「宿主现在说的协议」，任何 breaking 改动都必须升它，否则老驱动无从判断自己是不是对新宿主说谎。而 `MIN_PROTOCOL_VERSION` 是「还愿意接受的最后一个老驱动协议」，**它是一个主动放弃老驱动的产品决定，不是 breaking 的机械后果**：本 crate 自己的 `PROTOCOL_VERSION` 已经过 `1 → 4`，`MIN` 始终停在 `1`，这正是「窗口从不被自己悄悄收窄」的含义。若某次 breaking 确实不想再兼容任何老驱动，那是**另外**再升 `MIN`，让宿主在 `pv < MIN` 时明确拒绝，而不是把它当成 breaking 的处理方式。

**门禁**：`scripts/check-driver-protocol-compat.mjs`（`pnpm test:driver-protocol`，`.github/workflows/ci.yml:68` 硬门禁）。它对 10 个受治理符号的**删除 / 改写**，以及**在以 `;` 结尾的 trait 里新增 `fn` / `const` / `type`**，判为 `breaking` 并要求 `PROTOCOL_VERSION` 上升；并常驻三条断言：`min-protocol-never-lowered`、`protocol-window-non-empty`（`MIN ≤ PROTOCOL`）、`crate-version-advanced`。新增 DTO 字段或枚举取值走 `additive` / `cosmetic` 判定，不触发协议升级。第四档 `source-breaking` 的触发条件、失败配方与检测边界见 [§5.5](#55-source-breaking老驱动跑得好好的但新的编不出来)。

### 5.4 「协议升级与驱动发布是原子兼容门槛」

这条门槛的落地机制如下。

**禁止的做法**：宿主遇到驱动行为不兼容时，下调 `MIN_PROTOCOL_VERSION` 让旧驱动继续加载，并在降级模式下把缺失能力当 `false` 用。这会把兼容性问题变成运行期静默降级——正是「新增能力缺失不会 no-op 成功」要禁止的那类问题。当前 `MIN_PROTOCOL_VERSION = 1` 是在 `PROTOCOL_VERSION = 4` 之前三次演进中留下的兼容窗口，它的存在本身不代表可以随时下调。

**正确的做法**：任何 breaking 改动必须让**版本号、驱动实现、驱动发布**三者同一次落地完成，具体为：

1. 在 `packages/driver-api` 改 trait / 枚举 / DTO，同时把 `PROTOCOL_VERSION` 从 N 提到 N+1。
2. **同一个提交内**把所有受影响的 path 驱动改到能通过新契约的形态（缺失能力显式 `Unsupported`，不得 `Ok(())`）。
3. path 驱动在**本仓库内**改完即发布，不存在跨仓库同步窗口。
4. git 驱动在**自己的仓库**里跟进，在 DataZen 侧以 `drivers-registry.json` 的**钉死 `ref`**（`source: "git"` + 指定 `ref`，参见 `olap` 条目 `7096c8737dbe25ab4529e5c3997c9f67309154d9`）提交 DataZen registry PR。钉死 ref 是可复现构建的前提：registry 不指向浮动分支，也不指向未打 tag 的提交。
5. 宿主升级后，`PROTOCOL_VERSION` 必须 ≥ 全部已发布驱动的 `protocol_version()`。**注意这里的现状是缺口而不是保障**：`registry.rs:134-140` 的 `pv > PROTOCOL_VERSION` 分支目前只是 `tracing::warn!` 然后照常加载，既没有拒绝、也没有把它挡在降级分支之外。要让这条真正成立，P2 必须先在**硬拒绝（返回 `Err`、不 `create()`）**与**加载但标记为不可用并禁止 `execute_driver_command`**之间**二选一并落成门禁**（定案见 [§9](#9-待-p0-验证清单) 待定义 H；闸门现状见 §5.1）——**在定案并落成门禁前，「版本更高的 git 驱动会被拒绝、不静默降级」这句话是不成立的**。

**冲突时的判据**：如果某个 breaking 改动无法在本仓库内一次性覆盖全部 path 驱动（例如需要 3 个 git 驱动分别发版），那它**不能**靠下调 `MIN` 蒙混过关，必须先让 DataZen 侧以新 `PROTOCOL_VERSION` 构建、通过 registry 钉 ref 升级全部 git 驱动，最后再打开闸门。在中间状态，宿主应保持 `PROTOCOL_VERSION` 不变、拒绝加载尚未跟进的新能力，而不是接受一个半兼容的组合。

**AI 侧同理**：`AI_PROTOCOL_VERSION` 变更时需要同步更新所有 AI Provider 插件，这条规则在 `AGENTS.md` 中已写明，与本文的驱动协议规则是同一套原子门槛的两次应用。

### 5.5 `source-breaking`：老驱动跑得好好的，但新的编不出来

门禁实际有**四档**判定，不是三档：`additive` / `breaking` / `source-breaking` / `cosmetic`（`scripts/lib/compatMatrix.mjs:24` 的 `COMPAT_MATRIX.classes`）。

第四档不是给三档打补丁。有一类改动**根本不回答 §5.3 的判定原则**——「老驱动在新宿主上是否仍能安全运行」这个问题对它来说是空的。代表物是给受治理 `struct` 加 `#[non_exhaustive]`（下文把这种「以后不能再加字段」的 FOREVER 冻结简称 FOREVER 冻结）。这类改动落地的瞬间：

- 已经**编好**的老驱动**完全正常**。它报告的协议版本没变，宿主不降级、不拒绝，行为一模一样。
- 任何**还没编**的驱动**立刻编不出来**。`rustc` 报 `error[E0639]`，在对方自己的仓库里。

于是旧的两档都不是「不够精确」，是**错的**：

- **判 `additive` 是撒谎。** `additive` 的定义就是「现有实现方继续能编译」，而 `#[non_exhaustive]` 恰恰是让它们停止编译的那一个属性。它又是唯一会对 out-of-tree 驱动说「你没事，继续走」的那一档——那正是唯一不能发出去的消息。**用一档承诺「你不受影响」的判定，去描述一个会打破你的改动，是在骗下一个人。**
- **判 `breaking` 会逼出一场根本没发生的线上事故。** `breaking` 的义务是升 `PROTOCOL_VERSION`（`scripts/check-driver-protocol-compat.mjs:702` 的 `breaking-change-requires-protocol-bump`）。但线上一个字节都没动，升它等于向所有驱动宣布一个不存在的线上不兼容，同时对真正会炸的地方——别人仓库里的 `cargo build`——一个字都没说。

所以 `source-breaking` 自己成档：它按 `additive` 的方式动 crate 版本（`COMPAT_MATRIX.classes['source-breaking'].requires = ['crateVersion']`，`scripts/lib/compatMatrix.mjs:69-73`），并额外携带一份显式迁移说明。

#### 边界一：FOREVER 冻结 ≠ 升 `PROTOCOL_VERSION`

两者管的不是同一件事。`PROTOCOL_VERSION` 是宿主与驱动之间**线上**说的协议（`packages/driver-api/src/lib.rs:99`，当前 `4`；`MIN_PROTOCOL_VERSION` 在 `:105`，当前 `1`），它能表达的是一个 `[MIN, PROTOCOL]` 的**版本窗口**：装一个旧驱动，宿主据此知道该怎么对待它。FOREVER 冻结是**源码级**的，协议窗口里没有「你必须重编」这个词汇，也永远不会有——线上一位比特都没动。

把它表达成协议升级，等于**用一台测不到故障的仪器去报告一个只在编译期存在的故障**。因此：改线上语义 → 升 `PROTOCOL_VERSION`；FOREVER 冻结 → 升 crate 版本 + 发迁移配方，**协议号一位都不动**。

#### 边界二：失败配方，以及一个必须为 0 的自查

命中 `source-breaking` 时门禁报 `source-break-requires-out-of-tree-migration`（`scripts/check-driver-protocol-compat.mjs:718`），并把 `scripts/lib/sourceBreak.mjs:44-57` 的 `OUT_OF_TREE_MIGRATION_NOTE` 原样打进失败信息——因为对 out-of-tree 作者来说，**这段门禁输出往往是他编不过之前唯一能看到的东西**。这条没有「已满足」可以 latch（上面两条有）：升 crate 版本不告诉任何人该敲什么代码，所以**配方本身就是违规项**，命中即报。

同一份报告里，`breaking-change-requires-protocol-bump` 的出现次数**必须是 0**。这是防自己搞混的自查项，不是顺带的效果：`CLASS_REQUIRES` 由 `COMPAT_MATRIX.classes[cls].requires` 派生（`scripts/check-driver-protocol-compat.mjs:209`、`:699`），而 `source-breaking` 的 requires 里**没有** `protocol`。哪天这条 token 在一次 FOREVER 冻结提交里出现，就说明判定漏回了 `breaking`，第四档等于白设。

#### 边界三：检测是故意窄的，不要当成「什么都抓」

`SOURCE_BREAKING_ATTRIBUTES`（`scripts/lib/sourceBreak.mjs:82-91`）目前只有一条 `struct` 规则，下面四个边界都是刻意的：

1. **只认整行。** 正则是 `/^\s*#\s*\[\s*non_exhaustive\s*\]\s*$/`（`:86`）。`isCosmeticLine`（`scripts/check-driver-protocol-compat.mjs:285-296`）会先丢掉纯注释行，所以「文档里提到这个属性」不会被误判；但一行 `// TODO: 加 #[non_exhaustive]` 后面确实声明着字段——子串匹配会为这个 TODO 拦下一次门禁，把一次普通字段新增报成 source break。
2. **只认受治理 span 内的非 cosmetic 新增行。** 改写一行在 diff 里是**删除**，永远在 `breaking` 分支就返回了，压根到不了新增分支（`:533-546`）。所以这一档只能由**纯新增**触发。
3. **只认 `kind === 'struct'`。** `trait` 的表是空的（`:90`）。
4. **`#[serde(...)]` 仍然是 `additive`，这是对的。** 它改的是线上写出去的东西，不是能不能编译，归本文档 serde 行管，不由这张表管。**两个排除的理由不能互换**：把它排除的理由是「改线上」，而「在 `enum` 上限制的是**匹配**、不是构造」是 `non_exhaustive` 加在 enum 上时被排除的理由（`:72-75`）。拿后者去论证前者是错的——那会把一个线上问题说成构建问题，正是这张表存在的意义。

#### `CapabilitySet` 的当前位置（当前事实，不是计划）

`packages/driver-api/src/capabilities.rs:282` 的 `pub struct CapabilitySet` **目前没有** `#[non_exhaustive]`，紧邻的 `:281` 是 `#[serde(rename_all = "camelCase")]`。**这是当前状态，不是「待补」。**

之所以现在加这个属性是**一行一处**的改动：`1434d29e3` 之后，全仓库**穷举式 `CapabilitySet` 字面量数量为 0**。12 个 path 驱动全部改为 `CapabilitySet::default()` 加逐字段赋值，形态见 `packages/drivers/postgres/src/resource/capabilities.rs:128` 的 `let mut capabilities = CapabilitySet::default();` 及其后 12 条 `capabilities.<字段> = …`（`:130-155`、`:194`、`:228`）。加属性这一步确实只有一处；但**代价落在 out-of-tree 驱动上**，那是边界二的迁移配方要交代的事：旧写法在别的仓库里编不过，而本仓库内的驱动早已全部改完。

### 5.6 `supportsBackup`：判据不是「有没有覆写 dump」

§5.5 讲的是**改动**该怎么判。这一节是同一类错误在**今天的状态**里已经发生的样子：UI 侧 `packages/drivers/<id>/ui/meta.ts` 上的 `supportsBackup` 布尔，与这个驱动到底能不能被备份对不上。

先说判据，因为它反直觉。

`DatabaseDriver` 给 `dump_database_with_progress` 提供了**通用默认实现**（`packages/driver-api/src/traits.rs:1000-1020`），原文是：

```rust
/// Same as [`Self::dump_database`] with per-object progress.
async fn dump_database_with_progress(
    /* ... */
) -> Result<String, DriverError> {
    if opts.create_database {
        return Err(DriverError::NotSupported(
            "Backup option 'create' (CREATE DATABASE) is not supported for this driver".into(),
        ));
    }
    crate::sql_dump::dump_sql_database_with_progress::<Self, _>(
        /* ... */
    ).await
}
```

也就是说**驱动不覆写 dump，不代表它不能 dump**。默认实现往下走的是 `sql_dump::dump_sql_database_with_progress`（`packages/driver-api/src/sql_dump/dump.rs:323`），而它对驱动的要求只有两项：

- **`get_tables`** —— `dump.rs:334-336`，枚举可 dump 对象清单；
- **`get_table_schema`** —— 经 `dump_table_ddl`（`dump.rs:200-202`，其默认实现在 `traits.rs:936-945`）落到 `dump.rs:54` 渲染 DDL，并在 `dump.rs:215-217` **再取一次**，这次用于生成 `INSERT INTO` 的列清单。

其余环节都不是门槛：`dump_routines` / `dump_triggers` 的缺省是空串（`traits.rs:966` / `:977`）；`dump_view_ddl` 的缺省确实返回 `NotSupported`（`traits.rs:951`），但 `dump.rs:196-197` 把它接住并写成一行注释跳过。

**所以「有没有自定义覆写 dump」不是判据；判据是 `get_tables` + `get_table_schema` 是否为真实现。** 一个没有 SQL 方言的 HTTP 类驱动只要实现了这两项就能跑通用 dump——尽管它压根不写 SQL。

#### 普查结果

**先说覆盖范围。** 这是对 **git 跟踪的 15 个 path 驱动**做的**静态源码普查**——读 `packages/drivers/<id>/ui/meta.ts` 与 Rust 源码。它**不是**运行时枚举，因此**与链接期注册无关**：`iter_driver_factories()`（`packages/driver-api/src/factory.rs:87`）确实只能枚举被链接进二进制的 crate，但那约束的是「运行时能拿到几个 factory」，不约束这份文档读了几个目录。mongodb 不在默认 basic 构建里，它的源码却在仓库中，普查照样覆盖它。**不要**据此把「未链接」当作本表的盲区。

**真正没覆盖的是 registry 里 3 个 `source: "git"` 的驱动**（kiwi / olap / superset）：它们由 `resolve-drivers.mjs` 在构建时克隆到 `packages/drivers/<id>/`，落在 `.gitignore` 的 `/packages/drivers/*` 之下，**未克隆时目录根本不存在**，静态普查无从读到。它们的 `supportsBackup` 取值**本文未核查**，不要默认它们落进下面任一栏。见 [§1.3](#13-17-个-path-驱动这一表述与代码事实的出入)。

15 个 path 驱动带这个字段（`http-support` 不带，它不是驱动，见 [§1.2](#12-http-support共享-helper-crate不是驱动)）。两个方向：

| 方向 | 数量 | 驱动 |
| --- | --- | --- |
| 声明 `true` 但 `get_tables` / `get_table_schema` 未实现 | **0** | —— |
| 声明 `false` 但两项均为真实现 | **6** | elasticsearch / hbase / influxdb / mongodb / vector / victoriametrics |

这 6 个都不是 `NotSupported` 桩，`get_tables` 每个都打到真实接口：

| 驱动 | `get_tables` 实际调用 | 声明位置 | `get_table_schema` |
| --- | --- | --- | --- |
| elasticsearch | `GET {_cat/indices}` | `elasticsearch/src/elasticsearch.rs:201` | `:245` |
| hbase | `GET /` 取 `table` 数组 | `hbase/src/hbase.rs:272` | `:301` |
| influxdb | `SHOW MEASUREMENTS` | `influxdb/src/influxdb.rs:182` | `:229` |
| mongodb | `list_collection_names()` | `mongodb/src/mongodb.rs:309` | `:337` |
| vector | `GET /collections` | `vector/src/vector.rs:194` | `:223` |
| victoriametrics | `GET /api/v1/label/__name__/values` | `victoriametrics/src/victoriametrics.rs:210` | `:238` |

（`mongodb/src/resource/test_support.rs:133` 另有一份测试用实现，不参与本表。）

#### `false` 不总是错的 —— redis 是正确的那一个

**不要把上表读成「`false` 就是 bug」。** 7 个声明 `false` 的驱动里，redis 是**正确**的：它显式覆写了 `dump_database_with_progress`（`packages/drivers/redis/src/driver/database.rs:340-350`）来拒绝：

```rust
Err(DriverError::NotSupported(
    "Redis does not use SQL dump; export keys via driver commands".into(),
))
```

覆写让通用默认实现根本不会执行——在第一道门口就返回了。它同时**也**有 `get_tables`（`database.rs:110`）和 `get_table_schema`（`:126`），但那两个是 schema 浏览用的，不是备份入口。

**判定顺序：先看有没有覆写并拒绝，再看 `get_tables` / `get_table_schema`。顺序反过来就会把 redis 误判成上表的第七行。**

#### 那 6 个为什么不改

**当前裁定：不改，但记在这里。** 理由不是「改了会坏」——通用默认实现对它们是能跑的。理由是**打开它等于第一次启用一条从未有人跑过的路径**：六个驱动每一个都要一次真实验证（连真实集群，比对 dump 与 restore 往返），**不是一行配置，是一次未排期的验证工程**。

它们的 `false` 因此是**已知偏差**而非隐藏问题。改动的前置条件是六个驱动各自的往返验证，不是本文件。

#### 与 `CapabilitySet.backup` 的区别

本节的 `supportsBackup` 是 UI 的 `ui/meta.ts` 字段；`CapabilitySet` 里另有一个 `pub backup: BackupSupport`（`packages/driver-api/src/capabilities.rs:298`）。**两者独立，不要互推**——前者描述 UI 入口是否可达，后者是逐驱动声明的能力。按 §2.1 的约定，本节不往矩阵里补 `backup` 列。

## 6. 过渡 adapter：遗留驱动的独立受控 Command

### 6.1 允许与禁止

连接管理 §5.2 的收尾约束是：**「遗留驱动可以支持独立受控操作，但不能因为『池大小为 1』就声明固定会话：底层重建和不同 consumer 仍会破坏语义。」** 拆成可执行的两侧：

**允许（保留独立受控 Command，不必立刻实现资源契约）**：

- 遗留 trait 上已有的独立操作继续可用，只要它自身携带完整语义（例：sqlserver 的 `set_identity_insert` 本身就是会话级开关，其作用域由驱动自己负责；sqlserver 的 `discard_connection` 本身就是一次性丢弃）。
- 宿主可以对 `unsupported` 的能力继续提供**明确标注降级**的旧路径，但该路径必须在 UI 与 `ExecutionCompletion.effectOutcome` 上标出 `partial` 或 `unknown`，不得呈现为成功。
- 迁移期的驱动可以同时实现新旧两套接口，新接口为准，旧接口作为 adapter 转发。

**必须永不开放**：

- **不得因为「池大小为 1」或「每个 `pool_id` 只有一条连接」就声明 `statefulSession: supported`。** 具体到本文的三个驱动：sqlite 的 `max_connections(1)` 是 sqlx pool；duckdb 的 `Arc<Mutex<Connection>>` 是互斥访问而非基线校验；redis 的常驻 `live` 连接承载了大多数操作，但同样没有基线校验、没有独占声明。§5.2 的反例原话就是这一条。
- **不得因为「驱动没实现」就把 `Unsupported` 翻译成成功。** trait 默认体已经是 fail-closed 的（`discard_connection`、`begin_read_snapshot`、`cancel_query_with_execution` 都返回 `Unsupported`），adapter 若把它们捕获后返回 `Ok(())`，等于抹掉默认体的保护。
- **不得让 adapter 跨 consumer 复用会话级句柄。** §6.5 的句柄登记机制还不存在，adapter 若在没有登记的情况下交出句柄，宿主将无法对句柄终态负责。

### 6.2 退役条件与删除时机

每个驱动从「adapter 双轨」切换到「只走资源契约」的条件是全部满足：

1. 该驱动在 §2.1 矩阵中的每一格都已是 §5.2 的合法取值，不再有「待 P0 验证」。
2. §5.1 的 9 个操作全部实现，且 `executeOnResource` 不再自行从随机池取连接。
3. 所属批次的 D 层用例全绿，且 `CM-19` 明确断言「没有把单连接池假装成固定资源」。
4. `resetResource` 要么返回 `Clean`（通过 CM-26 / CM-69）要么返回 `Discard`，没有第三条静默路径。
5. 宿主对该驱动的所有 namespace API 调用点已迁移。当前调用点包括 `src-tauri/src/data_transfer/execute/dispatcher.rs:696`（`tgt_driver.discard_connection(tgt_handle)`）、`src-tauri/src/data_sync/execute.rs:196-199`（`executor.discard_connection()`，仅在 `cleanup_error` 已发生时）与 `src-tauri/src/commands/sync/exec.rs:88-93`（同一 executor 的转发实现）、`src-tauri/src/workflow/executor.rs:60`（持有 `driver_has_multi_database: bool`）、`src-tauri/src/bootstrap/run.rs:250,258`（注册 `close_database` / `get_open_databases`）、`src-tauri/src/testing/mock_driver.rs:361-374`（伪造 `close_database` / `open_databases`）。这几处正是 P2 要收敛的宿主侧驱动分支。

**删除时机**：adapter 的删除与**宿主侧对应分支的删除**必须同一个提交。只删驱动侧 adapter 而留宿主分支，宿主就会退回到 no-op；只删宿主分支而留驱动侧，则驱动仍可能被旧路径调用。这个原子性与 §5.4 的协议发布门槛是同一条原则的两次应用。

## 7. unsupported 的拒绝断言要求

连接管理 §17 的收尾约束是：**「能力 Unsupported 的 driver 测试断言『正确拒绝』，不能静默 skip 后宣称能力已验证。真实测试环境不可用必须报告未验证范围，不能用 fake 代替真实协议结论。」**

### 7.1 头号违反点：13 处 `cancel_query` 的 no-op 成功

15 个 path 驱动的 `cancel_query` 实现体逐个读过，结果是 **13 个返回 `Ok(())`**，即「用户请求取消，驱动回一句成功，实际什么都没做」：

| 驱动 | 位置 | 当前行为 |
| --- | --- | --- |
| mongodb | `mongodb.rs:679` | `Ok(())` |
| duckdb | `duckdb.rs:570` | `Ok(())` |
| clickhouse | `clickhouse.rs:552` | `Ok(())` |
| rqlite | `rqlite.rs:389` | `Ok(())` |
| turso | `turso.rs:419` | `Ok(())` |
| elasticsearch | `elasticsearch.rs:511` | `Ok(())` |
| influxdb | `influxdb.rs:396` | `Ok(())` |
| victoriametrics | `victoriametrics.rs:402` | `Ok(())` |
| hbase | `hbase.rs:484` | `Ok(())` |
| vector | `vector.rs:429` | `Ok(())` |
| redis | `database.rs:319` | `Ok(())` |
| sqlite | `sqlite.rs:696` | `debug!` 日志 + `Ok(())` |
| sqlserver | `sqlserver.rs:1759` | `Ok(())` |

只有 **postgres**（`postgres.rs:467`）和 **mysql**（`mysql.rs:1886`）返回 `Err(DriverError::Unsupported("legacy session-wide query cancellation is disabled; use an execution handle"))`。

其中 **sqlserver 最严重**：`supports_query_execution_cancel` 未覆盖（默认 `false`），`cancel_query_with_execution` 未覆盖（默认 `Err(Unsupported(...))`）。也就是说它的 `preciseCancel` 在矩阵里是明确的 `unsupported`，而 legacy 取消却静默返回成功——宿主据此显示「已取消」，用户以为写事务被回滚了，实际没有。这正是 §13 说的「不能因为『看到取消』就写 rolledBack」。

**P2 必须做的改造**：这 13 处全部改为显式 `Unsupported`（或按 §5.1 的 `requestCancel` 语义返回 `CancelReceipt.disposition = unsupported`），并让宿主把 `unsupported` 呈现为「该连接不支持取消，请显式中止会话」，而不是「已取消」。

### 7.2 逐能力的拒绝断言要求

对每一个标 `unsupported` 的能力，M 层（驱动内）必须有一条断言，D 层（宿主 × 驱动）也要有一条；具体落在哪一层以「对应用例」列标注的层标记为准。**当前唯一的缺口是 `sessionScopedHandles`：连接管理 §16 里与 `sessionHandles` 相关的只有 CM-74，标记为（H/F），没有 D 层用例**（§9 待定义 G）——在该 D 层断言补齐前，这一行不得声称「M 层与 D 层各有一条断言」。

| 能力 | 触发的契约操作 | 必须断言的行为 | 对应用例 |
| --- | --- | --- | --- |
| `statefulSession` | `describeResource` | `sessionContinuity` 不得声明为固定；CM-19 断言「池大小为 1」不被当作固定会话 | CM-19 |
| `namespaceSwitch` | `changeContext` | 返回 `Unsupported` 而非静默留在旧命名空间；失败后仍可查原目标（AGENTS.md 交互原则） | CM-16（H/D/F，适用于一切 `requiresReplacement` 驱动）；CM-11（D/F，**仅** mysql 的 `USE` 路径） |
| `contextObservation` | `observeSession` | 返回 `Unknown`，且 unknown 字段**不得**填 `initialTarget` 假充确认（§5.1） | CM-13、CM-18 |
| `transactionObservation` | `begin/commit/rollback` | 返回 `Unsupported` / `TransactionError`；不得回退成 autocommit 继续跑 | CM-18、CM-45 |
| `sessionScopedHandles` | `executeOnResource` | `sessionHandles` 返回空数组，**不得**返回未登记的句柄 | CM-74（H/F 层，**无 D 层用例**——P2 需为该能力补一条 D 层断言，见 §9 待定义 G） |
| `resetForReuse` | `resetResource` | 不得返回 `Clean`；不满足全部清理条件时直接走关闭路径（§9.4） | CM-26（H/D）、CM-69（纯 H） |
| `preciseCancel` | `requestCancel` | `disposition` 返回 `unsupported`；**不得**调用 session-wide cancel 作为隐式回退（§5.1 明文禁止） | CM-22、CM-23、CM-24 |
| `snapshots` | `begin` | 返回 `Unsupported("stable read snapshots are not supported by this driver")` 同义错误 | CM-48 |
| `transactions` | `begin` | 返回 `TransactionError`；不得静默以非事务方式执行 | CM-17、CM-18 |
| `ddlAtomicity` | `describeResource` | `Unknown` 时宿主不得包事务，直接按「非事务 DDL」执行 | CM-42 |

三条贯穿性要求：

1. **错误码与生效结论正交。** `errorCode` 是 `sqlError`，`effectOutcome` 可能是 `notStarted`/`completed`/`rolledBack`/`partiallyApplied`/`unknown` 任一（§13）。取消未被支持时 `CancelReceipt.disposition=unsupported`（**正常返回值，不是异常 code**），`effectOutcome` 必须是 `unknown`，且不得写 `rolledBack`。
2. **不得静默 skip。** D 层用例遇到 `unsupported` 时，判定结果必须区分「断言了正确拒绝」与「跳过」两种状态，前者绿、后者红。测试报告要能回答「这个能力是被验证过的拒绝，还是没测」。
3. **不得用 fake 代替真实结论。** 真实测试环境不可用时，报告未验证范围。批次 3 里 7 个驱动（rqlite / turso / elasticsearch / hbase / influxdb / victoriametrics / vector）没有任何 `tests/` 集成测试目录、sqlserver 依赖 `tests/live_*.rs` 的真实服务端，正是这条的典型场景。

## 8. 依赖边界约束

### 8.1 允许与禁止的划分

`packages/driver-api` 的公共接口**只允许**使用标准库类型、API 自有类型和 serde（连接管理 §5.1 末段）。据此：

| 类别 | 允许出现在驱动公共 API 表面 | 允许出现在驱动内部实现 | 禁止 |
| --- | --- | --- | --- |
| 宿主类型 | — | — | ❌ 任何 `datazen`（宿主 crate）的类型 |
| Tokio | — | ✅ 15 个 path 驱动**全部**依赖 `tokio 1` | ❌ 出现在 `driver-api` 公共签名或 DTO 里 |
| HTTP 客户端 | — | ✅ `reqwest`（经 `http-support` 或直连） | ❌ `reqwest::Response` 等出现在公共签名里 |
| SQLx | — | ✅ postgres / mysql / sqlite 依赖 `sqlx 0.8` | ❌ `sqlx::Pool` / `sqlx::Row` 出现在 `driver-api` 公共签名里 |
| 第三方后端类型 | — | ✅ `tiberius::Client`（sqlserver）、`mongodb::Client`、`::duckdb::Connection`、`redis::aio::Connection` | ❌ 出现在 `driver-api` 公共签名里 |

**关键区分**：依赖 tokio / reqwest / sqlx / tiberius 作为**私有实现依赖**是正常的、也是现状；禁止的是它们出现在**公共 API 表面**。§5.1 明确要求「结果写入等待表示背压；不暴露 Tokio channel 类型」「`ResultSink` 提供带字节大小的异步写入/完成/失败方法」——即背压语义要表达，Tokio 的具体类型不能表达。

P2 新增的 `capabilities.rs` / `resource.rs` / `session.rs` 必须逐条过这条：`ResourceHandle` 是 API 定义的**不可伪造 opaque ID**，由 provider 校验所有权和 epoch（§5.1）；`SessionObservation` / `TransactionState` / `ExecutionCompletion` 全部用 API 自有结构表达；`snapshots`、`transactions`、`ddlAtomicity` 三项的自由文本范围若用结构体表达，其中的隔离级别列表应当是 API 自有枚举，不得直接复用 `sqlx::TransactionIsolation` 或后端枚举。

### 8.2 资源成本与预算

`BudgetPort` 返回 opaque 许可并按**实际物理连接**申请/释放（§5.1 末段）。对本文的驱动：

- SQLx 驱动（postgres / mysql / sqlite）必须约束内部 pool 的真实建连数；**只在 acquire 时计数会漏掉 idle 连接**。对照 §9.4 归池前检查与 CM-30（idle 连接计入预算），postgres 的 `control_pool`（max 1 / min 0）就是一个典型的「不计入主池统计的常驻连接」，P2 必须把它纳入预算。
- 第三方 SDK 无法统计实际 socket 时（`redis` 的 cluster-async、`mongodb` 的 `Client` 内部池、duckdb 的进程内连接），声明**保守资源成本/硬上限**，或在严格服务端预算模式下**直接拒绝该能力**，不得报告精确值。

### 8.3 改动 `packages/driver-api` 后必跑的门禁

| 门禁 | 命令 | 基线 |
| --- | --- | --- |
| 驱动协议兼容 | `pnpm test:driver-protocol` | 干净。`source-breaking` 命中时只应报 `source-break-requires-out-of-tree-migration`；同一次运行里 `breaking-change-requires-protocol-bump` 出现次数**必须为 0**（自查项，见 [§5.5](#55-source-breaking老驱动跑得好好的但新的编不出来)） |
| 依赖边界 | `pnpm test:boundaries` | 生产代码 0 违规；fixture 2 处已知违规（不是 0 目标值） |
| 类型 | `pnpm typecheck` | 干净，**含测试文件**（`tsconfig.json` 已不排除 `__tests__/` 与 `*.test.ts(x)`） |
| 驱动 UI 单测 | `pnpm test:unit:drivers` | 干净 |
| 驱动 Rust | `cargo test -p datazen-driver-<id>` | 全绿 |
| 契约矩阵 E2E | `pnpm e2e:contract:matrix` | 走 `pnpm tauri:build:webdriver` 构建，**禁止**裸 `cargo build` |

另外，§17 结尾的总要求同样适用于本阶段：生产无裸 `unwrap`/`expect`、公共 API 无实现库类型、测试 typecheck 干净、生成文件（`generated.ts` / `driver_init.rs` / `.driver-features.json` / `capabilities/default.json`）未提交、**无新增宿主数据库名称分支**、无敏感日志。最后一条「无新增宿主数据库名称分支」是 P2 的自我约束：上面 §6.2 列出的 `driver_has_multi_database`、`close_database` / `get_open_databases` 等宿主分支只能被删除或替换，**不得新增**按数据库名判断的分支。

## 9. 待 P0 验证清单

以下是 §2.1 中写「待 P0 验证」的 7 格，以及若干需要 P0 / P2 **定义**（而非验证）的项。每一项都注明了具体该测什么、怎么算验证通过。

| # | 驱动 | 能力 | 现状 | P0 需要的证据 |
| --- | --- | --- | --- | --- |
| 1 | redis | `contextObservation` | `open_pinned_conn` 会 `select_db`，但没有读到任何「从活连接读当前 db index」的路径 | 找到一个能在同一活连接上读回当前 db index 的调用点；找不到则填 `unsupported`。用例落 `packages/drivers/redis/tests/`，CM-09 |
| 2 | mongodb | `namespaceSwitch` | 按 `client.database(name)` 逐请求寻址，无会话可切 | §5.2 枚举缺「无会话按请求寻址」档；P0 须先扩展枚举或定义 `namespaceShape` 映射，再回填取值。CM-62 |
| 3 | duckdb | `namespaceSwitch` | `get_databases` = `["main","temp"]`（这两个名字是 **schema** 名不是 catalog 名，见 [§3.8](#38-duckdb)）；`ATTACH` 仅在 `#[cfg(test)]`；无 `USE` | 决定生产路径是否接 `ATTACH`/`USE`；不接则 `unsupported` + 拒绝断言。CM-62 |
| 4 | duckdb | `contextObservation` | 未读到任何读当前 catalog / schema 的实现 | 找或补一个 `SELECT current_database()` 等价调用点；找不到则 `unsupported` |
| 5 | clickhouse | `namespaceSwitch` | 库在建连时取自 `config.database` 并随每次查询下发 | 同 #2 的枚举缺口。CM-62 |
| 6 | influxdb | `namespaceSwitch` | `SHOW DATABASES` 真实枚举，但库随请求下发 | 同 #2 的枚举缺口。CM-62 |
| 7 | redis | `namespaceSwitch` | 同一驱动内两种机制并存：`with_live_op!` 在常驻 `live` 连接上 `select_db`（就地切换），`open_pinned_conn` 另开一条连接再 `SELECT`（换连接） | P0 须定案这一格记 `inPlace` 还是 `requiresReplacement`（或先给 §5.2 补第三种表达），再回填取值。CM-11 / CM-12 针对 `USE`，不适用于 redis 的 `SELECT` 路径 |

另需 P0 **定义**（不填矩阵，先立规则）：

- **A. `statefulSession` 的 `unknown` 三格如何收紧。** sqlserver / duckdb / redis 各要给出把 `unknown` 变成 `supported` 所需的全部保证（基线校验、重建后的可观察性、并发下的独占性），或直接证明不成立改填 `unsupported`。
- **B. `namespaceSwitch` 枚举缺口。** 至少影响 mongodb、clickhouse、influxdb、duckdb 四个驱动；不定规则这四格永远填不出合法值。redis 是另一种情形——同一驱动内就地切换与换连接两种机制并存（§9 第 7 条），同样需要 P0 给出判定规则。
- **C. sqlserver 会话级开关的归类。** `explicit_identity_insert_requires_session_toggle` / `set_identity_insert` 归入 `sessionScopedHandles` 还是 `initializationRequirements`。
- **D. `contextObservation: partial` 的充分性。** postgres 只观察到 `current_database()`、未观察 `search_path`；mysql 只观察库、未观察其它上下文；sqlserver 只观察隔离级别。P0 要定义 `partial` 允许缺哪些维度，否则三格会长期停在含义不清的 `partial`。
- **E. DuckDB 引擎事务能力接不接。** 引擎支持、驱动未暴露；不接就必须保证拒绝是显式的。
- **F. `http-support` 是否满足「不共享实现库类型」。** 本文 §1.2 给出了「共享 HTTP 管道不构成共享后端类型」的读法，但这必须写成明确规则并加边界测试，否则批次 0 的 `pnpm test:boundaries` 判不出结果。
- **G. `sessionScopedHandles` 的 D 层断言缺位。** 连接管理 §16 里涉及 `sessionHandles` 的只有 CM-74，标记是（H/F），**没有 D 层用例**；§7.2 因此无法兑现「M 层与 D 层各有一条断言」。P2 需先定这条 D 层用例的形态（示例：在真实服务端执行一条会返回服务端预处理语句或游标的语句，断言 `ExecutionCompletion.sessionHandles` 要么是空数组、要么是已登记句柄），再落到驱动 crate 内。**在此之前该格不得声称「D 层已验证」。**
- **H. 高于宿主的协议版本当前不会被拒绝。** `src-tauri/src/db/registry.rs:134-140` 与 `src-tauri/src/ai/registry.rs:179-186` 都是「`warn!` 后照常加载」，而 §5.4 要求的是显式拒绝。P0/P2 需要定：改成硬拒绝（返回 `Err`、不 `create()`），还是保留加载但把驱动标记为不可用并禁止 `execute_driver_command`；同时要确定「驱动版本 > 宿主版本」与「降级模式」是否互斥（若不互斥，就存在一条能同时进入未知兼容态和降级提示的路径）。**在规则定案前，§5.4 第 5 条只能当作目标，不能当作现状。**

## 10. 验收映射表

### 10.1 P2 退出门槛 → 本文章节

开发计划 §6 的 P2 退出门槛原文只有三条，与连接管理 §17 的 D 层用例合并后映射到本文：

| P2 退出门槛（开发计划 §6 原文） | 展开后的 D 层用例 | 本文对应章节 |
| --- | --- | --- |
| D 层 CM-07～19、22～26、30、45、48 | 会话连续性与切库：CM-07、CM-08、CM-09～CM-14、CM-15、CM-16、CM-17、CM-18、CM-19；并发 / 清理 / 取消：CM-22、CM-23、CM-24、CM-25、CM-26；预算：CM-30；事务失效可回收：CM-45、CM-48 | [§2.2](#22-读表之前必须知道的三条结构事实)、[§3.1](#31-postgresql含-questdb--cloudberry-复用)、[§3.2](#32-mysql含-mariadb--doris--starrocks--manticore--ob_oracle-复用)、[§3.4](#34-sql-server)、[§4.1](#批次-0契约测试模板不接任何真实驱动)、[§4.2](#批次-1代表驱动每种语义模型各一个)、[§7.1](#71-头号违反点13-处-cancel_query-的-no-op-成功)、[§7.2](#72-逐能力的拒绝断言要求)、[§4.5](#45-批次顺序的硬性依赖) |
| 不同 driver 不共享实现库类型 | （边界门禁，非 CM 用例） | [§1.2](#12-http-support共享-helper-crate不是驱动)、[§8.1](#81-允许与禁止的划分) |
| 新增能力缺失不会 no-op 成功 | CM-24 + §7.2 逐能力拒绝断言 | [§2.2](#22-读表之前必须知道的三条结构事实)（事实一）、[§7](#7-unsupported-的拒绝断言要求) 全章 |

三点必须说清楚，否则这张表会被当成比原文更强的承诺：

- **CM-08（两个编辑器隔离，H/D/F）此前在本文完全没有出现过**，而它落在门槛区间 `CM-07～19` 内。批次 0 的模板现在显式承接它，见 [§4.1](#批次-0契约测试模板不接任何真实驱动) 的必做断言。
- **CM-14（字符串、注释、过程与批次，D）与 CM-16（替换失败保留旧连接，H/D/F）同样在门槛区间内**。它们只在部分驱动上被触发，但不能因此省掉对应驱动的 D 层用例：CM-14 是 D 层单值判定，必须逐驱动跑；CM-16 是**一切** `requiresReplacement` 驱动的必做项，当前矩阵里就是 postgres。
- **「驱动测试只写在驱动 crate 内」不是 P2 退出门槛**，它是 `AGENTS.md` 的常驻规则。本文 [§4](#4-迁移批次) 开头的四种落点表是执行该规则的产物，属于门槛的达成手段，不属于门槛本身，因此不列在上表。

### 10.2 P10 目标：通用 contract × driver 矩阵

开发计划 P10 要求产出「通用 contract × driver 矩阵」。本文的 [§2.1](#21-矩阵) 就是这张矩阵的**当前**列，[§4](#4-迁移批次) 是它的**目标**列，两者一一对应：

- 每一行是一个 path 驱动，行为固定不变（`postgres` 不会在后续新增或合并）。
- 每一列是 §5.2 的一条能力 + `namespaceShape`，行为固定不变：能力集合只增不改，取值集合只按 §5.3 的规则演进。
- 矩阵的填充规则固定：**已核实格由代码推出、待验证格必须走 P0、禁止猜测**。
- 矩阵的变更规则固定：任何一格的改动只能发生在其所属批次的进出门槛通过之后，且必须附 D 层用例证据。

因此 P10 读这张表时可以直接回答「某个能力在哪些驱动上可用、在哪些驱动上必须拒绝」——前者是 `supported`/`verified`/`inPlace` 的格，后者必须配 §7.2 的对应拒绝断言。

### 10.3 阶段完成的判据

P2 完成时，上表所有章节都应转为「已实现」：§2.1 不再有「待 P0 验证」格、§9 清单清空、§7.1 的 13 处 no-op 全部改为显式拒绝、§6.2 列出的宿主侧驱动分支（`driver_has_multi_database`、`close_database` / `get_open_databases`、dispatcher 的 `discard_connection` 调用）全部删除或替换为契约调用。按 `AGENTS.md` 文档纪律，届时应把本文的「当前」内容改写为已实现事实并入 [连接管理](connection-management.md)，本文转为协议与矩阵的长期维护说明。

## 与既有设计的关系

- **[连接与资源运行时设计](connection-management.md)** 是本文的上游契约：§3 借用它的 `ResourceDescriptor` 七项必填字段与 `namespaceShape`，§4 的批次以它的 §16 用例编号为门槛，§6 沿用 §5.2 的「遗留驱动可以支持独立受控操作」约束与 §9.4 的归池前检查，§7 的「不得静默 skip」引自 §17。本文**不重新定义**它的任何枚举取值、任何 trait 方法、任何用例编号。
- **[平台开发计划](../../development/platform-development-plan.md)** 是本文的阶段归属：本文的批次划分对应 P2 的交付与退出门槛，§10.2 对应 P10 的「通用 contract × driver 矩阵」。本文只解释「怎么清点、怎么分批、怎么升版本」，不重复计划里已有的排期与里程碑。
- **[Driver API 依赖边界](../../development/driver-api-dependency-boundary.md)** 是 §8 的规范来源：公共接口只用标准库类型、API 自有类型和 serde，宿主类型、Tokio、HTTP 客户端、SQLx 的禁止范围由该文档定义，本章只补充「私有实现依赖 vs 公共 API 表面」的区分与改动后的门禁命令。
- **[可选驱动](../../development/optional-drivers.md)** 与 **[独立驱动开发指南](../../development/independent-driver-development.zh-CN.md)** 是 §1.3 与 §4 测试落点的来源：git 驱动钉 ref 升级、驱动专属测试只写在 `packages/drivers/<id>/` 之下、禁止把驱动专属测试加到 Host，均引自这两份文档。
- **代码事实**（而非上述任何文档）是 §2 矩阵与 §7.1 清单的唯一证据来源。凡本文与代码不一致，以代码为准并回头修订本文。

## 本文不覆盖什么

- **不定义 `capabilities.rs` / `resource.rs` / `session.rs` 的具体 API 形状。** §5.1 的 9 个操作、§4 的 DTO 字段、§5.2 的能力枚举都由连接管理定义；本文只说明它们对现有 trait 的映射关系，不新增方法、不新增类型、不重命名。
- **不设计 Host 侧运行时。** 连接池、Tokio actor、channel、semaphore、`PoolKey`、`BudgetPort` 的实现、Lease/Attachment 状态机、结果流与背压策略属于 Host 连接运行时（P3 及之后），不在本文范围。本文只在 §8.2 说明驱动如何配合预算计数。
- **不覆盖 git 驱动（kiwi / olap / superset）的能力矩阵。** 它们在 `drivers-registry.json` 里是 `source: "git"`，不在 workspace 内，`describeResource` 只能由各自仓库实现。本文只在 §1.3 记录它们的存在与计数差异，在 §5.4 记录它们的版本升级机制（钉 ref 升级）。
- **不覆盖前端 UI 与 i18n 的驱动能力呈现。** 能力如何在 UI 上呈现、降级路径如何标注，属于前端设计与宿主绑定层（见独立驱动开发指南 §6），本文只规定「必须标注降级、不得呈现为成功」这条行为要求。
- **不排期、不列进度、不列缺陷清单。** 按 `AGENTS.md` 文档纪律，本文是一次性设计产物；落地后改写为已实现事实并入对应架构文档。
- **不重新评估 `AGENTS.md` 中已有的驱动选型、双版本构建、四维扩展体系与测试落点规则**，只引用。
- **不代替真实测试环境。** §9 的「待 P0 验证」项必须由真实服务端测试环境下的 D 层用例判定；按 §17，测试环境不可用时必须报告未验证范围，不能用 fake 代替真实协议结论。
