# DataZen 全应用 UI 审计报告 #1

> 独立审计，无分工协作  
> 审计日期：2026-08-28

## 概述

DataZen 是基于 Tauri v2 + React 18 + Tailwind CSS 4 的跨平台桌面数据库工具。整体采用**暗色优先的语义化设计令牌**（`src/styles/themes.css`），通过 `--c-surface` / `--c-accent` / `--dt-*` 等 token 与运行时主题包扩展，视觉基调统一、信息密度偏高，符合专业 DBA 工具定位。

应用壳层由 `MainPage` → `WelcomePage` / `ConnectionPage` 分支构成；有连接后进入以 **左侧 40px 模式轨 + 可折叠连接导航 + 多 Panel 标签** 为主轴的统一工作区（`src/windows/connection/ConnectionPage.tsx`）。Workflow、Dashboard、Workspace 插件页、插件管理、Settings 均作为同一壳内的模式切换，迁移类工具（Data Sync / Schema Diff / Data Transfer / Backup）则为独立子窗口。

共享 UI 层（`src/components/ui/`、`DataTable/`、`ai/`、`chart/`）复用度较高：`Button`、`Dialog`、`Select`、`WebContextMenu` 等形成基础组件库；`DataTable` 虚拟滚动 + 过滤/分页/导出能力完整；AI 与图表模块嵌入 Query/Workflow 面板，而非孤立页面。

**主要印象**：视觉与交互模式在连接工作区内高度一致，i18n 基础设施完善（`useI18n` + `en.ts`/`zh-CN.ts`），加载/错误/空状态在多数模块有覆盖。短板集中在**无障碍（a11y）**、**对话框交互规范**、**少量硬编码英文**、**设计令牌使用不一致（`blue-500` vs `accent`）**，以及 Workflow / 连接导航等**超大组件带来的可维护性与认知负担**。

## 优点

1. **统一的设计系统** — `themes.css` 定义完整的 surface/fg/edge/accent/DataTable 色 token；主题包可覆盖 `--dt-*` 与 CodeMirror 语法色，Appearance 设置（`AppearanceSection.tsx`）与 `ThemeToggle` 联动良好。
2. **壳层导航清晰** — `ConnectionPage` 左侧模式轨（连接 / Workflow / Dashboard / Workspace / 插件 / 设置）图标 + `title` 提示，模式切换不丢上下文；`ConnectionWorkspaceHome` 提供快捷操作与最近 Panel，降低冷启动认知成本。
3. **Panel + Tab 模式成熟** — `PanelTabBar`（连接区）与 `WorkspaceTabBar` 视觉一致（底边 accent 指示条、横向滚轮滚动）；Query / Table / ER / 结构编辑等通过 `ContentView` + `panelStore` 统一管理，符合 IDE 式多文档习惯。
4. **DataTable 能力完整** — 虚拟行渲染、列宽拖拽、过滤条、分页、行选/Delete 键删除、右键菜单、导出对话框；`ToolbarButton` 在紧凑模式用 `sr-only` 保留可读标签，是可访问性方面的正面范例。
5. **Web 上下文菜单体系** — 项目约定禁止 Tauri 原生 `Menu.popup()`，统一走 `WebContextMenu`（`role="menu"`、Escape 关闭、视口边界定位）+ `contextMenuStore`。
6. **跨平台 TitleBar 处理细致** — `TitleBar.tsx` 区分 macOS overlay drag region 与 Win/Linux `startDragging()` 阈值拖拽；`WindowControls` 仅在非 macOS 渲染。
7. **i18n 覆盖广** — Welcome、Settings 各 Section、插件管理、Workspace 空状态、Chart 空状态、多数连接视图均走 `t()`。
8. **空/错/加载状态多数到位** — `MainPage` 连接加载 spinner + 加载失败重试；`ExtensionPageShell` 10s 加载超时 + 重载；`ChartEmptyState` 分原因提示。
9. **插件 Workspace 设计合理** — `WorkspaceView` + `ExtensionPageShell`（sandbox iframe、`datazen://` 协议、bridge RPC）+ 默认卡片网格。
10. **Welcome 首启体验** — `WelcomePage.tsx` 功能卡片 + 创建/导入连接 CTA，布局居中、层级清楚。

