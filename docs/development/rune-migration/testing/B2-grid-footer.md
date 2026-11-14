# TN B2 网格页脚 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`ea0d008` `feat(rune): page the grid with a prev/next footer (issue-rune B2)`
- `git show --stat ea0d008` 两个文件：`crates/datazen-ui/src/data_grid.rs`（`pages()` 页数助手、`footer()` 页脚元素、panel 高 448 容纳页脚、翻页保列窗口单测）、`crates/datazen-ui/src/lib.rs`（AppState.table_page、next/prev_table_page 边界守卫、reload_table_page、sync 时页归零）。没有 `packages/frontend`、`packages/web`。React 未改
- 页脚内容：`page-prev`（‹ Prev）、`grid-page-label`（Page X / Y）、`page-next`（Next ›）、页大小与全表行数（N per page · M rows）。按 P3 的后续要求，页大小目前是常量 TABLE_PAGE_SIZE=25，P1 落地设置后由此处读取

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 31 passed（B1 时 29，净增 2：`the_footer_pages_through_the_table_and_stops_at_the_edges`、`loading_the_next_page_keeps_the_column_window_and_the_frozen_column`），1.73s；`datazen-ui-api` 6 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「翻页后列窗口和冻结列还在」核对，断言来自 GR1 实跑：

1. **页脚翻页与边界**（`the_footer_pages_through_the_table_and_stops_at_the_edges`）：customers（40 行、25/页、2 页）。首页点 `page-prev` 无操作（page 仍 0）；点 `page-next` 落到第 2 页——`page()==1`、`rows==15`、`cell(0,1)=="customer-25"`；末页再点 `page-next` 无操作；点 `page-prev` 回第 1 页且 `cell-0-1` 仍挂载。通过
2. **翻页后列窗口与冻结列**（`loading_the_next_page_keeps_the_column_window_and_the_frozen_column`，data_grid 单测）：30 列 50 行、25/页。第 1 页滚动到 origin_col=20 后装第 2 页：`page()==1`、`pages()==2`、新页 origin_col 归 0；再滚到 20，`built_columns()` 同时含冻结列 0 和窗口列 20。通过
3. **翻页路径复用 show_table**（代码核对）：`next/prev_table_page` → `reload_table_page` → `show_table`，与标签切换同一打开路径；`show_table` 采纳 `open_table` 钳制后的页码，页码不会越过末页累计

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。页脚提供上一页/下一页/页码/页大小/行数；翻页在两端无操作；每次翻页重新走 `open_table` 分页路径，新页从第 1 列起、冻结列机制不变。

遗留（不构成拒绝）：

- 切换标签时页码归零（`sync_grid_to_selected_tab` 设 `table_page = 0`），回到先前标签不恢复页位——单标签页位记忆留给后续任务按需补。
- 页脚按钮未做禁用置灰，两端点击为无操作；视觉禁用态留给 C 系列打磨。
