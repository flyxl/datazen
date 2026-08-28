# DataZen 全应用 UI 审计报告 #3

> 独立审计，无分工协作  
> 审计日期：2026-08-28

## 概述

DataZen 是基于 Tauri v2 + React 的跨平台桌面数据库工具，整体采用**暗色优先的语义化设计系统**（`src/styles/themes.css` + Tailwind 语义 token），主壳为 **TitleBar + 左侧 40px 模式轨 + 可折叠侧栏 + 内容区** 的统一工作区。首次启动无连接时展示 Welcome，有连接后进入连接工作区；Workflow、Dashboard、Workspace 插件页、插件管理均内嵌于同一壳层切换。

**总体印象**：视觉与组件体系成熟，面向数据库 power user 的信息密度与功能深度到位；主要短板在**无障碍（a11y）**、**部分 i18n 遗漏**、**复杂工具页的学习曲线**，以及**设置保存模式不一致**。

## 优点

1. **设计系统扎实** — token 体系、Light/Dark 双主题 + 主题包扩展路径清晰。
2. **跨平台 Shell 工程化** — TitleBar 针对 macOS / Win-Linux 分别处理拖拽。
3. **i18n 覆盖广** — 核心页面均走 `useI18n()`。
4. **连接工作区信息架构合理** — 连接 Tab + Panel Tab 双层，侧栏虚拟化 Schema 树，空态引导清晰。
5. **DataTable 能力完整** — 虚拟滚动、筛选/排序/分页、导出、右键菜单，标杆级实现。
6. **Web 右键菜单统一** — `WebContextMenu.tsx` 含子菜单定位与 Escape 关闭。
7. **AI 集成 UX 友好** — 未配置时引导至设置，Query 错误可复制/诊断。
8. **插件/扩展体系 UI 完备** — 安装对话框、权限 Badge、iframe 加载/超时/重载。
9. **Toolbar 无障碍意识** — `ToolbarButton` 在 compact 模式保留 `sr-only` 文本。
10. **空态与错误态有设计** — Welcome 功能卡片、MainPage 连接加载失败重试等。

## 问题清单

| 严重级别 | 位置 | 问题 | 建议 |
|---------|------|------|------|
| **critical** | `ErrorBoundary.tsx` | 崩溃页硬编码英文 | 接入 i18n |
| **critical** | `Dialog.tsx` | 无 focus trap、无 Esc 关闭 | 增加 focus trap + Escape handler |
| **major** | `ConnectionPage.tsx` L902–953 | 左侧模式轨仅图标，新用户难发现 | 可折叠展开带标签或 onboarding |
| **major** | `SettingsContent.tsx` | 保存模式分裂；返回无 dirty 警告 | 统一保存策略 + ConfirmDialog |
| **major** | `globals.css` L58–61 | 全局 user-select: none | 审查更多内容区 opt-in selectable |
| **major** | `MenuBar.tsx` | 快捷键显示 Ctrl | 按平台显示 Cmd/Ctrl |
| **major** | `WindowControls.tsx` | aria-label 硬编码英文 | 使用 i18n |
| **major** | `PanelTabBar.tsx` | 无 tab ARIA；关闭按钮 hover 才可见 | 补 ARIA + 关闭按钮常显/focus 可见 |
| **major** | `DataTable.tsx` L439 | loading 在无选中时几乎不可见 | overlay spinner/skeleton |
| **major** | Data Sync 等子窗口 | 信息密度极高 | 分步 wizard、进度条 |
| **major** | `ConnectionPage.tsx` L851–864 | 裸 bg-blue-500 button | 统一 Button 组件 |
| **major** | `Input.tsx` | focus:border-blue-500 硬编码 | 改为 accent token |
| **minor** | `ThemeToggle.tsx` | 下拉无键盘导航 | 参考 Select 键盘逻辑 |
| **minor** | `WorkflowPage.tsx` L736 | feedback 用字符串判断 Error | 结构化 status |
| **minor** | `AiChatPanel.tsx` L174 | 消息 key={i} | 使用 message id |
| **suggestion** | 全局 | 无 Skip to content / landmark | Shell 加 main landmark |
| **suggestion** | 全局 | 无统一 Toast 系统 | 引入轻量 toast store |
| **suggestion** | 全局 | 无快捷键帮助入口 | Help 菜单或 `?` 面板 |

## 优先改进项 Top 10

1. 修复 ErrorBoundary i18n
2. Dialog 无障碍基线
3. Settings 保存模式与 dirty 离开警告
4. 左侧模式轨可发现性
5. DataTable 加载态可见性
6. 统一 focus/accent 色为 design token
7. PanelTabBar ARIA tabs + 关闭按钮键盘可达
8. WindowControls / 菜单快捷键 i18n
9. 复杂工具页分步与引导
10. 全局反馈 Toast + 键盘快捷键帮助

## 综合评分

| 维度 | 评分 |
|------|------|
| 视觉设计 | 8.0 |
| 布局与信息架构 | 7.5 |
| 交互与 UX 流程 | 7.0 |
| 无障碍 (a11y) | 5.5 |
| 一致性与组件化 | 7.5 |
| 国际化 (i18n) | 8.0 |
| 错误/空态/加载 | 7.0 |
| **综合** | **7.2 / 10** |
