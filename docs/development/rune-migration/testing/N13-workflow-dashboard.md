# TN N13 工作流与仪表盘持久化 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`a4644d1 feat(rune): workflows and dashboards persist through the registry and widget use cases (issue-rune N13)`
- 改动面：`datazen-ui-api`（`DesktopAdapter` 新增 W 系列六个方法，默认 `DESKTOP_NOT_IMPLEMENTED`）、`datazen-ui`（六个调用点先问宿主、回落内存端口，两个新测试）、`datazen-rune-host`（新文件 `dashboards.rs`：两个文件仓储 + `WidgetCreationHost` 胶水；`live.rs`：组装与六个实现；清单加 `dashboard` 特性，连带 `workflow`）。

## 实现要点

- **工作流**：`WorkflowRegistry`（既有用例）over `FileWorkflowStore`（`workflows.json`，原子写）。UI 的 steps 即定义的 query steps（一语句一步）；id 由名字 slug 生成（无 slug 字符回退 FNV 哈希，撞名加后缀）；编辑同名替换步骤。
- **仪表盘**：`FileDashboardStore`（`dashboards.json`：dashboards + widgets 两张行表，原子写）实现 `DashboardRepository` 九法；建部件走 `create_widget_from_sql`（既有用例）：部件带一条 `DashboardHidden` 的隐藏工作流（不进用户工作流列表），默认仪表盘行首次建部件时落地。UI 的 u64 部件 id = UUID 前 16 位十六进制，跨重启稳定。
- **执行**：工作流的每一步与部件的查询都走 N7 的执行用例路径（`run_query`：token → execute_at_target → artifact 收集），在适配器的 sync runtime 上驱动——与 N10 同判据（本地 sqlite 有界 IO 类；远程驱动到日需作业通道，如实记）。非 query 步报告"not wired"。
- **回落语义**：读点任意错误回落内存端口；写点仅 `DESKTOP_NOT_IMPLEMENTED` 回落，真实错误上屏（与 N8 同形）。
- 部件需先连库（隐藏工作流要绑连接），空标题/空名在端口拒绝——错误句子与内存端口一致。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `a_workflow_round_trips_through_the_registry_over_the_file` | dashboards.rs | 经 registry 存取往返；新 registry over 同文件读回同一定义（持久化非会话缓存） |
| `a_widget_created_through_the_use_case_survives_a_reopen` | dashboards.rs | 经 `create_widget_from_sql` 建部件；新开两文件后部件行与隐藏工作流（含 SQL）俱在 |
| `widget_ui_ids_are_stable_and_workflow_ids_slug_cleanly` | dashboards.rs | u64 映射稳定；slug 规则（含中文回退哈希、撞名后缀） |
| `a_saved_workflow_round_trips_and_survives_a_restart` | live.rs | **计划验收线**：存→重启（新适配器同数据目录）仍在；编辑替换步骤；空名拒绝 |
| `a_widget_survives_a_restart_and_its_bars_read_the_live_database` | live.rs | **计划验收线**：真 sqlite 会话上建部件→重启仍在；bars 是活库真数据（user-00..03/0..3）；空标题拒绝 |
| `a_workflow_runs_its_steps_through_the_live_session` | live.rs | 两步工作流：count 真跑（ok/1 rows），坏语句失败带句子；未知工作流报错 |
| `a_host_registry_and_dashboard_replace_the_demo_lists` | datazen-ui | 宿主列表替换演示列表；编辑器开宿主步骤；部件图读宿主 bars |
| `the_widget_and_workflow_forms_save_through_the_host` | datazen-ui | 部件表单保存落宿主并清空；工作流保存/运行落宿主，结果上屏 |

既有测试零回归（宿主 27→33、UI 122→124、ui-api 54→55 计入契约测试扩展）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（124 + 55 passed） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（33 passed；新增 6 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 工作流与仪表盘经既有用例 + 两份 JSON 文件持久化，验收线"建一个部件后重启进程仍在"以真 sqlite 会话验证通过；工作流步骤与部件查询经执行用例真跑。遗留如实记：非 query 步（command/ai/condition/foreach/merge/transform）在宿主路径报告 not wired——完整工作流引擎（grants/plan guard/资源块）是另立任务；部件运行历史（`execute_widget_once` 需要 `WidgetExecutionHost`）同属引擎侧；widget_bars 的两列契约（label,value）为原生 UI 的图表面，桌面 React 侧的 chart_config 聚合不在本期。
