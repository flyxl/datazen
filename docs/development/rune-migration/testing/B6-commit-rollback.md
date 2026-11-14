# TN B6 提交与撤销 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`640d3dc` `feat(rune): stage, commit and rollback cell edits (issue-rune B6)`
- `git show --stat 640d3dc` 三个文件：`crates/datazen-ui-api/src/lib.rs`（StoredTable 加 `staged`、stage_cell 改暂存、open_table 覆盖暂存值、`commit_staged`/`rollback_staged` 端口、disconnect 清暂存）、`crates/datazen-ui/src/data_grid.rs`（页脚 Commit/Rollback 按钮）、`crates/datazen-ui/src/lib.rs`（commit/rollback_staged_edits 重载页）。没有 `packages/frontend`、`packages/web`。React 未改
- 语义：编辑先暂存（页面上可见、随行穿越排序过滤）；提交落到存储行并报数；撤销丢弃暂存；断开连接清暂存

## 门禁

环境：`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RUSTFLAGS="-C debuginfo=0"`，rustc 1.99.1。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 36 passed（B5 时 35，净增 1：`commit_and_rollback_buttons_drive_the_staged_edits`），2.13s；`datazen-ui-api` 14 passed（B5 时 11，净增 3：`rollback_restores_the_original_value`、`commit_keeps_the_new_value_after_reopening`、`staged_values_follow_their_row_through_sorting`） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

## 验证

按 tasks.md 验收标准「端口增加提交和撤销。两个按钮。提交后重开表仍是新值；撤销回到原值」核对，断言来自 GR1 实跑：

1. **撤销回原值**（`rollback_restores_the_original_value`）：暂存后重开页显示 "staged"（暂存可见）；`rollback_staged()` 后重开回 "customer-0"。通过
2. **提交后重开仍是新值**（`commit_keeps_the_new_value_after_reopening`）：`commit_staged()` 返回 1；重开页 "kept"；此后再撤销也回不去（提交不可逆）。通过
3. **暂存随行穿越排序**（`staged_values_follow_their_row_through_sorting`）：行 0 的 name 暂存为 "aaa-first"，按 name 升序重开——该行带暂存值排到第 1 位。通过
4. **两个按钮**（`commit_and_rollback_buttons_drive_the_staged_edits`）：页脚有 `commit-staged` 与 `rollback-staged`；经应用路径（select→begin→输入→commit_grid_edit）编辑后脏数 1；点 Rollback → 格子回 "customer-0"、脏数 0；再编辑后点 Commit → 格子 "renamed"，经 `open_table` 重开表第 0 行 name 仍是 "renamed"。通过

既有 B1/B3 测试（`connect_open_and_stage_round_trip`、`staging_addresses_the_row_behind_the_open_page`、`opening_a_connection_loads_the_table_page_into_the_grid`）在暂存语义下复跑通过：暂存值经 open_table 覆盖可见，合同未破。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。端口有 commit/rollback；页脚两个按钮驱动；提交持久、撤销回原值；暂存值在页面上可见且随行穿越排序过滤；断开连接清暂存。

遗留（不构成拒绝）：

- commit_staged 是工作区级（提交所有表的全部暂存），不区分当前表；真实驱动接入后按连接粒度收窄。
- 暂存数没有独立于脏数的显示（B5 合同：状态行只数脏格）；提交/撤销按钮没有禁用态，空暂存时点击为无害操作。
