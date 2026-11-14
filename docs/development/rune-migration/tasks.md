# Rune 迁移任务

状态只有三种：未开始 / 进行中 / 已完成。只有 coder 提交、tester 报告、收尾提交都走完，才能写成已完成。

React 页面和组件不删不改。一个任务一个提交，不并行。

本 issue 的门禁：

| 编号 | 命令 | 通过标准 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib` | 全过 |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 退出码 0 |
| G1–G4 | 仓库根目录的 vitest / typecheck / 命名 / 全量前端测试 | 仅当该任务改了 `packages/frontend` 或 `packages/web` 时必跑。Rune 任务不改这些目录，写明不适用 |

| 任务 | 状态 | coder | tester 报告 |
|---|---|---|---|
| R0 | 已完成 | 骨架编译（流程建立前） | 无，流程建立前已验收 |
| R1 | 已完成 | SQL Editor POC | 无，流程建立前已验收 |
| R2 | 已完成 | DataGrid POC | 无，流程建立前已验收 |
| R3 | 已完成 | UI 端口和内存表页 | 无，流程建立前已验收 |
| S1 | 已完成 | 52e7bab | testing/S1-tabs.md |
| S2 | 已完成 | a02dd07 | testing/S2-object-tree.md |
| S3 | 已完成 | 63d75d2, 4fe37e5 | testing/S3-table-tab.md, testing/S3-table-tab-r2.md |
| S4 | 已完成 | e5f29ca | testing/S4-query-tab.md |
| S5 | 已完成 | 0f21ead | testing/S5-disconnect.md |
| S6 | 已完成 | 3b78739 | testing/S6-sidebar.md |
| B1 | 已完成 | 093b77a | testing/B1-table-paging.md |
| B2 | 已完成 | ea0d008 | testing/B2-grid-footer.md |
| B3 | 已完成 | 4c89e32 | testing/B3-column-sort.md |
| B4 | 已完成 | 0be8cc5 | testing/B4-column-filter.md |
| B5 | 已完成 | 815f525 | testing/B5-dirty-cells.md |
| B6 | 已完成 | 640d3dc | testing/B6-commit-rollback.md |
| B7 | 已完成 | bcfc0a8 | testing/B7-read-only.md |
| B8 | 已完成 | f4b5d2e | testing/B8-result-grid.md |
| Q1 | 已完成 | 9f73f2f | testing/Q1-undo-redo.md |
| Q2 | 已完成 | df65321 | testing/Q2-find-replace.md |
| Q3 | 已完成 | ec844ee | testing/Q3-statements.md |
| Q4 | 已完成 | c0048c6 | testing/Q4-query-api.md |
| Q5 | 已完成 | 9db816d | testing/Q5-generations.md |
| Q6 | 已完成 | fbfef17 | testing/Q6-cancel.md |
| Q7 | 已完成 | b204b3e | testing/Q7-run-wiring.md |
| Q8 | 进行中 | | |
| O1 | 未开始 | | |
| O2 | 未开始 | | |
| O3 | 未开始 | | |
| O4 | 未开始 | | |
| O5 | 未开始 | | |
| O6 | 未开始 | | |
| O7 | 未开始 | | |
| O8 | 未开始 | | |
| O9 | 未开始 | | |
| O10 | 未开始 | | |
| O11 | 未开始 | | |
| P1 | 未开始 | | |
| P2 | 未开始 | | |
| P3 | 未开始 | | |
| P4 | 未开始 | | |
| P5 | 未开始 | | |
| P6 | 未开始 | | |
| P7 | 未开始 | | |
| P8 | 未开始 | | |
| P9 | 未开始 | | |
| D1 | 未开始 | | |
| D2 | 未开始 | | |
| D3 | 未开始 | | |
| D4 | 未开始 | | |
| D5 | 未开始 | | |
| D6 | 未开始 | | |
| D7 | 未开始 | | |
| D8 | 未开始 | | |
| D9 | 未开始 | | |
| D10 | 未开始 | | |
| W1 | 未开始 | | |
| W2 | 未开始 | | |
| W3 | 未开始 | | |
| W4 | 未开始 | | |
| W5 | 未开始 | | |
| W6 | 未开始 | | |
| W7 | 未开始 | | |
| H1 | 未开始 | | |
| H2 | 未开始 | | |
| H3 | 未开始 | | |
| H4 | 未开始 | | |
| H5 | 未开始 | | |
| H6 | 未开始 | | |
| H7 | 未开始 | | |
| H8 | 未开始 | | |
| C1 | 未开始 | | |
| C2 | 未开始 | | |
| C3 | 未开始 | | |
| C4 | 未开始 | | |
