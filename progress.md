# P5 Wave-1 · Data Transfer（Job 化迁移）进度台账

> 开发期台账，交付即销毁。本文件不是交付物。

- 轨道分支：`feature/p5-data-transfer`
- 基线 HEAD：`214c44565154639595bda1997a300888625e6aac`
- 提交：`08493c333`（包实现 + CM 测试）、`422341c4b`（commands 改写 + 单测）、`20c9ff122`（台账）、`760af1cf0`（round-1 缺陷修复 D1–D6/D8）、本文件（台账）
- 目标文档：`docs/architecture/platform/data-migration-jobs.md` §2/§6/§7/§8/§10、`docs/architecture/backend/data-transfer.md`、`docs/architecture/platform/persistence-model.md` §3.6

## 1. 自证

### 已完成

| # | 任务 | 落点 |
| --- | --- | --- |
| 1 | `DataTransferHandler`（prepare/apply）+ 8MiB 有界管道 `PipelineBudget`（§6.2） | `packages/data-transfer/src/job/{handler,pipeline,plan,checkpoint,recovery,sqlfile,mod}.rs` |
| 2 | CM-46 / CM-47 / CM-48 / CM-49 测试 | `packages/data-transfer/src/job/tests/`、`packages/data-transfer/src/resume/tests.rs` |
| 3 | commands 改写：`prepare_data_transfer_job` → Job，`apply_data_transfer_job` → Job（planId + confirmation）；§8 跨 backend 显式拒绝；取消走 Job 仓库 | `src-tauri/src/commands/data_transfer/job_api/**`、`bootstrap/run.rs`、`plans.rs`、`exec.rs` |
| 4 | round-1 评审缺陷 D1–D6 / D8 修复（见 §3） | 同上 + `packages/data-transfer/src/job/**` |
| 5 | 四道门禁全绿（见 §4） | — |
| 6 | 本台账 | `progress.md` |

### 未开始 / 不在本轨范围

- 前端 IPC 调用方迁移：`src/commands/**` 属禁区（前端封装不在本轨可写范围），见 §6 遗留。
- §10 联调（真库 E2E）与 Pro 打包验证。

### 关键事实（可从代码与测试直接读出）

- prepare 载荷 **不含** `consumedPlanId`，apply 载荷 **只带** planId/planDigest/selectionRevision/selection/confirmedDestructive，端点与凭据不进入 apply 载荷：`job_api/{mod.rs:prepare_payload,apply_payload}` + `job_api/tests/mod.rs::apply_payload_never_carries_endpoints_or_credentials`。
- 载荷不是照抄运行时规则，而是喂给真实投影器断言：`job_api/tests/` 直接调用 `datazen_runtime::job::project_frozen_plan`。
- §8 为 fail-closed：必须显式声明 `backendScope`、只接受本地 backend 作用域、端点引用必须是内存会话 token 形态（≤200 字符、无空白、不含 `/\@?#:`），否则 `CommandError::Validation` 拒绝并提示重新 prepare：`job_api/scope.rs` + `job_api/tests/mod.rs::same_backend_scope_*`。
- §6.1 SQL 文件 Job 没有目标会话，同 backend 判定只在确有目标端点时才解析 database target（`job_api/scope.rs:99-102` 先判 `sql_file_target.is_none()`），因此 SQL 文件迁移不会被同 backend 门闸误拒。
- 一个 planId 至多服务一个 apply Job；apply 在跑之前先 `claim` 已存储的 plan（一次性消耗），重放只会得到「一次性事实」而非时序相关理由：`job_api/admission.rs`、`plans.rs::claim` + `job_api/tests/mod.rs::admission_reports_one_shot_consumption_before_expiry`、`job_api/tests/job_lifecycle.rs::an_apply_job_claims_the_plan_and_refuses_the_legacy_manager`、`two_concurrent_apply_jobs_share_exactly_one_plan_claim`。
- plan 的 revision 与 expiry 是存储事实，不是重算结果：apply 前 digest 不符、review 过期、选择版本漂移一律要求重新 prepare；SQL 文件产物按内容寻址（CM-49）：`plans.rs`、`job_api/mod.rs::plan_digest/file_artifact_id`。
- 两端共用同一个 `service_key`，自覆盖因此是硬拒绝而非按连接 id 判断：`job_api/runtime.rs::endpoint_refs` + `job_api/tests/mod.rs::both_endpoints_share_one_service_key_and_sql_file_has_no_writer`。
- 取消以 Job 仓库为准：P5 运行时只读 `view.cancel_requested`，`services::job_registry` 从没见过这些 Job id，所以 `cancel_data_transfer` 先问仓库、只对陌生 id 才回落旧注册表；已终态的 Job 返回「已处理」而不是报错（§10.1.1 终态即裁决）：`job_api/cancel.rs` + `job_api/tests/job_lifecycle.rs::a_cancel_request_lands_on_the_job_repository`。
- apply 结果公布 §7 恢复裁决（`recoveryVerdict` / `recoveryResumeThrough` / `recoveryReason`），裁决证据来自本 Job 自己的提交边界，不是裸指纹：`job_api/runtime.rs::recovery_report` + `job_api/tests/job_lifecycle.rs::an_apply_job_publishes_a_recovery_verdict_over_its_own_boundaries`。
- 命令注册契约仍成立：`bootstrap::tests::every_tauri_command_is_registered_and_resolvable` 通过（新增两个命令已注册）。

