/**
 * 幂等镜像：`IdempotentOperation` / `SubmissionToken`。
 *
 * 与 `packages/platform-api/src/dto/idempotency.rs` 逐字对应（camelCase wire）。
 *
 * **为什么每个写操作都要先取令牌**：连接 §13.1 规定执行是非终态的——网络断了、
 * 页面关了、宿主重启了，前端无法从本地区分「请求没到」和「执行已经受理但回执丢了」。
 * 受理记录由宿主持久化且**不存活句柄**；重启后仍非终态的执行一律回 `OutcomeUnknown`，
 * 而不是猜一个结论。幂等键的作用就是让「重发」变成安全动作。
 *
 * **令牌不带任何秘密**：默认 24 小时有效，只含 nonce / operation / 组织 / 主体 / 客户端 /
 * 签发与过期时刻 / keyVersion。会话语义的操作还额外绑定 owner 的 runtimeEpoch。
 * `createProfile` 是配置写入，**不绑定**任何会话与纪元。
 */

import type { Timestamp } from './ids';

/** 幂等语义的操作种类。**新增取值等同协议版本升级**，前后端必须同时更新。 */
export type IdempotentOperation =
  | 'createProfile'
  | 'openSession'
  | 'executeInSession'
  | 'executeAtTarget'
  | 'setSessionContext'
  | 'startJob';

/**
 * 受理令牌。前端应在**构造请求之前**取一次，跨重试复用同一个 `idempotencyKey`；
 * 重新取一个键等于发起一次全新的操作。
 */
export interface SubmissionToken {
  readonly idempotencyKey: string;
  readonly expiresAt: Timestamp;
}

/** 是否为会话语义的操作（需要绑定 runtimeEpoch 的那一类）。 */
export function bindsRuntimeEpoch(operation: IdempotentOperation): boolean {
  return operation !== 'createProfile';
}

/** 是否需要 `sessionHandle`：`issueSubmissionToken` 只在会话语义操作上要求。 */
export function requiresSessionHandle(operation: IdempotentOperation): boolean {
  return bindsRuntimeEpoch(operation);
}
