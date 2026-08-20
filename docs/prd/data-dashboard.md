# DataZen 数据看板 PRD

| 项目 | 内容 |
|------|------|
| 产品模块 | 数据看板（Data Dashboard） |
| 文档版本 | v1.2 |
| 创建日期 | 2026-08-12 |
| 更新日期 | 2026-08-12 |
| 状态 | Draft |
| 关联现状 | 现称「运营看板 / Ops Dashboard」；实现见 `src-tauri/src/dashboard/`、`src-tauri/src/monitor/`、`src/windows/dashboard/` |
| 关联文档 | [实施方案](./data-dashboard-implementation.md)、[开发进度](./data-dashboard-progress.md)、[运营看板手册（待更名）](../ops-dashboard-guide.md)、[Workflow 手册](../workflow-guide.md)、[图表设计](../chart-visualization-design.md)、[Tableau 对比](../tableau-comparison.md) |

---

## 1. 背景与问题

### 1.1 现状

DataZen 已具备「运营看板」能力：多图网格、SQL + 已保存连接作为数据源、后台 `MonitorEngine` 定时刷新、阈值告警、运行历史、托盘、导入导出。查询结果与 Workflow 步骤结果已支持表/图切换，但**无法一键沉淀到看板**。

### 1.2 问题

1. **定位过窄**：产品文案与「设置 → 监控」把场景锁在运维盯盘，掩盖经营分析主场景。
2. **数据源分裂**：组件直绑 SQL，与 Workflow 编排割裂，定时刷新时日期参数等能力难统一。
3. **展示形态不足**：看板磁贴仅有图表，且图/表不能在同一组件内切换。
4. **创作路径断裂**：调好指标后无法钉到看板；主窗口先弹「创建看板」对话框打断心智。
5. **设置错位**：刷新/告警放在全局设置，而用户真正要「监控的是某一个看板」。

### 1.3 机会

将 SQL 指标统一收敛为（可隐藏的）单步 Workflow，看板**只认 Workflow 最终输出**；定时刷新时注入日期入参。数据看板成为指标展示、定时与告警的统一层。

---

## 2. 产品定位

### 2.1 一句话

**数据看板**是 DataZen 内统一的指标展示、定时刷新与告警框架：数据分析人员定义经营指标并以图表（可切换数据表格）呈现，可对转化率、GMV 等做定时刷新与阈值告警；运维人员在同一框架下配置技术指标与告警。**监控的对象是具体看板**，而不是全局「监控开关」。

### 2.2 不是什么

| 是 | 不是 |
|----|------|
| 桌面端、以 Workflow 最终结果为源的指标看板 | Tableau / Power BI 级语义模型与自助 BI |
| 可保存、可定时刷新、可告警的组件画布 | 仅单次查询结果可视化（连接窗口「表/图」） |
| 分析与运维共用的同一套框架 | 两套「分析看板 / 运维看板」；也不是「设置里的监控模块」 |
| SQL / Workflow 产出的「沉淀」层 | 替代 Workflow 编排或 SQL 探索；也不在看板绑定 Workflow 中间步骤 |

### 2.3 与相邻能力的关系

```text
SQL 编辑器                     Workflow 窗口（用户可见流水线）
    │                                    │
    │ 添加到看板                          │ 添加到看板
    ▼                                    │
 自动生成「看板专用」单步 Workflow         │
 （不出现在 Workflow 列表）                 │
    │                                    │
    └────────────┬───────────────────────┘
                 ▼
        ┌─────────────────────┐
        │      数据看板         │
        │ 唯一数据源：Workflow  │
        │ 取 finalOutput 表格   │
        │ 图 ⇄ 表 切换 / 告警   │
        └─────────────────────┘
```

| 能力 | 目的 | 与看板 |
|------|------|--------|
| SQL 编辑器结果「表/图」 | 探索、一次性看数 | 「添加到看板」→ 生成隐藏单步 Workflow 并挂组件 |
| Workflow（用户可见） | 多步编排、参数化、AI | 执行**最终输出**作为看板数据源；删除前校验看板引用 |
| 数据看板 | 展示经营/技术指标；看板内配置定时与告警 | 本 PRD 主体 |
| 后台刷新引擎 / 托盘 | 关窗后仍可调度与通知 | 由**看板/组件上的定时配置**驱动，无独立「设置 → 监控」 |

