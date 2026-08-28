# DataZen 全应用 UI 审计报告 #4

> 独立审计，无分工协作  
> 审计日期：2026-08-28

## 概述

DataZen 是一款面向数据库工程师的 Tauri v2 + React 桌面工具，整体采用**暗色优先的语义化设计系统**（`src/styles/themes.css` + Tailwind 语义 token），主壳层为 `TitleBar` + `MenuBar`（Windows/Linux）+ 左侧 40px 模式轨 + 可变宽侧栏 + 内容区。应用从 `MainPage` 分流至 `WelcomePage` 或 `ConnectionPage`，后者再嵌入连接工作区、Workflow、Dashboard、Workspace 插件页、插件管理等子模式。

**总体印象**：视觉风格统一、信息密度适中，偏专业工具向；核心连接/SQL/DataTable 体验成熟，空状态与 i18n 基础设施较好。主要短板集中在**无障碍（a11y）**、**对话框交互规范**、**设置保存模式不一致**、**部分模块硬编码英文**，以及若干超大组件带来的 UX 一致性与可维护性风险。

## 优点

1. **设计系统扎实** — token 体系支持亮/暗主题与主题包扩展。
2. **壳层跨平台处理细致** — TitleBar、WindowControls、MenuBar 平台差异处理好。
3. **主工作区导航清晰** — 左侧 40px 图标轨模式切换直观；ConnectionWorkspaceHome 降低冷启动成本。
4. **共享 UI 组件质量较高** — Button、Select、WebContextMenu、DataTable 能力完整。
5. **空/未配置状态有引导** — Welcome、AiChatPanel、WorkspaceDefaultCards、ExtensionManagementPage。
6. **i18n 覆盖广** — 绝大多数页面通过 useI18n() 驱动。
7. **E2E 友好** — 大量 data-testid。
8. **可调整布局** — 侧栏、AI 面板等使用 useResizable 并持久化宽度。
9. **错误可复制** — CopyableError、右键菜单丰富。
10. **插件体系 UI 完整** — ExtensionManagementPage 卡片式列表、API 版本 Badge、权限展示。

## 问题清单

| 严重级别 | 位置 | 问题 | 建议 |
|---------|------|------|------|
| **critical** | `ErrorBoundary.tsx` | 崩溃页全部硬编码英文 | 接入 i18n |
| **critical** | `Dialog.tsx` | 无 Esc 关闭、无 focus trap | 增加 focus trap + Esc 关闭 |
| **major** | `WorkflowForm.tsx` | 大量硬编码英文 | 迁入 en.ts/zh-CN.ts |
| **major** | `ConnectionPage.tsx` WorkspaceModeButton | 无 aria-label | 添加 aria-label + aria-current |
| **major** | `PanelTabBar.tsx` | 关闭按钮 hover 才可见 | 始终显示或 focus 时显示 |
| **major** | `SettingsContent.tsx` | 保存模式分裂 | 统一即时生效或全局 draft |
| **major** | `SettingsContent.tsx` LOG_LEVEL_OPTIONS | 硬编码英文 | 使用 i18n |
| **major** | `WindowControls.tsx` | aria-label 硬编码英文 | 使用 i18n |
| **major** | `ConnectionPage.tsx` | 无显式连接 Tab 条 | 增加连接 Tab 条或状态标注 |
| **major** | `MenuBar.tsx` | 子菜单仅 hover 展开 | 增加键盘导航 |
| **major** | `ConnectionNavigatorTree.tsx` | ~2700+ 行 | 拆分子组件 |
| **major** | `WorkflowPage.tsx`、`QueryPanel.tsx` | ~1400+ 行 | 拆分 + 统一 AsyncState |
| **minor** | `ConnectionPage.tsx` L851-872 | inline bg-blue-500 button | 统一 Button |
| **minor** | `ThemeToggle.tsx` | 缺少 aria-expanded | 补齐 ARIA |
| **minor** | `Select.tsx` | 固定 LIST_ID | useId() |
| **minor** | 多处 focus 样式 | blue-500 vs accent 不统一 | 统一 accent token |
| **suggestion** | 全局 | 无快捷键帮助 | 增加 Cheatsheet |
| **suggestion** | 全局 | 无统一 toast | 引入 toast 层 |
| **suggestion** | 全局 | 无 skeleton | DataTable/树用 skeleton |

## 优先改进项 Top 10

1. 修复 Dialog 无障碍基线
2. 国际化 ErrorBoundary
3. WorkflowForm 硬编码英文清零
4. 左侧模式轨与 Panel Tab 的 a11y
5. 统一 Settings 保存/生效模式
6. 多连接会话的可视化 Tab
7. 拆分 ConnectionNavigatorTree / QueryPanel / WorkflowPage
8. WindowControls + ThemeToggle ARIA/i18n
9. 统一 focus ring 与 Button 组件使用
10. 增加快捷键帮助入口

## 综合评分

| 维度 | 评分 |
|------|------|
| 视觉设计 | 8.0 |
| 布局与信息架构 | 7.5 |
| 交互与 UX 流程 | 7.0 |
| 无障碍 (a11y) | 5.0 |
| 一致性与组件复用 | 7.0 |
| i18n | 7.5 |
| 空/加载/错误状态 | 7.5 |
| **综合** | **7.2 / 10** |
