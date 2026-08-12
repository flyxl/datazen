# 数据看板实施方案

| 项目 | 内容 |
|------|------|
| 关联 PRD | [data-dashboard.md](./data-dashboard.md)（v1.2） |
| 文档版本 | v1.1 |
| 创建日期 | 2026-08-12 |
| 更新日期 | 2026-08-12 |
| 状态 | Draft |
| 目标分支 | `data-dashboard-prd` |
| 进度文件 | [data-dashboard-progress.md](./data-dashboard-progress.md) |
| 覆盖率门禁 | 前端相关包 **lines ≥ 80%**；Rust dashboard/monitor/workflow 相关模块单测 **lines ≥ 80%** |

---

## 0. 摘要与决策

将现有「运营看板」升级为 **Workflow 最终输出驱动的数据看板**。Workflow 定义与 Dashboard / Widget / WidgetRun **共用同一 SQLite**（`{data_dir}/datazen.sqlite`），通过 FK 引用；**不考虑老版本应用迁移**（可直接以新模型落地）。

| 决策 | 结论 |
|------|------|
| 存储 | **统一库** `datazen.sqlite`：`workflows` + `dashboards` + `widgets` + `widget_runs`；去掉 YAML 文件主存储与 `dashboards.json` |
| 数据源 | Widget 仅 `workflow_id`（FK → `workflows.id`）；始终取 `finalOutput`；禁止 `stepId` |
| SQL 入口 | 生成 `visibility = dashboardHidden` 单步 Workflow，不进 Workflow 列表 |
| **看板内编辑** | **支持**：隐藏源可在组件抽屉编辑连接+SQL（写回 Workflow）；可见源可改绑定，并可跳转 Workflow 双模编辑器调整后刷新 |
| **隐藏 WF 历史** | 看板刷新**只写** `widget_runs`；**不写** `workflow_history`；用户可见 WF 的手动/调度执行才写 history |
| **日期时间** | 使用引擎**内置变量** `current_date` / `current_time` / `current_month` / `current_year`；SQL 写 `{{current_date}}`；**不再**由看板层单独注入业务 date 入参 |
| Workflow 编辑 | 入库后，编辑页提供 **可视化 + YAML** 双模式 |
| 监控配置 | 删除设置「监控」分区；刷新 / 告警仅在看板编辑 UI |
| 主入口 | 主窗「数据看板」→ 直接 `openDashboardWindow()`，无创建 Dialog |
| 刷新默认 | `manual`；`interval` 最小值 30s；`< 60s` 非阻断警告 |
| 暂停作用域 | 面板工具栏「暂停本面板定时」 |
| 导出 | `format: datazen.dashboard` / `version: 2`，可附 `embeddedWorkflows` |
| 兼容 | **不迁移**旧 `dashboards.json` / `dashboard-runs` / YAML workflows（开发期可丢弃旧数据） |

---

## 1. 现状 → 目标对照

| 维度 | 现状 | 目标 |
|------|------|------|
| 产品名 | 运营看板 | **数据看板 / Dashboards** |
| 主入口 | 主窗 Dialog 创建/打开 | 直达看板窗口；窗内建面板 |
| Widget 源 | `configId` + `sql` | `workflowId` → finalOutput |
| 执行 | `driver.query_multi`（monitor 连接池） | `WorkflowExecutor::execute(id, vars)` |
| 图/表 | 仅图表磁贴 | 同一 Widget 内 chart ⇄ table |
| 刷新/告警 | 设置 → 监控 + Widget.`refreshSec` | 看板编辑抽屉 + `RefreshPolicy` |
| 存储 | JSON + YAML 文件 | **统一 `datazen.sqlite`（含 workflows）** |
| Workflow 可见性 | 全部列出 | `user` \| `dashboardHidden` |
| 删除 Workflow | 无引用检查 | FK / 引用检查阻止删除 |
| Workflow 编辑 | 仅可视化/表单 | **可视化 + YAML 双模** |
| 看板内改 SQL | 组件抽屉直绑 SQL | **编辑隐藏 Workflow（SQL/连接）并写回 DB** |

---

## 2. 用户旅程（User Journey）

以下旅程为产品验收与 E2E 覆盖的**完整集合**。每个 Journey 对应 §8 测试用例与 §9 E2E 场景 ID。

### UJ-01 主入口直达

```text
主窗口 → 点击「数据看板」→ 看板窗口打开（无 Dialog）
  ├─ 已有面板：默认选中最近/第一个面板画布
  └─ 无面板：空状态 CTA「创建第一个面板」→ 新建 → 空画布
```

**成功标准**：不出现 `dashboard-dialog`；窗口标题/文案为「数据看板」。

### UJ-02 窗内面板管理

```text
看板窗口 Tab 栏
  → 新建面板（命名）
  → 切换面板
  → 重命名
  → 删除（确认；级联删 widgets + runs）
```

### UJ-03 从 SQL 结果添加到看板（P0）

```text
连接窗口已保存连接 → 执行 SQL 得表格结果
  → 结果区 / ChartToolbar「添加到看板」
  → 选择目标面板（或新建）
  → 系统创建 dashboardHidden 单步 Workflow（connection + SQL；SQL 可含 {{current_date}} 等内置变量）
  → 创建 Widget（workflowId、viewMode 跟随 resultViewMode、refresh=manual）
  → 可选写入首条 WidgetRun 快照（**不**写入 workflow_history）
  → 聚焦到该面板组件
```

