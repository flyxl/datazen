# TN B8 结果网格 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`f4b5d2e` `feat(rune): share one grid component between table and result pages (issue-rune B8)`
- `git show --stat f4b5d2e` 三个文件：`crates/datazen-ui-api/src/lib.rs`（`open_result` 端口、`result_open` 锁——结果期暂存拒绝、重开表/断开释放）、`crates/datazen-ui/src/data_grid.rs`（`GridSlot::{Table,Result}`、`grid_of`、`sheet()` 抽取共享、`result_panel()` 无过滤栏无页脚、结果键 `result-` 前缀）、`crates/datazen-ui/src/lib.rs`（`AppState.result_grid`、`load_result_page`、`query_workspace` 渲染共享网格、`app_poll` 按挂载面同步滚动）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 38 passed（B7 时 37，净增 1：`a_query_result_renders_in_the_shared_grid_and_cannot_stage`），2.32s；`datazen-ui-api` 16 passed（B7 时 15，净增 1：`open_result_is_read_only_and_blocks_staging`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「结果页和表页用同一个网格组件，结果页不能暂存。测：结果页调用暂存返回错误」核对，断言来自 GR1 实跑：

1. **open_result 只读且锁暂存**（`open_result_is_read_only_and_blocks_staging`）：返回页 `table=="result"`、`total_rows==2`、`read_only==Some("query result")`；结果期 `stage_cell` 返回 Err；重开 customers 后暂存成功且重开可见（锁释放）。通过
2. **结果页用共享网格渲染**（`a_query_result_renders_in_the_shared_grid_and_cannot_stage`）：查询标签下 `load_result_page` 3 行 2 列 → `result-grid`、`result-cell-0-0`、`result-cell-2-1` 挂载，`query-results` 占位符消失；结果网格 `is_editable()==false`；端口暂存被拒；切到 customers 标签后表网格键 `cell-0-1` 照常（键不冲突），重开表后暂存恢复。通过
3. **同一组件**（代码核对）：表页 `panel` 与结果页 `result_panel` 都经 `sheet(grid, slot, …)` 构建滚动面（同一 `header_row`/`virtual_rows`/`built_columns` 列窗口/冻结列路径）；结果页仅少过滤栏与页脚、不可编辑、键带 `result-` 前缀
4. **既有网格行为回归**：B1–B7 全部测试（分页/页脚/排序/过滤/脏数/提交撤销/只读）复跑通过，`app_poll` 对表页行为不变

开发中修正一处：`data_row` 初版把 slot 硬编码为 Table，导致结果格键无前缀；改为 slot 贯穿后通过。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。结果页与表页共用同一网格组件（`GridSlot` 区分、键前缀隔离）；结果页只读、无过滤栏无页脚；结果期端口暂存被拒，重开表或断开释放。

遗留（不构成拒绝）：

- `load_result_page` 目前只从测试与内部路径触达；SQL 执行 API 与“执行后填充结果区”是 Q4/Q7 的范围。
- 结果网格的键盘导航（方向键）与列排序点击未接线（结果表头不可点）；留给 Q 系列。
- 查询标签关闭后结果网格保留到下一次查询或断开；不构成数据错误。
