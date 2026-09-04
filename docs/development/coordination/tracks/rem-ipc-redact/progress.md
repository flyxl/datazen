# Track `rem-ipc-redact` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-ipc-redact）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

IPC 错误脱敏：返回路径统一 `redact_secrets_for_log`，修正 `error.rs` 明文断言测试。见计划 §2。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 9a34257c8
- 测试 commit: —
- 合并 commit: —

## 心跳

- 2026-09-03 — BOOTSTRAP: pwd确认/worktree/分支正确，读取规格文档
- 2026-09-03 — CODING: 读完error.rs/command_runtime.rs/db_tools.rs/log_redact.rs/connection_manager.rs，完成3文件修改
- 2026-09-03 — CODING: 全仓grep确认无其他泄漏点；cargo test 1298 passed 0 failed

## 自验结果

- `cargo test -p datazen --lib`：1298 passed, 0 failed, 3 ignored
- 修改文件：`src-tauri/src/commands/error.rs`（Serialize脱敏 + 测试更新）、`src-tauri/src/workflow/command_runtime.rs`（L35脱敏）、`src-tauri/src/services/db_tools.rs`（L33脱敏）
- 关键断言：IPC payload 不含明文密码 `s3cret`、含 `***@` 占位、保留 `Connection failed` 分类
- 错误分类语义不变：NotFound/Validation/Connection/Driver 等枚举保持

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 连接失败错误不含明文密码 | 需 GUI + 坏连接 | 留待 R 回归 |
