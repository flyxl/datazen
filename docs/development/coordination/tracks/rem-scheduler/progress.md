# Track `rem-scheduler` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-scheduler）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

WorkflowScheduler `in_flight` panic 泄漏修复。见计划 §2。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 88000b879
- 测试 commit: —
- 合并 commit: —

## 心跳

- 2026-09-04 BOOTSTRAP: pwd confirmed, all docs read, scheduler.rs analyzed
- 2026-09-04 CODING: InFlightGuard + catch_unwind implemented
- 2026-09-04 SELF-VERIFY: cargo test -p datazen --lib workflow — 71 passed, 0 failed

## 自验结果

- `cargo test -p datazen --lib workflow` → **71 passed, 0 failed**
- 新增 3 个单测全部通过：
  - `in_flight_guard_removes_on_normal_drop` — guard drop 清理 in_flight ✓
  - `in_flight_cleanup_after_panic` — panic 后 in_flight 被清理、workflow 可再次触发 ✓
  - `in_flight_retriggerable_after_normal_completion` — 正常完成后可再次触发 ✓
- 既有 4 个 scheduler 测试（disabled_or_missing / clamps / never_run_arms / clamps_interval）全绿 ✓
- 调度语义（Arm/Skip/Fire）未改动 ✓

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 定时 workflow 失败后仍可再次触发 | 需 scheduler + 时间推进 | 留待 R 回归 |
