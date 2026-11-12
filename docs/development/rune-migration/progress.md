## Current Task
- **任务**: DataGrid POC（虚拟行、列窗口、单元格编辑、复制粘贴）
- **负责人**: Coder Agent
- **状态**: POC 已接到 Data Browser，并通过 `cargo test -p datazen-ui --lib`。未跑 Tester。
- **开始时间**: 2026-11-12

## Completed Work
- [x] 创建独立开发分支
- [x] 编写并提交迁移方案
- [x] 创建 Rune 初始 crate 骨架
- [x] 扩展 shared UI shell
- [x] Vendor Rune 到 `vendor/rune`（submodule `2e4999f`）
- [x] `[workspace.package] edition = "2024"`，`cargo check -p datazen-ui` 通过
- [x] SQL Editor POC（`crates/datazen-ui/src/sql_editor.rs`）
- [x] DataGrid POC（`crates/datazen-ui/src/data_grid.rs`）

## DataGrid POC
侧栏 Data Browser。2 万行 × 200 列，格子文本是生成的，改过的值存在网格状态里。

- 行：`virtual_rows`。视口外的行不会建出来（`grid-row-5000` 不在树上）。
- 列：Rune 不按列卸载。每行只建当前窗口的 12 列，再加冻结的第 0 列。窗口跟着横向滚动偏移走。
- 编辑：Enter 打开 `text_input`，提交写回 `(row, col)`。滚走的行可以被拆掉，值还在。
- 复制粘贴：选区是矩形。`⌘/Ctrl+C` 写成 TSV，粘贴从焦点格铺开。

还没接数据库、分页、外键、列宽和结果写回。

## SQL Editor POC
按 CodeMirror 6 的分层，而不是把文档放进 `text_input`。

- 文档是 `String`。选区是多个 `Sel`（anchor/head），其中一个是 main。
- 普通按键走 `AppSpec.on_key`，对每个选区做 `changeByRange`。
- IME 走 `App::on_ime`。preedit 只画在主光标上，不进文档；commit 才替换主选区，其它光标按这个变更映射。
- 视图是不换行的行 + 高亮 `Span` + 光标条。主光标是 `El::ime_cursor`，用来打开系统输入法。长行放在比视口更宽的 `scroll_xy` 里。

## Verification
```
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 \
RAYON_NUM_THREADS=1 RUSTFLAGS="-C debuginfo=0" \
cargo test -p datazen-ui --lib
```
结果：16 passed。链接测试需要 `libxkbcommon.so`（本机只有 `.so.0` 时做了符号链接，不入库）。

## Next Steps
1. 把网格接到现有表数据端口（分页行、列定义、脏写回），仍然不要搬整个 React 界面。
2. 更高内存机器上复跑默认并行的 `cargo test -p datazen-ui`。