**约束**：无成功表格结果则按钮禁用；连接未保存则提示先保存。

### UJ-04 从可见 Workflow 最终结果添加（P0）

```text
Workflow 窗口跑通 → 最终输出可表格式化
  → 「添加到看板」
  → 选面板 → 绑定 workflowId（不绑 step）
  → 若在步骤详情点添加：提示「仍绑定最终输出」
```

> 看板源**不再要求**「仅日期类型用户变量」。日期时间请使用引擎内置 `{{current_date}}` / `{{current_time}}` 等。用户自定义 variables 仍可用于可见 Workflow 手动执行；看板定时刷新时以空/默认用户变量 + 内置变量执行（若 required 用户变量无默认值，保存为看板源时校验失败或提示改用内置变量）。

### UJ-05 组件图 ⇄ 表切换

```text
磁贴工具条「图 | 表」→ 切换 viewMode
  → 不触发 refresh / 不重跑 Workflow
  → ChartCanvas ↔ 只读 DataTable 读同一 WidgetRun
```

### UJ-06 组件编辑（含刷新、告警、SQL/Workflow 编辑）

```text
磁贴菜单「编辑」→ 抽屉
  → 标题 / 启用
  → 数据源：
       · 隐藏源（dashboardHidden）：编辑连接 + SQL（及说明可用 {{current_date}} 等内置变量）
         → 保存写回 workflows 表同一 id；可选「试跑」
       · 可见源：选择/更换 Workflow；「在 Workflow 编辑器中打开」→ 可视化或 YAML 调整后返回
  → 展示：默认视图 + chartConfig
  → 刷新：manual | onOpen | interval（<60s 警告）
  → 告警：metric / op / threshold / channels / cooldown
  → 保存 → 写 datazen.sqlite + 通知 MonitorEngine reload
```

### UJ-07 手动 / 定时 / 打开时刷新（内置日期变量）

```text
手动：工具栏「刷新」/「全部刷新」
打开时：面板打开且 widget.refresh.mode=onOpen → 执行一次
定时：enabled 面板 + interval 组件 → MonitorEngine 调度
  → WorkflowExecutor.execute（自动 set_builtin_variables）
  → SQL/模板中的 {{current_date}} / {{current_time}} 等解析为本地时间
  → 解析 finalOutput 表格 → 写 widget_runs（不写 workflow_history）→ 评告警 → 事件推送
```

### UJ-06b Workflow 双模编辑

```text
Workflow 窗口 → 编辑某条可见 Workflow
  → Tab：「可视化」|「YAML」
  → 可视化：步骤卡片/表单
  → YAML：直接编辑完整定义文本；失焦/保存时校验反序列化
  → 保存写入 datazen.sqlite.workflows
```

### UJ-08 暂停本面板定时

```text
面板工具栏「暂停本面板定时」→ 仅暂停当前面板 interval
  → 不改各组件 refresh 配置
  → 恢复后按原配置调度
```

### UJ-09 运行历史回看

```text
磁贴「历史」→ 列表（SQLite index）→ 选一条快照
  → 图/表均可展示该快照；不重跑
```

### UJ-10 删除 Workflow 引用检查

```text
Workflow 窗口删除可见 Workflow
  → findDashboardRefs(workflowId)
  → 有引用：阻止删除，对话框列出「面板名 / 组件标题」
  → 无引用：删除 YAML
```

### UJ-11 设置页无监控分区

```text
打开设置 → 无「监控」导航 / 表单项
  → 托盘仍可由引擎默认 + 是否存在启用 interval 驱动显示
```

### UJ-12 导入导出（Phase 2，方案预留）

```text
导出 version=2 JSON（含 embeddedWorkflows 隐藏定义）
  → 导入重建面板 + 隐藏 Workflow + 重绑 ID
```

### UJ-13 隐藏 Workflow 不出现在列表

```text
SQL 添加后 → Workflow 窗口 list 数量不变
  → 看板刷新仍可执行该隐藏定义
```

### Journey → 需求映射

| Journey | PRD |
|---------|-----|
| UJ-01 | DD-001, DD-002 |
| UJ-02 | DD-003 |
| UJ-03 | DD-006, DD-009 |
| UJ-04 | DD-004, DD-005, DD-010 |
| UJ-05 | DD-007 |
| UJ-06 / UJ-06b | DD-008, DD-012, DD-013 + SQL/WF 编辑 + 双模 |
| UJ-07 | DD-011（改为内置变量）, DD-017 |
| UJ-08 | DD-017 |
| UJ-09 | DD-016 |
| UJ-10 | DD-014 |
| UJ-11 | DD-015 |
| UJ-12 | DD-019 |
| UJ-13 | DD-006 |

---

## 3. 数据模型

### 3.1 领域对象（目标态）

