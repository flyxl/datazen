# Track `rem-scheduler` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-scheduler）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

WorkflowScheduler `in_flight` panic 泄漏修复。见计划 §2。

## 状态

- Phase: PASSED
- 编码 commit: 88000b879
- 测试 commit: (coordinator-verified; no subagent tester available)
- 合并 commit: —

## 心跳

- 2026-09-04 BOOTSTRAP: pwd confirmed, all docs read, scheduler.rs analyzed
- 2026-09-04 CODING: InFlightGuard + catch_unwind implemented
- 2026-09-04 SELF-VERIFY: cargo test -p datazen --lib workflow — 71 passed, 0 failed

## 自验结果

- `cargo test -p datazen --lib workflow` → **71 passed, 0 failed**

## 协调者独立验证（2026-09-08）

> 注：子代理派发机制临时不可用（测试子代理全部早期崩溃），由协调者执行独立复验。

- `cargo test -p datazen --lib workflow::scheduler` → **7 passed, 0 failed**（含 3 个新单测 + 4 个既有）
- `cargo test -p datazen --lib`（全库）→ **1301 passed, 0 failed, 3 ignored**
- 修复正确性：InFlightGuard Drop 用 try_write；catch_unwind 包裹 spawn body；Arm/Skip/Fire 语义未改动 ✓
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
