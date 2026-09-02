# v0.1.2 UI Polish 实施计划

## 目标

收敛第二轮 UI/UX 评审中的 P0/P1 遗留项，保持现有 DataZen 暗色主题和功能语义，重点改善可访问性、错误反馈、设置离开保护、控件一致性和空/加载状态。

## 范围

- Dialog：焦点圈闭、Esc 关闭、关闭按钮无障碍名称、焦点恢复。
- ErrorBoundary：本地化文案和可操作错误反馈。
- Settings：统一 dirty/save 模型，未保存离开时给出确认。
- Navigation/controls：PanelTabBar、Select、MenuBar、WindowControls 的键盘与 ARIA 语义。
- DataTable：空态、加载态和可访问状态反馈。
- Workflow：消除 `window.alert`，统一错误/无效 YAML 反馈和 i18n。
- Visual consistency：将生产 UI 的 `blue-500` 硬编码收敛到 `accent` token；补齐 permission labels 和遗留文案。

## 轨道与冲突边界

1. `v012-i18n-contract`：先冻结新增 i18n keys；只修改 `src/locales/en.ts`、`src/locales/zh-CN.ts` 及 locale 测试。
2. `v012-dialog-errorboundary`：Dialog 与 ErrorBoundary；不修改共享 locale 文件。
3. `v012-settings-dirty`：Settings 保存/dirty/离开保护；不修改共享 locale 文件，使用轨道 1 的 keys。
4. `v012-navigation-controls`：PanelTabBar、Select、MenuBar、WindowControls；不修改共享 locale 文件。
5. `v012-datatable-workflow`：DataTable 空/加载状态与 Workflow alert/i18n；不修改共享 locale 文件。
6. `v012-accent-sweep`：前述轨道合并后执行生产代码的 accent token sweep、permission labels 和残留 UI 文案清理。

## 统一验收

- 新增/变更 UI 有组件或行为测试，变更 TS 文件行覆盖率目标不低于 80%。
- `pnpm typecheck`、Host Vitest、Driver UI Vitest 全绿。
- 不改变数据库、IPC、Driver API 或业务状态语义。
- 不在 UI 中新增 driver-id 方言分支。
- 每轨遵循“编码代理 → 全新测试代理 → 必要时修复代理”的状态机，并在本轨 `progress.md` / `bugs.md` 留痕。
- 全部轨道合并后统一执行 R 阶段回归；真实桌面 E2E 在各轨只登记，不在功能轮重复构建。
