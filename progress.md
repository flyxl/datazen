# progress.md — P5 Wave-1 / data-sync 轨（`feature/p5-data-sync`）

> 进度台账。随分支流转，**验收合并时删除**。分支根目录：本文件。缺陷结论只存在本台账里即视为未完成，故每条结论都能在代码/测试/文档里读到对应事实。

- 轨道基线（track base）：`214c44565`
- 已有提交：`91678192b`、`9e640ac06`（本次**新增**提交，不 amend）
- 工作树：`.worktrees/datazen-p5-data-sync`，`CARGO_TARGET_DIR=/tmp/p5-ds-target`

---

## 1. 自证

### 1.1 已完成

| 项 | 证据 |
| --- | --- |
| CM-43/44/45 crate 层 handler 契约测试（fake-host fixture） | `packages/data-sync/tests/cm43_review_conflict.rs`(3) `cm44_batch_cancel.rs`(4) `cm45_session_mode.rs`(4)，提交 `91678192b` |
| prepare 经 prepare Job（planId + ChangeSet 产物） | `commands/sync/jobs.rs::submit_prepare`、`host/state.rs::store_artifact` |
| apply 经 apply Job（planId + selectionRevision + confirmation） | `commands/sync/jobs.rs::submit_apply`、`exec.rs::execute_data_sync_plan_impl` |
| 关闭窗口只退订，不释放资源（§8） | `host/mod.rs::close_endpoint` 只归还 budget 许可；db 会话归属窗口，`jobs.rs::drive` 终态后 `state::forget` |
| 8 条经**生产命令层**的宿主契约测试 | `src-tauri/src/commands/sync/jobs_contract.rs` |
| 门禁 4 条全绿（见 §2） | EXIT=0,0,0,0 |
| 禁区零改动 | 见 §6 |

### 1.2 进行中

无（实现已停在本文件记录的状态）。

### 1.3 未开始 / 不在本轨范围

- 真实数据库端到端（无 DB 会话，只做到编译期 + 单元/契约层，见 §5.1）。
- `packages/runtime/**` 的 stage 内取消检查（禁区，见 §4.1 blocker）。
- ChangeSet 产物落盘持久化（当前驻留内存，见 §5.2）。

---

## 2. 门禁实测（最终一轮，逐字结论行）

运行时环境：`CARGO_TARGET_DIR=/tmp/p5-ds-target`；长输出落系统 temp 日志，退出码单独打印。

```
PRE_HEAD=9e640ac0656de8306fb4bcb3689cddf52603af31
PRE_PATH_SHA=69ace8700d83aea9db989829358e8cbed786e964ed30fab1d7b9883788ec5305
PRE_CONTENT_SHA=ead461afe4323218ae0a7a80964f5429ccd0fe4e90470f4c5ec505a18971a382
```

| # | 命令 | EXIT | 逐字结论 |
| --- | --- | --- | --- |
| 1 | `cargo test -p datazen-data-sync` | **0** | `test result: ok. 179 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s`（lib）<br>`test result: ok. 3 passed; …`（`tests/cm43_review_conflict.rs`）<br>`test result: ok. 4 passed; …`（`tests/cm44_batch_cancel.rs`）<br>`test result: ok. 4 passed; …`（`tests/cm45_session_mode.rs`）<br>`test result: ok. 1 passed; …`（`tests/cm46_artifact_meta.rs`）<br>`test result: ok. 9 passed; …`（`tests/handler_plan_guard.rs`）<br>`test result: ok. 0 passed; …`（Doc-tests）⇒ 合计 200 通过 / 0 失败 |
| 2 | `cargo test -p datazen-runtime` | **0** | 32 条 `test result: ok.`，passed 求和 **834**，0 条非 ok |
| 3 | `cargo test -p datazen --lib` | **0** | `test result: ok. 1718 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 12.24s` |
| 4 | `pnpm --config.verify-deps-before-run=false typecheck` | **0** | `error TS` 计数 = 0；末行 `$ tsc -p tsconfig.pack-ep.json --noEmit` |

