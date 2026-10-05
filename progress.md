# P5 Wave 0 · p5-job-core 轨进度台账

> 台账规则：本文件随分支流转，轨道验收合并时销毁；结论必须在代码与测试中自明。

## 完成定义核对（§7 P5 退出门槛中的本轨份额）

| 项 | 状态 | 证据 |
| --- | --- | --- |
| JobHandler/plan/checkpoint 版本/取消意图/effectOutcome 聚合/恢复决策表冻结 | 完成 | `packages/runtime/src/job/handler.rs`、`plan.rs`、`runtime.rs` |
| JobRepository：持久化接受、幂等 receipt、planId 唯一消费、claim/renew/fencing | 完成 | `packages/runtime/src/job/repository.rs`；job_kernel.rs |
| jobs.plan apply 投影与唯一约束 + checkpoint 带版本 | 完成（内存等价物） | accept 同锁事务内建立唯一消费；checkpoint 以 (job_id, state_version) 冲突 |
| 多端预算原子预留 + EndpointOverlap 先拒绝 | 完成 | `job/budget.rs` |
| 幂等同键重试原 receipt、窗口裁决不释放资源 | 完成 | cm54；queued cancel intent-only 断言 |
| 持久化路径无运行时句柄；事件载荷有界 | 完成 | persisted_shapes_never_carry_runtime_handles 断言 |

## 门禁实测

- `cargo test -p datazen-runtime`：EXIT=0，全部测试通过（job_kernel 13 个）
- `pnpm --config.verify-deps-before-run=false typecheck`：EXIT=0

## CM 覆盖矩阵（对照 data-migration-jobs.md §10）

| CM | 覆盖点 | 测试 |
| --- | --- | --- |
| CM-31 | endpoint 重叠先拒绝、不持许可 | `cm31_overlap_is_rejected_before_any_permit_is_held` |
| CM-40 | 排队取消意图-notStarted 终结（窗口裁决只订阅） | `queued_cancel_is_an_intent_only_and_notstarted_finalizes`、`queued_cancel_before_run_finalizes_not_started_without_budget_or_claim` |
| CM-41 | planId 唯一消费 / 版本守卫 | `cm54_plan_id_is_consumed_once_by_apply_jobs`、`cm67_unknown_version_major_is_rejected_at_accept` |
| CM-54 | 幂等同键原 jobId、同键不同指纹冲突 | `cm54_same_key_returns_same_job_id_without_reexecution` |
| CM-65 | 多端全组预留或全排队、拒绝路径不持许可 | `cm65_multi_endpoint_reserve_is_all_or_nothing` |
| CM-67 | 未知 major 拒绝（计划/处理器版本复验） | `cm67_unknown_version_major_is_rejected_at_accept` + `fencing_old_claim_writes_are_rejected_after_takeover` |

## 遗留与待裁定项

1. `Checkpoint` DTO 缺独立的 `checkpointVersion` 字段，内存仓储以 `JobStateVersion` 代用；建议单独轨补齐（需要 platform-api + DDL 对齐复核）。
2. `JobRepository` 端口方法名未保留 `organization_id` 参数（组织隔离由 `RequestContext` 承担）；内存实现以 job_id 为键的单组织演示，完整组织键隔离由服务端仓储落地。
3. `request_cancel`/`mark_failed_unstarted`/`confirm_cancelled_not_started` 等意图与终结接口尚未进入 platform-api 端口 trait，属 runtime 层裁决，待下次端口版本收口。
4. 事件流尚未接入 `EventSink`（运行时目前不发事件）；事件载荷白名单仅以序列化白名单断言覆盖。接入时再补有界载荷断言。
5. 恢复旅程的"rereRresume→新 claim 续跑未执行阶段"当前以核验断言替代派发（runtime.run 只接受 Queued）；续跑派发由 P9 多 worker 轨统一建立。
6. domains 抽取（schema_diff/data_sync/data_transfer）与三件套真实 handler 留给 Wave 1。
