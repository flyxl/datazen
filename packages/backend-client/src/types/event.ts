/**
 * 事件镜像：`EventEnvelope<T>` / `ArchivedEnvelope` / `ConnectionEvent`。
 *
 * 与 `packages/platform-api/src/dto/event.rs`、`dto/profile.rs` 逐字对应。
 *
 * **归档白名单**：`EventEnvelope` 带 `sessionHandle`（活句柄），**不可持久化**；
 * `ArchivedEnvelope` 只有六个白名单字段，**没有** `sessionHandle`。
 * 前端如果要「存下来」，必须走归档投影，不能把原始 envelope 塞进 localStorage。
 */

import type { Counter, Id } from './ids';
import type { ExecutionView, StatementResultSource } from './execution';
import type { SessionHandle, SessionView } from './session';

/** 流内序号（wire 为十进制字符串 Counter）。 */
export type EventSequence = Counter;

/** 流重置事件：客户端必须重新订阅，**不得据此推断执行未发生**。 */
export interface StreamResetRequiredEvent {
  readonly kind: 'streamResetRequired';
  readonly reason: string;
}

/** 结果块到达事件携带的来源标签。`kind` 是内部标签，wire 上与字段同级。 */
export interface ResultChunkEvent {
  readonly kind: 'resultChunk';
  readonly artifactId: Id;
  readonly chunkIndex: Counter;
  readonly source: StatementResultSource;
}

/** 连接事件载荷（wire 上是 `kind` 内部标签的 camelCase 判别联合）。 */
export type ConnectionEvent =
  | { readonly kind: 'sessionChanged'; readonly session: SessionView }
  | { readonly kind: 'executionChanged'; readonly execution: ExecutionView }
  | ResultChunkEvent
  | StreamResetRequiredEvent;

/** 实时事件信封。**带活句柄，只存在于订阅期。 */
export interface EventEnvelope<TPayload> {
  readonly streamId: Id;
  readonly sequence: EventSequence;
  /** 运行时纪元。与句柄里的 epoch 不一致说明宿主重启过。 */
  readonly runtimeEpoch: Id;
  readonly executionId: Id | null;
  readonly jobId: Id | null;
  /** 仅实时通道有值；归档投影会丢掉它。 */
  readonly sessionHandle: SessionHandle | null;
  readonly contextRevision: Counter;
  readonly payload: TPayload;
}

/**
 * 归档信封。**字段白名单**：只有下面六个。
 * 没有 `sessionHandle`——句柄永不落盘。
 */
export interface ArchivedEnvelope {
  readonly streamId: Id;
  readonly sequence: EventSequence;
  readonly runtimeEpoch: Id;
  readonly executionId: Id | null;
  readonly jobId: Id | null;
  readonly contextRevision: Counter;
}

/**
 * 投影为归档信封：丢掉落盘敏感字段。
 * 与 Rust `ArchivedEnvelope::project()` 同构。
 */
export function archiveEvent(envelope: EventEnvelope<ConnectionEvent>): ArchivedEnvelope {
  return {
    streamId: envelope.streamId,
    sequence: envelope.sequence,
    runtimeEpoch: envelope.runtimeEpoch,
    executionId: envelope.executionId,
    jobId: envelope.jobId,
    contextRevision: envelope.contextRevision,
  };
}

/** 该事件是否需要客户端重新订阅。 */
export function requiresResubscribe(event: ConnectionEvent): boolean {
  return event.kind === 'streamResetRequired';
}

/**
 * 信封是否属于指定运行时纪元。纪元不匹配的 envelope 必须丢弃：
 * 那是上一个宿主实例的尾部事件。
 */
export function belongsToEpoch(envelope: EventEnvelope<ConnectionEvent>, epoch: Id): boolean {
  return envelope.runtimeEpoch === epoch;
}

/**
 * 是否应保留该信封：序号的空洞意味着中间块丢了，
 * 继续拼接只会产出**看起来完整其实缺块**的结果，因此宁可丢弃后续块并要求重订阅。
 */
export function isContiguousAfter(previous: EventSequence | null, next: EventSequence): boolean {
  if (previous === null) return true;
  const from = Number.parseInt(previous, 10);
  const to = Number.parseInt(next, 10);
  if (!Number.isSafeInteger(from) || !Number.isSafeInteger(to)) return false;
  return to === from + 1;
}