### 2.4 命名策略

| 场景 | 命名 |
|------|------|
| 产品主入口（中文） | **数据看板** |
| 产品主入口（英文） | **Dashboards** |
| 设置 | **移除「监控」分区**；刷新/告警只在看板窗口内配置 |
| 手册 / 帮助 | 「数据看板」 |
| 内部模块 | 可暂保留 `dashboard` / `monitor` 技术名 |

---

## 3. 目标用户与场景

### 3.1 人物角色

| 角色 | 诉求 | 典型用法 |
|------|------|----------|
| **数据分析 / 经营分析**（主） | GMV、转化、留存；图/表切换；定时 + 告警（如转化率下降） | 从 SQL/Workflow 钉指标；看板内开定时与告警 |
| **数据工程师** | 流水线上墙；日期参数报表 | 可见 Workflow（日期入参）→ 看板 |
| **运维 / 值班** | 延迟、错误率；告警与托盘 | 同一看板框架 |
| **开发 / DBA** | 调试查询沉淀 | SQL → 隐藏单步 Workflow → 看板 |

### 3.2 核心用户故事

1. 作为分析师，我从主页打开数据看板后直接进入看板窗口，在窗口内新建面板并添加组件，而不是先被创建对话框拦住。
2. 作为分析师，我把 SQL 结果添加到看板后，系统自动生成不出现在 Workflow 列表里的单步流水线，看板统一只依赖 Workflow。
3. 作为分析师，我绑定的 Workflow 只需保证**最终输出**是表格；不需要也不允许指定某一步。
4. 作为分析师，我在图表磁贴上一键切换到数据表格查看明细（同一组件、同一份 run 数据）。
5. 作为分析师，我的指标 SQL 依赖「当天日期」入参；开启定时刷新时，系统把当前日期注入 Workflow 再执行。
6. 作为分析师/运维，我在**该看板的编辑界面**配置刷新间隔与告警，而不是去设置页找「监控」。
7. 作为用户，我删除 Workflow 时若仍被看板引用，系统阻止删除并列出引用，避免看板静默失效。

---

## 4. 目标与非目标

### 4.1 目标

1. 定位与文案：**数据看板**；定时/告警服务经营与技术指标。
2. **唯一数据源：Workflow 最终输出**（表格形态）。从 SQL 添加时自动物化为隐藏单步 Workflow。
3. **图/表一体**：图表组件必可切换到对应数据表格（同一 widget、同一 `WidgetRun`）。
4. 主入口：**直接打开看板窗口**；在窗口内管理/创建面板（Dashboard）。
5. **刷新与告警仅在看板编辑 UI 内配置**；去掉设置中的监控分区。
6. Workflow 入参约束：作为看板源时，**仅支持日期类型入参**（可无入参）；定时刷新时注入「当前日期」。
7. **默认手动刷新**；`refreshSec < 60` 时非阻断警告数据库负担（允许最小值仍为 30s）。
8. 删除 Workflow **必须**做看板引用检查。

### 4.2 非目标

- 指定 Workflow **中间步骤**作为看板输入。
- 看板组件再挂一套独立的「原生 SQL 数据源」（对外模型上取消；内部仅通过隐藏 Workflow 实现）。
- 设置页中的全局「监控」模块。
- Tableau 级跨图联动、语义层、地图、Web Portal。
- 在看板内重建完整 SQL IDE / Workflow 编辑器。
- 邮件告警真正发信（可预留）。
- 非日期类型的看板 Workflow 入参（如自由字符串、多选枚举）——本期不做。
- 跨机器自动同步依赖。

---

## 5. 快速创建入口分析

> **SQL 结果区与 Workflow 运行结果区为 P0**；主窗口只负责打开看板窗口；刷新/告警不在设置页。

### 5.1 候选入口

