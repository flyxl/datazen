# Track `prh-split-mcp` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-split-mcp** 章节。

## 状态

- Phase: PASSED
- 编码 commit: 8a54c0a15
- 测试 commit: （本次提交后更新）
- Agent: tester-prh-split-mcp（独立实例 #2）
- Worktree: `.worktrees/datazen-test-prh-split-mcp`

## 设计决策

- 将 `server.rs` 拆为编排层（290 行）+ 子模块：`handler.rs`（ServerHandler）、`tools.rs`（#[tool_router]）、`prompts.rs`（#[prompt_router]）、`resources.rs`（资源路由 + 测试辅助）、`types.rs`（输入类型）。
- 跨模块可见性：`#[tool_router(vis = "pub(crate)")]` / `#[prompt_router(vis = "pub(crate)")]`，各 tool/prompt 方法标记 `pub(crate)`，供 handler/resources/tests 调用。
- 集成测试保留在 `server/tests_integration.rs`，通过 `include!` 引入。

## 自验结果（编码代理）

| 套件 | 结果 | 备注 |
|------|------|------|
| cargo test -p datazen --lib | 1233 passed; 0 failed; 2 ignored | MCP 集成测试全绿 |

## 独立复验结果（测试代理）

| 验收项 | 结果 | 实测 |
|--------|------|------|
| server.rs ≤800 行 | PASS | 290 行 |
| MCP 契约不变 | PASS | `mod.rs` 公开 API 未改名；`MCP_ALL_TOOLS` 10 项一致；`start_mcp_stdio`/`start_mcp_transport` 未变；`contract.rs` golden 测试通过 |
| cargo test -p datazen --lib | PASS | 1251 passed; 0 failed; 3 ignored |
| 模块职责清晰 | PASS | handler/tools/prompts/resources/types 拆分合理，无冗余 pub 泄漏 |
| MCP 测试覆盖 | PASS | `cargo test -p datazen --lib mcp::` → 102 passed; 0 failed; 1 ignored |

模块行数：`handler.rs` 105、`tools.rs` 256、`prompts.rs` 136、`resources.rs` 225、`types.rs` 103、`server/tests_integration.rs` 758。

### 编码代理自报 vs 独立实测

| 指标 | 编码自报 | 独立实测 | 差异说明 |
|------|----------|----------|----------|
| 全量 lib 测试 | 1233 passed | 1251 passed | +18（含 contract 测试与本轨新增 6 项 tester 测试） |
| MCP 聚焦测试 | 94 passed | 102 passed | +8（contract + tester 新增） |

## 新增测试（tester）

| 函数 | 类别 | 覆盖路径 |
|------|------|----------|
| `test_tester_tool_is_registered_cross_module_consistency` | 跨模块 | handler 门闸 `tool_is_registered` ↔ tools.rs `tool_router` |
| `test_tester_listed_resources_readable_via_read_resource_inner` | 跨模块 | `list_resources_inner` 列出的 URI 均可被 `read_resource_inner` 读取 |
| `test_tester_call_tool_inner_rejects_empty_tool_name` | 边界 | 空工具名拒绝 |
| `test_tester_read_resource_blank_uri_returns_not_found` | 边界 | 空 URI 返回 resource_not_found |
| `test_tester_concurrent_mcp_operations` | 边界/并发 | 并发 read_resource + list_connections 无交叉污染 |
| `test_tester_list_tools_inner_snapshot_matches_mcp_all_tools` | 回归 | 拆分后 `list_tools_inner` 名称集与 `MCP_ALL_TOOLS` 一致 |

## 代码审查摘要

- 模块拆分逻辑正确：`handler.rs` 编排 ServerHandler；`tools.rs`/`prompts.rs` 各自承载 macro router；`resources.rs` 承载资源读写与 `call_tool_inner` 测试辅助；`types.rs` 纯类型；`server.rs` 保留 struct 与核心 helper。
- 可见性合理：子模块均为 `mod`（crate-private），对外 re-export 仅 `DataZenMcpServer` / `MCP_ALL_TOOLS`（`server.rs`）及 `mod.rs` 原有 client 导出，无新增 pub 泄漏。
- `handler.rs` / `resources.rs` 存在 `tool_router` unused import 警告（非功能问题，不登记 bug）。
- `explain_plan_prompt` 中 `unwrap_or_else` 用于 JSON 序列化 fallback（既有模式，测试模块除外可接受）。
- 无死代码或 API 契约破坏；`MCP_ALL_TOOLS` 与 golden fixture 对齐。

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | 【留待 R 回归】MCP stdio 契约无 UI 变更，本轨无 Host UI 改动 |

## 遗留

—
