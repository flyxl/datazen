# Connection Window 批量导出进度

> 分支：`feat/connection-batch-export`  
> Worktree：`/Users/wuxiaolong/code/rust-projects/datazen-conn-batch-export`  
> 目标：在 Connection Window 支持导出全部/所选表，模式为「数据+结构 / 仅数据 / 仅结构」。

## 状态图例

- `pending` / `dev` / `testing` / `fixing` / `done`

## 功能清单

| ID | 功能 | 状态 | Commit | 测试结果 |
|----|------|------|--------|----------|
| F1 | 批量导出纯逻辑：模式(data/structure/both)、多表内容组装、文件名约定 | done | （本提交） | PASS：25/25，lines 100% |
| F2 | BatchExportDialog UI：表全选/多选 + 导出模式 + 格式/进度 | done | （本提交） | PASS：job lines 100% |
| F3 | 拉取 DDL + 全量（分页）表数据供批量导出 | done | （本提交） | PASS：lines 97.77% |
| F4 | 接入 SqlConnectionView 顶栏 + Schema 树（库节点/空白/多表）入口 | done | （本提交） | PASS |
| F5 | i18n 全 locale + 文档(AGENTS/architecture) + E2E 用例调整 | pending | — | — |
| F6 | 合并到 main 并 push | pending | — | — |

## 测试约定

- 每功能附带单元测试；完成后由**新开独立测试 agent**验证（覆盖率 lines≥80%）；失败则另开编码 agent 修复再复测；通过后提交。

## 变更日志

### F1 — 批量导出纯逻辑（testing）

- 新增 `src/lib/batchExport.ts`：`selectTablesForExport` / `buildBatchExportFiles` / `combineBatchExportFiles` / `getBatchExportDefaultFilename`
- 模式：`structure_only`（每表 DDL `.sql`）、`data_only`（复用 `generateExport`）、`data_and_structure`（`sql_insert` 合并 DDL+INSERT；csv/json 拆成 `.sql` + 数据文件）
- 单测：`src/lib/__tests__/batchExport.test.ts`（表选择、三模式、合并、默认文件名）
- 无 UI；未 commit

### F2 — BatchExportDialog UI（testing）

- 新增 `src/windows/connection/BatchExportDialog.tsx`：表多选/全选、三模式、数据格式、单文件/ZIP
- 新增 `src/lib/batchExportJob.ts`：`runBatchExportJob` / `zipBatchExportFiles`（fflate + saveText/saveBase64）
- i18n：`batchExport.*`（en + zh-CN 实译；其它 locale 英文占位）
- 单测：`batchExportJob.test.ts` + `BatchExportDialog.test.tsx`
- `loadTableExportData` 必填（F3 接线）；未 commit

### F3 — 拉取 DDL + 全量表数据（testing）

- 新增 `src/lib/loadBatchExportTable.ts`：`loadBatchExportTableData`（schema + DDL + 分页全量 rows）
- deps 可注入：`getSchema` / `getDdl` / `getTableData` / `getDialect`（便于单测）
- DDL：有方言则 `getCachedDDL`；无方言或失败 → `ddl: null`，仍返回数据
- 单测：`src/lib/__tests__/loadBatchExportTable.test.ts`（多页合并、maxRows、无方言、DDL 失败）
- 未 commit

### F4 — 接入 SqlConnectionView（testing）

- 顶栏「批量导出」按钮（Download 图标）→ `BatchExportDialog`；`initialSelected` 在 activePanel 为 table 时预选该表
- Dialog：`tables` 来自 schemaStore；`loadTableExportData` → `loadBatchExportTableData`
- Schema 树：database / blank / table / view 右键「批量导出…」（`onBatchExport`）；表节点保留单表 `ExportDialog`
- `schemaTreeContextMenu`：labels.batchExport + handlers.onBatchExport；单测已更新
- 未 commit
