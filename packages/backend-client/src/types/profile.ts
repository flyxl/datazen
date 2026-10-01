/**
 * 连接配置镜像：`ProfileView` / `ProfileDraft` / `ProfilePatch`。
 *
 * 与 `packages/platform-api/src/dto/profile.rs` 逐字对应（camelCase wire）。
 *
 * **凭据纪律**：
 * - `ProfileDraft.credentials` 是**只写**字段。草稿一旦序列化出去即视为已提交，
 *   前端不得把它存进 store、日志或回显到任何视图。
 * - `ProfileView` **不返回** password、token、TLS 私钥或任何可直接解密材料，
 *   只有 `credentialConfigured` 这个布尔。
 * - 改凭据不需要也不允许走 `updateConnection` 的 `publicOptions`，凭据是独立字段。
 */

import type { Counter, Id, JsonObject } from './ids';
import type { NamespaceTarget } from './target';

/** 连接配置视图。不含 `secretRef` / `networkRouteRef` / 明文凭据。 */
export interface ProfileView {
  readonly connectionId: Id;
  readonly name: string;
  readonly driverId: string;
  /** 配置版本。变更时该连接上所有会话被判失效（租约仍保持固定）。 */
  readonly configRevision: Counter;
  /** 凭据修订号。与 `configRevision` 独立递增：改密码不动配置版本。 */
  readonly credentialRevision: Counter;
  readonly initialNamespace: NamespaceTarget;
  /** 可见且非敏感的配置项。 */
  readonly publicOptions: JsonObject;
  /** 只表示「是否已配置凭据」，不泄露是否存在、长度或内容。 */
  readonly credentialConfigured: boolean;
  readonly enabled: boolean;
}

/** 新建连接的配置草稿。`credentials` 是**只写**字段。 */
export interface ProfileDraft {
  readonly name: string;
  readonly driverId: string;
  readonly initialNamespace: NamespaceTarget;
  readonly publicOptions: JsonObject;
  /** 只写。序列化出去的草稿即视为已提交，不得再回写视图。 */
  readonly credentials: Readonly<Record<string, string>> | null;
}

/**
 * 配置补丁。**`driverId` 不可改**：driver 决定 `namespaceShape` 与能力，
 * 换 driver 等价于换一条配置，不做原地修改。
 * 与 Rust `ProfileDraftPatch` 同构（各字段可为 `null` 表示「不改」）。
 */
export interface ProfilePatch {
  readonly name: string | null;
  readonly initialNamespace: NamespaceTarget | null;
  readonly publicOptions: JsonObject | null;
  readonly credentials: Readonly<Record<string, string>> | null;
}

/** 组装一份新草稿（凭据默认 `null`，不写凭据是常见路径）。 */
export function profileDraft(
  name: string,
  driverId: string,
  initialNamespace: NamespaceTarget,
  publicOptions: JsonObject = {},
  credentials: Readonly<Record<string, string>> | null = null,
): ProfileDraft {
  return { name, driverId, initialNamespace, publicOptions, credentials };
}

/** 组装一份空补丁：`null` 表示该字段不变。 */
export function emptyProfilePatch(): ProfilePatch {
  return { name: null, initialNamespace: null, publicOptions: null, credentials: null };
}

/**
 * 补丁里是否带了敏感字段。凭据轮换只在为 `true` 时发生——
 * 改个显示名不该让已有凭据失效。
 */
export function touchesCredentials(patch: ProfilePatch): boolean {
  return patch.credentials !== null && Object.keys(patch.credentials).length > 0;
}

/**
 * 剥掉草稿里的凭据，用于把草稿转成可回显的部分（存草稿表、日志、错误上下文）。
 * 与 Rust `ProfileDraft::without_credentials()` 同构。
 */
export function withoutCredentials(draft: ProfileDraft): ProfileDraft {
  return { ...draft, credentials: null };
}

/** 草稿是否已填写凭据。 */
export function hasCredentials(draft: ProfileDraft): boolean {
  return draft.credentials !== null && Object.keys(draft.credentials).length > 0;
}

/**
 * 配置版本是否过期：请求带的 `expectedConfigRevision` 与当前视图不一致时，
 * 宿主回 `ConfigRevisionMismatch`（**不是** `TargetConflict`——命名空间目标本身合法，
 * 调用方要重读最新版本后由用户决定，不得自动覆盖）。
 */
export function isConfigRevisionStale(view: ProfileView, expected: Counter): boolean {
  return view.configRevision !== expected;
}
