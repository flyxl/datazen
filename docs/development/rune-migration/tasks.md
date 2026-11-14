# Rune 迁移任务

状态只有三种：未开始 / 进行中 / 已完成。只有 coder 提交、tester 报告、收尾提交都走完，才能写成已完成。

React 页面和组件不删不改。一个任务一个提交，不并行。

第一阶段（S/B/Q/O/P/D/W/H/C）已全部完成。第二阶段（N 系列，原生宿主接线）的任务与门禁见 [plan-native.md](plan-native.md)。第三阶段（T 系列，宿主能力与分发）的任务与门禁见 [plan-native-3.md](plan-native-3.md)。

两个阶段共用的门禁：

| 编号 | 命令 | 通过标准 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib` | 全过 |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 退出码 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib` | 全过（N2 起适用） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 退出码 0（N2 起适用） |
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
| W4 | 已完成 | 6fc883c35 | testing/W4-workflow-list.md |
| W5 | 已完成 | a122f55a8 | testing/W5-workflow-form.md |
| W6 | 已完成 | 6a0a4c1c3 | testing/W6-workflow-yaml.md |
| W7 | 已完成 | bde466a56 | testing/W7-run-results.md |
| H1 | 已完成 | 46c44ddbe | testing/H1-desktop-process.md |
| H2 | 已完成 | 69b82c82 | testing/H2-adapter-types.md |
| H3 | 已完成 | 3a441166 | testing/H3-list-connections.md |
| H4 | 已完成 | 031c1e71 | testing/H4-read-table-page.md |
| H5 | 已完成 | 235574f7 | testing/H5-commit-cells.md |
| H6 | 已完成 | 66c65eae | testing/H6-wasm-build.md |
| H7 | 已完成 | 9786c4b3 | testing/H7-static-page.md |
| H8 | 已完成 | bfc5f4b5 | testing/H8-chinese-font.md |
| C1 | 已完成 | 163db778 | testing/C1-navigation-keys.md |
| C2 | 已完成 | 5178765 | testing/C2-grid-a11y.md |
| C3 | 已完成 | e38fe31 | testing/C3-mount-counts.md |
| C4 | 已完成 | 061ba45 | testing/C4-unmigrated-notice.md |
| N1 | 已完成 | 25ef637 | testing/N1-job-interface.md |
| N2 | 已完成 | 0737388 | testing/N2-host-crate.md |
| N3 | 已完成 | cb72a28 | testing/N3-live-sqlite-session.md |
| N4 | 已完成 | a1cdadd | testing/N4-real-reads.md |
| N5 | 已完成 | 65f4b49 | testing/N5-runtime-assembly.md |
| N6 | 已完成 | 104a277 | testing/N6-real-commits.md |
| N7 | 已完成 | 0b24e12 | testing/N7-real-queries.md |
| N8 | 已完成 | f0babc4 | testing/N8-settings-persistence.md |
| N9 | 已完成 | f567955 | testing/N9-query-history.md |
| N10 | 已完成 | 48e8713 | testing/N10-structure-pages.md |
| N11 | 已完成 | 05c7f95 | testing/N11-export-backup.md |
| N12 | 已完成 | 54ca210 | testing/N12-sync-compare.md |
| N13 | 已完成 | a4644d1 | testing/N13-workflow-dashboard.md |
| N14 | 已完成 | cda8ca3 | testing/N14-release-docs.md |
| T1 | 已完成 | b9c7745 | testing/T1-clipboard.md |
| T2 | 已完成 | 5f7ac65 | testing/T2-file-dialogs.md |
| T3 | 已完成 | eeb1333 | testing/T3-snippets.md |
| T4 | 未开始 | | |
| T5 | 未开始 | | |
| T6 | 未开始 | | |
| T7 | 未开始 | | |
| T8 | 未开始 | | |
| T9 | 未开始 | | |
| T10 | 未开始 | | |
| T11 | 未开始 | | |
