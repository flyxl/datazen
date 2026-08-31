# DataZen v0.1.x R 阶段回归台账

日期：2026-08-31
工作目录：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/coordinator`
基线：`main`，HEAD `343a684d chore(coordination): complete page integration track`
原则：本轮只验证，不修改业务代码、配置、codegen 或 hub；仅补充本台账及对应 track 的 progress/bugs。

## 环境准备

- `node scripts/generate-builtin-locales.mjs`：通过，生成 `src/locales/builtinLocales.ts`。
- 初始依赖缺失；经用户授权后执行 `pnpm install --frozen-lockfile --ignore-scripts`，仅安装 ignored `node_modules`，未修改锁文件或运行 codegen。
- 因明确禁止改 codegen，未生成缺失的 `src/plugins/generated.ts`、`src/plugins/generated-locales.ts`、`src-tauri/src/driver_init.rs` 或 `src-tauri/capabilities/default.json`。

## 前端与 contract

- `pnpm typecheck`：未通过启动前置；`tsc` 可执行，但缺少 `src/plugins/generated.ts` / `generated-locales.ts`，并连带报告 `PluginSettingsSection.tsx` 的隐式 `any`。
- `pnpm exec vitest run`：`269 files`，`194 passed / 75 failed`；`1594 tests`，`1424 passed / 170 failed`。失败均由缺失 ignored generated driver/locale imports 主导，不能判定为业务断言失败。
- `pnpm test:unit:drivers`：`14 files`，`12 passed / 2 failed`；`76 passed`。2 个 Redis UI suite 均因缺少 `src/plugins/generated.ts` 导入失败。
- `pnpm test:unit:e2e-contract`：`3 files / 22 tests passed`。
- 可独立执行的安全/取消子集 `aiQueryActions.test.ts`、`QueryErrorPanel.test.tsx`、`queryStream.test.ts`：`3 files / 22 tests passed`。包含敏感字段/结果集过滤、QueryErrorPanel 动作、流式执行状态。

## Rust

- `cargo fmt --all -- --check`：未进入格式比较；缺少 ignored `src-tauri/src/driver_init.rs`。
- `CARGO_TARGET_DIR=/private/tmp/datazen-r-regression cargo test -p datazen --lib`：exit 101，未进入 Host 单测；编译被缺少 `driver_init` 模块及缺少 `default` capability 阻断。
- `cargo test -p datazen-driver-api`：`101 passed / 0 failed`；doc-tests `0 passed / 2 ignored`。
- `cargo test -p datazen-driver-mysql`：crate unit `72`，集成 `8`，合计 `80 passed / 0 failed`。
- `cargo test -p datazen-driver-postgres`：crate unit `86`，集成 `13`，合计 `99 passed / 0 failed`。
- `cargo test -p datazen-driver-sqlite`：unit `38`，集成 `5`，合计 `43 passed / 0 failed`。
- MySQL/MariaDB 精确 transaction/pending/stale/wrong-session/并发 execution cancel、PostgreSQL 对应 PID/cancel、ReuseDriver 精确能力闸门、SQLite fail-closed 相关测试均在上述 crate 测试中通过。MySQL 有一个既有 unused import warning，未修改。

## E2E 前置与未执行项

已尝试 `pnpm e2e:skip-build`，exit 1，未伪称通过。原因：

- `target/debug/datazen` 及 macOS app 内 webdriver binary 不存在；`dist/index.html` 也不存在。
- E2E setup 访问 PostgreSQL `127.0.0.1:5432` 返回 `Operation not permitted`。
- `TEST_PG_*`、`TEST_MYSQL_*`、`TEST_MARIADB_*` fixture 环境变量均未设置；本机没有 `mariadb` CLI。

因此真实桌面 IPC、PG/MySQL/MariaDB 事务慢查询精确取消、connection/object search、pending/filter/table actions、ResultWorkspace 真实路径仍为 R 未执行项。

## 静态回归结论

- 旧 session-wide cancel fallback：通过。Host `cancel_query_impl` 先校验 execution owner 与 `supports_query_execution_cancel`，随后只调用 `cancel_query_with_execution`；PG/MySQL 的 legacy `cancel_query(handle)` 明确返回 Unsupported；ReuseDriver 对 precise protocol 有独立能力闸门。未发现 Host 回退到旧接口的生产调用。
- PG/MySQL/MariaDB 精确取消与兼容父能力：通过静态检查及 driver-api/driver crate 测试；控制 SQL 分别为目标 PID 的 `pg_cancel_backend($1)` 和目标 thread 的 `KILL QUERY`，无 processlist/宽作用域扫描。
- Retry active connection/context guard：通过静态检查。确认返回后重新读取 panel、active connection、session、schema snapshot，并要求 map identity、`connected` 状态和非空且一致的 `dbSessionId`；随后仍经 fingerprint、SQL、参数校验后最多执行一次。
- Host 按 driver id 分支：未通过架构静态门禁。`src/windows/data-sync/DataSyncWindow.tsx` 与 `src/windows/schema-diff/useSchemaDiffEndpoints.ts` 各自按 `databaseType !== 'postgresql'` 决定 schema 加载；这是生产 Host 的 PostgreSQL 字面量分支，应登记为既有架构缺陷，详见 `v01x-page-integration/bugs.md` 的 BUG-PI-008。
- AI 诊断 raw SQL/error：UI `QueryPanel → DiagnosisPanel → aiStore` 路径通过 `buildQueryDiagnosisContext` 传递 `safeSql`/`safeErrorMessage`，但底层 `ai_diagnose_error_impl` 仍将 caller 参数原样拼入 provider prompt，并在 `generate.rs`、`aiStore.ts` 记录 raw error；直接 IPC/store 调用可绕过 UI 脱敏，静态门禁未通过，详见 BUG-PI-009。
- `git diff --check`：测试期间通过；docs 写入后将再次执行并只提交台账文件。

## 本轮需要修复但未修复的 bug

- BUG-PI-008：Host Data Sync/Schema Diff 对 PostgreSQL 的硬编码 driver-id 分支。
- BUG-PI-009：AI 诊断底层 prompt/log 信任并暴露 caller 提供的 raw SQL/error；UI 边界虽已脱敏，但不是端到端防线。