## 2. 单文件规模

`packages/data-transfer/src/job/handler.rs` 789、`job/pipeline.rs` 678、`job/tests/cm46_pipeline.rs` 619；
宿主 `job_api/runtime.rs` 461、`job_api/tests/mod.rs` 681、`job_api/tests/job_lifecycle.rs` 141、`job_api/mod.rs` ~420、`plans.rs` 574。
单测超限时按职责拆成目录模块（`tests.rs` → `tests/mod.rs` + `tests/job_lifecycle.rs`），未新增超大文件。

## 3. round-1 缺陷台账（缺陷 → 修复 → 测试）

| 缺陷 | 严重度 | 修复 | 覆盖测试 |
| --- | --- | --- | --- |
| D1 SQL 文件 Job 被同 backend 门闸拒绝（`scope.rs:96` 无条件取 `job.database_target()`） | Blocker | 同 backend 判定只对实际存在的目标端点取 database target | `job_api/tests/mod.rs::a_sql_file_job_clears_the_same_backend_gate_on_both_job_paths`、`a_sql_file_job_from_a_foreign_backend_is_still_refused`；包侧 `cm49_sql_file_transfer_uses_no_target_connection` |
| D2 `PipelineBudget::capacity()` 从未被调用、`account()` 不拒绝，§6.2 未实现 | High | 每个 stage 进入前先比对 `capacity()`，超限直接拒绝该 stage | `cm46_budget_overrun_keeps_the_confirmed_boundaries`、`cm46_pipeline_byte_accounting_is_bounded_and_counted` |
| D3 预算超限返回空 boundaries + `RolledBack`，违反 §7 | High | 超限走 `outcome::unbounded_stage`，保留已确认提交边界 | `cm46_budget_overrun_keeps_the_confirmed_boundaries` |
| D4 handler 收到 `cancelled: None`；运行时只在 stage 之间检查 `cancel_requested`；`cancel_data_transfer` 走旧 `services::job_registry`；`run_data` 只映射 Failed/Succeeded | High | handler 收到真实 cancel token；管道在 `execute` 与 `commit` 之间复查并回滚在途批次（不产生边界）；`run_data` 见到取消即终态 `Cancelled` + `PartiallyApplied`；新增 `job_api/cancel.rs` 先走 Job 仓库 | `cm46_cancel_flag_stops_the_pipeline_before_the_first_write`、`cm46_cancel_during_transfer_rolls_the_inflight_batch_back`、`cm46_cancelled_stage_keeps_the_confirmed_boundaries`；宿主 `job_lifecycle::a_cancel_request_lands_on_the_job_repository` |
| D5 `value_bytes` 对 Json/Timestamp/Bool/Float 一律记 16 字节 | Medium | 按实际 payload 字节计量 | `cm46_oversize_json_payload_is_not_charged_sixteen_bytes` |
| D6 §7 证据协议是死代码，`verify_recovery` 无人调用 | Medium | apply 视图公布 `recoveryVerdict` / `recoveryResumeThrough` / `recoveryReason`，证据由 `derive_evidence` + 本 Job 提交边界生成 | `job_lifecycle::an_apply_job_publishes_a_recovery_verdict_over_its_own_boundaries`；包侧 `cm47_48_*`、`resume::tests::*`、`derive_evidence_*` |
| D7 门禁计数归属错误（把 1726 记在 `08493c333` 名下） | Low | 见 §4 归属修正 | 门禁首尾 sha 记录 |
| D8 apply 路径从不认领已存 plan，同一 planId 可被执行两次 | Medium | apply 先 `plans::claim` 再执行 | `job_api/tests/mod.rs::a_plan_id_serves_at_most_one_apply_job`、`job_lifecycle::an_apply_job_claims_the_plan_and_refuses_the_legacy_manager`、`two_concurrent_apply_jobs_share_exactly_one_plan_claim` |