## 问题清单

| 严重级别 | 位置 | 问题 | 建议 |
|---------|------|------|------|
| **critical** | `src/components/ui/Dialog.tsx` | 对话框**不支持 Escape 关闭**、**无 focus trap/初始焦点** | 增加 Escape → `onClose`；引入 focus trap；关闭按钮补 `aria-label` |
| **critical** | `src/components/ErrorBoundary.tsx` | 全局错误 UI 文案硬编码英文 | 接入 `useI18n`；按钮改用 `Button` 组件 |
| **major** | `src/windows/workflow/WorkflowForm.tsx` | 多处硬编码英文 | 全部迁入 `en.ts` / `zh-CN.ts` |
| **major** | `src/components/MenuBar.tsx` | 快捷键显示 `Ctrl+N`，未按平台显示 `⌘` | 用 `usePlatform()` 动态渲染 |
| **major** | `src/windows/settings/SettingsContent.tsx` | 保存模型分裂；返回不检查未保存修改 | 返回前 `isDirty` 时弹出 ConfirmDialog |
| **major** | `src/components/DataTable/VirtualBody.tsx` | **`rows.length === 0` 时无空状态** | 增加居中空态 |
| **major** | `WorkflowPage.tsx`、`QueryPanel.tsx` | 多处使用 **`window.alert()`** | 改用 `ResultMessageDialog` / `CopyableError` |
| **major** | 全局 a11y | **`aria-*` / `role` 使用极少** | 图标按钮必配 `aria-label`；Tab 条用 `role="tablist"` |
| **major** | `ConnectionPage.tsx` L851-864 | 连接失败重试按钮使用 **`bg-blue-500`** 原生 button | 统一为 `<Button variant="primary">` |
| **major** | 多处 | 大量 **`bg-blue-500` / `text-blue-400`** 硬编码 | 批量替换为 semantic class |
| **minor** | `WindowControls.tsx` | `aria-label` 硬编码英文 | 使用 i18n key |
| **minor** | `SettingsContent.tsx` L29-35 | 日志级别选项硬编码英文 | 迁入 locale |
| **minor** | `ForeignKeysView.tsx` | 表头 "ON UPDATE" / "ON DELETE" 未 i18n | 增加 key |
| **minor** | `ExtensionManagementPage.tsx` L214 | `by ${plugin.author}` 硬编码英文 | 改为 i18n |
| **minor** | `ThemeToggle.tsx` | 无 `aria-label`/`aria-expanded` | 补充 aria 属性 |
| **minor** | `PanelTabBar.tsx` | 关闭按钮无 `aria-label` | 对齐 WorkspaceTabBar |
| **minor** | `ConnectionNavigatorTree.tsx` | ~2800+ 行单文件 | 按 concern 拆分 |
| **minor** | `WorkflowPage.tsx` | ~1400+ 行，触控目标偏小 | 拆分 sidebar / panel renderer |
| **suggestion** | 全局 | 无 skeleton 加载 | 大数据表/导航树首次加载用 skeleton |
| **suggestion** | 全局 | 无快捷键帮助面板 | Help 菜单或 `?` 打开快捷键面板 |

## 优先改进项 Top 10

1. Dialog 无障碍与交互规范（`Dialog.tsx`）
2. ErrorBoundary i18n
3. WorkflowForm 硬编码英文清零
4. 设置页未保存离开提示
5. 替换 `window.alert` 为应用内对话框
6. DataTable 空数据状态
7. 图标按钮 aria-label 批量补齐
8. `blue-500` → `accent` token 统一
9. MenuBar 快捷键平台化 + 键盘子菜单
10. ConnectionNavigatorTree 拆分

## 综合评分

| 维度 | 评分 |
|------|------|
| 视觉设计 / 一致性 | 8.0 |
| 布局与信息架构 | 8.5 |
| 交互与 UX 流程 | 7.5 |
| 无障碍 (a11y) | 5.0 |
| i18n 国际化 | 7.5 |
| 空/错/加载状态 | 7.5 |
| 组件复用与可维护性 | 7.0 |
| **综合** | **7.4 / 10** |
