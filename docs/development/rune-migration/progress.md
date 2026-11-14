## Current Task
- **任务**: 无。plan.md 的任务清单全部完成（R/S/B/Q/O/P/D/W/H/C 四个阶段加 POC）。
- **负责人**: Coder Agent
- **状态**: 迁移计划内无待办；后续新增能力另开任务。
- **计划**: [plan.md](plan.md)

## 约束
React 页面和组件保留。不删除、不改成空壳、不把现有入口摘掉。Rune 是并行的另一套界面。

## Completed Work
- [x] R0 Rune crate 能编译
- [x] R1 SQL Editor POC
- [x] R2 DataGrid POC
- [x] R3 UI 端口和内存表页
- [x] S1–S6、B1–B8、Q、O、P、D、W、H、C1–C2 全部完成（提交与测试报告见 [tasks.md](tasks.md)）
- [x] C3 收尾测量（数字见下节）
- [x] C4 未迁移区域提示（WAPPS/Onboarding 告知 React 页面还在，见 [testing/C4-unmigrated-notice.md](testing/C4-unmigrated-notice.md)）

## Verification
`cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：datazen-ui 113 passed、datazen-ui-api 54 passed；GR2 `cargo check` exit 0。

## 收尾测量（C3）

2 万行和 200 列在 Rune 网格上的实测数字，量自产品面（表标签页的表格视图），量法是 C2 建立的无障碍树读法——树里一行节点就是一行挂载，一格节点就是一格建成：

- **2 万行的页挂载 21 行**：首帧静止（视口 1440×900、sheet 高 380pt、行高 28pt）时，挂载窗口为列表头部的 21 行，与列表长度无关。数字由 `twenty_thousand_rows_mount_a_window_and_two_hundred_columns_build_one_window_of_cells` 断言钉死（报告见 [testing/C3-mount-counts.md](testing/C3-mount-counts.md)）。
- **200 列的页每行建 12 格**：列窗口 COL_WINDOW=12，静止时冻结首列落在窗口内共 12 列，表头同样建 12 个；横向滚动后冻结列在窗口外另建一格，每行 13 格（既有单元测试的上界 ≤ COL_WINDOW+1）。
- 量法不改产品代码：规模页经既有 `load_page` 装入——2 万条空记录，格值走网格的 `r{r}c{c}` 回退，页只占约 480 KB 而非 400 万格。