## 4. 门禁

`CARGO_TARGET_DIR=/tmp/p5-dt-target`；四个工作目录均在
`.worktrees/datazen-p5-data-transfer`。长输出落系统临时目录，退出码单独打印。

```bash
cargo test -p datazen-data-transfer
cargo test -p datazen-runtime
cargo test -p datazen --lib
pnpm --config.verify-deps-before-run=false typecheck
```

逐字结论行（round-2，修复后）：

| 命令 | EXIT | 结论行 |
| --- | --- | --- |
| `cargo test -p datazen-data-transfer` | 0 | `test result: ok. 209 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.10s` |
| `cargo test -p datazen-runtime` | 0 | 32 个测试二进制全 ok；合计 `passed=834 failed=0`（lib 单元 448） |
| `cargo test -p datazen --lib` | 0 | `test result: ok. 1732 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 11.71s` |
| `pnpm --config.verify-deps-before-run=false typecheck` | 0 | `grep -c "error TS"` = 0；末行 `tsc -p tsconfig.pack-ep.json --noEmit` |

门禁首尾各记录一次 HEAD 与工作区 sha，证明运行期间无人改动工作树（四道门禁逐道记录，四次首尾完全一致）：

```text
PRE/POST  HEAD=20c9ff122d34022a6fa23a1898d48cf42498f49c  worktree=4d13d177d2fe2111749ac8867ff646264548af17
```

该 worktree sha 即 `760af1cf0` 提交的内容（提交前门禁先跑，提交后 sha 不再变化）。

### 门禁计数归属修正（round-1 的 D7）

| 提交 / 状态 | `cargo test -p datazen --lib` |
| --- | --- |
| `08493c333`（包实现 + CM 测试，未动宿主测试） | 1710 passed |
| `422341c4b`（commands 改写 + 16 条 `job_api/tests.rs` 用例） | 1726 passed（round-1 实测值，此前误记在 `08493c333` 名下） |
| `760af1cf0`（本轮再 +6 条：4 条 SQL 文件 / plan 认领，2 条取消 / 恢复裁决） | 1732 passed |

包侧同法：`08493c333` 为 194 passed，`760af1cf0` 为 209 passed（新增 15 条 CM-46/47/48/49 用例）。

### 已知编译告警（全部在 HEAD 已存在，非本轨引入）

