/**
 * 身份上下文镜像：`RequestContext` / `OwnerRef` / `DelegationRef`。
 *
 * 与 `packages/platform-api/src/context.rs` 逐字对应（camelCase wire）。
 *
 * **前端只读**：概要 §6.1 与 CM INV-01 规定身份只能由 adapter 构造，任何请求体字段都
 * 不得覆盖身份。因此本文件只提供**类型镜像**与只读判定，不提供从请求体填充的构造器，
 * 也不提供 `Default`。前端要把这些字段展示出来，只能读宿主已经注入的值。
 */

import type { Id, Timestamp } from './ids';

/**
 * 单次调用的身份上下文。六字段与概要 §6.1 一一对应，顺序即字段顺序。
 *
 * `clientInstanceId` **不是授权依据**：它只用于定位 owner 与事件流归属。
 */
export interface RequestContext {
  readonly organizationId: Id;
  readonly principalId: Id;
  /** 未认证上下文为 `null`；端口据此判断能否落到用户态资源。 */
  readonly authenticationSessionId: Id | null;
  readonly clientInstanceId: Id;
  /** 落进 `ApiError.requestId` 与事件，供前端上报对账。 */
  readonly requestId: Id;
  /** 委托执行时非空；作用域与有效期由策略端口判定。 */
  readonly delegationId: Id | null;
}

/**
 * 归属意图（wire 上是 `kind` 内部标签的 camelCase 判别联合）。
 *
 * 后端必须确认：editor 属于当前 client、job/block 属于已授权 Job，
 * **不能允许用户声称任意 job owner**。
 */
export type OwnerRef =
  | { readonly kind: 'editor'; readonly clientInstanceId: Id; readonly editorSessionId: Id }
  | { readonly kind: 'job'; readonly jobId: Id; readonly stageId: Id }
  | { readonly kind: 'workflowBlock'; readonly jobId: Id; readonly blockId: Id }
  | { readonly kind: 'clientSession'; readonly clientInstanceId: Id; readonly purpose: string };

/** 委托凭据引用。作用域与有效期的匹配由宿主策略端口判定。 */
export interface DelegationRef {
  readonly delegationId: Id;
  readonly organizationId: Id;
  /** 被委托主体。 */
  readonly principalId: Id;
  readonly clientInstanceId: Id;
  /** 授权作用域标识。 */
  readonly scope: string;
  readonly expiresAt: Timestamp;
}

/** 是否处于委托执行上下文。 */
export function isDelegated(context: RequestContext): boolean {
  return context.delegationId !== null;
}

/**
 * 归属是否绑定在某个客户端实例上（editor / clientSession）。
 * 对应概要 §5.1(3)：这类 owner 必须属于**当前** client。
 */
export function isClientBoundOwner(owner: OwnerRef): boolean {
  return owner.kind === 'editor' || owner.kind === 'clientSession';
}

/** 归属是否指向一个 Job（job / workflowBlock）。对应 §5.1(3) 的另一半。 */
export function ownerJobId(owner: OwnerRef): Id | null {
  return owner.kind === 'job' || owner.kind === 'workflowBlock' ? owner.jobId : null;
}
