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

- Phase: TEST_DONE
- 编码 commit: ad1f0b0e8
- 测试 commit: 9fa32bc0f
- 合并 commit: 待合入

## 自验（编码代理）

- `npx vitest run src/windows/connection` — 320 passed
- `npx vitest run src/lib/__tests__/windowManager.test.ts` — 12 passed
- `npx tsc --noEmit` — pass

## 独立复验（测试代理）

- `npx tsc --noEmit` — pass
- `npx vitest run src/windows/connection src/lib/__tests__/windowManager.test.ts` — **37 files, 335 tests passed**
- 改动文件覆盖率（vitest --coverage，含增补测试）：
  - `windowManager.ts` — Stmts **88.07%** / Lines **93.54%**
  - `QueryPanel.tsx`（变更路径：取消即时显示、ResultMessageDialog）— 核心变更路径已覆盖；全文件 Lines 57.07%（文件体量大，非本次变更范围）

## 增补测试

- `QueryPanel.resultMessageDialog.test.tsx` — Postgres 双引号提示与 dashboard 创建失败走 ResultMessageDialog
- `windowManager.test.ts` — 单例已存在 focus/unminimize/show；inflight 重复打开时 focus 现有窗口

## E2E 登记

| 用例 | 状态 | 备注 |
|------|------|------|
| 查询执行后取消按钮立即可见 | 【留待 R 回归】 | 需真实 DB 连接 + 慢查询 |
| 菜单重复打开 data-sync 单例窗口前台激活 | 【留待 R 回归】 | 需 Tauri 桌面环境 |

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 编码完成：取消按钮即时显示、ResultMessageDialog 替换 alert、单例窗口 focus/unminimize
- 2026-09-04 测试完成：独立复验通过，覆盖率达标，无 Bug 登记
