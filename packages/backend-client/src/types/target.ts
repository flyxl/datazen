/**
 * 目标镜像：`NamespaceTarget` / `ObjectTarget` / `ExecutionTarget` / `CanonicalTarget`。
 *
 * 与 `packages/platform-api/src/target.rs` 逐字对应（camelCase wire）。
 *
 * 规则要点（连接 §4.3 / §4.4）：
 * - `NamespaceTarget` 的四个字段**必须出现在请求里**；不存在的层级传 `null`，
 *   不能省略键；空字符串非法。
 * - `path` 是驱动命名空间 ID 列表，**不是文件路径**。
 * - `ObjectTarget` 必须传完整身份；不允许把未指定的 schema 猜成 `public` / `dbo`。
 * - 展示名不参与身份比较；身份只由 `namespaceFingerprint()` / `objectSignature()` 决定。
 */

import type { Id, JsonValue } from './ids';

/** 命名空间层级。wire 取值与 Rust `NamespaceLayer` 一致（小驼峰）。 */
export type NamespaceLayer = 'database' | 'catalog' | 'schema' | 'path';

/** 层级声明：驱动声明自己有哪些层级、哪些必填。 */
export type LayerRequirement = 'required' | 'optional' | 'forbidden';

/**
 * 命名空间目标。四个键都必须存在（可为 `null`），与 Rust `Option<String>` + `Vec<String>` 一致。
 * `path` 缺省为 `[]`。
 */
export interface NamespaceTarget {
  readonly database: string | null;
  readonly catalog: string | null;
  readonly schema: string | null;
  readonly path: readonly string[];
}

/** 对象身份。`signature`（table / view …）无法确定时为 `null`，不得猜。 */
export interface ObjectTarget {
  readonly kind: string;
  readonly name: string;
  readonly signature: string | null;
}

/** 请求提交的目标：`connectionId` + 命名空间 + 可选对象。 */
export interface ExecutionTarget {
  readonly connectionId: Id;
  readonly namespace: NamespaceTarget;
  readonly object: ObjectTarget | null;
}

/** 规范化后的命名空间 ID（驱动已归一，非展示名）。 */
export interface CanonicalNamespaceId {
  readonly value: string;
}

/** 驱动归一后的命名空间：存在的层才有值。 */
export interface CanonicalNamespace {
  readonly database: CanonicalNamespaceId | null;
  readonly catalog: CanonicalNamespaceId | null;
  readonly schema: CanonicalNamespaceId | null;
  readonly path: readonly CanonicalNamespaceId[];
}

/** 驱动归一后的最终目标。由宿主在执行前计算，前端只消费。 */
export interface CanonicalTarget {
  readonly connectionId: Id;
  readonly namespace: CanonicalNamespace;
  readonly object: ObjectTarget | null;
}

const EMPTY_NAMESPACE: NamespaceTarget = { database: null, catalog: null, schema: null, path: [] };

/** 构造空命名空间目标（可写版本，便于前端组装请求）。 */
export function emptyNamespaceTarget(): {
  -readonly [K in keyof NamespaceTarget]: NamespaceTarget[K];
} {
  return { database: null, catalog: null, schema: null, path: [] };
}

/** 读单个层级的值；`path` 是列表形态，单值读法不适用。 */
export function namespaceValue(
  target: NamespaceTarget,
  layer: Exclude<NamespaceLayer, 'path'>,
): string | null {
  return target[layer];
}

/** 该层级是否带了非 null 值；`path` 只要非空即视为带了值。 */
export function hasNamespaceValue(target: NamespaceTarget, layer: NamespaceLayer): boolean {
  return layer === 'path' ? target.path.length > 0 : target[layer] !== null;
}

/**
 * 稳定指纹：层次用 `/` 拼接，不存在的层级留空段。
 * 与 Rust `NamespaceTarget::namespace_fingerprint()` 同构，用于「同一目标」的比较，
 * **不是**权限凭据。
 */
export function namespaceFingerprint(target: NamespaceTarget): string {
  return [target.database ?? '', target.catalog ?? '', target.schema ?? '', ...target.path].join(
    '/',
  );
}

/**
 * 对象签名 `kind:name/signature`（signature 为 `null` 时是 `kind:name/`）。
 * 与 Rust `ObjectTarget::object_signature()` 同构：`new ObjectTarget('table','orders')`
 * 得到 `"table:orders/"`。
 */
export function objectSignature(target: ObjectTarget | null): string | null {
  if (target === null) return null;
  return `${target.kind}:${target.name}/${target.signature ?? ''}`;
}

/** 两个命名空间目标是否指向同一目标（只比身份，不比展示名）。 */
export function sameNamespace(left: NamespaceTarget, right: NamespaceTarget): boolean {
  return namespaceFingerprint(left) === namespaceFingerprint(right);
}

/** 请求侧的 DTO 校验要点：四个键必须存在且非空字符串、空 `path` 元素非法。 */
export function isWellFormedNamespace(value: unknown): value is NamespaceTarget {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as Record<string, JsonValue>;
  if (!Array.isArray(candidate.path)) return false;
  if (!candidate.path.every((part) => typeof part === 'string' && part.length > 0)) return false;
  for (const layer of ['database', 'catalog', 'schema'] as const) {
    if (!(layer in candidate)) return false;
    const layerValue = candidate[layer];
    if (layerValue !== null && (typeof layerValue !== 'string' || layerValue.length === 0))
      return false;
  }
  return true;
}

export { EMPTY_NAMESPACE };
