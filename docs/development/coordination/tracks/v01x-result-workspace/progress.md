# F：Result Workspace 轨道进度

## 1. 功能摘要

- 范围：新增可复用 `ResultWorkspace`、`ResultTableView` adapter 及纯 view/capability helper；不接入 `QueryPanel` 或其他共享页面。
- 状态：编码完成，待提交。
- 编码 commit：待提交。
- 测试 commit：不适用（由编码代理完成定向自验；独立 R 回归留后续轨道）。

## 2. E2E 用例登记

| 用例 | 前置 | 步骤与断言 | 状态 |
| --- | --- | --- | --- |
| F-RW-001 | 查询返回含数值列的 active statement | 在同一结果中切换 Table/Chart；断言不重新执行查询且 Chart 配置仍被传入 | 【留待 R 回归】 |
| F-RW-002 | 查询返回空结果或无数值列 | 请求 Chart；断言 UI 降级为 Table，Chart 入口不可用 | 【留待 R 回归】 |
| F-RW-003 | 结果存在 row detail 定位 | 在 Table/Chart 触发行详情；断言定位回调收到相同行索引 | 【留待 R 回归】 |

本轨只新增可复用组件，未新增共享页面交互；真实桌面 E2E 由 R 在 I 接线后执行。

## 3. 测试结果与覆盖率

- `pnpm exec vitest run src/windows/connection/result-workspace/__tests__`：3 个测试文件，14 个测试通过。
- `git diff --check`：通过（提交前将再次检查 staged diff）。
- `pnpm exec tsc --noEmit`：被 worktree 基线缺失的生成文件 `src/locales/builtinLocales.ts` 阻断，并连带报告既有 `src/windows/settings/SettingsContent.tsx` 隐式 any；本轨未修改生成文件或该既有文件。
- Rust：不需要运行；本轨仅新增 React/TypeScript 组件和测试，没有 Rust 变更。
- 未单独运行覆盖率；本轨定向测试覆盖 helper 的所有降级分支及组件的 Table/Chart、配置、row detail、空/错误契约。

## 4. 设计决策 / 遗留注意

- `ResultWorkspace` 接收单个 active `StatementResult`；多 statement 的 index 选择继续由调用方负责，不合并结果。
- `ResultTableView` 复用 `DataTable`，不使用会触发数据库加载的 `TableView`，因此不会引入第二条 SQL 执行链。
- Chart 视图仅在结果可推断且调用方提供 `chartConfig` 时渲染；空结果、缺 config、无数值列均无副作用地实际降级到 Table。
- chart config、active view、row detail index 通过 props/callback 由调用方维护，切换组件不会清理这些状态。
- QueryPanel/ContentView/PanelContentRenderer/panelStore/shared locales/types 仍未修改，最终页面接线留给 I 轨；E2E 留待 R。
