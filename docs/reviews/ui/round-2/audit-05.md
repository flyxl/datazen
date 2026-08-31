# DataZen 全应用 UI 审计报告 #5

> 独立审计，无分工协作  
> 审计日期：2026-08-31  
> 基准代码：`main` @ `2e8acf9c`（PR #15 合并后）

## 概述

DataZen 采用 **Tauri v2 + React 18 + Tailwind CSS 4** 的桌面应用架构，主壳层为 `TitleBar` + 左侧 40px 图标导航轨 + 可变内容区。核心路由由 `MainPage` 分流：无连接时显示 `WelcomePage`，有连接时进入 `ConnectionPage` 统一工作区；Settings、Workflow、Dashboard、Workspace、Plugins 均以**内嵌模式**挂载于 `ConnectionPage`。

PR #15 在 **Navigator 拆分、WorkflowForm 统一、Dialog 去重、MigrationEndpointsBar、FK i18n、E2E testid** 方面交付了可验证的架构改善，五方共识均值从 7.2 升至 7.36。但本审计员采用**更严格的 a11y 与交互一致性标尺**：P0 项（Dialog focus trap、ErrorBoundary i18n）100% 未修复；`window.alert`、permissionLabels 英文、加载态无 aria-live、全局无 focus-visible 等问题依旧，故综合分 **7.1** 为五方最低——并非否认 PR #15 价值，而是强调 **UI polish 债与 a11y 基线仍未偿还**。

## 优点

1. **Navigator 可维护性跃升** — `navigator/*` 模块边界清晰，Toolbar/Dialogs/TreeRow 可独立测试与迭代。
2. **Workflow 编辑单轨** — `WorkflowForm` 取代重复实现，降低 Visual/YAML 双模式分叉风险（数据一致性 bug 属后端/逻辑层，不在本审计 UI 范围加权）。
3. **迁移工具 Dialog 复用** — Sync/Diff/Transfer 共用 LimitationsDialog、AdminCreateDialog，减少三处 UX 漂移。
4. **设计体系统一** — 语义色 token + Tailwind 别名；主题包可覆盖 DataTable `--dt-*`。
5. **Shell 跨平台处理细致** — TitleBar 区分 macOS / Win/Linux；MenuBar 仅在非 macOS 渲染。
6. **连接工作区信息架构清晰** — Navigator + PanelTabBar + ContentView 多面板模型。
7. **共享组件质量较高** — Select 键盘导航；WebContextMenu 有 role="menu"；CopyableError 可复制。
8. **DataTable 功能完整** — 虚拟滚动、分页、筛选、右键菜单、导出、Detail Panel。
9. **插件/扩展 UX** — ExtensionManagementPage 卡片布局、API 版本 mismatch 提示、卸载数据删除文案。
10. **E2E testid 扩大** — PR #15 提升自动化回归稳定性（不替代 a11y 人工审计）。

## 问题清单

| 严重级别 | 位置 | 问题 | 建议 |
|---------|------|------|------|
| **critical** | `Dialog.tsx` | 无 focus trap、Esc 未实现、关闭按钮无 aria-label | 引入 focus trap；Esc → onClose |
| **critical** | `ErrorBoundary.tsx` | 崩溃页全部硬编码英文 | 改用 i18n + Button 组件 |
| **critical** | `ConnectionPage.tsx` WorkspaceModeButton | 仅图标 + title，无 aria-label / aria-current | 加 aria-label + aria-current="page" |
| **major** | `WorkflowPage.tsx`、`QueryPanel.tsx` | 多处 window.alert() | Dialog / ResultMessageDialog |
| **major** | `MenuBar.tsx` | 快捷键硬编码 Ctrl | formatShortcut() 按平台 |
| **major** | `en.ts` query.shortcutHint | 英文 locale 写 ⌘+Enter，未动态 | usePlatform() 生成 |
| **major** | `SettingsContent.tsx` | 保存模型不一致 | 统一即时保存或草稿+Save |
| **major** | `permissionLabels.ts` | 插件权限 tooltip 硬编码英文 | 迁入 i18n；Badge 主文案可读化 |
| **major** | `SettingsContent.tsx` LOG_LEVEL_OPTIONS | 硬编码英文 | 加入 i18n |
| **major** | `ConnectionPage.tsx` 连接错误态 | bg-blue-500 原生 button | Button variant="primary" |
| **major** | `ThemeToggle.tsx` | bg-blue-500/10 绕过 accent | accent token |
| **major** | 全局 | text-red-400 等硬编码状态色 | text-danger 等 utility |
| **major** | `PanelTabBar.tsx` | 关闭按钮无 aria-label，hover 才可见 | aria-label；focus 可见 |
| **major** | `navigator/NavigatorToolbar.tsx` | Export Upload / Import Download 反 | 交换图标 |
| **major** | 全局 grep | **0 处 focus-visible**（Host） | Button 基类 mandatory ring |
| **major** | 全局 loading | **30+ spinner 无 aria-live / role="status"** | 加载播报 |
| **major** | `QueryPanel.tsx`、`IndexesView.tsx`、`PrivilegeView.tsx` | inline modal 未复用 Dialog | 收敛双轨 |
| **major** | `NewConnectionDialog.tsx` | 自建 portal，与 Dialog a11y 双轨 | 统一 primitive |
| **minor** | `MainPage.tsx` | 加载态仅 spinner | aria-live + loading 文案 |
| **minor** | `Select.tsx` | LIST_ID 重复 | useId() |
| **minor** | `DataTable/VirtualBody.tsx` | 零行无空态 | empty state |
| **minor** | `VirtualBody.tsx` L57 | 每行 tabIndex={0} | roving tabindex |
| **suggestion** | 全局 | 缺少 Toast 层 | toast 系统 |
| **suggestion** | 全局 | 无快捷键帮助面板 | cheatsheet |
| **suggestion** | `AiChatPanel.tsx` | 清空按钮无确认 | ConfirmDialog |

