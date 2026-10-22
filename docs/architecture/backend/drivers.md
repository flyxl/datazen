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
- `DriverEventOutlet` / `DriverEventSink`（形态中立的事件出口，见 §2.11）

公共 API 不暴露 sqlx、mongodb、redis 等数据库实现类型；具体连接池、Row、事务实现由 Driver 自己管理。公共 API 也不暴露任何**宿主形态**的类型（`tauri::Emitter` 之类）——驱动能看到的宿主面只有本文件列出的这些契约。

当前 API：

- `PROTOCOL_VERSION = 6`
- `MIN_PROTOCOL_VERSION = 1`

## 2. DatabaseDriver

Driver 基础能力包括：

- connect / test_connection / disconnect
- get_databases / get_tables / get_table_schema
- query / query_multi / query_stream
- query_with_params / execute / execute_with_params
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

Host 生成的写语句必须把 SQL 模板与 `Vec<Value>` 分开，通过 `parameter_placeholder()` 和 `execute_with_params()` 传值；没有绑定写能力的驱动返回 `Unsupported`，Host 在开启事务前拒绝该批写入。`build_update_statement()` / `build_delete_statement()` 生成的预览只含占位符，不能把值插入 SQL 文本。

SQL 筛选与 SQL 文件产物使用 `try_format_sql_literal()`。每个 SQL 驱动显式声明 `SqlLiteralDialect`，共享格式器按方言使用对会话转义模式稳定的表示（例如 MySQL 的 UTF-8 十六进制转换、PostgreSQL / DuckDB 的 dollar-quoted 文本）；未声明方言时返回 `Unsupported`，不会猜测反斜杠规则。无方言的键值 / 文档驱动不应调用 SQL 字面量接口。旧的无结果 `format_sql_literal()` 和内插式 `build_*_sql()` 仅保留兼容，产品写路径不使用它们。

### 2.1 Schema 元数据目标

`get_tables`、`get_table_schema`、`get_columns` 和 `get_all_columns` 都接收显式 `database` 与可选 `schema`，驱动按传入目标读取，不依赖会话当前选中的数据库。Host 的 `list_catalog`、`read_relation_columns`、`read_relation_schema` 和 `refresh_schema_metadata` Driver Commands 以 `RelationRef { database, schema, name }` 表达目标；前端与扩展共用 Driver Command 网关，不再需要按数据库类型选择旧 Host schema IPC。

有 schema 层级的驱动要求读取时提供精确 schema；schema-less 驱动使用 `null`。会话身份由 command envelope 的 `dbSessionId` 提供，与持久化连接配置 ID 分开传递。Host 缓存按 session、database、schema 和 relation 名称隔离，刷新支持 session、database 或 relation 范围。详见 [services.md](services.md) 的 Schema Metadata Gateway 与 [cache.md](cache.md) §1.8。

### 2.2 分页契约（PaginationSyntax）

Host **不拼任何方言分页子句**：每次读取都向驱动要 `pagination_syntax(limit, offset)`。

| 字段 | 含义 |
| --- | --- |
| `clause` | 追加在 `ORDER BY` 之后的分页子句 |
| `requires_order_by` | 该子句是否要求语句已有 `ORDER BY` |
| `order_by_fallback` | 调用方未指定排序时用于占位的排序表达式 |

- 默认实现输出 `LIMIT {n} OFFSET {m}`；驱动声明 `supports_offset() == false`（`superset`）时自动退化为 `LIMIT {n}`。
- `requires_order_by == true` 时，Host 仅在**调用方没有排序**时补 `order_by_fallback`；用户显式排序优先，不叠加兜底排序。
- 调用点：`packages/feature-core/src/services/query_executor.rs::build_select_sql`、`packages/data-transfer/src/execute.rs`、`packages/data-sync/src/keyset.rs`、`src-tauri/src/commands/sync/keyset_source.rs`。`data_transfer` 仍以 `supports_offset()` 作为翻页循环的终止条件（不能 OFFSET 的引擎不继续翻页），不参与 SQL 拼接。
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

