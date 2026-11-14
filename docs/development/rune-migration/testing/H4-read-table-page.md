# TN H4 读一页表接到现有 use case 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`031c1e71 feat(rune): read a table page through the existing use case (issue-rune H4)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`UseCaseAdapter` 增加 `workspace: MemoryWorkspace`（构造时连接演示会话）；`read_table_page` 委托现有读用例 `MemoryWorkspace::open_table`（B1/B3/B4 的分页、排序、过滤逻辑））、`crates/datazen-ui/src/lib.rs`（`show_table` 适配器优先、端口兜底；`open_connection` 的首页改走同一路径）

## 接线方式

- **现有读用例**：共享 UI 世界里现成的读用例是内存工作区的 `open_table`（分页窗口、排序、过滤、暂存覆盖都在其中）。适配器持有自己的已连接内存世界并委托它。桌面宿主之后换活驱动用例（`datazen::table_browsing::api::read_table_page` 需要真实驱动与执行端口，演示进程不可达），接缝不变。
- **UI 读路径**：全部表读取汇聚到 `show_table`（树点击、页脚翻页、重载、标签切换都经它）。H4 起先问适配器，未接线时端口兜底；两个都失败时 `show_table` 返回 false，网格保持上一页（页码、行数、内容都不动）。`open_connection` 的首页读取改走 `show_table`，读路径唯一。

## 测试

| 测试 | 位置 | 断言 |
| --- | --- | --- |
| `the_wired_adapter_reads_pages_through_the_existing_use_case` | datazen-ui-api | 用例答出 customers 首页（列、行、总数 40），第 2 页从存储行 25 开始 |
| `the_wired_adapter_honors_the_sort_and_filter_of_the_read` | datazen-ui-api | 排序（字典序降序首行 customer-9）与过滤（只含 customer-3 的行）都经用例生效 |
| `an_unknown_table_fails_the_wired_read` | datazen-ui-api | 未知表读失败 |
| `the_wired_adapter_serves_the_table_page_before_the_port` | datazen-ui（runtime） | 注入单行适配器后，网格加载的是适配器的页（total_rows=1），不是端口的 40 行 |
| `a_read_that_fails_everywhere_keeps_the_previous_grid_page` | datazen-ui（runtime） | 加载 customers 后读一张不存在的表：`show_table` 返回 false，网格的页码、总行数、已加载页全部保持 |

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 105 passed，datazen-ui-api 53 passed；新增 5 个测试） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 读一页表经适配器接到现有用例，失败时网格保持上一页。遗留：H5 接提交脏格（失败时脏标记还在）；接线后适配器世界与端口世界的暂存/提交在演示进程里的完全一致要等 H5 收口（运行时测试全走端口兜底，不受影响）。
