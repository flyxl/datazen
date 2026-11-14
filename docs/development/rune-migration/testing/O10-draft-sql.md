# TN O10 草稿 SQL 预览 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`21ecd93` `feat(rune): drafts preview as one SQL block, not executed (issue-rune O10)`
- `git show --stat 21ecd93` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`ColumnDraft::sql()` + 1 条单测）、`crates/datazen-ui/src/lib.rs`（`draft-sql` 预览块 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 26 passed（净增 1：`drafts_preview_as_alter_statements`）；`datazen-ui` 74 passed（净增 1：`the_drafts_preview_as_sql_without_executing`），3.93s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「把 O9 变成一段 SQL 预览，不执行。测：改类型出现对应 SQL」核对，断言来自 GR1 实跑：

1. **改类型出现对应 SQL**（`drafts_preview_as_alter_statements` + 运行时测试）：改型草稿 (name → varchar(80)) 的 SQL 为 `ALTER TABLE customers ALTER COLUMN name TYPE varchar(80);`；加列草稿为 `ALTER TABLE customers ADD COLUMN score int;`。UI 上排队改型草稿后 `draft-sql` 块呈现（无草稿时无此块）；端口列数不变——预览不执行。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。草稿渲染为一段 ALTER 语句预览（逐行显示），仅展示不执行。

遗留（不构成拒绝）：预览无复制按钮（C 系列）；应用是 O11。
