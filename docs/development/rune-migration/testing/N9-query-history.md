# TN N9 查询历史 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`f567955 feat(rune): executed statements land in the query history (issue-rune N9)`
- 改动面：新 `crates/datazen-rune-host/src/history.rs`（FileQueryHistory/FileMigrationRuns + 测试）、`crates/datazen-rune-host/src/live.rs`（LiveAdapter 持 HistoryService、worker 的 RunQuery 分支执行后记录、测试的 opened_adapter 改 with_data_dir）、`crates/datazen-rune-host/src/lib.rs` 与清单（history 特性）。**UI 侧零改动**（Rune UI 尚无历史面板——前端扩展另立任务）。

## 实现要点

- **记录时机**：worker 的 RunQuery 分支在语句执行完成后（无论成败）经 `HistoryService::add_query_history` 落一条——连接、库、SQL、起止耗时、成功标记、失败句。历史写失败不夺走查询答案（用例只记日志，桌面同款纪律）。
- **查询历史文件**（`query-history.json`）：JSON 数组、原子写、**保留策略是仓储契约交给存储侧的职责**——add 时保最新 1000 条。窄读取（recent，连接/库/schema 过滤）、分页读取（三种排序：最近/最旧/最慢）、delete（幂等 Ok(0)）、purge（Query/All 域、按天数）全部实现。
- **迁移运行文件**（`migration-runs.json`）：N12 的域提前备好——save（插或更）/get/list（operation/status/profile/连接过滤，连接匹配源或目标任一侧）。
- **测试修正**（顺手）：`opened_adapter` 夹具原用默认数据目录（写到用户家目录），改 `with_data_dir` 指测试临时目录——历史/设置类断言才能看见文件。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `add_then_recent_reads_the_same_entry_back` | history.rs | add→新仓储重开→recent 读回同一条（连接/库过滤生效，他连接为空） |
| `retention_keeps_the_newest_entries` | history.rs | 1010 条 add 后磁盘恰 1000 条，最旧的 10 条被弃、最新的保留 |
| `migration_runs_save_get_and_list` | history.rs | run 存取往返；连接过滤匹配目标侧、无关连接为空 |
| `executed_statements_land_in_the_history_file` | live.rs（端到端） | 一条成功+一条失败的查询作业后，历史文件两条、连接归属正确、成败标记与失败句在、耗时已测 |

既有测试零回归（宿主 23、UI 122+55）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（122 + 55 passed，本任务无 UI 改动） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（23 passed；新增 4 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 执行过的 SQL 进历史且跨进程可见（计划验收标准"add 后 recent 读回同一条"达成并超出——端到端经真查询作业）。遗留：N10 结构页接真目录；历史面板的前端展示（列表/过滤/重跑）属 UI 扩展任务，不在本系列。
