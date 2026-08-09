# 设计：多图运营看板（定时监控）

**日期：** 2026-08-09  
**状态：** 已批准  
**分析分支：** `docs/multi-chart-ops-dashboard-feasibility`（worktree: `.worktrees/multi-chart-ops-dashboard`）  
**实现计划：** [docs/superpowers/plans/2026-08-09-multi-chart-ops-dashboard.md](../plans/2026-08-09-multi-chart-ops-dashboard.md)（M1+M2；M3 另案）

## 目标

1. 在独立页面/窗口展示**多个图表**，每个图表绑定不同数据指标（SQL + 图表配置）。
2. 支持**运营监控**：后台定时刷新、阈值告警、查看**历史运行结果**。
3. MVP 以**托盘常驻**保证关看板/关主窗后仍可调度；无头服务与邮件发送为二期。
4. 看板配置纳入现有**应用数据导入导出**；并支持看板单文件分享。

## 非目标（MVP）

- 成为 Tableau 级 BI（跨视图联动筛选、语义模型、地图等）。
- 无头 `--monitor` / 登录项常驻服务（二期复用同一引擎）。
- 邮件实际发送（预留配置与 channel，MVP 不发送）。
- 修改现有单查询结果「表/图」切换的主路径行为（看板为独立产品面）。

## 已批准决策

| 议题 | 选择 |
|------|------|
| 产品形态 | **C**：定时监控看板（可保存） |
| 后台深度 | **C3** + **R3**：托盘常驻 MVP；无头服务二期 |
| 告警触达 | **A3**：桌面通知 + Webhook；邮件结构预留、二期发送 |
| 架构 | **方案二**：Rust `MonitorEngine` + 看板 UI 解耦 |
| 连接模型 | **监控专用连接**，与 UI session 隔离 |
| 历史 | 每次 run 落盘；默认保留 200 条 / 30 天 / 单次最多 500 行 |
| 导入导出 | 应用数据 ZIP 含看板定义 + 历史；另提供看板单文件（仅定义） |

## 可行性结论

**可行。** 现有 Recharts 管线（`ChartCanvas` / `transform` / `recommend` / `ChartConfig`）与 `QueryExecutor`、加密连接存储、应用数据 ZIP 均可复用。缺口是看板实体、网格 UI、监控调度、连接隔离、历史存储与告警通道——无架构死胡同，属中大型分期功能。

产品文档曾将 Dashboard 标为 P4；本设计将其提升为可选独立窗口，默认不干扰主连接/查询流程，并支持全局暂停监控。

---

## 1. 数据模型与持久化

### 1.1 实体

```
Dashboard
  id, name, createdAt, updatedAt
  layout: { cols: 12, rowHeight: number }
  widgets: DashboardWidget[]
  enabled: bool                    // 是否参与后台调度

DashboardWidget
  id, title
  configId: string                 // 持久化连接 ID（同 MCP）
  sql: string                      // 单语句；多语句取第一个结果集
  chartConfig: ChartConfig         // 复用 src/types/chart.ts
  layout: { x, y, w, h }
  refreshSec: number               // 下限 30
  alert?: AlertRule
  enabled: bool

AlertRule
  metric: {
    kind: 'column' | 'aggregation'
    column: string
    agg?: 'last' | 'max' | 'min' | 'avg' | 'sum'
  }
  op: '>' | '>=' | '<' | '<=' | '==' | '!='
  threshold: number
  cooldownSec: number              // 默认 300
  channels: ('desktop' | 'webhook' | 'email')[]

WidgetRun
  id, dashboardId, widgetId
  startedAt, finishedAt
  status: 'ok' | 'error' | 'timeout'
  error?: string
  rowCount: number
  columns: string[]
  rows: JsonValue[][]              // 最多 500 行
  alertFired?: bool
  alertValue?: number

MonitorSettings
  trayEnabled: bool
  closeToTray: bool                // 有启用看板时默认 true
  defaultWebhookUrl?: string
  email?: { /* SMTP 预留 */ }
  maxConcurrentQueries: number     // 默认 2
  exportIncludeDashboardRuns: bool // 默认 true；为 false 时应用数据导出跳过 dashboard-runs/
  runRetentionCount: number        // 默认 200 / widget
  runRetentionDays: number         // 默认 30
```

