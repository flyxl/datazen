# Track `prh-split-mcp` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-split-mcp** 章节。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 8a54c0a15
- 测试 commit: —
- Agent: rescuer-prh-split-mcp
- Worktree: .worktrees/datazen-prh-split-mcp

## 设计决策

- 将 `server.rs` 拆为编排层（286 行）+ 子模块：`handler.rs`（ServerHandler）、`tools.rs`（#[tool_router]）、`prompts.rs`（#[prompt_router]）、`resources.rs`（资源路由 + 测试辅助）、`types.rs`（输入类型）。
- 跨模块可见性：`#[tool_router(vis = "pub(crate)")]` / `#[prompt_router(vis = "pub(crate)")]`，各 tool/prompt 方法标记 `pub(crate)`，供 handler/resources/tests 调用。
- 集成测试保留在 `server/tests_integration.rs`，通过 `include!` 引入。

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| cargo test -p datazen --lib | 1233 passed; 0 failed; 2 ignored | MCP 集成测试全绿 |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | 【留待 R 回归】MCP stdio 契约无 UI 变更，本轨无 Host UI 改动 |

## 遗留

—
