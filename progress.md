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

- `cargo test -p datazen-runtime`：EXIT=0，全部测试通过（job_kernel 16 个）
- `pnpm --config.verify-deps-before-run=false typecheck`：EXIT=0

## 第 2 轮 Tester 裁定修复记录（D1–D5，commit 4d8881c52）

| 编号 | 修复 |
| --- | --- |
| D1 | runtime.rs 拆出 dispatch()，claim/阶段失败/CAS 冲突等路径统一 release permits |
| D2 | record_commit_boundary 落库 Inner.boundaries（+ committed_boundaries 读取器）；checkpoint 聚合一次写、错误向上抛 |
| D3 | 新增 cm65 反向：不可准入 → 整组回滚、permits==0 |
| D4 | 旅程 A：pending_verification_reason 非空 + committed_boundaries==1 + handler 未重跑；旅程 B：verify_recovery→ResumeAfterVerify + calls==0 |
| D5 | budget.rs 实现改按 (service_key, connection_id) 排序，与注释对齐 |

补齐的持久用例（本轮新增）：

- `run_stage_error_propagates_and_releases_all_permits`：run_stage 直接 Err → run 透传 Err，BudgetLedger 许可 count==0
- `success_path_releases_permits_and_persists_both_boundaries`：成功路径 permits count==0，且单 stage 两条 CommitBoundary 经 committed_boundaries() 读出 2 条

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
5. 恢复旅程的"resume→新 claim 续跑未执行阶段"当前以核验断言替代派发（runtime.run 只接受 Queued）；续跑派发由 P9 多 worker 轨统一建立。
6. domains 抽取（schema_diff/data_sync/data_transfer）与三件套真实 handler 留给 Wave 1。

## 验收结果

- 第 1 轮 Tester 裁定：D1–D5 测试与台账未补齐（TEST_FAILED）。
- 本轮补齐后：job_kernel 16 个用例全绿；D1 失败路径 permits==0、D2 双边界落库、D4 旅程 A/B 决策断言均有持久用例断言；第 2 轮验收结论记入台账，待 Tester 复测裁定。
