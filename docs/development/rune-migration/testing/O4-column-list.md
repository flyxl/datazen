# TN O4 列清单 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`9565120` `feat(rune): a column list fed by the port (issue-rune O4)`
- `git show --stat 9565120` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`ColumnSpec`、`table_columns` 端口 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（`TableView::Columns`、`view-columns` 按钮、`columns_view` 列表 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 22 passed（净增 1：`table_columns_lists_one_entry_per_column`）；`datazen-ui` 68 passed（净增 1：`the_columns_view_lists_one_row_per_column`），3.28s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`，CARGO_BUILD_JOBS=2 等）。

## 验证

按 tasks.md 验收标准「列清单：名、类型、可空。不是结构编辑器。测：一行对应一列」核对，断言来自 GR1 实跑：

1. **一行对应一列**（`the_columns_view_lists_one_row_per_column`）：customers 开 Columns 视图 → `columns-list`/`columns-header` 在，`column-row-0`..`column-row-7` 共 8 行（8 列），`column-row-8` 不存在；网格离开。端口单测同步核对 8 条 ColumnSpec（id 非空键、name 可空、类型 text、未知表 Err）。通过
2. **不是结构编辑器**（代码核对）：`columns_view` 只有文本行，无输入框、无按钮、无变更路径

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。列清单只读呈现端口的名/类型/可空三字段，一行一列。

遗留（不构成拒绝）：

- 类型全部 "text"（演示端口）；真实类型由驱动适配器给出。
- 无排序/筛选（清单小，不需要）；列宽固定。
