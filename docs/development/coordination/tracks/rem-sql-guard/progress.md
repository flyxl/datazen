# Track `rem-sql-guard` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-sql-guard）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

SQL Guard 加固：NFKC 全角归一、`\0` 拒绝、注释剥离后重分类、反斜杠转义、MCP permission 同步。见计划 §2。

## 状态

- Phase: PASSED
- 编码 commit: 42c08e740
- 测试 commit: (coordinator-verified; no subagent tester available)
- 合并 commit: —

## 心跳

- 2026-09-08 CODING: NFKC 全角归一 + `\0` 拒绝 + 注释剥离后重分类 + 保守判定注释藏写动词
- 2026-09-08 FIX: `strip_sql_comments` 注释移除处补分隔符（防 `DROP/**/TABLE` → `DROPTABLE` 粘连）

## 自验结果

- `cargo test -p datazen --lib sql_guard` → **43 passed, 0 failed**
- `cargo test -p datazen --lib mcp::permission` → **37 passed, 0 failed**
- `cargo test -p datazen --lib`（全库）→ **1312 passed, 0 failed, 3 ignored**
- 覆盖：`ＤＲＯＰ`/`DROP\0TABLE`/`DROP/**/TABLE`/`/* DROP */ TABLE x` 在 safe_mode+read_only 下全部拦截；`/* DROP TABLE t */ SELECT 1` 正确放行

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| Safe Mode 拦截旁路变体（全角/`\0`/注释分割） | 需 GUI + SQLite | 留待 R 回归 |
