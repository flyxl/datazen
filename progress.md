# P5 Wave-1 · Data Transfer（Job 化迁移）进度台账

> 开发期台账，交付即销毁。本文件不是交付物。

- 轨道分支：`feature/p5-data-transfer`
- 基线 HEAD：`214c44565154639595bda1997a300888625e6aac`
- 提交：`08493c333`（包实现 + CM 测试）、`422341c4b`（commands 改写 + 单测）、本文件（台账）
- 目标文档：`docs/architecture/platform/data-migration-jobs.md` §2/§6/§7/§8/§10、`docs/architecture/backend/data-transfer.md`、`docs/architecture/platform/persistence-model.md` §3.6

## 1. 自证

### 已完成

| # | 任务 | 落点 |
| --- | --- | --- |
| 1 | `DataTransferHandler`（prepare/apply）+ 8MiB 有界管道 `PipelineBudget`（§6.2） | `packages/data-transfer/src/job/{handler,pipeline,plan,checkpoint,recovery,sqlfile,mod}.rs` |
| 2 | CM-46 / CM-47 / CM-48 / CM-49 测试 | `packages/data-transfer/src/job/tests*/`、`packages/data-transfer/src/resume/tests.rs` |
| 3 | commands 改写：`prepare_data_transfer_job` → Job，`apply_data_transfer_job` → Job（planId + confirmation）；§8 跨 backend 显式拒绝 | `src-tauri/src/commands/data_transfer/job_api/**`、`bootstrap/run.rs`、`plans.rs`、`exec.rs` |
| 4 | 四道门禁全绿（见下） | — |
| 5 | 本台账 | `progress.md` |

### 未开始 / 不在本轨范围

- 前端 IPC 调用方迁移：`src/commands/**` 属禁区（前端封装不在本轨可写范围），见 §4 遗留。
- §10 联调（真库 E2E）与 Pro 打包验证。

### 关键事实（可从代码与测试直接读出）

- prepare 载荷 **不含** `consumedPlanId`，apply 载荷 **只带** planId/planDigest/selectionRevision/selection/confirmedDestructive，端点与凭据不进入 apply 载荷：`job_api/{mod.rs:prepare_payload,apply_payload}` + `job_api/tests.rs::apply_payload_never_carries_endpoints_or_credentials`。
- 载荷不是照抄运行时规则，而是喂给真实投影器断言：`job_api/tests.rs` 直接调用 `datazen_runtime::job::project_frozen_plan`。
- §8 为 fail-closed：必须显式声明 `backendScope`、只接受本地 backend 作用域、端点引用必须是内存会话 token 形态（≤200 字符、无空白、不含 `/\@?#:`），否则 `CommandError::Validation` 拒绝并提示重新 prepare：`job_api/scope.rs` + `job_api/tests.rs::same_backend_scope_*`。
- 一个 planId 至多服务一个 apply Job；一次性消耗先于过期/digest/revision 判断，重放只会得到「一次性事实」而非时序相关理由：`job_api/admission.rs` + `job_api/tests.rs::admission_reports_one_shot_consumption_before_expiry`。
- plan 的 revision 与 expiry 是存储事实，不是重算结果：apply 前 digest 不符、review 过期、选择版本漂移一律要求重新 prepare；SQL 文件产物按内容寻址（CM-49）：`plans.rs`、`job_api/mod.rs::plan_digest/file_artifact_id`。
- 两端共用同一个 `service_key`，自覆盖因此是硬拒绝而非按连接 id 判断：`job_api/runtime.rs::endpoint_refs` + `job_api/tests.rs::both_endpoints_share_one_service_key_and_sql_file_has_no_writer`。
- 命令注册契约仍成立：`bootstrap::tests::every_tauri_command_is_registered_and_resolvable` 通过（新增两个命令已注册）。

## 2. 门禁

`CARGO_TARGET_DIR=/tmp/p5-dt-target`；四个工作目录均在
`.worktrees/datazen-p5-data-transfer`。长输出落系统临时目录，退出码单独打印。

```bash
cargo test -p datazen-data-transfer
cargo test -p datazen-runtime
cargo test -p datazen --lib
pnpm --config.verify-deps-before-run=false typecheck
```

逐字结论行：

| 命令 | EXIT | 结论行 |
| --- | --- | --- |
| `cargo test -p datazen-data-transfer` | 0 | `test result: ok. 194 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.38s` |
| `cargo test -p datazen-runtime` | 0 | 31 个测试二进制全 ok；lib 单元 `test result: ok. 448 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s`；合计 `passed=834 failed=0` |
| `cargo test -p datazen --lib` | 0 | `test result: ok. 1726 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 12.71s` |
| `pnpm --config.verify-deps-before-run=false typecheck` | 0 | `grep -c "error TS"` = 0；末行 `tsc -p tsconfig.pack-ep.json --noEmit` |

门禁首尾各记录一次 HEAD 与工作区 sha，证明运行期间无人改动工作树：

```text
PRE-GATE  HEAD=08493c3330e2a98dbfd3001850c95cb8f4d5e544  worktree=559ddd970642502febd8d9f18a13f2c4
POST-GATE HEAD=08493c3330e2a98dbfd3001850c95cb8f4d5e544  worktree=559ddd970642502febd8d9f18a13f2c4
```

