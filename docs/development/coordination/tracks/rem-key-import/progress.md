# Track `rem-key-import` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-key-import）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

备份导入 `.key` 覆盖防护：警告 + 拒绝覆盖（legacy 改 opt-in）。见计划 §2。

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
| 导入含 `.key` 的备份被拒绝/警告 | 需 GUI + 备份 zip | 留待 R 回归 |
