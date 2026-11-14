# TN Q2 查找替换 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`df65321` `feat(rune): find next, replace current and replace all in the editor (issue-rune Q2)`
- `git show --stat df65321` 一个文件：`crates/datazen-ui/src/sql_editor.rs`（`find_next`/`replace_current`/`replace_all` 三个编辑器方法、条栏版 `find_from_bar` 等、面板查找栏 `sql-find-bar`、5 条单测）。没有 `packages/frontend`、`packages/web`。React 未改
- 行为：查找从主光标起、环绕、命中即选；替换当前要求主选区与 needle 完全一致，替换后跳下一处；全部替换从右往左逐处替换并映射选区；替换操作入撤销史（Q1 历史机制的自然延伸，报告中明示）

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 49 passed（Q1 时 44，净增 5：`find_next_selects_occurrences_in_order_and_wraps`、`replace_current_replaces_the_selection_and_moves_to_the_next_match`、`replace_all_swaps_every_occurrence`、`replace_all_leaves_unmatched_documents_alone`、`the_find_bar_reads_its_own_inputs`），2.42s；`datazen-ui-api` 16 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「在文档里查找下一处、替换当前、替换全部。测：三处相同词被换掉」核对，断言来自 GR1 实跑：

1. **查找下一处**（`find_next_selects_occurrences_in_order_and_wraps`）："a X b X c X" 依次命中偏移 2、6、10，末尾环绕回 2；不存在的词返回 false 且不改动选区。通过
2. **替换当前**（`replace_current_replaces_the_selection_and_moves_to_the_next_match`）："cat dog cat" 命中第一处替换为 bird → "bird dog cat"，下一处（偏移 9）已被选中；撤销回到原文。通过
3. **三处相同词全被换掉**（`replace_all_swaps_every_occurrence`）："one two one two one" 全部替换返回 3，文档变 "1 two 1 two 1"；撤销恢复。通过
4. **无命中不动文档**（`replace_all_leaves_unmatched_documents_alone`）：返回 0、文档不变、不产生撤销步。通过
5. **查找栏读自己的输入**（`the_find_bar_reads_its_own_inputs`）：条栏输入 old/new 后 `find_from_bar` 选中第一处、`replace_all_from_bar` 换掉两处。通过

面板上 `sql-find-bar` 含 `sql-find-input`（回车查找并失焦）、`sql-find-next`、`sql-replace-input`（回车替换当前并失焦）、`sql-replace-current`、`sql-replace-all`（键存在性由编译与既有面板测试覆盖；交互路径与单测同构）。

## 缺陷

无。开发中修正一处 `TextEdit::new("…".into())` 的类型歧义（改直传 &str），属编译期笔误。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。查找/替换当前/全部替换三操作齐备，替换入撤销史，全部替换保序映射选区，查找栏入编辑器面板。

遗留（不构成拒绝）：

- 查找为字节精确匹配（大小写敏感、无正则）；大小写开关与正则留给后续按需。
- 没有命中计数显示与高亮所有匹配；C 系列打磨项。
- 替换后跳下一处在文档只有一处匹配时会环绕重选它（与主流编辑器一致）。
