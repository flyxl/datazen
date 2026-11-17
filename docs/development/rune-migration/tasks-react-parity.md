# React → Rune 逐文件迁移台账

本文件是执行台账，不是设计文档：任务由 `scripts/rune-parity.mjs` 从
`packages/frontend/src` 现况生成，改代码后重新生成即可，不要手改表。

## 1. 三条硬约束怎么落地

用户要求：① 每个 task ≤ 1 天；② 逐文件迁移；③ 迁移期允许短暂编译失败，但不能长期编译不过。

| 约束 | 机制 | 证据 |
| --- | --- | --- |
| ≤ 1 天 | 台账按工作量估算贪心装桶，单桶 ≤ 1.0 天；超过 1 天的单文件单独成 task 并标 `split`，按子组件再拆 | `node scripts/rune-parity.mjs` 的 252 个桶 |
| 逐文件 | 一 task 对应一到三个 React 文件，Rune 侧落在同一路径（`Foo.tsx` → `foo.rs`），提交信息里逐条列出 | 表内 `react file` 列 |
| 不长红 | 新文件在挂 `mod` 之前不参与编译；每 task 收尾跑 `scripts/rune-migrate-gate.sh`，红则当场修或整 task revert | 门禁 2.4s（check）/ 6.5s（tests），全环含构建截图约 16s |

编译红灯窗口的边界：

- 工作区允许红，**提交必须绿**。一个 task 的红窗上限是它的耗时（≤1 天），实测门禁 9 秒内出结果，所以实际操作中红窗是分钟级。
- 新页面先写文件、后挂 `mod`。`crates/datazen-ui/src/views/mod.rs` 与 `crates/datazen-ui/src/app.rs` 的 `Section` 分发是唯一开关点，切换提交里同时删除旧实现。
- 同时最多一个 task 处于「文件已写、未过门禁」状态；第二个 task 开工前必须把第一个推过门禁或 revert。

## 2. 台账口径

- 统计口径：`packages/frontend/src` 下非测试 `.tsx`，310 个文件 / 70,059 行。
- 分类：`P` 纯展示（可 1:1 直译）、`C` 连接型（文件 1:1，hook 重接 `AppState`）、`H` 依赖第三方引擎（CodeMirror / xyflow / recharts / xlsx / dagre / marked / react-virtual，实为同路径重写）、`X` 纯 re-export 壳（迁移后消失）。
- 工作量估算：P 0.3–0.8 天（按行数）、C 0.9–1.3 天、H max(2.0, 行数/250) 天、X 0 天。H 的天数只算「引擎之上的接线」，引擎本身在 §3 单独立项。
- 单轨总计 **252 tasks / 259.4 工作日**。

## 3. Phase 0：基座（不属于台账，必须先做）

台账假定设计系统已经存在。Rune 侧现在只有 `crates/datazen-ui/src/ui/{mod,theme,nav,field}.rs` 共 536 行，
而 React 页面通过 `packages/frontend/src/components/ui/*` 这层薄壳调用 `@datazen/ui` 的组件，
实测被引用次数：Button 92、Spinner 39、CopyableError 28、Dialog 27、Select 25、Input 22、ErrorBanner 10、
Checkbox 10、useCopyFeedback 7、Badge 6、ToolbarButton 5、ResultMessageDialog 2、ToolbarShell 2、Tabs 2、
LimitationsDialog 2、WebContextMenuHost 1、ConfirmDialog 1、Slider 1、Radio 1。

所以先补这 22 个基座 task（每个 ≤1 天），完成后再进 §4：

| # | task | 落点 | 验收 |
| --: | --- | --- | --- |
| T0-01 | Button（primary/secondary/ghost/danger × size × icon × disabled × loading） | `crates/datazen-ui/src/ui/button.rs` | 6 种组合的矩形与文字基线断言 + 点击回调 |
| T0-02 | Spinner（小/中，tone，可 label） | `crates/datazen-ui/src/ui/spinner.rs` | 动画帧不改变占位矩形 |
| T0-03 | 图标集对齐 lucide 实际用例 | `crates/datazen-ui/src/icons.rs` | 缺失清单清零，未用图标删除 |
| T0-04 | CopyableError + 复制反馈定时 | `crates/datazen-ui/src/ui/copyable.rs` | 复制后 1.5s 复位 |
| T0-05 | Dialog 基座（scrim/标题/关闭/页脚槽/Esc/焦点归还） | `crates/datazen-ui/src/ui/dialog.rs` | 960×640 内完整可见，Esc 关闭 |
| T0-06 | Select（popup list、键盘上下、labels、无匹配） | `crates/datazen-ui/src/ui/select.rs` | 键盘选中与鼠标一致 |
| T0-07 | Input（label/hint/error 态包 TextField） | `crates/datazen-ui/src/ui/input.rs` | 错误态高度不跳变 |
| T0-08 | ErrorBanner（variant） | `crates/datazen-ui/src/ui/error_banner.rs` | 长文本换行不越界 |
| T0-09 | Checkbox + Radio | `crates/datazen-ui/src/ui/toggle.rs` | 焦点环与点击命中区 |
| T0-10 | Badge + Divider + Label | `crates/datazen-ui/src/ui/badge.rs` | 主题色取自 `tok::theme` |
| T0-11 | ToolbarShell + ToolbarButton | `crates/datazen-ui/src/ui/toolbar.rs` | 窄窗换行 |
| T0-12 | Tabs（受控 tab + 渲染回调） | `crates/datazen-ui/src/ui/tabs.rs` | 选中态与键位 |
| T0-13 | ConfirmDialog + ResultMessageDialog + LimitationsDialog | `crates/datazen-ui/src/ui/confirm.rs` | 三者共用 Dialog 基座 |
| T0-14 | Slider | `crates/datazen-ui/src/ui/slider.rs` | 拖拽到两端取值 |
| T0-15 | WebContextMenuHost（右键菜单宿主） | `crates/datazen-ui/src/ui/context_menu.rs` | 视口边缘翻转 |
| T0-16 | i18n：zh-CN 表 → `t()` 运行时（2,940 处 key / 2,222 唯一） | `crates/datazen-ui/src/i18n.rs` | 缺键回显 key 且不 panic |
| T0-17 | PathInput（路径补全 + PathPicker 回调） | `crates/datazen-ui/src/ui/path_input.rs` | 6 个调用点的键位 |
| T0-18 | TemporalValueInput | `crates/datazen-ui/src/ui/temporal.rs` | 日期/时间/日期时间三型 |
| T0-19 | JsonViewer + BinaryViewer（hex 行） | `crates/datazen-ui/src/ui/viewers.rs` | 大 payload 不爆栈 |
| T0-20 | VirtualTree + 树行算术（TREE_TOP_LEVEL/effectiveExpanded/searchedRowLevels/ariaLevelOf/indentOf） | `crates/datazen-ui/src/ui/tree.rs` | 1 万行滚动帧率与展开状态 |
| T0-21 | `cn()` 审计（257 处）确认在 Rune 侧无需对应物 | 文档 + `ui/theme.rs` | 逐处给出等价 token 或删除理由 |
| T0-22 | 基座回归：每组件 960×640 + 1440×816 几何断言 | `crates/datazen-ui/src/tests/components.rs` | 越界节点为 0 |

