# Track `rem-sync-taxonomy` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-sync-taxonomy，Wave 2）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

sync category/family 下沉 driver-api trait。见计划 §2。Wave 2：等 Wave 1 合并后启动。

## 状态

- Phase: PASSED
- 编码 commit: d0e9f94d7
- 测试 commit: ad12351fd
- 合并 commit: —

## 心跳

- 2026-09-04T21:28+08:00 CODER 完成实现与自验，待独立 Tester 复测
- 2026-09-04T21:40+08:00 TESTER 独立复验通过，补齐 taxonomy 测试，Phase → PASSED

## 自验结果（编码代理）

| 套件 | 结果 |
|------|------|
| `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen-driver-api --lib` | 113 passed |
| `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen --lib` | 1319 passed |
| `npx tsc --noEmit` | OK |
| `npx vitest run src/lib/__tests__/syncPairing.test.ts` | 9 passed |

## 独立复验结果（测试代理）

| 套件 | 编码自报 | 独立实测 | 差异 |
|------|---------|---------|------|
| `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen-driver-api --lib` | 113 | **116** | +3（tester 新增 sync_taxonomy 用例） |
| `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen --lib` | 1319 | **1320** | +1（tester 新增 pairing alias 用例） |
| `npx tsc --noEmit` | OK | OK | — |
| `npx vitest run src/lib/__tests__/syncPairing.test.ts src/lib/__tests__/syncTaxonomy.test.ts` | 9 | **15** | +6（tester 新增 taxonomy / hook 用例） |

## 覆盖率（改动核心模块）

| 模块 | 行覆盖率 | 说明 |
|------|---------|------|
| `src/lib/syncPairing.ts` + `syncTaxonomy.ts` | **96.55%** lines / **94.11%** stmts | vitest `--coverage.include` 聚焦 |
| `packages/driver-api/src/sync_taxonomy.rs` | **~100%**（逻辑审查 + 7 单测） | 含 alias / unregistered fallback |
| `src-tauri/src/transfer/pairing.rs` | **~100%**（13 单测） | 含 alias tidb/oceanbase |
| `packages/driver-api/src/reuse.rs` sync 透传 | 已有 `reuse_driver_forwards_sync_taxonomy` | 无需新增 |

## 改动摘要

- `packages/driver-api`: 新增 `SyncCategory`、`sync_category()`/`sync_family()` trait 方法、`sync_taxonomy` 查找模块；`ReuseDriver` 转发
- 驱动覆写：`mysql`/`postgres`/`sqlite`/`rqlite`/`turso` 的 `sync_family`
- `src-tauri/src/transfer/pairing.rs`: 删除 match 硬编码，改调 `sync_category_of`/`sync_family_of`
- 前端：`databaseMeta.ts` 新增 `syncCategory`/`syncFamily`；`syncPairing.ts` 只读 registry meta；驱动 meta 补 `syncFamily`（generic dialect 类型）

## Tester 新增测试

- `packages/driver-api/src/sync_taxonomy.rs`: `test_tester_unregistered_sync_categories`, `test_tester_oceanbase_alias_normalizes_to_mysql_id`, `test_tester_unknown_id_passthrough`
- `src-tauri/src/transfer/pairing.rs`: `test_tester_alias_types_use_normalized_family`
- `src/lib/__tests__/syncTaxonomy.test.ts`: `[tester] syncTaxonomy` alias 映射
- `src/lib/__tests__/syncPairing.test.ts`: `[tester] syncPairing taxonomy helpers`, `[tester] useSyncPairingState`

## 代码审查摘要

- trait 默认实现与 `driver_category` 对齐；kiwi/superset 显式 `Other` ✓
- `ReuseDriver` 正确透传 `sync_category`/`sync_family` ✓
- 后端 `pairing.rs` 完全委托 driver-api，无残留硬编码 match ✓
- 前端 `syncCategory`/`normalizeSyncFamily` 读 registry + alias fallback，Data Sync 配对仍走 IPC ✓
- 无生产路径 unwrap/expect 引入；无 Bug 登记

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 数据同步配对无回归 | 需双库 | 留待 R 回归 |
