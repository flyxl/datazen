# p5-client 进度台账（Wave-1，分支 feature/p5-client）

基线：与其它 P5 Wave-1 轨一致（`214c44565`）。本轨边界：`packages/backend-client/**`、`src/**`（三个窗口最小接线 + en.ts）。未改任何 Rust 源码。

## 已完成

1. **backend-client Job DTO 对齐**（`packages/backend-client/src/types/jobs.ts`、`client.ts`）：
   - `JobView` 增补 `effectOutcome` / `cancelRequested` / `pendingVerificationReason` / `progress`（五类计数：read/converted/attempted/committed/unknown，缺省为零）。
   - 新增 `JobProgress`、`emptyJobProgress()`、`CommitBoundary`（operationId/batchId/payloadDigest/evidence/verifiedAt，与 `packages/platform-api/src/dto/job.rs` §7 对齐）。
   - `client.ts` 新增 `parseJobView` / `parseCancelReceipt`：宽线协议（number 或 ISO 时间戳）归一化、缺失 P5 字段给文档化默认、非法 state/disposition 直接 `ServiceUnavailable` 报错。`startJob`/`getJob`/`listJobs`/`cancelJob` 全部经过解码。
   - `watchJob`：轮询 getJob；任一错误后下一次成功视为 `resubscribed: true`（重连后重新订阅语义）；`stop()` 只清本地定时器，不取消/不释放后端 Job。
   - `submitJobIdempotent`：超时/OutcomeUnknown 先查原幂等回执（进程内 token→jobId 映射 + `listJobs` 按 kind/创建时间扫描唯一候选），找不到则报 `OutcomeUnknown`，**绝不二次派发 apply**。
2. **三窗口最小接线**：
   - 新增 `src/hooks/useMigrationJobHydration.ts`：挂载时先 `listJobs` 再 `getJob` 的活动 Job 清单 + 待核验清单；unmount 只 `stop()` watch（只退订），不调 cancel/remove。
   - 新增 `src/lib/migrationJobHydration.ts`：`MIGRATION_JOB_KINDS`、`hydrateMigrationJobs`、`classifyJobView`、`latestApplyJob`、`isStalePlanError`（匹配 `PlanStale`/`SourceChanged`/`PermissionDenied`，含 invoke 错误的 message 兜底）。
   - `SchemaDiffWindow` / `DataSyncWindow` / `DataTransferWindow`：挂载先查 Job，顶部横幅展示 pending-verification 与 hydration 错误；apply 失败且命中 stale-plan 错误码时 `setError*` + 清空 plan/result，不自动用旧 SQL 应用。
3. **en.ts**：`src/locales/en/sync.ts` 新增 `migrationJob.reprepareOnStalePlan`、`migrationJob.pendingVerificationHint`（仅 en）。
4. **测试**：
   - `packages/backend-client/__tests__/jobs.test.ts`（13 例）：DTO 解码默认值/ISO 时间戳归一化/非法 state/CancelReceipt、watchJob 重订阅与 stop() 只退订、submitJobIdempotent 三路径（回执恢复/未知提交不新建 Job/唯一近期 Job）。
   - `src/lib/__tests__/migrationJobHydration.test.ts`（5 例）：窗口 kind 映射、分类、stale-plan 判定、hydrate 顺序（listJobs→getJob）。

## 门禁（逐字结论）

- `pnpm --config.verify-deps-before-run=false typecheck` → EXIT=0，0 条 `error TS`。
- `node <主仓>/node_modules/vitest/vitest.mjs run` → EXIT=0，`Test Files  567 passed (567) | Tests  5948 passed (5948)`。

## 覆盖的 §8 条款

- 「BackendClient 的 startJob/getJob/cancelJob 为通用路径」→ client.ts 解析对齐 + watchJob 重新订阅语义落地。
- 「任务窗口重新打开先查 Job，再读计划/结果」→ 三个窗口挂载 effect 以 `hydrateMigrationJobs`（listJobs→getJob）为先。
- 「UI unmount 只退订（不释放资源）」→ `useMigrationJobHydration` cleanup 只置 cancelled 标志 + `watchJob.stop()`；无任何 cancel/remove 调用。
- 「planId 过期、目标漂移或权限变化要求重新准备，不自动用旧 SQL 应用」→ `isStalePlanError` → 清 plan/result + 提示重准备。
- 「apply 响应超时先查原幂等回执，未知提交不新建 Job」→ `submitJobIdempotent` 与单测覆盖。

## 遗留

- 桌面 adapter 的 kernel job 命令（`start_job`/`get_job`/`list_jobs`/`cancel_job`/`read_artifact`）尚未在 Rust 侧注册（基线即如此），`tauriBackendTransport` 会映射到不存在的 invoke；窗口在 desktop 真实运行时会在 hydration 横幅显示 ServiceUnavailable 类提示，不阻塞旧路径。若 Wave-1 handler 轨登记这些命令，本轨代码即可直接生效。
- 若 Wave-1 其它轨已落地 kernel 命令签名与 DTO 不一致，需在对应窗口加薄适配（本轨未触发 Rust 改动）。
