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
按 CodeMirror 6 的分层，而不是把文档放进 `text_input`。

- 文档是 `String`。选区是多个 `Sel`（anchor/head），其中一个是 main。
- 普通按键走 `AppSpec.on_key`，对每个选区做 `changeByRange`。
- IME 走 `App::on_ime`。preedit 只画在主光标上，不进文档；commit 才替换主选区，其它光标按这个变更映射。
- 视图是不换行的行 + 高亮 `Span` + 光标条。主光标是 `El::ime_cursor`，用来打开系统输入法。长行放在比视口更宽的 `scroll_xy` 里。

`cargo test -p datazen-ui --lib`：10 passed。未跑 Tester。

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
