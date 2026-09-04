# Track `ui-button-focus` — progress

> Initiative: UI/UX Enhancement
> Plan: `docs/development/coordination/ui-enhancement-plan.md`（Wave 1）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/ui-enhancement`

## 范围

P0-4：Button 焦点与可访问性治理
- 移除 Button 的全局 `onMouseDown` 吞焦点行为（`e.preventDefault()`），保留正常的 DOM 焦点与可访问性。
- 在 `Button` 基础样式上补充清晰的键盘焦点环：`focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60`（若有自定义 ring 则兼容）。
- `ToolbarButton` 的默认 `variant` 调整为 `'ghost'`，避免未指定时错误掉进 `primary` 强色实心蓝样式。
- 保证已有的 Select/Dropdown、按钮事件、表单组件交互行为正常，无样式与事件破损。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: ba1fa2b12
- 测试 commit: 待测试
- 合并 commit: 待合入

## 自验结果（Coder）

| 套件 | 结果 |
|------|------|
| `npx vitest run src/components/ui` | 7 files, 35 tests passed |
| `npx tsc --noEmit` | 无类型报错 |

## 改动摘要

- `Button.tsx`：删除 `onMouseDown preventDefault`；基础样式加入 `focus-visible` 焦点环。
- `ToolbarButton.tsx`：默认 `variant='ghost'` 并显式传递给 `Button`。
- `Button.test.tsx`：更新焦点行为断言；新增 ToolbarButton ghost 默认样式测试。

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 BOOTSTRAP：worktree 自检通过，分支 `feature/ui-button-focus`
- 2026-09-04 CODING 完成：commit ba1fa2b12，三件套自验通过
