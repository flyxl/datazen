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
- 测试 commit: 29138aa8d

## 设计决策

- `app_menu.rs`：macOS 菜单标签解析、`MenuAction` 映射、`setup_menu` / `rebuild_menu` Tauri 命令、原生菜单事件单次注册。
- `bootstrap.rs`：`run` / `run_mcp_stdio` 入口、日志/AppState 装配、`invoke_handler` 与窗口生命周期事件。
- `lib.rs` 保留模块声明、crate 内 re-export（含 `SyncAdapterRegistry` 以兼容既有测试 import 路径）。

## 自验结果

| 套件 | 编码代理自报 | 独立实测 (tester) | 备注 |
|------|-------------|-------------------|------|
| lib.rs 行数 | 56 行 | 56 行 | app_menu.rs 729 / bootstrap.rs 950 |
| cargo test -p datazen --lib | 1243 passed; 0 failed; 2 ignored | 1253 passed; 0 failed; 3 ignored | +8 test_tester_ 新增 |
| cargo test app_menu | 11 passed | 14 passed | +3 菜单映射/分支完整性 |
| cargo test bootstrap::tests | — | 17 passed | +5 AppState/IPC/MCP 入口 |
| npx vitest run pathIpcWiring | 8 passed; 859ms | 8 passed; 1.22s | bootstrap.rs 引用路径正确 |

## 新增测试 (tester)

| 测试 | 模块 | 覆盖路径 |
|------|------|----------|
| `test_tester_setup_menu_ids_map_to_concrete_actions` | app_menu | setup_menu 全部自定义 id → 非 Ignore 映射 |
| `test_tester_menu_action_variants_all_reachable` | app_menu | MenuAction 各变体可达 |
| `test_tester_native_menu_handler_covers_all_menu_action_variants` | app_menu | 原生 handler match 分支完整性 |
| `test_tester_lib_reexports_public_entry_points` | bootstrap | lib.rs pub 入口契约 |
| `test_tester_finish_app_state_initializes_extensions_and_registry` | bootstrap | AppState 装配 + 插件目录 |
| `test_tester_invoke_handler_registers_critical_commands` | bootstrap | invoke_handler IPC 面 |
| `test_tester_run_mcp_stdio_entry_chain_wiring` | bootstrap | main → run_mcp_stdio 链路 |
| `test_tester_run_registers_plugins_and_uri_scheme` | bootstrap | GUI 启动插件/URI 注册 |

## E2E 用例登记

| 编号 | 前置 | 步骤摘要 | 断言 | 执行时机 |
|------|------|----------|------|----------|
| — | — | — | — | 【留待 R 回归】菜单/启动行为无功能变更，仅结构拆分 |

## 遗留

—
