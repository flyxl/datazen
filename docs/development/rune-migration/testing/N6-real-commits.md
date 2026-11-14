# TN N6 真实提交脏格 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`104a277 feat(rune): dirty cells commit through the edit use case over real rows (issue-rune N6)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`TablePage.primary_key`、`CellEditBatch`/`RowEdit`、`AdapterJob::CommitCells` 载荷升级）、`crates/datazen-ui/src/data_grid.rs`（网格存 PK、`column_index`/`sort_name`/`commit_batch`）、`crates/datazen-ui/src/lib.rs`（`commit_staged_edits` 作业路径发批；全部 TablePage 字面量补字段）、`crates/datazen-rune-host/src/live.rs`（`SessionColumns`、`RuntimeChunks`、`commit_edits`）、`crates/datazen-rune-host/src/backend.rs`（execute 的 boundParameters 应用、`revision_of`、assemble 返回 profiles）、宿主 crate 清单（table-editing/schema 特性）

## 实现要点

- **UI 词汇升级**：`TablePage` 增 `primary_key`（内存端口恒空，活宿主的读页带上驱动的答案）；网格新增 `commit_batch()`——每编辑行一条 `RowEdit{index, identity(PK→页显值), changes[(列, 原值, 暂存值)]}`，与 React 网格同一模型：底行变更表现为过期原值失败，而非静默覆写。内存端口的回落路径（H5/B6 契约）零改动。
- **宿主提交链**（全走既有用例）：`preview_changes` 在活 schema 上建计划——身份校验、指纹、`grid.cell.noRowIdentity` 拒绝都是用例自己的裁决，宿主只做词汇映射（UI 批 → `PendingRowChange`）；`commit_changes` 在 runtime 上以 owned-session 模式执行（N5 的地基），产物读取器 = runtime 固有 `read_artifact_chunk` 的投影。`ConnectionFacts` 的 revision 来自 HostProfiles（与开会话同源）。
- **boundParameters 修复（测试首跑抓到的真缺陷）**：`BoundSqlStatement.sql` 带占位符、`parameters` 携值，而 provider 的 execute/query 臂收成品 SQL——宿主后端按桌面处理 `params` 的同一方式先 `apply_params` 再下发。没有这一步占位符绑成 NULL、写匹配 0 行（"affectedMismatch" 即其症状）。
- **列来源**：`SessionColumns` 走活会话驱动的 `get_columns`（读路径用过的同一目录），计划建在读页的同一 schema 上。

## 测试

| 测试 | 位置 | 断言 |
|---|---|---|
| `the_commit_batch_addresses_rows_by_the_primary_key_with_originals` | datazen-ui | 批按页显 PK 值寻址行、变更携带页载原值与暂存值；无编辑无批；无 PK 页的批身份为空（宿主将拒，诚实答案） |
| `commit_jobs_write_through_the_use_case_and_read_back` | datazen-rune-host | 读页带驱动 PK；提交批落库 1 行；重读首行是新值、他行未动；未知列是失败句非静默 |
| `a_commit_without_identity_is_refused` | datazen-rune-host | 无身份变更被用例以 `noRowIdentity`/主键句拒绝——用例的门，非宿主重写 |

既有测试零回归（UI 121+55、宿主 14）。

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（121 + 55 passed；新增 1 个测试） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（14 passed；新增 2 个测试） |
| GR4 | `cargo check -p datazen-rune-host --offline --all-targets` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 编辑-提交-读回全链路经真 sqlite 验证，行身份与丢失更新防护由用例保证。遗留：N7 查询执行走 runtime 会话（`execute_in_session` + 代次取消——取消走 `cancel_execution`，Q6 的代次语义已就位）；多行删除/插入（`delete_marked`/`insert_marked`）UI 尚无入口，属前端能力扩展，不在本系列。
