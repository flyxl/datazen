# TN B7 只读原因 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`bcfc0a8` `feat(rune): carry a read-only reason on the page (issue-rune B7)`
- `git show --stat bcfc0a8` 三个文件：`crates/datazen-ui-api/src/lib.rs`（TablePage/StoredTable 加 `read_only`、orders 演示表只读原因为 "no primary key"、stage_cell 与 commit_staged 拒绝只读）、`crates/datazen-ui/src/data_grid.rs`（网格记录只读原因、`grid-readonly` 横幅）、`crates/datazen-ui/src/lib.rs`（show_table/open_connection 按 `read_only.is_none()` 设置可编辑）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 37 passed（B6 时 36，净增 1：`a_read_only_table_blocks_editing_and_shows_its_reason`），2.25s；`datazen-ui-api` 15 passed（B6 时 14，净增 1：`a_read_only_table_rejects_staging_and_commit`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「页上带只读原因。有原因时不能编辑。测：只读页提交被拒绝」核对，断言来自 GR1 实跑：

1. **只读页拒绝暂存与提交**（`a_read_only_table_rejects_staging_and_commit`）：orders 页 `read_only == Some("no primary key")`；`stage_cell` 返回 Err；`commit_staged` 返回 Err；换回 customers 后暂存并提交成功（可写表不受影响）。通过
2. **只读页不能编辑且原因在页上**（`a_read_only_table_blocks_editing_and_shows_its_reason`）：开 orders 标签后 `is_editable()==false`、`begin_edit` 进不了编辑、`grid-readonly` 横幅呈现、端口暂存被拒；切回 customers 后可编辑恢复、横幅消失。通过
3. **断开仍封锁**（既有 `disconnect_clears_the_tree_and_blocks_editing` 复跑通过）

开发中的两处修正：datazen-ui-api 是 edition 2021 不能用 let 链（改嵌套 if）；直接改模型切标签后横幅的退场需要 `settle_all()` 再 `frame` 才呈现（B 系列既有经验，测试已按此写）。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。只读原因随页携带、页面横幅展示、编辑器封锁、端口暂存/提交拒绝；可写表路径无回归。

遗留（不构成拒绝）：

- 只读原因是表级静态属性（演示数据写死 "no primary key"）；真实驱动的只读判定（权限、连接只读模式）在 D/H 系列接入。
- 只读页的页脚 Commit/Rollback 按钮仍渲染（提交会被端口拒绝）；禁用态与 C 系列打磨合并。
