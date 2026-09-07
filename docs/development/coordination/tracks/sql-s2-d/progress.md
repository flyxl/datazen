# Track Progress: sql-s2-d

- Task: 风险 classifier 与 Host guard 回归
- Plan: docs/todo/sql-editor/implementation-plan.md
- BaseCommit: 197b2856b
- Worktree: .worktrees/datazen-sql-s2-d
- Branch: feature/sql-s2-d
- Phase: PASSED (Round 3 — BUG-001 + BUG-002 fixes verified by fresh Tester)
- Agent: coder-sql-s2-d
- CodingCommit: 5cce0a7b2
- FixCommit1: f2e2a2efc (BUG-001: block/line comment WHERE)
- FixCommit2: 229b6778f (BUG-002: # comment + dollar-quote WHERE)
- TestCommit: f305a622d
- MergeCommit: —
- LastHeartbeat: 2026-09-05T12:30:00+08:00
- UnresolvedBugs: 0

## 详细进展

- 2026-09-05 11:45 BOOTSTRAP → 12:10 CODING → 12:20 READY_FOR_TEST (feat 5cce0a7b2)
- 2026-09-05 12:25 TEST_FAILED Round 1: BUG-001 (block/line comment WHERE bypass)
- 2026-09-05 ~12:00 FIX Round 2: has_top_level_where(&stripped) — f2e2a2efc
- 2026-09-05 ~12:15 TEST_FAILED Round 2: BUG-002 (# comment + dollar-quote WHERE bypass)
- 2026-09-05 12:09 FIX Round 3: strip_sql_comments 加 # + dollar-quote 保留 + has_top_level_where 加 skip_dollar_quoted — 229b6778f
- 2026-09-05 12:30 TEST_DONE Round 3（fresh Tester）：BUG-001 + BUG-002 全部复测通过，新增 6 个回归测试 + 3 个 fixture 边界用例，覆盖率 ≥90%，tsc 0 errors

## 自验结果（Round 3 修复后 · 独立 Tester 实测）

| 套件 | 结果 |
|------|------|
| `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen --lib sql_guard` | 66 passed, 0 failed |
| `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen --lib`（全量） | 1354 passed, 0 failed, 3 ignored |
| `npx vitest run src/lib/__tests__/dangerousSql.test.ts src/windows/connection/query/__tests__/queryExecutionRisk.test.ts` | 138 passed, 0 failed |
| `npx vitest run src/windows/connection/__tests__/QueryPanel.dangerousSql.test.tsx src/lib/__tests__/sqlTransactionGuard.test.ts` | 28 passed, 0 failed |
| risk/guard 相关 vitest 合计 | 166 passed, 0 failed |
| `npx vitest run`（Host 全量） | 324 files / 2881 tests passed；3 files / 2 tests failed（均为既有 docs-consistency，见备注） |
| `npx tsc --noEmit` | 0 errors |

### 覆盖率（risk 模块 · 独立实测 ≥90%）

| 模块 | Stmts | Branch | Funcs | Lines | 阈值 | 结果 |
|------|-------|--------|-------|-------|------|------|
| `src/lib/dangerousSql.ts` | 95.96% | 91.50% | 100% | 99.01% | ≥90% | ✓ |
| `src/windows/connection/query/queryExecutionRisk.ts` | 97.14% | 98.14% | 100% | 100% | ≥90% | ✓ |

### 共享 fixture 无漂移

- `sqlRiskCases.json` 现共 **34 cases**。
  - Coder Round 3 新增 2 个（→31）：`update-hash-comment-where-not-real`、`update-dollar-quote-where-not-real`
  - Tester Round 3 复验新增 3 个（→34）：`delete-hash-comment-where-not-real`、`update-dollar-quote-named-tag-where-not-real`、`update-hash-inside-string-where-real`（后 2 者为正例/边界控制）
- 4 个消费者（Rust `test_tester_shared_risk_fixture_no_drift` + TS `classifyRisk` / `assessExecutionRisk` / `hostGuardWouldBlock` 三组 `it.each`）对全部 34 cases 一致，frontend/Host 零漂移。

## 改动摘要

### Round 3 新增（BUG-002 修复）
- `src-tauri/src/sql_guard/scanner.rs`: 新增 `pub(crate) skip_dollar_quoted` 函数，跳过 PG `$tag$…$tag$` 字符串
- `src-tauri/src/sql_guard/safety.rs`:
  - `strip_sql_comments`: 新增 `#` 行注释剥离 + dollar-quote 保留
  - `has_top_level_where`: 在 `skip_quoted` 后新增 `skip_dollar_quoted` 调用
- `src/lib/__tests__/fixtures/sqlRiskCases.json`: 新增 2 个 fixture 用例（`#` 注释 WHERE / dollar-quote WHERE），共 31 cases
- `src-tauri/src/sql_guard/tests/safety_regression.rs`: 新增 2 个测试

### Tester Round 3 复验新增（fresh Tester）
- `src-tauri/src/sql_guard/tests/safety_regression.rs`: 新增 6 个 `test_tester_` 回归测试（DELETE+`#`、命名 tag dollar-quote、DELETE+dollar-quote、dollar-quote 值正例、字符串内 `#`、字符串内 `$q$`）
- `src/lib/__tests__/fixtures/sqlRiskCases.json`: 新增 3 个边界用例（`delete-hash-comment-where-not-real`、`update-dollar-quote-named-tag-where-not-real`、`update-hash-inside-string-where-real`），共 **34 cases**

### Round 2 修复（BUG-001）
- `src-tauri/src/sql_guard/safety.rs`: `check_sql` line 164 改用 `has_top_level_where(&stripped)`

### 原始编码（5cce0a7b2）
- 前端 risk classifier、queryExecutionRisk.ts facade、共享 fixture、Rust guard 测试

## 备注

- BUG-001 根因：`check_sql` 已有 `strip_sql_comments(&stmt)` 但 `has_top_level_where` 用原始 `stmt`
- BUG-002 根因：`strip_sql_comments` 不处理 `#`/dollar-quote；`has_top_level_where` 不跳 dollar-quote
- binder→guard 顺序不变
- Tester 代码审查未发现新引入缺陷；`comment_hides_write_verb` 等 safety 函数未被本次改动影响（仍对原始 `&stmt` 检测 `/* */` 内写动词）
- 全量 vitest 的 3 个失败文件均为既有 docs-consistency（`scripts/__tests__/check-ci-docs-consistency.test.ts` 的 windowManager.ts 单例子窗 `data-sync`/`schema-diff`/`data-transfer` 文档不一致，及同族收集/一致性脚本）。经确认 `windowManager.ts`、`check-ci-docs-consistency.*` 均未被 sql-s2-d 任何 commit 触碰，与本次改动无关。
- Tester 审查观察（非缺陷）：`scanner.rs::skip_dollar_quoted` 与既有私有 `dollar_quote_end` 逻辑完全重复，可合并；`#` 一律视为行注释符合 MySQL/前端 scanner（前端第 134 行 `#`→LineComment），但 PostgreSQL 中 `#` 亦为 bitwise-XOR 运算符，属既有前端行为、后端为对齐而同步，未改变安全判定。
