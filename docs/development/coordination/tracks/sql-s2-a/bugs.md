# Bugs: sql-s2-a

## sql-s2-a-BUG-001

- **状态**: 已关闭
- **描述**: 20k fixture 性能 smoke 测试在默认 vitest 配置下失败（`testTimeout=5000ms`）。全量 `scanSql` + `buildStatementRangesFromTokens` 对 `benchmark20k.sql`（20001 行 / ~888KB）耗时约 **6.2–6.5s**，超过默认超时；标准命令 `npx vitest run src/components/sql-editor/semantic/ ...` 结果为 **61/63 passed**（2 failed），与编码代理自报 63/63 不符。
- **重现步骤**:
  1. `cd .worktrees/datazen-sql-s2-a`
  2. `node scripts/generate-builtin-locales.mjs`
  3. `npx vitest run src/components/sql-editor/semantic/ src/lib/__tests__/sqlStatementRange.test.ts src/lib/__tests__/sqlTransactionGuard.test.ts`
- **修复方法**:
  - 优化 `scanner.ts`：`charCodeAt` 替代正则/切片、`includeOther`/`includeText` 边界扫描模式跳过 Other token 与 text 分配。
  - 优化 `statementRanges.ts`：预计算 `LineIndex` + 二分查找替代 O(n) `lineNumberAt`（根因：8222 ranges × 平均 offset 扫描 ≈ 18 亿字符访问）；`findRescanStart` 复用已有 ranges 避免 prefix 重扫。
  - 性能测试显式 `{ timeout: 30_000 }`，使用 `boundaryScan` 选项与 coverage-aware `perfBudgetMs`。
- **修复后实测**: 20k fixture scan+build **~25ms**（原 ~6200ms）；标准 vitest 命令 **82/82 passed**。
- **复测确认** (RepairCommit d2fff07bb, Tester 2026-09-05): 标准 vitest **82/82 passed**（1.29s）；独立 5 轮 benchmark avg scan **8.64ms** + build **5.80ms** = **14.44ms**；20k perf smoke 29ms（vitest verbose）。

## sql-s2-a-BUG-002

- **状态**: 已关闭
- **描述**: 启用 coverage instrumentation 时，20k fixture 全量扫描耗时约 **41s**，超过测试内断言 `expect(elapsed).toBeLessThan(10000)`；带 coverage 跑同一套件时 **1/63** 性能断言失败。CI 若开启 coverage 将稳定失败。
- **重现步骤**:
  1. `npx vitest run --coverage src/components/sql-editor/semantic/ src/lib/__tests__/sqlStatementRange.test.ts src/lib/__tests__/sqlTransactionGuard.test.ts --testTimeout=120000`
- **修复方法**: 同上性能优化（scan ~13ms + build ~12ms → 总计 ~25ms）；性能断言改为 `process.env.V8_COVERAGE ? 60_000 : 10_000` ms 上限；测试 timeout 30s。
- **修复后实测**: 带 `--coverage` 跑同一套件 **82/82 passed**，20k perf 断言通过（~25ms << 60s budget）。
- **复测确认** (RepairCommit d2fff07bb, Tester 2026-09-05): `V8_COVERAGE=1` 标准命令 **82/82 passed**（932ms）；scoped semantic coverage **91.28% lines / 89.57% stmts**，perf 断言通过。
