# Track `prh-sql-guard` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-sql-guard** 章节。

## 状态

- Phase: PASSED
- 编码 commit: ce551214ec
- 测试 commit: 6d18142bc
- Agent: tester-prh-sql-guard
- Worktree: .worktrees/datazen-prh-sql-guard

## 设计决策

- Safe Mode / 只读文档写入 `docs/architecture/security.md` §5，明确 best-effort、非形式化保证
- 后端单测扩展 `sql_guard.rs`：只读写拦截、Safe Mode 无 WHERE、DROP/TRUNCATE、多语句混合
- Safe Mode 关闭时，Query Panel 对 DROP/TRUNCATE 走前端 `dangerousSql.ts` 二次确认（复用 `splitSqlStatements`）
- 设置页 `safeModeHint` 补充尽力防护表述（en / zh-CN）

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen --lib sql_guard` | 28 passed / 0 failed | tester 独立复验 2026-09-03 |
| `npx vitest run src/lib/__tests__/dangerousSql.test.ts` | 3 passed / 0 failed | tester 独立复验 2026-09-03 |

## 测试代理复验（2026-09-03）

| 验收项 | 结果 | 证据 |
|--------|------|------|
| 文档「尽力防护 / 非形式化保证」 | PASS | `docs/architecture/security.md` §5.1–5.4 |
| 单测：只读拦截写 | PASS | `read_only_blocks_*`, `mixed_statements_read_only_*` |
| 单测：Safe Mode 无 WHERE UPDATE/DELETE | PASS | `safe_mode_blocks_update_without_where`, `safe_mode_blocks_delete_without_where`, `mixed_statements_safe_mode_blocks_update_without_where` |
| 单测：DROP/TRUNCATE | PASS | `safe_mode_blocks_drop`, `safe_mode_blocks_truncate`, `mixed_statements_safe_mode_blocks_drop_and_truncate` |
| 单测：多语句混合 | PASS | `mixed_statements_*` 系列 4 项 |
| 高危操作 GUI 确认（QueryPanel） | PASS | `requestExecute` / `handleConfirmUnclosedTx` + `confirmDangerousDialog` |
| 无完整 AST 重写 | PASS | `sql_guard.rs` 启发式关键字 + `dangerousSql.ts` 正则 |
| i18n en/zh-CN 键 | PASS | `query.dangerousSql*`, `settings.safeModeHint` |
| i18n 其他语言 | 留待发布前 | `dangerousSql*` 缺 8 语言（符合开发期只改 en/zh-CN 约定） |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| E1 | Safe Mode 关、已连接 DB | Query Panel 执行 `DROP TABLE t` | 弹出破坏性 SQL 确认；取消不执行 | 【留待 R 回归】 |
| E2 | Safe Mode 开 | 执行 `UPDATE t SET x=1`（无 WHERE） | 后端拦截 | 【留待 R 回归】 |

## 遗留

—
