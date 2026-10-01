/**
 * 会话镜像：`SessionHandle` / `SessionContext` / `SessionView` / 三个回执 / `CancelReceipt`。
 *
 * 与 `packages/platform-api/src/dto/session.rs` 逐字对应（camelCase wire）。
 *
 * **`connectionId` 与 `dbSessionId` 不可混用**：`connectionId` 是持久化配置 id，
 * `dbSessionId` 是运行时数据库会话 id（内存态，永不落盘）。`SessionHandle` 把两者
 * 连在一起时用 `dbSessionId` 字段名，前端看到字段名就该知道不能拿去当配置 id 用。
 */

import type { Counter, Id, Timestamp } from './ids';
import type { EffectOutcome, ExecutionState } from './execution';
import type { ExecutionTarget, NamespaceTarget } from './target';
import type { OwnerRef } from './context';

/**
 * 会话句柄。`runtimeEpoch` 用于发现「宿主重启后旧句柄失效」，
 * 宿主收到不匹配的 epoch 必须回 `RuntimeEpochMismatch`。
 */
export interface SessionHandle {
  readonly dbSessionId: Id;
  readonly runtimeEpoch: Id;
}

/** 事务状态。`unsupported` 表示该驱动不支持事务，不是「没有事务」。 */
export type TransactionState = 'none' | 'active' | 'aborted' | 'unknown' | 'unsupported';

/**
 * 上下文置信度。`partial` / `unknown` 时**不允许**把会话已确认值当作默认值补齐
 * 请求目标（连接 §4.3）。
 */
export type ContextConfidence = 'confirmed' | 'partial' | 'unknown';

/** 会话当前上下文快照。 */
export interface SessionContext {
  readonly namespace: NamespaceTarget;
  readonly searchPath: readonly string[] | null;
  readonly effectiveIdentity: string | null;
  readonly transactionState: TransactionState;
  readonly autocommit: boolean | null;
  readonly confidence: ContextConfidence;
}

/** 附着状态。`expired` 表示附件令牌已过期，需重新 attach。 */
export type AttachmentState = 'attached' | 'detached' | 'expired';

/** 会话状态机。`lost` 表示物理资源丢失。 */
export type SessionState =
  | 'new'
  | 'opening'
  | 'ready'
  | 'executing'
  | 'reconfiguring'
  | 'closing'
  | 'closed'
  | 'lost';

/** 物理资源释放结论。`quarantined` 表示仍未确认释放，逻辑额度已回收但物理占用未归还。 */
export type ResourceRelease = 'pending' | 'confirmed' | 'quarantined';

/** 取消控制请求的结果分类。`unsupported` 是**正常取值**，不以异常表达。 */
export type CancelDisposition = 'requested' | 'unsupported' | 'alreadyFinished';

/** 客户端可见的会话全貌。 */
export interface SessionView {
  readonly handle: SessionHandle;
  readonly connectionId: Id;
  /** 配置版本。变更时该连接上所有会话被判失效（租约保持固定）。 */
  readonly configRevision: Counter;
  readonly owner: OwnerRef;
  readonly initialTarget: ExecutionTarget;
  readonly observedContext: SessionContext;
  /** 上下文版本。`executeInSession` 用它做 CAS 冲突判定。 */
  readonly contextRevision: Counter;
  readonly state: SessionState;
  readonly attachmentState: AttachmentState;
  /** 执行中指向唯一执行（一个 session 一次一个常规执行）。 */
  readonly activeExecutionId: Id | null;
  /** 单调 deadline。 */
  readonly expiresAt: Timestamp | null;
}

/** 打开会话回执。`attachmentToken` 仅 owner 持有。 */
export interface OpenSessionReceipt {
  readonly session: SessionView;
  readonly attachmentToken: string;
}

/**
 * 上下文变更回执。`replacedSessionId` 非空表示旧会话被替换：
 * 旧 owner 必须收到 `SessionLost`，失效旧句柄不得继续执行。
 */
export interface ContextChangeReceipt {
  readonly session: SessionView;
  readonly replacedSessionId: Id | null;
  /** 仅原 owner 重新附着时返回；原地切换为 `null`。 */
  readonly attachmentToken: string | null;
}

/** 关闭会话回执。`state='lost'` 时 `effectOutcome` 可能是 `unknown`。 */
export interface CloseReceipt {
  readonly dbSessionId: Id;
  readonly state: SessionState;
  readonly effectOutcome: EffectOutcome;
  readonly resourceRelease: ResourceRelease;
}

/**
 * 取消请求结果。`disposition='requested'` **不构成「写入已回滚」的证据**；
 * 业务是否已回滚只能看随后的 `ExecutionView.effectOutcome`。
 */
export interface CancelReceipt {
  readonly executionId: Id;
  readonly disposition: CancelDisposition;
  readonly state: ExecutionState;
}

/**
 * 是否满足「可以使用该 session 已确认的默认值」的前置条件：
 * 会话可用、上下文已确认、当前没有正在执行的常规执行。
 * 与 Rust `SessionView::session_defaults_usable()` 同构。
 */
export function sessionDefaultsUsable(session: SessionView): boolean {
  return (
    session.state === 'ready' &&
    session.observedContext.confidence === 'confirmed' &&
    session.activeExecutionId === null
  );
}

/** 会话是否还能接受新执行。 */
export function isSessionUsable(session: SessionView): boolean {
  return session.state === 'ready' || session.state === 'executing';
}

/** 两个句柄是否指向同一物理会话（epoch 不同即视为失效）。 */
export function isSameSession(left: SessionHandle, right: SessionHandle): boolean {
  return left.dbSessionId === right.dbSessionId && left.runtimeEpoch === right.runtimeEpoch;
}