### 2.4 领域契约：`SqlDialect` 与共享 SQL builder

`DatabaseDriver` 正在收敛为「驱动入口」，SQL 文本规则已先拆成窄契约，运行时执行仍走 `ResourceProvider`。

| 契约 | 位置 | 现状 |
| --- | --- | --- |
| `SqlDialect` | `packages/driver-api/src/dialect.rs` | 已提取：`quote_ident`、`pagination_syntax`、`try_format_sql_literal`、`parameter_placeholder`、`max_bound_parameters` |
| `LegacySqlDialect<'a, D>` | 同上 | 借用适配器：方法逐项转发给 `DatabaseDriver`，是**单向**迁移缝 |
| 共享 SQL builder | `packages/driver-api/src/sql_builder.rs` | `build_update_sql` / `build_delete_sql` / `build_update_statement` / `build_delete_statement` 只接收 `&dyn SqlDialect` 与调用方显式给出的绑定上限 |
| `CatalogReader` | `packages/driver-api/src/catalog.rs` | 已提取：`get_databases`、`get_schemas`、`get_tables`、`get_table_schema`、`get_columns`、`get_all_columns`，每次关系读取都带显式 `database` / 可选 `schema` |
| `LegacyCatalogReader<'a, D>` | 同上 | 借用适配器，同一单向迁移规则 |

规则：

- **单向委托。** 消费者 → `SqlDialect` / `LegacySqlDialect` → 驱动方法。驱动默认方法不再互相转发；`DatabaseDriver` 的 `build_*` 默认实现委托给共享 builder，旧 override 继续生效。
- **纯文本规则。** `SqlDialect` 的方法不得获取资源、连接或会话状态。共享 builder 因此无需连接池即可生成 SQL，测试可用独立 dialect 实现，不必造驱动。
- **绑定上限有来源。** `BoundParameterLimit` 区分 `Dialect`（方言语法常量）、`Runtime`（服务器或绑定实现限制）与 `Declared`（旧 API 无法区分来源时的声明）。批量写入只读 `limit()`，文档与诊断读来源。
- **兼容入口保留。** `format_sql_literal` / `build_*_sql` 仍存在但已标记为 deprecated，仅由旧调用点使用；产品写路径继续使用 `build_*_statement` + `execute_with_params`。`legacy_format_sql_literal` 是这两个 deprecated builder 的兼容渲染入口，新代码不得调用。
- **能力声明不另起一套。** 本轮只新增文本契约；「是否支持绑定写」「精确取消」「事务」等仍由既有 `CapabilitySet` / `ResourceProvider` 决定，不引入第二个能力集合。
- **旧 trait 方法在被新契约取代后仍然保留。** 借用适配器（`LegacySqlDialect` / `LegacyCatalogReader` / `LegacyCommandCatalog` / `LegacySchemaMigration` / `LegacyBackupDialect`）正是**委托给这些旧方法**的，删掉它们等于删掉契约的委托目标，新契约会失去唯一实现——那会变成两份实现或互相转发，违反「同一项行为只有一个实际实现」。因此可删的旧方法只有一种：**driver-api 之外零消费者，且契约侧已有自有实现**。`DatabaseDriver::split_restore_sql` 即按此退役（`BackupDialect` 的实现改为直连 `sql_text::split_restore_sql`，全仓 15 个驱动无一显式声明过该方法，删除对现存实现方零编译影响）；`uses_sql_restore_pipeline` / `new_sql_scanner` / `type_normalizer` / `format_sql_literal` 等仍是被适配器委托的方法，**保留**。

已迁移的消费者：

方言规则（`SqlDialect`）——`quote_ident` / `pagination_syntax` / `try_format_sql_literal` / `parameter_placeholder` / `max_bound_parameters` 全部经 `LegacySqlDialect`：