```
GATES_EXIT=0,0,0,0
POST_HEAD=9e640ac0656de8306fb4bcb3689cddf52603af31
POST_PATH_SHA=69ace8700d83aea9db989829358e8cbed786e964ed30fab1d7b9883788ec5305
POST_CONTENT_SHA=ead461afe4323218ae0a7a80964f5429ccd0fe4e90470f4c5ec505a18971a382
```

- `PATH_SHA` = `git status --porcelain` 的 sha（脏文件清单）。
- `CONTENT_SHA` = `git diff HEAD` 文本 + 每个未跟踪文件字节 sha 的汇总 sha（内容级）。
- PRE == POST ⇒ 门禁运行期间无人改动本工作树；四个门禁的数字与本台账、以及随后创建的提交内容同源。
- 本文件是在上述门禁之后写入的唯一新增内容（不含代码），不参与门禁。

日志：`$TMPDIR/dz-final-ds.1bz6zyd6OT.log`、`dz-final-rt.rW0hx7dn1a.log`、`dz-final-lib.GNl8K6fmph.log`、`dz-final-tsc.RM7kWvGCYq.log`。

### 2.1 宿主测试计数归因（消除“少测试”疑点）

在 detached 验证工作树（`git worktree add --detach /tmp/dz-base-wt 214c44565`，只补 gitignored codegen）实测基线：

```
test result: ok. 1710 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 12.57s
```

本轨交付态 = 1718 通过 + 6 ignored ⇒ **+8**，恰好等于 `jobs_contract.rs` 的 8 条新契约测试，**零测试丢失**。

---

## 3. CM-43/44/45 矩阵

| CM | 含义 | crate 层（`packages/data-sync/tests`） | 宿主层（`src-tauri/src/commands/sync/jobs_contract.rs`，经生产命令层） |
| --- | --- | --- | --- |
| **CM-43** | review 之后的行/结构冲突 | `cm43_review_conflict`：`structure_drift_after_review_is_rejected_before_any_write`、`review_conflict_rejects_affected_batch_and_keeps_committed_ones`、`recompare_after_conflict_can_continue` | `apply_job_commits_the_confirmed_selection_once` 钉住「提交前按主键回读复验」这一缺陷现场（`host/mod.rs::select_by_key_sql`）；`the_budget_refuses_a_self_overlapping_pair_before_the_compare_reads_a_row`、`compare_refuses_a_non_database_target_before_any_job_is_submitted` 是进入 Job 之前的门闸分支 |
| **CM-44** | 批次取消保留已提交范围 | `cm44_batch_cancel`：`cancel_preserves_committed_batch_and_rolls_back_the_blocked_one`、`cancel_before_first_commit_rolls_back_everything`、`cancelled_token_stops_before_any_batch_is_executed`、`commit_outcome_unknown_is_reported_as_unknown_not_cancelled` | `cancel_job_reaches_the_flag_the_running_stage_polls`：`cancel_job` 置位生产分页源/执行器在 stage 内轮询的同一个 `Arc<AtomicBool>`（`host/executor.rs:40,103,124`；flag 由 `jobs.rs::open_job` 从 `state::cancel_flag` 取得） |
| **CM-45** | session mode 与 Lease 清理一致 | `cm45_session_mode`：`prepare_and_apply_bind_one_lease_each_and_return_every_session`、`permission_denial_closes_both_sessions_and_leaves_no_pending_write`、`target_open_failure_returns_the_source_session_without_executing`、`failed_close_destroys_the_resource_and_the_next_job_gets_a_fresh_lease` | `apply_job_commits_the_confirmed_selection_once` 断言 `open_transaction_count()==0`（批次租约在 Job 报 Succeeded 前全部释放）；`prepare_job_freezes_a_change_set_and_closes_both_snapshots` 断言两侧快照都已关闭 |

