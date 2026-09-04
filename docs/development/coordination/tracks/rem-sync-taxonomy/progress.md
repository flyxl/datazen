# Track `rem-sync-taxonomy` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-sync-taxonomy，Wave 2）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

sync category/family 下沉 driver-api trait。见计划 §2。Wave 2：等 Wave 1 合并后启动。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 0beeae257
- 测试 commit: —
- 合并 commit: —

## 心跳

- 2026-09-04T21:28+08:00 CODER 完成实现与自验，待独立 Tester 复测

## 自验结果

| 套件 | 结果 |
|------|------|
| `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen-driver-api --lib` | 113 passed |
| `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen --lib` | 1319 passed |
| `npx tsc --noEmit` | OK |
| `npx vitest run src/lib/__tests__/syncPairing.test.ts` | 9 passed |

## 改动摘要

- `packages/driver-api`: 新增 `SyncCategory`、`sync_category()`/`sync_family()` trait 方法、`sync_taxonomy` 查找模块；`ReuseDriver` 转发
- 驱动覆写：`mysql`/`postgres`/`sqlite`/`rqlite`/`turso` 的 `sync_family`
- `src-tauri/src/transfer/pairing.rs`: 删除 match 硬编码，改调 `sync_category_of`/`sync_family_of`
- 前端：`databaseMeta.ts` 新增 `syncCategory`/`syncFamily`；`syncPairing.ts` 只读 registry meta；驱动 meta 补 `syncFamily`（generic dialect 类型）

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 数据同步配对无回归 | 需双库 | 留待 R 回归 |
