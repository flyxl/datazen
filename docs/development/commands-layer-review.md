# commands 层逐文件 review：测试分层与瘦身总账

- 日期：2026-10-31
- 基线：`review/web-ready-2026-10`（WR-D2 第二切片之后）
- 触发指令（用户原话）：
  1. 单元测试不要直接测 command 层，应该测 modules 层；
  2. 什么场景下要直接测 command 层？
  3. desktop 下 commands 层的 rust 代码仍然很多，逐个文件 review 是否必要或者可迁移，尤其是测试文件。

本文是这三个问题的成文答案与执行总账，作为 WR-A2 后续刀（`_impl` 拆除 + 逻辑下沉）
与测试归位的清单权威。进度回写仍走 `web-ready-plan.md` §11。

---

## 1. 测试分层策略（问题 2 的答案）

**默认规则（问题 1 的政策化）**：单元测试住进被测逻辑所在的层。modules 的逻辑在
modules 测，宿主绑定在宿主测。**command 层不承载业务行为的单元测试。**

**判据**：把宿主从 Tauri 换成 axum，这个测试还能原样通过吗？
- 能 → 它测的是形态中立的业务，位置错了，迁去 modules；
- 不能 → 它测的是绑定本身，留在 command 层。

**仅四类测试允许直接测 command 层**（每类都有仓库内既有范本）：

| # | 类别 | 测什么 | 仓库内范本 |
|---|---|---|---|
| C1 | **IPC 线格式钉子** | 参数名（camelCase）、DTO 序列化形状、错误码到信封的映射与消息首位前缀（`grid.*` 分类）、事件名与载荷（`ai:stream-chunk`）——前端分类逻辑依赖的线契约 | `named_views_error_tests.rs`、`ipc_surface_tests.rs`、`ai/ipc_contract_guards.rs`、`fk_navigation/tests.rs`（其模块文档明言「规则住在共享层并在那里测试；这里只钉壳的契约」） |
| C2 | **宿主装配守卫** | bootstrap 注册表（合并命令注册且旧命令消失）、`AppState` 形状上限、组织作用域不得进 IPC 载荷（INV-01/P7） | `app_state_shape_tests.rs`、`org_scope_surface_tests.rs`、`wapps/tests.rs` 的注册守卫、`backup_tests.rs` 的 bootstrap 守卫 |
| C3 | **宿主资源绑定行为** | 对话框、窗口、剪贴板、可执行文件路径、app 数据目录——只有 mock_app/真实宿主能证明的行为 | `config_tests.rs`（executable path）、`dialog.rs`/`window.rs` 相关测试 |
| C4 | **端口适配器接线烟测** | 同一用例经宿主端口装配后端到端走通（每适配器一两根烟测；深度行为覆盖在 modules 侧） | `backup_tests.rs` 的 `backup_and_restore_roundtrip`、`ai/tests.rs` 的 `ai_providers_config_prompt_workflow` |

**推论**：
- 通过 `TestAppState` + 直调命令入口来测**业务规则**（校验、映射、状态机、SQL 生成）
  是反模式——那正是 `_impl` 转发层长期存在的唯一理由（测试需要 `&AppState` 形参）。
  WR-A2 已在 AI 域证明：拆掉 `_impl` 后，逻辑进 modules、测试跟逻辑走，命令入口
  由 C4 烟测覆盖。
- modules 侧测试基建已具备：`datazen_driver_api::mock_driver`（MockDriver/
  MockDriverOptions）、modules dev-deps 已含 postgres/mysql/sqlite 真实驱动与
  serde_json。搬测试不需要新基建。

---

## 2. 数据总览（问题 3 的测量基线）

`apps/desktop/src-tauri/src/commands/` 共 **150 个文件、51,793 行**：

| 构成 | 文件数 | 行数 | 占比 |
|---|---|---|---|
| 生产代码（含内联 `#[cfg(test)]`） | 106 | 32,737 + 5,695 | 63% + 11% |
| 独立测试文件 | 40 | 18,264 | 35% |
| 测试夹具（fixture / test host） | 4 | 792 | 2% |

