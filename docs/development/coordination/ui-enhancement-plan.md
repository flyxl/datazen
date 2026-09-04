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

### Wave 3（2 轨并行：迁移向导统一 + 查询工具栏体验）— 已完成 (PASSED)

| Track | 任务摘要 | 主要写路径 | 状态 |
|-------|----------|------------|------|
| **ui-wizard-prefill** | 迁移三件套（Sync/Transfer/Diff）向导守卫与参数预填统一：windowManager 统一 prefill，DataSync 改端点保留已有配置选项，只读目标给出提示阻断 | `src/lib/windowManager.ts`、`src/windows/data-sync/DataSyncWindow.tsx`、`src/windows/schema-diff/SchemaDiffWindow.tsx` | **PASSED** (合入) |
| **ui-query-toolbar** | 查询工具栏体验优化：运行即显取消按钮（消除 300ms 延迟与布局跳动）；window.alert 绕过设计系统治理，单例窗口重复打开时自动聚焦与还原 | `src/windows/connection/QueryPanel.tsx`、`src/lib/windowManager.ts` | **PASSED** (合入) |

---

## 2. 成果与状态总结

全量波次已按照并发 Playbook 标准执行完毕：
- **P0-1（GlobalObjectSearch）**：清理入口及休眠状态，无残留无效计算。
- **Wave 1**：P0-4（Button 焦点与可访问性）+ P0-2/P0-3（DataTransfer 向导守卫）全部通过测试并合流。
- **Wave 2**：统一强调色至 accent token + SettingsPage 保留系统菜单与平台快捷键全部通过测试并合流。
- **Wave 3**：迁移向导三件套 prefill 统一与只读阻断 + 查询工具栏消除 300ms 延迟与单例窗口激活全部通过测试并合流。
- **R 阶段回归**：全量编译检查 `npx tsc --noEmit` 0 报错；受影响模块测试（60 个测试文件、484 个测试）100% 通过。




