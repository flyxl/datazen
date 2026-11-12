/**
 * columnOverrides — 表列覆盖持久化（issue-32 T10，设计 §5/§6，PRD §5/§6 验收 6）。
 *
 * 存储形状（version 1 + 可选字段，不升版；理由同 T08 rules：加法性
 * 可选字段，升版会迫使无迁移的 v2 走「未知版本 → 空快照」分支，
 * 反而丢配置）：
 *
 * ```ts
 * StoredValuePresentationSettings['columnOverrides']
 *   = Record<columnOverrideKey, ColumnPresentationOverride>
 * ```
 *
 * - 键：`columnOverrideKey`（presentationPlan.ts）=
 *   JSON.stringify([connectionId, database ?? null, namespace ?? null,
 *   table, column])。键本身就是来源身份：当前用户/组织作用域由存储
 *   层负责（Desktop 复用设置仓储、Web 复用用户级设置用例），不相信
 *   客户端传入的任意用户 ID；键里不含面板 ID、会话 ID、dbSessionId，
 *   连接身份只用稳定的 ConnectionConfig.id（connectionId）。
 * - 大小写：不归一，保留驱动返回的标识符大小写语义；缺省字段存
 *   null、空字符串存 ''，二者键不同（missing vs empty 不碰撞，
 *   T04 BUG-004 同类教训）。
 * - 值：只存 `raw` / `custom`；`inherit` 即无条目——解析时丢弃
 *   inherit 条目，保存 inherit 即删除本列条目（删除后恢复规则，
 *   PRD 验收 4）。
 * - 数量上限：MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES（200 条，
 *   存储体积护栏；规则上限 20 条的同类设限，先到先得、超限拒绝
 *   保存，不静默截断已存配置）。
 * - 未知顶层版本：不解析、不写覆盖字段（T06/T08 规则；保存仅
 *   version 1 写覆盖字段，v99 等未知版本返回 `unknown-version`，
 *   原文保留在 store）。
 *
 * 保存链路（`saveColumnOverride`，面板默认保存路径）：
 *
 * 1. 无来源（查询结果列）→ `no-source`，不写任何存储（面板保存
 *    按钮本身已禁用，此为防御性复核，T09 行为不变）；
 * 2. 覆盖非法 → `invalid-override`，不写存储；
 * 3. 版本非 1 → `unknown-version`，不写存储、不写会话；
 * 4. 乐观写会话（`setSessionColumnOverride`）：持久化失败时会话
 *    条目保留，用户刚改的配置不丢；`inherit` 在会话层即删除条目，
 *    失败时无条目可保留，但面板草稿仍在、失败可重试；
 * 5. 经 `persist`（即 `updateValuePresentationSettings`）写持久化；
 *    失败 → `save-failed`，旧配置继续生效（快照仍是旧值）、会话
 *    条目保留、面板保持打开并就地报错；
 * 6. 成功后清除同一键的会话条目（`clearSessionColumnOverride`），
 *    以免会话值遮住已持久化的值；之后快照即持久值。
 *
 * 优先级（T04/T08 已有链，T10 只填快照、不改计划层）：覆盖 >
 * 规则 > 全局默认（`buildColumnPresentationPlans` 先查
 * snapshot.overrides，见 presentationPlan.ts）。
 *
 * 纯逻辑 + 会话存储调用；不读 Store / IPC / DOM。`persist` 由调
 * 用方注入（面板传 settingsStore 的 updateValuePresentationSettings），
 * 便于测试注入 mock。
 */

import type { ColumnPresentationOverride, ColumnSource } from '@datazen/extension-points';
import { validateColumnPresentationOverride } from './columnPresentation';
import { clearSessionColumnOverride, setSessionColumnOverride } from './columnPresentationSession';
import { columnOverrideKey } from './presentationPlan';
import type { StoredValuePresentationSettings } from './settings';

/** 表列覆盖条数上限（存储体积护栏；超限拒绝保存，不静默截断）。 */
export const MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES = 200;

/**
 * 覆盖键的形状校验：必须是 JSON 数组
 * [connectionId, database | null, namespace | null, table, column]，
 * 其中 connectionId / table / column 为字符串，database / namespace
 * 为字符串或 null。非此形状的键丢弃——键是来源身份，不能容忍
 * 拼写错误的键在重启后命中错误的列。
 */
function isValidColumnOverrideKey(key: string): boolean {
  let parsed: unknown;
  try {
    parsed = JSON.parse(key);
  } catch {
    return false;
  }
  if (!Array.isArray(parsed) || parsed.length !== 5) return false;
  const connectionId: unknown = parsed[0];
  const database: unknown = parsed[1];
  const namespace: unknown = parsed[2];
  const table: unknown = parsed[3];
  const column: unknown = parsed[4];
  return (
    typeof connectionId === 'string' &&
    (typeof database === 'string' || database === null) &&
    (typeof namespace === 'string' || namespace === null) &&
    typeof table === 'string' &&
    typeof column === 'string'
  );
}

/**
 * 宽容解析存储的覆盖记录（存储边界用）：非对象 → {}；逐条校验键形
 * 状与覆盖值，无效键 / 无效覆盖 / `inherit` 条目丢弃（原文保留在
 * store，不写回）；超过上限只保留前 MAX 条有效条目。
 */
