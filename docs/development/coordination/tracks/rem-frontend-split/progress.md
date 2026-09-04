# Track `rem-frontend-split` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-frontend-split，Wave 2）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

大 store/大组件拆分 + 性能 + 类型守卫 + 运行时校验。见计划 §2。Wave 2：等 rem-host-decouple 合并后启动。

## 状态

- Phase: NOT_STARTED
- 编码 commit: —
- 测试 commit: —
- 合并 commit: —

> 未启动：子代理派发机制自 Wave 1 中段起持续不可用（近 12 次派发全部失败，连最小任务都无法执行，无 closing message），playbook 的"编码→独立 Tester→修复循环"流程无法执行；协调者按用户指示停止推进 Wave 2。待派发机制恢复后启动。Wave 1 全部 8 轨已于 2026-09-08 合入集成分支（`0d9ca4d95`，后端全库 1319✓、tsc✓）。

## 心跳

- —

## 自验结果

- —

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 大表滚动/分页/编辑无回归 | 需 GUI | 留待 R 回归 |
| Workflow 页面面板切换无回归 | 需 GUI | 留待 R 回归 |