| 入口 | 推荐 | 建议形态 |
|------|------|----------|
| **SQL 结果区 / ChartToolbar** | **P0** | 「添加到看板」→ 选已有面板或在看板窗口新建 → 生成隐藏单步 Workflow + 组件；默认图/表视图跟随当前 `resultViewMode` |
| **Workflow 运行结果（最终输出）** | **P0** | 「添加到看板」→ 绑定该 `workflowId`（仅 final）；若在步骤详情点添加，语义仍是「用该 Workflow 的最终输出」，并提示不绑定本步 |
| **主窗口「数据看板」** | **P0** | **直接 `openDashboardWindow()`**，无创建弹窗 |
| **看板窗口内** | **P0** | 面板列表 / 新建面板、添加组件、编辑刷新与告警 |
| ChartToolbar | P1 | 与结果区共用 action |
| 查询收藏 / 表浏览 / AI | P2 或不做 | — |
| **设置 → 监控** | **移除** | 不再作为入口或配置面 |
| 托盘 | 控制面 | 打开看板、暂停/恢复**已开启定时的看板**调度 |

### 5.2 交互原则

1. 有成功表格结果才启用「添加到看板」。
2. SQL 路径：连接须已保存；生成隐藏 Workflow 时写入 `configId` + SQL（及日期变量占位，见 §8）。
3. Workflow 路径：校验 final 可表格式化；校验入参仅含日期类型（或空）。
4. 默认 `refresh.mode = manual`；定时与告警在看板内组件编辑中配置。

---

## 6. 信息架构（目标态）

| 入口 | 路径 | 职责 |
|------|------|------|
| 打开看板 | 主窗口 → **数据看板** | **直接进入看板窗口**（无创建 Dialog） |
| 看板窗口 | 独立窗口 | 面板列表、新建/重命名/删除面板、画布、组件、**刷新与告警编辑**、导入导出 |
| 从查询添加 | SQL 结果工具栏 | 生成隐藏 Workflow + 挂到选定面板 |
| 从 Workflow 添加 | Workflow 运行成功后的最终结果区 | 绑定可见 Workflow |
| 设置 | — | **无监控/看板刷新分区** |
| 帮助 | 帮助 → 数据看板 | 使用说明 |
| 托盘 | 有定时组件时 | 打开看板 / 暂停定时刷新 / 退出 |

### 6.1 看板窗口信息架构（示意）

```text
┌─ 数据看板窗口 ─────────────────────────────────────┐
│ [面板 A] [面板 B] [+] 新建面板    导入/导出  帮助     │
│────────────────────────────────────────────────────│
│ 工具栏：全部刷新 | 暂停本面板定时 | 添加组件          │
│────────────────────────────────────────────────────│
│  磁贴网格（组件：图⇄表、菜单含编辑）                 │
│                                                    │
│  编辑抽屉：数据源(Workflow) / 展示 / 刷新 / 告警      │
└────────────────────────────────────────────────────┘
```

「暂停定时」作用域为**当前面板（Dashboard）**：暂停当前面板上所有 interval 组件（符合「监控具体看板」）。托盘可另提供全局暂停（可选增强），不替代面板工具栏默认行为。

---

## 7. 概念模型

### 7.1 面板（Dashboard）

命名画布：组件列表、布局、面板级启用（是否参与后台定时）。用户在看板窗口内创建多个面板。

### 7.2 组件（Widget）

```text
Widget = WorkflowSource + View (+ RefreshPolicy) (+ optional Alert)
```

| 部分 | 说明 |
|------|------|
| **WorkflowSource** | 唯一数据源：`workflowId`；始终取 **finalOutput**（表格） |
| **View** | `viewMode: 'chart' \| 'table'`；`chartConfig` 在 chart 模式使用；**任何曾以图表展示的数据均可切到 table** |
| **RefreshPolicy** | 默认 `manual`；可选 `onOpen` / `interval` |
| **Alert** | 可选；经营与技术指标共用 |

要点：**不存在独立的「只能表、不能图」与「只能图、不能表」两套互斥组件类型作为主模型**。用户建的是指标组件；默认视图可以是图或表，但只要存在表格型 run 数据，即可切换到表。若当前是 chart 视图，则**一定**能切到 table（同一份 columns/rows）。

### 7.3 运行记录（WidgetRun）

每次取数快照。图与表都读同一快照；历史回看不重跑。