- `QueryExecutor::build_select_sql` / `build_count_sql`（宿主读表）接收 `&dyn SqlDialect` 与 `limit`/`offset`；`commands/table_target.rs` 在边界处用 `LegacySqlDialect` 包装驱动。
- 数据迁移写入路径：`data-transfer` 的绑参上限 / 占位符 / 分页子句 / 字面量（`writer.rs`、`dispatcher.rs`、`resume.rs`、`resume/fingerprint.rs`、`job/pipeline.rs`、`job/handler.rs`、`sql_file.rs`），宿主 `packages/data-transfer/src/preview.rs`、`src-tauri/src/commands/data_transfer/exec/execution.rs`，feature-core `export/format.rs`（`emit_insert_batch` 形参即是 `&dyn SqlDialect`）；`bound_insert*` 因同时需要驱动专属的显式自增子句而保留驱动形参。
- 结果编辑校验与同步批读：`commands/result_write.rs` 的 UPDATE 前置 SELECT、`sync/keyset_source.rs`、`sync/host/select.rs`（形参已收窄为 `&dyn SqlDialect`）、`sync/host/statements.rs`、`sync/apply.rs`、`sync/filter_validation.rs`。
- 驱动专属的规则（`quote_char` / `transfer_explicit_identity_insert_clause` / `supports_offset` / `driver_type`）不在此契约内，仍在驱动上显式调用。
- 同步 IR 适配器的 `SyncTargetAdapter::quote_ident` 是另一个契约（SQL Server 用 `[x]`、ClickHouse 用反引号，与驱动方言的有意分歧），不得并入。

目录读取（`CatalogReader`）——`get_databases` / `get_schemas` / `get_tables` / `get_table_schema` / `get_columns` / `get_all_columns` 全部经 `LegacyCatalogReader`：

- `SchemaCache::get_columns` / `get_table_schema` / `warmup` 只接收 `&dyn CatalogReader`；读表、ER 图、AI 过滤解析与 `read_relation_columns` 在边界处用 `LegacyCatalogReader` 包装驱动。缓存键仍是 session + database + schema + relation，跨目标读取不共享条目。
- Schema Diff 目录读取：`schema_diff` 的 plan / compare / unified plan / dependencies / recovery 与 `feature-core` `schema_metadata.rs` 的 `list_catalog` / `read_relation_schema`（driver 形参同时承载 `driver_type()`、外键目录可见性等驱动专属规则，适配器在函数内构造）。

其余消费者按调用链逐步迁移；收窄函数形参（如 `fetch_target_table_schema`）留待其调用方迁完后统一做。

版本：`datazen-driver-api` 0.0.13 → 0.0.21——0.0.14 `SqlDialect`、0.0.15 `CatalogReader`、0.0.16 `ReuseDriver` 显式转发补齐、0.0.17 `CommandCatalog`、0.0.18 `SchemaMigration`、0.0.19 `BackupDialect`、0.0.20 退役 `DatabaseDriver::split_restore_sql`（唯一一次 trait 表面删除，故单独记一版）、0.0.21 事件出口契约（`DriverEventOutlet` / `DriverEventSink`，见 §2.11；纯新增 trait，无既有 trait 表面变动）。新契约本身都是 additive（新 trait + 适配器），`PROTOCOL_VERSION` 保持 6（2026-10-21 起协议版本冻结于 6，兼容门禁族退役——治理决定见 §2.5）；crate 版本记录的是**本仓库**的 trait 表面变化，仓库内各驱动按最新 trait 实现即可。

### 2.5 协议版本闸门

`packages/driver-api/src/lib.rs` 定义两个常量：`PROTOCOL_VERSION = 6` 与 `MIN_PROTOCOL_VERSION = 1`。同处的文档注释写明契约语义：**版本低于 `MIN` 的驱动被拒绝加载**；**`MIN <= version < PROTOCOL_VERSION`** 的驱动以**降级模式**运行，缺失的能力按 `false` 处理。

宿主侧有两处闸门，结构相同，但**只拒绝对老版本，不拒绝更新版本**：