8 条宿主契约测试清单（全部走 `compare_data_sync_impl` / `submit_prepare` / `submit_apply` / `execute_data_sync_plan_impl` / `cancel_job`）：

1. `prepare_job_freezes_a_change_set_and_closes_both_snapshots`
2. `apply_job_commits_the_confirmed_selection_once`
3. `compare_refuses_a_non_database_target_before_any_job_is_submitted`
4. `an_empty_database_scope_resolves_to_the_connection_database`
5. `the_budget_refuses_a_self_overlapping_pair_before_the_compare_reads_a_row`
6. `a_failed_apply_still_consumes_the_plan_id_for_the_legacy_path`（CM-41）
7. `two_concurrent_applies_of_one_plan_id_commit_once`（§2.1 幂等 + 真并发）
8. `cancel_job_reaches_the_flag_the_running_stage_polls`（CM-44）

CM-41 结论（写在测试里，不只在本台账）：`repo.accept` 在受理时写 `consumed_plans[plan_id]=job_id`，因此**失败的 apply 同样消费 planId**；测试 6 先让 apply 因指纹漂移失败，再修好结构、走 legacy 路径，断言被 `PlanAlreadyConsumed` 拒绝（"already submitted"）。

---

## 4. 本轮发现并修复的缺陷

### 4.1 `select_by_key_sql` 漏掉主键绑定谓词（已修）

- 现象：apply 报 `TargetConflictRows: target row at [Integer(2)] changed since review` —— 复核根本没读目标行。
- 原因：`host/mod.rs::select_by_key_sql` 生成 `WHERE pk IS NOT NULL ORDER BY pk` 而**不带键值**，于是回读命中任意一行。
- 修复：按方言生成 1-based 占位符谓词（postgres `${i}[::cast]`，MySQL `?`），`pk = $1 AND …` + `ORDER BY` 引用主键；无主键时保留 `SELECT *`。
- 配套：`packages/driver-api/src/mock_driver.rs` 新增 **opt-in** 能力 `filter_rows_by_key_equality`（默认 `false`，不影响其它测试），`query_with_params` 在启用时按参数做等值过滤；自由函数 `value_matches_param` 承担比较（`Value` 无 `PartialEq`，故不做 `assert_eq!`）。

### 4.2 apply 失败原因被吞（已修）

`handler/apply.rs` 早期版本 `let _ = err;`，失败集合恒为空，IPC 只剩通用文案。现改为 `host.record_stage_failure("apply", err.to_string())`（`host/recording.rs:140`），失败码与原因随 Job 终态上报；`prepare.rs` 同样统一走 `stage_failed`（L91/L106/L139）。诊断期遗留的 `eprintln!` 已清零（`grep -rn "eprintln!" packages/data-sync src-tauri/src/commands/sync` = 0）。

### 4.3 `comparison_store` 子进程“ready 文件半写”竞态（已修，门禁实测偶发失败）

- 现象：本轮一次 `cargo test -p datazen --lib` 得 `test result: FAILED. 1717 passed; 1 failed; 6 ignored`，失败者 `commands::sync::comparison_store::tests::test_tester_abrupt_child_exit_releases_sqlite_owner_lease` 崩在 `tests.rs:829 called Option::unwrap() on a None value`。
- 归因：`tests.rs:816` 的父进程轮询 `while !ready.exists()`，子进程 `fs::write` 先 `O_CREAT` 建文件、再写内容 ⇒ 父进程存在**读到 0 字节**的窗口，`PathBuf::from("")` 的 `file_name()` 为 `None` ⇒ `unwrap()` 崩。机器负载高时窗口变大。
- 该文件与本轨基线 diff 为 0 行（`git diff 214c44565 -- src-tauri/src/commands/sync/comparison_store/ | wc -l` = 0），**不是本轨引入**。
- 修复（测试侧、单点、跨两个用例同时受益）：子进程先写 `child-store-path.staging` 再 `fs::rename` 原子发布；`rename` 之后文件必然内容完整。
- 证据：`cargo test -p datazen --lib owner_lease` 三路并发 × 10 轮 = **30/30 全绿**（`4 passed; 0 failed` ×30）。之后最终门禁整轮 1718/0。

