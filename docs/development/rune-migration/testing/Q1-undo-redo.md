# TN Q1 编辑器撤销重做 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`9f73f2f` `feat(rune): undo and redo in the SQL editor (issue-rune Q1)`
- `git show --stat 9f73f2f` 两个文件：`crates/datazen-ui/src/sql_editor.rs`（`Snapshot` 快照、撤销/重做栈、`undo()`/`redo()`/`push_history()`、`insert`/`backspace` 入史、on_key 接 Command+Z / Command+Shift+Z / Command+Y、6 条单测）、`crates/datazen-ui/src/lib.rs`（运行时接线测试 1 条）。没有 `packages/frontend`、`packages/web`。React 未改
- 范围按任务限定：**只覆盖字符插入和删除**。IME 提交（commit）、多光标添加、选区变化不入历史；每步插入/删除为一个撤销步（不合并）

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 44 passed（B8 时 38，净增 6：sql_editor 5 条撤销重做单测 + `command_z_on_a_query_tab_undoes_and_redoes_typing`），2.40s；`datazen-ui-api` 16 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「编辑器撤销和重做，只覆盖字符插入和删除。测：输入再撤销，文档回到原来」核对，断言来自 GR1 实跑：

1. **输入再撤销回到原样**（`typing_then_undo_returns_the_document`）：连插 "X"、"Y" 后文档 "XYab"；一次撤销 "Xab"（一步一插）；再撤 "ab" 回原文；历史空后撤销返回 false。通过
2. **删除可撤销、重做恢复**（`delete_is_undoable_and_redo_restores_it`）：行尾退格删 "c" → "ab"；撤销 → "abc"；重做 → "ab"；重做栈空。通过
3. **新编辑清空重做栈**（`a_fresh_edit_clears_the_redo_stack`）：撤销后重做，再插入新字符——重做栈已清，文档为最新编辑结果。通过
4. **撤销连选区一起恢复**（`undo_restores_the_selection_with_the_document`）：撤销后主光标回到插入前位置。通过
5. **键路径**（`command_z_and_command_shift_z_go_through_on_key`、`command_z_on_a_query_tab_undoes_and_redoes_typing`）：on_key 单测覆盖 Command+Z 撤销、Shift+Command+Z 重做；运行时经 `app_key` 在查询标签上实按 Command+Z / Shift+Command+Z，文档先回原样再恢复 "Q" 开头。通过

开发中修正三处测试预期（插入发生在光标 0 之前、行首退格无操作），属测试笔误，产品逻辑未改。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。插入与删除入史（上限 100 步）、文档与选区一并恢复、新编辑清重做栈、Command+Z / Shift+Command+Z / Command+Y 三条键路径、运行时接线可用。

遗留（不构成拒绝）：

- 撤销步不合并不分组（连续输入逐字符撤销）；合并策略留给 C 系列体验打磨。
- IME 提交不入史（任务范围明确排除）；真机 IME 旅程在 H 系列验证。
- 历史栈深度 100 为常量；P1 设置落地后可考虑配置化。
