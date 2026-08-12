# 数据看板开发进度

| 项目 | 内容 |
|------|------|
| 分支 | `data-dashboard-prd` |
| 关联方案 | [data-dashboard-implementation.md](./data-dashboard-implementation.md) |
| 关联 PRD | [data-dashboard.md](./data-dashboard.md) |
| 更新规则 | 每完成一个功能开发/测试/提交后更新本文件 |

## 设计拍板（相对方案 v1.0 的变更）

| # | 决策 | 说明 |
|---|------|------|
| 1 | **看板内可编辑 SQL / Workflow** | 组件编辑抽屉：隐藏源可改连接+SQL；可见源可改绑定或打开 Workflow 编辑；改完写回同一 DB |
| 2 | **`dashboardHidden` 不写 Workflow History** | Widget 刷新只写 `widget_runs`；`workflow_history` 仅记录用户可见 Workflow 的手动/调度执行 |
| 3 | **统一 SQLite** | 单库 `{data_dir}/datazen.sqlite`：workflows + dashboards + widgets + widget_runs（可 FK）；**不考虑老版本迁移** |
| 4 | **内置日期时间变量** | 引擎已有 `current_date` / `current_month` / `current_year`；补齐 `current_time`；看板刷新**不再**单独注入业务 date 变量，SQL 直接写 `{{current_date}}` |
| 5 | **Workflow 入库 + 双模编辑** | 定义存 SQLite；Workflow 编辑页提供「可视化」与「YAML」两种编辑模式 |

## 功能清单与状态

| ID | 功能 | 状态 | 单测 | E2E agent | 提交 |
|----|------|------|------|-----------|------|
| F0 | 更新实施方案 + 进度文件 + 设计拍板 | `done` | n/a | n/a | `c7d0cea` |
| F1 | 统一 `datazen.sqlite` Schema（workflows/dashboards/widgets/runs）+ AppDb CRUD | `done` | 7/7 | [Verify](0dc7d514) 16 PASS | `8a763c2` |
| F2 | Workflow 入库替换 YAML registry；visibility；list 过滤 hidden | `done` | 4/4 + IPC | [Verify](310a57bd) PASS | `c469639` |
| F3 | 内置变量补齐 `current_time`；文档化；隐藏 WF 不写 workflow_history | `done` | 4/4 | [Verify](6ae9b084) PASS | pending |
| F4 | Workflow 编辑页：可视化 + YAML 双模 | `pending` | — | — | — |
| F5 | 入口更名/直达窗口；移除设置→监控 | `pending` | — | — | — |
| F6 | 看板窗内面板 CRUD（SQLite） | `pending` | — | — | — |
| F7 | Widget 仅 workflowId；执行改 WorkflowExecutor；Monitor 调度 | `pending` | — | — | — |
| F8 | 图⇄表切换；编辑抽屉（刷新/告警/**SQL·WF 编辑**） | `pending` | — | — | — |
| F9 | 从 SQL / 可见 Workflow「添加到看板」 | `pending` | — | — | — |
| F10 | 删除 WF 引用检查；面板暂停；运行历史；日期变量用法验收 | `pending` | — | — | — |
| F11 | 整体 Review + 修复验证 | `pending` | — | — | — |
| F12 | 全量单测 + e2e:dashboard；文档收尾提交 | `pending` | — | — | — |

**状态枚举**：`pending` → `developing` → `testing` → `fixing` → `done`

## 测试循环约定

1. 功能开发时同步写单元测试并本地跑通。
2. 功能完成后：**新建独立 agent** 做该功能 E2E/验证测试；只输出用例、结果、bug 重现步骤，**不修复**。
3. 若不通过 → 编码 agent 修复 → 再开**新**测试 agent 回归。
4. 通过后更新本文件并 `git commit`。
5. 全部功能完成后：整体 review → 新 agent 验证 → 全量单测+E2E → 修问题 → 更新文档并提交。

## 变更日志

| 日期 | 内容 |
|------|------|
| 2026-08-12 | 创建进度文件；写入五项设计拍板；功能拆分为 F0–F12 |
| 2026-08-12 | F0 文档提交 `c7d0cea` |
| 2026-08-12 | F1 AppDb 落地；独立测试 agent 16 PASS |
| 2026-08-12 | F2 Workflow 入库 + visibility；测试 agent PASS；`c469639` |