- `packages/data-transfer/src/sql_file.rs:287` `AtomicSqlFile::create` / `create_with_encoding`、`:693` `insert_sql`、`:770` `insert_sql_with_target`、`packages/data-transfer/src/writer.rs:6` `bound_insert`：dead code，仅 `#[cfg(test)]` 调用方。
- `packages/data-transfer/src/transfer/adapter_registry.rs:148-168`：11 处 `unexpected cfg: driver-*`。
- 禁区 `packages/data-sync/src/compare.rs:221/238`：2 处 dead code。
- 宿主：`store/app_db.rs:10` 未用 `Deserialize`/`Serialize`；`store/app_db/{dashboards,runs,workflows}.rs` 未用 `OptionalExtension`；`store/key_store.rs:34` `KeyBackend::PlatformVault` 未构造；`commands/sync/comparison_store.rs:25` 未用 `PermissionsExt`；`commands/data_transfer/exec.rs` 未用 `std::os::unix::fs::PermissionsExt`；`commands/data_transfer/plans.rs:552/560` dead code（HEAD 已有，行号因本轨改动下移 1 行）。
- lib 测试额外告警：`commands/app_archive_tests.rs:4`、`commands/ipc_surface_tests.rs:7,8`、`commands/sync/plans.rs:47,746`。
- 本轨新增/改动文件 `job_api/*.rs`、`job_api/tests/*.rs`、`packages/data-transfer/src/job/**` 告警数为 0；生产路径无裸 `unwrap()` / `expect()`。

## 5. CM 覆盖矩阵

| CM | 契约 | 测试 |
| --- | --- | --- |
| CM-46 | 管道内存有界且按字节计量；慢写端必须反压；单值超限显式失败；预算超限不得丢边界（§6.2 / §7） | `job::tests::cm46_pipeline::cm46_pipeline_byte_accounting_is_bounded_and_counted`、`cm46_wide_rows_stay_inside_the_pipeline_bound`、`cm46_oversize_single_value_fails_explicitly`、`cm46_oversize_json_payload_is_not_charged_sixteen_bytes`、`cm46_budget_overrun_keeps_the_confirmed_boundaries`、`cm46_slow_writer_pauses_source_reader` |
| CM-46（取消） | 取消必须真的停下管道，且在途批次回滚而不产生提交边界 | `cm46_cancel_flag_stops_the_pipeline_before_the_first_write`、`cm46_cancel_during_transfer_rolls_the_inflight_batch_back`、`cm46_cancelled_stage_keeps_the_confirmed_boundaries` |
| CM-47 | 重启后不得凭裸指纹续传；checkpoint 丢失但提交边界仍在时不得回退重放（§7 `checkpoint-ack-missing`） | `job::tests::cm47_48_recovery::cm47_48_restart_never_resumes_from_bare_fingerprint`、`cm47_48_checkpoint_missing_but_committed_boundaries_preserved`；`resume::tests::test_tester_chunk_ack_loss_fences_checkpoint_after_target_commit`、`test_tester_confirmed_chunk_commit_fences_failed_checkpoint_advance` |
| CM-48 | 快照证明缺失、源变更、未知提交、清理未确认、禁止续传五类裁决均 fail-closed（§7） | `job::tests::cm47_48_recovery::cm47_48_source_changed_is_rejected`、`cm47_48_unknown_commit_requires_target_proof`、`cm47_48_cleanup_unconfirmed_requires_manual_review`、`cm47_48_resume_forbidden_policy_is_rejected`；`resume::tests::test_tester_source_fingerprint_failure_with_unknown_snapshot_rollback_fences_resume`；宿主 `job_api/tests/job_lifecycle.rs::an_apply_job_publishes_a_recovery_verdict_over_its_own_boundaries` |
| CM-49 | SQL 文件迁移不使用目标连接；无 IR 适配器的结构对象显式失败；产物按内容寻址 | `job::tests::cm49_sql_file::cm49_sql_file_transfer_uses_no_target_connection`、`cm49_structure_objects_without_ir_adapter_fail_explicitly`；宿主 `job_api/tests/mod.rs::emitted_sql_artifacts_are_content_addressed` |

