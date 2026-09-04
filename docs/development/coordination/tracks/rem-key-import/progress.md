# Track `rem-key-import` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-key-import）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

备份导入 `.key` 覆盖防护：警告 + 拒绝覆盖（legacy 改 opt-in）。见计划 §2。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 199c1f483
- 测试 commit: —
- 合并 commit: —

## 心跳

- 2026-09-08 BOOTSTRAP: worktree confirmed, branch=feature/rem-key-import
- 2026-09-08 CODING: added ImportOptions + key-overwrite guard + IPC param + 4 unit tests
- 2026-09-08 SELF-VERIFY: `cargo test -p datazen --lib app_data_archive` — 32 passed, 0 failed

## 自验结果

- `cargo test -p datazen --lib app_data_archive`：32 passed, 0 failed
- 新增 4 个 key-overwrite guard 测试全绿：
  - `import_rejects_zip_with_key_over_existing_key` — zip 含 `.key` + 目标已有 `.key` → 拒绝
  - `import_succeeds_when_zip_has_no_key` — 无 `.key` 导入正常
  - `import_with_allow_key_overwrite_overwrites_existing_key` — opt-in 放行
  - `import_with_key_succeeds_when_no_existing_key` — 目标无 `.key` 不触发守卫
- 既有 28 个 app_data_archive 测试全绿（含更新后的 `import_with_legacy_key_in_zip_restores_key_from_archive`）
- IPC 入口 `commands/config.rs` 新增 `allow_key_overwrite: Option<bool>` 参数，默认 false

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| 导入含 `.key` 的备份被拒绝/警告 | 需 GUI + 备份 zip | 留待 R 回归 |
