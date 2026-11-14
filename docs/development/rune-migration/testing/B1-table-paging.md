# TN B1 分页打开表 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`093b77a` `feat(rune): page open_table through the memory workspace (issue-rune B1)`
- `git show --stat 093b77a` 三个文件：`crates/datazen-ui-api/src/lib.rs`（open_table 加页码和页大小、TablePage 加 total_rows、OpenTable 记录打开页、stage_cell 按页偏移寻址）、`crates/datazen-ui/src/data_grid.rs`（网格记录 page/page_size/total_rows）、`crates/datazen-ui/src/lib.rs`（TABLE_PAGE_SIZE=25、show_table/open_connection 传分页参数）。没有 `packages/frontend`、`packages/web`。React 未改
- 附带修复：`commit_edit` / `set_editable` / `load_page` 释放编辑器的 IME 焦点（`editor.focused = false`），单元格编辑器的 `on_submit` 增加 `tx.blur()`。原因：25 行页面上卸载后的聚焦编辑器仍按焦点路径被 rune 的 `virtual_inputs` 离屏输入解析（runtime/input/keys.rs `with_focused_state`），把 `Command('c')` 当成自己的空选区复制（copy_request 变空串），网格的 on_key 收不到。40 行全表时该路径不触发，纯属基线侥幸。网格编辑器的提交测试改走生产路径（第二次 Enter → on_submit → blur），直接 `commit_edit` 的模型行为由 data_grid 单测覆盖
- 探针在 `/home/z/my-project/scripts/probes/b1_probe.rs`，用 `#[path]` 挂进 `datazen-ui` 测试模块，跑完已从 lib.rs 摘除（当前树上无挂载）

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1（rune 上游锁定）。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 29 passed（S6 时 28，净增 1：`opening_a_table_loads_the_first_page_of_the_table`），1.51s；`datazen-ui-api` 6 passed（S6 时 3，净增 3：`the_second_page_is_not_the_first_page`、`an_out_of_range_page_clamps_to_the_last_page`、`staging_addresses_the_row_behind_the_open_page`），0.00s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「第 2 页不是第 1 页的行」逐条核对，运行时断言来自 GR1 中的测试（`--test-threads=1` 实跑）：

1. **第 2 页不是第 1 页**（`the_second_page_is_not_the_first_page`）：customers 40 行、页大小 25，第 1 页 25 行、首行 id "0"；第 2 页 15 行、首行 id "25"；`total_rows` 40；两页首行 `assert_ne`。通过
2. **越界页钳到末页**（`an_out_of_range_page_clamps_to_the_last_page`）：请求第 99 页返回 page=1、15 行。通过
3. **暂存按页偏移寻址**（`staging_addresses_the_row_behind_the_open_page`）：打开第 2 页后暂存第 0 行 name，落到存储行 25——重开第 2 页可见 "staged"，第 1 页第 0/24 行原值不动。通过
4. **UI 只装一页**（`opening_a_table_loads_the_first_page_of_the_table`）：连接后开 customers 标签，`grid.rows == 25`（不是 40），`page() == 0`、`page_size() == 25`、`total_rows() == 40`，`cell(24,1) == "customer-24"`。通过
5. **焦点回归**（`data_grid_edits_a_cell_and_copies_the_selection`，改造后）：提交后 `Command('c')` 的 copy_request 为 `Some("A")`（不再是空串）。通过

调试记录（探针实跑，已删）：`b1_p_deep` 显示 25 行页面上 `Command('c')` 时 app 的 on_key 不触发、copy 发生在 frame 期间且内容为空串；基线（40 行全表）同一探针 on_key 正常触发。据此定位为 rune `virtual_inputs` 离屏聚焦输入的派发，按上文方式修复。

## 缺陷

无。修复项（编辑器焦点释放）随本提交进入，属于 B1 范围内的回归处理。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。`open_table(table, page, page_size)` 只返回该页切片并带全表行数，越界钳到末页；`stage_cell` 的行号按打开页的偏移映射到存储行。UI 以 25 行为页。编辑器卸载后不再持有 IME 焦点。

遗留（不构成拒绝）：

- `show_table` 目前固定打开第 1 页；页间导航是 B2 的页脚任务。
- 排序、过滤下的分页（B3、B4）会在排序/过滤后的行序列上分页，本任务的钳制逻辑不变。