Phase 0 合计约 22 个工作日。它做完之前不碰 §4 的台账任务，否则每个页面都会手搓一遍组件。

## 4. 逐文件任务台账（252 个 task，自动生成）

生成命令：`node scripts/rune-parity.mjs --md docs/development/rune-migration/tasks-react-parity.md.tmp`。
下表为 2026-11-17 的快照。

| # | area | days | react file | loc | class | hooks | keys |
| --: | --- | --: | --- | --: | --- | --: | --: |
| 1 | components/ui | 0.90 | `components/ui/Dialog.tsx` | 10 | P | 1 | undefined |
| 1 | components/ui | 0.90 | `components/ui/ErrorBanner.tsx` | 7 | P | 0 | undefined |
| 1 | components/ui | 0.90 | `components/ui/Select.tsx` | 16 | P | 1 | undefined |
| 2 | components/ui | 0.30 | `components/ui/Spinner.tsx` | 8 | P | 0 | undefined |
| 3 | components/ui | 0.90 | `components/ui/WebContextMenu.tsx` | 242 | C | 7 | undefined |
| 4 | components/DataTable | 0.80 | `components/DataTable/CellRenderer.tsx` | 146 | P | 2 | undefined |
| 4 | components/DataTable | 0.80 | `components/DataTable/ColumnStatsDialog.tsx` | 104 | P | 1 | undefined |
| 5 | components/DataTable | 1.30 split | `components/DataTable/DataExportDialog.tsx` | 452 | C | 18 | undefined |
| 6 | components/DataTable | 4.00 split | `components/DataTable/DataTable.tsx` | 1008 | H | 24 | undefined |
| 7 | components/DataTable | 0.90 | `components/DataTable/DataTableSelectionBar.tsx` | 122 | C | 0 | undefined |
| 8 | components/DataTable | 0.30 | `components/DataTable/DefaultOrderSelect.tsx` | 75 | P | 1 | undefined |
| 9 | components/DataTable | 0.80 | `components/DataTable/DetailPanel.tsx` | 317 | P | 10 | undefined |
| 10 | components/DataTable | 0.80 | `components/DataTable/DetailPanelToggle.tsx` | 29 | P | 1 | undefined |
| 10 | components/DataTable | 0.80 | `components/DataTable/DraftRow.tsx` | 157 | P | 1 | undefined |
| 11 | components/DataTable | 0.90 | `components/DataTable/EditableCell.tsx` | 61 | P | 0 | undefined |
| 11 | components/DataTable | 0.90 | `components/DataTable/editors/BinaryCellEditor.tsx` | 29 | P | 1 | undefined |
| 11 | components/DataTable | 0.90 | `components/DataTable/editors/BooleanCellEditor.tsx` | 77 | P | 0 | undefined |
| 12 | components/DataTable | 0.80 | `components/DataTable/editors/CellEditorHost.tsx` | 443 | P | 21 | undefined |
| 13 | components/DataTable | 0.90 | `components/DataTable/editors/EnumCellEditor.tsx` | 108 | P | 1 | undefined |
| 13 | components/DataTable | 0.90 | `components/DataTable/editors/JsonCellEditor.tsx` | 86 | P | 1 | undefined |
| 13 | components/DataTable | 0.90 | `components/DataTable/editors/LongTextCellEditor.tsx` | 46 | P | 0 | undefined |
| 14 | components/DataTable | 0.90 | `components/DataTable/editors/NumberCellEditor.tsx` | 69 | P | 1 | undefined |
| 14 | components/DataTable | 0.90 | `components/DataTable/editors/TemporalCellEditor.tsx` | 59 | P | 1 | undefined |
| 14 | components/DataTable | 0.90 | `components/DataTable/editors/TextCellEditor.tsx` | 62 | P | 1 | undefined |
| 15 | components/DataTable | 0.80 | `components/DataTable/GridCell.tsx` | 354 | P | 1 | undefined |
| 16 | components/DataTable | 0.30 | `components/DataTable/GridKeyboardHints.tsx` | 28 | P | 1 | undefined |
| 17 | components/DataTable | 0.80 | `components/DataTable/Pagination.tsx` | 391 | P | 2 | undefined |
| 18 | components/DataTable | 0.90 | `components/DataTable/QuickLookDialog.tsx` | 235 | C | 5 | undefined |
| 19 | components/DataTable | 0.80 | `components/DataTable/TableHeader.tsx` | 342 | P | 1 | undefined |
| 20 | components/DataTable | 3.10 split | `components/DataTable/VirtualBody.tsx` | 778 | H | 13 | undefined |
| 21 | components/filter | 0.60 | `components/filter/FilterBetweenInput.tsx` | 92 | P | 0 | undefined |
| 21 | components/filter | 0.60 | `components/filter/FilterConditionChip.tsx` | 54 | P | 0 | undefined |
| 22 | components/filter | 0.90 | `components/filter/FilterConditionEditor.tsx` | 245 | C | 4 | undefined |
| 23 | components/filter | 0.30 | `components/filter/FilterConnectorToggle.tsx` | 83 | P | 0 | undefined |
| 24 | components/filter | 0.90 | `components/filter/FilterSqlPreviewDialog.tsx` | 123 | C | 2 | undefined |
| 25 | components/filter | 0.80 | `components/filter/FilterTokenInput.tsx` | 124 | P | 3 | undefined |
| 25 | components/filter | 0.80 | `components/filter/FilterValueInput.tsx` | 106 | P | 5 | undefined |
| 26 | components/filter | 0.80 | `components/filter/NamedViewMenu.tsx` | 635 | P | 20 | undefined |
| 27 | components/schema | 0.50 | `components/schema/SchemaDiffPanel.tsx` | 152 | P | 1 | undefined |
| 28 | components/history | 1.30 split | `components/history/GlobalQueryHistoryDialog.tsx` | 357 | C | 19 | undefined |
| 29 | components/history | 1.00 | `components/history/HistoryEntryCard.tsx` | 150 | P | 1 | undefined |
| 29 | components/history | 1.00 | `components/history/HistoryFilterBar.tsx` | 168 | P | 2 | undefined |
| 30 | components/query | 0.30 | `components/query/ExplainPlanTree.tsx` | 89 | P | 3 | undefined |
| 31 | components/query | 0.90 | `components/query/QueryArtifactExport.tsx` | 63 | C | 4 | undefined |
| 32 | components/query | 1.00 | `components/query/QueryContextSelectors.tsx` | 229 | P | 2 | undefined |
| 32 | components/query | 1.00 | `components/query/QueryErrorPanel.tsx` | 129 | P | 3 | undefined |
| 33 | components/query | 0.30 | `components/query/QueryExecutionStatus.tsx` | 82 | P | 1 | undefined |
| 34 | components/connection | 0.80 | `components/connection/ConnectionAdvancedSettings.tsx` | 362 | P | 4 | undefined |
| 35 | components/connection | 0.90 | `components/connection/ConnectionAiNotes.tsx` | 92 | C | 7 | undefined |
| 36 | components/connection | 1.00 | `components/connection/ConnectionFormBody.tsx` | 131 | P | 1 | undefined |
| 36 | components/connection | 1.00 | `components/connection/ConnectionImportFields.tsx` | 123 | P | 1 | undefined |
| 37 | components/connection | 0.90 | `components/connection/ConnectionShareDialog.tsx` | 167 | C | 6 | undefined |
| 38 | components/connection | 0.90 | `components/connection/ConnectionShareDialogHost.tsx` | 63 | C | 10 | undefined |
| 39 | components/connection | 0.90 | `components/connection/FileConnectionFields.tsx` | 196 | C | 12 | undefined |
| 40 | components/connection | 0.60 | `components/connection/HttpProxyTunnelFields.tsx` | 77 | P | 1 | undefined |
| 40 | components/connection | 0.60 | `components/connection/IndexConnectionFields.tsx` | 32 | P | 1 | undefined |
| 41 | components/connection | 0.90 | `components/connection/NewConnectionDialog.tsx` | 242 | C | 19 | undefined |
| 42 | components/connection | 0.90 | `components/connection/ObjectFilterDialog.tsx` | 98 | P | 6 | undefined |
| 42 | components/connection | 0.90 | `components/connection/SaveTunnelDialog.tsx` | 97 | P | 4 | undefined |
| 42 | components/connection | 0.90 | `components/connection/shared.tsx` | 18 | P | 0 | undefined |
| 43 | components/connection | 0.80 | `components/connection/SshTunnelFields.tsx` | 256 | P | 1 | undefined |
| 43 | components/connection | 0.80 | `components/connection/StandardConnectionFields.tsx` | 76 | P | 1 | undefined |
| 44 | components/connection | 0.80 | `components/connection/WebSocketTunnelFields.tsx` | 60 | P | 1 | undefined |
| 44 | components/connection | 0.80 | `components/chart/AxisConfigurator.tsx` | 227 | P | 2 | undefined |
| 45 | components/chart | 2.00 split | `components/chart/ChartCanvas.tsx` | 37 | H | 0 | undefined |
| 46 | components/chart | 0.80 | `components/chart/ChartEmptyState.tsx` | 23 | P | 1 | undefined |
| 46 | components/chart | 0.80 | `components/chart/ChartToolbar.tsx` | 180 | P | 2 | undefined |
| 47 | components/chart | 0.80 | `components/chart/ChartView.tsx` | 287 | P | 10 | undefined |
| 48 | components/chart | 2.00 split | `components/chart/LogScaleTooltip.tsx` | 31 | H | 0 | undefined |
| 49 | components/chart | 2.00 split | `components/chart/renderers/AreaChartRenderer.tsx` | 125 | H | 4 | undefined |
| 50 | components/chart | 2.00 split | `components/chart/renderers/BarChartRenderer.tsx` | 107 | H | 3 | undefined |
| 51 | components/chart | 2.00 split | `components/chart/renderers/LineChartRenderer.tsx` | 140 | H | 4 | undefined |
| 52 | components/chart | 2.00 split | `components/chart/renderers/PieChartRenderer.tsx` | 56 | H | 0 | undefined |
| 53 | components/chart | 2.00 split | `components/chart/renderers/ScatterChartRenderer.tsx` | 75 | H | 0 | undefined |
| 54 | components/ai | 5.30 split | `components/ai/AiChatPanel.tsx` | 1322 | H | 42 | undefined |
| 55 | components/ai | 0.80 | `components/ai/AiCodeBlock.tsx` | 261 | P | 9 | undefined |
| 56 | components/ai | 0.90 | `components/ai/AiEgressNotice.tsx` | 58 | C | 2 | undefined |
| 57 | components/ai | 0.80 | `components/ai/AiInput.tsx` | 295 | P | 8 | undefined |
| 58 | components/ai | 2.00 split | `components/ai/AiMessageContent.tsx` | 158 | H | 1 | undefined |
| 59 | components/ai | 0.90 | `components/ai/AiSessionHistory.tsx` | 171 | C | 13 | undefined |
| 60 | components/ai | 1.30 split | `components/ai/ContextPicker.tsx` | 395 | C | 17 | undefined |
| 61 | components/ai | 0.90 | `components/ai/DiagnosisPanel.tsx` | 178 | C | 11 | undefined |
| 62 | components/ai | 0.90 | `components/ai/ExplainPanel.tsx` | 249 | C | 10 | undefined |
| 63 | components/ai | 0.90 | `components/ai/Nl2SqlPanel.tsx` | 287 | C | 16 | undefined |
| 64 | components/ai | 0.90 | `components/ai/NlFilterInput.tsx` | 199 | C | 15 | undefined |
| 65 | components/ai | 1.30 split | `components/ai/WorkflowChatPanel.tsx` | 477 | C | 23 | undefined |
| 66 | components/ai | 1.30 split | `components/ai/WorkflowPanel.tsx` | 411 | C | 18 | undefined |
| 67 | components/migration | 0.80 | `components/migration/MigrationEndpointsBar.tsx` | 372 | P | 2 | undefined |
| 68 | components/migration | 0.50 | `components/migration/MigrationJobFailureNotice.tsx` | 122 | P | 1 | undefined |
| 69 | components/migration | 1.30 split | `components/migration/MigrationJobVerdictPanel.tsx` | 324 | C | 1 | undefined |
| 70 | components/migration | 0.90 | `components/migration/MigrationRunHistoryDialog.tsx` | 171 | C | 6 | undefined |
| 71 | components/sql-editor | 2.70 split | `components/sql-editor/SqlEditor.tsx` | 667 | H | 42 | undefined |
| 72 | components/admin | 0.90 | `components/admin/AdminCreateDialog.tsx` | 146 | C | 8 | undefined |
| 73 | components/admin | 0.60 | `components/admin/AdminSchemaFields.tsx` | 76 | P | 0 | undefined |
| 73 | components/admin | 0.60 | `components/admin/PrivilegeSelector.tsx` | 56 | P | 1 | undefined |
| 74 | components | 0.90 | `components/AiWriteProposalHost.tsx` | 290 | C | 9 | undefined |
| 75 | components | 0.90 | `components/DbTypeBadge.tsx` | 108 | P | 2 | undefined |
| 75 | components | 0.90 | `components/ErrorBoundary.tsx` | 108 | P | 1 | undefined |
| 75 | components | 0.90 | `components/export/ExportColumnPicker.tsx` | 104 | P | 1 | undefined |
| 76 | components/export | 0.60 | `components/export/ExportSuccessPanel.tsx` | 21 | P | 0 | undefined |
| 76 | components/export | 0.60 | `components/FilterBar.tsx` | 66 | P | 1 | undefined |
| 77 | components | 2.60 split | `components/FilterEditor.tsx` | 651 | H | 5 | undefined |
| 78 | components | 0.30 | `components/LocaleDomainLoading.tsx` | 45 | P | 0 | undefined |
| 79 | components | 0.90 | `components/McpApprovalHost.tsx` | 216 | C | 8 | undefined |
| 80 | components | 1.30 split | `components/MenuBar.tsx` | 467 | C | 13 | undefined |
| 81 | components | 2.00 split | `components/SqlCodeBlock.tsx` | 187 | H | 6 | undefined |
| 82 | components | 0.80 | `components/SqlEditor.tsx` | 17 | P | 0 | undefined |
| 82 | components | 0.80 | `components/SqlSyntaxPreview.tsx` | 137 | P | 1 | undefined |
| 83 | components | 0.80 | `components/StatusBar.tsx` | 23 | P | 0 | undefined |
| 83 | components | 0.80 | `components/ThemedIcon.tsx` | 144 | P | 2 | undefined |
| 84 | components | 0.90 | `components/ThemeToggle.tsx` | 72 | C | 5 | undefined |
| 85 | components | 0.30 | `components/WindowChromeFallback.tsx` | 29 | P | 1 | undefined |
| 86 | windows/settings | 0.90 | `windows/settings/ai/AiContextSettings.tsx` | 287 | C | 9 | undefined |
| 87 | windows/settings | 0.30 | `windows/settings/ai/ModelProfileDialog.tsx` | 44 | P | 2 | undefined |
| 88 | windows/settings | 0.80 | `windows/settings/ai/ModelProfileForm.tsx` | 508 | P | 1 | undefined |
| 89 | windows/settings | 0.50 | `windows/settings/ai/ModelProfileListItem.tsx` | 178 | P | 1 | undefined |
| 90 | windows/settings | 0.90 | `windows/settings/AiSettingsSection.tsx` | 236 | C | 15 | undefined |
| 91 | windows/settings | 0.90 | `windows/settings/AiToolBridgeSection.tsx` | 109 | C | 6 | undefined |
| 92 | windows/settings | 0.90 | `windows/settings/AppearanceSection.tsx` | 157 | C | 6 | undefined |
| 93 | windows/settings | 0.90 | `windows/settings/DataCleanupSection.tsx` | 186 | C | 8 | undefined |
| 94 | windows/settings | 0.90 | `windows/settings/DriverSettingsSection.tsx` | 91 | C | 5 | undefined |
| 95 | windows/settings | 0.90 | `windows/settings/InstallLanguagePackDialog.tsx` | 178 | C | 4 | undefined |
| 96 | windows/settings | 0.30 | `windows/settings/JsonSchemaSettingsForm.tsx` | 55 | P | 2 | undefined |
| 97 | windows/settings | 0.90 | `windows/settings/LanguagePacksCard.tsx` | 186 | C | 11 | undefined |
| 98 | windows/settings | 1.30 split | `windows/settings/McpClientSection.tsx` | 490 | C | 8 | undefined |
| 99 | windows/settings | 1.30 split | `windows/settings/McpSettingsSection.tsx` | 775 | C | 27 | undefined |
| 100 | windows/settings | 1.30 split | `windows/settings/MemorySection.tsx` | 719 | C | 16 | undefined |
| 101 | windows/settings | 0.90 | `windows/settings/PromptSettingsSection.tsx` | 204 | C | 6 | undefined |
| 102 | windows/settings | 0.30 | `windows/settings/SettingHint.tsx` | 91 | P | 5 | undefined |
| 103 | windows/settings | 1.30 split | `windows/settings/SettingsContent.tsx` | 757 | C | 19 | undefined |
| 104 | windows/settings | 0.30 | `windows/settings/settingsUi.tsx` | 79 | P | 0 | undefined |
| 105 | windows/settings | 0.90 | `windows/settings/SkillsSection.tsx` | 191 | C | 7 | undefined |
| 106 | windows/settings | 0.50 | `windows/settings/SnippetEditDialog.tsx` | 170 | P | 5 | undefined |
| 107 | windows/settings | 0.90 | `windows/settings/SqlSnippetsCard.tsx` | 305 | C | 11 | undefined |
| 108 | windows/settings | 0.90 | `windows/settings/TableDataSettingsSection.tsx` | 135 | C | 3 | undefined |
| 109 | windows/settings | 0.90 | `windows/settings/TunnelDeleteDialog.tsx` | 185 | C | 4 | undefined |
| 110 | windows/settings | 0.90 | `windows/settings/TunnelEditDialog.tsx` | 307 | C | 7 | undefined |
| 111 | windows/settings | 0.90 | `windows/settings/TunnelSettingsSection.tsx` | 290 | C | 13 | undefined |
| 112 | windows/settings | 0.90 | `windows/settings/TunnelTestDialog.tsx` | 160 | C | 5 | undefined |
| 113 | windows/settings | 0.90 | `windows/settings/UpdateSection.tsx` | 155 | C | 5 | undefined |
| 114 | windows/connection | 2.00 split | `windows/connection/BatchExportDialog.tsx` | 475 | H | 10 | undefined |
| 115 | windows/connection | 1.30 split | `windows/connection/ConnectionNavigatorTree.tsx` | 888 | C | 52 | undefined |
| 116 | windows/connection | 1.30 split | `windows/connection/ConnectionPage.tsx` | 967 | C | 62 | undefined |
| 117 | windows/connection | 1.30 split | `windows/connection/ConnectionWorkspaceHome.tsx` | 527 | C | 9 | undefined |
| 118 | windows/connection | 0.80 | `windows/connection/ContentStatusBar.tsx` | 112 | P | 2 | undefined |
| 118 | windows/connection | 0.80 | `windows/connection/ContentToolbar.tsx` | 246 | P | 3 | undefined |
| 119 | windows/connection | 1.30 split | `windows/connection/ContentView.tsx` | 777 | C | 65 | undefined |
| 120 | windows/connection | 0.90 | `windows/connection/ContentViewDialogs.tsx` | 256 | C | 4 | undefined |
| 121 | windows/connection | 1.30 split | `windows/connection/ContentViewDrawers.tsx` | 444 | C | 16 | undefined |
| 122 | windows/connection | 0.60 | `windows/connection/CreateDatabaseDialog.tsx` | 36 | P | 1 | undefined |
| 122 | windows/connection | 0.60 | `windows/connection/CreateSchemaDialog.tsx` | 39 | P | 1 | undefined |
| 123 | windows/connection | 0.90 | `windows/connection/CreateUserDialog.tsx` | 288 | C | 18 | undefined |
| 124 | windows/connection | 0.90 | `windows/connection/DatabaseObjectView.tsx` | 138 | C | 7 | undefined |
| 125 | windows/connection | 0.50 | `windows/connection/DDLView.tsx` | 134 | P | 7 | undefined |
| 126 | windows/connection | 3.60 split | `windows/connection/DocumentConnectionView.tsx` | 907 | H | 40 | undefined |
| 127 | windows/connection | 0.30 | `windows/connection/er/ErRelationLegend.tsx` | 101 | P | 1 | undefined |
| 128 | windows/connection | 2.00 split | `windows/connection/er/TableNode.tsx` | 172 | H | 1 | undefined |
| 129 | windows/connection | 2.20 split | `windows/connection/ErDiagramView.tsx` | 543 | H | 28 | undefined |
| 130 | windows/connection | 0.50 | `windows/connection/ExecuteSqlFileDialog.tsx` | 160 | P | 8 | undefined |
| 131 | windows/connection | 1.30 split | `windows/connection/ExportDialog.tsx` | 410 | C | 18 | undefined |
| 132 | windows/connection | 0.90 | `windows/connection/ForeignKeysView.tsx` | 135 | C | 4 | undefined |
| 133 | windows/connection | 0.90 | `windows/connection/home/ConnectionCardList.tsx` | 262 | C | 5 | undefined |
| 134 | windows/connection | 0.60 | `windows/connection/home/HomeHero.tsx` | 55 | P | 1 | undefined |
| 134 | windows/connection | 0.60 | `windows/connection/home/McpPromoBar.tsx` | 68 | P | 4 | undefined |
| 135 | windows/connection | 0.50 | `windows/connection/home/RecentQueriesList.tsx` | 213 | P | 6 | undefined |
| 136 | windows/connection | 0.90 | `windows/connection/home/ShortcutFooter.tsx` | 53 | C | 3 | undefined |
| 137 | windows/connection | 3.70 split | `windows/connection/ImportDialog.tsx` | 931 | H | 20 | undefined |
| 138 | windows/connection | 1.30 split | `windows/connection/IndexesView.tsx` | 542 | C | 16 | undefined |
| 139 | windows/connection | 1.30 split | `windows/connection/navigator/DatabaseValueSearchDialog.tsx` | 507 | C | 15 | undefined |
| 140 | windows/connection | 1.00 | `windows/connection/navigator/GlobalObjectSearch.tsx` | 164 | P | 3 | undefined |
| 140 | windows/connection | 1.00 | `windows/connection/navigator/NavigatorDialogs.tsx` | 155 | P | 0 | undefined |
| 141 | windows/connection | 0.50 | `windows/connection/navigator/NavigatorToolbar.tsx` | 155 | P | 0 | undefined |
| 142 | windows/connection | 1.30 split | `windows/connection/navigator/NavigatorTreeRow.tsx` | 812 | C | 1 | undefined |
| 143 | windows/connection | 0.90 | `windows/connection/navigator/treeStatusDot.tsx` | 37 | C | 0 | undefined |
| 144 | windows/connection | 0.90 | `windows/connection/ObjectBrowser.tsx` | 307 | C | 12 | undefined |
| 145 | windows/connection | 1.30 split | `windows/connection/PanelContentRenderer.tsx` | 595 | C | 5 | undefined |
| 146 | windows/connection | 0.90 | `windows/connection/PanelTabBar.tsx` | 131 | C | 2 | undefined |
| 147 | windows/connection | 0.30 | `windows/connection/PendingPlanDialog.tsx` | 96 | P | 1 | undefined |
| 148 | windows/connection | 1.30 split | `windows/connection/PrivilegeView.tsx` | 930 | C | 24 | undefined |
| 149 | windows/connection | 0.90 | `windows/connection/ProcessListView.tsx` | 194 | C | 9 | undefined |
| 150 | windows/connection | 0.30 | `windows/connection/query/ExecutionStrategyAskModal.tsx` | 97 | P | 1 | undefined |
| 151 | windows/connection | 0.90 | `windows/connection/query/QueryBuilderHostAdapter.tsx` | 167 | C | 7 | undefined |
| 152 | windows/connection | 1.30 split | `windows/connection/query/QueryEditorSection.tsx` | 723 | C | 20 | undefined |
| 153 | windows/connection | 1.30 split | `windows/connection/query/QuerySidebarSection.tsx` | 529 | C | 30 | undefined |
| 154 | windows/connection | 1.30 split | `windows/connection/query/QueryTransactionModals.tsx` | 648 | C | 19 | undefined |
| 155 | windows/connection | 0.90 | `windows/connection/query/toolbar/ExecutionStrategySelect.tsx` | 74 | C | 4 | undefined |
| 156 | windows/connection | 0.90 | `windows/connection/query/toolbar/RefreshCompletionButton.tsx` | 71 | C | 3 | undefined |
| 157 | windows/connection | 0.90 | `windows/connection/query/toolbar/SnippetMenuButton.tsx` | 67 | C | 3 | undefined |
| 158 | windows/connection | 1.30 split | `windows/connection/query/useQueryExecutionGate.tsx` | 674 | C | 23 | undefined |
| 159 | windows/connection | 1.30 split | `windows/connection/QueryPanel.tsx` | 1000 | C | 91 | undefined |
| 160 | windows/connection | 0.80 | `windows/connection/result-workspace/ExecutionSummaryCard.tsx` | 90 | P | 3 | undefined |
| 160 | windows/connection | 0.80 | `windows/connection/result-workspace/ResultFindBar.tsx` | 255 | P | 4 | undefined |
| 161 | windows/connection | 1.30 split | `windows/connection/result-workspace/ResultTableView.tsx` | 437 | C | 19 | undefined |
| 162 | windows/connection | 0.50 | `windows/connection/result-workspace/ResultWorkspace.tsx` | 188 | P | 3 | undefined |
| 163 | windows/connection | 3.50 split | `windows/connection/ServerStatusView.tsx` | 876 | H | 11 | undefined |
| 164 | windows/connection | 0.80 | `windows/connection/SetValueDateDialog.tsx` | 89 | P | 3 | undefined |
| 164 | windows/connection | 0.80 | `windows/connection/structure/StructureColumnTable.tsx` | 253 | P | 1 | undefined |
| 165 | windows/connection | 0.80 | `windows/connection/structure/StructureIndexTable.tsx` | 199 | P | 2 | undefined |
| 165 | windows/connection | 0.80 | `windows/connection/structure/StructurePlanPreview.tsx` | 69 | P | 1 | undefined |
| 166 | windows/connection | 0.90 | `windows/connection/StructureView.tsx` | 253 | C | 5 | undefined |
| 167 | windows/connection | 0.30 | `windows/connection/TableClipboardStatus.tsx` | 70 | P | 1 | undefined |
| 168 | windows/connection | 0.80 | `windows/connection/TableColumnFilter.tsx` | 264 | P | 8 | undefined |
| 169 | windows/connection | 0.50 | `windows/connection/TableFilterToolbar.tsx` | 233 | P | 3 | undefined |
| 170 | windows/connection | 0.90 | `windows/connection/TableFkBreadcrumb.tsx` | 119 | C | 1 | undefined |
| 171 | windows/connection | 0.90 | `windows/connection/TableFkErrorBanner.tsx` | 54 | P | 2 | undefined |
| 171 | windows/connection | 0.90 | `windows/connection/TableLoadErrorState.tsx` | 35 | P | 0 | undefined |
| 171 | windows/connection | 0.90 | `windows/connection/TableNoPrimaryKeyTip.tsx` | 52 | P | 0 | undefined |
| 172 | windows/connection | 0.90 | `windows/connection/TablePendingChangesBar.tsx` | 302 | C | 13 | undefined |
| 173 | windows/connection | 0.60 | `windows/connection/TableReadOnlyTip.tsx` | 32 | P | 1 | undefined |
| 173 | windows/connection | 0.60 | `windows/connection/TableStatusBanners.tsx` | 61 | P | 1 | undefined |
| 174 | windows/connection | 1.30 split | `windows/connection/TableStructureEditor.tsx` | 630 | C | 24 | undefined |
| 175 | windows/connection | 0.50 | `windows/connection/TableToolbar.tsx` | 151 | P | 1 | undefined |
| 176 | windows/connection | 5.70 split | `windows/connection/TableView.tsx` | 1419 | H | 63 | undefined |
| 177 | windows/connection | 0.90 | `windows/connection/WorkspaceModeSidebar.tsx` | 200 | C | 6 | undefined |
| 178 | windows/workspace | 0.30 | `windows/workspace/WappIcon.tsx` | 39 | P | 2 | undefined |
| 179 | windows/workspace | 0.90 | `windows/workspace/WappPageShell.tsx` | 252 | C | 11 | undefined |
| 180 | windows/workspace | 0.30 | `windows/workspace/WorkspaceDefaultCards.tsx` | 79 | P | 1 | undefined |
| 181 | windows/workspace | 0.90 | `windows/workspace/WorkspaceNavigator.tsx` | 83 | C | 3 | undefined |
| 182 | windows/workspace | 0.90 | `windows/workspace/WorkspaceTabBar.tsx` | 77 | C | 5 | undefined |
| 183 | windows/workspace | 0.90 | `windows/workspace/WorkspaceView.tsx` | 121 | C | 10 | undefined |
| 184 | windows/dashboard | 0.90 | `windows/dashboard/AddToDashboardDialog.tsx` | 153 | C | 8 | undefined |
| 185 | windows/dashboard | 0.80 | `windows/dashboard/ChartWidgetTile.tsx` | 395 | P | 6 | undefined |
| 186 | windows/dashboard | 1.30 split | `windows/dashboard/DashboardPanel.tsx` | 765 | C | 50 | undefined |
| 187 | windows/dashboard | 0.90 | `windows/dashboard/MonitorScheduleAuthorization.tsx` | 91 | C | 3 | undefined |
| 188 | windows/dashboard | 0.90 | `windows/dashboard/RunHistoryDrawer.tsx` | 289 | C | 7 | undefined |
| 189 | windows/dashboard | 1.30 split | `windows/dashboard/WidgetEditorDrawer.tsx` | 473 | C | 6 | undefined |
| 190 | windows/workflow | 0.80 | `windows/workflow/WorkflowEditorActionBar.tsx` | 35 | P | 1 | undefined |
| 190 | windows/workflow | 0.80 | `windows/workflow/WorkflowExecutionResultPanel.tsx` | 242 | P | 5 | undefined |
| 191 | windows/workflow | 1.30 split | `windows/workflow/WorkflowForm.tsx` | 937 | C | 14 | undefined |
| 192 | windows/workflow | 0.50 | `windows/workflow/WorkflowHistorySection.tsx` | 141 | P | 0 | undefined |
| 193 | windows/workflow | 1.30 split | `windows/workflow/WorkflowPage.tsx` | 1392 | C | 52 | undefined |
| 194 | windows/workflow | 0.30 | `windows/workflow/workflowStepResultUtils.tsx` | 60 | P | 0 | undefined |
| 195 | windows/workflow | 2.00 split | `windows/workflow/WorkflowYamlEditor.tsx` | 107 | H | 3 | undefined |
| 196 | windows/main | 0.90 | `windows/main/MainPage.tsx` | 144 | C | 13 | undefined |
| 197 | windows/main | 0.90 | `windows/main/RunningExecutionCloseHost.tsx` | 42 | C | 2 | undefined |
| 198 | windows/onboarding | 0.90 | `windows/onboarding/BrandSidebar.tsx` | 119 | C | 3 | undefined |
| 199 | windows/onboarding | 0.90 | `windows/onboarding/OnboardingWindow.tsx` | 20 | C | 0 | undefined |
| 200 | windows/onboarding | 0.90 | `windows/onboarding/OnboardingWizard.tsx` | 172 | C | 8 | undefined |
| 201 | windows/onboarding | 0.90 | `windows/onboarding/steps/AiProviderStep.tsx` | 54 | C | 3 | undefined |
| 202 | windows/onboarding | 1.00 | `windows/onboarding/steps/DoneStep.tsx` | 130 | P | 1 | undefined |
| 202 | windows/onboarding | 1.00 | `windows/onboarding/steps/ImportStep.tsx` | 145 | P | 2 | undefined |
| 203 | windows/onboarding | 0.90 | `windows/onboarding/steps/ManualConnectionStep.tsx` | 186 | C | 13 | undefined |
| 204 | windows/onboarding | 0.90 | `windows/onboarding/steps/SampleStep.tsx` | 173 | C | 7 | undefined |
| 205 | windows/onboarding | 1.00 | `windows/onboarding/steps/WelcomeStep.tsx` | 135 | P | 1 | undefined |
| 205 | windows/onboarding | 1.00 | `windows/onboarding/WizardFooter.tsx` | 121 | P | 1 | undefined |
| 206 | windows/wapps | 0.90 | `windows/wapps/InstallWappDialog.tsx` | 200 | C | 4 | undefined |
| 207 | windows/wapps | 1.30 split | `windows/wapps/WappManagementPage.tsx` | 422 | C | 15 | undefined |
| 208 | windows/data-transfer | 1.30 split | `windows/data-transfer/ColumnMappingEditor.tsx` | 525 | C | 2 | undefined |
| 209 | windows/data-transfer | 0.90 | `windows/data-transfer/DataTransferAttachedJobs.tsx` | 111 | C | 1 | undefined |
| 210 | windows/data-transfer | 0.90 | `windows/data-transfer/DataTransferTerminalResults.tsx` | 146 | C | 3 | undefined |
| 211 | windows/data-transfer | 1.30 split | `windows/data-transfer/DataTransferWindow.tsx` | 1901 | C | 76 | undefined |
| 212 | windows/data-transfer | 0.90 | `windows/data-transfer/MappingGateNotice.tsx` | 36 | C | 1 | undefined |
| 213 | windows/data-transfer | 0.90 | `windows/data-transfer/SourceFilterEditor.tsx` | 211 | C | 1 | undefined |
| 214 | windows/data-transfer | 0.90 | `windows/data-transfer/TransferExecuteConfirmDialog.tsx` | 69 | C | 1 | undefined |
| 215 | windows/data-transfer | 0.30 | `windows/data-transfer/TransferJobResultPanel.tsx` | 119 | P | 2 | undefined |
| 216 | windows/data-transfer | 0.90 | `windows/data-transfer/TransferMappingStep.tsx` | 90 | C | 2 | undefined |
| 217 | windows/data-transfer | 0.60 | `windows/data-transfer/TransferPairingNote.tsx` | 18 | P | 1 | undefined |
| 217 | windows/data-transfer | 0.60 | `windows/data-sync/CompareSummary.tsx` | 82 | P | 3 | undefined |
| 218 | windows/data-sync | 0.90 | `windows/data-sync/DataSyncCompareStep.tsx` | 148 | C | 1 | undefined |
| 219 | windows/data-sync | 0.30 | `windows/data-sync/DataSyncDialogs.tsx` | 117 | P | 1 | undefined |
| 220 | windows/data-sync | 0.90 | `windows/data-sync/DataSyncEndpointStep.tsx` | 115 | C | 1 | undefined |
| 221 | windows/data-sync | 0.90 | `windows/data-sync/DataSyncJobStatusPanel.tsx` | 111 | C | 1 | undefined |
| 222 | windows/data-sync | 0.90 | `windows/data-sync/DataSyncObjectsStep.tsx` | 58 | C | 1 | undefined |
| 223 | windows/data-sync | 0.90 | `windows/data-sync/DataSyncOptionsStep.tsx` | 29 | C | 1 | undefined |
| 224 | windows/data-sync | 0.90 | `windows/data-sync/DataSyncPreviewStep.tsx` | 82 | C | 1 | undefined |
| 225 | windows/data-sync | 0.90 | `windows/data-sync/DataSyncResultStep.tsx` | 81 | C | 1 | undefined |
| 226 | windows/data-sync | 1.30 split | `windows/data-sync/DataSyncWindow.tsx` | 343 | C | 22 | undefined |
| 227 | windows/data-sync | 0.60 | `windows/data-sync/DataSyncWizardNav.tsx` | 47 | P | 1 | undefined |
| 227 | windows/data-sync | 0.60 | `windows/data-sync/DataSyncWizardStepper.tsx` | 51 | P | 1 | undefined |
| 228 | windows/data-sync | 1.30 split | `windows/data-sync/DiffDetail.tsx` | 358 | C | 5 | undefined |
| 229 | windows/data-sync | 0.60 | `windows/data-sync/EndpointsBar.tsx` | 74 | P | 1 | undefined |
| 229 | windows/data-sync | 0.60 | `windows/data-sync/ExecuteBar.tsx` | 79 | P | 1 | undefined |
| 230 | windows/data-sync | 0.50 | `windows/data-sync/MappingPanel.tsx` | 204 | P | 1 | undefined |
| 231 | windows/data-sync | 0.90 | `windows/data-sync/OptionsBar.tsx` | 80 | C | 1 | undefined |
| 232 | windows/data-sync | 0.90 | `windows/data-sync/RecordsetEditor.tsx` | 217 | C | 2 | undefined |
| 233 | windows/data-sync | 0.90 | `windows/data-sync/SqlPreview.tsx` | 155 | C | 7 | undefined |
| 234 | windows/data-sync | 0.50 | `windows/data-sync/TableListPanel.tsx` | 136 | P | 1 | undefined |
| 235 | windows/schema-diff | 0.90 | `windows/schema-diff/SchemaDiffDeployPanel.tsx` | 159 | C | 1 | undefined |
| 236 | windows/schema-diff | 0.90 | `windows/schema-diff/SchemaDiffExecutionPanels.tsx` | 209 | C | 1 | undefined |
| 237 | windows/schema-diff | 0.90 | `windows/schema-diff/SchemaDiffJobStatusPanel.tsx` | 156 | C | 1 | undefined |
| 238 | windows/schema-diff | 0.30 | `windows/schema-diff/SchemaDiffObjectsStep.tsx` | 100 | P | 1 | undefined |
| 239 | windows/schema-diff | 1.30 split | `windows/schema-diff/SchemaDiffPlanPanel.tsx` | 336 | C | 4 | undefined |
| 240 | windows/schema-diff | 0.90 | `windows/schema-diff/SchemaDiffRightPanel.tsx` | 150 | C | 1 | undefined |
| 241 | windows/schema-diff | 0.30 | `windows/schema-diff/SchemaDiffSavedSetupDialogs.tsx` | 102 | P | 1 | undefined |
| 242 | windows/schema-diff | 0.90 | `windows/schema-diff/SchemaDiffSelectionSteps.tsx` | 97 | C | 0 | undefined |
| 243 | windows/schema-diff | 0.30 | `windows/schema-diff/SchemaDiffTableListPanel.tsx` | 75 | P | 1 | undefined |
| 244 | windows/schema-diff | 0.90 | `windows/schema-diff/SchemaDiffUnifiedObjectsPicker.tsx` | 269 | C | 2 | undefined |
| 245 | windows/schema-diff | 1.30 split | `windows/schema-diff/SchemaDiffWindow.tsx` | 799 | C | 40 | undefined |
| 246 | windows/schema-diff | 0.90 | `windows/schema-diff/SchemaDiffWindowJobSection.tsx` | 45 | C | 0 | undefined |
| 247 | windows/schema-diff | 0.60 | `windows/schema-diff/SchemaDiffWindowNavigation.tsx` | 53 | P | 1 | undefined |
| 247 | windows/schema-diff | 0.60 | `windows/schema-diff/SchemaDiffWizardProgress.tsx` | 57 | P | 1 | undefined |
| 248 | windows/backup | 1.30 split | `windows/backup/BackupWindow.tsx` | 565 | C | 27 | undefined |
| 249 | windows/backup | 2.00 split | `windows/backup/ProgressLog.tsx` | 96 | H | 7 | undefined |
| 250 | hooks | 0.30 | `hooks/useConfirmDialog.tsx` | 70 | P | 3 | undefined |
| 251 | bootstrap | 2.00 split | `bootstrap/startSharedApp.tsx` | 223 | H | 0 | undefined |
| 252 | App.tsx | 0.30 | `App.tsx` | 108 | P | 2 | undefined |

## 5. 执行规约

每个 task 的定义（DoD）：

1. Rune 文件落在与 React 同名路径（`windows/settings/SettingsPage.tsx` → `views/settings/settings_page.rs`），可以按 800 行上限拆模块，但不改目录语义。
2. `scripts/rune-migrate-gate.sh` 输出 `gate: PASS`。
3. 新页面挂进 `Section` 分发，`--tree` 快照里关键 `key` 存在。
4. 960×640 与 1440×816 两档无横向越界（复用 `crates/datazen-ui/src/tests/chrome_metrics.rs` 的断言模式）。
5. 一个 task 一个提交：`feat(rune-ui): port <react path>`；文档/台账变动单独 `docs(rune):`。

节奏：

- 每 5 个 task 跑一次全量：`scripts/rune-migrate-gate.sh` + headless 六页扫描。
- 每个 area（表内 `area` 列）收尾更新 `docs/development/rune-migration/progress.md` 一行。
- 台账快照过期就重新生成，不手改。
