# 数据看板（Dashboard）

> [返回架构总览](../README.md)

将查询结果以 Workflow **最终输出（finalOutput）** 驱动的图表/表格展示在多面板看板中。与 Workflow 共用 `{data_dir}/datazen.sqlite`。

## 数据模型

```text
datazen.sqlite
├── workflows          # user | dashboardHidden
├── dashboards         # 面板（含 refresh_paused）
├── widgets            # 仅 workflow_id → finalOutput 表
└── widget_runs        # 刷新快照；隐藏 WF 不写 workflow_history
```

- SQL「添加到看板」→ 创建 `visibility = dashboardHidden` 单步 Workflow。
- 用户可见 Workflow 列表过滤 hidden；删除有引用时 FK / `find_workflow_refs` 拦截。
- 导出格式 `datazen.dashboard` **v2**，可附 `embeddedWorkflows`。

## 执行链

```text
Widget.refresh
  → WorkflowExecutor（内置 current_date / current_time 等）
  → 仅解析 final_output 为表格
  → 写入 widget_runs
  → （可选）告警通道
```

MonitorEngine 按面板 `refresh_paused` + 组件 `refresh.mode === interval`（≥30s）调度；默认手动刷新。

## 前端

| 路径 | 职责 |
|------|------|
| `src/windows/dashboard/` | 窗口、磁贴、编辑/历史抽屉、添加到看板对话框 |
| `src/stores/dashboardStore.ts` | 列表 / 挂载 / 刷新 / run-updated |
| `src/commands/dashboard.ts` | IPC 封装 |
| `src/lib/dashboard/` | run → chart / table 适配 |

Workflow 编辑页支持 **可视化 | YAML** 双模（`WorkflowYamlEditor` + `workflow_save_yaml`）。

## 测试

```bash
cargo test -p datazen --lib dashboard::
cargo test -p datazen --lib monitor::
npx vitest run src/windows/dashboard src/lib/dashboard
pnpm e2e:dashboard   # dashboard suite（e2e/specs/data-dashboard*.ts；与其他分组一致，需已有 webdriver 构建）
```

详见 [PRD](../../features/ops-dashboard-guide.zh-CN.md) 与 [实施方案](../../features/ops-dashboard-guide.en.md)。
