# UI/UX Enhancement — 实施计划

> **来源**：2026-09-04 DataZen 全应用 UI/UX 评审报告与 P0 治理项。
> **Playbook**：`docs/development/subagent-dev-playbook.md` + `docs/development/subagent/`（Coordinator/Coder/Tester/Rescuer）。
> **集成分支**：`feat/ui-enhancement`（各轨 `feature/ui-*` 合入此分支）。
> **协调总览**：`docs/development/coordination/hub.md`（只读生成物，`node scripts/aggregate-hub.mjs`）。
> **轨目录**：`docs/development/coordination/tracks/<track-id>/`（各轨仅维护本轨 `progress.md` + `bugs.md`）。

---

## 0. 角色与基线约定

| 角色 | 约束 |
|------|------|
| 协调者 | 不写业务代码；维护 hub；merge / worktree 清理；写锁台账；关键节点向用户同步 |
| 编码代理 | 仅在 `.worktrees/datazen-<track>`；`scripts/new-feature-worktree.sh <track> feat/ui-enhancement`；全新实例 |
| 测试代理 | 全新实例（禁复用 Coder）、独立 worktree；只测不修；零信任；覆盖率 ≥80% |
| 基线 | 所有轨道 base = `feat/ui-enhancement` |

---

## 1. 波次编排

### Wave 1（2 轨并行：写路径完全互斥）— 已完成 (PASSED)

| Track | 问题项 | 任务摘要 | 主要写路径 | 状态 |
|-------|--------|----------|------------|------|
| **ui-button-focus** | P0-4 | 修 Button 焦点与 WCAG 可访问性：移除 Button 全局 onMouseDown preventDefault，补充 focus-visible:ring-2 ring-accent/60；ToolbarButton 默认 variant 改为 ghost | `src/components/ui/Button.tsx`、`src/components/ui/ToolbarButton.tsx` | **PASSED** (合入) |
| **ui-transfer-guards** | P0-2, P0-3 | DataTransfer 向导预览与空表守卫：preview 失败/为空时显示错误提示并提供重试与返回；objects 步 0 表禁用 Next 并提供重新检测提示 | `src/windows/data-transfer/DataTransferWindow.tsx` | **PASSED** (合入) |

### Wave 2（2 轨并行：强调色收敛 + 导航外壳统一）— 已完成 (PASSED)

| Track | 任务摘要 | 主要写路径 | 状态 |
|-------|----------|------------|------|
| **ui-accent-tokens** | 统一强调色到 `accent` token：清理 `focus:border-blue-500` 与 `text-blue-500` 等硬编码，Badge/Tag 与危险色统一使用语义 token，补齐样式回归测试 | `src/windows/settings/`、`src/components/` 等相关样式文件 | **PASSED** (合入) |
| **ui-shell-unification** | 外壳与导航打磨：SettingsPage 接入主 TitleBar 与菜单，ContentStatusBar 快捷键平台感知（Cmd/Ctrl 自适应）与上下文匹配 | `src/windows/connection/ConnectionPage.tsx`、`src/windows/settings/SettingsPage.tsx`、`src/windows/connection/ContentStatusBar.tsx` | **PASSED** (合入) |


