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
- 测试 commit: b0ad5c210
- Agent: tester-prh-sql-guard
- Worktree: `.worktrees/datazen-test-prh-sql-guard`（分支 `feature/test-prh-sql-guard`）

## 设计决策

- Safe Mode / 只读文档写入 `docs/architecture/security.md` §5，明确 best-effort、非形式化保证
- 后端单测扩展 `sql_guard.rs`：只读写拦截、Safe Mode 无 WHERE、DROP/TRUNCATE、多语句混合
- Safe Mode 关闭时，Query Panel 对 DROP/TRUNCATE 走前端 `dangerousSql.ts` 二次确认（复用 `splitSqlStatements`）
- 设置页 `safeModeHint` 补充尽力防护表述（en / zh-CN）

## 自验结果

| 套件 | 编码代理自报 | Tester 独立实测 |
|------|-------------|----------------|
| `cargo test -p datazen --lib sql_guard` | 28 passed | **37 passed** / 0 failed |
| `npx vitest run src/lib/__tests__/dangerousSql.test.ts` | 3 passed | **10 passed** / 0 failed |
| `npx vitest run src/windows/connection/__tests__/QueryPanel.dangerousSql.test.tsx` | — | **7 passed** / 0 failed |

## 测试代理复验（2026-09-03）

| 验收项 | 结果 | 证据 |
|--------|------|------|
| 文档「尽力防护 / 非形式化保证」 | PASS | `docs/architecture/security.md` §5.1–5.4 |
| 单测：只读拦截写 | PASS | `read_only_blocks_*`, `mixed_statements_read_only_*` |
| 单测：Safe Mode 无 WHERE UPDATE/DELETE | PASS | `safe_mode_blocks_update_without_where`, `mixed_statements_safe_mode_blocks_update_without_where` |
| 单测：DROP/TRUNCATE | PASS | `safe_mode_blocks_drop`, `safe_mode_blocks_truncate`, `mixed_statements_safe_mode_blocks_drop_and_truncate` |
| 单测：多语句混合 | PASS | `mixed_statements_*` 系列 4 项 |
| 高危操作 GUI 确认（QueryPanel） | PASS | `QueryPanel.dangerousSql.test.tsx` 取消/确认/unclosed-tx 路径 |
| 无完整 AST 重写 | PASS | `sql_guard.rs` 启发式关键字 + `dangerousSql.ts` 正则 |
| i18n en/zh-CN 键 | PASS | `query.dangerousSql*`, `settings.safeModeHint` |
| i18n 其他语言 | 留待发布前 | `dangerousSql*` 缺 8 语言（符合开发期只改 en/zh-CN 约定） |

## Tester 新增测试

### Rust（`sql_guard.rs`，前缀 `test_tester_`）

- 内联块注释 `DROP/**/TABLE` — 启发式缺口（动词变为 TABLE）
- 嵌套块注释内 DROP — 仍拦截
- 仅注释含 DROP — 放行
- 全角 Unicode DROP — 绕过（文档化缺口）
- NULL 字节拆分 DROP — 绕过
- 控制字符 SELECT — 不崩溃
- 大小写/空白变体 DROP/TRUNCATE — 拦截
- `/* DROP */ TABLE` — 绕过（文档化缺口）

### 前端（`[tester]` 描述块）

- `dangerousSql.test.ts`：空/空白、超长 SQL、注释后 DROP、WITH…DROP、前后端 inline-comment 差异注释
- `QueryPanel.dangerousSql.test.tsx`：Safe Mode 关时确认弹窗、取消/确认路径、Safe Mode 开跳过确认、unclosed-tx 二次路径

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| E1 | Safe Mode 关、已连接 DB | Query Panel 执行 `DROP TABLE t` | 弹出破坏性 SQL 确认；取消不执行 | 【留待 R 回归】 |
| E2 | Safe Mode 开 | 执行 `UPDATE t SET x=1`（无 WHERE） | 后端拦截 | 【留待 R 回归】 |

## 遗留

—
