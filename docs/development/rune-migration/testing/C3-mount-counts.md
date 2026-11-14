# TN C3 2 万行挂载行数与 200 列建格数 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`e38fe31 feat(rune): 20,000 rows mount a 21-row window and a 200-column page builds 12 cells a row (issue-rune C3)`
- 改动面：`crates/datazen-ui/src/lib.rs`（新增 1 个运行时测量测试）。不改产品代码（完成标准的要求）；数字按任务要求写进 `progress.md` 的「收尾测量（C3）」一节。

## 实现

- 规模页经既有 `DataGrid::load_page` 装入：2 万条**空**记录 × 200 列名——页只占约 480 KB（每条空 Vec 24 字节），格值走网格既有的 `r{r}c{c}` 回退，不造 400 万个字符串。
- 挂载与建格从无障碍树数（C2 建立的读法）：树中一行节点即一行挂载，一格节点即一格建成，表头一个 columnheader 即一列建成；窗口外的行没有场景节点，树里数不到。
- 测量面是产品面：表标签页的表格视图（`TableView::Data` → `panel` → `sheet`），视口 1440×900、sheet 高 380pt、行高 28pt，全部为产品常量。

## 测试

| 测试 | 位置 | 断言 |
| --- | --- | --- |
| `twenty_thousand_rows_mount_a_window_and_two_hundred_columns_build_one_window_of_cells` | datazen-ui（runtime，经真实表标签页面 + `Runtime::accessibility_json`） | 装入 20,000×200 页后：网格节点 value="20000 rows × 200 columns"、name=customers；挂载行恰为列表头部 21 行（grid-row-0..20）；每个挂载行建 12 格（cell-{r}-{0..11}）；表头建 12 个（head-0..11）；焦点格 cell-0-0 在挂载内 |

## 实测数字（已记入 progress.md）

- **2 万行的页挂载 21 行**：首帧静止时挂载窗口为列表头部 21 行（380pt sheet / 28pt 行高的窗口），与列表长度无关——2 万行不会挂成 2 万行。
- **200 列的页每行建 12 格**：COL_WINDOW=12，静止时冻结首列落在窗口内共 12 列，表头同样 12 个；横向滚动后冻结列在窗口外另建一格，每行 13 格——既有单元测试 `scroll_offset_moves_the_column_window_and_keeps_the_frozen_column` 的上界（≤ COL_WINDOW+1）。

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 112 passed，新增 1 个测量测试；datazen-ui-api 54 passed） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0（datazen-ui 既有 3 条警告、ui-api 既有 1 条，非本次引入） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 两个数字都有测试钉死：2 万行挂载 21 行、200 列每行建 12 格（滚动后 13）。遗留：挂载窗口随滚动按 rune 的规则增长（视口两侧最多两个视口），静止首帧的 21 行是本任务判据要求的数字；不同视口/行高下数字不同，但都由产品常量决定，测试里以常量断言。
