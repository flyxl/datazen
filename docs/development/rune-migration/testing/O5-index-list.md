# TN O5 索引清单 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`86f124b` `feat(rune): an index list with ordered compound columns (issue-rune O5)`
- `git show --stat 86f124b` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`IndexSpec`、`table_indexes` 端口 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（`TableView::Indexes`、`view-indexes`、`indexes_view` + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 23 passed（净增 1：`a_compound_index_keeps_its_column_order`）；`datazen-ui` 69 passed（净增 1：`the_indexes_view_lists_the_indexes_with_ordered_columns`），3.44s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「索引清单。测：复合索引的列顺序还在」核对，断言来自 GR1 实跑：

1. **复合索引列顺序**（`a_compound_index_keeps_its_column_order` + 运行时测试）：customers 两索引——`customers_pkey`（unique，[id]）与 `customers_name_city_idx`（[name, city]，顺序保持）；UI 上 `index-row-0/1` 呈现、`index-row-2` 不存在，列序经端口断言 `["name", "city"]`。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。索引清单只读：名、列（按索引序拼接）、唯一性；复合顺序在端口与页面两端保持。

遗留（不构成拒绝）：索引元数据（部分索引谓词、方法）未建模；真实驱动接入后扩展。
