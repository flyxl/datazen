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

定时执行先校验后台持有的 widget grant，再将 `monitorOccurrence` 受理为 SQLite durable Job，经共享 JobRuntime/Runtime 派发只读查询。`workflowRevision` 沿用生产 grant 的 64 位裸 SHA256；durable codec 接受该精确格式，并兼容原有 `sha256:` 标记形式。定义版本摘要仍保存为 `sha256:` 摘要，重复 schedule/definition/slot 返回原 receipt，不再执行 SQL。

仅 webdriver 构建提供按当前批准用户/widget 范围读取的阶段诊断：授权检查、occurrence 准备、Job 受理、派发、run 就绪或失败；最多保留 128 个 widget 的固定枚举，不保存或返回 SQL、凭据及底层错误。旅程等待真实 timer receipt，超时输出该固定阶段及 receipt 数量；受控 tick 仅用于真实 timer 首次成功后的重复 slot/撤权断言。


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
cargo test -p datazen_desktop --lib dashboard::
cargo test -p datazen_desktop --lib monitor::
npx vitest run src/windows/dashboard src/lib/dashboard
pnpm e2e:dashboard   # dashboard suite（e2e/specs/data-dashboard*.ts；与其他分组一致，需已有 webdriver 构建）
```

详见 [PRD](../../features/ops-dashboard-guide.zh-CN.md) 与 [实施方案](../../features/ops-dashboard-guide.en.md)。

## 服务端形态（W8）

W8 刀3d 点亮 dashboard 域：`crates/datazen-server/src/dashboard/` 四件 + `http/domains/dashboard.rs` 十五路由，与桌面宿主共用共享层的执行、登记与校验规则。存储走服务端管理库 **V14 三存储件**（`dashboards` / `widgets` / run history，组织复合主键）——行词汇与登记规则（保存归一化、消失组件清理、保留策略）单家在共享层，服务端存储只提供行。

| 文件 | 职责 |
|---|---|
| `creation.rs` | 共享组件创建用例的宿主（from-sql / from-workflow / 隐藏组件 SQL 更新）；dashboardHidden 私有面不进用户列表 |
| `execution.rs` | `ServerWidgetExecutionHost`（交互执行）+ `GrantScopedHost`（授权单作用域装饰器：规范目标比对 + 服务身份上下文） |
| `grants.rs` | monitor 授权面：authorize / revoke / read，桌面同链校验（ttl 词表、状态闸、工作流形状、只读 SQL AST、目标规范化、冻结事实复验） |
| `scheduler.rs` | `ServerMonitorHost` + `ServerMonitorScheduler`：定时刷新监控引擎的 server 接线 |

- **契约**（[HTTP API 契约](../platform/web-http-api.md) §6.15，十五路由）：CRUD 六 + 组件创建三 + 运行历史二 + 手工执行一 + monitor 授权三。
- **刷新监控引擎双端共享**：共享 `MonitorEngine`（到期判定 / 在途守卫 / arm-not-fire）+ server 接线（授权闸门失败关闭、授权单作用域执行、运行落账）——桌面 `MonitorEngine` 同款语义，两形态各自装配宿主件。
- **两个 `*_with_dialog` 不进 HTTP**：`export_dashboard_with_dialog` / `import_dashboard_with_dialog` 是桌面对话框语义；浏览器侧导出/导入走产物上传（契约 §6.5 在账端点）与下载。
