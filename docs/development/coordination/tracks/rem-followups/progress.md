# Track `rem-followups` — progress

> Initiative: Deep-Review Remediation
> Plan: `docs/development/coordination/deep-review-remediation-plan.md`（§2 rem-followups，Wave 2）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/deep-review-remediation`

## 范围

P2/P3 跟进项合集。见计划 §2。Wave 2：等 Wave 1 合并后启动。

## 状态

- Phase: PASSED
- 代理: tester-rem-followups
- Worktree: .worktrees/datazen-rem-followups
- 分支: feature/rem-followups
- 编码 commit: 95b5faf6e
- 测试 commit: f7a671f1c
- 合并 commit: 512d847a5

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
- 2026-09-04 TESTER BOOTSTRAP + 独立复验 + 覆盖率补齐完成

## 自验结果（编码代理）

- `node scripts/__tests__/resolve-drivers.test.mjs` — 10/10 pass
- `node scripts/check-version-consistency.mjs` — OK (0.1.2)
- `npx tsc --noEmit` — pass
- `npx vitest run src/lib/__tests__/extensionBridge.test.ts` — 22/22 pass
- `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen --lib` — 1322 passed, 0 failed, 3 ignored

## 独立复验结果（测试代理）

- `node scripts/__tests__/resolve-drivers.test.mjs` — **10/10 pass**（与编码一致）
- `node scripts/check-version-consistency.mjs` — **OK (0.1.2)**（与编码一致）
- `node scripts/__tests__/check-version-consistency.test.mjs` — **4/4 pass**（新增）
- `npx tsc --noEmit` — **pass**
- `npx vitest run src/lib/__tests__/extensionBridge.test.ts` — **23/23 pass**（+1 tester 用例）
- `CARGO_TARGET_DIR=target/cargo-wt cargo test -p datazen --lib` — **1327 passed**, 0 failed, 3 ignored（+5 tester 用例；沙箱首次跑 1320/2 fail 为 PermissionDenied 环境假阳性，全权限复验通过）

## 覆盖率（改动模块 ≥80%）

| 模块 | 估计覆盖率 | 说明 |
|------|-----------|------|
| `scripts/check-version-consistency.mjs` | ~100% | 4 用例覆盖 OK / mismatch / missing / invalid semver |
| `scripts/__tests__/resolve-drivers.test.mjs` | ~95% | preset/expander/dedupe/registry 快照 |
| `src-tauri/src/workflow/error.rs` | ~85% | 6 单测覆盖主要 variant + into_ipc_string |
| `src-tauri/src/mcp/allowlist.rs` | 100% | 3 既有单测 |
| `src-tauri/src/mcp/client.rs` (spawn allowlist) | ~90% | allow/deny + .exe strip + datazen |
| `src-tauri/src/mcp/server.rs` (deny-all default) | ~85% | 新增 default empty allowlist 集成测 |
| `src/lib/extensionBridge.ts` (denylist) | ~95% | 全 denylist 遍历 + bridge E_PERMISSION 路径 |

## E2E 登记

| 用例 | 前置 | 执行点 |
|------|------|--------|
| MCP allowlist deny-all UI | 设置页勾选连接 | 留待 R 回归 |
| 插件 command.invoke denylist | 示例插件 | 留待 R 回归 |
| 逐项按需登记 | — | 留待 R 回归 |

## 审查摘要

- WorkflowError 枚举 + `into_ipc_string()` 保持 IPC String 兼容；executor/command_runtime 已收敛。
- MCP spawn allowlist deny-by-default + `env_clear()` 正确；allowlist 空列表 deny-all 已在 server 集成测验证。
- extensionBridge `PLUGIN_COMMAND_DENYLIST` 在 bridge 层拦截，审计日志不泄露 args。
- check-version-consistency 已接入 CI/pre-commit；异常路径（mismatch/missing/invalid）已补齐单测。
- 无阻断性 Bug。