```ts
type WorkflowVisibility = 'user' | 'dashboardHidden';

type RefreshPolicy =
  | { mode: 'manual' }
  | { mode: 'onOpen' }
  | { mode: 'interval'; refreshSec: number }; // clamp ≥ 30

interface Dashboard {
  id: string;
  name: string;
  createdAt: string; // RFC3339
  updatedAt: string;
  layout: { cols: number; rowHeight: number };
  enabled: boolean; // 是否参与后台 interval
  /** UI 临时态，可不落库或落 paused_until */
  refreshPaused?: boolean;
}

interface DashboardWidget {
  id: string;
  dashboardId: string;
  title: string;
  workflowId: string; // 唯一数据源；永远 finalOutput
  viewMode: 'chart' | 'table';
  chartConfig?: ChartConfig; // 切表时保留
  layout: { x: number; y: number; w: number; h: number };
  refresh: RefreshPolicy; // 默认 manual
  alert?: AlertRule;
  enabled: boolean;
  sortOrder: number;
}

interface WidgetRun {
  id: string;
  dashboardId: string;
  widgetId: string;
  workflowId: string;
  startedAt: string;
  finishedAt: string;
  status: 'ok' | 'error' | 'timeout';
  error?: string;
  rowCount: number;
  columnsJson: string; // JSON string[]
  rowsJson: string;    // JSON Value[][]，写入前 cap 500
  variablesJson?: string; // 本次注入的变量（含日期）
  alertFired?: boolean;
  alertValue?: number;
}

interface DashboardWorkflowRef {
  workflowId: string;
  dashboardId: string;
  widgetId: string;
  dashboardName: string;
  widgetTitle: string;
}
```

**显式删除字段**：`configId`、`sql`、`refreshSec`（标量）、`output.step` / `stepId`、用户可配的 `AppSettings.monitor`。

### 3.2 Workflow 存储与可见性

定义存于 `datazen.sqlite.workflows.definition_yaml`（完整 YAML 文本，供双模编辑）。

| 字段 | 说明 |
|------|------|
| `visibility` | `user` \| `dashboardHidden`；缺省 `user` |
| list API | 默认仅 `user`；引擎/看板内部可按 id 加载 hidden |
| 看板绑定 | 通过 `widgets.workflow_id` FK；刷新用内置变量，不强制「仅 date 用户入参」 |
| 隐藏编辑 | 组件抽屉改 SQL/连接 → 更新同一 `workflows` 行 |
| 可见编辑 | Workflow 页可视化或 YAML → `upsert` |

### 3.3 引擎内置默认（替代 MonitorSettings UI）

```rust
pub struct DashboardEngineDefaults {
    pub max_concurrent: u32,          // 2
    pub run_retention_count: u32,     // 200
    pub run_retention_days: u32,      // 30
    pub query_timeout_sec: u64,       // 60
    pub max_run_rows: usize,          // 500
    pub tray_when_interval_active: bool, // true
    pub min_refresh_sec: u32,         // 30
    pub refresh_warn_below_sec: u32,  // 60
}
```

迁移：读取旧 `AppSettings.monitor` 一次写入引擎运行时覆盖（可选），随后设置 UI 不再暴露；序列化可保留字段 `#[serde(default)]` 以免旧 settings 文件解析失败，但前端不展示。

### 3.4 导出格式 v2

```json
{
  "format": "datazen.dashboard",
  "version": 2,
  "dashboard": { /* 无 widgets 内嵌 SQL；仅 workflowId */ },
  "widgets": [ /* ... */ ],
  "embeddedWorkflows": [
    { "visibility": "dashboardHidden", /* full WorkflowDefinition */ }
  ]
}
```

Webhook URL 导出策略：沿用现状，默认剥离敏感 channel URL。

---

## 4. SQLite 数据库表结构（统一库）

### 4.1 文件位置与职责

| 文件 | 职责 |
|------|------|
| `{data_dir}/datazen.sqlite` | **唯一应用业务库**：workflows、dashboards、widgets、widget_runs |
| `{data_dir}/history.sqlite` | 查询历史 + **用户可见** Workflow 执行历史（hidden 不写入） |
| 废弃 | `dashboards.json`、`dashboard-runs/`、`workflows/*.yaml` 作为主存储（本迭代不迁移） |

模块建议：`src-tauri/src/store/app_db.rs`（`AppDb`），Dashboard / Workflow registry 均经此访问。

### 4.2 Schema（DDL）

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY NOT NULL,
  applied_at TEXT NOT NULL
);

