# TN P5 SQL 片段列表 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`6756b9f` `feat(rune): a SQL snippet list with add, update and delete (issue-rune P5)`
- `git show --stat 6756b9f` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`Snippet`、`snippets`/`save_snippet`/`update_snippet`/`delete_snippet` 端口 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（设置页片段区：输入 + Save/Update、逐行 Delete、错误行 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 31 passed（净增 1：`snippets_add_update_delete_and_refuse_duplicates`）；`datazen-ui` 79 passed（净增 1：`snippets_add_update_delete_and_duplicate_names_are_refused`），4.60s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「SQL 片段列表：增、改、删。测：重名被拒绝」核对，断言来自 GR1 实跑：

1. **增改删与重名拒绝**（api + 运行时）：保存 "top" → 1 条入列；再存同名 → Err 且原 SQL 不动、界面 `snippet-error` 呈现、行数仍 1；Update 同名 → SQL 改为 "SELECT 1"；Delete → 行消失、端口清空。空名与未知名的增改删均拒绝。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。片段经端口增改删，重名拒绝，界面与端口一致。

遗留（不构成拒绝）：片段未接入编辑器（插入到光标处）；无导入导出。均为后续任务范围。
