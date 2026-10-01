/**
 * 执行镜像：`ExecutionState` / `EffectOutcome` / `ExecutionErrorCode` / `ExecutionReceipt` /
 * `ExecutionView` / `CapabilitySnapshot` / `ResultProvenance` / `StatementResultSource`。
 *
 * 与 `packages/platform-api/src/dto/execution.rs` 逐字对应（camelCase wire）。
 *
 * **终态失败用 `ExecutionState='failed'` + `errorCode` 表达，不走 `ApiError`**（CM §13.1）。
 * `ApiError` 只表示「这次调用本身没被受理」。两者不可混用。
 */

import type { Counter, Id, JsonObject, JsonValue, Timestamp } from './ids';
import type { ExecutionTarget } from './target';
import type { SessionContext, SessionHandle } from './session';

/** 执行状态机。终态是 `succeeded` / `failed` / `cancelled`。 */
export type ExecutionState =
  | 'queued'
  | 'running'
  | 'cancelRequested'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

/**
 * 副作用结论。与执行状态**正交**：`closeSession` 中资源丢失时 `state='lost'` 而
 * `effectOutcome='unknown'`，不得写成 `rolledBack`（CM §13.1）。
 */
export type EffectOutcome =
  | 'notStarted'
  | 'completed'
  | 'rolledBack'
  | 'partiallyApplied'
  | 'unknown';

/** 执行失败原因。**不是** `ApiError` 的 code，两套枚举互不包含。 */
export type ExecutionErrorCode =
  | 'sqlError'
  | 'protocolError'
  | 'cancelled'
  | 'timeout'
  | 'resourceLost'
  | 'pipelineAborted'
  | 'hostRejected';

/** 结果是否收齐。`truncated` 时 `truncationReason` 非空。 */
export type ResultCompleteness = 'pending' | 'complete' | 'truncated';

/** 结果可写性（表映射探测结论）。 */
export type WritableMapping = 'verified' | 'readOnly';

/** 执行开始时冻结的能力快照。**冻结后不可变**，用于回放与结果溯源。 */
export interface CapabilitySnapshot {
  readonly driverId: string;
  readonly driverVersion: string;
  readonly protocolVersion: number;
  /** 能力修订号（wire 为十进制字符串 Counter）。 */
  readonly capabilityRevision: Counter;
  readonly confirmed: JsonObject;
}

/** 结果溯源。全部字段在执行**开始时**写入，不事后补写。 */
export interface ResultProvenance {
  readonly organizationId: Id;
  readonly principalId: Id;
  readonly connectionId: Id;
  /** 配置版本（wire 为十进制字符串 Counter）。 */
  readonly configRevision: Counter;
  readonly contextBefore: SessionContext;
  readonly contextAfter: SessionContext;
  readonly requestedTarget: ExecutionTarget;
  readonly capabilitySnapshot: CapabilitySnapshot;
  readonly executedAt: Timestamp;
}

/** 单条语句的结果来源。 */
export interface StatementResultSource {
  readonly executionId: Id;
  readonly statementIndex: number;
  readonly context: SessionContext;
  readonly relation: ExecutionTarget | null;
  readonly writableMapping: WritableMapping;
}

/** 受理回执。**返回它不代表 SQL 已成功**，只代表请求被接受并已落幂等记录。 */
export interface ExecutionReceipt {
  readonly executionId: Id;
  readonly streamId: Id;
  readonly state: ExecutionState;
}

/** 运行时结果绑定：持有活句柄，**只存在于实时视图，绝不落盘**。 */
export interface RuntimeResultBinding {
  readonly handle: SessionHandle;
  readonly resourceBindingId: Id;
  readonly executionId: Id;
}

/** 客户端可见的执行全貌。 */
export interface ExecutionView {
  readonly executionId: Id;
  readonly state: ExecutionState;
  readonly effectOutcome: EffectOutcome;
  readonly provenance: ResultProvenance | null;
  readonly artifactIds: readonly Id[];
  readonly resultCompleteness: ResultCompleteness;
  readonly truncationReason: string | null;
  readonly errorCode: ExecutionErrorCode | null;
  /**
   * 实时绑定。宿主重连 / 页面重载后为 `null`；此时 `provenance` 仍然是可用的。
   * 前端**不得**把它当作可复用句柄缓存。
   */
  readonly runtimeBinding: RuntimeResultBinding | null;
}

const TERMINAL_STATES: readonly ExecutionState[] = ['succeeded', 'failed', 'cancelled'];

/** 是否已终态。终态之后 `cancelExecution` 只能拿到 `alreadyFinished`。 */
export function isTerminalState(state: ExecutionState): boolean {
  return TERMINAL_STATES.includes(state);
}

/** 该视图是否仍挂着活运行时绑定。 */
export function hasLiveRuntime(view: ExecutionView): boolean {
  return view.runtimeBinding !== null;
}

/**
 * 终态失败的读取入口：把 `state` + `errorCode` 合成一句可展示文案。
 * 未知码原样透出，不猜语义。
 */
export function describeExecutionFailure(view: ExecutionView): string | null {
  if (view.state !== 'failed') return null;
  if (view.errorCode === null) return 'execution failed';
  return `execution failed: ${view.errorCode}`;
}

/** `CapabilitySnapshot.confirmed` 的自由形状读取；缺失返回 `null` 而不是伪造默认值。 */
export function confirmedCapability(snapshot: CapabilitySnapshot, key: string): JsonValue | null {
  return Object.prototype.hasOwnProperty.call(snapshot.confirmed, key)
    ? snapshot.confirmed[key]
    : null;
}
