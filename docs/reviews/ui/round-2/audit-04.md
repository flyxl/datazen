# DataZen 全应用 UI 审计报告 #4

> 独立审计，无分工协作  
> 审计日期：2026-08-31  
> 基准代码：`main` @ `2e8acf9c`（PR #15 合并后）

## 概述

DataZen 是一款面向数据库工程师的 Tauri v2 + React 桌面工具，整体采用**暗色优先的语义化设计系统**（`src/styles/themes.css` + Tailwind 语义 token）。主壳层为 `TitleBar` + `MenuBar`（Windows/Linux）+ 左侧 40px 模式轨 + 可变宽侧栏 + 内容区。应用从 `MainPage` 分流至 `WelcomePage` 或 `ConnectionPage`，后者再嵌入连接工作区、Workflow、Dashboard、Workspace 插件页、插件管理等子模式。

PR #15 是本审计员给出 **7.6**（五方最高）的主要依据：**Navigator 拆分**使连接树模块从不可维护变为可迭代；**WorkflowForm 统一**消除双轨表单；**Dialog 去重**与 **MigrationEndpointsBar** 显著降低迁移工具 UI 分叉；**ForeignKeysView i18n** 与 **E2E testid** 体现 remediation 闭环意识。上述改善属于「结构性 polish」，虽不解 Dialog a11y 等 P0，但让应用在专业工具气质上更趋成熟。

**总体印象**：视觉风格统一、信息架构第二轮提升最明显；核心连接/SQL/DataTable 体验仍属成熟档位。主要短板仍在 **无障碍基线**、**Settings 保存模型**、**硬编码色/英文漏网**，以及部分 inline modal 与共享 `Dialog` 的双轨实现。

## 优点

1. **Navigator 拆分是本轮最大 UI 架构 win** — `ConnectionNavigatorTree.tsx` ~660 行 + `navigator/*` 8 文件，Toolbar / Row / Dialogs / flatRows 职责清晰，后续 a11y 与图标修正可局部落地。
2. **WorkflowForm 统一** — `variant: compact | page` 覆盖侧栏编辑与全页编辑，Command 动态 schema 单一路径。
3. **迁移三件套 UI 收敛** — `MigrationEndpointsBar` + `LimitationsDialog` + `AdminCreateDialog` 减少重复 modal 与端点选择 UX 分叉。
4. **设计系统扎实** — token 体系支持亮/暗主题与主题包扩展；`StructureView` 仍用 `--dt-*` 类型色。
5. **壳层跨平台处理细致** — TitleBar、WindowControls、MenuBar 平台差异处理好。
6. **主工作区导航清晰** — 左侧模式轨 + `ConnectionWorkspaceHome` 降低冷启动成本。
7. **共享 UI 组件质量较高** — Button、Select、WebContextMenu、DataTable 能力完整；`NewConnectionDialog` 自带 `aria-modal`（虽未复用 Dialog）。
8. **空/未配置状态有引导** — Welcome、AiChatPanel、WorkspaceDefaultCards、ExtensionManagementPage。
9. **i18n 覆盖广且第二轮有修复** — FK 表头；绝大多数页面通过 `useI18n()` 驱动。
10. **E2E 友好** — PR #15 扩大 data-testid；Navigator 单测文件仍庞大但可分区维护。

## 问题清单

