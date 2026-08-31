# DataZen 全应用 UI 审计报告 #2

> 独立审计，无分工协作  
> 审计日期：2026-08-31  
> 基准代码：`main` @ `2e8acf9c`（PR #15 合并后）

## 概述

本次审计基于 PR #15 合并后的 `main` @ `2e8acf9c` 源码阅读，覆盖主窗口壳层（`TitleBar`、`MenuBar`）、欢迎页、连接工作区（含拆分后的 `navigator/*`）、设置、Workflow、插件管理、Workspace 扩展页，以及共享组件（`ui/`、`DataTable/`、`ai/`、`connection/`、`chart/`）和子窗口（Data Sync / Schema Diff / Data Transfer / Backup）。

DataZen 整体呈现**专业桌面数据库工具**气质：暗色优先、语义化 design token、左侧模式导航 + 可缩放侧栏 + 多 Tab 面板。PR #15 带来的 WorkflowForm 统一、Dialog 去重、Navigator 拆分、MigrationEndpointsBar、FK i18n 修复与 E2E testid 化，使**一致性 / 组件复用**维度较第一轮明显提升（7.1 → 7.5 共识均值）。

主要短板仍集中在：**无障碍（对话框焦点/键盘、全局 focus-visible 缺失）**、**Settings 保存模式不一致**、**硬编码色/英文漏网**、**空态/加载态 a11y 不完整**。以下问题清单采用 **U2-xx** 编号便于台账引用。

## 优点

1. **Navigator 架构改善（PR #15）** — `navigator/NavigatorToolbar.tsx`、`NavigatorTreeRow.tsx`、`NavigatorDialogs.tsx` 职责分离，原 ~2800 行巨石文件已消解。
2. **Workflow 编辑路径单一化** — `WorkflowForm` + `WorkflowPage` 共用，消除 `WorkflowPanel` 双轨维护。
3. **迁移工具 UI 收敛** — `MigrationEndpointsBar` 统一 Sync / Diff / Transfer 端点选择；`LimitationsDialog` 三处复用。
4. **设计系统成熟** — `themes.css` 语义 token；主题包可覆盖字体与图表色。
5. **跨平台窗口壳层处理细致** — `TitleBar.tsx` 区分 macOS overlay 拖拽与 Win/Linux 阈值逻辑。
6. **i18n 架构完整** — 欢迎页、连接树、Query、Workflow、插件、Workspace 空态等均走翻译 key；FK 表头第二轮已修。
7. **核心交互模式统一** — 右键菜单统一 Web Context Menu；Confirm / Result 对话框模式在多数路径一致。
8. **空态与引导** — `WelcomePage`、`ConnectionWorkspaceHome`、`AiChatPanel` 未配置引导、`ChartEmptyState`。
9. **错误展示可运维** — `CopyableError` 支持复制、`role="alert"`；插件 iframe 加载失败有专门处理。
10. **可测试性** — 大量 `data-testid`；Navigator 拆分后单测粒度更细。

## 问题清单

| ID | 严重级别 | 位置 | 问题 | 建议 |
|----|---------|------|------|------|
| **U2-01** | critical | `Dialog.tsx` | 对话框无 focus trap，Esc 不支持关闭，关闭钮无 aria-label | 增加 focus trap + Esc + aria-label |
| **U2-02** | critical | `ErrorBoundary.tsx` | 崩溃页文案硬编码英文 | 接入 i18n |
| **U2-03** | major | `SettingsContent.tsx` | 保存心智模型不一致（draft+Save vs 即时保存） | 统一即时生效或全局 draft 策略 |
| **U2-04** | major | `SettingsContent.tsx` | 切换分区 / 返回无 dirty 警告 | `isDirty` 时 ConfirmDialog |
| **U2-05** | major | `PanelTabBar.tsx` | 无 tab ARIA 语义（tablist/tab/aria-selected） | 补 WAI-ARIA Tabs 模式 |
| **U2-06** | major | `PanelTabBar.tsx` | 关闭按钮 hover 才可见，键盘/触控难发现 | focus-within 时常显；≥24px 热区 |
| **U2-07** | major | `ConnectionPage.tsx` WorkspaceModeButton | 模式轨无 aria-label / aria-current | 补充无障碍名称与当前态 |
| **U2-08** | major | 全局 | `bg-blue-500` / `text-blue-400` 硬编码而非 accent token | 批量替换 semantic class |
| **U2-09** | major | `MenuBar.tsx` | 快捷键固定 Ctrl，未平台化 | 按 macOS / Win 显示 ⌘ / Ctrl |
| **U2-10** | major | `WindowControls.tsx` | aria-label 硬编码英文 | 使用 i18n |
| **U2-11** | major | `WorkflowPage.tsx` L704 等 | 侧栏 Tab / 表单硬编码英文 | 改为 `t('workflows.*')` |
| **U2-12** | major | `WorkflowPage.tsx`、`QueryPanel.tsx` | 多处 `window.alert()` | 改用应用内 Dialog / inline 错误 |
| **U2-13** | major | `DataTable.tsx` + `VirtualBody.tsx` | 零行无空数据提示 | 增加 empty state |
| **U2-14** | major | `DataTable.tsx` | loading 在无选中时几乎不可见 | overlay spinner / skeleton |
| **U2-15** | major | `navigator/NavigatorToolbar.tsx` | Export 用 Upload、Import 用 Download | 交换或换图标 |
| **U2-16** | major | 全局 grep | Host 无 `focus-visible` 样式 | Button 基类加 focus ring |
| **U2-17** | minor | `Select.tsx` L23 | 固定 LIST_ID 多实例冲突 | 使用 `useId()` |
| **U2-18** | minor | `QueryPanel.tsx` L877 | 收藏对话框 inline modal 未复用 Dialog | 迁移至共享 Dialog |
| **U2-19** | minor | `IndexesView.tsx`、`PrivilegeView.tsx` | 结构/权限编辑 inline modal 双轨 | 统一 modal  primitive |
| **U2-20** | minor | 全局 loading | 30+ spinner 无 aria-live / role="status" | 加载区加读屏反馈 |

