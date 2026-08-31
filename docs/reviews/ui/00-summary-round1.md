# DataZen 全应用 UI 综合评审报告

> 评审日期：2026-08-28  
> 方法：5 位独立审计员各自完整审查整个应用 UI（无分工、无协作），主代理汇总共识  
> 分项报告：[audit-01.md](./audit-01.md) ~ [audit-05.md](./audit-05.md)

---

## 执行摘要

5 位独立审计员对 DataZen **全应用 UI** 分别给出综合评分：

| 审计员 | 综合分 |
|--------|--------|
| #1 | 7.4 |
| #2 | 7.3 |
| #3 | 7.2 |
| #4 | 7.2 |
| #5 | 7.0 |
| **均值** | **7.2 / 10** |

**共识结论**：DataZen 已具备成熟桌面数据库工具的 UI 骨架——语义化 design token、三栏/多 Panel 工作区、DataTable 虚拟滚动、Web Context Menu 统一、i18n 基础设施完善。距离「产品级 polish」的主要差距不在功能缺失，而在 **Dialog 无障碍**、**ErrorBoundary i18n**、**Settings 保存模型**、**硬编码色/英文漏网** 等跨页面共性问题。

---

## 五方共识优点（≥4/5 审计员提及）

| 优点 | 提及次数 |
|------|---------|
| 设计系统 / 语义 token（themes.css、surface/accent/dt-*） | 5/5 |
| 跨平台 Shell（TitleBar 拖拽、WindowControls、MenuBar 平台差异） | 5/5 |
| 连接工作区信息架构（模式轨 + Schema 树 + Panel Tab） | 5/5 |
| DataTable 能力完整（虚拟滚动、筛选、导出、右键菜单） | 5/5 |
| i18n 基础设施（useI18n 覆盖主流程） | 5/5 |
| Web Context Menu 统一替代原生菜单 | 4/5 |
| 空态/引导设计（Welcome、Workspace、AI 未配置） | 5/5 |
| 插件体系 UI（安装流程、权限 Badge、iframe 沙箱） | 4/5 |
| 可调整布局（侧栏 resize 持久化） | 3/5 |
| E2E / data-testid 可测试性 | 3/5 |

---

## 五方共识问题（≥4/5 审计员独立发现）

### Critical — 100% 共识（5/5）

| 问题 | 位置 | 共识度 |
|------|------|--------|
| **Dialog 无 focus trap、无 Esc 关闭、关闭按钮无 aria-label** | `src/components/ui/Dialog.tsx` | 5/5 |
| **ErrorBoundary 崩溃页硬编码英文** | `src/components/ErrorBoundary.tsx` | 5/5 |

### Major — 高共识（≥4/5）

| 问题 | 位置 | 共识度 |
|------|------|--------|
| **Settings 保存模型分裂**（draft+Save vs 即时保存；切换/返回无 dirty 警告） | `SettingsContent.tsx` | 5/5 |
| **`bg-blue-500` 等硬编码色 vs accent token 不一致** | ConnectionPage、WorkflowPage、ThemeToggle 等 | 5/5 |
| **左侧模式轨 / Panel Tab 无障碍不足**（无 aria-label、无 tab ARIA、关闭按钮 hover-only） | `ConnectionPage.tsx`、`PanelTabBar.tsx` | 5/5 |
| **MenuBar / 快捷键平台化缺失**（Ctrl 写死，query hint 用 ⌘） | `MenuBar.tsx`、`en.ts` | 4/5 |
| **WindowControls aria-label 硬编码英文** | `WindowControls.tsx` | 5/5 |
| **WorkflowForm / WorkflowPage 硬编码英文** | `WorkflowForm.tsx`、`WorkflowPage.tsx` L704 | 4/5 |
| **`window.alert()` 破坏桌面体验** | `WorkflowPage.tsx`、`QueryPanel.tsx` | 4/5 |
| **DataTable 零行无空态 / loading 不可见** | `VirtualBody.tsx`、`DataTable.tsx` | 4/5 |
| **Select 固定 LIST_ID 重复** | `Select.tsx` | 4/5 |
| **巨型组件**（ConnectionNavigatorTree ~2800 行、WorkflowPage/QueryPanel ~1400 行） | 多处 | 4/5 |

### 部分审计员额外发现（2-3/5，仍值得关注）

| 问题 | 共识度 | 说明 |
|------|--------|------|
| 多连接无显式 Tab 条 | 2/5 | #4 强调；其他审计员未重点提及 |
| 复杂迁移工具学习曲线陡 | 3/5 | Data Sync / Schema Diff 等 |
| 全局无 Toast / 快捷键帮助 | 4/5 | 反馈分散、discoverability 不足 |
| permissionLabels 硬编码英文 | 2/5 | #5 强调 |
| 全局 user-select: none 副作用 | 2/5 | #3、#4 提及 |

---

## 维度评分矩阵（五方均值）

