# TN N7 真实查询执行 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`0b24e12 feat(rune): the editor's statements run through the runtime (issue-rune N7)`
- 改动面：`crates/datazen-rune-host/src/live.rs`（`HostJob::RunQuery`、`run_query`、投递映射、测试）。**UI 侧零改动**——N1 已把 RunQuery 作业与投递臂接好，内存回落路径不变。

## 实现要点

- **查询走 runtime**（N5 地基）：`execute_at_target` owned-session 模式——令牌签发、目标 CAS（对 HostProfiles 的 revision，与开会话同源）、执行、轮询到终态、产物读回。列名来自 statementMetadata 块、行来自行块（provider 的 `ResultChunk` 形状）、首语句即网格页。坏语句 = 失败句；无会话 = "the live session is not open"。
- **取消语义**：延续 UI 的代次（Q5/Q6）——过期投递到达即弃；宿主侧无取消作业（UI 未发），失控查询由 runtime 的空闲策略回收。如实记录，未做假取消。
- **写语句同路**：UPDATE 经同一命令通道执行（rows_affected 随块上报），读回验证真落库。

## 测试（全在 datazen-rune-host）

| 测试 | 位置 | 断言 |
|---|---|---|
| `query_jobs_run_real_sql_and_answer_real_rows` | live.rs | 真 SELECT：列/行是驱动答案（WHERE id<3 恰 3 行、序正确、只读标记）；UPDATE 落库（读回 "touched"）；坏语句失败句非空页 |
| `a_query_without_a_session_answers_a_sentence` | live.rs | 无会话时查询作业答含 "session" 的句子 |

既有 14 个测试零回归；GR1 121+55 零回归（UI 未改）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（121 + 55 passed，本任务无 UI 改动） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（16 passed；新增 2 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 查询标签页的编辑器语句在活宿主上真跑真答。**真数据阶段（N3–N7）至此完成**：连接→目录→读页→提交→查询全链路经真 sqlite 验证。遗留：N8 起进入持久化阶段（设置/历史经文件仓储）；多语句批（首语句之外的语句结果）网格暂只显示首语句，与桌面一致的后续扩展另立任务。