### 4.4 dead code 清理（本轨自己引入的 4 个未用函数 + 1 个未读字段）

GATE3 全树 31 条 warning 中，归属本轨的恰好 5 处（4 个未用函数 + 1 处 `unused_mut`），已全部处理：删除 `host/selection.rs::load_artifact`、`host/selection.rs::store_selection`、`host/state.rs::selection_of`、`plans.rs::issue_plan_with_store`（均为本次改写后遗留的孤儿包装）；`SyncJobOutcome.job_id` 改为在 `jobs.rs::drive` 终态前 `tracing::info!` 输出。最终门禁 `cargo test -p datazen --lib` 中 `commands/sync/(host|jobs|jobs_contract|artifact_view)` 命中 **0** 条 warning（整棵树 31 条 warning 全部为基线既有，见 §5.4）。另修掉 `host/mod.rs:406` 一处 `unused_mut`。

---

## 5. 遗留、偏离与待裁定项

### 5.1 无真实数据库（验证边界）

本轨全部验证是编译期 + 单元 + 宿主契约（mock driver）。没有可用的真实 postgres/mysql 实例，因此**方言级行为（占位符渲染、批量原子性、真实事务隔离级别）未被真机验证**；`supports_batch_atomicity`、事务包裹等结论来自驱动契约与 mock 行为，属间接证据。E2E/WDIO 需由具备 DB 的验证方补做。

### 5.2 ChangeSet 产物驻留内存，未落盘

`host/state.rs` 以 `artifact_ids` 索引内存中的 `ChangeSetArtifact`，进程重启即失效。当前设计里重启后不恢复旧快照继续 apply（与 §8/CM-47「重启后不恢复旧快照」一致），但产物**丢失**而非**标记为失效**，与文档预期仍有差距。待后续轨处理产物持久化或显式失效态。

### 5.3 `backendScope` / 空数据库作用域的解释

`super::types::resolve_db_name(selected, config_default) -> String` 永不返回错误 ⇒ 生产路径没有「缺数据库作用域」的拒绝分支。契约测试 4 `an_empty_database_scope_resolves_to_the_connection_database` 把这一现状**钉成断言**（回落连接默认库），而不是假装有拒绝分支。若设计要求「缺作用域必须拒绝」，那是 `resolve_db_name` 的职责，需要单独裁定，不在本轨擅改。

### 5.4 基线既有 warning（不动）

`RawPageSource`/`validate_page`、`StreamingComparisonStoreWriter`/`StreamingTable`/`read_all_rows`、`ComparisonStore::load`、`COMPARISON_FULL_LOAD_LIMIT`、未使用的 `PermissionsExt`、`bytes` 未读字段，以及 `compare.rs:238`、data-transfer `scan.rs:85`/`sql_file.rs:287,693,770`/`writer.rs:6`、`ai/util.rs:441`、`data_transfer/plans.rs:479,487`、`wapps/tests.rs:3`、`ipc_surface_tests.rs:74,77`、`platform_vault.rs:89,318`。均与本轨无关，未清理以免跨轨噪声。

### 5.5 格式化纪律（有意不跑 `cargo fmt`）

仓库基线**不是 rustfmt-clean**，`cargo fmt -p <crate>` 会顺手重排无关历史代码。本轨因此：

- 只对**自己新增/改写的文件**做格式核对：`rustfmt --edition 2021 --check` 逐文件通过（`host/mod.rs`、`host/selection.rs`、`host/state.rs`、`plans.rs`、`jobs.rs`、`jobs_contract.rs`、`artifact_view.rs`、`comparison_store/tests.rs` 均无 diff 输出）。
- `packages/driver-api/src/mock_driver.rs` **不**跑 rustfmt（该文件第 7–22 行 import 顺序存在基线既有 hunk，跑一次就会引入无关 diff），新增代码按现有风格手写对齐。
- 未执行 `cargo fmt`（任何形式）。

