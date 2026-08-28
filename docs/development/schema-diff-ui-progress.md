# Schema Diff UI 重构（方案 B）— 进度台账

> 协调者维护总览与 bug 台账；各轨道代理只写本功能小节。  
> 规格：[schema-diff-ui-redesign.zh-CN.md](../features/schema-diff-ui-redesign.zh-CN.md) §4 方案 B

## 1. 功能总览

| 编号 | 功能 | 轨道 | 状态 | 编码 commit | 测试 commit |
|------|------|------|------|-------------|-------------|
| F1 | EndpointsBar + dedicated db/schema 会话 | schema-diff-ui-endpoints | 编码完成 | 79357dec | — |
| F2 | 双栏面板（左表/diff · 右 plan/deploy） | schema-diff-ui-panels | 编码完成 | a0eca1f8 | — |
| F3 | SchemaDiffWindow 集成 + bg-surface shell | schema-diff-ui-b | 编码完成 | c80c1030 | — |
| F4 | Host 单测 | schema-diff-ui-tests | 未开始 | — | — |
| F5 | E2E 适配 | schema-diff-ui-e2e | 未开始 | — | — |

## 2. Bug 台账

| Bug ID | 所属 | 描述 | 状态 | 记录时间 |
|--------|------|------|------|----------|
| — | — | — | — | — |

## 3. 测试约定

- Host 单测：`npx vitest run src/windows/schema-diff`
- E2E 套件：`pnpm e2e:schema-diff`（R 阶段统一 `--skip-build` 或 `:build`）
- 方案 B 验收：Sync 式双栏；EndpointsBar 常驻；Deploy 在右栏/抽屉；保留 DEPLOY token

## 4. 功能小节

### F1 schema-diff-ui-endpoints

- **范围**：`SchemaDiffEndpointsBar.tsx`、`useSchemaDiffEndpoints.ts`（新文件，勿改 `SchemaDiffWindow.tsx`）
- **验收**：
  - [x] 源/目标连接 + database + schema（有则显示）Select
  - [x] Swap 按钮交换源/目标
  - [x] Compare 主按钮在 bar 内
  - [x] testid：`schema-diff-source`、`schema-diff-target`、`*-database`、`*-schema`
  - [x] 复用 Sync EndpointsBar 视觉 token（`border-edge px-6 py-4`）
- **测试**（`npx vitest run src/windows/schema-diff`）：
  - `SchemaDiffEndpointsBar.test.tsx`：3 passed（渲染 testid、swap/compare 回调、空 schema 隐藏）
  - Hook 单测：F4 轨（集成后补 `useSchemaDiffEndpoints` mock 覆盖）
- **F3 集成遗留**：
  - `SchemaDiffWindow` 接入 `useSchemaDiffEndpoints` + `SchemaDiffEndpointsBar`；移除内联连接 Select
  - Compare/Plan/Deploy 改用 hook 的 `ensureConnected('source'|'target')` 与 `validateEndpoints()`
  - 表名 placeholder 可默认前缀 `sourceSchema.`（PG）；config import/export 需写入 database/schema 字段（v3？）
  - `busy` 由 window 传入（compare/plan/deploy loading）

### F2 schema-diff-ui-panels

- **范围**：`SchemaDiffTableListPanel.tsx`、`SchemaDiffRightPanel.tsx`（新文件；`SchemaDiffDeployDrawer` 延后至 F3 集成评估）
- **验收**：
  - [x] 左栏：表列表 + 选中高亮 + diff 摘要/badge
  - [x] 右栏：Plan tab（SQL 列表 + options）+ Deploy 区（含 DEPLOY 输入）
  - [x] Props 驱动，无 window 状态耦合
  - [x] testid 覆盖 list、plan、deploy
- **导出接口**：
  - `SchemaDiffTableListPanelProps` — `tables`, `selectedTable`, `onSelect`, `tableHasDiff?`
  - `SchemaDiffRightPanelProps` + `SchemaDiffRightPanelTab` — tab 状态 + plan/deploy 全套回调
- **单测**：`src/windows/schema-diff/__tests__/SchemaDiffPanels.test.tsx`

### F3 schema-diff-ui-b（集成）

- **范围**：`SchemaDiffWindow.tsx` 重构为 Sync 式布局；接入 F1/F2 组件
- **验收**：
  - [x] `bg-surface`；无双栏滚动堆叠旧 UI
  - [x] Limitations 弹窗仍可用
  - [x] 行为回归：compare → plan → deploy 闭环
- **测试**（`npx vitest run src/windows/schema-diff`）：
  - `SchemaDiffWindow.test.tsx`：smoke（shell + endpoints + 双栏 testid）
- **E2E testid 变更（F5 适配）**：
  - **移除**：`schema-diff-step-review`（原 Plan 卡片内 Review 按钮；改点 `schema-diff-deploy-tab`）
  - **新增**：`schema-diff-detail-panel`、`schema-diff-table-list`、`schema-diff-table-row-{name}`、`schema-diff-right-panel`、`schema-diff-plan-tab`、`schema-diff-deploy-tab`、`schema-diff-plan-panel`、`schema-diff-deploy-panel`、`schema-diff-swap`、`*-database`、`*-schema`
  - **保留**：`schema-diff-window`、`schema-diff-compare`、`schema-diff-generate-plan`、`schema-diff-tables-input`、`schema-diff-allow-destructive`、`schema-diff-include-indexes`、`schema-diff-deploy`、`schema-diff-limitations*`

### F4 / F5 测试轨

- 编码合并后启动；E2E 用例变更登记于本文件
