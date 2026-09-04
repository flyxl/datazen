# Track `rem-followups` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-followups，Wave 2）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

P2/P3 跟进项合集。见计划 §2。Wave 2：等 Wave 1 合并后启动。

## 状态

- Phase: READY_FOR_TEST
- 代理: coder-rem-followups
- Worktree: .worktrees/datazen-rem-followups
- 分支: feature/rem-followups
- 编码 commit: 95b5faf6e
- 测试 commit: —
- 合并 commit: —

## 任务完成情况

| # | 任务 | 状态 | 说明 |
|---|------|------|------|
| 1 | resolve-drivers.mjs 单测 | ✅ | `scripts/__tests__/resolve-drivers.test.mjs`（node:test，10 用例 + registry 快照） |
| 2 | 版本一致性守护 | ✅ | `scripts/check-version-consistency.mjs` + `pnpm test:version`；接入 CI / ci-local / pre-commit |
| 3 | WorkflowError 枚举化 | ✅ | `src-tauri/src/workflow/error.rs`；executor + command_runtime 收敛，IPC 仍 String |
| 4 | save_settings 原子化 | ✅ 已有 | `store/mod.rs` `write_file_atomic` + `save_json_file`；`save_json_file_leaves_no_tmp_artifacts` 单测覆盖 |
| 5 | MCP 子进程安全 | ✅ | `client.rs` spawn allowlist + `env_clear()`；`allowlist.rs` 默认 deny-all |
| 6 | 插件 command:invoke denylist | ✅ | `extensionBridge.ts` `PLUGIN_COMMAND_DENYLIST` + 单测 |
| 7 | 文档修正 | ✅ | `testing.md` 契约层 §4.7 + e2e 路径修正 |
| — | CSP 收紧 | ⏭ 取舍 | 需评估前端直连需求，留待 R |
| — | Windows `.key`/ACL | ⏭ 取舍 | 性价比低，未实施 |
| — | L1/L2/L4/L9 | ⏭ 取舍 | Zeroize/SQLCipher/导入限大小/限流，未实施 |

## 心跳

- 2026-09-04 BOOTSTRAP + CODING 完成

## 自验结果

- `node scripts/__tests__/resolve-drivers.test.mjs` — 10/10 pass
- `node scripts/check-version-consistency.mjs` — OK (0.1.2)
- `npx tsc --noEmit` — pass
- `npx vitest run src/lib/__tests__/extensionBridge.test.ts` — 22/22 pass
- `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen --lib` — 1322 passed, 0 failed, 3 ignored

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| MCP allowlist deny-all UI | 设置页勾选连接 | 留待 R 回归 |
| 插件 command.invoke denylist | 示例插件 | 留待 R 回归 |
| 逐项按需登记 | — | 留待 R 回归 |
