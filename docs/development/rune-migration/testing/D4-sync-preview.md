# TN D4 同步只读预览 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`d8d0992` `feat(rune): the sync preview as a read-only grid (issue-rune D4)` + 修复提交 `03aeaa8` `fix(rune): give the sync preview its own grid slot`
- `git show --stat d8d0992`：`crates/datazen-ui-api/src/lib.rs`（`sync_preview` 端口（源必须看得见该表；页只读 "sync preview"）+ 1 条单测）、`crates/datazen-ui/src/lib.rs`（第 3 步 Preview 按钮、`sync_preview_grid` 只读网格 + 1 条运行时测试）
- `git show --stat 03aeaa8`：`crates/datazen-ui/src/data_grid.rs`（`GridSlot::SyncPreview`（解析 `sync_preview_grid`，与结果页共享外观与键——工具页与工作区不同时挂载）、`table_label()`）、`crates/datazen-ui/src/lib.rs`（两处 `result_panel` 调用传槽位）
- 没有 `packages/frontend`、`packages/web`。React 未改

## 过程说明

feat 提交因命令管道退出码绕过了门禁检查而入库（GR1 当时 1 条测试失败：B8 共享面按槽位从 AppState 解析网格，同步预览复用 `result_panel` 时 Result 槽位 panic）。修复提交 `03aeaa8` 为同步预览增加独立槽位并全绿。**最终门禁以修复提交后为准：**

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 87 passed（净增 1：`the_sync_preview_is_a_read_only_grid`），4.73s；`datazen-ui-api` 36 passed（净增 1：`a_sync_preview_is_a_read_only_page`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「同步：预览用 B8 的只读网格。测：预览页不能编辑」核对，断言来自修复后 GR1 实跑：

1. **预览页不能编辑**（运行时 + api）：预览网格 `is_editable()==false`、`begin_edit` 进不了编辑；经 B8 共享面（`result-grid`/`result-cell-0-0`）绘制；replica 预览 analytics 表被拒（源看不见）；未知表拒绝。通过

## 缺陷

无（过程缺陷已由修复提交关闭）。

## 结论与遗留

**放行。**

GR1、GR2 通过（修复提交后）。G1–G4 不适用。预览走 B8 只读共享网格。

遗留（不构成拒绝）：预览只显示第一张选中表；运行步是 D5。
