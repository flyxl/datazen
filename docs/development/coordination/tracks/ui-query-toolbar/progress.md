# Track `ui-query-toolbar` — progress

> Initiative: UI/UX Enhancement
> Plan: `docs/development/coordination/ui-enhancement-plan.md`（Wave 3）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/ui-enhancement`

## 范围

查询工具栏体验优化与交互规范化：
1. `src/windows/connection/QueryPanel.tsx`：
   - 消除取消按钮 300ms 延迟：执行查询时立即显示“取消”按钮，不再延迟 300ms 后才切换，消除最需要取消时点不了以及工具栏宽度跳动的问题。
   - 替换 `window.alert`：使用现有的设计系统对话框（`ResultMessageDialog` / `onShowMessage`）替代裸 `window.alert`。
2. `src/lib/windowManager.ts`：
   - 单例窗口防重复打开优化：当单例子窗口已存在时，调用 `setFocus()` 与 `unminimize()` 将已存在的窗口恢复并聚焦，避免用户二次点击菜单时“毫无反应”。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: ad1f0b0e8
- 测试 commit: 待测试
- 合并 commit: 待合入

## 自验

- `npx vitest run src/windows/connection` — 320 passed
- `npx vitest run src/lib/__tests__/windowManager.test.ts` — 12 passed
- `npx tsc --noEmit` — pass

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 编码完成：取消按钮即时显示、ResultMessageDialog 替换 alert、单例窗口 focus/unminimize
