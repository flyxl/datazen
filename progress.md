# Driver 领域化的进度台账（交付前删除）

> 本文件只在开发期间存在。AGENTS.md 允许它在分支根目录记录并行开发事实，
> 验收合并时必须删除。结论必须同时落在代码、测试与 `docs/` 里，不能只活在这里。

## 分支与工作树

- 分支：`refactor/driver-domain-contracts`
- 工作树：`.agents/worktrees/refactor/driver-domain-contracts`
- 基底：`59ea5237b`（main 已还原到该提交；本分支不受 main 后续变动影响）
- 提交：
  - `d852b2c24` refactor(driver-api): extract SqlDialect and shared SQL builder
  - `e837444ff` refactor(driver-api): extract CatalogReader and detach schema cache from the driver
  - `1b06ab884` fix(reuse): decide on every DatabaseDriver rule in ReuseDriver
  - `3a4a2c982` chore(ledger): record driver-domain refactor progress on this branch
  - `3c6fedf92` refactor(driver-api): extract CommandCatalog and move command descriptions off the driver
  - `f438ae588` chore(ledger): record CommandCatalog round on this branch
  - `5f5817aba` refactor(driver-api): extract SchemaMigration and read migration rules off the driver
  - `64ea54a35` refactor(driver-api): extract BackupDialect for restore-file rules

## 已完成（对应方案 §六 的 PR 顺序）

### 1. 方言接口试点（`d852b2c24`）

- `packages/driver-api/src/dialect.rs`：`SqlDialect`（quote_ident / pagination_syntax /
  try_format_sql_literal / parameter_placeholder / max_bound_parameters）、
  `LegacySqlDialect` 单向借用适配器、`BoundParameterLimit`
  （Dialect / Runtime / Declared 三种来源）。
- 契约登记：`scripts/lib/driver-protocol-rules.mjs` 增加
  `sql-dialect` / `legacy-sql-dialect` / `bound-parameter-limit`。
- 消费者迁移：`QueryExecutor::build_select_sql` / `build_count_sql` 改为接收
  `&dyn SqlDialect` 与 `limit`/`offset`；`commands/table_target.rs` 在边界处包适配器。

### 2. SQL builder 解耦（同上提交）

- `packages/driver-api/src/sql_builder.rs`：四个共享 builder 只接收 `&dyn SqlDialect`
  与调用方显式给出的绑定上限；`DatabaseDriver::build_*` 默认实现委托给它，
  旧 override 仍生效；超限在返回 SQL 前失败。
- `format_sql_literal_ansi` 从 `traits/sql_text.rs` 移入，保留旧字面量默认体语义。

### 3. 目录读取试点（`e837444ff`）

- `packages/driver-api/src/catalog.rs`：`CatalogReader`（显式 database/schema 目标）
  + `LegacyCatalogReader` 适配器。
- `SchemaCache::get_columns` / `get_table_schema` / `warmup` 只依赖
  `&dyn CatalogReader`；读表、ER、AI 过滤解析、`read_relation_columns/schema`、
  tool-host 端口在边界处包适配器。
- 契约登记 `catalog-reader` / `legacy-catalog-reader`。

### 4. 复用包装器不丢能力（`1b06ab884`）

- 补齐 `ReuseDriver` 缺失的 16 个转发（含 `has_multi_database`、`get_schemas`、
  `qualify_sql_target`、`default_host/port`、migration renderer/capabilities、
  `query_stream_with_execution_at` 等）。
- `reuse_forwarding_tests.rs` 机械守卫：trait 每个方法必须被包装器显式决定；
  已用变异检验（删掉 `get_schemas` 转发 → 测试按名失败）。

### 5. CommandCatalog 命令描述契约（`3c6fedf92`，2026-10-21）

- `packages/driver-api/src/command_catalog.rs`：`CommandCatalog`
  （`command_definitions` + `target_requirements`）+ 单向适配器
  `LegacyCommandCatalog`；`lib.rs` 再导出；crate 0.0.16 → 0.0.17。
