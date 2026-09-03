# Track `prh-split-lib` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-split-lib** 章节。

## 状态

- Phase: PASSED
- 编码 commit: 6fc0c65ca
- 测试 commit: 936243d13

## 设计决策

- `app_menu.rs`：macOS 菜单标签解析、`MenuAction` 映射、`setup_menu` / `rebuild_menu` Tauri 命令、原生菜单事件单次注册。
- `bootstrap.rs`：`run` / `run_mcp_stdio` 入口、日志/AppState 装配、`invoke_handler` 与窗口生命周期事件。
- `lib.rs` 保留模块声明、crate 内 re-export（含 `SyncAdapterRegistry` 以兼容既有测试 import 路径）。

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| lib.rs 行数 | 56 行 | app_menu.rs 629 / bootstrap.rs 850 |
| cargo test -p datazen --lib | 1243 passed; 0 failed; 2 ignored | 独立复验 2026-09-03 |
| cargo test app_menu | 11 passed; 0 failed | 菜单映射/标签/单次注册 |
| cargo test bootstrap | 16 passed; 0 failed | 启动/MCP/AppState/IPC 契约 |
| npx vitest run pathIpcWiring | 8 passed; 0 failed | 859ms |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | 【留待 R 回归】菜单/启动行为无功能变更，仅结构拆分 |

## 遗留

—
