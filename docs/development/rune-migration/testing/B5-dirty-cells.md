# TN B5 脏标记 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`815f525` `feat(rune): count dirty cells, not edits, in the status line (issue-rune B5)`
- `git show --stat 815f525` 一个文件：`crates/datazen-ui/src/data_grid.rs`（`original()` 取加载页原值、`dirty_cells()` 只数与原值不同的编辑、状态行抽为纯函数 `status_line()` 并改报脏数）。没有 `packages/frontend`、`packages/web`。React 未改
- 语义：显示值（`edits` 覆盖层）与脏标记分开；脏 = 显示值 ≠ 加载页原值；改回原值即不脏。状态行只数脏格，不再数编辑次数

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 35 passed（B4 时 33，净增 2：`a_cell_edited_back_to_its_original_value_is_not_dirty`、`the_status_line_counts_dirty_cells_not_edits`），1.93s；`datazen-ui-api` 11 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「脏标记和显示值分开。状态栏只数脏格。改一格再改回原值，脏数为 0」核对，断言来自 GR1 实跑：

1. **改回原值脏数为 0**（`a_cell_edited_back_to_its_original_value_is_not_dirty`）：编辑 (2,2) 追加 "x" → `cell=="r2c2x"`、`dirty_cells()==1`；再次编辑回 "r2c2" → `cell=="r2c2"`、`dirty_cells()==0`。通过
2. **状态行只数脏格**（`the_status_line_counts_dirty_cells_not_edits`）：一次有效编辑加一次改回原值后，状态行含 "0 dirty" 且选择区照常显示。通过
3. **显示值与脏标记分离**（代码核对）：`edits` 仍是显示覆盖层（`cell()` 优先读它），`dirty_cells()` 每次对 `original()` 比对，不存在单独的脏集合会失步

## 缺陷

无。开发中断言曾误写 "1×"（POC 网格 20000×200），修正为选择区断言后通过，属于测试笔误非产品缺陷。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。脏标记与显示值分离，状态行只报脏格数，改回原值清脏。

遗留（不构成拒绝）：

- 工作区侧 `stage_cell` 仍是立即写透（B1 语义）；提交/撤销的端口与按钮是 B6 的范围，届时网格脏数与工作区暂存数对齐。
- 翻页/重开页会清 `edits`，脏数随之归零——与 B6 提交按钮配合的跨页暂存显示留给 B6 验证。
