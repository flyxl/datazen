# T8 工作流引擎 — 测试报告

- 任务：宿主实现 `WorkflowHost`（connection_name 走 profiles、execute_command 走真执行、complete_ai 如实报"AI 未配置"、sqlite 标准方言），`WorkflowExecutor` 走 N1 作业通道，步骤结果仍进 W7 报告 UI。
- 日期：2026-11-14
- 提交：见 tasks.md T8 行。

## 结论

通过。GR1 138+55、GR3 46 全绿。

## 实现面（如实）

- **词汇**：`AdapterJob::RunWorkflow { generation, name }` + `AdapterDelivery::WorkflowRun { generation, results }`。UI 的运行按钮改作业优先（`workflow_generation`/`workflow_loading` 代次守卫，Q5 同款）；拒绝时回落 N13 同步跑法 → 内存端口演示。
- **宿主四方法**（`LiveWorkflowHost`）：
  - `connection_name(id)`：经连接用例查文件存储（名字是报告的 connectionName）。
  - `execute_command(step, workflow_connection, permission_mode, deadline)`：deadline 已过 → 如实拒绝（引擎 deadline 契约的宿主半边）；解析步/工作流的连接必须是活会话；`query` 命令=活会话驱动真跑 SQL（行限生效、truncated 标记、结果按共享层 `MultiQueryResult` 形状——引擎 `normalize_query_result` 直接消费）；`list_tables`/`get_schema`/`describe_table`=驱动目录真读；其余命令如实报 not wired。**计划文档"进程执行"措辞已更正**：引擎的 command 词汇是七类读工具，非 shell 进程；按真实词汇实现。
  - `complete_ai`：Err "AI is not configured in the native host"（不做空接线）。
  - `sql_dialect`：Standard（本期 sqlite）。
- **worker 臂**：注册表按名找定义 → 引擎 `execute(definition, host, 活会话连接, {})` 在 worker 的 runtime 上真跑（与会话的池同 runtime）→ `StepExecutionResult` 映射回 UI 的 `WorkflowStepResult`（step=SQL 或 step_id，ok=状态，summary=错误句/行数/结果 JSON）。引擎内部处理 condition/foreach/merge/transform/错误策略/超时——宿主不再逐步手写（N13 的 not wired 消息删除）。
- **发现并绕开的语义坑（如实记录）**：条件表达式的左侧在引擎里经 `resolve_expression` 二次解析——`{{...}}` 模板替换后的纯字面量（"3"）会被当变量名解析成空串。正确写法是裸路径（`steps.count.rows_count > 0`）；测试踩中后按引擎语义修正，已在测试注释里写明。

## 测试（GR1 新增 1 例；GR3 新增 2 例）

- UI `a_workflow_run_lands_through_the_job_channel`：JobAdapter 受理 RunWorkflow → loading 标记在 → 两帧投递落定 → 步报告进 W7 列表。
- 宿主 `the_workflow_engine_runs_every_step_kind_over_the_live_session`：query（3 行）+ list_tables（真目录 users/orders/events）+ condition（裸路径真值→then 分支跑了）+ foreach（对 query 的 3 行逐行模板渲染 SQL 真跑）——8 条步骤结果全 ok，foreach 汇总 `iterations_completed:3`。定义直接写进注册表（UI 保存路径只产 Query 步，引擎全词汇测试绕开它——这是已知形状限制，UI 的 YAML 编辑器已可手写全词汇）。
- 宿主 `an_ai_step_and_a_past_deadline_answer_honestly`：AI 步失败且句子点名"AI is not configured"；宿主 `execute_command` 直调带过期 deadline → 如实拒绝。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：138 + 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline`：exit 0。
- GR3 `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1`：46 passed。
- GR4 `cargo check -p datazen-rune-host --offline`：exit 0。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 命令词汇只接了四类读工具（query/list_tables/get_schema/describe_table）；search_tables/explain_query/list_databases 未接（同一 `execute_command` 接缝，按需加）。
- 引擎的 grants/plan guard/资源块语义（MCP 权限模式）经 `None` 传参——单租户宿主无审批面，引擎的 SQL 守卫（`enforce_workflow_query_guards`）仍生效。
- 部件运行历史（widget run history）仍在引擎侧，未接。
- merge/transform 步未单独设测试例（引擎自身有测试矩阵；宿主环已证全词汇经同一 execute_command 路径）。
