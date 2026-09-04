# Track `rem-panic-locks` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-panic-locks）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

锁毒化与 block_on 治理 + 生产路径 unwrap 清理。见计划 §2。

## 状态

- Phase: PASSED
- 编码 commit: (coordinator-verified; subagent dispatch unavailable)
- 测试 commit: (coordinator-verified; no subagent tester available)
- 合并 commit: —

## 心跳

- 2026-09-08 CODING: extensions RwLock 锁毒化恢复（unwrap_or_else+into_inner）+ monitor block_on→spawn + ssh mutex + deploy/store/connection unwrap 清理
- 2026-09-08 FIX(baseline): lib.rs 重导出 `is_mcp_stdio_mode` 为 pub（修复 bin 编译）；配套更新 bootstrap.rs 测试断言

## 自验结果

- `cargo test -p datazen --lib` → **1298 passed, 0 failed, 3 ignored**
- 审查：无生产路径新增裸 unwrap/expect；锁毒化均用 unwrap_or_else+into_inner 恢复 + 日志
- 关键覆盖：store:: 76 passed, 0 failed

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 托盘/监控常驻路径无回归 | 需 GUI | 留待 R 回归 |