### 已知编译告警（全部在 HEAD 已存在，非本轨引入）

- `packages/data-transfer/src/sql_file.rs:287` `AtomicSqlFile::create` / `create_with_encoding`、`:693` `insert_sql`、`:770` `insert_sql_with_target`、`packages/data-transfer/src/writer.rs:6` `bound_insert`：dead code，仅 `#[cfg(test)]` 调用方。
- `packages/data-transfer/src/transfer/adapter_registry.rs:148-168`：11 处 `unexpected cfg: driver-*`。
- 禁区 `packages/data-sync/src/compare.rs:221/238`：2 处 dead code。
- 宿主：`store/app_db.rs:10` 未用 `Deserialize`/`Serialize`；`store/app_db/dashboards.rs:5`、`store/app_db/runs.rs:6`、`store/app_db/workflows.rs:5` 未用 `OptionalExtension`；`commands/sync/comparison_store.rs:25` 未用 `PermissionsExt`；`commands/data_transfer/exec.rs` 未用 `std::os::unix::fs::PermissionsExt`；`commands/data_transfer/plans.rs:553/561` dead code（HEAD 已有）。
- 本轨新增文件 `job_api/*.rs` 与 `packages/data-transfer/src/job/**` 告警数为 0；生产路径无裸 `unwrap()` / `expect()`。

## 3. CM 覆盖矩阵

| CM | 契约 | 测试 |
| --- | --- | --- |
| CM-46 | 管道内存有界且按字节计量；慢写端必须反压；单值超限显式失败（§6.2） | `job::tests::cm46_pipeline::cm46_pipeline_byte_accounting_is_bounded_and_counted`、`cm46_slow_writer_pauses_source_reader`、`cm46_oversize_single_value_fails_explicitly` |
| CM-47 | 重启后不得凭裸指纹续传；checkpoint 丢失但提交边界仍在时不得回退重放（§7 `checkpoint-ack-missing`） | `job::tests::cm47_48_recovery::cm47_48_restart_never_resumes_from_bare_fingerprint`、`cm47_48_checkpoint_missing_but_committed_boundaries_preserved`；`resume::tests::test_tester_chunk_ack_loss_fences_checkpoint_after_target_commit`、`test_tester_confirmed_chunk_commit_fences_failed_checkpoint_advance` |
| CM-48 | 快照证明缺失、源变更、未知提交、清理未确认、禁止续传五类裁决均 fail-closed（§7） | `job::tests::cm47_48_recovery::cm47_48_source_changed_is_rejected`、`cm47_48_unknown_commit_requires_target_proof`、`cm47_48_cleanup_unconfirmed_requires_manual_review`、`cm47_48_resume_forbidden_policy_is_rejected`；`resume::tests::test_tester_source_fingerprint_failure_with_unknown_snapshot_rollback_fences_resume` |
| CM-49 | SQL 文件迁移不使用目标连接；无 IR 适配器的结构对象显式失败；产物按内容寻址 | `job::tests::cm49_sql_file::cm49_sql_file_transfer_uses_no_target_connection`、`cm49_structure_objects_without_ir_adapter_fail_explicitly`；宿主 `job_api/tests.rs::emitted_sql_artifacts_are_content_addressed` |

包内可续传前置条件（声明式主键、chunk 驱动、两端一致快照、源目标不同会话）由
`resume::tests::only_exact_nonnullable_declared_primary_key_order_is_resumable`、
`resume::tests::test_tester_rejects_ambiguous_primary_key_resume_contracts` 覆盖。

## 4. 遗留与待裁定

1. **前端调用方未迁移**：`src/commands/**` 属禁区，本轨保留既有 `preview_data_transfer` /
   `execute_data_transfer` IPC 未删除。前端切到 `prepare_data_transfer_job` /
   `apply_data_transfer_job` 需在允许写 `src/**` 的轨道进行。
2. **`sql_file_destination` 与 `TransferEndpoints` 构造无单元测试**：需要真实驱动与 IR 适配器，
   成本高；当前由 CM-49 端到端用例 + `file_artifact_id` 单测覆盖。集成时可在 §10 联调补一条
   「SQL 文件 apply 返回 artifact id」的 IPC 级断言。
3. **`plans.rs:553/561` 的 `create_checkpoint` / `update_checkpoint` 死代码**：HEAD 已有（调用方只存在于
   `plans/checkpoint.rs` 与 `plans/tests.rs`）。§10 接入持久化 checkpoint 前应接上真实调用方，
   不要在并轨时顺手删除。
4. **`cargo fmt` 基线漂移**：`cargo fmt -p datazen-data-transfer --check` 在 HEAD 即有既存漂移。
   本轨只对新增目录 `packages/data-transfer/src/job/**` 与
   `src-tauri/src/commands/data_transfer/job_api/**` 执行 rustfmt 并验证 clean，
   未重排任何既有非 clean 文件。
5. **§10 联调**需在真库上确认：过期 planId / 目标漂移 / 权限变更后要求重新 prepare，
   以及 apply 响应超时时回查原幂等回执而不是新建 Job。