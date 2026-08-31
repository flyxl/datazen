# DataZen 全应用 UI 审计报告 #1

> 独立审计，无分工协作  
> 审计日期：2026-08-31  
> 基准代码：`main` @ `2e8acf9c`（PR #15 合并后）

## 概述

DataZen 是基于 Tauri v2 + React 18 + Tailwind CSS 4 的跨平台桌面数据库工具。PR #15（code-review-remediation）合并后，应用在**架构可维护性**上取得明显进展：`ConnectionNavigatorTree` 从 ~2800 行拆分为 660 行壳层 + `navigator/*` 子模块；Workflow 编辑统一为 `WorkflowForm`（`variant: compact | page`）；Sync / Diff / Transfer 共用 `LimitationsDialog`、`AdminCreateDialog`、`ConfirmDialog`；迁移工具端点选择统一为 `MigrationEndpointsBar`；`ForeignKeysView` 表头 i18n 漏网已修复；E2E 稳定化通过 `data-testid` 全面化推进。

整体仍采用**暗色优先的语义化设计令牌**（`src/styles/themes.css`），通过 `--c-surface` / `--c-accent` / `--dt-*` 等 token 与运行时主题包扩展。应用壳层由 `MainPage` → `WelcomePage` / `ConnectionPage` 分支构成；有连接后进入以 **左侧 40px 模式轨 + 可折叠连接导航 + 多 Panel 标签** 为主轴的统一工作区。Workflow、Dashboard、Workspace 插件页、插件管理、Settings 均作为同一壳内的模式切换；Data Sync / Schema Diff / Data Transfer / Backup 为独立子窗口。

**主要印象**：PR #15 显著改善了组件复用与 Navigator 可维护性，综合 UI 体验较第一轮（2026-08-28）略有提升。但 **P0/P1 共识项几乎未动**——Dialog 无障碍、ErrorBoundary i18n、Settings 保存模型、导航 a11y、`blue-500` 硬编码色等跨页面问题依旧。第二轮新发现包括 Navigator 导出/导入图标语义反置、若干 inline modal 未复用 `Dialog`、全局缺少 `focus-visible` 样式、加载态普遍缺少 `aria-live`。

## 优点

1. **Navigator 模块化（PR #15）** — `ConnectionNavigatorTree.tsx` 瘦身至 ~660 行，逻辑分散至 `navigator/NavigatorToolbar.tsx`、`NavigatorTreeRow.tsx`、`NavigatorDialogs.tsx`、`buildFlatRows.ts` 等，后续 UI 迭代风险显著降低。
2. **Workflow 表单统一（PR #15）** — `WorkflowForm` 取代 `WorkflowPanel` 与 `WorkflowPage` 内重复表单，Command 步骤动态 schema 渲染路径单一，符合「零硬编码驱动差异」原则。
3. **Dialog 去重（PR #15）** — `LimitationsDialog`、`AdminCreateDialog` 在 Sync / Diff / Transfer 三件套复用，减少三处平行维护的 modal 壳层。
4. **统一的设计系统** — `themes.css` 定义完整的 surface/fg/edge/accent/DataTable 色 token；主题包可覆盖 `--dt-*` 与 CodeMirror 语法色。
5. **壳层导航清晰** — `ConnectionPage` 左侧模式轨 + `ConnectionWorkspaceHome` 快捷操作与最近 Panel，降低冷启动认知成本。
6. **Panel + Tab 模式成熟** — `PanelTabBar` 与 `WorkspaceTabBar` 视觉一致；Query / Table / ER / 结构编辑通过 `ContentView` + `panelStore` 统一管理。
7. **DataTable 能力完整** — 虚拟行渲染、列宽拖拽、过滤条、分页、行选/Delete 键删除、右键菜单、导出对话框；`ToolbarButton` 在 compact 模式用 `sr-only` 保留可读标签。
8. **Web 上下文菜单体系** — 统一走 `WebContextMenu`（`role="menu"`、Escape 关闭）+ `contextMenuStore`，符合项目约定。
9. **跨平台 TitleBar 处理细致** — macOS overlay drag region 与 Win/Linux `startDragging()` 阈值拖拽；`WindowControls` 仅在非 macOS 渲染。
10. **E2E 可测试性提升（PR #15）** — 关键路径 `data-testid` 覆盖扩大，Navigator 拆分后单测（`ConnectionNavigatorTree.test.tsx`）可针对性维护。