| 闸门 | 位置 | `pv < MIN` | `pv > PROTOCOL_VERSION` | `MIN <= pv < PROTOCOL_VERSION` |
| --- | --- | --- | --- | --- |
| 驱动 | `packages/runtime/src/driver_registry/registry.rs` 的 `register_from_inventory` | `return Err`，硬拒绝、不创建驱动 | 只 `tracing::warn!`，照常加载 | `warn!` 提示降级后照常加载 |
| AI Provider | `packages/ai-providers/src/registry.rs` 的 factory 遍历（`AI_PROTOCOL_VERSION` / `MIN_AI_PROTOCOL_VERSION` 是 `packages/ai-api` 的另一套常量） | `tracing::error!` 后 `continue`，跳过注册 | 只 `warn!`，照常注册 | `warn!` 降级提示 |

**降级模式并不真的把能力改写成 `false`**：两处降级分支只打一条日志，消息里回显的是 factory **自己声明**的能力值（驱动侧 `supports_cancel_query` / `supports_explain` / `supports_streaming_results`，Provider 侧 `supports_streaming` / `supports_tools`），驱动随后照常 `create()` 并按自己声明的能力运行。这是 `driver-api` 侧写下的契约承诺与宿主当前实现之间的已知差距。

**版本怎么声明**：`DatabaseDriverFactory::protocol_version() -> u32` 的默认体返回 `crate::PROTOCOL_VERSION`，**本仓内没有一个 path 驱动覆盖它**——仓库内驱动永远自动跟随 `driver-api` 的协议号，协议号对它们是纯记录项，不是需要各自维护的状态。git 驱动是否覆盖取决于其自身仓库。

**plan 级复核**：数据迁移另有独立校验。plan 携带 `source_driver_protocol` / `target_driver_protocol`，执行时再次核对（Data Transfer 的 `src-tauri/src/commands/data_transfer/exec.rs` 与 `job_api/assembly.rs`，Data Sync 的 `src-tauri/src/commands/sync/exec.rs`），取值统一走 `datazen_driver_api::driver_protocol_version`（`packages/driver-api/src/factory.rs`）——Data Sync 与 Data Transfer 共用同一实现；驱动未链接进本次构建时回落到编译期的 `PROTOCOL_VERSION`。

**治理决定（2026-10-21）**：仓库当前没有第三方驱动生态（path 驱动全部在本仓内，git 驱动由 registry 的 `git` 字段按需克隆，`ref` / `commit` 可选钉选——当前 kiwi / superset 条目未钉 ref），协议版本随之**冻结在 6**，`MIN` 停在 1。此前维护的逐符号四档协议兼容机检（`scripts/check-driver-protocol-compat.mjs`、`scripts/check-driver-protocol-release.mjs`、其规则表与测试，以及 `package.json` 的 `test:driver-protocol` / `test:driver-protocol-release` 与 CI 对应步骤）**整体退役删除**——在「零外部驱动 + 版本冻结」的前提下，逐符号机检的维护成本高于其收益（门禁历史仍可从 git 历史找回）。

冻结期内「声明更高协议版本的驱动」不可能出现，`pv > PROTOCOL_VERSION` 分支只是一条空路径；若未来重新出现外部驱动生态，**必须先定案该分支的处置**（硬拒绝，或加载并标记为不可用，且不得以下调 `MIN` 绕过）再接入外部驱动。

契约演进退回朴素约束：**改公共契约 = 同一次交付内全树编译绿**，所有仓内驱动与消费方按最新契约一起迁移。保留两条人工判据（机器不查，靠评审纪律）：

| 改动 | 判据 | 处理 |
| --- | --- | --- |
| 新增 trait 方法且带 fail-closed 默认体 | 现有实现方继续能编译 | 直接交付 |
| 新增无默认体方法、删除/改写签名、改枚举穷举性 | 现有实现方必须整改 | **同一次提交内**迁移全部仓内实现方与消费者；版本号不动（冻结），`MIN` 不下调 |

另有两条仍然成立的人工红线：

1. **不得下调 `MIN_PROTOCOL_VERSION` 来掩盖契约不兼容**——那会把兼容问题变成运行期静默降级。`MIN` 在历次演进中停在 `1`，动它是产品决定，不是技术操作。
2. git 驱动接入时，其兼容性核对发生在**接入那一刻**（clone 后一次全树编译绿），而不是每次交付时。