### 1.2 存储布局

```
{appData}/dashboards.json
{appData}/dashboard-runs/
  {dashboardId}/
    {widgetId}/
      {yyyy}/{mm}/{runId}.json
      index.jsonl
```

- 连接凭据仍仅存现有加密 connections；看板只引用 `configId`。
- 内存可缓存「每 widget 最新成功 run」以加速 UI；**每次调度结果必须落盘**。

### 1.3 历史回看

- 图表默认展示最新成功 run。
- `RunHistoryDrawer`：按时间点选择历史 run，用快照重绘（不重跑 SQL）。

### 1.4 导入导出

| 内容 | 应用数据 ZIP（现有整包 appData） | 看板单文件 `.datazen-dashboard.json` |
|------|--------------------------------|--------------------------------------|
| 看板定义 | ✅（`dashboards.json`） | ✅ |
| MonitorSettings / webhook | ✅ | ❌ |
| 运行历史 | ✅（`dashboard-runs/`） | ❌ |
| 连接凭据 | ✅（现有） | ❌（仅 `configId`；缺失时提示先导入连接或映射） |

IPC：`export_dashboard` / `import_dashboard`（单文件）。

应用数据 ZIP：`dashboards.json` 始终纳入；`dashboard-runs/` 默认纳入，当 `exportIncludeDashboardRuns=false` 时在 `app_data_archive::should_exclude`（或等价钩子）中跳过。导入后需补充测试断言上述路径存在/可恢复。

---

## 2. 调度引擎与告警

### 2.1 生命周期（MVP）

```
App 启动 → MonitorEngine::start()
  → 存在 enabled dashboard 时创建托盘（关主窗 ≠ 退出）
  → 按 widget.refreshSec 注册 interval
用户「退出」→ 停调度、关托盘
```

- 关闭看板窗口：**调度继续**。
- 托盘：打开看板 / 暂停全部监控 / 退出。
- 二期：`--monitor` 无头模式复用同一引擎，不依赖 WebView。

### 2.2 MonitorEngine

```
tick(widget):
  1. 获取监控专用连接（见 2.3）
  2. 执行 sql（默认超时 60s，可 widget 覆盖）
  3. 写 WidgetRun + index；裁剪超限历史
  4. 若有 AlertRule → 评估
  5. emit dashboard:run-updated（看板窗口打开时刷新）
  6. 触发告警 channels（cooldown）
```

约束：

| 项 | MVP |
|----|-----|
| 最小 refreshSec | 30 |
| 查询超时 | 60s |
| 全局并行 | `maxConcurrentQueries = 2` |
| 同 configId 监控 | 串行排队（避免自争用） |
| 失败重试 | 不等待重试；下一 interval 再跑 |
| 配置变更 | `save_dashboard` 后热更新调度表 |

### 2.3 连接隔离（关键）

现状：`ConnectionManager` 按会话复用；PG/MySQL 每会话 sqlx 池 `max_connections = 3`，SQLite `= 1`。若监控与 UI 共用 handle，会争用池槽位导致 UI 查询超时失败。

**决策：**

```
UI：      用户窗口 session handle
监控：    monitor_connect(config_id) → 独立 handle
          键：monitor:{config_id}
          驱动池建议 max_connections = 1
          空闲回收可短于 UI（如 5–10 min）
```

- 监控拿不到连接 → 本次 run=`error`，**不得**断开或占用 UI 已持有连接。
- SQLite 仍可能有文件锁；文档要求监控 SQL 短小；与 UI 互斥队列列为二期优化。

### 2.4 告警