- 已迁移宿主消费点（5 处，只读命令描述；执行仍走既有 `execute_command` 生命周期）：
  feature-core `workflow/resource_execution.rs`、src-tauri
  `services/runtime_command_source.rs`、`commands/driver_command/streaming.rs`、
  `platform/adapter.rs`、`platform/session_backend/execution.rs`（explain 能力判定）。
- 协议规则登记 `command-catalog` / `legacy-command-catalog`；
  `drivers.md` 新增 §2.6。
- 门禁实测：driver-api lib `366 passed`；feature-core `298 passed`；
  host `cargo test -p datazen --lib` `1378 passed; 9 ignored`（worktree 首次，
  codegen 文件已从主树复制）；协议 compat gate additive（crate 0.0.13→0.0.17，
  PROTOCOL 6→6）；vitest gate `68 passed`。

### 6. SchemaMigration 结构迁移契约（`5f5817aba`，2026-10-21）

- `packages/driver-api/src/schema_migration.rs`：`SchemaMigration`
  （renderer / capabilities / normalizer / ddl_atomicity /
  validate_schema_migration_plan / validate_schema_migration，无默认体）
  + 单向适配器 `LegacySchemaMigration`；crate 0.0.17 → 0.0.18。
- 已迁移：schema-diff `plan.rs`（2 处 create_driver→规则读取）、`objects.rs`
  （4 个 object-plan 块）、`deploy.rs` + `transaction.rs`（原子性与两处校验）、
  data-sync `types_eq.rs`、src-tauri schema_diff 命令
  （`compare_commands.rs` / `object_plan_commands.rs` / `unified_plan.rs` /
  `deploy_commands.rs`）；执行渲染出的 DDL 仍走既有资源生命周期。
- 协议规则登记 `schema-migration` / `legacy-schema-migration`；drivers.md §2.7。

### 7. BackupDialect 备份文件规则契约（`64ea54a35`，2026-10-21）

- `packages/driver-api/src/backup_dialect.rs`：`BackupDialect`
  （`uses_sql_restore_pipeline` / `new_sql_scanner` / `split_restore_sql`，
  无默认体）+ 单向适配器 `LegacyBackupDialect`；crate 0.0.18 → 0.0.19。
- 已迁移：feature-core `backup.rs` 管线路由 + `RestoreSession` 扫描器
  （`stream_sql_file_into_session` 增加 `&dyn BackupDialect` 形参，驱动保留
  执行/恢复）；带句柄的 dump/restore 执行入口明确不在契约内，归 §五 收敛。
- 协议规则登记 `backup-dialect` / `legacy-backup-dialect`；drivers.md §2.8。
- 追记（`1d5252292`）：`stream_sql_file_into_session` 的 `&dyn BackupDialect`
  形参撤回——唯一调用点与其传同一 driver 直接造出的适配器，改为在函数体内
  `LegacyBackupDialect::new(driver)` 推导，恢复参数数量净增（clippy
  too_many_arguments 新警告消除）。scanner 来源仍是契约。

### 8. §六 起步：clippy 清场 + 协议审计（`1d5252292`，2026-10-21）

- 辨析方法：clippy 全量警告逐条与 `main...HEAD` 变更 hunk 求交，只修本分支
  造成的警告，不动 main 上既有的（types_eq.rs:27、resource_execution.rs:447/477、
  plan.rs 各条、get_table_data 13/7 等均核对为既有）。
- 三处修复：
  1. `capabilities.rs` 零参包装 `postgres_capability_registry()` 在别名工厂改造
     （`ca4f8af97`）后仅剩 test 调用者 → 死代码；两个测试改调
     `postgres_capability_registry_for(POSTGRES_PROVIDER_ID)`，删除包装，
     保持单一能力快照实现。
  2. `backup.rs` `stream_sql_file_into_session` 形参撤回（见上）。
  3. `query_executor.rs::build_select_sql` 因 ` dialect: &dyn SqlDialect`
     净增 1 参越过 clippy 上限 → 按仓内既有 40 处惯例 `#[allow]`+注释；
     另删 postgres `tests/restore_sql.rs` 一处测试内重复的死适配器绑定。
- 实测：postgres 13/14→13（死代码消失）、feature-core 25→23（两个本分支警告
  消失）；两 crate clippy EXIT=0。

## 门禁实测（逐字结论，历史轮）