**`source-breaking` 的判据**：把改动判成 breaking 并升 `PROTOCOL_VERSION` 是谎报——协议号表达的是**线上兼容**，而 `#[non_exhaustive]`（FOREVER 冻结）这类改动线上一个字节都没变：已经编好的老驱动照常运行，只有**还没编译**的代码编不出来。正解是只动 crate 版本，并在说明里写清楚「以后不能再加字段」与迁移配方。判成 additive 同样是谎报：它对还没编译的代码承诺「你没事」，而那正是会炸的地方。

### 2.6 备份能力的判定方式

「这个驱动能不能备份」由三处**互相独立**的事实决定，任何一处都推不出另一处：

| 面 | 位置 | 作用 |
| --- | --- | --- |
| UI 入口是否可达 | 驱动 UI 元数据 `supportsBackup`（`packages/drivers/<id>/ui/meta.ts`） | 前端连接导航菜单据此决定要不要给出「备份 / 恢复」入口 |
| 声明式能力 | `CapabilitySet.backup`（`BackupSupport`：`ArtifactAndRestore` / `ArtifactOnly` / `RestoreOnly` / `Unsupported` / `Unknown`，`packages/driver-api/src/capability_domains.rs`） | 逐驱动声明备份域的档位 |
| 通用 dump 的运行前提 | `DatabaseDriver::dump_database_with_progress` 的默认实现（`packages/driver-api/src/traits.rs`）→ `sql_dump::dump_sql_database_with_progress`（`packages/driver-api/src/sql_dump/dump.rs`） | 通用 SQL dump 对驱动只有两项要求：`get_tables` 与 `get_table_schema` 必须是**真实现** |

**判据不是「有没有覆写 dump」，而是「通用实现能不能真的跑起来」。** 默认实现把工作交给 `sql_dump::dump_sql_database_with_progress`，它对驱动只有两项要求：`get_tables` 枚举可 dump 的对象清单，`get_table_schema` 渲染 DDL 并生成 `INSERT INTO` 的列清单。其余环节都不是门槛：`dump_routines` / `dump_triggers` 的缺省是空串；`dump_view_ddl` 的缺省确实返回 `NotSupported`，但通用实现把它接住并写成一行注释跳过。

**判定顺序：先看有没有覆写并显式拒绝，再看 `get_tables` / `get_table_schema`。** 覆写 `dump_database_with_progress` 并显式拒绝的驱动根本不会执行通用实现，顺序反过来就会把「显式拒绝」误判成「缺实现」。

`CapabilitySet.backup` 的两个入口 `require_backup_artifact()` / `require_restore_from_artifact()`（`packages/driver-api/src/capabilities.rs`）目前**只有测试调用**，尚未接进备份执行路径——执行路径是 `packages/feature-core/src/backup.rs` 直接调驱动的 `dump_database_with_progress`。声明与执行之间还没有自动咬合，接线前不要把「已声明」读成「已生效」。

### 2.7 复用包装器：`ReuseDriver` 必须显式决定每个方法

`ReuseDriver` 把内层驱动以另一个 id 呈现（QuestDB / Cloudberry 复用 PostgreSQL 线协议，Doris / StarRocks / Manticore / OceanBase MySQL 模式复用 MySQL）。不变量只有一条：**`DatabaseDriver` 的每个方法都必须在包装器里显式决定**——转发，或有意覆写。不允许某个方法落到 trait 默认体。

原因是默认体为通用驱动而写，静默降级正好抹掉别名引擎的规则：`has_multi_database` 固定返回 `false`、`get_schemas` 返回 `None`、`qualify_sql_target` 返回 `None`、`migration_renderer` 返回 `None`。而同一个包装器却在转发 `open_databases` / `close_database`（按库池操作）——同一驱动出现两份互相矛盾的事实；产品元数据（`questdbMeta.hasMultiDatabase = true`、`defaultPort = 8812`）又是第三份。

已补齐的转发（此前缺失 16 个）：

