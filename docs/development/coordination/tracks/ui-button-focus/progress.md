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

- Phase: PASSED
- 编码 commit: ba1fa2b12
- 测试 commit: 641c748bc
- 合并 commit: 待合入

## 自验结果（Coder）

| 套件 | 结果 |
|------|------|
| `npx vitest run src/components/ui` | 7 files, 35 tests passed |
| `npx tsc --noEmit` | 无类型报错 |

## 独立复验结果（Tester）

| 套件 | Coder 自报 | Tester 实测 |
|------|-----------|-------------|
| `npx vitest run src/components/ui` | 7 files / 35 tests | **7 files / 40 tests** ✅ |
| `npx tsc --noEmit` | 无报错 | **无报错** ✅ |
| 改动文件行覆盖率（Button + ToolbarButton） | — | **100% lines / 100% branches** ✅ |

### 代码审查（Phase A）

| 验收项 | 结论 |
|--------|------|
| `Button.tsx` 移除全局 `onMouseDown preventDefault` | ✅ 已移除；`{...props}` 直传，用户 `onMouseDown` 可正常挂载 |
| `focus-visible:ring-2 focus-visible:ring-accent/60` | ✅ 基础 `cn()` 含三档 focus-visible 类 |
| `ToolbarButton` 默认 `variant='ghost'` | ✅ 解构默认 + 显式 `variant={variant}` 传给 `Button` |
| Select/Dialog 局部 preventDefault | ✅ 未改动；属下拉/对话框专用，不在本轨范围 |

### 新增测试（Phase C）

Tester 在 `Button.test.tsx` 增补 5 条 `[tester]` 用例：

- 键盘可聚焦（programmatic focus / Tab 可达性代理）
- Button `secondary` / `danger` variant 渲染
- ToolbarButton 显式 `primary` override
- ToolbarButton `compact` 模式 `sr-only` 标签
- ToolbarButton 显式 `title` 属性

### E2E 登记

| 用例 | 状态 | 说明 |
|------|------|------|
| 工具栏按钮点击（ContentToolbar / QueryPanel） | 【留待 R 回归】 | 现有 `e2e/specs/*` 已通过 ToolbarButton 间接覆盖点击路径；焦点环视觉回归无专用 spec |
| Select 下拉不抢 Button 焦点 | 【本机可执行·间接】 | `Select.test.tsx` 保留局部 preventDefault；与 Button 变更正交 |

## 改动摘要

- `Button.tsx`：删除 `onMouseDown preventDefault`；基础样式加入 `focus-visible` 焦点环。
- `ToolbarButton.tsx`：默认 `variant='ghost'` 并显式传递给 `Button`。
- `Button.test.tsx`：更新焦点行为断言；新增 ToolbarButton ghost 默认样式测试；Tester 增补 5 条边界用例。

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 BOOTSTRAP：worktree 自检通过，分支 `feature/ui-button-focus`
- 2026-09-04 CODING 完成：commit ba1fa2b12，三件套自验通过
- 2026-09-04 TESTER BOOTSTRAP：worktree `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-ui-button-focus`，分支 `feature/ui-button-focus`
- 2026-09-04 TEST_DONE：独立复验通过，无 Bug 登记
