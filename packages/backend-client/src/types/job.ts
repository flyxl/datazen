/**
 * 任务镜像：`JobState` / `JobView` / `JobDefinition` / `StageRecord`。
 *
 * 与 `packages/platform-api/src/dto/job.rs` 逐字对应（camelCase wire）。
 *
 * 任务执行是**后台执行**：概要 §5.1(2) 规定后台执行不带 `clientInstanceId`，
 * 身份来自 `JobDefinition.owner`。因此 `OwnerRef` 在这里是记录的一部分，不是请求参数。
 */

import type { Counter, Id, JsonValue, Timestamp } from './ids';
import type { OwnerRef } from './context';

/** 任务状态机。终态是 `succeeded` / `failed` / `cancelled`。 */
export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

/** 客户端可见的任务全貌。 */
export interface JobView {
  readonly jobId: Id;
  readonly kind: string;
  readonly state: JobState;
  /** 当前阶段名；无阶段任务为 `null`。 */
  readonly stage: string | null;
  readonly executionIds: readonly Id[];
  readonly artifactIds: readonly Id[];
  readonly createdAt: Timestamp;
  readonly updatedAt: Timestamp;
}

/**
 * 计划正文。**敏感内容只允许引用凭据引用，禁止内联凭据**——
 * 前端构造 payload 时不得把密码/token 塞进来。
 */
export interface JobDefinition {
  readonly jobId: Id;
  readonly kind: string;
  readonly owner: OwnerRef;
  readonly payload: JsonValue;
  readonly createdAt: Timestamp;
}

/** 阶段记录。`claimedBy` 为空表示尚未被任何 worker 认领。 */
export interface StageRecord {
  readonly jobId: Id;
  readonly stageId: Id;
  readonly kind: string;
  readonly claimedBy: Id | null;
  readonly executionIds: readonly Id[];
  readonly startedAt: Timestamp | null;
  readonly finishedAt: Timestamp | null;
}

/**
 * 任务查询过滤条件。分页游标**不落盘、不跨重启**，因此用 `Counter` 而非持久化游标。
 * 与 Rust `JobFilter` 同构。
 */
export interface JobFilter {
  readonly states: readonly JobState[];
  readonly owner: OwnerRef | null;
  readonly after: Counter | null;
  readonly limit: number | null;
}

const TERMINAL_JOB_STATES: readonly JobState[] = ['succeeded', 'failed', 'cancelled'];

/** 是否已终态。终态任务的 `cancelJob` 只会拿到 `alreadyFinished`。 */
export function isTerminalJobState(state: JobState): boolean {
  return TERMINAL_JOB_STATES.includes(state);
}

/**
 * 任务是否已完成（无论成功失败）。前端据此停止轮询。
 * `cancelRequested` 之类的中间态不在本文件判定范围内——任务没有该状态。
 */
export function isJobSettled(view: JobView): boolean {
  return isTerminalJobState(view.state);
}

/** 任务的全部产物 id（只读副本）。 */
export function jobArtifactIds(view: JobView): readonly Id[] {
  return view.artifactIds;
}

/** 阶段显示名；无阶段时回退到 kind，避免 UI 出现空白。 */
export function jobStageLabel(view: JobView): string {
  return view.stage ?? view.kind;
}