**测试相关合计约 24,751 行（48%）**。`_impl` 转发函数残留在 25 个文件、共 **115 个**，
集中在 connection(14)/dashboard(10)/query(9)/config(9)/tunnel(7)/wapps(10)/sync(16)/mcp(4)。

按测试文件实际所测逻辑的位置分类（40 个测试文件、18,264 行）：

| 实际所测 | 行数 | 处置 |
|---|---|---|
| modules 逻辑（`datazen::*`，测试却在宿主） | 6,123 | 迁移（本轮已迁 1,355，其余见 §4） |
| 经 `TestAppState` 直调命令/`_impl` 的业务规则 | 4,766 | 随对应 `_impl` 拆除刀下沉 |
| 桌面 crate 内 commands 之外逻辑（多为 modules re-export） | 2,975 | 逐个甄别后迁移 |
| 命令入口行为（AI 域，WR-A2 后的直调入口测试） | 1,998 | 按 C1/C4 甄别拆分 |
| 宿主守卫/装配（C1/C2/C3） | 2,402 | 保留 |

---

## 3. 逐文件裁决表

裁决词汇：
- **保留**——宿主绑定就是该文件的本质，长期留在桌面；
- **守卫**——C1/C2/C3 类测试，永远留宿主；
- **瘦身**——命令壳保留，内嵌形态中立逻辑随 WR-A2 后续刀下沉 modules（列出 `_impl` 数）；
- **测试迁移**——被测逻辑已在 modules，测试文件直接搬家（本轮或下轮）；
- **随刀下沉**——测试与其所钉的 `_impl`/编排逻辑一起下沉；
- **下沉**——生产文件本体是形态中立逻辑，应整体（或主体）迁 modules。

### 3.1 ai/（WR-A2 已完成转发层拆除；本轮完成测试拆分）

| 文件 | 行 | 裁决 | 依据 |
|---|---|---|---|
| ai/mod.rs | 66 | 保留 | 域注册 |
| ai/chat.rs | 476 | 瘦身（后续刀） | 编排本体（schema 管线+取消登记）待下沉，M3 已端口化 |
| ai/config.rs | 121 | 保留 | 配置存取壳，逻辑已在 `ai::api`/`AiConfigStore` |
| ai/generate.rs | 491 | 瘦身（后续刀） | 同 chat，编排待下沉 |
| ai/grant.rs | 272 | 保留 | 会话所有权授予的宿主绑定（M2 语义在 modules） |
| ai/prompts.rs | 54 | 保留 | 提示词模板 IO 壳 |
| ai/util.rs | 284 | 保留 | `emit_stream_chunk_or_error` 等 tauri 事件绑定 |
| ai/command_test_host.rs | 99 | 基建 | mock_app 托管 AppState（WR-A2 产物，C3/C4 用） |
| ai/ipc_contract_guards.rs | 88 | 守卫 | C1 参数提取 |
| ai/tests.rs | 233 | 已拆（本轮） | 纯逻辑 25 测已归位 modules；剩 9 测为 C3/C4 |
| ai/mock_provider_tests.rs | 370 | 随刀下沉 | AI 供应商协议回放，测的是 modules providers，待 harness 判断（wiremock 在宿主） |
| ai/integration_tests.rs | 1,468 | 随刀下沉 | `ai_chat`/`ai_generate_sql` 编排下沉后拆分：编排用例测试进 modules，事件/取消 IPC 钉子留宿主 |

### 3.2 connection / tunnel / 长连接域

| 文件 | 行 | 裁决 | 依据 |
|---|---|---|---|
| connection.rs | 1,281 | 瘦身（14 `_impl`，最大域） | 逻辑已在 `connection::api`（resolve_test_config 等用例已下沉） |
| tunnel.rs | 754 | 瘦身（7 `_impl`，583 行内联测试） | 隧道生命周期壳；probe/summary 逻辑形态中立 |
| tunnel_summary_tests.rs | 289 | 随刀下沉 | 钉 `get_tunnel_summaries_impl` 的脱敏映射——映射本体应下沉 `tunnel` 域 |
| tunnel_probe_tests.rs | 1,109 | 随刀下沉 | probe 语义测试经 fixture |
| tunnel_probe_fixture.rs | 338 | 基建（随刀） | 对应 modules 侧需同款 fixture |

