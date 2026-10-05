# P5 Wave-1 `p5-schema-diff` 轨进度台账

> 分支：`feature/p5-schema-diff`，worktree：`.worktrees/datazen-p5-schema-diff`。
> 目标：Schema Diff 经 JobRuntime 执行（prepare/apply 分离的 JobHandler 接入）。
> 本文在收口合并时删除，不得带进 main。

## 改动清单

**packages/schema-diff**
- `Cargo.toml`：新增 `datazen-platform-api`、`datazen-runtime` 依赖。
- `src/lib.rs`：新增 `pub mod job`。
- `src/job/mod.rs`：模块导出（backend/handler/plan/recovery）。
- `src/job/plan.rs`：`SchemaDiffFrozenPlan`（§2.2）、`RecoveryPolicy`、`PlanStore`、`SchemaDiffPlanError`、`fnv1a64_hex`、`SUPPORTED_PLAN_MAJOR`；`validate_for_apply` 校验 fingerprint/endpointEvidence/selectionRevision/过期/版本守卫。
- `src/job/backend.rs`：`SchemaDiffJobBackend` 接缝（`prepare_plan`/`verify_authorization`/`read_target_fingerprint`/`deploy`/`read_only_verify`）、`PrepareRequest`（Table/Unified）、`ApplyRequest`（含 profile 透传）、`PreparedPlan`、`ReadOnlyVerdict`。
- `src/job/handler.rs`：`SchemaDiffHandler`（`for_prepare`/`for_apply`）实现 `JobHandler::validatePlan`（版本守卫 + apply 的 planId 一次性 + selectionRevision + PlanStore 校验）、`runStage`（prepare：短读→比对→渲染计划入 PlanStore；apply：复验授权快照、fingerprint PlanStale、`take_for_apply`、`deploy` → §4.2 语义映射与逐 operation CommitBoundary）、`verifyRecovery`（委托 `decide_recovery`）。
- `src/job/recovery.rs`：`decide_recovery`（纯函数）——版本/能力变化→Reject；任一边界或顶层证据含 `ddlResponseLost`/`commitAckLost` 未核验→RequireManualReview；全部边界 verified→ResumeAfterVerify。
- `tests/job_handler.rs`：CM-41（planId 一次消费/过期/selectionRevision/自覆盖 EndpointOverlap）、CM-42（非事务 DDL 部分成功 PartiallyApplied/未知 operation→Unknown+只读核验 RequireManualReview）、§4.2 三行（整组回滚 RolledBack/DDL 响应丢失 Unknown+RequireManualReview/取消 Cancelled+边界保留在途实际终态）+ prepare 阶段冒烟，共 10 例。

**src-tauri**
- `Cargo.toml`：新增 `datazen-runtime` 依赖。
- `src/commands/mod.rs`：`AppState.schema_diff_jobs: Arc<SchemaDiffJobInfra>`（4 处构造点同步：bootstrap/app_state.rs、mcp.rs、context.rs×2）。
- `src/commands/schema_diff/job.rs`（新）：`SchemaDiffJobInfra`（InMemoryJobRepository/BudgetLedger/PlanStore）、`SystemJobClock`、`AppStateBackend`（prepare 走既有 prepare impl，deploy 走既有 deploy impl，capability 快照复验、fingerprint 重读比对、只读核验保守裁决 `Indeterminate`）、`run_prepare_job`/`run_apply_job`（JobRuntime 编排 + 端点预算 + EndpointRef）、`register_plan_for_apply`（旧携带正文路径的兼容注册）、`SchemaDiffPrepareEnvelope`。
- `src/commands/schema_diff.rs`：`prepare_schema_diff_plan` → JobRuntime；`execute_schema_diff_deploy` 新增 `planId`/`selectionRevision` 参数、经 `run_apply_job`；`pub mod job`。
- `src/commands/schema_diff/unified_plan.rs`：`prepare_schema_unified_plan` 经 JobRuntime；抽出 `prepare_schema_unified_plan_impl`（pub(crate)）。

**前端**
- `src/commands/schemaDiff.ts`：`SchemaDiffPrepareEnvelope` 类型；prepare* 包装返回 envelope；`executeDeploy` 透传 `planId`/`selectionRevision`。
- `src/windows/schema-diff/SchemaDiffWindow.tsx`：保存 `planMeta`（envelope），deploy 时传 `planId`/`selectionRevision`。
- 测试 mock 同步 envelope 形状（`schemaDiff.test.ts`/`SchemaDiffWizard`/`SchemaDiffProfileLoad`）。

## 门禁（本地实测）

| 命令 | 结论 |
| --- | --- |
| `CARGO_TARGET_DIR=/tmp/p5-sd-target cargo test -p datazen-schema-diff` | EXIT=0；225 + 10(job_handler) + 0 通过 |
| `CARGO_TARGET_DIR=/tmp/p5-sd-target cargo test -p datazen-runtime` | EXIT=0；32 个 ok 块，0 失败 |
| `CARGO_TARGET_DIR=/tmp/p5-sd-target cargo test -p datazen --lib` | EXIT=0；1710 passed, 0 failed, 6 ignored |
| `pnpm --config.verify-deps-before-run=false typecheck` | EXIT=0 |
| `npx vitest run src/commands/__tests__/schemaDiff.test.ts` | EXIT=0；21 passed |
| `npx vitest run src/windows/schema-diff` | EXIT=0；9 文件 66 passed |

## 遗留 / 待裁定

1. `prepare_schema_view_plan`/`prepare_schema_routine_trigger_plan`/`prepare_schema_sequence_plan`/`prepare_schema_type_plan` 仍走旧直接管理器路径（未进 JobRuntime）；窗口不调用这四个命令，本轨未改。
2. `AppStateBackend::read_only_verify` 保守返回 `Indeterminate`（未知 operation 一律人工核验）；精确的逐 operation 只读核验记录器随工件台账后续补齐。
3. 取消：部署内取消沿用 `services/job_registry.rs` 标志（前端取消按钮 → `cancel_schema_diff_deploy` → deploy 每语句观察）；JobRuntime 的 `CancelToken` 覆盖阶段边界。
4. `execute_schema_diff_deploy` 旧签名兼容：未传 `planId` 时把客户端正文计划注册入 PlanStore 再经 apply Job（等价走新管线）。
5. `progress.md`/`hub.md` 在验收合并时必须删除。