## 优先改进项 Top 10

1. Dialog 焦点管理 + Esc 关闭 + 关闭钮 aria-label（P0）
2. ErrorBoundary 国际化（P0）
3. 左侧工作区图标导航 aria-label + aria-current（P0 级 major）
4. 消灭 window.alert，统一应用内反馈
5. 平台感知快捷键（MenuBar + query hint）
6. Settings 保存交互模型统一 + dirty guard
7. permissionLabels + 日志级别 + WorkflowForm i18n
8. 状态色语义化（blue-500 / red-400 → token）
9. 全局 focus-visible + 加载态 aria-live（系统性 a11y）
10. inline modal / NewConnectionDialog 收敛至 Dialog

## 评分 Matrix（10 维度）

| # | 维度 | 评分 | 说明 |
|---|------|------|------|
| 1 | **视觉设计** | 7.5 | 暗色 professional 气质；硬编码色仍广泛 |
| 2 | **布局与信息架构** | 7.8 | Navigator 拆分加分；垂直 chrome 仍偏多 |
| 3 | **交互与 UX 流程** | 7.0 | Settings 分裂；alert 破坏流 |
| 4 | **无障碍 (a11y)** | **5.0** | **五方最低档；Dialog/导航/加载全面缺口** |
| 5 | **一致性与组件复用** | 7.5 | PR #15 Dialog/Workflow 去重；inline modal 仍双轨 |
| 6 | **国际化 (i18n)** | 7.5 | 主流程覆盖好；permissionLabels/Workflow 漏网 |
| 7 | **空态 / 错误态 / 加载态** | 6.8 | 空态多数有；loading 读屏反馈差；DataTable 零行 |
| 8 | **可维护性** | 7.8 | Navigator/Workflow 拆分显著；QueryPanel 仍大 |
| 9 | **可测试性** | 8.0 | data-testid + E2E 意识强 |
| 10 | **专业工具契合度** | 7.5 | 信息密度与功能深度匹配 DBA 预期 |
| | **综合（算术平均）** | **7.1 / 10** | 五方最低；a11y 与加载态权重拉低 |

### 维度雷达解读

- **最高分**：可测试性 8.0 — PR #15 E2E 稳定化与本审计员重视自动化一致。
- **最低分**：无障碍 5.0 — Dialog 无 focus trap 为阻断级；全局无 focus-visible；30+ 无 aria-live spinner。
- **PR #15 主要提升维度**：可维护性（7.8）、布局 IA（7.8）、一致性（7.5）。
- **未改善维度**：a11y（5.0 vs 第一轮 5.5 本员评分反降，因审计发现 focus-visible 全局缺失与 inline modal 双轨）、空错加载（6.8）。

### 与第一轮对比（本审计员）

| 维度 | 第一轮 | 第二轮 | Δ |
|------|--------|--------|---|
| 视觉设计 | 7.5 | 7.5 | 0 |
| 布局 / IA | 7.5 | 7.8 | +0.3 |
| 交互 / UX | 7.0 | 7.0 | 0 |
| a11y | 5.5 | 5.0 | −0.5 |
| 一致性 | 7.0 | 7.5 | +0.5 |
| i18n | 7.5 | 7.5 | 0 |
| 空错加载 | 7.0 | 6.8 | −0.2 |
| 可维护性 | — | 7.8 | 新增维度 |
| 可测试性 | — | 8.0 | 新增维度 |
| 专业契合 | — | 7.5 | 新增维度 |
| **综合** | 7.0 | **7.1** | +0.1 |

*本审计员第二轮对 a11y 与加载态采用更细粒度检查（focus-visible grep、spinner aria-live 抽样），故 a11y 分低于第一轮自评；架构改善仅部分抵消。*
