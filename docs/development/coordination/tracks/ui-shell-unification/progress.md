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

- Phase: PASSED
- 编码 commit: 80e316d6a
- 测试 commit: 29d7907ea
- 合并 commit: 待合入

## 自验结果（编码代理）

- `npx vitest run src/windows/settings src/windows/connection` — 40 files, 361 tests passed
- `npx tsc --noEmit` — passed

## 独立复验结果（测试代理）

- BOOTSTRAP: worktree `.worktrees/datazen-ui-shell-unification`，分支 `feature/ui-shell-unification`，编码基线 `80e316d6a`
- `npx vitest run src/windows/settings src/windows/connection` — **40 files, 363 tests passed**（+2 tester 增补）
- `npx tsc --noEmit` — passed
- 改动文件覆盖率（独立实测）：
  - `ContentStatusBar.tsx` — **100% lines**（macOS / Windows / Linux / unknown 分支）
  - `ConnectionPage.tsx` — 50.15% lines（全文件，既有基线；本轨变更路径：TitleBar 常驻、settings 嵌入、返回恢复 mode 均已单测/E2E 覆盖）
- E2E：`e2e/specs/settings.ts` F1-E2E-001~003 覆盖 settings 壳层导航（MenuBar 保留、返回、section 深链）— 【留待 R 回归】

## 改动摘要

1. **ConnectionPage**：TitleBar（含 MenuBar）始终渲染；设置视图嵌入 `SettingsContent`，左侧 TitleBar 增加返回按钮。
2. **ContentStatusBar**：通过 `usePlatform()` 显示 Mac `⌘N/⌘W` 或 Win/Linux `Ctrl+N/Ctrl+W`。
3. **测试**：ConnectionPage / ContentStatusBar 单测更新以覆盖 MenuBar 保留与平台快捷键；Tester 增补 settings TitleBar 标题与 unknown 平台分支。

## 心跳

- 2026-09-04 轨道初始化
- 2026-09-04 编码完成，自验通过，Phase → READY_FOR_TEST
- 2026-09-04 测试代理独立复验通过，Phase → PASSED
