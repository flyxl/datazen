# TE E2 部件条作业化 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`e8a233745 feat(rune): the widget bars travel as a job (issue-rune E2)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`AdapterJob::WidgetBars`/`AdapterDelivery::WidgetBarsRead`）、`crates/datazen-ui/src/lib.rs`（AppState 增 `widget_bars_generation`/`widget_bars` 缓存与 `WidgetBarsState`；进仪表盘页/保存部件/手动刷新三处发作业；`bar_chart` 只读缓存 + 在途/失败/未加载提示；页面新增 Refresh 按钮；投递分发带代次；新会话与断开清缓存；2 个新测试 + `DashboardsJobHost` 假件 + JobAdapter 补臂）、`crates/datazen-rune-host/src/live.rs`（`HostJob::WidgetBars`/`HostAnswer::WidgetBars`；worker 分支与 `widget_bars` 异步函数；同步 `widget_bars` 改诚实拒绝；N13 重启测试迁移到作业路径；2 个新测试）

## 实现要点

- **查询上作业通道**：部件的隐藏工作流携带查询，worker 经执行用例在活会话上跑（与编辑器语句同路）——N13 的同步形态在同一 runtime 上做同样两次读，远程驱动使其成为无界 IO，整体搬到作业通道。
- **渲染零查询**：`bar_chart` 只读 `widget_bars` 缓存；在途（"reading the widget's data…"）、失败句子、未加载提示（"refresh the page"）各自如实渲染；`DashboardsJobHost` 的同步 bars 读 panic，测试断言渲染全程不触发。
- **三个触发点**：进仪表盘页（`select`）、保存部件成功后、页面新增的 Refresh 按钮——全部重启整套部件读；拒绝时回退同步读（内存端口演示路径当场填缓存，既有 W 系列测试零改动通过）。
- **同步路径封死**：宿主 `widget_bars` 固定答 "the widget's data goes through the bars job"——查询是无界 IO，不给任何调用方误走帧循环的机会；N13 重启测试的两处同步调用随迁作业路径，并新增拒绝断言。
- **poll 先于渲染的时序**（实测确认）：rune 的 frame 内 poll 在视图构建之前执行——假件若首次 poll 即应答，提示态永远不会渲染；`DashboardsJobHost` 与 `JobAdapter` 同形（第二次 poll 应答），在途状态保有自己的帧。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `entering_the_dashboards_page_sends_a_bars_job_and_the_chart_renders_the_answer` | ui lib.rs | 进页即发一条 bars 作业；在途提示渲染一帧；投递落地后条形渲染（`widget-block-7-mon`） |
| `a_late_bars_delivery_for_a_superseded_generation_is_dropped` | ui lib.rs | 刷新使条目换代次；同批投递旧代次先到不写缓存 |
| `widget_bars_jobs_answer_from_sqlite` | live.rs | 真会话：`SELECT 'x' AS label, 42 AS value` → `[("x", 42)]`；同步路径拒绝且句子点名 bars job |
| `a_widget_whose_query_has_one_column_answers_a_sentence` | live.rs | 单列查询 → "needs two columns" 诚实句 |
| `a_widget_survives_a_restart_and_its_bars_read_the_live_database`（迁移） | live.rs | N13 场景不变：部件跨重启持久、bars 四行 user-00..03，全部经作业路径 |

既有测试零回归（内存回退路径被 W2/W3 与 N13 表单测试隐式覆盖）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（55 + 144 passed，新增 2） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib` | 通过，exit 0（57 passed，新增 2） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 帧循环上不再有部件查询：柱状图只读缓存，三个触发点带代次投递。遗留：入站引用清单仍是帧循环同步读、宿主同步会话路径（`sync_session`/`cached_schema`）仍在——E3 按计划处理。