-- Workflow 定义（原 YAML 文件）
CREATE TABLE IF NOT EXISTS workflows (
  id            TEXT PRIMARY KEY NOT NULL,
  name          TEXT NOT NULL,
  description   TEXT NOT NULL DEFAULT '',
  visibility    TEXT NOT NULL DEFAULT 'user'
                  CHECK (visibility IN ('user', 'dashboardHidden')),
  definition_yaml TEXT NOT NULL,          -- 完整 WorkflowDefinition 的 YAML 文本（双模编辑源）
  updated_at    TEXT NOT NULL,
  created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workflows_visibility
  ON workflows(visibility);

CREATE TABLE IF NOT EXISTS dashboards (
  id            TEXT PRIMARY KEY NOT NULL,
  name          TEXT NOT NULL,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  layout_cols   INTEGER NOT NULL DEFAULT 12,
  layout_row_height INTEGER NOT NULL DEFAULT 80,
  enabled       INTEGER NOT NULL DEFAULT 1,
  refresh_paused INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS widgets (
  id            TEXT PRIMARY KEY NOT NULL,
  dashboard_id  TEXT NOT NULL REFERENCES dashboards(id) ON DELETE CASCADE,
  title         TEXT NOT NULL,
  workflow_id   TEXT NOT NULL REFERENCES workflows(id) ON DELETE RESTRICT,
  view_mode     TEXT NOT NULL CHECK (view_mode IN ('chart', 'table')),
  chart_config_json TEXT,
  layout_x      INTEGER NOT NULL,
  layout_y      INTEGER NOT NULL,
  layout_w      INTEGER NOT NULL,
  layout_h      INTEGER NOT NULL,
  refresh_mode  TEXT NOT NULL CHECK (refresh_mode IN ('manual', 'onOpen', 'interval')),
  refresh_sec   INTEGER,
  alert_json    TEXT,
  enabled       INTEGER NOT NULL DEFAULT 1,
  sort_order    INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_widgets_dashboard
  ON widgets(dashboard_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_widgets_workflow
  ON widgets(workflow_id);

CREATE TABLE IF NOT EXISTS widget_runs (
  id            TEXT PRIMARY KEY NOT NULL,
  dashboard_id  TEXT NOT NULL,
  widget_id     TEXT NOT NULL REFERENCES widgets(id) ON DELETE CASCADE,
  workflow_id   TEXT NOT NULL,
  started_at    TEXT NOT NULL,
  finished_at   TEXT NOT NULL,
  status        TEXT NOT NULL CHECK (status IN ('ok', 'error', 'timeout')),
  error         TEXT,
  row_count     INTEGER NOT NULL DEFAULT 0,
  columns_json  TEXT NOT NULL,
  rows_json     TEXT NOT NULL,
  variables_json TEXT,
  alert_fired   INTEGER,
  alert_value   REAL
);

CREATE INDEX IF NOT EXISTS idx_widget_runs_widget_started
  ON widget_runs(widget_id, started_at DESC);

CREATE TABLE IF NOT EXISTS widget_latest_run (
  widget_id     TEXT PRIMARY KEY NOT NULL REFERENCES widgets(id) ON DELETE CASCADE,
  run_id        TEXT NOT NULL REFERENCES widget_runs(id) ON DELETE CASCADE,
  started_at    TEXT NOT NULL,
  status        TEXT NOT NULL
);
```

**引用完整性**：`widgets.workflow_id → workflows.id ON DELETE RESTRICT`，删除可见/隐藏 Workflow 前必须先解绑或删组件；`find_refs` 与 SQLite 错误均可用于 UX。

### 4.3 内置变量（Workflow Engine）

`WorkflowContext::set_builtin_variables()`（每次执行自动注入，用户无需声明）：

| 变量 | 格式（本地时区） | 状态 |
|------|------------------|------|
| `current_date` | `YYYY-MM-DD` | 已有 |
| `current_month` | `YYYY-MM` | 已有 |
| `current_year` | `YYYY` | 已有 |
| `current_time` | `HH:MM:SS` | **本迭代补齐** |

编辑 SQL / YAML 时可直接写：

```sql
SELECT * FROM orders WHERE order_date = '{{current_date}}'
```

看板刷新路径**不再**做额外「业务 date 入参注入」。

### 4.4 执行历史策略

| 场景 | `widget_runs` | `workflow_history` |
|------|---------------|--------------------|
| 看板刷新（含 hidden / 可见源） | ✅ | ❌ |
| Workflow 窗口手动执行（`visibility=user`） | — | ✅ |
| Workflow 窗口执行 hidden（若允许） | — | ❌ |
| Workflow 自带 schedule（user） | — | ✅（保持现状） |

### 4.5 AppDb API（建议）

| 方法 | 说明 |
|------|------|
| `open(data_dir) -> Arc<AppDb>` | 建库、migrate schema |
| Workflow：`list(visibility?)` / `get` / `upsert(yaml)` / `delete` | delete 前检查 refs 或依赖 RESTRICT |
| Dashboard：`list` / `get` / `upsert` / `delete` | CASCADE widgets/runs |
| Widget：`upsert` / `delete` | |
| `find_refs(workflow_id)` | 面板名+组件标题 |
| `write_run` / `list_runs` / `get_run` | |
---

## 5. 数据流向

### 5.1 总览

```text
┌─────────────┐  add from SQL   ┌──────────────────────────┐
│ QueryPanel  │ ───────────────►│ workflows 表 (hidden)    │
│ / Chart     │                 │ visibility=dashboardHidden│
└─────────────┘                 └────────────▲─────────────┘
                                             │ FK workflow_id
┌─────────────┐  add from WF    ┌────────────┴─────────────┐
│ Workflow UI │ ───────────────►│ widgets 表 (同库)         │
│ 可视 / YAML │                 └────────────┬─────────────┘
└─────────────┘                              │
                    refresh / schedule        ▼
                                   ┌─────────────────────┐
                                   │ MonitorEngine       │
                                   │ + builtin vars      │
                                   └──────────┬──────────┘
                                              ▼
                                   ┌─────────────────────┐
                                   │ WorkflowExecutor    │
                                   │ → finalOutput       │
                                   └──────────┬──────────┘
                                              ▼
                                   ┌─────────────────────┐
                                   │ widget_runs 写入    │
                                   │ （跳过 workflow_history）
                                   │ emit run-updated    │
                                   └─────────────────────┘
```

### 5.2 刷新执行链

```text
tick_widget(dashboard_id, widget_id)
  → load widget + dashboard.enabled + refresh_paused
  → load WorkflowDefinition from AppDb (含 hidden)
  → WorkflowExecutor.execute(def, vars={}, timeout=60s)
       // set_builtin_variables: current_date/time/month/year
  → parse_final_output_as_table(final_output)
  → evaluate_alert(...)
  → write widget_runs + prune  // 绝不写 workflow_history
  → emit dashboard:run-updated
```

### 5.3 从 SQL 添加

```text
FE: addToDashboard({ configId, sql, viewMode, chartConfig?, title?, dashboardId? })
  → IPC create_widget_from_sql
  → BE: INSERT workflows (hidden yaml) + INSERT widgets（同事务）
  → 可选首屏 snapshot → widget_runs
  → FE focus 看板窗
```

### 5.4 删除 Workflow

```text
workflow_delete(id)
  → refs = find_refs OR SQLite RESTRICT
  → 有引用：Err(Validation { refs })
  → 无引用：DELETE FROM workflows
```

### 5.5 IPC 变更

| IPC | 变更 |
|-----|------|
| dashboard CRUD / runs | 走 AppDb |
| `create_widget_from_sql` / `create_widget_from_workflow` | 新增 |
| `find_dashboard_workflow_refs` | 新增 |
| `set_dashboard_refresh_paused` | 面板级暂停 |
| `workflow_list` | 默认仅 `user`；过滤 hidden |
| `workflow_save` / `workflow_save_yaml` | 写入 AppDb；支持 YAML 原文 |
| `workflow_delete` | 引用检查；RESTRICT |
| `workflow_execute` | `user` → 写 history；hidden/dashboard 路径 → 不写 history |

---

## 6. 前端 UI/UX

### 6.1 信息架构

```text
主窗口
  └─ 「数据看板」→ openDashboardWindow()   // 无 Dialog

看板窗口
  ├─ 顶栏：面板 Tabs [A][B][+] | 导入/导出 | 帮助
  ├─ 工具栏：全部刷新 | 暂停本面板定时 | 添加组件
  ├─ 画布：Widget 磁贴网格
  │     ├─ 角标/工具条：图 | 表
  │     └─ 菜单：编辑 / 刷新 / 历史 / 删除
  └─ 抽屉
        ├─ WidgetEditorDrawer（数据源 / 展示 / 刷新 / 告警）
        └─ RunHistoryDrawer

连接窗口 QueryPanel / ChartToolbar
  └─ 「添加到看板」→ 面板选择器 Popover/Dialog（非主窗创建看板 Dialog）

Workflow 窗口最终结果区
  └─ 「添加到看板」→ 同上；非法入参 toast/对话框说明

设置窗口
  └─ 移除 monitor section 与相关 i18n 入口
```

### 6.2 关键界面规格

#### 主窗口

- 去掉 `dashboard-dialog` 及相关 create/list UI。
- 按钮文案：`action.dashboard` → 「数据看板」/「Dashboards」。
- 点击即开窗；若多显示器/已开窗则 focus 已有看板窗。

#### 看板窗口空状态

- 文案：「还没有面板」+ 主按钮「创建第一个面板」。
- 不引导回主窗口。

#### 磁贴

- **图 | 表** 分段控件；切换只改 `viewMode` 并 `save`（可 debounce），**不** invoke run。
- Chart：复用 `ChartCanvas` + `runToChart`。
- Table：只读 `DataTable` + `runToResult`。
- 状态：无 run / running / error / ok；错误展示可折叠。
- `refresh_mode=interval` 时角标显示间隔；`refreshSec < 60` 仅在编辑抽屉警告，磁贴可不重复。

#### 编辑抽屉分区（顺序固定）

1. **基本**：标题、启用开关  
2. **数据源（可编辑）**：  
   - **隐藏源**：连接选择 + SQL 编辑器（提示内置变量 `{{current_date}}` 等）+「试跑」；保存写回 `workflows`  
   - **可见源**：Workflow 下拉更换；链接「在 Workflow 编辑器打开」（可视化/YAML）  
3. **展示**：默认视图、图表类型与轴  
4. **刷新**：Radio manual / onOpen / interval；min=30；`<60` 非阻断警告  
5. **告警**：可选；通道含本组件 Webhook URL  

#### Workflow 编辑页双模

- Tab：`可视化` | `YAML`  
- YAML：Monaco/CodeMirror；保存前 `serde_yaml` 反序列化校验  
- 两者切换前可提示未保存变更  

### 6.3 文案与 i18n（摘录）

| Key（建议） | zh-CN |
|-------------|-------|
| `action.dashboard` | 数据看板 |
| `win.dashboard` | 数据看板 - DataZen |
| `dashboard.emptyBoards` | 还没有面板，创建第一个开始 |
| `dashboard.addToDashboard` | 添加到看板 |
| `dashboard.view.chart` / `.table` | 图 / 表 |
| `dashboard.refresh.manual` | 手动 |
| `dashboard.refresh.onOpen` | 打开面板时 |
| `dashboard.refresh.interval` | 定时 |
| `dashboard.refresh.warnDense` | 刷新间隔短于 60 秒可能对数据库造成较大负担，请确认必要。 |
| `dashboard.pausePanel` | 暂停本面板定时 |
| `dashboard.workflowInUse` | 该工作流仍被看板引用，无法删除 |
| 移除 | `settings.monitor.*` 整组 |

同步 `zh-CN` / `zh-TW` / `en`（及项目已有 locale）。

### 6.4 组件文件落点（建议）

| 路径 | 职责 |
|------|------|
| `src/windows/dashboard/DashboardWindow.tsx` | 壳：Tabs、工具栏、空状态 |
| `src/windows/dashboard/WidgetTile.tsx` | 原 ChartWidgetTile 扩展图/表 |
| `src/windows/dashboard/WidgetEditorDrawer.tsx` | 新模型编辑 |
| `src/windows/dashboard/AddToDashboardDialog.tsx` | 共用选择器 |
| `src/windows/dashboard/RunHistoryDrawer.tsx` | SQLite runs |
| `src/lib/dashboard/*` | parse final、日期注入预览、refresh 警告、迁移类型 |
| `src/stores/dashboardStore.ts` | 面板暂停、viewMode、事件 |
| `src/windows/main/MainWindow.tsx` | 直达开窗 |
| `src/windows/settings/SettingsWindow.tsx` | 删 monitor |
| `src/windows/connection/QueryPanel.tsx` | 添加入口 |
| `src/windows/workflow/WorkflowWindow.tsx` | 添加入口 + 删除拦截 UI |

---

## 7. 分期实施

### Phase 0 — 入口与文案（可先合）

- DD-001 / DD-002 / DD-015  
- i18n、主窗直达、设置去监控  
- 托盘文案改为「打开数据看板 / 暂停定时刷新」  
- **暂不改**存储与执行路径（仍 JSON+SQL），但避免新增强依赖 `AppSettings.monitor` UI  

### Phase 1 — MVP

1. `AppDb`（`datazen.sqlite`）统一 Schema  
2. Workflow 入库 + visibility + 双模编辑  
3. 内置 `current_time`；hidden/看板刷新不写 workflow_history  
4. Widget 仅 `workflowId`；执行走 Executor  
5. 图⇄表、抽屉内 SQL/WF 编辑、刷新/告警、面板暂停、历史  
6. SQL / Workflow「添加到看板」  
7. 删除引用检查  
8. E2E + 覆盖率门禁  

> **不做**旧 JSON/YAML 迁移。

### Phase 2

- 布局拖拽、导入导出 v2、隐藏 WF 孤儿回收  
- 托盘全局暂停（可选）  
- 面板级 Webhook 默认  

### Phase 3

- 看板级日期范围、KPI、MCP  

---

## 8. 测试用例（单元 / 集成）

> Host 测试只验证宿主能力；不测驱动方言。SQLite 夹具可用内存或 tempfile。

### 8.1 Rust — `dashboard/db.rs`

| ID | 用例 | 期望 |
|----|------|------|
| UT-DB-01 | 空库 open + migrate | 表存在，version=1 |
| UT-DB-02 | upsert dashboard + widgets | get 返回有序 widgets |
| UT-DB-03 | delete dashboard | CASCADE 删 widgets/runs |
| UT-DB-04 | find_refs | 多面板多组件正确列出 |
| UT-DB-05 | write_run + list_run_index | 最新在前；row cap 500 |
| UT-DB-06 | prune retention count/days | 超限删除文件行 |
| UT-DB-07 | widget_latest_run 更新 | 与最新 ok/error 一致 |
| UT-DB-08 | refresh_sec clamp | 5 → 30；非法 mode 拒绝 |
| UT-DB-09 | 迁移 dashboards.json | 生成 hidden yaml + widgets 无 sql 列 |
| UT-DB-10 | 迁移 dashboard-runs | runs 可 get |
| UT-DB-11 | 迁移幂等 | 二次启动不重复插入 |
| UT-DB-12 | FK 打开时插入无效 dashboard_id | 失败 |

### 8.2 Rust — execute / alert / 日期注入

| ID | 用例 | 期望 |
|----|------|------|
| UT-EX-01 | 无 variables 执行 | 成功解析表格 |
| UT-EX-02 | date 变量 | 注入本地今天 |
| UT-EX-03 | 多 date 变量 | 均同一今天 |
| UT-EX-04 | finalOutput 非表 | status=error，有 error 文案 |
| UT-EX-05 | timeout | status=timeout |
| UT-EX-06 | 告警触发 | alert_fired + channel 调用（mock） |
| UT-EX-07 | 告警 cooldown | 冷却期内不重复 |
| UT-EX-08 | 禁止走旧 query_multi 主路径 | 单测/集成断言调用 executor（可用 mock） |

### 8.3 Rust — Workflow visibility / 删除

| ID | 用例 | 期望 |
|----|------|------|
| UT-WF-01 | list 过滤 hidden | 不含 dashboardHidden |
| UT-WF-02 | list_all 含 hidden | 引擎能加载 |
| UT-WF-03 | delete 有引用 | Err + refs |
| UT-WF-04 | delete 无引用 | Ok |
| UT-WF-05 | 缺省 visibility | 视为 user |
| UT-WF-06 | 绑定校验 string 变量 | create_widget_from_workflow 失败 |

### 8.4 Rust — MonitorEngine

| ID | 用例 | 期望 |
|----|------|------|
| UT-MON-01 | manual 不进 schedule | |
| UT-MON-02 | interval + dashboard.enabled | 进入调度表 |
| UT-MON-03 | refresh_paused | 跳过该面板 |
| UT-MON-04 | onOpen 不在后台 tick | 仅打开时 FE/IPC 触发 |
| UT-MON-05 | 并发信号量 | ≤ 2 |

### 8.5 前端 Vitest

| ID | 范围 | 要点 |
|----|------|------|
| UT-FE-01 | `dashboardStore` | run-updated、paused、viewMode 切换不 invoke run |
| UT-FE-02 | `lib/dashboard` | runToChart/Result、日期格式、dense refresh warn |
| UT-FE-03 | `MainWindow` | 点击开窗、**无** dialog |
| UT-FE-04 | `SettingsWindow` | 无 monitor section |
| UT-FE-05 | `WidgetTile` | 图⇄表切换渲染 |
| UT-FE-06 | `WidgetEditorDrawer` | refresh 三模式、`<60` 警告、保存 payload |
| UT-FE-07 | `AddToDashboardDialog` | 选面板 / 新建 |
| UT-FE-08 | QueryPanel 入口 | 无结果禁用；有结果可点 |
| UT-FE-09 | WorkflowWindow | 删除引用错误展示；添加入口 |
| UT-FE-10 | i18n keys | 关键文案存在（可轻量 snapshot） |

### 8.6 手工 / 黑盒（`test/`，可选补充）

- 托盘在存在 interval 组件时出现；打开看板  
- 关看板窗后 interval 仍跑（引擎存活）  

---

## 9. E2E 用例（覆盖全部 User Journey）

**规范**：Host WebdriverIO；`pnpm e2e:dashboard`；夹具用 basic/SQLite 或已有 E2E 连接，**不**断言驱动方言。将 `e2e/specs/ops-dashboard.ts` 重命名/拆分为 `e2e/specs/data-dashboard*.ts`。

### 9.1 文件与 Journey 映射

| Spec 文件（建议） | 覆盖 Journey |
|-------------------|--------------|
| `e2e/specs/data-dashboard-entry.ts` | UJ-01, UJ-11 |
| `e2e/specs/data-dashboard-boards.ts` | UJ-02, UJ-08 |
| `e2e/specs/data-dashboard-sql-add.ts` | UJ-03, UJ-13 |
| `e2e/specs/data-dashboard-workflow-add.ts` | UJ-04, UJ-10 |
| `e2e/specs/data-dashboard-widget-ux.ts` | UJ-05, UJ-06, UJ-09 |
| `e2e/specs/data-dashboard-refresh.ts` | UJ-07 |
| `e2e/specs/data-dashboard-import.ts` | UJ-12（Phase 2） |

`package.json`：`e2e:dashboard` 跑上述 specs（Phase 1 不含 import）。

### 9.2 场景明细

#### E2E-01 主入口直达（UJ-01）

1. 启动主窗 → 点击 `action.dashboard`  
2. **断言**：新窗口打开；**不存在** `[data-testid=dashboard-dialog]`  
3. 无面板时见空状态 CTA；创建面板后画布可见  

#### E2E-02 设置无监控（UJ-11）

1. 打开设置  
2. **断言**：无 monitor 导航项 / `settings-monitor-*`  

#### E2E-03 面板 CRUD（UJ-02）

1. 新建面板 A、B → 切换 → 重命名 B → 删除 B（确认）  
2. **断言**：Tab 与 `list_dashboards` 一致；删后面板 runs 不可再 get  

#### E2E-04 SQL 添加到看板（UJ-03, UJ-13）

1. 保存连接 → 跑 `SELECT 1 AS n` → 「添加到看板」→ 选/建面板  
2. **断言**：看板出现组件；`workflow_list` 长度与添加前相同  
3. 刷新组件 → 表格/图有数据  
4. IPC `get_dashboard`：widget 含 `workflowId`，无 `sql`  

#### E2E-05 Workflow 添加与非法入参（UJ-04）

1. 准备仅 date 变量（或无变量）且 final 为表的 Workflow → 添加成功  
2. 准备含 `type: string` 变量的 Workflow → 添加失败，有错误提示  
3. 多步 Workflow：改中间步不影响绑定契约（绑定仍无 step 选择器）  

#### E2E-06 图⇄表（UJ-05）

1. 有 ok run 的图表组件 → 切「表」→ 见 DataTable 同行数  
2. **断言**：切换期间 **无** 新 `run_dashboard_widget`（可通过 run 列表条数不变证明）  
3. 切回「图」仍显示  

#### E2E-07 编辑刷新与警告（UJ-06）

1. 打开编辑 → 设 interval=30 → 见警告条 → 保存成功  
2. 设 manual → 保存 → 调度不跑该组件（配合暂停/时间 stub 或 refresh_sec 极大对比）  

#### E2E-08 日期注入刷新（UJ-07）

1. 隐藏/可见 WF：`SELECT '{{biz_date}}' AS d` 或等价可断言输出  
2. 手动刷新 → run.variables 含今天；结果列等于今天（本地）  

#### E2E-09 全部刷新与面板暂停（UJ-07, UJ-08）

1. 两组件 interval → 「全部刷新」两 run 更新  
2. 「暂停本面板定时」→ 等待超过 interval → **无**新自动 run  
3. 恢复 → 之后有新 run  

#### E2E-10 运行历史（UJ-09）

1. 连续刷新 2 次 → 历史列表 ≥2  
2. 点旧快照 → 展示旧数据；不增加 run 数  

#### E2E-11 Workflow 删除引用检查（UJ-10）

1. 可见 WF 已绑组件 → 删除 → 失败对话框列出面板/组件名  
2. 删组件后 → 删除 WF 成功  

#### E2E-12 隐藏 WF 列表隔离（UJ-13）

1. 仅通过 SQL 添加产生 hidden → Workflow 窗 UI 列表不可见该 id  
2. `workflow_get(hiddenId)` 仍可读（若 IPC 允许；或仅引擎内部）  

#### E2E-13 导入导出（UJ-12，Phase 2）

1. 导出 v2 → 删面板 → 导入 → 组件可刷新；hidden 定义恢复  

### 9.3 E2E 夹具约定

- 使用 `invokeBackend('save_dashboard' | 'create_widget_from_sql' | …)` 加速 seed，但**每个 Journey 至少有一条从真实 UI 走完的路径**。  
- `data-testid` 建议：`dashboard-window`、`dashboard-tab`、`dashboard-empty`、`widget-tile`、`widget-view-chart`、`widget-view-table`、`add-to-dashboard`、`widget-editor`、`refresh-warn-dense`、`pause-panel-refresh`。  
- 清理：`after` 删测试面板 + 测试 workflow 文件。  

---

## 10. 代码覆盖率要求（≥ 80%）

### 10.1 前端（Vitest + v8）

将下列路径加入 `vitest.config.ts` 的 `coverage.include` 与 `thresholds`（与现有 Option C 一致，**lines ≥ 80**）：

```text
src/windows/dashboard/**/*.{ts,tsx}
src/lib/dashboard/**/*.{ts,tsx}
src/stores/dashboardStore.ts
```

建议阈值：

| 路径 | lines | statements | functions | branches |
|------|-------|------------|-----------|----------|
| `src/lib/dashboard/**` | 80 | 80 | 75 | 70 |
| `src/stores/dashboardStore.ts` | 80 | 80 | 75 | 55 |
| `src/windows/dashboard/**` | 80 | 80 | 70 | 55 |

`MainWindow` / `SettingsWindow` / `workflow/**` 已有 80% 门禁，本需求改动必须保持不降。

命令：`pnpm test:unit:coverage`（CI 已有则对齐）。

### 10.2 Rust

| 模块 | 要求 |
|------|------|
| `src-tauri/src/dashboard/**` | `cargo llvm-cov` 或 `tarpaulin`：**lines ≥ 80%**（Phase 1 引入脚本 `pnpm test:cov:dashboard-rust` 或文档约定本地门禁） |
| `monitor/engine.rs` 调度相关 | ≥ 80% lines（重点 schedule / pause） |
| `workflow/registry.rs` visibility + delete refs | 关键路径全覆盖 |

最低可执行集（CI 必跑）：

```bash
cargo test -p datazen --lib dashboard::
cargo test -p datazen --lib monitor::
# 以及 workflow registry/delete 相关测试名过滤
pnpm test:unit:coverage
pnpm e2e:dashboard
```

### 10.3 覆盖率与 E2E 分工

| 逻辑 | 单测 | E2E |
|------|------|-----|
| SQLite CRUD / 迁移 / refs | ✅ | 抽样 |
| 日期注入 / final 解析 / alert | ✅ | 一条注入场景 |
| 图⇄表不重跑 | store/组件测 | ✅ |
| 主入口无 Dialog、去监控设置 | 组件测 | ✅ |
| SQL/WF 添加完整漏斗 | IPC 测 | ✅ 主路径 |
| 托盘 | 单元（纯函数） | 可选手工 |

---

## 11. 风险与开放问题（实施侧）

| 项 | 缓解 |
|----|------|
| finalOutput 字符串形态不一 | 统一 parser：优先 JSON `{columns,rows}`；兼容已有 command 结果；失败明确 error |
| 隐藏 WF 与用户 WF 同目录 | list 过滤 + 文件名/元数据；导出打包 embedded |
| JSON→SQLite 迁移损坏 | `.bak` + 迁移单测 + 启动日志 |
| 去掉设置后并发不可调 | 内置默认 2；Phase 2 面板高级 |
| 抽屉内完整 SQL 编辑 | **已拍板：允许**，写回 hidden Workflow |
| 日期变量 | **已拍板**：用引擎内置 `current_date`/`current_time` 等，看板不另注入 |
| 统一库 | **已拍板**：`datazen.sqlite` 含 workflows + dashboards |
| 隐藏执行历史 | **已拍板**：不写 `workflow_history` |
| 老版本迁移 | **不做** |

**已拍板（继承 PRD + v1.1）**：去设置监控、直达窗口、图必可切表、仅 Workflow final、默认手动、`<60s` 警告、删除引用检查、暂停当前面板、看板可编辑 SQL/WF、统一 SQLite、内置日期变量、WF 双模编辑。

---

## 12. 验收清单（Phase 1）

- [ ] 主页进入无创建弹窗，文案为数据看板  
- [ ] 设置无监控分区  
- [ ] 100% 组件仅 `workflowId`；列表无 hidden  
- [ ] SQL/Workflow 添加漏斗可用  
- [ ] 图⇄表不重查  
- [ ] 刷新时 date 注入今天  
- [ ] 被引用 Workflow 不可删  
- [ ] 旧 JSON 看板迁移后可刷新  
- [ ] `pnpm test:unit:coverage` 达标；dashboard 相关 Rust 单测达标  
- [ ] `pnpm e2e:dashboard` 覆盖 UJ-01～UJ-11、UJ-13  

---

## 13. 参考路径

| 区域 | 路径 |
|------|------|
| PRD | `docs/prd/data-dashboard.md` |
| 本方案 | `docs/prd/data-dashboard-implementation.md` |
| 现看板 UI | `src/windows/dashboard/` |
| 现类型 | `src-tauri/src/dashboard/types.rs` |
| 现执行 | `src-tauri/src/dashboard/execute.rs` |
| 现引擎 | `src-tauri/src/monitor/engine.rs` |
| Workflow | `src-tauri/src/workflow/` |
| History SQLite 范式 | `src-tauri/src/store/history_db.rs` |
| 现 E2E | `e2e/specs/ops-dashboard.ts` |
