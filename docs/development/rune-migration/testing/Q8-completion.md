# TN Q8 画布内补全 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`457148d` `feat(rune): an in-canvas keyword completion list (issue-rune Q8)`
- `git show --stat 457148d` 两个文件：`crates/datazen-ui/src/sql_editor.rs`（`Completion` 状态、`refresh_completion`/`completion_down/up/close/accept(_index)`、on_key 接 Down/Up/Enter/Escape、`completion_list` 画布渲染、`word_start`/`starts_with_ignore_ascii_case` 助手、4 条单测）、`crates/datazen-ui/src/lib.rs`（1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改
- 行为：输入时按光标前的词刷新候选（大小写不敏感前缀匹配，`COMPLETION_LIMIT=8`）；列表画在编辑器画布里光标行下方（`sql-completion`，非系统弹层）；Down/Up 循环（环绕）、Enter 插入（覆盖前缀、入撤销史）、Escape 只关列表、退格与继续输入重新过滤、无匹配关闭；列表项可点击

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 64 passed（Q7 时 59，净增 5：sql_editor 4 条补全单测 + `the_completion_list_is_drawn_in_the_canvas`），2.61s；`datazen-ui-api` 19 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「画布内的关键字补全列表，最多 8 条，没有系统弹层。测：输入 `SEL` 出现 `SELECT`，回车插入」核对，断言来自 GR1 实跑：

1. **输入 SEL 出现 SELECT，回车插入**（`typing_sel_offers_select_and_enter_inserts_it` 与运行时 `the_completion_list_is_drawn_in_the_canvas`）：逐键输入 S/E/L → 候选只有 `["SELECT"]`；Enter → 文档变 "SELECT"、光标在词尾、列表关闭；撤销回到 "SEL"。运行时断言 `sql-completion`/`sql-completion-item-0` 在画布内呈现、插入后消失。通过
2. **方向键循环与 Escape**（`arrow_keys_cycle_the_completion_and_escape_closes`）："S" → `["SELECT", "SET", "SUM"]`；Down 1→2→0（环绕）；Up 回绕到 2；Escape 只关列表，文档仍是 "S"。通过
3. **输入收窄、退格放宽、无匹配关闭**（`typing_narrows_the_completion_and_no_match_closes_it`）："SE" → 2 条；退格回 "S" → 3 条；"Q" → "SQ" 无匹配关闭，此时 Enter 是普通换行。通过
4. **大小写不敏感**（`completion_matches_ignore_case`）：小写 "sel" 同样给出并插入 "SELECT"。通过
5. **最多 8 条**：`COMPLETION_LIMIT = 8` 以 `.take(8)` 截断（代码核对；当前关键字表中无前缀超过 3 条命中，截断路径由常量与实现保证）

既有编辑器测试（输入、撤销、查找替换、IME、多光标）全部复跑通过——补全只在 on_key 路径介入，模型方法不受影响；seed 文档的既有测试（如输入 "select 1" 的 Q7 测试）中途会开列表但空格/数字即关闭，不改变最终文档。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。画布内关键字补全齐备：前缀过滤、8 条上限、方向键循环、回车插入（可撤销）、Escape 关闭、点击接受、无系统弹层。

遗留（不构成拒绝）：

- 候选只有 SQL 关键字；表名/列名补全（来自会话对象树）留给 O 系列接线后。
- 列表画在光标行下方（占行高，不悬浮）；悬浮定位与 C 系列视觉打磨合并。
- 补全未接入 Tab 键（Tab 目前不是编辑器键）；留给键盘习惯任务。
