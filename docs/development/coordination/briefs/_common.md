# 子代理共用纪律（所有轨道必读）

> 全文方案：`docs/todo/sql-editor/implementation-plan.md`（深查字典，日常不读）。
> 本轨 brief：`docs/development/coordination/briefs/<track>.md`（必读）。

## 1. 身份与工作区

- 只在分配的 worktree（`.worktrees/datazen-<track>`，分支 `feature/<track>`）工作；严禁触碰主检出。
- 只读写本轨 `docs/development/coordination/tracks/<track>/progress.md` + `bugs.md`；**严禁碰 `hub.md`**（协调者聚合生成）。
- Coder 只报 `READY_FOR_TEST`，不得自称通过；Tester 必须是全新实例，只测不修。

## 2. 硬禁令

- **dbx clean-room**：禁止 import `docs/todo/sql-editor/dbx` 下任何文件；禁止机械翻译其结构/文件名/符号。Tester 必查。
- **单生产文件 <500 行**（推荐 200–300）；超了先拆。
- **Host 禁止按驱动 ID 写分支**；行为差异走 `DB_REGISTRY` / `DatabaseTypeMeta` / 方言适配器。
- **ID 术语**：`connectionId` = 持久化配置 id；`dbSessionId` = 运行时会话 id；不得混用，不得依赖 `resolve_session` 回退。
- **i18n**：开发期只改 `en.ts` + `zh-CN.ts`；不碰其他语言。
- **生产路径禁止裸 `unwrap()` / `expect()`**（`#[cfg(test)]` 除外）。
- **驱动相关测试落点**：驱动方言/专属 UI/专属 Command 测试必须写在 `packages/drivers/<id>/` 内，禁止放 Host（`src-tauri/`、`src/`、`e2e/specs/`）。
- Host 新增/变更 UI 路径必须有 E2E；驱动 E2E 进 `packages/drivers/<id>/e2e/`。

## 3. 命令与环境

- 严禁 `pnpm install`（依赖软链主检出，CLI 用 `npx` 调用）。
- 严禁 bash 全仓 grep，用 Grep 工具；编辑文件前必须先 Read。
- Rust 操作设置 `CARGO_TARGET_DIR=target/cargo-wt`。
- 禁止 merge / rebase / cherry-pick；禁止提交 codegen / gitignored 文件。

## 4. 测试门禁

- 新模块 statement/branch/function/line coverage ≥80%；安全、binder、range、risk 类目标 ≥90%。
- 测试命名：新增测试前缀 `test_tester_` 或标注 `[tester]` 以区分来源。
- Tester 四阶段：A 审查 → B 独立重跑 → C 覆盖率补齐+E2E → D 判定（PASSED/FAILED，一次性上报 Bug 清单）。
- 同一轨最多 5 轮修复-复测循环，超限标 `ESCALATED` 上报。

## 5. 核心架构不变量（各轨 brief 按需引用 implementation-plan §3/§4 细节）

- 键入期分析只读前端本地快照；completion/hover/inlay 回调不得直接发 IPC。
- 前端语句范围/语义/风险/参数共享同一 TS 词法核心（S2-A 的 scanner），禁止各写一套。
- Rust Host 安全扫描独立实现，用共享 JSON fixture 对齐行为，不跨语言复用运行时。
- 安全确认只是体验门；Rust `readOnly` + Safe Mode 是最终防线。

## 6. 完工报告协议（防协调者空等）

- 完工（或确认 BLOCKED）后，必须在同一 turn 内返回结构化收尾报告，不得静默结束：
  - 首行状态关键字：`READY_FOR_TEST` / `TEST_DONE` / `TEST_FAILED` / `BLOCKED`
  - Commit hash、改动文件清单、实测数字（命令与结果）
  - Tester 另附：审查摘要、覆盖率报告、新增测试清单；FAILED 另附 Bug ID 清单与阻断原因
- 先写回本轨 `progress.md`（Phase 流转 + commit hash + 数字），再返回；两者一致。
- 工作期间至少每 5 分钟更新 `progress.md` 的 LastHeartbeat；卡住超 10 分钟无进展必须主动上报 BLOCKED + 证据，不等被问。
- 协调者以子代理的结算通知为推进依据，不做高频轮询。