---

## 6. 禁区核对

```
forbidden_tracked_diff_lines   = 0    # git diff --numstat 214c44565..HEAD -- <禁区>
forbidden_worktree_diff_lines  = 0    # git diff --numstat HEAD -- <禁区>
forbidden_untracked            = 0    # git ls-files -o --exclude-standard -- <禁区>
```

禁区 = `packages/{runtime,application,platform-api,backend-client}/**`、`packages/{schema-diff,data-transfer}/**`、`src/**`、`packages/drivers/**`、`src-tauri/src/commands/{schema_diff,data_transfer}/**`。

---

## 7. Blocker：stage 内取消依赖宿主侧临时桥接（`p5-runtime-cancel-watcher` 轨）

- 事实：本工作树的 `packages/runtime` 只在 **stage 之间** 检查 `CancelToken::is_cancelled`（`runtime.rs:108`），**没有** `CancelToken::flag()`，因此“批次执行途中被取消”无法由内核观察。
- 本轨做法（对齐 data-transfer 语义，不用第二个 `spawn_cancel_watch` 轮询器）：`jobs.rs::open_job` 取 `state::cancel_flag(job_id, ctx)` 的 `Arc<AtomicBool>` 交给 `HostDataSync`，生产分页源与目标执行器在**批次循环内**轮询它；取消后回滚在途批次并 `break`，`Cancelled` 终态映射为 `PartiallyApplied`/`RolledBack`，源端查询随即停止；`cancel_job` 一次调用同时置位 host flag 与内核 token。
- **临时性**：这条线是宿主侧实现，待 `p5-runtime-cancel-watcher` 合并后，应改为直接使用内核的 stage 内取消能力，届时删除 `host/state.rs::cancel_flag` 的 flag 桥接（`cancel_job` 保留对 token 的取消）。两轨**不得**同时改 `commands/sync/**` 或 `packages/data-sync/**`。
- 未决：内核 watcher 的接口形态尚未在本工作树出现，故本轨**没有**写任何依赖 `CancelToken::flag()` 的代码（避免编译期耦合未合并 API）。

---

## 8. 本次提交的文件清单

修改：
`Cargo.lock`、`packages/data-sync/src/job/{artifact.rs,handler.rs,host.rs}`、`packages/data-sync/src/job/handler/{apply.rs,prepare.rs}`、`packages/data-sync/tests/support/host.rs`、`packages/driver-api/src/mock_driver.rs`、`src-tauri/Cargo.toml`、`src-tauri/src/commands/sync/{apply.rs,comparison_store/tests.rs,exec.rs,filter_validation.rs,jobs.rs,mod.rs,plans.rs}`

新增：
`src-tauri/src/commands/sync/artifact_view.rs`、`src-tauri/src/commands/sync/host/{mod.rs,executor.rs,recording.rs,selection.rs,state.rs,statements.rs}`、`src-tauri/src/commands/sync/jobs_contract.rs`、`progress.md`（本文件，合并时删除）

单文件规模：**新增文件全部 ≤ 800 行**（最大 `host/mod.rs` 780 行、`jobs_contract.rs` 524 行）。基线既有超限文件的变化：`apply.rs` 1103→966（−137）、`exec.rs` 1019→972（−47）、`plans.rs` 1884→1889（+5，仅新增一条 §2.1 注释）、`mock_driver.rs` 861→902（+41，新增 opt-in 过滤能力与比较函数）、`comparison_store/tests.rs` 851→852（+1，即 §4.3 的原子发布）。未在本轨继续拆分基线既有超限文件，避免跨轨噪声。

状态：**READY_FOR_TEST**