### 7.4 唯一数据源：Workflow

#### 7.4.1 用户可见 Workflow

- 在 Workflow 窗口创建/编辑的流水线。
- 作为看板源时约束见 §8.3。
- **只取 finalOutput**，不支持 `stepId`。

#### 7.4.2 看板专用隐藏 Workflow（由 SQL 添加生成）

| 项 | 约定 |
|----|------|
| 触发 | 「从 SQL 添加到看板」或看板内「从 SQL 新建」 |
| 形态 | 单步 query/command Workflow，connection + SQL（+ 日期变量） |
| 可见性 | **`visibility: dashboardHidden`（或等价）**；**不出现在 Workflow 窗口列表 / 选择器** |
| 生命周期 | 随组件删除可孤儿回收（P1）；导出面板时可内联或一并导出定义 |
| 编辑 | 用户改 SQL：在组件编辑中改（写回该隐藏 Workflow），不引导去 Workflow 页 |

这样看板运行时只有一条执行路径：`executeWorkflow(workflowId, variables)` → 解析 final 表格。

### 7.5 日期入参与定时刷新

**约束**：挂到看板的 Workflow（含隐藏单步）允许的 variables：

- 无 variables，或  
- 仅包含 **类型为日期（date / datetime，产品定一种主类型，建议 `date`）** 的变量。

若存在非法类型入参（string/number/bool 等），**不可选为看板数据源** / 保存组件时校验失败。

**定时或手动刷新时的注入规则**：

| 场景 | 行为 |
|------|------|
| Workflow 无日期变量 | 直接执行 |
| 有日期变量 | 刷新时传入**当前日期**（本地时区，格式与 Workflow 变量声明一致，如 `YYYY-MM-DD`） |
| 多日期变量 | 均注入同一「当前日期」（本期简化）；若未来要「开始/结束」再扩展看板级日期范围 |

**典型场景**：SQL 中 `WHERE order_date = {{biz_date}}`，隐藏 Workflow 声明 `biz_date: date`；每小时定时刷新时自动把当天日期传入，无需人工改参。

---

## 8. 功能需求

### 8.1 功能总览

```text
数据看板
├── 看板窗口（直接进入；窗内建面板）
├── 面板管理（新建 / 重命名 / 删除 / 导入导出）
├── 组件
│   ├── 数据源：仅 Workflow（最终输出）
│   ├── 展示：chart ⇄ table 切换
│   ├── 刷新：默认手动；可选打开时 / 定时（<60s 警告）
│   └── 告警：在组件/面板编辑内配置
├── 从 SQL 添加 → 隐藏单步 Workflow
├── 从可见 Workflow 添加（final only）
├── 删除 Workflow 引用检查
└── 后台引擎 / 托盘（由面板定时配置驱动；无设置页监控）
```

### 8.2 需求清单

| 编号 | 功能 | 优先级 | 描述 |
|------|------|--------|------|
| DD-001 | 产品更名与文案 | P0 | 「数据看板」；去掉设置「监控」入口与文案 |
| DD-002 | 主入口直达窗口 | P0 | 主窗口点击后直接打开看板窗口，**不弹创建 Dialog** |
| DD-003 | 窗内面板管理 | P0 | 在看板窗口内新建/切换/重命名/删除面板 |
| DD-004 | **唯一 Workflow 数据源** | P0 | 组件只绑 `workflowId`；只读 finalOutput |
| DD-005 | **禁止 step 绑定** | P0 | 数据模型与 UI 均无 step 选择器 |
| DD-006 | **SQL → 隐藏单步 Workflow** | P0 | 从 SQL 添加时生成；不进 Workflow 列表 |
| DD-007 | **图⇄表切换** | P0 | chart 视图必可切 table；共用 WidgetRun |
| DD-008 | 组件编辑（含刷新/告警） | P0 | **刷新与告警只在看板编辑 UI**；无设置页监控 |
| DD-009 | 从 SQL 结果添加 | P0 | QueryPanel / ChartToolbar |
| DD-010 | 从 Workflow 最终结果添加 | P0 | 绑定 workflowId；校验日期入参约束 |
| DD-011 | 日期入参注入 | P0 | 刷新时对 date 变量注入当前日期 |
| DD-012 | 默认手动刷新 | P0 | 新建默认 `manual`；`refreshSec < 60` 时非阻断警告数据库负担（最小值仍 ≥30） |
| DD-013 | 告警 | P0 | 组件编辑内配置；经营/技术示例 |
| DD-014 | **删除 Workflow 引用检查** | P0 | 有引用则阻止删除并列出面板/组件 |
| DD-015 | 移除设置→监控 | P0 | 删除该设置分区；并发/保留等用合理默认或后续再议 |
| DD-016 | 运行历史 | P0 | 图/表均可按快照回看 |
| DD-017 | 全部刷新 / 暂停本面板定时 | P0 | 作用域为当前面板 |
| DD-018 | 布局拖拽 | P1 | 网格拖拽缩放 |
| DD-019 | 导入导出 | P1 | 含可见/隐藏 Workflow 依赖处理 |
| DD-020 | 隐藏 Workflow 孤儿回收 | P1 | 组件删除后清理无引用隐藏定义 |
| DD-021 | KPI 单值 + 可切表 | P2 | 可选 |
| DD-022 | 看板级日期范围（多日期变量） | P2 | 开始/结束日 |
| DD-023 | MCP | P3 | — |

