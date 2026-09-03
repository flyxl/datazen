# Track `rem-sql-guard` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-sql-guard）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

SQL Guard 加固：NFKC 全角归一、`\0` 拒绝、注释剥离后重分类、反斜杠转义、MCP permission 同步。见计划 §2。

## 状态

- Phase: DISPATCHED
- 编码 commit: —
- 测试 commit: —
- 合并 commit: —

## 心跳

- —

## 自验结果

- —

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| Safe Mode 拦截旁路变体（全角/`\0`/注释分割） | 需 GUI + SQLite | 留待 R 回归 |