| 维度 | #1 | #2 | #3 | #4 | #5 | **均值** |
|------|----|----|----|----|-----|---------|
| 视觉设计 | 8.0 | 8.0 | 8.0 | 8.0 | 7.5 | **7.9** |
| 布局 / 信息架构 | 8.5 | 7.5 | 7.5 | 7.5 | 7.5 | **7.7** |
| 交互 / UX | 7.5 | 7.5 | 7.0 | 7.0 | 7.0 | **7.2** |
| 无障碍 (a11y) | 5.0 | 5.5 | 5.5 | 5.0 | 5.5 | **5.3** |
| 一致性 / 组件复用 | 7.0* | 7.0 | 7.5 | 7.0 | 7.0 | **7.1** |
| i18n | 7.5 | 7.5 | 8.0 | 7.5 | 7.5 | **7.6** |
| 空/错/加载态 | 7.5 | 7.0 | 7.0 | 7.5 | 7.0 | **7.2** |

\* #1 将一致性合并在「视觉设计」维度中

**观察**：
- **视觉设计（7.9）** 是最高分——token 体系、暗色默认、专业工具气质获一致认可
- **无障碍（5.3）** 是最大共识短板——5 位审计员均给出 5.0–5.5
- **交互/UX（7.2）** 被 Settings 保存模型、alert、复杂工具学习曲线拖累

---

## 综合优先改进项 Top 10（按五方共识排序）

| 优先级 | 改进项 | 共识度 | 预期影响 |
|--------|--------|--------|---------|
| **P0-1** | **Dialog 无障碍基线**：focus trap + Esc 关闭 + aria-label | 5/5 | 所有对话框受益；键盘/读屏用户 |
| **P0-2** | **ErrorBoundary i18n** | 5/5 | 全局崩溃页面向所有语言用户一致 |
| **P1-1** | **Settings 保存模型统一 + dirty 离开警告** | 5/5 | 防止静默丢失配置 |
| **P1-2** | **图标导航 / Panel Tab a11y**：aria-label、tab ARIA、关闭按钮可见 | 5/5 | 最高频导航路径 |
| **P1-3** | **`blue-500` → accent/danger token 统一** | 5/5 | 主题包一致性 |
| **P1-4** | **消灭 window.alert** | 4/5 | 专业桌面工具体验 |
| **P1-5** | **i18n 漏网清理**：WorkflowForm、WindowControls、日志级别、FK 表头 | 4/5 | 中文用户体验 |
| **P2-1** | **DataTable 空态 + loading 可见性** | 4/5 | 数据浏览核心路径 |
| **P2-2** | **MenuBar / 快捷键平台化** | 4/5 | Win/Linux 用户 |
| **P2-3** | **巨型组件拆分**（NavigatorTree、WorkflowPage、QueryPanel） | 4/5 | 可维护性与 UI 一致性 |

---

## 改进路线图建议

### 第一阶段（1 周）— 100% 共识项

```
Dialog a11y → ErrorBoundary i18n
```

- 改动面小、收益全局最大
- 文件：`Dialog.tsx`、`ErrorBoundary.tsx`

### 第二阶段（1–2 周）— 高共识交互/一致性

```
Settings 保存统一 → aria-label 批量补齐 → blue-500 替换 → window.alert 移除
```

- 文件：`SettingsContent.tsx`、`ConnectionPage.tsx`、`PanelTabBar.tsx`、`WorkflowPage.tsx`

### 第三阶段（2–3 周）— 体验 polish

```
DataTable 空态 → MenuBar 平台化 → i18n 漏网 → Select useId → Toast 系统
```

### 第四阶段（持续）— 结构性

```
NavigatorTree / WorkflowPage / QueryPanel 拆分 → 迁移工具 wizard → 快捷键帮助
```

---

## 审计分歧点（供决策参考）

以下问题并非五方一致，但个别审计员独立发现，值得产品侧评估：

| 分歧点 | 支持审计员 | 建议 |
|--------|-----------|------|
| 多连接需显式 Tab 条 | #4 | 专业用户多连接场景可调研后决定 |
| 左侧模式轨需展开文字标签 | #3、#5 | 可用 onboarding tooltip 替代永久展开 |
| 全局 user-select: none 副作用 | #3、#4 | 审计 help/文档区域是否需要 selectable |
| permissionLabels i18n | #5 | 插件管理是新增模块，中文用户占比高时优先 |

---

## 文件索引

| 文件 | 说明 |
|------|------|
| [audit-01.md](./audit-01.md) | 独立审计员 #1 完整报告（综合 7.4） |
| [audit-02.md](./audit-02.md) | 独立审计员 #2 完整报告（综合 7.3） |
| [audit-03.md](./audit-03.md) | 独立审计员 #3 完整报告（综合 7.2） |
| [audit-04.md](./audit-04.md) | 独立审计员 #4 完整报告（综合 7.2） |
| [audit-05.md](./audit-05.md) | 独立审计员 #5 完整报告（综合 7.0） |
| **00-summary.md**（本文件） | 五方共识汇总 |

---

## 结论

5 位独立审计员在无协调的情况下，对 DataZen 全应用 UI 给出了 **高度收敛** 的评价：

- **综合分 7.0–7.4**，均值 **7.2/10**
- **100% 共识** 的两个 critical 问题：`Dialog.tsx` 无障碍、`ErrorBoundary.tsx` i18n
- **100% 共识** 的三个 major 方向：Settings 保存模型、硬编码色、导航 a11y
- **无障碍（5.3）** 是唯一低于 6 分的维度，且五方评分极差仅 0.5

按 P0 → P1 → P2 路线图推进，无需大规模视觉改版，预计 **4–6 周** 可将综合体验稳定在 **8.0+**。