- 从结果集取标量：`last` / `max` / `min` / `avg` / `sum` + `op` + `threshold`。
- 边沿触发：未告警→告警时通知；持续超阈靠 `cooldownSec`；默认发送「已恢复」一条。
- Channels：
  - `desktop`：系统通知（`tauri-plugin-notification`）；点击打开对应看板。
  - `webhook`：POST JSON（dashboard/widget/value/threshold/op/at）；用全局 `defaultWebhookUrl`。
  - `email`：MVP 不发送；配置 UI 灰显；日志 stub。
- 查询连续失败 ≥3 次才桌面通知（避免刷屏）。

### 2.5 安全

- Webhook URL 仅本地 settings；随应用数据 ZIP；**不进**看板单文件导出。
- SQL 信任边界同手动查询（用户自有连接与语句）。

---

## 3. UI / 窗口与前端

### 3.1 窗口

- `windowKind`: `dashboard`
- `openDashboardWindow(dashboardId?)`：MVP **按 id 多开**
- 入口：主菜单「运营看板」+ 托盘；「从查询添加到看板」为二期

### 3.2 页面结构

```
DashboardWindow
  ├─ 顶栏：名称 / 全局暂停·恢复 / 手动刷新全部 / 单看板导入导出
  ├─ 网格画布
  │    └─ ChartWidgetTile（紧凑 ChartCanvas + 状态/告警徽章）
  └─ WidgetEditorDrawer + RunHistoryDrawer
```

### 3.3 复用 vs 新建

| 复用 | 新建 |
|------|------|
| `ChartCanvas`、renderers、`transform`、`recommend`、`ChartConfig` | `ChartWidgetTile`、网格布局 |
| 主题 `charts.json` 调色板 | `dashboardStore`、`commands/dashboard.ts` |
| `QueryExecutor` / Driver（引擎侧） | `MonitorEngine`、托盘、通知、webhook |
| `app_data_archive` 整包 zip | `export_dashboard` / `import_dashboard` |

完整 `ChartView`（工具栏 + 轴侧栏）过重，tile 用紧凑模式；详细配置进抽屉。

### 3.4 设置

- MonitorSettings：托盘、并行度、Webhook、保留策略、导出含历史说明。
- Email SMTP：灰显 +「二期」。

---

## 4. 分期

| 阶段 | 范围 |
|------|------|
| **M1** | 模型 + CRUD + Dashboard 窗口 + 多图网格 + 手动刷新 + 历史落盘/回看 + ZIP 覆盖测试 + 看板单文件导入导出 |
| **M2** | MonitorEngine + 监控连接隔离 + 托盘 + desktop 通知 + Webhook + cooldown |
| **M3** | Email 发送、无头 `--monitor`、从查询「添加到看板」、SQLite 与 UI 互斥优化 |

---

## 5. 测试要点

- 单元：告警评估、历史裁剪、看板 JSON 导入校验。
- 集成：监控连接与 UI 会话并行查询，UI 侧不因监控出现 pool `acquire` 超时。
- E2E：打开看板 → 多 widget 渲染 → 单看板导出/导入。
- 手工：托盘关主窗后仍刷新并弹通知；应用数据 ZIP 含 `dashboards.json` 与 `dashboard-runs/`。

---

## 6. 风险

| 风险 | 缓解 |
|------|------|
| UI 与监控抢连接 | 监控专用 handle（§2.3） |
| 历史磁盘膨胀 | 保留策略 + 写时裁剪 |
| 定位偏离 DB 工具 | 独立窗口；可全局暂停 |
| 托盘/通知权限 | 首次引导；失败降级站内徽章 |
| 实现体量 | M1→M2→M3 |

---

## 7. 与现状对照（摘要）

- 图表：成熟单图管线，绑在 `QueryTab`；会话级、非持久化、同时仅一图可见。
- 窗口：无 `dashboard` kind；无网格多图。
- 连接：无应用级共享大池；驱动小池 per session——监控必须隔离。
- 导入导出：`export_app_data` 打包整个 appData（除 logs/`.key`）——看板文件放 appData 即可纳入。