### 8.3 Workflow 作为看板源（细则）

#### 8.3.1 可选条件

1. Workflow 存在且可加载。  
2. 最近一次或试跑 finalOutput 可解析为 `columns` + `rows`（保存时试跑或依赖用户刚跑出的结果）。  
3. `variables` 为空，或全部为日期类型。  
4. 非（或允许）隐藏类型：用户选择器**只列出可见 Workflow**；隐藏类型仅由系统创建。

#### 8.3.2 执行

1. 看板刷新 → `executeWorkflow(id, { dateVars: today, ... })`。  
2. 解析 **finalOutput** 为表格；失败则组件 error。  
3. 不支持截取中间 step。  
4. 超时/并发：沿用引擎侧默认（设置页不再暴露时，采用保守默认，如并行 2）。

#### 8.3.3 删除引用检查（DD-014）

删除可见 Workflow 时：

1. 扫描所有面板组件的 `workflowId`。  
2. 若存在引用：**禁止删除**，对话框列出「面板名 / 组件标题」。  
3. 用户需先解除绑定或删除组件后再删 Workflow。  
4. 隐藏 Workflow 不出现在 Workflow 页，删除入口主要在组件删除/回收流程。

#### 8.3.4 验收

- SQL 添加后 Workflow 列表项数不增加，但看板刷新能跑通。  
- 多步 Workflow 改中间步不影响「绑 final」的契约；改最终产出逻辑则下次刷新生效。  
- 带 `{{biz_date}}` 的查询在定时刷新下使用当天日期。  
- 试图绑定含 string 入参的 Workflow 时被拒绝并说明原因。

---

## 9. 快速创建详细需求

### 9.1 从 SQL 结果添加（DD-009 / DD-006）

1. 用户点击「添加到看板」。  
2. 若看板窗口未开可打开；选择目标面板（或新建面板）。  
3. 系统创建隐藏单步 Workflow（连接 + SQL；若 SQL 含日期绑定参数则声明 date 变量）。  
4. 创建 widget：`workflowId` 指向该隐藏定义；`viewMode` 随当前表/图；`refresh = manual`；写入可选首条 run 快照。  
5. 跳转/聚焦到该面板组件。

### 9.2 从可见 Workflow 添加（DD-010）

1. 前置：最终输出为表格；入参合法。  
2. 「添加到看板」→ 选面板 → 绑定该 `workflowId`。  
3. 默认手动刷新；图/表视图可跟随当前查看模式。

---

## 10. 交互与界面要点

### 10.1 主入口

- 点击「数据看板」→ **立即打开看板窗口**。  
- 窗口内若无面板：空状态「创建第一个面板」，而非退回主窗 Dialog。

### 10.2 组件磁贴

