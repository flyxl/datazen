# F：Result Workspace 轨道进度

## 1. 功能摘要

- 范围：新增可复用 `ResultWorkspace`、`ResultTableView` adapter 及纯 view/capability helper；不接入 `QueryPanel` 或其他共享页面。
- 状态：测试中（发现 2 个待验证 Bug）。
- 编码 commit：`feb7b4d5`。
- 测试 commit：本次验证提交（见最终回复）。

## 2. E2E 用例登记

| 用例 | 前置 | 步骤与断言 | 状态 |
| --- | --- | --- | --- |
| F-RW-001 | 查询返回含数值列的 active statement | 在同一结果中切换 Table/Chart；断言不重新执行查询且 Chart 配置仍被传入 | 【留待 R 回归】 |
| F-RW-002 | 查询返回空结果或无数值列 | 请求 Chart；断言 UI 降级为 Table，Chart 入口不可用 | 【留待 R 回归】 |
| F-RW-003 | 结果存在 row detail 定位 | 在 Table/Chart 触发行详情；断言定位回调收到相同行索引 | 【留待 R 回归】 |
| F-RW-004 | 首次 chartable 查询且 `chartConfig` 尚未生成 | 点击 Chart；断言入口可用、ChartView 生成推荐配置并通过 callback 保存 | 【留待 R 回归；当前 BUG-001】 |
| F-RW-005 | Chart 视图存在可点击数据点 | 点击数据点；断言切回 Table 且 row detail index 保留 | 【留待 R 回归；当前 BUG-002】 |

本轨只新增可复用组件，未新增共享页面交互；真实桌面 E2E 由 R 在 I 接线后执行。

## 3. 测试结果与覆盖率

- `pnpm exec vitest run src/windows/connection/result-workspace/__tests__`：3 个测试文件，14 个测试通过。
- 覆盖率（`--coverage` + `coverage.include`，报告写入 `/private/tmp/datazen-f1-coverage`）：总体行 `91.42%`、语句 `91.89%`、分支 `94.11%`；`ResultTableView.tsx` 行 `84.61%`、`ResultWorkspace.tsx` 行 `90.91%`、`resultWorkspaceHelpers.ts` 行 `100%`。
- `pnpm typecheck`：失败；worktree 基线缺失生成文件 `src/locales/builtinLocales.ts`，并连带报告既有 `src/windows/settings/SettingsContent.tsx` 隐式 any；本轨文件无报错，未修改生成文件。
- 全量 `pnpm exec vitest run`：266 个测试文件中 195 通过、71 失败；1563 个测试中 1393 通过、170 失败，另有 2 个 unhandled errors。失败主体由同一缺失 `src/locales/builtinLocales.ts` 导致，未将其归因于本轨；另有既有 `ConnectionPage` 关闭窗口断言失败，待 R 环境修复后复核。
- `git diff --check`：提交 diff 与当前工作树均通过。
- Rust：不需要运行；本轨仅新增 React/TypeScript 组件和测试，没有 Rust 变更。
- 定向测试覆盖 helper 的所有降级分支及组件的 Table/Chart、配置、row detail、空/错误契约；未覆盖首次缺省 chart config 的可用性和 Chart 数据点切回 Table。

## 4. 设计决策 / 遗留注意

- `ResultWorkspace` 接收单个 active `StatementResult`；多 statement 的 index 选择继续由调用方负责，不合并结果。
- `ResultTableView` 复用 `DataTable`，不使用会触发数据库加载的 `TableView`，因此不会引入第二条 SQL 执行链。
- Chart 视图当前仅在结果可推断且调用方提供 `chartConfig` 时渲染；空结果、缺 config、无数值列均无副作用地实际降级到 Table，但缺 config 与现有 ChartView 自动推荐语义冲突，已登记 BUG-001。
- chart config、active view、row detail index 通过 props/callback 由调用方维护，切换组件不会清理这些状态。
- Chart 数据点只转发 `onRowDetail`，没有复现现有 QueryPanel 的 `setResultViewMode('table')`，已登记 BUG-002。
- QueryPanel/ContentView/PanelContentRenderer/panelStore/shared locales/types 仍未修改，最终页面接线留给 I 轨；E2E 留待 R。