| 类别 | 方法 |
| --- | --- |
| 方言 / 文本 | `quote_ident`、`build_update_statement`、`build_delete_statement` |
| 目标与端点 | `has_multi_database`、`qualify_sql_target`、`default_host`、`default_port` |
| 目录 | `get_schemas` |
| 结构迁移 | `migration_renderer`、`migration_capabilities`、`validate_schema_migration`、`validate_schema_migration_plan` |
| 执行 / 流式 | `query_stream_with_params`、`query_stream_with_execution_at` |
| 备份 / 传输 | `render_transfer_identity_sequence_sync_sql` |
| 提示 | `dialect_notes` |

有意不转发的只有三类，且都在包装器里有注释：

- `driver_type`：别名 id 正是包装器存在的理由。
- `test_connection`：回写别名的 `server_type`。
- 精确取消协议：由构造参数门控，并且仍要求内层声明支持。

机械守卫：`packages/driver-api/src/reuse_forwarding_tests.rs` 读取 `traits.rs` 与 `reuse.rs` 的源码，断言 trait 的每个方法都出现在包装器实现中，并带正/反控制用例。`DatabaseDriver` 以后新增方法而包装器不更新，`cargo test -p datazen-driver-api` 直接失败；这条不变量不再依赖评审。

### 2.8 命令描述契约：`CommandCatalog`

Driver Command 的"声明"与"执行"分成两个契约。`CommandCatalog`（`packages/driver-api/src/command_catalog.rs`）只包含描述：`command_definitions()`（该驱动暴露哪些命令）与 `target_requirements(command)`（每条命令需要什么目标；未知命令返回 `None`，平台据此在获取资源之前拒绝）。它刻意不包含执行路径——一个只会列出命令的宿主面（目标元数据、workflow 校验、Runtime 只读命令源）依赖 `CommandCatalog`，而不是整个 `DatabaseDriver`；执行继续走 `execute_command` 这一既有的资源生命周期入口。

`LegacyCommandCatalog` 是自 `DatabaseDriver` 到 `CommandCatalog` 的单向适配器，两个方法全部转发，驱动保留且仅保留一份实现。已迁移的调用点：workflow 步骤校验（`resource_execution.rs`）、Runtime 命令源（`runtime_command_source.rs`）、流式查询网关（`streaming.rs`）、平台 `explain` 能力判定（`adapter.rs`、`session_backend/execution.rs`）。

### 2.9 结构迁移契约：`SchemaMigration`

Schema Diff 领域消费的结构迁移决策统一收敛到 `SchemaMigration`（`packages/driver-api/src/schema_migration.rs`）：`migration_renderer()` / `migration_capabilities()` / `type_normalizer()` / `ddl_atomicity()`，加上两个部署期校验（`validate_schema_migration_plan`、`validate_schema_migration`）。领域侧（`schema-diff` 的 plan/objects/deploy/transaction、`data-sync` 同族类型等价门、宿主 schema_diff 命令）只读这份窄契约，执行渲染出的 DDL 仍走各自的资源生命周期；`MigrationRenderer` / `MigrationCapabilities` / `TypeNormalizer` 保持驱动专属实现。

`LegacySchemaMigration` 是自 `DatabaseDriver` 到 `SchemaMigration` 的单向适配器，六个方法全部转发。已迁移的调用点通过它读取规则：同族/跨方言结构 plan（`plan.rs`）、对象 plan（`objects.rs`、`object_plan_commands.rs`、`unified_plan.rs`）、部署原子性与校验（`deploy.rs`、`transaction.rs`、`deploy_commands.rs`）、同步类型等价（`types_eq.rs`、`compare_commands.rs`）。

### 2.10 备份文件规则契约：`BackupDialect`

备份域中真正"纯规则"的部分收敛到 `BackupDialect`（`packages/driver-api/src/backup_dialect.rs`）：`uses_sql_restore_pipeline()`（共享 SQL 恢复管线的路由）、`new_sql_scanner()`（语句扫描器，选择/关闭 `DELIMITER` 处理）、`split_restore_sql()`（整段 SQL 缓冲的切分）。带 `ConnectionHandle` 的执行入口（`dump_database*`、`restore_sql*`、`recover_restore_statement`）不在此契约内，保留在驱动上，待运行时路径收敛（§5 计划）统一处理；共享流程本身继续复用 `sql_dump` 模块。

