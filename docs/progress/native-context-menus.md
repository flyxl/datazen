# Native Context Menu 统一改造进度

> 分支：`feat/native-context-menus`  
> Worktree：`/Users/wuxiaolong/code/rust-projects/datazen-native-ctx-menus`  
> 目标：全部 ContextMenu 统一为系统原生菜单；完成 P0/P1/P2。

## 状态图例

- `pending` — 未开始
- `dev` — 开发中
- `testing` — 独立测试 agent 执行中
- `fixing` — 编码 agent 修复中
- `done` — 开发 + 测试通过 + 已提交

## 功能清单

| ID | 功能 | 优先级 | 状态 | Commit | 测试结果 |
|----|------|--------|------|--------|----------|
| F1 | 原生 ContextMenu 共享辅助层（TS popup helper + 事件约定） | P0 基建 | done | b029d7f | PASS：单测 8/8，lines 100%；E2E 设计 4 条（待 UI 接入后跑） |
| F2 | SQL 编辑器原生菜单：Cut/Copy/Paste/SelectAll + 收藏 + 完整 i18n | P0 | done | （本提交） | PASS：复测通过，BUG-F2-001 CLOSED；lines 100% |
| F3 | 移除 SqlConnectionView 整区 Web ContextMenu，消除双菜单 | P0 | done | （本提交） | PASS：单测 2/2；静态断言无 Web ContextMenu |
| F4 | Schema 树原生菜单（表/视图/库/空白；按 nodeKind 分支） | P1 | done | （本提交） | PASS：单测 7/7，lines 100% |
| F5 | DataTable 原生菜单（导出/复制单元格/复制行） | P1 | done | （本提交） | PASS：lines 100% |
| F6 | 连接窗口 Tab 栏原生菜单（关闭/关闭其他/关闭全部） | P1 | pending | — | — |
| F7 | 收藏 / 历史侧栏原生菜单 | P2 | pending | — | — |
| F8 | Redis key 列表原生菜单（驱动 UI） | P2 | pending | — | — |
| F9 | Workflow 列表 / 历史原生菜单 | P2 | pending | — | — |
| F10 | ER 图节点原生菜单 | P2 | pending | — | — |
| F11 | 清理 Web ContextMenu / uiStore 死代码 + 架构文档 / AGENTS.md | P2 | pending | — | — |
| F12 | 合并到 main 并 push | 收尾 | pending | — | — |

## 测试约定

- 每个功能开发必须附带单元测试。
- 功能完成后由**新开的独立测试 agent**执行验证（禁止开发 agent 自测充当本步）。
- 测试 agent 输出：E2E 用例、结果、覆盖率（目标 ≥80%）、失败时的复现步骤；**本步不修复**。
- 若测试不通过，另开编码 agent 修复，再开新测试 agent 复测，通过后提交。

## 变更日志

### F1 — 原生 ContextMenu 共享辅助层
- 新增 `src/lib/nativeContextMenu.ts`：`showNativeContextMenu` / `normalizeNativeMenuItems` / `nativeEditMenuItems` / `createNativeContextMenuHandler`
- 单测：`src/lib/__tests__/nativeContextMenu.test.ts`
- 独立测试 agent：PASS（覆盖率 lines 100%）

### F2 — SQL 编辑器原生菜单
- 新增 `src/lib/sqlEditorContextMenu.ts`；QueryPanel 改用 `showNativeContextMenu`
- SqlEditor 空文档也可右键；移除 Rust `show_editor_context_menu`
- `normalizeNativeMenuItems` **保留** `enabled: false` 项（灰显）
- 修复 BUG-F2-001；独立测试复测 PASS

### F3 — 移除 SqlConnectionView 整区 Web ContextMenu（草稿）
- 从 `SqlConnectionView.tsx` 删除整区 `<ContextMenu>` 包裹及 `contextMenuItems` / `handleContextAction` / `createIndexTrigger`
- IndexesView 不再接收 `createIndexTrigger`（自身已有「新建索引」）
- 单测：`src/windows/connection/__tests__/sqlConnectionViewNoWebContextMenu.test.ts`（源码断言无 ContextMenu import / JSX）
- 目的：消除与 DataTable 的双菜单；表数据右键由后续 F5 原生菜单承接

### F4 — Schema 树原生菜单（草稿）
- 新增 `src/lib/schemaTreeContextMenu.ts`：`SchemaTreeNodeKind` + `buildSchemaTreeContextMenuItems`（table/view/database/blank；labels 由调用方传入）
- `SchemaTree` / `StandardSchemaTree` / `MultiDatabaseSchemaTree`：`onTableContextMenu` → `onNodeContextMenu({ kind, name, x, y, schema? })`；空白处右键 `kind: 'blank'`（stopPropagation）
- `SqlConnectionView`：删除 tableCtx portal / mousedown+Esc 关闭；改为 `showNativeContextMenu(buildSchemaTreeContextMenuItems(...))`；复制名称走 `navigator.clipboard.writeText`
- i18n：补齐 `schemaTree.open` / `schemaTree.openTable` / `schemaTree.copyName` / `schemaTree.copyDatabaseName`（全 locale）
- 单测：`src/lib/__tests__/schemaTreeContextMenu.test.ts`（只读隐藏导入/新建表等分支）

### F5 — DataTable 原生菜单（草稿）
- 新增 `src/lib/dataTableContextMenu.ts`：`buildDataTableContextMenuItems` + `serializeDataTableRowsAsTsv`（copy cell / copy selected rows / export；labels 由调用方传入）
- `DataTable`：删除 ctxMenu portal/backdrop Web 菜单；`onContextMenu` → `preventDefault` + `stopPropagation` + `showNativeContextMenu`
- 复制单元格：`getContextCellText` optional prop，否则 `window.getSelection()`；复制选中行：TSV 写入 clipboard；导出仍打开 `DataExportDialog`
- 单测：`src/lib/__tests__/dataTableContextMenu.test.ts`；更新 `DataTable.test.tsx`（mock native menu）