## 问题清单

| 严重级别 | 位置 | 问题 | 建议 |
|---------|------|------|------|
| **critical** | `src/components/ui/Dialog.tsx` | 对话框**不支持 Escape 关闭**、**无 focus trap/初始焦点**、关闭按钮无 `aria-label` | 增加 Escape → `onClose`；引入 focus trap；关闭按钮补 `aria-label={t('common.close')}` |
| **critical** | `src/components/ErrorBoundary.tsx` L43–56 | 全局错误 UI 文案硬编码英文（"Something went wrong" / "Dismiss" / "Reload"） | 接入 i18n；按钮改用 `Button` 组件 |
| **major** | `navigator/NavigatorToolbar.tsx` L50–60 | **导出用 Upload、导入用 Download**，图标语义与惯例相反 | 交换图标或改用 Share / FileInput |
| **major** | `src/windows/workflow/WorkflowForm.tsx` | 多处硬编码英文（"Command input JSON" 等） | 全部迁入 `en.ts` / `zh-CN.ts` |
| **major** | `src/components/MenuBar.tsx` | 快捷键显示 `Ctrl+N`，未按平台显示 `⌘` | 用 `usePlatform()` 动态渲染 |
| **major** | `src/windows/settings/SettingsContent.tsx` | 保存模型分裂（draft+Save vs 即时保存）；切换分区/返回不检查未保存修改 | 统一保存策略 + dirty 时 ConfirmDialog |
| **major** | `src/components/DataTable/VirtualBody.tsx` | `rows.length === 0` 时无空状态 | 增加居中空态组件 |
| **major** | `WorkflowPage.tsx`、`QueryPanel.tsx` | 多处使用 `window.alert()` | 改用 `ResultMessageDialog` / inline 错误区 |
| **major** | `ConnectionPage.tsx` WorkspaceModeButton | 左侧模式轨仅 icon + title，无 `aria-label` / `aria-current` | 补充 ARIA 属性 |
| **major** | `PanelTabBar.tsx` | 无 tab ARIA 语义；关闭按钮 hover 才可见 | 补 `role="tablist"`；关闭钮 focus 时常显 |
| **major** | `ConnectionPage.tsx` 连接错误态 | 重试按钮使用 `bg-blue-500` 原生 button | 统一为 `<Button variant="primary">` |
| **major** | 全局（grep `focus-visible`） | Host 源码 **0 处** `focus-visible` 样式（仅 extension-sdk theme.css 有） | 在 `Button` 基类与交互控件加统一 focus ring |
| **major** | `QueryPanel.tsx` L877、`IndexesView.tsx` L102、`PrivilegeView.tsx` L281 | **inline modal 未复用 `Dialog`**，a11y 行为双轨 | 迁移至共享 `Dialog` 或抽取 `ModalShell` |
| **minor** | `NewConnectionDialog.tsx` | 自建 portal 壳层，未复用 `Dialog.tsx` | 评估统一至 `Dialog` 以继承未来 a11y 修复 |
| **minor** | `WindowControls.tsx` | `aria-label` 硬编码英文 | 使用 i18n key |
| **minor** | `SettingsContent.tsx` L29–35 | 日志级别选项硬编码英文 | 迁入 locale |
| **minor** | `ExtensionManagementPage.tsx` | `permissionLabels.ts` tooltip 硬编码英文 | 迁入 i18n |
| **minor** | `ThemeToggle.tsx` | 无 `aria-label` / `aria-expanded` | 补充 aria 属性 |
| **minor** | `Select.tsx` L23 | 固定 `LIST_ID = 'dz-select-listbox'` | 每实例 `useId()` |
| **minor** | 全局 loading | 30+ spinner 无 `role="status"` / `aria-live` | 加载区加播报语义 |
| **suggestion** | 全局 | 无 skeleton 加载 | 大数据表/导航树首次加载用 skeleton |
| **suggestion** | 全局 | 无快捷键帮助面板 | Help 菜单或 `?` 打开快捷键面板 |
| **suggestion** | Data Sync 等子窗口 | 虽有 `MigrationEndpointsBar` 统一端点 UI，整体学习曲线仍陡 | 增加分步 wizard 与进度反馈 |

