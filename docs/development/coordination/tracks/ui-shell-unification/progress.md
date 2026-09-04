# Track `ui-shell-unification` — progress

> Initiative: UI/UX Enhancement
> Plan: `docs/development/coordination/ui-enhancement-plan.md`（Wave 2）
> Hub: `docs/development/coordination/hub.md`（只读，禁改）
> Integration branch: `feat/ui-enhancement`

## 范围

外壳与导航打磨：
- SettingsPage 保持与主窗口一致的 TitleBar（含 MenuBar），避免切到设置页时系统菜单消失；返回按钮融入左侧。
- ContentStatusBar 快捷键提示增加平台感知（通过 `usePlatform()` 区分 Mac 的 `⌘` 与 Win/Linux 的 `Ctrl`）。
- 状态栏按活动面板状态渲染，避免切换面板后信息错位。

## 状态

- Phase: READY_FOR_TEST
- 编码 commit: 36bf6c9a0
- 测试 commit: 待测试
- 合并 commit: 待合入

## 自验结果

- `npx vitest run src/windows/settings src/windows/connection` — 40 files, 361 tests passed
- `npx tsc --noEmit` — passed

## 改动摘要

1. **ConnectionPage**：TitleBar（含 MenuBar）始终渲染；设置视图嵌入 `SettingsContent`，左侧 TitleBar 增加返回按钮。
2. **ContentStatusBar**：通过 `usePlatform()` 显示 Mac `⌘N/⌘W` 或 Win/Linux `Ctrl+N/Ctrl+W`。
3. **测试**：ConnectionPage / ContentStatusBar 单测更新以覆盖 MenuBar 保留与平台快捷键。

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 编码完成，自验通过，Phase → READY_FOR_TEST