### 3.3 查询与结果域

| 文件 | 行 | 裁决 | 依据 |
|---|---|---|---|
| query.rs | 449 | 瘦身（9 `_impl`） | 执行编排，M4 后读取已用例化 |
| query_tests.rs | 687 | 随刀下沉 | 直调 `execute_driver_command_stream_impl` |
| query_favorites.rs | 236 | 瘦身（5 `_impl`） | 收藏 CRUD 壳 |
| driver_command/mod.rs + types.rs | 144 | 保留 | 请求信封 DTO（C1 线格式） |
| driver_command/execute.rs / resolve.rs / streaming.rs / helpers.rs / discovery.rs | 689 | 瘦身（resolve 内含策略裁决，逻辑在 `mcp::application::command_policy`） | 命令执行壳 |
| driver_command/tests.rs | 1,003 | 测试迁移（线格式部分留为 C1） | wire 格式测试留；`resolve_command_driver`/策略/流式语义测试迁 `modules/src/mcp/application/` |
| result_mapping.rs | 159 | 下沉 | 保守结果映射（SQL 识别 + 驱动确认）纯形态中立 |
| result_origin.rs | 334 | 瘦身（2 `_impl`） | 结果来源标注 |
| result_write.rs | 382 | 瘦身（1 `_impl`） | 窄写路径编排，`commit_query_result_cell_impl` 待下沉 |
| table_target.rs | 273 | 下沉 | 目标执行选择逻辑 |
| value_search.rs | 88 | 保留 | 壳；逻辑已在 `services/value_search`（有测试） |
| sample.rs | 87 | 保留 | 种子数据集 IO 壳 |

### 3.4 data / 表编辑域

| 文件 | 行 | 裁决 | 依据 |
|---|---|---|---|
| data.rs | 215 | 瘦身（2 `_impl`） | 行变更命令壳 |
| data/table_columns.rs | 129 | 保留 | 列元数据读取壳 |
| data/tests.rs | 401 | 随刀下沉 | `preview_pending_changes_impl` 等 |
| data/insert_tests.rs | 651 | 随刀下沉 | 插入编排 |
| data/insert_guard_tests.rs | 528 | 随刀下沉 | W-3/V-2/IB-D2/IB-D4 守卫——规则本体在 `table_editing`，随刀后规则测试进 modules，仅线格式留 C1 |
| data/target_read_tests.rs | 640 | 随刀下沉 | 目标读取 |
| data/result_source_tests.rs | 609 | 随刀下沉 | 钉 `commit_query_result_cell_impl` |
| data/result_write_tests.rs | 231 | 随刀下沉 | 同上 |
| data/test_fixtures.rs | 81 | 基建（随刀） | MockDriverOptions 组合 |

### 3.5 sync / schema_diff / data_transfer（迁移三巨头）

modules 侧 `migration/` 已有同域测试基建（`filter_tests.rs`、`plan_tests.rs`、
`data_transfer/job/tests/` 等）。桌面侧剩余的是 job 编排与宿主桥接：

