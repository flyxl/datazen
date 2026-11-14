# TN O2 树展开状态 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`e8eaf1e` `feat(rune): a three-level object tree with kept collapse state (issue-rune O2)`
- `git show --stat e8eaf1e` 一个文件：`crates/datazen-ui/src/lib.rs`（`collapsed: BTreeSet<String>`、`toggle_expand`/`refresh_catalog`、`object_tree` 三层渲染（`tree-db-*`/`tree-schema-*`/`tree-table-*`）、侧栏 `tree-refresh` 按钮、2 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 66 passed（O1 时 64，净增 2：`the_tree_collapses_and_a_refresh_keeps_it_collapsed`、`collapsing_the_database_hides_the_whole_tree`），3.18s；`datazen-ui-api` 20 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「树的展开状态。刷新名字时保留展开。测：收起后再取树，仍是收起」核对，断言来自 GR1 实跑：

1. **收起后刷新仍是收起**（`the_tree_collapses_and_a_refresh_keeps_it_collapsed`）：连接后三层齐现（db/schema×2/表×3）；点 `tree-schema-local-public` 收起——public 两表消失、analytics 照开；点 `tree-refresh` 重取目录——public 仍收起；再点 schema 行——表回来。通过
2. **库级折叠**（`collapsing_the_database_hides_the_whole_tree`）：点 `tree-db-local` 整树收起只剩库行；再点恢复。通过

既有树相关测试（`clicking_a_session_table_opens_a_table_tab`、`the_open_table_tab_is_the_table_in_the_grid`、`disconnect_clears_the_tree_and_blocks_editing`）复跑通过——默认全展开保持 S2 行为，表行键 `tree-table-{name}` 不变，断开清空折叠集。

## 缺陷

无。开发中修一处闭包移动后的借用（先读折叠标志再进闭包），编译期问题未入库。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。三层树 UI（库/schema/表，缩进，默认展开）；折叠是 UI 状态，刷新目录、重渲染都不丢；连接与断开重置。

遗留（不构成拒绝）：

- 折叠以路径字符串（"db"、"db/schema"）为键；同名 schema 在不同库下各自独立（键含库名）。
- 树没有图标与表行数徽标；视觉打磨在 C 系列。
