# TN B4 单列过滤 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`0be8cc5` `feat(rune): filter one column from a bar above the grid (issue-rune B4)`
- `git show --stat 0be8cc5` 三个文件：`crates/datazen-ui-api/src/lib.rs`（`ColumnFilter`、trait 加 filter 参数、过滤→排序→分页的行序列、TablePage 回显 filter、total 为过滤后行数）、`crates/datazen-ui/src/data_grid.rs`（过滤栏 `grid-filter`：列芯片 + 文本框 + 行数、panel 高 484）、`crates/datazen-ui/src/lib.rs`（AppState 的 filter/filter_column/filter_editor、apply_filter、cycle_filter_column、各重置路径）。没有 `packages/frontend`、`packages/web`。React 未改
- 过滤语义：单列 contains 文本匹配；空文本 = 无过滤；过滤后 total_rows 为命中行数；页脚页数按过滤后总数

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 33 passed（B3 时 32，净增 1：`the_filter_bar_narrows_the_page_and_resets_to_the_first_page`），2.00s；`datazen-ui-api` 11 passed（B3 时 8，净增 3：`a_filter_narrows_the_page_and_the_total`、`a_filter_with_sort_pages_over_the_matching_rows`、`filtering_an_unknown_column_is_an_error`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「一个文本过滤，作用在单个列，过滤后回到第 1 页；过滤掉的行不在页里」核对，断言来自 GR1 实跑：

1. **过滤收窄页与总数**（`a_filter_narrows_the_page_and_the_total`）：name 列 contains "customer-3" → 11 行（customer-3 与 customer-30..39），`total_rows==11`，全部行命中，filter 回显。通过
2. **过滤 + 排序组合**（`a_filter_with_sort_pages_over_the_matching_rows`）：同过滤加 name 降序 → `rows[0][1]=="customer-39"`、`rows[10][1]=="customer-3"`，排序作用在命中集上。通过
3. **未知过滤列报错**（`filtering_an_unknown_column_is_an_error`）。通过
4. **UI 过滤栏交互**（`the_filter_bar_narrows_the_page_and_resets_to_the_first_page`）：先翻到第 2 页；点 `filter-column` 芯片从 id 列切到 name 列（`filter_column==1`）；点 `filter-input` 聚焦、键入 Text("customer-3")、Enter 提交 → `page()==0`（回第 1 页）、`total_rows()==11`、`rows==11`、`cell(0,1)` 与 `cell(10,1)` 都含 "customer-3"、提交后输入框失焦；清空编辑器再应用 → 40 行 25 行返回。通过
5. **过滤掉的行不在页里**：第 4 条的 11 行断言即覆盖（customer-4 等非命中行不出现在任何页位）

## 缺陷

无。推送首次遇到瞬时 TLS/认证抖动，重试成功，与本提交内容无关。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。单列文本过滤、列芯片循环选择、Enter 应用并失焦、过滤回第 1 页、空文本清除、页脚按命中数翻页；内存工作区按过滤→排序→分页的次序产出页。

遗留（不构成拒绝）：

- 过滤在 Enter 提交时应用，不是逐键即时过滤；即时过滤留给 C 系列按性能记录决定。
- 标签切换/连接/断开都会清过滤；同表重开不保留过滤条件。
