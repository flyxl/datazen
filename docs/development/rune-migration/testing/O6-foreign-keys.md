# TN O6 外键清单 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`5c0aa0e` `feat(rune): a foreign key list that keeps compound keys together (issue-rune O6)`
- `git show --stat 5c0aa0e` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`ForeignKeySpec`、`table_foreign_keys` 端口 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（`TableView::ForeignKeys`、`view-fks`、`foreign_keys_view` + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 24 passed（净增 1：`a_compound_foreign_key_is_not_split`）；`datazen-ui` 70 passed（净增 1：`the_foreign_keys_view_keeps_compound_keys_together`），3.39s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「外键清单：本表列、目标表、目标列。测：复合外键不拆散」核对，断言来自 GR1 实跑：

1. **复合外键不拆散**（`a_compound_foreign_key_is_not_split` + 运行时测试）：orders 的 `orders_customer_fk`——本表列 `["id","name"]`、目标表 customers、目标列 `["id","name"]`，两侧成组不散；UI 上 `fk-row-0` 呈现且仅一行；catalog 无外键；未知表 Err。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。外键清单只读：名、本表列组、目标（表 + 列组）；复合键两侧均整组保持。

遗留（不构成拒绝）：级联动作（ON DELETE 等）未建模；真实驱动接入后扩展。