| ID | 严重级别 | 位置 | 问题 | 建议 |
|----|---------|------|------|------|
| **A01** | critical | `ErrorBoundary.tsx` L43–56 | 崩溃页全部硬编码英文 | 接入 i18n；按钮用 Button 组件 |
| **A02** | critical | `Dialog.tsx` L49–55 | 无 Esc 关闭、无 focus trap、关闭钮无 aria-label | 增加 focus trap + Esc + aria-label |
| **A03** | major | `WorkflowForm.tsx` | 硬编码英文（Command JSON 等） | 迁入 en.ts/zh-CN.ts |
| **A04** | major | `ConnectionPage.tsx` WorkspaceModeButton | 无 aria-label / aria-current | 添加无障碍名称与当前态 |
| **A05** | major | `PanelTabBar.tsx` | 关闭按钮 hover 才可见 | 始终显示或 focus-within 显示 |
| **A06** | major | `SettingsContent.tsx` | 保存模式分裂（5 区 draft+Save vs 其他即时） | 统一即时生效或全局 draft |
| **A07** | major | `SettingsContent.tsx` LOG_LEVEL_OPTIONS | Trace/Debug 等硬编码英文 | 使用 i18n |
| **A08** | major | `WindowControls.tsx` | aria-label 硬编码英文 | 使用 i18n |
| **A09** | major | `ConnectionPage.tsx` | 多连接无显式 Tab 条，仅靠侧栏树切换 | 增加连接 Tab 条或强化分组 |
| **A10** | major | `MenuBar.tsx` L257+ | 子菜单仅 hover 展开，无键盘导航 | WAI-ARIA Menu 模式 |
| **A11** | major | `navigator/NavigatorToolbar.tsx` | Export→Upload、Import→Download 图标反 | 交换或换语义图标 |
| **A12** | major | `WorkflowPage.tsx`、`QueryPanel.tsx` | window.alert() 残留 | ResultMessageDialog / inline |
| **A13** | major | 全局 grep focus-visible | Host 源码 0 处 focus-visible | Button 基类统一 ring |
| **A14** | major | `VirtualBody.tsx` | 零行无空态 | 增加 empty state |
| **A15** | minor | `ConnectionPage.tsx` 错误态 | inline bg-blue-500 button | 统一 Button |
| **A16** | minor | `ThemeToggle.tsx` | 缺少 aria-expanded | 补齐 ARIA |
| **A17** | minor | `Select.tsx` | 固定 LIST_ID | useId() |
| **A18** | minor | `QueryPanel.tsx` L877 | 收藏框 inline modal | 复用 Dialog |
| **A19** | minor | `IndexesView.tsx`、`PrivilegeView.tsx` | 权限/索引编辑 inline modal 双轨 | 统一 modal primitive |
| **A20** | suggestion | 全局 | 无快捷键帮助 / Toast / skeleton | 分阶段引入 cheatsheet、toast、skeleton |

## 优先改进项 Top 10

1. **A02** 修复 Dialog 无障碍基线（focus trap + Esc + aria-label）
2. **A01** 国际化 ErrorBoundary
3. **A03** WorkflowForm 硬编码英文清零
4. **A04 + A05** 左侧模式轨与 Panel Tab 的 a11y
5. **A06** 统一 Settings 保存/生效模式 + dirty guard
6. **A11** Navigator 导出/导入图标修正（低成本高收益）
7. **A09** 多连接会话的可视化 Tab（专业用户场景）
8. **A12** 消灭 window.alert
9. **A13 + A15** 统一 focus ring 与 accent token
10. **A08 + A16** WindowControls + ThemeToggle ARIA/i18n

## 综合评分

| 维度 | 评分 | 权重说明 |
|------|------|---------|
| 视觉设计 | 8.0 | token 体系统一；blue-500 仍偶发 |
| 布局与信息架构 | **8.5** | **Navigator 拆分 + MigrationEndpointsBar 显著加分** |
| 交互与 UX 流程 | 7.5 | WorkflowForm 统一；Settings 仍分裂 |
| 无障碍 (a11y) | 5.5 | Dialog/MenuBar/导航轨缺口未变 |
| 一致性与组件复用 | **8.0** | **PR #15 Dialog/Workflow 去重核心收益** |
| i18n | 7.8 | FK 修复；WorkflowForm 漏网 |
| 空/加载/错误状态 | 7.5 | 多数到位；DataTable 零行仍缺 |
| **综合（加权）** | **7.6 / 10** | 五方最高；架构改善权重高于 a11y 债 |

### 与第一轮对比（本审计员）

| 维度 | 第一轮 (#4) | 第二轮 | Δ |
|------|------------|--------|---|
| 视觉设计 | 8.0 | 8.0 | 0 |
| 布局 / IA | 7.5 | 8.5 | **+1.0** |
| 交互 / UX | 7.0 | 7.5 | +0.5 |
| a11y | 5.0 | 5.5 | +0.5 |
| 一致性 | 7.0 | 8.0 | **+1.0** |
| i18n | 7.5 | 7.8 | +0.3 |
| 空错加载 | 7.5 | 7.5 | 0 |
| **综合** | 7.2 | **7.6** | **+0.4** |

*本审计员认为 PR #15 的结构性 remediation 应体现在综合分上；P0 a11y 未修但不抵消 IA/一致性收益。*
