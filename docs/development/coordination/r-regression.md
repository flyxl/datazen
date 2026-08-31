# DataZen v0.1.x R 阶段回归台账

日期：2026-08-31
工作目录：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/coordinator`
基线：`main`，当前工作树 HEAD 以 `git rev-parse HEAD` 为准
原则：本轮验证 v0.1.x 本次改动；必要的 E2E fixture/等待逻辑与 `src-tauri/src/commands/data.rs` 产品修复均保留在当前实现中。

## 环境准备

- `node scripts/generate-builtin-locales.mjs`：通过，生成 `src/locales/builtinLocales.ts`。
- 初始依赖缺失；经用户授权后执行 `pnpm install --frozen-lockfile --ignore-scripts`，仅安装 ignored `node_modules`，未修改锁文件或运行 codegen。
- E2E 构建使用 `node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs --drivers=basic -- node scripts/e2e-tauri-build.mjs`，Tauri WebDriver app binary 构建通过。
- `src/locales/builtinLocales.ts` 已按仓库流程生成；其他 ignored driver codegen 由构建流程生成，未加入提交。

## 前端与 contract

- `pnpm typecheck`：通过。
- `pnpm exec vitest run`：`269 files / 2240 tests` 全部通过（此前已完成）。
- `pnpm test:unit:drivers`：`14 files / 84 tests` 全部通过（此前已完成）。
- `pnpm test:unit:e2e-contract`：`3 files / 22 tests passed`。
- 可独立执行的安全/取消子集 `aiQueryActions.test.ts`、`QueryErrorPanel.test.tsx`、`queryStream.test.ts`：`3 files / 22 tests passed`。包含敏感字段/结果集过滤、QueryErrorPanel 动作、流式执行状态。

## Rust

- `cargo test -p datazen --lib commands::data`：`19 passed / 0 failed`，包含本次 schema-context 修复的正反例。
- Host 全量 `cargo test -p datazen --lib`：`1198 passed / 0 failed / 2 ignored`（此前已完成）；本轮产品改动后再次执行的定向 data tests 全绿。
- `cargo test -p datazen-driver-api`：`101 passed / 0 failed`；doc-tests `0 passed / 2 ignored`。
- `cargo test -p datazen-driver-mysql`：crate unit `72`，集成 `8`，合计 `80 passed / 0 failed`。
- `cargo test -p datazen-driver-postgres`：crate unit `86`，集成 `13`，合计 `99 passed / 0 failed`。
- `cargo test -p datazen-driver-sqlite`：unit `38`，集成 `5`，合计 `43 passed / 0 failed`。
- MySQL/MariaDB 精确 transaction/pending/stale/wrong-session/并发 execution cancel、PostgreSQL 对应 PID/cancel、ReuseDriver 精确能力闸门、SQLite fail-closed 相关测试均在上述 crate 测试中通过。MySQL 有一个既有 unused import warning，未修改。

## E2E 实测结果（WDIO / Tauri WebDriver）

所有本次相关桌面路径均使用 WDIO，经 Tauri WebDriver plugin（4445）执行，没有使用 computer-use：

- `connection-window.ts`：`33 passed`。
- `sql-query.ts`：`26 passed`，包含长查询取消、错误动作、上下文选择器和历史。
- `table-filter.ts`：`12 passed`，包含 quick filter、空草稿/Apply、AND/OR、清空恢复。
- `table-edit.ts`：`7 passed`，包含暂存、Preview SQL、Commit、Rollback 和持久化。
- `table-indexes.ts`：`5 passed`；冷启动首两轮曾在 schema refresh race 超时，第三轮通过，未发现稳定业务失败。
- `table-structure.ts`：`15 passed`。
- `chart-views.ts`：`5 passed`；`chart-expand.ts`：`7 passed`。
- Host contract matrix：PostgreSQL `10 passed`、MySQL `10 passed`、SQLite `9 passed / 1 skipped`（SQLite 不支持对象 journey）。

上述本次相关独立 runner 合计 `139 passed / 1 skipped`。

另行尝试完整 `pnpm e2e:db` 与 `pnpm e2e:core`：前置 DB specs（connection-window、sql-query、table-data、table-filter、table-indexes、table-edit、table-structure、export-import、object-browser）通过；随后既有 `mysql.ts`、`data-sync-real.ts`、`client-parity.ts` 等失败导致同一 app 进程连接池/事务状态级联，后续 transfer/部分 core specs 出现超时，因此停止该全量 runner，不能标记为全量通过。该问题不纳入本次功能验收，需另立回归修复轮。

此前已完成的 AI/dashboard 组结果：AI `19 passed / 14 skipped`（跳过项因未设置 `E2E_AI_API_KEY`），Dashboard `13 passed`。

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