| 命令 | 结果 |
| --- | --- |
| `cargo test -p datazen-driver-api --lib` | `363 passed; 0 failed`（旧轮）→ `366 passed`（新轮） |
| `cargo test -p datazen-driver-postgres --lib` | `187 passed; 0 failed` |
| `cargo test -p datazen-driver-mysql --lib` | `206 passed; 0 failed`（5 ignored） |
| `cargo test -p datazen-runtime --lib` | `626 passed; 0 failed` |
| `cargo test -p datazen-feature-core --lib` | `298 passed; 0 failed` |
| `cargo check -p datazen --lib` | 通过（worktree 现可编译，89 warnings 均为既有） |
| `cargo test -p datazen --lib` | `1378 passed; 9 ignored` |
| `node scripts/check-driver-protocol-compat.mjs --base main` | ok；新增契约均 additive；crate 0.0.13→0.0.17；PROTOCOL 6→6 |
| `npx vitest run scripts/__tests__/check-driver-protocol-compat.test.ts` | `68 passed`（worktree 经 node_modules 符号链接） |
| §四 收官轮（SchemaMigration + BackupDialect）逐项复核 | driver-api `371 passed`；schema-diff `148`；data-sync `179`；data-transfer `226`；feature-core `298`；host `1378`；compat gate additive（crate 0.0.13→0.0.19）；vitest pass |
| §六 起步轮（`1d5252292`） | 触及 crate 批量 clippy EXIT=0；fmt `--all --check` EXIT=0；`cargo test -p datazen-driver-postgres` 全套 ok（lib `207 passed; 5 ignored` + 各集成 ok）；`cargo test -p datazen-feature-core` `308 passed; 0 failed`；compat gate ok（additive only，crate 0.0.13→0.0.19，PROTOCOL 6→6）；协议规则 vitest `68 passed` |



未在本工作树跑过的门禁：完整 `cargo test -p datazen --lib`（worktree 缺 gitignored codegen
`src-tauri/src/driver_init.rs`，需先跑 `node scripts/resolve-drivers.mjs`）、
`pnpm typecheck`（worktree 无 node_modules）、驱动 live E2E。

## 待办（按方案顺序）

1. **§四：备份 + 结构迁移领域化（完成）**——CommandCatalog / SchemaMigration /
   BackupDialect 三个契约均已落地并迁移消费者；带句柄的备份执行入口（dump_database*、
   restore_sql*、recover_restore_statement）按方案明确留给 §五 运行时收敛。
2. **§五：运行时路径收敛（进行中）**——
   - [x] 第 1 片（`ca4f8af97`）：postgres 别名（questdb / cloudberry）接入资源契约
     ——此前 alias factory 无 provider，平台路径 `DriverRegistry::resource_provider`
     对别名一律 `ResourceProviderMissing` 失败关闭，而 legacy 句柄路径静默服务；
     `postgres_alias_factory!` 让别名注册表绑定到**别名自己身份**的
     `PostgresResourceProvider::with_provider_id`（能力快照 revision 4 / epoch 逐 //
     实例），`create()` 从同一 memoized inner 包 `ReuseDriver`（方言 wrapper 留在
     driver seam）。`tests/alias_resource_factory.rs` + 收敛后的
     `every_factory_this_crate_registers_resolves_a_provider_under_its_own_identity`
     钉住断言；随后 test(postgres) 对齐上游 `777b0d02b` 的 revision 3→4。
     mysql 系别名（mariadb/doris/starrocks/manticore/ob_oracle）已自带逐一别名的
     provider（`MysqlResourceProvider::new(driver, id, …)`），此前即在册。
     宿主回归：`cargo test -p datazen --lib` `1378 passed; 9 ignored`。
   - [ ] 第 2 片（进行中）：宿主 §6.2(5) 列出的遗留 namespace 调用点收敛；
     资源契约贯通复查：平台 `session_backend/execution.rs` 的执行链已在
     `provider.execute_on_resource` / `provider.begin_transaction` /
     `commit_transaction` / `rollback_transaction` 上全程收敛（实测确认），
     metadata 命令经 `provider.connection_handle` 限界借出。§6.2(5) 里
     `data_transfer/execute/dispatcher.rs`、`data_sync/execute.rs`、
     `workflow/executor.rs` 三处已不存在于代码中（文档引用过期），仍在册的
     分支：`commands/connection.rs` close_database / `commands/sync` executor
     discard_connection / `restore_job.rs` has_multi_database —— 待专轮设计
     runtime 侧 per-database 关闭语义（ResourceProvider 无 per-database 池
     操作；收敛点应在 runtime 层出需求，而不是宿主直接调用驱动）。
   - [x] 第 2.5 片（`e1035ed87`）：`restore_sql::prepare` 的 scanner 依赖切到
     `BackupDialect`（§四 BackupDialect 的收尾边界），宿主/测试边界包适配器。
   - [x] 第 2.6 片（`b13848354`，2026-10-21）：`restore_job.rs` 的 multi-database
     规则改读 `registry.get_capabilities` 的事实行（注册表在注册时自
     factory+driver 对取一次事实，与 connection info 同源）；测试注册的驱动无
     事实行时回退驱动实例，不会对库存驱动静默放宽。宿主
     `cargo test -p datazen --lib` `1378 passed; 9 ignored`。
     宿主 `has_multi_database()` 直调已清零（workflow `resource_execution.rs` 保留：
     driver 本就在作用域内做命令目录读取，无第二实例）。
