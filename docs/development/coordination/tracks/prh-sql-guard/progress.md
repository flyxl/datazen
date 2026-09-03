# Track `prh-sql-guard` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-sql-guard** 章节。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 4e3cdfe57
- 测试 commit: —
- Agent: rescuer-prh-sql-guard
- Worktree: .worktrees/datazen-prh-sql-guard

## 设计决策

- Safe Mode / 只读文档写入 `docs/architecture/security.md` §5，明确 best-effort、非形式化保证
- 后端单测扩展 `sql_guard.rs`：只读写拦截、Safe Mode 无 WHERE、DROP/TRUNCATE、多语句混合
- Safe Mode 关闭时，Query Panel 对 DROP/TRUNCATE 走前端 `dangerousSql.ts` 二次确认（复用 `splitSqlStatements`）
- 设置页 `safeModeHint` 补充尽力防护表述（en / zh-CN）

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| `cargo test -p datazen --lib sql_guard` | 28 passed | rescuer 自验 |
| `npx vitest run src/lib/__tests__/dangerousSql.test.ts` | 3 passed | rescuer 自验 |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| E1 | Safe Mode 关、已连接 DB | Query Panel 执行 `DROP TABLE t` | 弹出破坏性 SQL 确认；取消不执行 | 【留待 R 回归】 |
| E2 | Safe Mode 开 | 执行 `UPDATE t SET x=1`（无 WHERE） | 后端拦截 | 【留待 R 回归】 |

## 遗留

—
