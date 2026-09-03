# Track `rem-scheduler` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-scheduler）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

WorkflowScheduler `in_flight` panic 泄漏修复。见计划 §2。

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
| 定时 workflow 失败后仍可再次触发 | 需 scheduler + 时间推进 | 留待 R 回归 |