## 优先改进项 Top 10

1. **Dialog 无障碍与交互规范**（`Dialog.tsx`）— focus trap + Esc + aria-label
2. **ErrorBoundary i18n** — 全局崩溃页面向所有语言用户一致
3. **Settings 保存统一 + dirty 离开警告** — 防止静默丢失配置
4. **模式轨 + PanelTabBar ARIA** — 最高频导航路径
5. **`blue-500` → accent token 全局 sweep** — 主题包一致性
6. **消除 `window.alert`** — Workflow / Query 专业桌面体验
7. **Navigator 导出/导入图标修正** — 低改动、高认知收益
8. **inline modal 收敛至 Dialog** — QueryPanel 收藏框、IndexesView、PrivilegeView
9. **DataTable 空态 + loading overlay** — 数据浏览核心路径
10. **全局 `focus-visible` 基线** — 从 `Button` 基类开始

## 综合评分

| 维度 | 评分 | 说明 |
|------|------|------|
| 视觉设计 / 一致性 | 7.8 | token 体系成熟；`blue-500` 硬编码仍广泛 |
| 布局与信息架构 | 8.0 | PR #15 Navigator 拆分提升 IA；模式轨可发现性仍弱 |
| 交互与 UX 流程 | 7.5 | Workflow 表单统一改善编辑流；Settings 保存模型仍分裂 |
| 无障碍 (a11y) | 5.0 | Dialog/导航/加载播报均为系统性缺口 |
| i18n 国际化 | 7.8 | FK 表头已修；WorkflowForm、permissionLabels 仍有漏网 |
| 空/错/加载状态 | 7.2 | 多数模块有覆盖；DataTable 零行、spinner 语义不足 |
| 组件复用与可维护性 | 7.8 | PR #15 Dialog/Workflow/Navigator 拆分显著加分 |
| **综合** | **7.4 / 10** | 较第一轮 +0.0（本审计员维度加权）；架构改善明显，polish 债未偿还 |

## 页面级速览

| 页面 / 模块 | 评分 | 亮点 | 主要问题 |
|-------------|------|------|---------|
| **Welcome / Main** | 7.5 | 状态分流清晰；`data-testid` 完善 | 加载态无 `aria-live`；壳层 TitleBar 三处重复 |
| **Connection 工作区** | 7.6 | Navigator 拆分后可维护；虚拟 Schema 树 | 导出/导入图标反；模式轨 a11y；`blue-500` 错误态 |
| **Query / DataTable** | 7.3 | DataTable 功能完整；Query 历史有 aria-label | 零行空态；`window.alert`；inline 收藏 modal |
| **Settings** | 6.8 | 11 分区导航清晰；危险操作部分有 Confirm | 保存模型分裂；label 未关联；Select LIST_ID 重复 |
| **Workflow** | 7.5 | `WorkflowForm` 统一；Command 动态表单 | 硬编码英文；`window.alert`；Tab 无 ARIA |
| **Extensions / Workspace** | 7.4 | 安装两步确认；iframe 沙箱成熟 | permissionLabels 英文；Toggle 与 Settings 不一致 |
| **Dashboard** | 7.2 | 与主壳模式切换一致 | 图表空态尚可；无额外 polish |
| **Data Sync / Diff / Transfer** | 7.0 | `MigrationEndpointsBar` + 共用 Dialog | 信息密度高；无 wizard |
| **共享组件 (ui/)** | 6.5 | WebContextMenu、Select 键盘导航较好 | **Dialog 无 focus trap**；无 focus-visible |
| **Shell (TitleBar/MenuBar)** | 7.0 | 跨平台拖拽成熟 | MenuBar 无 ARIA/键盘；快捷键 Ctrl 写死 |