`LegacyBackupDialect` 是自 `DatabaseDriver` 到 `BackupDialect` 的单向适配器。已迁移的调用点：feature-core 备份流程（`backup.rs` 的管线路由与 `RestoreSession` 扫描器；执行侧恢复循环继续持有驱动）、retained-restore 预检 `restore_sql::prepare`（scanner 取自 `BackupDialect`，feature-core `restore_gateway.rs` 与 postgres/mysql 驱动的 `restore_sql` 集成测试在边界包适配器）。

### 2.11 事件出口契约：`DriverEventOutlet` / `DriverEventSink`

有些驱动会产生「给人看的事件」——Redis 驱动的 Pub/Sub 消息是现成例子。事件怎么送达用户**不是驱动能决定的事**：桌面形态用 Tauri 事件发给前端，server 形态会用它自己的传输通道。契约因此拆成两半，与 driver factory 同一形态（`packages/driver-api/src/event_sink.rs`）：

| 契约 | 归属 | 作用 |
| --- | --- | --- |
| `DriverEvent` | 驱动 | 一条事件：稳定事件名 + 已序列化 payload |
| `DriverEventOutlet` | 驱动 | 声明「我有一个事件出口，宿主给我一个 sink」；`outlet_id()` 是出口标识，`install(sink)` 接受宿主给的 sink |
| `DriverEventSink` | 宿主 | 事件的目的地，每个形态一个实现；`emit(&DriverEvent)` 把事件送达该形态的消费者 |
| `register_event_outlet!` | 驱动 | 链接期注册（`inventory`），与 `register_driver!` 同形 |
| `iter_event_outlets()` | 宿主 | 启动期遍历收集已链接进来的出口 |

两条不变量：

- **链接期常量只有「有一个出口」。**「装哪个 sink」是宿主在**启动期**的运行时决定：宿主遍历 `iter_event_outlets()`，把自己形态的 sink 装进每一个出口。宿主不需要知道本次构建链接了哪些驱动（驱动是编译期选型，宿主也不可能静态引用某个具体驱动）。
- **sink 未装之前的事件按驱动自己的策略处理。** Redis Pub/Sub 是**静默丢弃**（不缓存、不补发），与改造前的行为逐字一致；契约不替驱动决定这条策略。

形态中立是硬要求：契约里不得出现任何宿主类型。改造前 Redis 的注入通道是一个 Tauri 插件（驱动 crate 内的 `plugin.rs` + `build.rs`，均已删除），于是 `tauri` 进了驱动的可选依赖闭包 —— server 形态要支持 Redis Pub/Sub 就必须启用 `tauri-plugin` feature，把 Tauri 拖进 server 依赖闭包，直接撞上[共享边界 §2.2](../platform/shared-boundaries-and-ports.md)对 server 的禁令。**Pub/Sub 曾经是「桌面独占」能力，根因是注入通道的形态，不是能力本身。** 换成 `inventory` 注册后，驱动的 `tauri` 可选依赖、`tauri-plugin` 构建依赖、`build.rs` 与插件 ACL 目录一并删除，同一份驱动 crate 可被 server 形态复用。

桌面形态的实现见 `src-tauri/src/platform/event_sink.rs`（把驱动事件转成 Tauri 事件，事件名与 payload 逐字转发）。

## 3. Driver Commands

Driver 可以通过 `command_definitions()` 声明 Command，通过 `execute_command()` 执行。

Host / Workflow 只依赖 Command Definition 和 JSON input/output，不按 database type 复制 Driver-specific dispatch。

标准 SQL Driver 默认提供 query / execute，以及 schema catalog commands；Driver 可以扩展管理、KV、NoSQL 等 Command。

