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
| F1 | 统一 `datazen.sqlite` Schema + AppDb CRUD | `done` | 7/7 | [Verify](0dc7d514) 16 PASS | `8a763c2` |
| F2 | Workflow 入库替换 YAML registry；visibility | `done` | 4/4 + IPC | [Verify](310a57bd) PASS | `c469639` |
| F3 | `current_time`；隐藏 WF 不写 workflow_history | `done` | 4/4 | [Verify](6ae9b084) PASS | `24455f6` |
| F4 | Workflow 编辑页：可视化 + YAML 双模 | `done` | PASS | [Verify](5f332e7e) PASS | `705858e` |
| F5 | 入口更名/直达窗口；移除设置→监控 | `done` | 70 | [Verify](f89eb288) PASS | `167887d` |
| F6 | 看板窗内面板 CRUD（SQLite） | `done` | dashboard:: | [Verify](5f332e7e) PASS | `705858e` |
| F7 | Widget 仅 workflowId；Executor；Monitor | `done` | dashboard+monitor | [Verify](5f332e7e) PASS | `705858e` |
| F8 | 图⇄表切换；编辑抽屉 | `done` | FE unit | [Verify](5f332e7e) PASS | `705858e` |
| F9 | 从 SQL / 可见 Workflow「添加到看板」 | `done` | create:: | [Verify](5f332e7e) PASS | `705858e` |
| F10 | 删除引用检查；面板暂停；运行历史 | `done` | Rust | [Verify](5f332e7e) PASS | `705858e` |
| F11 | 整体 Review + 修复验证 | `done` | 44+17+34 | [Verify](282d7a1e) 12/12 PASS | `66e1ba4` |
| F12 | 全量单测 + e2e:dashboard；文档收尾提交 | `done` | dashboard 44 / monitor 17 / FE 34；e2e **6/6** | — | `6dcd35f` |

## 变更日志

| 日期 | 内容 |
|------|------|
| 2026-08-12 | F0–F5 完成 |
| 2026-08-12 | F4 YAML 双模 + F6–F10 看板核心落地（`705858e`） |
| 2026-08-12 | F11：补齐面板 Tab、AddToDashboardDialog、hidden SQL 写回、仅 finalOutput、export v2、E2E 重写（`66e1ba4`） |
| 2026-08-12 | F12：`pnpm e2e:dashboard` 6/6 PASS；架构/手册文档收尾 |
