# TN D2 同步选表 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`59aff4a` `feat(rune): the sync table step with multi-select (issue-rune D2)`
- `git show --stat 59aff4a` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`connection_tables` 端口：不开会话列出连接可见表，replica 仍无 analytics）、`crates/datazen-ui/src/lib.rs`（同步向导第 2 步：表多选芯片、Select all/Clear/Next + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 85 passed（净增 1：`the_sync_table_step_selects_all_and_clears`），4.72s；`datazen-ui-api` 35 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「同步：选表，多选。测：全选和清空」核对，断言来自 GR1 实跑：

1. **全选和清空**（`the_sync_table_step_selects_all_and_clears`）：第 2 步列出源（local）的 3 张表；Select all → 3 张全选；Clear → 清空；单点 `sync-table-customers` 芯片切换一张；有选择点 Next → 进入第 3 步；清空后 Next → 停留并报错（无选择不能前进，衔接 D5 的运行门槛）。通过

## 缺陷

无。测试时机修正（直接改模型后 `settle_all` 再 `frame`，B 系列已知规则）。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。表多选、全选/清空、无选择不前进。

遗留（不构成拒绝）：无表级方向（push/pull 双向）；选项步是 D3。