**Command 的 `permissions` 字段不是 Tauri ACL 权限。** `DriverCommandDefinition::permissions`（`packages/driver-api/src/command.rs`）里的标识符是 **DataZen 自己的权限词表**，用途只有风险分类：`required_access_level()` 与 `check_command_access()` 按它把命令分成读 / 写 / 高危三档。`redis:allow-info`、`redis:allow-set-string` 这类名字**长得像** Tauri ACL 权限，但两者不是一回事：

| | Driver Command 权限词表 | Tauri ACL 权限 |
| --- | --- | --- |
| 来源 | 驱动在 `command_definitions()` 里自己写的标识符 | 驱动的 Tauri 插件 `permissions/` manifest（构建期生成的 ACL 权限清单） |
| 消费方 | `required_access_level()` / `check_command_access()`——只做风险分类 | tauri-build 的 ACL 校验与 `src-tauri/capabilities/default.json` |

redis 驱动已随「Pub/Sub 事件出口改走形态中立 inventory 注册」删除 Tauri 插件与 `permissions/` 目录（PR #95），驱动侧不再有任何 `tauri` 依赖；因此**仓库里现在不会生成任何 `<id>:default` 权限**——`scripts/resolve-drivers.mjs` 的 `syncDriverCapabilities` 只为声明了非空 `tauriPlugin.commands` 的驱动并入 `<tauriPlugin.id>:default`，而当前没有任何驱动满足该条件。读到 `<id>:allow-*` 时不要拿它去 capabilities 文件里找对应条目。

### 3.1 驱动推送事件的归属字段

除「Host 调 Driver」外，驱动还可以**向 Host 推送事件**（Tauri event）。推送事件的载荷里如果带身份字段，那个字段必须是 **Host 认得的 id**，而不是驱动内部的句柄——否则前端按它过滤时永远匹配不上，事件会被静默丢弃。

Redis Pub/Sub 是当前唯一的这类事件：事件名 `redis-pubsub-message`，载荷由 `packages/drivers/redis/src/ops/pubsub.rs` 的 `RedisPubSubMessageEvent` 序列化（camelCase）：

```text
{ connectionId, subscriptionId, channel, payload, ts }
```

`connectionId` 的取值是**持久化连接配置 id**（Host 的 `connectionId`），不是驱动自己的运行时句柄 `ConnectionHandle.pool_id`（形如 `redis_<uuid>`），也不是 Host 的 `dbSessionId`（形如 `dbs_<hex>`）——三者分属不同 id 空间，只有第一个是前端拿得到、也拿得准的那个。驱动在 `RedisDriver::connect` 时把 `ConnectionConfig.id` 记进连接记录，订阅时据此给事件归属。

两侧的 id 各司其职，不做双模回退：

| 面 | 用哪个 id | 理由 |
|---|---|---|
| 调用侧（`pubsub_subscribe` / `pubsub_unsubscribe` / `pubsub_publish`） | `dbSessionId` | 对已建立会话的操作（[naming.md](../naming.md) §5） |
| 事件侧（`redis-pubsub-message` 载荷） | `connectionId` | 归属语义：这条消息属于哪份连接配置（同上 §5「UI 连接归属」） |

订阅本身建在**会话**上（`pubsub_subscribe` 用 `dbSessionId` 调用，Host 解析出活会话后驱动用该会话的连接计划开订阅，随会话销毁），但 Redis Pub/Sub 的频道是**服务器级**而非会话级的，所以事件按连接归属。

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

Driver Registry 实现在 `packages/runtime/src/driver_registry/`（`datazen_runtime::driver_registry`），与 SchemaCache 同处共享 runtime 内核。宿主原 `src-tauri/src/db/` 门面已删除，连接层统一直接引用 `datazen_runtime::driver_registry` 与 `datazen_driver_api`。

Driver 通过 inventory 在编译期注册。构建时由 registry / build tooling 决定哪些 Driver 被编译进 DataZen。

## 7. Driver 开发

独立 Driver 推荐使用独立 Git repository，通过 DataZen registry 的 `source: "path"` 在本地宿主中调试，发布后可使用固定 commit 的 Git dependency。

详见 [独立驱动开发指南](../../development/independent-driver-development.zh-CN.md)。
