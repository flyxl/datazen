# TN C2 网格的行数、列数、当前格读给无障碍树 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`5178765 feat(rune): rows, columns and the current cell read to the a11y tree (issue-rune C2)`
- 改动面：`crates/datazen-ui/src/data_grid.rs`（sheet/行/格/表头四个构建点）、`crates/datazen-ui/src/lib.rs`（1 个运行时测试）、`crates/datazen-ui/Cargo.toml` + `Cargo.lock`（测试读 a11y 树需要 serde_json，取工作区已固定的版本）

## 实现

- 依据 rune 既有的无障碍设施（`Runtime::accessibility_json` 与 rune-kit 表格组件的同款模式），改动都在我们这一侧，不动 vendor/rune：
- **网格节点**（`sheet()` 的滚动容器，Table/Result/SyncPreview 三槽共享）：`role("grid")`、`label` 为表名、`a11y_value` 为已加载页的形状 `"{rows} rows × {cols} columns"`，并 `active_descendant` 指向焦点格的键。
- **行**：带虚拟列表的行键（`grid-row-{index}`，与列表键一致），`role("row")`；列表索引自动给出 position/setSize，行数随读。
- **格与表头**：`role("gridcell")`、`role("columnheader")`；当前格的名字即其单元格内容。
- 树随模型走：移动焦点换 descendant，换页读新页自己的形状。焦点格未挂载（虚拟化窗口外）时 descendant 省略——与 rune-kit 表格对游标行的处理一致。

## 测试

| 测试 | 位置 | 断言 |
| --- | --- | --- |
| `the_grid_reads_its_shape_and_current_cell_to_the_accessibility_tree` | datazen-ui（runtime，经 `app_key` 真实键路 + `Runtime::accessibility_json`） | 打开 customers 后树中网格节点 role=grid、name=customers、value="25 rows × 8 columns"、activeDescendant=cell-0-0；该格节点 role=grid-cell、名字为其值"0"。Down×2+Right 后 descendant 变 cell-2-1、该格名字"customer-2"。翻到第 2 页后 value="15 rows × 8 columns"（40 行、页大小 25 余 15；经 `runtime.update` 标脏，树读到新页形状） |

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 111 passed，新增 1 个测试；datazen-ui-api 54 passed） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0（datazen-ui 既有 3 条警告，非本次引入） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 网格节点对读屏器给出角色、表名、页形状与焦点格；焦点移动与换页都被树跟随。遗留：焦点格滚出虚拟化窗口时 descendant 省略（虚拟列表的固有行为，与 rune-kit 组件一致）；编辑态下焦点格内是文本编辑器节点，由 rune 自身按 textbox 读出，未额外断言——均不属本任务判据（不要求屏幕阅读器全套剧本）。
