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
| Q8 | 已完成 | 457148d | testing/Q8-completion.md |
| O1 | 已完成 | 525d8d9 | testing/O1-catalog.md |
| O2 | 已完成 | e8eaf1e | testing/O2-tree-expand.md |
| O3 | 已完成 | 83cd143 | testing/O3-ddl-page.md |
| O4 | 已完成 | 9565120 | testing/O4-column-list.md |
| O5 | 已完成 | 86f124b | testing/O5-index-list.md |
| O6 | 已完成 | 5c0aa0e | testing/O6-foreign-keys.md |
| O7 | 已完成 | 8641bd5 | testing/O7-O8-er-canvas.md |
| O8 | 已完成 | 8641bd5 | testing/O7-O8-er-canvas.md |
| O9 | 已完成 | 39a0028 | testing/O9-column-drafts.md |
| O10 | 已完成 | 21ecd93 | testing/O10-draft-sql.md |
| O11 | 已完成 | 32b75bf | testing/O11-apply-drafts.md |
| P1 | 已完成 | 7c95aa3 | testing/P1-settings-port.md |
| P2 | 已完成 | 0221bf0 | testing/P2-theme.md |
| P3 | 已完成 | 53e75b0 | testing/P3-page-size.md |
| P4 | 已完成 | 35cbd1f | testing/P4-connection-form.md |
| P5 | 已完成 | 6756b9f | testing/P5-snippets.md |
| P6 | 已完成 | 230de1e | testing/P6-export-dialog.md |
| P7 | 已完成 | 9b248ef | testing/P7-progress.md |
| P8 | 已完成 | 77d86e6 | testing/P8-import-wizard.md |
| P9 | 已完成 | 3ecc608 | testing/P9-backup.md |
| D1 | 已完成 | b45490f | testing/D1-sync-endpoints.md |
| D2 | 已完成 | 59aff4a | testing/D2-sync-tables.md |
| D3 | 已完成 | 7625a19 | testing/D3-sync-options.md |
| D4 | 已完成 | d8d0992, 03aeaa8 | testing/D4-sync-preview.md |
| D5 | 已完成 | 5a7e176d0 | testing/D5-sync-run.md |
| D6 | 已完成 | 176b0cf | testing/D6-compare-objects.md |
| D7 | 已完成 | de44da5 | testing/D7-compare-plan.md |
| D8 | 已完成 | 860c4e0 | testing/D8-compare-apply.md |
| D9 | 已完成 | ca5ab44 | testing/D9-transfer-mapping.md |
| D10 | 已完成 | 1f0eecc | testing/D10-transfer-confirm.md |
| W1 | 已完成 | bb00522 | testing/W1-dashboard-list.md |
| W2 | 已完成 | cd80202 | testing/W2-bar-chart.md |
| W3 | 已完成 | 745b60c3b | testing/W3-widget-form.md |
| W4 | 进行中 | | |
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