export function validateStoredColumnOverrides(
  input: unknown,
): Readonly<Record<string, ColumnPresentationOverride>> {
  if (typeof input !== 'object' || input === null || Array.isArray(input)) {
    return {};
  }
  const overrides: Record<string, ColumnPresentationOverride> = {};
  for (const [key, entry] of Object.entries(input)) {
    if (Object.keys(overrides).length >= MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES) break;
    if (!isValidColumnOverrideKey(key)) continue;
    const validated = validateColumnPresentationOverride(entry);
    if (!validated.ok) continue;
    // `inherit` 即无条目：不存（存了也只会在快照里被计划层当作
    // 无覆盖处理，还会在重启后误导“本列有覆盖”的判断）。
    if (validated.override.mode === 'inherit') continue;
    overrides[key] = validated.override;
  }
  return overrides;
}

/**
 * 纯函数：把一列覆盖应用到存储值上，返回下一份存储值（原值不动）。
 *
 * - 版本非 1 → 原样返回传入存储值（不写覆盖字段，见文件头）；
 * - `inherit` → 删除本列条目（无条目时返回原值本身，无变化）；
 * - `raw` / `custom` → 按键写入（非法覆盖返回原值；写入后超限
 *   返回原值——超限由 saveColumnOverride 报 `too-many-overrides`）；
 * - 与原值 JSON 相等时返回原值本身（不凭空产生一次保存写操作）。
 */
export function withColumnOverride(
  stored: Readonly<StoredValuePresentationSettings>,
  source: Readonly<Omit<ColumnSource, 'column'>>,
  columnName: string,
  override: Readonly<ColumnPresentationOverride>,
): StoredValuePresentationSettings {
  if (stored.version !== 1) return stored;
  const key = columnOverrideKey({ ...source, column: columnName });
  const current = stored.columnOverrides ?? {};
  if (override.mode === 'inherit') {
    if (!(key in current)) return stored;
    const nextOverrides = { ...current };
    delete nextOverrides[key];
    return { ...stored, columnOverrides: nextOverrides };
  }
  const validated = validateColumnPresentationOverride(override);
  if (!validated.ok) return stored;
  if (key in current && JSON.stringify(current[key]) === JSON.stringify(validated.override)) {
    return stored;
  }
  if (!(key in current) && Object.keys(current).length >= MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES) {
    return stored;
  }
  return { ...stored, columnOverrides: { ...current, [key]: validated.override } };
}

/** saveColumnOverride 的输入：`persist` 由调用方注入保存链路。 */
export interface SaveColumnOverrideInput {
  readonly stored: Readonly<StoredValuePresentationSettings>;
  /** 表身份；缺省表示查询结果等无来源列——保存禁用（PRD §3.2）。 */
  readonly source: Readonly<Omit<ColumnSource, 'column'>> | undefined;
  readonly columnName: string;
  readonly override: Readonly<ColumnPresentationOverride>;
  readonly persist: (next: StoredValuePresentationSettings) => Promise<void>;
}

/** saveColumnOverride 的结果：成功或失败原因（调用方据此就地报错）。 */
export type SaveColumnOverrideResult =
  | { readonly ok: true }
  | {
      readonly ok: false;
      readonly reason:
        | 'no-source'
        | 'invalid-override'
        | 'unknown-version'
        | 'too-many-overrides'
        | 'save-failed';
    };

/**
 * 保存一列覆盖：乐观写会话 → 经 `persist` 写持久化 → 成功后清除
 * 同一键的会话条目。失败时返回原因，不抛异常；失败路径见文件头
 * （`save-failed` 时会话条目保留、旧配置继续生效）。
 */
export async function saveColumnOverride(
  input: Readonly<SaveColumnOverrideInput>,
): Promise<SaveColumnOverrideResult> {
  const { stored, source, columnName, override, persist } = input;
  if (source === undefined) return { ok: false, reason: 'no-source' };
  const validated = validateColumnPresentationOverride(override);
  if (!validated.ok) return { ok: false, reason: 'invalid-override' };
  if (stored.version !== 1) return { ok: false, reason: 'unknown-version' };
  const key = columnOverrideKey({ ...source, column: columnName });
  const current = stored.columnOverrides ?? {};
  if (
    validated.override.mode !== 'inherit' &&
    !(key in current) &&
    Object.keys(current).length >= MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES
  ) {
    return { ok: false, reason: 'too-many-overrides' };
  }
  // 乐观写会话：写在 persist 之前（同步执行），持久化失败时本条
  // 目仍在，面板报错后用户刚改的配置不丢。
  setSessionColumnOverride(source, columnName, validated.override);
  const next = withColumnOverride(stored, source, columnName, validated.override);
  if (next === stored) {
    // 无变化（如与已存值相同、删除不存在的条目）：不写持久化，但
    // 仍须清除同一键的会话条目——上面已乐观写会话，custom/raw 与
    // 已存值相同时残留条目会遮住持久值、抬高合并快照 revision
    //（T10-BUG-001）；inherit-删不存在条目时清除为空操作。
    clearSessionColumnOverride(source, columnName);
    return { ok: true };
  }
  try {
    await persist(next);
  } catch {
    return { ok: false, reason: 'save-failed' };
  }
  // 成功后清除同一键的会话条目，以免会话值遮住已持久化的值。
  clearSessionColumnOverride(source, columnName);
  return { ok: true };
}