3. **§六：协议升级与清场（进行中）**——
   - [x] clippy 清场（`1d5252292`，见 §8）：分支造成的警告清零，既有警告不动。
   - [x] 协议判定（本轮）：全部分支变更 additive（compat gate 逐条 additive，
     无 break）；crate 0.0.13→0.0.19 满足 additive bump 规则；
     `PROTOCOL_VERSION` 6→6、`MIN_PROTOCOL_VERSION` 1→1 无须动；68 条
     规则单测全绿。Git 驱动兼容状态：未迁移即 `ResourceProviderMissing`
     （带驱动名），明确失败而非静默——无需额外改动。
   - [ ] 遗留旧方法消费者逐点核验后再删（2026-10-21 初核实测）：
     `uses_sql_restore_pipeline` / `new_sql_scanner` → 仅 feature-core
     `backup.rs` 两处（均经 LegacyBackupDialect）；`type_normalizer` →
     4 处（schema-diff plan.rs、data-sync types_eq.rs、data-transfer 测试、
     host compare_commands.rs，全部经 LegacySchemaMigration）；
     `format_sql_literal` → 剩 sqlite / http-support 的契约测试与 probe
     转发，属驱动契约自检；`split_restore_sql` 在 driver-api 外零直接消费者，
     唯一链条是 BackupDialect 适配器 → `DatabaseDriver::split_restore_sql`
     默认实现 → `sql_text::split_restore_sql`（driver trait 上的方法现仅存
     转发一跳，删除需先把契约默认实现直连 sql_text，属后续独立 PR）。
     `drivers.md` / `driver-capability-migration.md` §6.2(5) 收尾改写后裁决。
   - [ ] `drivers.md` 与 `driver-capability-migration.md` §6.2(5) 收尾改写。
   - [ ] 交付时删除本台账（progress.md）。

## 工作树基建备忘

- worktree 编译宿主 crate 需要从主树复制 gitignored codegen：
  `src-tauri/src/driver_init.rs`、`src-tauri/capabilities/default.json`、
  `src-tauri/resources/builtin-ep/`（learned 2026-10-21，缺最后这个会导致 tauri
  build script 失败）。
- worktree 跑 vitest 门禁：`ln -s 主树/node_modules node_modules`（已建符号链接）。

## 待裁定 / 风险

- 转发修复后，questdb/cloudberry/doris/starrocks/manticore/ob_oracle 继承父驱动的
  `migration_renderer` 与 `qualify_sql_target`（此前被静默丢弃）。这与产品元数据一致，
  但需要一次真实别名库的 schema-diff / 跨库执行验证（本轮无 live 环境）。
- `BackupDialect` 提取范围：`dump_*`/`restore_*` 全部带 `ConnectionHandle`，属资源绑定操作；
  纯规则与运行时操作的切分点需要先定清楚，否则会把执行域提前拖进来。