| 文件 | 行 | 裁决 | 依据 |
|---|---|---|---|
| sync/mod.rs | 443 | 保留（注册）| 域注册 + re-export 收口 |
| sync/apply.rs | 681 | 瘦身（5 `_impl`） | apply/compare 编排 |
| sync/exec.rs | 1,228 | 瘦身（4 `_impl`） | 执行编排 |
| sync/jobs.rs | 884 | 瘦身 | job 生命周期 |
| sync/jobs_contract.rs + jobs_cancel_contract.rs | 1,063 | 守卫 + 随刀拆分 | IPC 参数契约留；job 语义随刀 |
| sync/tasks.rs | 253 | 瘦身（4 `_impl`） | 任务 CRUD 壳 |
| sync/job_api.rs / inspect.rs / compare.rs / keyset_source.rs / artifact_view.rs | 1,001 | 瘦身 | 逐个随刀 |
| sync/host/*（mod/executor/recording/selection/state） | 1,348 | 保留（本切片） | 宿主执行器桥接（HostTargetExecutor 等），WR-F5 服务端需对照实现 |
| sync/tests.rs | 887 | 随刀下沉 | 直调 `*_impl` |
| ~~sync/filter_validation_tests.rs~~ | ~~501~~ | **已迁移（本轮）** | → `modules/src/migration/data_sync/filter_validation_tests.rs` |
| schema_diff.rs + compare/deploy/plan/profile_commands | 846 | 瘦身 | 命令壳 |
| schema_diff/job.rs + job/desktop.rs + job/recovery.rs | 1,590 | 瘦身 | job 编排（`reviewed_repository` 已在 modules 有测试） |
| schema_diff/reviewed_repository.rs | 569 | 下沉核对 | modules 侧已有 `reviewed_repository_tests`，桌面这层应只余桥接 |
| schema_diff/unified_plan.rs | 116 | 瘦身（1 `_impl`） | 统一计划壳 |
| schema_diff/tests.rs + tests/plan_deploy.rs | 501 | 随刀下沉 | plan/deploy 语义 |
| data_transfer/*（exec/inspect/preview/job_api/plan_scope…） | 5,279 | 瘦身（分刀） | 最大编排域；job_api 的 admission/assembly/runtime/scope 形态中立 |
| data_transfer/tests.rs | 1,069 | 随刀下沉 | 编排行为 |
| data_transfer/job_api/tests/*（6 文件） | 2,456 | 甄别：queries/addressability/endpoint_identity 指向 modules 逻辑 → 测试迁移；mod/job_lifecycle/replay 随刀 | 逐个核对被测面 |
| data_transfer/resume_preflight_tests.rs | 478 | 随刀下沉 | preflight 语义 |

### 3.6 其余命令域

| 文件 | 行 | 裁决 | 依据 |
|---|---|---|---|
| mod.rs | 437 | 保留 | 命令注册总表 |
| error.rs | 629 | 保留 | 宿主错误信封 + From 映射（C1 核心；308 行内联测试为线格式钉子） |
| config.rs | 299 | 瘦身（9 `_impl`） | 设置读写壳 |
| config_tests.rs | 177 | 保留（C3）+ 随 `_impl` 拆除适配 | executable path 等宿主行为 |
| context.rs | 766 | 瘦身 | 上下文命令（534 行内联测试随刀） |
| dashboard.rs | 685 | 瘦身（10 `_impl`） | 仪表盘 CRUD 壳 |
| history.rs | 536 | 瘦身（1 `_impl`） | 历史记录壳 |
| named_views.rs | 160 | 瘦身（3 `_impl`） | 命名视图壳 |
| workflow.rs | 197 | 瘦身（2 `_impl`） | 工作流壳 |
| mcp.rs | 381 | 瘦身（4 `_impl`） | M 系列后剩绑定壳 |
| fk_navigation.rs | 123 | 瘦身（2 `_impl`） | 壳；规则在 `services/fk_navigation`（有测试） |
| schema.rs | 127 | 瘦身（2 `_impl`） | 壳 |
| schema/tests.rs + count_tests.rs | 432 | 随刀下沉 | `read_databases` 等读取语义 |
| backup.rs | 362 | 保留 | 对话框/路径/事件绑定；网关在 modules |
| ~~backup_tests.rs（原 322）~~ | 276 | **已拆（本轮）** | 5 个纯逻辑测试 → `modules/src/backup/options_tests.rs`；留 From 守卫/bootstrap 守卫/IPC 参数守卫/往返烟测（C1/C2/C4） |
| restore_gateway.rs / restore_job.rs / restore_job/input.rs | 399 | 保留（桥接核对） | modules `backup::restore_*` 已有对应本体 |
| restore_job/tests.rs | 365 | 测试迁移（甄别） | 被测恢复语义指向 modules `restore_job` |
| restore_job/provider_fixture.rs | 274 | 基建（随刀） | — |
| export.rs + export/stream.rs | 846 | 保留 | 文件导出宿主绑定 |
| export/live_acceptance.rs | 347 | 保留（C4 验收） | 真实驱动验收烟测 |
| file.rs / file_jobs.rs | 524 | 保留 | tauri 文件系绑定 |
| dialog.rs | 528 | 保留 | tauri 对话框 |
| window.rs | 753 | 保留 | tauri 窗口 |
| clipboard.rs / theme.rs / encryption_key.rs | 85 | 保留 | 宿主小绑定 |
| app_archive.rs | 131 | 保留 | app 数据目录 ZIP |
| port_call.rs | 37 | 下沉候选 | 端口调用→命令结果收口，形态中立，与 WR-C 错误词汇对齐时处理 |
| connection_import/ipc.rs | 509 | 保留（拆分甄别） | 文件对话框 + 密码校验；导入解析已在 modules（各格式有测试） |
| connection_import/ipc_tests.rs | 783 | 测试迁移（拆分） | 导入行为测试迁 `modules/src/connection_import/`；文件过滤/IPC 形状留 C1 |
| wapps/mod.rs / driver.rs / sessions.rs / storage.rs / registry.rs | 1,161 | 瘦身（分刀） | 会话/存储编排部分形态中立 |
| wapps/tests.rs | 472 | 守卫（注册部分）+ 随刀拆分 | — |
| wapps/session_tests.rs | 151 | 随刀下沉 | 会话语义 |
| app_archive_tests.rs / app_state_shape_tests.rs / org_scope_surface_tests.rs / ipc_surface_tests.rs / named_views_error_tests.rs / encryption_key_tests.rs | 983 | 守卫（保留） | C1/C2/C3 范本 |

---

## 4. 迁移批次（执行顺序）

- **B0（本轮已执行）**：三块「逻辑已在 modules、测试直接搬家」——
  `filter_validation_tests`（501 行整迁）、`backup_tests` 纯逻辑 5 测、
  `ai/tests.rs` 纯逻辑 25 测（response 18 + tools 4 + api 1 + single_turn 2；
  另删 1 个与 modules 既有覆盖重复的 `provider_defaults` 测试）。
  共 **1,355 行测试代码归位 modules**，桌面 commands 层净减约 1,050 行。
- **B1（下一刀，纯迁移）**：`connection_import/ipc_tests` 拆分、
  `driver_command/tests` 拆分（wire 格式留 C1）、`restore_job/tests` 甄别、
  `data_transfer/job_api/tests/{queries,addressability,endpoint_identity}`。
- **B2（域刀，`_impl` 拆除 + 逻辑下沉 + 测试跟随）**：按域推进，顺序
  connection → tunnel（summary/probe）→ query/driver_command → data（写路径）→
  schema/schema_diff → dashboard/config/context/history/named_views/workflow/
  query_favorites/mcp/fk_navigation → sync → data_transfer → wapps。
  每域刀法与 WR-A2 AI 域相同：`_impl` 删除、逻辑内联进命令函数或下沉 modules
  用例、业务测试进 modules、C1–C4 测试留宿主、IPC 契约零变化。
- **B3（编排巨石下沉）**：`ai/integration_tests` 拆分、`ai_chat`/`ai_generate_sql`
  编排本体下沉（WR-A2 既登记遗留）、`sync/host` 与 `data_transfer` 桥接对照
  WR-F5 服务端实现复核。
- **B4（收口）**：`port_call.rs` 下沉候选与 WR-C 错误词汇对齐；
  `result_mapping`/`table_target` 下沉；`testing/` 基建的 modules 侧对照清点。

## 5. 与既有轨道的衔接

- 本 review 是 **WR-A2 后续刀的清单权威**（进度区原「其余域的 `_impl` 拆除按同刀法
  逐域推进」自此有逐文件账本）；
- B2 各域刀同时服务 **WR-B/M 系列的收尾**（`_impl` 消灭后命令壳与 HTTP handler
  共享同一 modules 用例，即「同一用例、两种薄绑定」的最终形态）；
- 测试分层策略（§1）进入 **WR-C 轨**的契约测试基线：HTTP 面 σ 同样只测
  C1（线格式）+ C4（适配器烟测），业务行为全部在 modules。
