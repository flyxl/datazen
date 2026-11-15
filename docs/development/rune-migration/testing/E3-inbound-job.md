# TE E3 入站引用作业化 + 同步会话退场 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`ef116f8ba feat(rune): the inbound scan travels as a job, the sync session retires (issue-rune E3)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`AdapterJob::InboundRefs`/`AdapterDelivery::InboundRefsRead`）、`crates/datazen-ui/src/lib.rs`（AppState 增 `inbound_generation`/`inbound_loading`；`load_inbound_refs` 改作业投递；投递分发带代次与表名双守卫；断开与新会话清理；1 个新测试 + JobAdapter 补臂）、`crates/datazen-ui/src/data_grid.rs`（入站在途提示行）、`crates/datazen-rune-host/src/live.rs`（`HostJob::InboundRefs`/`HostAnswer::InboundRefs` + worker 分支 + `inbound_references` 异步函数；删除 `SyncSession`/`sync_handle`/`sync_session`/`cached_schema`；`table_ddl`/`table_inbound_references` 改诚实拒绝；`cached()` 缓存读带诚实 miss 句；confirm_import 改 `futures_executor`；run_workflow 注明 UI 不可达回退；N10 旧测试 DDL 段迁移作业路径；入站测试迁移作业路径；2 个新测试）

## 实现要点

- **扫描上作业通道**：`list_inbound_foreign_keys` 用例原样搬到 worker 的活会话上（`LiveNavigationSource` 绑定 worker 会话的驱动与句柄）——T5 的扫描逻辑零改动，只换了执行位置。
- **双守卫投递**：入站答案只在「代次仍当前 且 表名是当前加载表」时写状态——新请求换代次作废旧答案，网格换表后旧表清单不落（列表描述的是加载表）。
- **同步会话退场**：`SyncSession`/`sync_session()`/惰性连接/`cached_schema()` 的取数路径整体删除；schema 缓存改由 worker 独占维护（E1 结构作业与每次页读取暖），适配器侧 `cached()` 只读、miss 给 "the structure of 'x' is not loaded — open the table and the job reads it"。
- **同步路径封死**：`table_ddl` 与 `table_inbound_references` 固定答指向各自作业的句子（"goes through the structure job"/"goes through the inbound job"）；`sync_runtime` 只剩 `run_workflow` 回退（注释写明 UI 不可达——UI 的运行走 `RunWorkflow` 作业）；confirm_import 的两个文件仓调用改 `futures_executor::block_on`（其 future 恒就绪）。
- **零行为回归的演示路径**：拒绝作业的宿主（演示进程）走原同步回退，"the live host lists the references" 的诚实拒绝不变。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `the_inbound_list_arrives_as_a_job_and_only_for_the_loaded_table` | ui lib.rs | 按钮发作业、在途提示渲染；投递落地列表渲染（`fk-inbound-0`）；他表投递不落地；旧代次投递不清新请求的在途标记 |
| `inbound_references_answer_the_scan_over_the_live_session`（迁移） | live.rs | 真会话：users 的入站=orders.user_id→id；events 空清单；同步路径拒绝且句子点名 inbound job |
| `the_sync_structure_reads_serve_the_job_filled_cache_and_refuse_the_rest` | live.rs | 缓存 miss 诚实句（无同步取数）；结构作业后同步列/索引读从缓存答；同步 DDL 拒绝且句子点名 structure job |
| `structure_reads_answer_the_real_catalog`（迁移） | live.rs | 页读暖缓存后同步读照答；DDL 段改经结构作业（含 create table 与表名） |

既有测试零回归（T5 演示路径的同步回退假件未动）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（55 + 145 passed，新增 1） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib` | 通过，exit 0（58 passed，新增 2、迁移 2） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 帧循环上的无界 IO 清零：结构四读（E1）、部件查询（E2）、入站扫描（E3）全部经作业通道带代次投递，宿主侧同步网络路径删除，`sync_runtime` 只剩 UI 不可达的工作流回退。前置完成，E4 可以链接远程驱动。
