# v0.1.2 DataTable/Workflow 进度

## 功能摘要

- 状态：编码完成，待独立测试代理复验
- 范围：DataTable 空/加载状态、Workflow alert 替换和遗留 i18n。

### 编码结果

- DataTable 在表格滚动区域提供可访问的 loading/empty 状态，包含 `role=status`、live region、`aria-busy` 和本地化提示。
- WorkflowPage 移除 4 处原生 `window.alert`，改为可关闭的 `role=alert` 错误区域；YAML 解析、删除和仪表盘操作复用现有本地化 key。
- 未改变数据库、IPC、驱动或 Workflow 执行语义。

## E2E 登记

- 留待 R 阶段验证空态、加载态和 Workflow 错误反馈。

## 测试结果与覆盖率

- 定向 Vitest：4 个文件，51/51 通过；DataTable 空/加载态新增断言后 15/15 通过。
- `pnpm typecheck`：通过。
- `git diff --check`：通过。
- 覆盖率：待独立测试代理按 playbook 复验。

## 设计决策 / 遗留

- 不修改共享 locale 文件；使用 i18n contract 轨道预先冻结的 key。