## 优先改进项 Top 10

1. **U2-01** Dialog 无障碍套件（focus trap + Esc + aria-label）
2. **U2-02** ErrorBoundary 国际化
3. **U2-03 + U2-04** Settings 保存模式统一 + dirty 离开警告
4. **U2-05 + U2-06 + U2-07** 导航轨与 Panel Tab 完整 a11y
5. **U2-08** 统一 accent 色，消除 blue-500
6. **U2-12** 消灭 window.alert
7. **U2-13 + U2-14** DataTable 空态与 loading 增强
8. **U2-09 + U2-10** MenuBar / WindowControls 平台化与 i18n
9. **U2-15** Navigator 导出/导入图标修正
10. **U2-16** 全局 focus-visible 基线

## 综合评分

### 各维度评分

| 维度 | 评分 | 较第一轮变化 |
|------|------|-------------|
| 视觉设计 | 7.8 | −0.2（硬编码色仍扣分） |
| 布局与信息架构 | 8.0 | +0.3（Navigator 拆分） |
| 交互与 UX 流程 | 7.5 | +0.3（Workflow 统一） |
| 无障碍 (a11y) | 5.5 | +0.2（E2E testid 不抵 a11y 基线） |
| 一致性与组件复用 | 7.8 | +0.7（PR #15 最大收益） |
| 国际化 (i18n) | 7.8 | +0.3（FK 修复） |
| 错误/空态/加载态 | 7.2 | 0 |
| **综合** | **7.4 / 10** | +0.1 |

### 评分矩阵（模块 × 维度）

| 模块 | 视觉 | 布局 | UX | a11y | 一致性 | i18n | 空错加载 | **小计** |
|------|------|------|-----|------|--------|------|---------|---------|
| Welcome/Main | 8.0 | 8.0 | 7.5 | 4.5 | 7.5 | 8.0 | 7.0 | 7.2 |
| Connection | 7.5 | 8.5 | 7.5 | 5.0 | 8.0 | 7.5 | 7.0 | 7.3 |
| Settings | 7.5 | 7.5 | 6.5 | 4.5 | 6.0 | 7.5 | 7.0 | 6.6 |
| Workflow | 7.5 | 7.5 | 7.5 | 5.5 | 8.0 | 6.5 | 7.0 | 7.1 |
| Extensions | 7.5 | 7.5 | 7.5 | 5.5 | 7.0 | 7.0 | 7.5 | 7.1 |
| 共享 ui/ | 7.5 | — | — | 4.0 | 7.5 | — | — | 6.3 |
| 子窗口 (Sync等) | 7.0 | 7.0 | 6.5 | 5.5 | 8.0 | 7.5 | 7.0 | 7.0 |
| **均值** | **7.6** | **7.7** | **7.2** | **4.9** | **7.4** | **7.3** | **7.1** | **7.1** |

*注：共享 ui/ 与 Shell 的 a11y 缺口（Dialog、focus-visible）拉低全局均值；Connection 因 Navigator 拆分在布局/一致性上得分最高。*
