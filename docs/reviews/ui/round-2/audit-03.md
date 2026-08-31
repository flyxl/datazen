# DataZen 全应用 UI 审计报告 #3

> 独立审计，无分工协作  
> 审计日期：2026-08-31  
> 基准代码：`main` @ `2e8acf9c`（PR #15 合并后）

## 概述

DataZen 是基于 Tauri v2 + React 的跨平台桌面数据库工具，整体采用**暗色优先的语义化设计系统**（`src/styles/themes.css` + Tailwind 语义 token）。主壳为 **TitleBar + 左侧 40px 模式轨 + 可折叠侧栏 + 内容区** 的统一工作区；首次启动无连接时展示 Welcome，有连接后进入连接工作区；Workflow、Dashboard、Workspace 插件页、插件管理均内嵌于同一壳层切换。

PR #15 合并后，**组件复用与信息架构**是本轮最显著进步：Navigator 拆分、`WorkflowForm` 统一、Dialog 去重、`MigrationEndpointsBar` 三件套收敛、FK i18n 修复。综合分由第一轮 7.2 微调至 **7.3**——架构债偿还明显，但 **a11y 基线、Settings 保存模型、ErrorBoundary i18n** 等 polish 项基本未动，与五方共识一致。

**总体印象**：视觉与组件体系仍属成熟档位，面向数据库 power user 的信息密度与功能深度到位；主要短板在**无障碍**、**部分 i18n 遗漏**、**inline modal 双轨**、**加载态读屏反馈缺失**，以及 Settings 交互模型不一致。

## 优点

1. **PR #15 架构 remediation 有效** — Navigator ~2800 行 → 660 行 + `navigator/*`；Workflow 双表单 → 单一 `WorkflowForm`；Sync/Diff/Transfer Dialog 去重。
2. **设计系统扎实** — token 体系、Light/Dark 双主题 + 主题包扩展路径清晰。
3. **跨平台 Shell 工程化** — TitleBar 针对 macOS / Win-Linux 分别处理拖拽。
4. **i18n 覆盖广** — 核心页面均走 `useI18n()`；`ForeignKeysView` 表头第二轮已补翻译。
5. **连接工作区信息架构合理** — Panel Tab 双层，侧栏虚拟化 Schema 树（`buildFlatRows.ts` + `@tanstack/react-virtual`），空态引导清晰。
6. **DataTable 能力完整** — 虚拟滚动、筛选/排序/分页、导出、右键菜单。
7. **Web 右键菜单统一** — `WebContextMenu.tsx` 含子菜单定位与 Escape 关闭。
8. **AI 集成 UX 友好** — 未配置时引导至设置，Query 错误可复制/诊断。
9. **插件/扩展体系 UI 完备** — 安装对话框、权限 Badge、iframe 加载/超时/重载。
10. **E2E 稳定化** — PR #15 扩大 `data-testid` 覆盖，回归更可重复。

## 模块速览