包内可续传前置条件（声明式主键、chunk 驱动、两端一致快照、源目标不同会话）由
`resume::tests::only_exact_nonnullable_declared_primary_key_order_is_resumable`、
`resume::tests::test_tester_rejects_ambiguous_primary_key_resume_contracts` 覆盖。

## 6. 运行时轨道待裁定项（本轨为禁区，只读记录，未改动）

1. **`packages/runtime/src/job/runtime.rs` 只在 stage 之间检查 `cancel_requested`**（`CancelToken` 仅在阶段边界翻转），
   因此单个 stage 内部的取消无法被运行时中断。本轨改为把 token 交给 handler，由管道在每批之间复查来补齐；
   若运行时愿意按 batch 翻转，`cancel.rs` 的 watcher 可退化为只写 `request_cancel`。
2. **`StageOutcome` 只有 `error_code`**，没有错误消息；**`JobResult.error` 硬编码 `None`**
   （`runtime.rs` 终态映射 `:218-225`）。失败原因目前只能落在 Job 的 checkpoint/状态上，前端拿不到人类可读消息。
3. **`runtime.rs:214` 硬编码 `recovery_policy = "resumeAfterVerify"`**，与冻结计划算出的 `"forbidAutoResume"`
   冲突；本轨只能在 `verify_recovery` 前把 checkpoint 的 policy 对齐回冻结值（`job/handler.rs::verify_recovery`），
   属于绕行而非修复。
4. **`InMemoryJobRepository::request_cancel` 对终态 Job 返回 `PortError::CasConflict`**（`repository.rs:107`），
   语义上等于「并发变更，请重试」，容易被误读成真实冲突；本轨在 `job_api/cancel.rs` 先取 view、终态直接答「已处理」。
5. 以上四点建议由运行时轨道裁定：1 与 3 影响取消与恢复的真实语义，2 影响前端可诊断性。

## 7. 遗留与待裁定

1. **前端调用方未迁移**：`src/commands/**` 属禁区，本轨保留既有 `preview_data_transfer` /
   `execute_data_transfer` IPC 未删除。前端切到 `prepare_data_transfer_job` /
   `apply_data_transfer_job` 需在允许写 `src/**` 的轨道进行。
2. **`sql_file_destination` 与 `TransferEndpoints` 构造无单元测试**：需要真实驱动与 IR 适配器，
   成本高；当前由 CM-49 端到端用例 + `file_artifact_id` 单测覆盖。集成时可在 §10 联调补一条
   「SQL 文件 apply 返回 artifact id」的 IPC 级断言。
3. **`plans.rs:552/560` 的 `create_checkpoint` / `update_checkpoint` 死代码**：HEAD 已有（调用方只存在于
   `plans/checkpoint.rs` 与 `plans/tests.rs`）。§10 接入持久化 checkpoint 前应接上真实调用方，
   不要在并轨时顺手删除。
4. **`cargo fmt` 基线漂移**：`cargo fmt -p datazen-data-transfer --check` 在 HEAD 即有既存漂移。
   本轨只对新增目录 `packages/data-transfer/src/job/**` 与
   `src-tauri/src/commands/data_transfer/job_api/**` 执行 rustfmt 并验证 clean，
   未重排任何既有非 clean 文件。
5. **§10 联调**需在真库上确认：过期 planId / 目标漂移 / 权限变更后要求重新 prepare，
   以及 apply 响应超时时回查原幂等回执而不是新建 Job。
6. **取消语义边界**：`cancel_data_transfer` 对已终态 Job 答「已处理」、对陌生 id 答「不归我管」，
   两者都是 `Ok(true)/Ok(false)` 而非错误，前端不应据此区分成功与否，只应据此决定是否回落旧注册表。
7. **本台账在验收合并时必须删除**。
