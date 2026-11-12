## Current Task
- **任务**: SQL Editor POC（多光标、中文输入、语法高亮、长行水平滚动）
- **负责人**: Coder Agent
- **状态**: POC 已实现并通过 `cargo test -p datazen-ui --lib`（10 passed）。本次按要求未跑 Tester。
- **开始时间**: 2026-11-11

## Completed Work
- [x] 创建独立开发分支
- [x] 编写并提交迁移方案
- [x] 创建 Rune 初始 crate 骨架
- [x] 扩展 shared UI shell
- [x] Vendor Rune 到 `vendor/rune`（submodule `2e4999f`）
- [x] `[workspace.package] edition = "2024"`，`cargo check -p datazen-ui` 通过
- [x] SQL Editor POC（`crates/datazen-ui/src/sql_editor.rs`）
- [ ] DataGrid POC

## SQL Editor POC
打开侧栏 SQL Editor。字段绑定 Rune `TextEdit`（主光标、撤销、IME）。

| 门禁 | 做法 | 测试 |
|---|---|---|
| 语法高亮 | 自写 SQL lexer，画成 `Span`（关键字 / 字符串 / 注释 / 数字） | `highlights_keywords_strings_comments_and_numbers` |
| 多光标 | `TextEdit` 只有一个 caret。额外 caret 存在 `SqlEditor::extra`。有额外光标时按键在每一处插入同一段文本 | `multi_cursor_inserts_the_same_text_at_each_caret` |
| 中文输入 | 主光标走 Rune IME：preedit 不进 buffer，commit 才写入 | `chinese_ime_commits_on_the_primary_caret_without_entering_the_buffer_early` |
| 长行水平滚动 | 多行字段宽 2800，放在 `scroll_xy` 里。Rune 的多行 `text_input` 不能关 wrap，所以用比视口更宽的编辑面代替 nowrap | `sql_editor_scrolls_a_surface_wider_than_the_viewport` |

IME 只作用于主光标。额外光标重复的是已经提交的按键文本，不是组合态。这不是 CodeMirror 等价实现。

## Verification
```
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 \
RAYON_NUM_THREADS=1 RUSTFLAGS="-C debuginfo=0" \
cargo test -p datazen-ui --lib
```
结果：10 passed。链接测试需要 `libxkbcommon.so`（本机只有 `.so.0` 时做了符号链接，不入库）。

## Next Steps
1. DataGrid POC（虚拟滚动、单元格编辑、复制粘贴）。
2. 更高内存机器上复跑默认并行的 `cargo test -p datazen-ui`。
