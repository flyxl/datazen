# v0.1.x page-integration 进度

## 范围与边界

- 工作目录：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-v01x-page-integration`
- 分支：`feature/v01x-page-integration`
- 本轨只修改共享页面接线、允许的共享 locale/测试，以及本目录的 `progress.md`、`bugs.md`；未修改 `hub.md`。工作区已有的 `hub.md` regular-file → symlink 类型变化来自 bootstrap，未纳入提交。
- 未新增 database type/plugin id 分支、identity/workspace/audit 业务模型或新的 TableWorkspace store；表工作区仍是既有 `TablePanel + SubTabId + PanelContentRenderer`。

## 实现调用链

### A. Connection discovery

`NavigatorToolbar` 的全局对象搜索入口 → `GlobalObjectSearch` → `searchSchemaObjects`；连接导航原有的 `buildNavigatorFlatRows` → `rankConnections` 排序和去重保持不变，结果继续由 `ConnectionNavigatorTree` 展示 name/host/database/type/schema/object/path 命中 reason/context。对象命中后经 `ConnectionNavigatorTree` 选择连接/表，最终进入现有 `ConnectionPage`/`ContentView`。

`ConnectionPage`、`connectionViews/types`、`navigator/types`、`usePanelHandlers` 传递 database/schema context；新建/编辑表单的 Basic/Advanced/SSH 分层仍由组件轨负责。本轨验证现有新建默认 Basic、编辑 SSH 自动展开及保存字段不丢失的相关页面回归未回退。

### B. Object search / table actions / TablePanel

`GlobalObjectSearch` 使用已加载的 `schemaObjectSearch` 索引，列命中保留 owning `tableName` 和 matched field；table/view 结果使用 `buildTableSqlAction` 生成 action 描述，使用 `buildQueryOpenContext` 携带 `connectionId`、`dbSessionId`、database/schema/table context。

`ConnectionNavigatorTree`/`useNavigatorContextMenus`/`ContentView` → `ConnectionPage.viewActions` → `usePanelHandlers.handleOpenTableAction` → `handleSelectTable` 或 `handleNewQuery` → `panelStore`/`PanelContentRenderer`/现有 `TablePanel`、`QueryPanel`。页面不拼接方言 SQL；既有非本轨管理危险操作路径保持原状。

### C. Pending changes

`TableView` → `tableDataStore.stageCellChange` / `stageRowDelete` → pending count bar → immutable preview plan/fingerprint/SQL/参数摘要 → commit confirmation → `commitPendingChanges`，Rollback 调用 `rollbackPendingChanges`。DataTable 的 Delete/Backspace 仍只进入 staged 集合；store 对无主键或不稳定 row identity 拒绝静默 UPDATE/DELETE，右键危险 action 的确认保持不变。

### D. Filter / pagination

TableView quick expression → `parseFilterForApply(columns)` → `filterExpressionToConditions`/结构化 conditions → `tableDataStore.setFilters(filters, logic)`；非法语法、未知列及不支持的混合逻辑在发请求前被拒绝，filter 变化重置 page=0。既有 `FilterEditor`/`FilterBar`/`Pagination`/context-menu root/二级层次继续走 DataTable/TableView；tableDataStore 的 request revision 和 loading guard 防止旧响应覆盖新结果及 busy 状态变更。原始 filter 文本没有进入 SQL 拼接。

### E. Query execution / result / error / AI

`QueryPanel` → `toQueryExecutionViewModel` → `QueryExecutionStatus`：显示 Running/Cancelling/terminal、耗时、rows/affected rows、error 状态和精确 cancel capability。取消请求仍只带 executionId + dbSessionId；只有 stream/promise 终态转移后才显示 Cancelled，SQLite/unknown capability 不被伪称支持。

查询结果 → 本轨接入的 `result-workspace/ResultWorkspace`：Table/Chart 共享同一 StatementResult，chart 不可用时回落 Table，图表数据点返回 Table row detail。`QueryContextSelectors` 压缩为保留 database/schema 的 breadcrumb/context selector。

`QueryErrorPanel` → `buildQueryDiagnosisContext`/`buildExplainAction`/`buildFixSqlAction`/`buildRetryAction`：Copy Error、Explain、Fix SQL、Retry 均接入；AI 上下文脱敏并受结构/结果上限保护，Fix 仅回填草稿，Retry 校验 fingerprint/SQL/参数并经过用户确认。

本轮安全修复收紧了 `QueryPanel → DiagnosisPanel → useAiStore → aiCommands` 诊断边界：DiagnosisPanel 只消费 `buildQueryDiagnosisContext` 的有效结果，并以 `safeSql`/`safeErrorMessage` 组装 AI payload；缺失/无效 context 不发送诊断。原始 SQL 仍保留在 context 中，仅用于必要的错误 UI 与 Fix draft-only 编辑器回填；现有单一脱敏 helper 同时覆盖转义 JSON assignment。

## 文件边界

共享页面：

- `src/windows/connection/ConnectionNavigatorTree.tsx`
- `src/windows/connection/ConnectionPage.tsx`
- `src/windows/connection/ContentView.tsx`
- `src/windows/connection/PanelContentRenderer.tsx`
- `src/windows/connection/QueryPanel.tsx`
- `src/components/ai/DiagnosisPanel.tsx`
- `src/lib/aiQueryActions.ts`
- `src/windows/connection/TableView.tsx`
- `src/windows/connection/usePanelHandlers.ts`
- `src/windows/connection/navigator/GlobalObjectSearch.tsx`
- `src/windows/connection/navigator/NavigatorToolbar.tsx`
- `src/windows/connection/navigator/types.ts`
- `src/windows/connection/navigator/useNavigatorContextMenus.ts`
- `src/windows/connection/result-workspace/{ResultWorkspace,ResultTableView,resultWorkspaceHelpers,index}.tsx/ts`
- `src/components/query/{QueryContextSelectors,QueryErrorPanel,QueryExecutionStatus}.tsx`
- `src/lib/connectionViews/types.ts`
- `src/stores/{panelStore,tableDataStore}.ts`

测试与文案：

- `src/windows/connection/__tests__/PageIntegration.test.tsx`
- `src/components/ai/__tests__/DiagnosisPanel.test.tsx`
- `src/stores/__tests__/aiStore.test.ts`
- `src/windows/connection/__tests__/QueryPanel.executeCancel.test.tsx`
- `src/locales/en.ts`
- `src/locales/zh-CN.ts`

## 验证状态

- `pnpm exec vitest run`：269 个文件，2214/2214 通过。
- 诊断安全边界定向回归：4 个文件，65/65 通过（包含 DiagnosisPanel、QueryPanel、aiStore 和 aiQueryActions）。
- 共享页面定向回归：6 个文件，99/99 通过。
- 页面接线新增集成测试：1 个文件，2/2 通过。
- `pnpm typecheck`：通过（0 diagnostics）。
- `node scripts/generate-builtin-locales.mjs`：通过；生成的 ignored `src/locales/builtinLocales.ts` 未提交。
- `git diff --check`：通过。
- Host UI E2E：本轨未运行。需要 `pnpm tauri build --debug --features webdriver`、桌面 Webdriver 和真实数据库 fixture；当前环境没有可用的桌面自动化/fixture，因此按 R 轨要求在 `bugs.md` 登记例外，未声称通过。建议后续执行连接发现、对象搜索→表动作、pending preview/commit/rollback、filter/pagination、query cancel/result/AI 五段 journey。

## 2026-08-31 独立复测（编码 21646559 / 修复 09c90bb2）

- 复测前运行 `node scripts/generate-builtin-locales.mjs` 成功；`src/locales/builtinLocales.ts` 为 ignored 生成物，未产生待提交变更。
- 诊断安全边界及页面相关定向回归：31 个文件，342/342 通过；覆盖 DiagnosisPanel → aiStore → aiCommands、QueryPanel 错误动作、executionId/dbSessionId 取消终态、ResultWorkspace、Connection/Object search、TablePanel context、Filter/Pagination/DataTable wiring。
- 完整 Host Vitest：269 个文件，2214/2214 通过。
- `pnpm typecheck`：通过（0 diagnostics）。
- `git diff --check`：通过。
- 静态复核 21646559..09c90bb2：未发现新增 driver id 或方言分支；E2E 仍因无 Tauri/Webdriver/真实 DB fixture 登记 R，未声称通过。
- 复测发现 `QueryPanel` Retry 确认后的最新 context 漏传 `schemaContext`，导致 schema 已加载时 fingerprint 改变，重试被最终校验拦截；详见 `bugs.md` 的 BUG-PI-005。本轮仅记录，未修改功能代码。

## 上轮复测收尾

页面接线与本轮诊断安全边界的复测证据已完成，但 Retry 存在 BUG-PI-005；本轮仅提交测试台账，不包含 `hub.md` 或功能代码。

## 2026-08-31 BUG-PI-005 修复

- 在 `QueryPanel.tsx` 抽取共享 diagnosis context builder，Explain/Fix/Retry 初始 context 与 Retry 确认后的 latest context 统一携带 database/schema、connectionId、dbSessionId、connectionContext 和 `{ tables, views, columns }` schemaContext。
- Retry 确认后从 schema store 读取最新 snapshot，仍通过既有 `retryAction.invoke` fingerprint、SQL、params 三重校验；context、SQL 或参数变化继续阻止重试，未直接调用 `runExecute` 绕过校验。
- 新增 QueryPanel 跨确认状态回归：schema context 不变确认后执行一次；schema、SQL、bound params 变化确认后均不执行。DiagnosisPanel 安全 payload、Fix draft-only、原始 SQL 仅用于编辑器的既有覆盖保持不变。
- 验证：locale 生成通过；QueryPanel 定向 1 file / 12 passed；AI/页面相关定向 4 files / 31 passed；`pnpm typecheck` 通过（0 diagnostics）；`git diff --check` 通过。
- 本轮仅修改 QueryPanel 共享页面功能、其回归测试及本轨 `progress.md`/`bugs.md`；工作区既有 `hub.md` 类型变更保持原样且未纳入提交。

## 收尾

BUG-PI-005 已修复并完成定向回归，待提交 `fix(query): preserve retry context fingerprint`；Host UI E2E 仍按 BUG-PI-001 记录，未在当前无桌面/真实数据库 fixture 环境声称通过。

## 2026-08-31 全新独立复测（21646559 / 09c90bb2 / 3950508e / 9a525c73）

- 基线确认：`feature/v01x-page-integration` HEAD 为 `3950508e`；四个目标提交均在当前分支祖先链中。既有 `docs/development/coordination/hub.md` 工作区改动保持原样，未触碰。
- `node scripts/generate-builtin-locales.mjs`：通过；`src/locales/builtinLocales.ts` 为 ignored 生成物，验证结束清理，不纳入提交。
- 定向 Host Vitest：31 个文件，432/432 通过；覆盖 QueryPanel diagnosis context、Explain/Fix/Retry、schemaContext tables/views/columns、确认后单次执行、schema/SQL/bound params 变化阻断、AI 脱敏 payload、Fix draft-only、取消/终态、Connection/Object search、TablePanel/ResultWorkspace 接线、Filter/Pagination/DataTable 与 context selector。
- 完整 Host Vitest：269 个文件，2218/2218 通过。
- `pnpm typecheck`：通过（退出码 0，0 diagnostics）；E2E contract 纯逻辑 Vitest：3 个文件，22/22 通过；`git diff --check`：通过。
- 静态复核确认共享 builder 在初始 Explain/Fix/Retry 及 Retry 确认后的 schema snapshot 中统一保留 `{ tables, views, columns }`，诊断只把 `safeSql`/`safeErrorMessage` 送入 AI，原始 SQL 仅用于 Fix 草稿/Retry 校验；同时发现确认等待期间 panel 的 database/schema/session/connection props 可能被旧闭包复用，详见新增 BUG-PI-006。本轮只记录，不修复。
- E2E R：`pnpm e2e:skip-build -- --suite core` 退出码 1；无 `dist/index.html`、无 Tauri webdriver debug binary，4445/5432/3306 均不可用，因此未声称 Host E2E 通过。
- 本轮仅更新本轨 `progress.md`/`bugs.md` 作为测试台账；未修改 hub、功能代码、配置或 codegen。