- 工具条或角标：**图 | 表** 切换（等同 QueryPanel 的 resultViewMode）。  
- Chart 模式：ChartCanvas；Table 模式：只读 DataTable。  
- 切换**不重新查询**，只切换呈现。  
- 菜单：编辑、刷新、历史、删除。

### 10.3 编辑抽屉（刷新与告警在此）

1. 标题、启用  
2. 数据源：选择可见 Workflow /（若为隐藏源）展示只读 SQL 与连接，允许编辑 SQL 并写回隐藏 Workflow  
3. 展示：默认视图 chart/table；图表轴配置（切到表仍保留 chartConfig）  
4. **刷新**（本页配置，非设置页）：  
   - 默认「手动」  
   - 「打开面板时」  
   - 「定时」+ 秒数（最小值保持 ≥30）；**当 `refreshSec < 60` 时展示非阻断警告**：定时过密可能对数据库造成较大负担，请确认必要。  
5. **指标告警**：阈值、通道（桌面 / Webhook URL 可写在本组件或本面板）；示例含转化率、错误率  

### 10.4 面板级「监控」

- 面板开关：`enabled`——关闭后该面板全部 interval 不调度。  
- 「暂停本面板定时」：临时暂停，不改组件配置。  
- 托盘：存在任意面板的启用 interval 组件时显示；菜单可打开看板窗口。

### 10.5 设置页

- **删除「监控」整个分区**（含托盘、Webhook、保留策略等表单项）。  
- Webhook 默认值改为组件/面板编辑内填写。  
- 运行历史保留策略：采用内置默认（如 200 条 / 30 天），本期不在设置暴露；必要时 P2 再放到面板高级设置。

---

## 11. 数据模型变更（草案）

```ts
/** 看板专用隐藏 Workflow，不进 Workflow 窗口列表 */
type WorkflowVisibility = 'user' | 'dashboardHidden';

interface Dashboard {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  layout: DashboardLayout;
  widgets: DashboardWidget[];
  /** 面板是否参与后台 interval 调度 */
  enabled: boolean;
}

type RefreshPolicy =
  | { mode: 'manual' } // 默认
  | { mode: 'onOpen' }
  | { mode: 'interval'; refreshSec: number };

interface DashboardWidget {
  id: string;
  title: string;
  /** 唯一数据源：始终取 Workflow finalOutput */
  workflowId: string;
  /** chart 时可切 table；table 为默认时亦可再配 chartConfig 后切图 */
  viewMode: 'chart' | 'table';
  chartConfig?: ChartConfig;
  layout: WidgetLayout;
  refresh: RefreshPolicy; // 默认 { mode: 'manual' }
  alert?: AlertRule;
  enabled: boolean;
}
```

**显式删除**：

- Widget 上的 `configId` / `sql` 直绑（迁移：生成隐藏 Workflow 后只留 `workflowId`）。  
- `output.step` / `stepId`。  
- `AppSettings.monitor` 作为用户可配设置面（引擎可保留内部默认常量）。

导出：`format: "datazen.dashboard"` → `version: 2`；可附带 `embeddedWorkflows` 以导出隐藏定义。

兼容迁移：旧 `configId+sql+chartConfig+refreshSec` → 创建 `dashboardHidden` Workflow + `viewMode: 'chart'` + `refresh: { mode: 'interval', refreshSec }`（或按产品决定旧定时是否保留）。

---

## 12. 技术约束与复用

| 项 | 说明 |
|----|------|
| 执行路径 | 统一 `WorkflowExecutor`；取消 dashboard 专用 `query_multi(sql)` 主路径（或仅作迁移期回退） |
| final 归一 | 与 executor 现有 `rows`/`columns` JSON 对齐 |
| 隐藏 Workflow 存储 | 与用户 Workflow 同存储或分文件，但 list API 过滤 `dashboardHidden` |
| 图/表 | `ChartCanvas` + `DataTable`；切换不触发 refresh |
| 日期 | 刷新前解析 variables schema，对 date 类写入 `today` |
| 引用索引 | 删除 Workflow 前 `findDashboardRefs(workflowId)` |
| 行数/超时 | 约 500 行 / 60s（可调内部常量） |
| 测试 | Host E2E；不测驱动方言 |

---

## 13. 分期计划

### Phase 0 — 入口与文案

