# Track `rem-ddl-atomicity` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-ddl-atomicity，Wave 2）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

`ddl_atomicity()` trait 化。见计划 §2。Wave 2：等 Wave 1 合并后启动。

## 状态

- Phase: PASSED
- 编码 commit: 2c571534f
- 测试 commit: f85c5b1cc
- 合并 commit: —

## 心跳

- 2026-09-04 BOOTSTRAP：worktree 确认，分支 `feature/rem-ddl-atomicity`
- 2026-09-04 CODING 完成：`DdlAtomicity` + trait 方法 + PG/SQLite/MySQL 覆写 + host 去硬编码
- 2026-09-04 TEST 完成：独立复验通过，新增 8 项 tester 测试，核心改动覆盖率 ≥ 80%

## 自验结果（编码代理）

- `cargo test -p datazen-driver-api --lib`：109 passed
- `cargo test -p datazen --lib`：1319 passed

## 独立复验结果（测试代理）

- `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen-driver-api --lib`：**111 passed**（+2 tester）
- `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen --lib`：**1322 passed**（+3 tester）
- `cargo test -p datazen-driver-postgres --lib test_tester_ddl_atomicity`：1 passed
- `cargo test -p datazen-driver-mysql --lib test_tester_ddl_atomicity`：1 passed
- `cargo test -p datazen-driver-sqlite --lib test_tester_ddl_atomicity`：1 passed

## 覆盖率（改动模块，逻辑审查 + 新增测试）

| 模块 | 覆盖路径 | 估计覆盖率 |
|------|----------|-----------|
| `driver-api/types.rs` `DdlAtomicity` | serde 三 variant roundtrip | ~100% |
| `driver-api/traits.rs` 默认 `Unknown` | `default_ddl_atomicity_is_unknown` | 100% |
| `driver-api/reuse.rs` 转发 | `test_tester_reuse_driver_forwards_ddl_atomicity` | 100% |
| `drivers/postgres\|sqlite\|mysql` 覆写 | 各 `test_tester_ddl_atomicity_*` | 100% |
| `services/transaction.rs` | Transactional / AutoCommit / Unknown 三分支 | ~95% |
| `schema_diff/deploy.rs` | 既有 `run_deploy_with_executor` 四用例 | ~90% |

## 新增测试清单（tester）

1. `types::tests::test_tester_ddl_atomicity_serde_roundtrip` — DdlAtomicity serde 契约
2. `reuse::tests::test_tester_reuse_driver_forwards_ddl_atomicity` — ReuseDriver 转发
3. `services::transaction::tests::test_tester_transactional_scope_begins_and_commits`
4. `services::transaction::tests::test_tester_auto_commit_scope_skips_begin`
5. `services::transaction::tests::test_tester_unknown_atomicity_skips_begin`
6. `postgres::tests::test_tester_ddl_atomicity_is_transactional`
7. `mysql::tests::test_tester_ddl_atomicity_is_auto_commit`
8. `sqlite::tests::test_tester_ddl_atomicity_is_transactional`

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 各驱动 DDL 事务性无回归 | 需各库 | 留待 R 回归 |
