# TN N1 作业接口 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`25ef637 feat(rune): the job interface — real IO without blocking the frame loop (issue-rune N1)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`AdapterJob`/`AdapterDelivery` 枚举、trait 的 `begin_job`/`poll_jobs` 默认实现）、`crates/datazen-ui/src/lib.rs`（`read_generation` 与三个载入标记、`show_table`/`commit_staged_edits`/`run_query` 的作业优先路径、`deliver_host_job`、`app_poll` 投递、断开与取消的清理）、`crates/datazen-ui/src/data_grid.rs`（页脚的载入标记，签名 `footer(grid,…)` → `footer(state,…)`）

## 实现要点

- **作业类型**（ui-api）：`AdapterJob::{ReadTable, CommitCells, RunQuery}` 是三个同步用例的作业孪生，各自携带请求代次；`AdapterDelivery::{TablePage, CommitCount, QueryPage}` 是完成的答案。trait 默认 `begin_job → false`、`poll_jobs → vec![]`——既有适配器（UnimplementedAdapter、UseCaseAdapter）零改动，H3–H5 的同步契约原样成立。
- **UI 路径**（ui）：读表、提交、执行先问作业。受理后：读保持上一页、页脚显 `Loading…`；提交保持脏标记、页脚显 `Saving…`；查询在结果区上方显 `Running…`。`app_poll` 无论当前在哪个 section 都先排空 `poll_jobs`，投递按代次过滤（不变量 3），过期代次只清自己的标记、不写状态。
- **收尾语义**：取消清查询标记（迟到投递只剩代次检查被丢弃）；断开提升读代次并清全部标记，会话外的投递一律作废。提交成功后的重载走同一条读路径——在受理型宿主上它也是一个作业（测试里观察到 `table_loading` 接棒）。
- **页脚标记不挤布局**：标记放在页脚固定高度行内（`commit-loading`/`table-loading`），网格布局在作业期间不位移。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `every_adapter_declines_jobs_and_polls_nothing_by_default` | datazen-ui-api | 未接线与用例适配器都拒收作业、投递为空——同步路径不受影响 |
| `an_accepted_read_job_keeps_the_previous_page_until_the_delivery_lands` | datazen-ui（runtime） | 翻页被受理后网格保持旧页、页脚有 `table-loading`；两帧后投递落地，换上新页、标记消失 |
| `a_stale_read_delivery_is_dropped` | datazen-ui（runtime） | 连续两次读，两份答案同时到达（旧的在前）：只落当前代次的页，被顶替的代次不覆写 |
| `an_accepted_query_job_lands_in_the_result_grid_when_it_delivers` | datazen-ui（runtime） | 执行被受理后无结果、结果区上方有 `query-loading`；投递后结果网格出现作业的行 |
| `an_accepted_commit_job_keeps_the_dirty_marks_until_the_delivery` | datazen-ui（runtime） | 提交被受理后脏标记在、页脚有 `commit-loading`；投递后标记清、重载作业接棒，最终脏数为 0 |
| `a_failed_commit_delivery_keeps_the_dirty_marks` | datazen-ui（runtime） | 失败投递清标记但脏标记与暂存值原样保留、不触发重载 |

测试宿主 `JobAdapter` 在文件内定义：受理每个作业、隔一次 poll 应答，罐头页携带请求代次，使两次读的答案可区分。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 118 passed，datazen-ui-api 55 passed；新增 6 个测试） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0（仅存量警告） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 作业接口就位：真实 IO 的宿主可以不阻塞帧循环地接进 UI，既有同步路径与 H3–H5 契约零回归。遗留：N2 起宿主 crate 建起来后，`JobAdapter` 的真身（工作线程 + 通道 + tokio）落在宿主侧；查询失败的错误展示沿用同步路径的静默语义，待接线真驱动时一并处理。
