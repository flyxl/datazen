# TN O9 列变更草稿 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`39a0028` `feat(rune): column-change drafts kept in UI state (issue-rune O9)`
- `git show --stat 39a0028` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`ColumnDraft::{AddColumn,ChangeType}` 类型 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（`column_drafts` 按表存草稿、`draft_name`/`draft_type` 输入、`add_column_draft`/`add_retype_draft`、`column_drafts_section` 草稿区 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 25 passed（净增 1：`column_drafts_are_data_not_state_changes`）；`datazen-ui` 73 passed（净增 1：`column_drafts_stay_separate_from_the_official_columns`），3.97s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「列变更草稿：加列、改类型。只在内存。测：草稿和 O4 的正式列分开」核对，断言来自 GR1 实跑：

1. **草稿与正式列分开**（`column_drafts_stay_separate_from_the_official_columns` + api 单测）：Columns 视图下经草稿区加 "score int" 草稿与 "name varchar(80)" 改型草稿——`draft-row-0/1` 入列；正式列仍 8 行（`column-row-8` 不存在）；端口 `table_columns` 仍 8 条。api 侧草稿纯数据、构造不动任何列。通过

测试时机修正（已知规则）：视图切换进入帧透明度为 0，等一帧完成淡入再点击。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。加列/改型两类草稿、只在内存（UI 状态、按表隔离）、与 O4 正式列清单严格分开。

遗留（不构成拒绝）：草稿无删除/编辑（应用或作废前一直在）；SQL 预览是 O10、应用是 O11。
