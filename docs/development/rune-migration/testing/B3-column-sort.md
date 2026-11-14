# TN B3 排序 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`4c89e32` `feat(rune): sort a table page by one column (issue-rune B3)`
- `git show --stat 4c89e32` 三个文件：`crates/datazen-ui-api/src/lib.rs`（`ColumnSort`、trait 加 sort 参数、TablePage 回显 sort、内存按全表排序后再分页）、`crates/datazen-ui/src/data_grid.rs`（网格记录排序列索引、表头可点并显示 ↑/↓）、`crates/datazen-ui/src/lib.rs`（AppState.sort、cycle_sort 三态轮换、排序回第 1 页、连接/断开重置）。没有 `packages/frontend`、`packages/web`。React 未改
- 排序语义：稳定字典序升序；降序为升序反转。表头点击轮换：无 → 升 → 降 → 无

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 32 passed（B2 时 31，净增 1：`clicking_a_header_cycles_none_ascending_descending`），1.81s；`datazen-ui-api` 8 passed（B2 时 6，净增 2：`sorting_orders_the_page_over_the_whole_table`、`sorting_by_an_unknown_column_is_an_error`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「一个排序列，表头点击轮换无序、升序、降序；内存排序，测一行顺序」核对，断言来自 GR1 实跑：

1. **内存排序顺序**（`sorting_orders_the_page_over_the_whole_table`）：按 name 升序开页，`rows[0][1]=="customer-0"`、`rows[2][1]=="customer-10"`（字典序第 3 位，与存储序不同，证明排序发生）；降序 `rows[0][1]=="customer-9"`、`rows[6][1]=="customer-39"`；sort 回显在页上。通过
2. **未知排序列报错**（`sorting_by_an_unknown_column_is_an_error`）：`open_table` 返回 Err。通过
3. **表头三态轮换 + 回第 1 页**（`clicking_a_header_cycles_none_ascending_descending`）：先翻到第 2 页；点 `head-1` → `page()==0`、`sort()==(1,false)`、`cell(2,1)=="customer-10"`；再点 → `(1,true)`、`cell(0,1)=="customer-9"`；三点 → `None`、`cell(2,1)=="customer-2"`（存储序恢复）。通过
4. **分页/排序组合**（代码核对 + GR1 既有 `the_footer_pages_through_the_table_and_stops_at_the_edges` 复跑通过）：`order` 序列先排序后分页，翻页在排序视图上继续

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。单一排序列、表头点击三态轮换、方向箭头、排序后回第 1 页；内存工作区对全表排序后再切页；页上回显 sort 供网格显示。

遗留（不构成拒绝）：

- 排序为字符串字典序（演示数据均为文本）；数值感知排序（按类型比较）留给接入真实驱动后的任务。
- `stage_cell` 仍按存储行寻址，排序视图上的暂存行号是页内行号（B1 合同不变）。