- DD-001、DD-002、DD-015（去设置监控、直达窗口、更名）

### Phase 1 — MVP

- DD-003 窗内面板管理  
- DD-004～DD-007 唯一 Workflow 源、禁 step、隐藏 SQL Workflow、图⇄表  
- DD-008～DD-013 编辑内刷新/告警、日期注入、默认手动、过短警告  
- DD-014 删除引用检查  
- DD-016～DD-017 历史与面板暂停  
- 旧数据迁移  

### Phase 2

- 布局拖拽、导入导出嵌隐藏 Workflow、孤儿回收  
- 面板级 Webhook 默认、保留策略高级项  

### Phase 3

- 看板级日期范围、KPI、MCP  

---

## 14. 成功指标

| 指标 | 说明 |
|------|------|
| 入口 | 用户从主页进入不再经过创建弹窗即可看到看板窗口 |
| 创作漏斗 | SQL/Workflow「添加到看板」完成率 |
| 源模型 | 100% 组件仅 workflowId；Workflow 列表无隐藏项 |
| 图表 | 图表磁贴可切表且无需重新查询 |
| 安全 | 被引用 Workflow 无法被误删 |
| 回归 | 定时 + 告警在面板内配置后仍可后台运行 |

---

## 15. 风险与开放问题

| 风险 / 问题 | 建议 |
|-------------|------|
| 隐藏 Workflow 与用户 Workflow 存储耦合 | 明确 list 过滤与权限；导出打包 |
| finalOutput 非表格 | 保存/添加时强校验 |
| 「只支持日期入参」过严 | 允许无入参；文档说明字符串参数需改写进 SQL 或后续迭代 |
| 去掉设置后并发/保留不可调 | Phase 1 用保守默认；Phase 2 面板高级 |
| 旧看板 SQL 迁移 | 自动生成隐藏 Workflow |
| interval 警告阈值 | **已拍板**：`refreshSec < 60` 警告（最小值仍 30） |
| 暂停定时作用域 | **已拍板**：默认暂停**当前面板**；托盘全局暂停为可选增强 |

**已拍板（本版）**：

1. 设置中移除监控；刷新/告警在看板编辑页。  
2. 主入口不弹创建窗，窗内建面板。  
3. 图必可切表。  
4. 数据源仅 Workflow final；SQL 变隐藏单步 Workflow。  
5. 不支持 step 输入源。  
6. 看板源 Workflow 仅日期入参（或无入参）；定时注入当前日期。  
7. 默认手动刷新；`refreshSec < 60` 非阻断警告。  
8. 删除 Workflow 必须引用检查并阻止。  
9. 「暂停定时」默认作用域为**当前面板**。  

**仍待确认**：

1. 隐藏 Workflow 的 SQL 编辑是否允许在组件抽屉内完整编辑（推荐允许）。  
2. 多日期变量是否一律注入同一「今天」（本期建议是）。  
3. 托盘是否需要「全局暂停所有面板定时」（P1 可选）。  

---

## 16. 附录

### 16.1 现状基线

- 主窗 Dialog 创建/打开；设置→监控；SQL 直绑；仅图表磁贴；无 Workflow 源；无引用检查。

### 16.2 文案对照

| 原 | 新 |
|----|----|
| 运营看板 | 数据看板 |
| 设置 → 监控 | **移除** |
| 暂停监控 | 暂停本面板定时刷新 / 托盘：暂停定时刷新 |
| 创建并打开（主窗 Dialog） | 直接进入窗口；窗内「新建面板」 |

### 16.3 参考代码

- 看板 UI：`src/windows/dashboard/`（`DashboardPanel` 等）  
- 主入口：`src/windows/main/MainPage.tsx` → `ConnectionPage` 内嵌 `DashboardPanel`  
- 设置：`src/windows/settings/SettingsPage.tsx` / `SettingsContent.tsx`（monitor 分区已移除）  
- SQL：`src/windows/connection/QueryPanel.tsx`  
- Workflow：`src/windows/workflow/WorkflowPage.tsx`  
- 执行：`src-tauri/src/dashboard/execute.rs`、`monitor/engine.rs`、`workflow/executor.rs`
