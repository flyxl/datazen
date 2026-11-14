# TN N4 真实读表页 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`a1cdadd feat(rune): real table pages through the existing read use case (issue-rune N4)`
- 改动面：`crates/datazen-rune-host/src/live.rs`（`HostJob::ReadTable`、`SessionRunner`、`read_table`、`host_request_context`、`cell_to_text`、LiveSession 增 `connection`）、`crates/datazen-rune-host/Cargo.toml`（datazen 增 `table-browsing` 特性）。**UI 侧零改动**——读作业通道（N1）与投递臂（N1 的 `AdapterDelivery::TablePage`）原样复用。

## 实现要点

- **用例直连**：读作业在工作线程的 tokio runtime 内调 `datazen::table_browsing::api::read_table_page(driver, request, revision, context, runner)`。分页窗口、排序、Contains 过滤、精确计数全部由用例组装的真 SQL 生效；宿主只做词汇映射（UI 的单排序/单过滤 → 用例的 `SortCondition`/`FilterCondition::Contains`）与结果映射（驱动格值 → 网格文本；缺计数回退到页末下界，不归零）。
- **SessionRunner（端口的单会话形态）**：`TargetCommandRunner` 的本宿主实现——命令在宿主持有的句柄上经驱动自己的 `execute_command` 内联执行，输出包成读算法消费的那一个 `commandResult` 块（查询的 `data.results`、schema 的 `data.schema`），与桌面/服务端 runner 从 runtime 门面发布的形状一致。没有提交令牌、没有轮询、没有第二套 runtime——这正是"消费者定义端口"留给单会话宿主的形态。锁不跨 await：进 runner 前克隆驱动 Arc 与句柄。
- **请求上下文**：单租户 `desktop` 组织 + 宿主合成主体（`datazen-native`）——本地单用户宿主唯一能填的授权词汇。
- **revision**：宿主自己开的会话自己持有，无第二写者竞争版本，全读一个稳定值。

## 测试（全在 datazen-rune-host）

| 测试 | 位置 | 断言 |
|---|---|---|
| `read_jobs_serve_real_pages_sorts_and_filters_from_sqlite` | live.rs | 60 行夹具：第 2 页 25 行是真数据（user-25..49）、总数精确 60；`id` 降序由 SQL 生效（第 2 页变 user-34..10）；`name contains 'user-5'` 过滤后恰 10 行（user-50..59；夹具名 `user-05` 不含 `user-5`）、计数跟随过滤；未知表是失败句不是空页 |
| `a_read_without_a_session_answers_a_sentence` | live.rs | 无会话时读作业答 "the live session is not open" |

既有 8 个测试（N2/N3）零回归；GR1 120+55 零回归（UI 未改）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（120 + 55 passed，本任务无 UI 改动） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（10 passed；新增 2 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 打开真连接后网格显示真数据，翻页/排序/过滤/计数由真 SQL 服务，UI 代码一行未动。遗留：N5 提交脏格走 `table_editing` 用例（行号口径：UI 页内行号 → 存储行，需经打开页窗口换算，H5 的契约在作业版延续）；过滤的 `Contains` 之外的算子（等于/大于等）UI 尚无入口，属前端词汇扩展，不在本系列。