| 模块 | 路径 | 评分 | 第二轮变化 | 亮点 | 主要问题 |
|------|------|------|-----------|------|---------|
| **Welcome / Main** | `windows/welcome/`, `main/` | 7.4 | → | 状态分流；testid 完善 | 加载无 aria-live；壳层重复 |
| **Connection 壳层** | `connection/ConnectionPage.tsx` | 7.6 | ↑ | 模式轨清晰；resize 持久化 | 模式轨 a11y；连接错误 blue-500 |
| **Navigator** | `connection/ConnectionNavigatorTree.tsx`, `navigator/*` | 7.8 | **↑↑** | **拆分后可维护**；虚拟树 | **导出/导入图标反** |
| **Content / Panel** | `ContentView.tsx`, `PanelTabBar.tsx` | 7.2 | → | 多 Panel IDE 式体验 | Tab 无 ARIA；关闭 hover-only |
| **Query / SQL** | `QueryPanel.tsx` | 7.0 | → | 历史搜索 aria-label | alert；inline 收藏 modal |
| **DataTable** | `components/DataTable/` | 7.5 | → | 虚拟滚动标杆 | 零行空态；loading 不可见 |
| **结构视图** | `StructureView`, `IndexesView`, `ForeignKeysView` | 7.3 | ↑ | FK i18n 已修 | Indexes inline modal；blue 硬编码 |
| **Settings** | `windows/settings/` | 6.7 | → | 11 分区；部分 Confirm | 保存分裂；label 未关联 |
| **Workflow** | `windows/workflow/` | 7.4 | **↑** | **WorkflowForm 统一** | 英文漏网；alert |
| **Extensions** | `windows/extensions/`, `workspace/` | 7.3 | → | 安装 inspect 流程 | permissionLabels 英文 |
| **Dashboard** | `windows/dashboard/` | 7.2 | → | ChartEmptyState | 无额外变化 |
| **Data Sync** | `windows/data-sync/` | 7.1 | ↑ | MigrationEndpointsBar | 密度高；无 wizard |
| **Schema Diff** | `windows/schema-diff/` | 7.1 | ↑ | 共用 LimitationsDialog | 学习曲线 |
| **Data Transfer** | `windows/data-transfer/` | 7.0 | ↑ | AdminCreateDialog 复用 | 步骤多 |
| **Backup** | `windows/backup/` | 7.0 | → | 独立子窗口 | 与迁移工具风格略异 |
| **AI Chat** | `components/ai/` | 7.5 | → | 未配置引导 | 消息 key={i} |
| **Chart** | `components/chart/` | 7.6 | → | 空态分原因 | — |
| **共享 ui/** | `components/ui/` | 6.4 | → | Select 键盘；WebContextMenu | **Dialog 无 focus trap** |
| **Shell** | `TitleBar`, `MenuBar`, `WindowControls` | 7.0 | → | 跨平台拖拽 | MenuBar 无键盘；Ctrl 写死 |
| **ErrorBoundary** | `components/ErrorBoundary.tsx` | 4.0 | → | — | **全英文硬编码** |

**图例**：↑ 较第一轮改善；→ 基本持平；↓↓ 退步（本轮无模块标记退步）

## 问题清单

| 严重级别 | 位置 | 问题 | 建议 |
|---------|------|------|------|
| **critical** | `ErrorBoundary.tsx` | 崩溃页硬编码英文 | 接入 i18n |
| **critical** | `Dialog.tsx` | 无 focus trap、无 Esc 关闭、关闭钮无 aria-label | 增加 focus trap + Escape handler |
| **major** | `ConnectionPage.tsx` L902–953 | 左侧模式轨仅图标，新用户难发现 | 可折叠展开带标签或 onboarding tooltip |
| **major** | `SettingsContent.tsx` | 保存模式分裂；返回无 dirty 警告 | 统一保存策略 + ConfirmDialog |
| **major** | `globals.css` L58–61 | 全局 user-select: none | 审查 help/文档区 opt-in selectable |
| **major** | `MenuBar.tsx` | 快捷键显示 Ctrl | 按平台显示 Cmd/Ctrl |
| **major** | `WindowControls.tsx` | aria-label 硬编码英文 | 使用 i18n |
| **major** | `PanelTabBar.tsx` | 无 tab ARIA；关闭按钮 hover 才可见 | 补 ARIA + 关闭按钮 focus 可见 |
| **major** | `DataTable.tsx` | loading 在无选中时几乎不可见 | overlay spinner/skeleton |
| **major** | Data Sync 等子窗口 | 虽有端点栏统一，信息密度仍极高 | 分步 wizard、进度条 |
| **major** | `ConnectionPage.tsx` | 裸 bg-blue-500 button | 统一 Button 组件 |
| **major** | `Input.tsx`、`McpClientSection.tsx` | focus:border-blue-500 硬编码 | 改为 accent token |
| **major** | `navigator/NavigatorToolbar.tsx` | 导出 Upload / 导入 Download 图标反 | 交换图标 |
| **major** | `QueryPanel.tsx`、`IndexesView.tsx`、`PrivilegeView.tsx` | inline modal 未复用 Dialog | 收敛至共享 primitive |
| **major** | 全局 | 无 focus-visible（Host grep 0 匹配） | Button 基类统一 focus ring |
| **major** | 全局 loading | spinner 普遍无 aria-live | 加 role="status" |
| **minor** | `ThemeToggle.tsx` | 下拉无完整键盘导航 | 参考 Select 键盘逻辑 |
| **minor** | `WorkflowPage.tsx` | feedback 用字符串判断 Error | 结构化 status |
| **minor** | `AiChatPanel.tsx` | 消息 key={i} | 使用 message id |
| **minor** | `NewConnectionDialog.tsx` | 未复用 Dialog.tsx | 评估统一 portal/a11y |
| **suggestion** | 全局 | 无 Skip to content / main landmark | Shell 加 landmark |
| **suggestion** | 全局 | 无统一 Toast 系统 | 引入轻量 toast store |
| **suggestion** | 全局 | 无快捷键帮助入口 | Help 菜单或 `?` 面板 |

## 优先改进项 Top 10

1. 修复 ErrorBoundary i18n
2. Dialog 无障碍基线（focus trap + Esc + aria-label）
3. Settings 保存模式与 dirty 离开警告
4. 左侧模式轨可发现性 + aria-label
5. DataTable 加载态可见性 + 空态
6. 统一 focus/accent 色为 design token；补 focus-visible
7. PanelTabBar ARIA tabs + 关闭按钮键盘可达
8. WindowControls / 菜单快捷键 i18n 与平台化
9. inline modal 收敛（QueryPanel / IndexesView / PrivilegeView）
10. Navigator 导出/导入图标修正 + 复杂工具页分步引导

## 综合评分

| 维度 | 评分 | 较第一轮 |
|------|------|---------|
| 视觉设计 | 7.7 | −0.1 |
| 布局与信息架构 | 7.9 | +0.2 |
| 交互与 UX 流程 | 7.2 | +0.2 |
| 无障碍 (a11y) | 5.5 | +0.2 |
| 一致性与组件化 | 7.6 | +0.1 |
| 国际化 (i18n) | 7.8 | +0.2 |
| 错误/空态/加载 | 7.2 | 0 |
| **综合** | **7.3 / 10** | +0.1 |

*本审计员对 PR #15 架构收益给予较高 IA/一致性权重，但 a11y 与 Settings 交互模型仍显著拖累综合分。*
