# DataZen 全应用 UI 审计报告 #2

> 独立审计，无分工协作  
> 审计日期：2026-08-28

## 概述

本次审计基于源码阅读，覆盖主窗口壳层（`TitleBar`、`MenuBar`）、欢迎页、连接工作区、设置、Workflow、插件管理、Workspace 扩展页，以及共享组件（`ui/`、`DataTable/`、`ai/`、`connection/`、`chart/`）和子窗口（Data Sync / Schema Diff / Data Transfer 等）。

DataZen 整体呈现**专业桌面数据库工具**气质：暗色优先、语义化设计 token（`themes.css` + Tailwind 映射）、左侧模式导航 + 可缩放侧栏 + 多 Tab 面板，信息密度与功能深度均较高。i18n 基础设施完善（`en.ts` 为 source of truth，内置 10+ 语言），多数页面通过 `useI18n()` 驱动文案。

主要短板集中在：**无障碍（对话框焦点/键盘）、少量硬编码英文、设置页保存模式不一致、部分组件未走设计 token（`blue-500` 直写）、空态/加载态覆盖不完整**。

## 优点

1. **设计系统成熟** — `themes.css` 定义语义 token；主题包可覆盖字体与图表色。
2. **跨平台窗口壳层处理细致** — `TitleBar.tsx` 区分 macOS overlay 拖拽与 Win/Linux `startDragging()` 阈值逻辑。
3. **i18n 架构完整** — 欢迎页、连接树、Query、Workflow、插件、Workspace 空态等均走翻译 key。
4. **核心交互模式统一** — 右键菜单统一 Web Context Menu；Confirm / Result 对话框模式一致。
5. **空态与引导做得较好** — `WelcomePage`、`ConnectionWorkspaceHome`、`AiChatPanel` 未配置引导、`ChartEmptyState`、`WorkspaceDefaultCards`。
6. **复杂功能 UI 分层清晰** — 连接页模式栏与内容区解耦；Workflow 左列表 + 右多 Panel。
7. **错误展示可运维** — `CopyableError` 支持复制、`role="alert"`；插件 iframe 加载失败有专门处理。
8. **表单与对话框质量较高** — `NewConnectionDialog` 带 `aria-modal`；`Select` 支持键盘导航。
9. **桌面应用细节** — `globals.css` 全局 `user-select: none` + 内容区 opt-in 可选中。
10. **可测试性** — 大量 `data-testid`、E2E 覆盖意识强。

## 问题清单

| 严重级别 | 位置 | 问题 | 建议 |
|---------|------|------|------|
| **critical** | `Dialog.tsx` | 对话框无 focus trap，Esc 不支持关闭 | 增加 focus trap + Esc 关闭 |
| **critical** | `ErrorBoundary.tsx` | 崩溃页文案硬编码英文 | 接入 i18n |
| **major** | `WorkflowPage.tsx` L704 | 侧栏 Tab `"Workflows"` 硬编码英文 | 改为 `t('menu.workflow')` |
| **major** | `ForeignKeysView.tsx` L89–90 | 表头未翻译 | 新增 i18n key |
| **major** | `SettingsContent.tsx` L29–35 | 日志级别硬编码英文 | 走 i18n |
| **major** | `WindowControls.tsx` | aria-label 硬编码英文 | 使用 i18n |
| **major** | `MenuBar.tsx` | 快捷键固定 Ctrl | 按平台显示 ⌘/Ctrl |
| **major** | `SettingsContent.tsx` | 保存心智模型不一致 | 统一即时生效 vs 需保存策略 |
| **major** | 多处 | `bg-blue-500` 而非 accent token | 替换为语义类 |
| **major** | `PanelTabBar.tsx` | 无 tab ARIA 语义 | 补 ARIA Tab 模式 |
| **major** | `DataTable.tsx` + `VirtualBody.tsx` | 零行无空数据提示 | 增加 empty state |
| **minor** | `ConnectionPage.tsx` | 连接失败按钮未复用 Button | 统一 Button 组件 |
| **minor** | `ConnectionPage.tsx` L904–943 | 模式栏无 aria-label | 补 aria 属性 |
| **minor** | `ThemeToggle.tsx` | 无 aria-expanded | 补无障碍属性 |
| **minor** | `Select.tsx` L23 | 固定 LIST_ID 冲突 | 使用 useId() |
| **minor** | `VirtualBody.tsx` L57 | 每行 tabIndex={0} | roving focus |
| **suggestion** | Data Sync 等子窗口 | 学习曲线陡 | 增加分步 wizard |
| **suggestion** | `WorkflowPage.tsx` | 1400+ 行 | 拆分子组件 |
| **suggestion** | 全局 | 缺少 skeleton | 列表/表格引入 skeleton |

## 优先改进项 Top 10

1. Dialog 无障碍套件（focus trap + Esc + aria-label）
2. ErrorBoundary 国际化
3. 消除硬编码英文漏网（Workflow、FK、日志级别、WindowControls）
4. 统一 accent 色使用
5. Settings 保存模式统一
6. DataTable 空态与 loading 增强
7. Tab 与导航 ARIA
8. MenuBar 快捷键平台化
9. Select 唯一 listbox id
10. VirtualBody 键盘模型优化

## 综合评分

| 维度 | 评分 |
|------|------|
| 视觉设计 | 8.0 |
| 布局与信息架构 | 7.5 |
| 交互与 UX 流程 | 7.5 |
| 无障碍 (a11y) | 5.5 |
| 一致性与组件复用 | 7.0 |
| 国际化 (i18n) | 7.5 |
| 错误/空态/加载态 | 7.0 |
| **综合** | **7.3 / 10** |
