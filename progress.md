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



未在本工作树跑过的门禁：完整 `cargo test -p datazen --lib`（worktree 缺 gitignored codegen
`src-tauri/src/driver_init.rs`，需先跑 `node scripts/resolve-drivers.mjs`）、
`pnpm typecheck`（worktree 无 node_modules）、驱动 live E2E。

## 待办（按方案顺序）

1. **§四：备份 + 结构迁移领域化（完成）**——CommandCatalog / SchemaMigration /
   BackupDialect 三个契约均已落地并迁移消费者；带句柄的备份执行入口（dump_database*、
   restore_sql*、recover_restore_statement）按方案明确留给 §五 运行时收敛。
2. **§五：运行时路径收敛（未开始）**——执行/事务/流式/取消按调用链迁到既有
   `ResourceProvider`；先贯通一个驱动；`ReuseDriver` 与 Git 驱动纳入验收。
   起点 参考：`packages/driver-api/src/restore_sql.rs`（retained-restore 契约
   `prepare` 目前只有驱动测试调用，可顺势改为 `BackupDialect` 扫描器）。
3. **§六：协议升级与清场（未开始）**——删除无消费者的旧方法/适配器；更新
   `drivers.md`、`driver-capability-migration.md`；Git 驱动兼容状态说明。

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
