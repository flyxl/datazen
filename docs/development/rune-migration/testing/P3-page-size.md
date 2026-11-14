# TN P3 页大小与查询上限 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`53e75b0` `feat(rune): the page size and query cap come from settings (issue-rune P3)`
- `git show --stat 53e75b0` 两个文件：`crates/datazen-ui-api/src/lib.rs`（execute 的 customers 查询按 `QueryMaxRows` 截断，Q4 测试并入上限断言）、`crates/datazen-ui/src/lib.rs`（`table_page_size()` 从端口读、show_table/open_connection 用之、设置页 Data 区两组输入+Set+错误行、1 条新测试）。没有 `packages/frontend`、`packages/web`。React 未改
- 更正：提交信息中 GR1 的 `datazen-ui-api 30 passed` 应为 **29 passed**（查询上限断言并入既有 `execute_answers_fixed_sql_with_a_page`，未新增测试函数）；`datazen-ui 77 passed` 正确

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 77 passed（净增 1：`the_page_size_setting_drives_the_grid_paging`），4.46s；`datazen-ui-api` 29 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「表页大小和查询最大行数。写 P1，B2 和查询读取它。测：页大小改完，下一页按新大小」核对，断言来自 GR1 实跑：

1. **页大小改完下一页按新大小**（`the_page_size_setting_drives_the_grid_paging`）：默认 25；设置页输入 10 → Set → 端口存 `TablePageSize{rows:10}`；回工作区点下一页 → 第 2 页恰 10 行（customer-10..customer-19）。通过
2. **查询读取上限**（api `execute_answers_fixed_sql_with_a_page` 扩展）：上限 100 时 customers 查询回 40 行（全表）；写 `QueryMaxRows{rows:10}` 后回 10 行。通过
3. **非法值拒绝且可见**：输入 0 → 端口仍存 10（旧值保留），设置页出现 `settings-error` 错误行。通过

连带更新：Q7 运行时测试的 customers 查询断言由 25 行改 40 行（上限语义变化，P3 有意行为）。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。B2 翻页与查询截断均读 P1 设置；设置页 Data 区可改两项并显示拒绝原因。

遗留（不构成拒绝）：页大小变化不重载当前已打开的页（下次翻页/重开生效）；输入框无失焦提交（点 Set 或回车）。
