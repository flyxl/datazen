# Track `rem-ipc-redact` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-ipc-redact）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

IPC 错误脱敏：返回路径统一 `redact_secrets_for_log`，修正 `error.rs` 明文断言测试。见计划 §2。

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
| 连接失败错误不含明文密码 | 需 GUI + 坏连接 | 留待 R 回归 |
