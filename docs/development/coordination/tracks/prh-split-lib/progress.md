# Track `prh-split-lib` — progress

> Initiative: Post-Review Hardening
> Plan: `docs/development/coordination/post-review-hardening-plan.md`
> Hub: `docs/development/coordination/hub.md`
> Integration branch: `feat/post-review-hardening`

## 范围

见计划中 **Track prh-split-lib** 章节。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 6fc0c65ca
- 测试 commit: —

## 设计决策

- `app_menu.rs`：macOS 菜单标签解析、`MenuAction` 映射、`setup_menu` / `rebuild_menu` Tauri 命令、原生菜单事件单次注册。
- `bootstrap.rs`：`run` / `run_mcp_stdio` 入口、日志/AppState 装配、`invoke_handler` 与窗口生命周期事件。
- `lib.rs` 保留模块声明、crate 内 re-export（含 `SyncAdapterRegistry` 以兼容既有测试 import 路径）。

## 自验结果

| 套件 | 结果 | 备注 |
|------|------|------|
| cargo test -p datazen --lib | 1243 passed; 0 failed; 2 ignored | CARGO_TARGET_DIR=target/cargo-wt |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | 【留待 R 回归】菜单/启动行为无功能变更，仅结构拆分 |

## 遗留

—
