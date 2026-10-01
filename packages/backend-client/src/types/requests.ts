/**
 * 请求镜像：`packages/application/src/dto/requests.rs` 的 TS 形态。
 *
 * **同一份契约的两个语言投影**。Rust 侧是权威定义（用例签名、字段校验、`thiserror`
 * 错误码都在那边），本文件只镜像 wire 形状，供前端构造请求。
 *
 * 关键不变量（前端必须遵守，否则宿主会拒）：
 * - `executeAtTarget` 的 `expectedConfigRevision` 是**必填 CAS**：配置已变就拒绝，
 *   不静默在新配置上执行。
 * - `setSessionContext` 的 `expectedContextRevision` 同样必填；冲突回 `ContextConflict`，
 *   客户端重读后**由用户**重新发送，不自动重试。
 * - `executeInSession` 的 `expectedContextRevision` 可选：普通编辑器执行应带上。
 * - 每个非会话读请求都带 `idempotencyKey`，重试复用同一个键。
 */

import type { Counter, Id, JsonValue } from './ids';
import type { NamespaceTarget, ExecutionTarget } from './target';
import type { OwnerRef } from './context';
import type { SessionHandle } from './session';
import type { IdempotentOperation } from './idempotency';

/** 幂等键（wire 为不透明字符串）。Rust 侧是 newtype 且 `Debug` 脱敏。 */
export type IdempotencyKey = string;

/** 统一网关 Command 调用。`command` 是**逻辑** Command 名（如 `query` / `describe`），不是物理命令串。 */
export interface CommandCall {
  readonly command: string;
  /** 透传的命令参数，由 driver Command schema 解释。 */
  readonly input: JsonValue;
}

/** 打开会话请求。`initialTarget` 必填：禁止把空库空 schema 当默认值。 */
export interface OpenSessionRequest {
  readonly initialTarget: ExecutionTarget;
  readonly owner: OwnerRef;
  readonly idempotencyKey: IdempotencyKey;
}

/** 在既有会话内执行。 */
export interface ExecuteInSessionRequest {
  readonly handle: SessionHandle;
  /** 可选上下文版本。普通编辑器执行应带上；显式 session 脚本内部可省略。 */
  readonly expectedContextRevision: Counter | null;
  readonly call: CommandCall;
  readonly idempotencyKey: IdempotencyKey;
}

/** 无会话一次性执行。缺层回 `TargetRequired`，绝不回退默认库。 */
export interface ExecuteAtTargetRequest {
  readonly target: ExecutionTarget;
  /** 必需 CAS。 */
  readonly expectedConfigRevision: Counter;
  readonly call: CommandCall;
  readonly idempotencyKey: IdempotencyKey;
}

/** 服务端核对上下文版本后切换，成功回 `ContextChangeReceipt`。 */
export interface SetSessionContextRequest {
  readonly handle: SessionHandle;
  /** 必需。缺失或不一致回 `ContextConflict`。 */
  readonly expectedContextRevision: Counter;
  readonly desired: NamespaceTarget;
  readonly idempotencyKey: IdempotencyKey;
}

/** 关闭模式。 */
export type CloseMode = 'requireNoTransaction' | 'rollbackAndClose';

/** 关闭会话请求。 */
export interface CloseSessionRequest {
  readonly handle: SessionHandle;
  readonly mode: CloseMode;
}

/**
 * 重新附着请求。页面刷新后用 `attachmentToken` 换回原 `dbSessionId`；
 * 令牌只发给原始 owner，因此它是不透明字符串。
 */
export interface AttachmentRequest {
  readonly handle: SessionHandle;
  readonly attachmentToken: string;
}

/** 订阅事件请求。`afterSequence` 为 `null` 表示从当前流头开始。 */
export interface SubscribeEventsRequest {
  readonly streamId: Id;
  readonly afterSequence: Counter | null;
}

/** 取受理令牌。`sessionHandle` 只在会话语义操作上必填（`createProfile` 传 `null`）。 */
export interface IssueSubmissionTokenRequest {
  readonly operation: IdempotentOperation;
  readonly sessionHandle: SessionHandle | null;
}

/** 读产物的两种寻址方式：按块序号，或按字节偏移 + 长度。 */
export interface ReadArtifactRequest {
  readonly artifactId: Id;
  readonly chunkIndex: Counter;
  /** 与 `chunkIndex` 二选一。 */
  readonly offset: Counter | null;
  /** 与 `chunkIndex` 二选一；按偏移读时必填。 */
  readonly limit: Counter | null;
}

/** 启动任务。幂等键来自 `issueSubmissionToken('startJob')`。 */
export interface StartJobRequest {
  readonly jobId: Id;
  readonly kind: string;
  readonly owner: OwnerRef;
  readonly payload: JsonValue;
  readonly idempotencyKey: IdempotencyKey;
}

/** 组装一次 Command 调用。 */
export function commandCall(command: string, input: JsonValue = null): CommandCall {
  return { command, input };
}
