# DataZen 全应用 UI 审计报告 #5

> 独立审计，无分工协作  
> 审计日期：2026-08-28

## 概述

DataZen 采用 **Tauri v2 + React 18 + Tailwind CSS 4** 的桌面应用架构，主壳层为 `TitleBar` + 左侧 40px 图标导航轨 + 可变内容区。核心路由由 `MainPage` 分流：无连接时显示 `WelcomePage`，有连接时进入 `ConnectionPage` 统一工作区；Settings、Workflow、Dashboard、Workspace、Plugins 均以**内嵌模式**挂载于 `ConnectionPage`。

整体设计遵循语义化 design token，暗色为默认体验，组件层有统一的 Button、Dialog、Select、WebContextMenu 等 primitives。国际化通过 `useI18n()` 覆盖绝大多数页面，但仍有若干硬编码英文与平台差异未处理之处。

## 优点

1. **设计体系统一** — 语义色 token + Tailwind 别名贯穿各页面，主题包可覆盖 DataTable 单元格色。
2. **Shell 跨平台处理细致** — TitleBar 区分 macOS / Win/Linux；MenuBar 仅在非 macOS 渲染。
3. **连接工作区信息架构清晰** — ConnectionNavigatorTree + PanelTabBar + ContentView 多面板模型。
4. **共享组件质量较高** — Select 键盘导航；WebContextMenu 有 role="menu"；CopyableError 错误可复制。
5. **DataTable 功能完整** — 虚拟滚动、分页、筛选、右键菜单、导出、Detail Panel。
6. **插件/扩展 UX 设计到位** — ExtensionManagementPage 卡片布局、API 版本 mismatch 提示。
7. **i18n 基础设施成熟** — 页面文案几乎全部走 t()。
8. **空态与引导较充分** — Welcome 首启、AI 未配置态、Workspace 空态均有 CTA。
9. **响应式工具栏** — ContentToolbar 通过 useCompactToolbar 折叠为纯图标并保留 aria-label。
10. **全局文本选择策略有意为之** — globals.css 默认 user-select: none，符合桌面工具预期。

## 问题清单

| 严重级别 | 位置 | 问题 | 建议 |
|---------|------|------|------|
| **critical** | `Dialog.tsx` | 无 focus trap、Esc 未实现、关闭按钮无 aria-label | 引入 focus trap；Esc 关闭 |
| **critical** | `ErrorBoundary.tsx` | 崩溃页全部硬编码英文 | 改用 useI18n |
| **critical** | `ConnectionPage.tsx` WorkspaceModeButton | 仅图标 + title，无 aria-label | 加 aria-label + aria-current |
| **major** | `WorkflowPage.tsx`、`QueryPanel.tsx` | 多处 window.alert() | 统一改用 Dialog / ResultMessageDialog |
| **major** | `MenuBar.tsx` | 快捷键硬编码 Ctrl | formatShortcut() 按平台区分 |
| **major** | `en.ts` query.shortcutHint | 英文 locale 也写 ⌘+Enter | 按 usePlatform() 动态生成 |
| **major** | `SettingsContent.tsx` | 保存模型不一致 | 统一即时保存或草稿+Save |
| **major** | `permissionLabels.ts` | 插件权限 tooltip 硬编码英文 | 迁入 i18n |
| **major** | `SettingsContent.tsx` LOG_LEVEL_OPTIONS | 硬编码英文 | 加入 i18n |
| **major** | `ConnectionPage.tsx` 连接错误态 | bg-blue-500 原生 button | 改用 Button 组件 |
| **major** | `ThemeToggle.tsx` | bg-blue-500/10 绕过 accent | 改用 accent token |
| **major** | 全局 | text-red-400 等硬编码状态色 | 建立 text-danger 等 utility |
| **major** | `PanelTabBar.tsx` | 关闭按钮无 aria-label，hover 才可见 | 加 aria-label；focus 时可见 |
| **minor** | `MainPage.tsx` | 加载态仅 spinner | 加 aria-live 与 loading 文案 |
| **minor** | `Select.tsx` | LIST_ID 重复 | useId() |
| **minor** | `WorkflowPage.tsx` | 1400+ 行单体组件 | 拆分 |
| **suggestion** | 全局 | 缺少 Toast 层 | 引入 toast 系统 |
| **suggestion** | 全局 | 无快捷键帮助面板 | 增加 cheatsheet |
| **suggestion** | `AiChatPanel.tsx` | 清空按钮无确认 | 加 ConfirmDialog |

## 优先改进项 Top 10

1. 为 Dialog 实现焦点管理与 Esc 关闭
2. 修复 ErrorBoundary 国际化
3. 左侧工作区图标导航补全 aria-label
4. 消灭 window.alert，统一应用内反馈
5. 平台感知快捷键展示
6. 统一 Settings 保存交互模型
7. 权限标签与日志级别 i18n
8. 状态色语义化（全局 → token）
9. PanelTabBar / Dialog 关闭按钮补 aria-label
10. 连接错误态改用 Button 组件

## 综合评分

| 维度 | 评分 |
|------|------|
| 视觉设计 | 7.5 |
| 布局与信息架构 | 7.5 |
| 交互与 UX 流程 | 7.0 |
| 可访问性 (a11y) | 5.5 |
| 一致性与设计系统 | 7.0 |
| 国际化 (i18n) | 7.5 |
| 错误/空态/加载态 | 7.0 |
| **综合** | **7.0 / 10** |
