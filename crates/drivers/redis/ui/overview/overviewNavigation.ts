/**
 * 屏 A → 屏 B jump model.
 *
 * PRD §3.0 lists 屏 A as a 选 db 入口（Key Space 格子 / 大 key 行 / 快捷动作 /
 * 最近浏览键），§3.1 asks those rows to "打开该 db 页签" / "跳到屏 B 并选中该键".
 * All of that is **host-owned state**: opening a database panel and selecting a
 * key live in the host panel store, and the frozen contract
 * `ConnectionHomeSlotProps` (packages/driver-sdk/src/types/kv-slots.ts) hands 屏 A
 * nothing but ids — there is no `bind*` / `useBound*` bridge in
 * `@datazen/driver-sdk` for it (only settings / connection / schema stores, the
 * confirm dialog and the context menu are bound).
 *
 * So every jump here is modelled as a *request* with an explicit outcome, and the
 * components never pretend a jump happened:
 *
 *   handled  ⇒ a host bridge was injected (`onOpenTarget`) and took the target.
 *   !handled ⇒ the block shows a named guidance hint (I-11) telling the user the
 *              entry that does work today (left navigation tree), and the
 *              affordance carries `data-overview-jump="unwired"` for E2E.
 *
 * Wiring the bridge is a host-side follow-up; this module never touches the host
 * and never reaches outside the driver UI.
 */

/** Where a 屏 A affordance wants to land. */
export type OverviewJumpTarget =
  | { kind: 'database'; dbIndex: number }
  /**
   * Jump to 屏 B selecting `key`. `keyType` is best-effort: every 屏 A entry that
   * already knows the type (big-key row, recent-key chip) forwards it so the host
   * can pre-colour the selection, but a target without it is still a valid jump.
   */
  | { kind: 'key'; dbIndex: number; key: string; keyType?: string | null }
  | { kind: 'console' }
  | { kind: 'pubsub' }
  | { kind: 'monitor'; section: 'info' | 'memory' | 'slowlog' }
  | { kind: 'importExport' }
  | { kind: 'newKey'; dbIndex: number };

/**
 * One-shot action the host wants the Redis panel to perform on mount after
 * the overview page (屏 A) jumps to 屏 B.  The panel consumes it exactly once
 * and clears it so repeated renders never replay it.
 */
export interface RedisPendingAction {
  /** Tab to switch to after the panel opens. */
  tab?: 'items' | 'console' | 'pubsub' | 'monitor';
  /** Sub-page inside the monitor tab. */
  monitorSubPage?: 'info' | 'monitor' | 'memory' | 'slowlog' | 'streams';
  /** Key to select + open in the editor (屏 A → 屏 B key jump). */
  selectKey?: string;
  /** Database the key lives in (for key jumps that target a different db). */
  keyDbIndex?: number;
  /** Open the import/export dialog (quick action). */
  openImportExport?: boolean;
  /** Open the create-new-key dialog (quick action). */
  openNewKey?: boolean;
}

/** Host-supplied jump bridge (absent until the follow-up track lands). */
export type OverviewJumpHandler = (target: OverviewJumpTarget) => void;

export interface OverviewJumpOutcome {
  target: OverviewJumpTarget;
  /** `true` ⇒ the host bridge accepted the request and 屏 B took over. */
  handled: boolean;
  /**
   * i18n key of the guidance to surface when `handled === false`
   * (`redis.overview.jump.pendingTree` — 左树点库是当前唯一有效入口;
   * `redis.overview.jump.pendingPanel` — 面板内页签需要面板先存在).
   */
  hintKey: string | null;
}

/** Targets that need a database panel ⇒ the left navigation tree is today's entry. */
const TREE_ENTRY_TARGETS = new Set(['database', 'key', 'newKey']);

/** Pure classification of a jump request against an optional bridge. */
export function planOverviewJump(
  target: OverviewJumpTarget,
  handler: OverviewJumpHandler | undefined,
): OverviewJumpOutcome {
  if (typeof handler === 'function') {
    return { target, handled: true, hintKey: null };
  }
  return {
    target,
    handled: false,
    hintKey: TREE_ENTRY_TARGETS.has(target.kind)
      ? 'redis.overview.jump.pendingTree'
      : 'redis.overview.jump.pendingPanel',
  };
}

/**
 * Execute a jump request: call the bridge when present, then report the outcome
 * so the caller can render the fallback hint. Never throws — a host bridge that
 * misbehaves must not take 屏 A down with it.
 */
export function requestOverviewJump(
  target: OverviewJumpTarget,
  handler: OverviewJumpHandler | undefined,
): OverviewJumpOutcome {
  const plan = planOverviewJump(target, handler);
  if (!plan.handled || !handler) return plan;
  try {
    handler(target);
  } catch {
    return { ...plan, handled: false, hintKey: 'redis.overview.jump.failed' };
  }
  return plan;
}

/** Stable `data-overview-jump` value for an affordance (drives E2E + the audit). */
export function jumpStateAttribute(handler: OverviewJumpHandler | undefined): 'wired' | 'unwired' {
  return typeof handler === 'function' ? 'wired' : 'unwired';
}

/** Colour token a Redis TYPE maps to on a 屏 A type badge. */
export type RedisTypeTone = 'accent' | 'success' | 'warning' | 'danger' | 'neutral';

/** Redis TYPE 令牌 → 徽标 tone（未知类型退化成 neutral，不编造颜色）。 */
export function typeTone(keyType: string | null | undefined): RedisTypeTone {
  switch ((keyType ?? '').toLowerCase()) {
    case 'string':
      return 'accent';
    case 'hash':
      return 'success';
    case 'list':
      return 'warning';
    case 'set':
    case 'zset':
      return 'danger';
    case 'stream':
      return 'accent';
    default:
      return 'neutral';
  }
}

/**
 * Tailwind classes for an outlined type pill in `typeTone`'s vocabulary. Kept in
 * the model file (not a component) so both 屏 A rows that show a type — the
 * big-key row and the recent-key chip — render identical colours, mirroring
 * `KeyTreeList`'s outlined-pill styling without importing a component here.
 */
const TYPE_TONE_CLASS: Record<RedisTypeTone, string> = {
  accent: 'border-accent/40 text-accent',
  success: 'border-success/40 text-success',
  warning: 'border-warning/40 text-warning',
  danger: 'border-danger/40 text-danger',
  neutral: 'border-edge text-fg-muted',
};

export function typeBadgeClass(keyType: string | null | undefined): string {
  return TYPE_TONE_CLASS[typeTone(keyType)];
}

/** 解析 `db7` / `7` / `db0` 形式的逻辑库标识（与 RedisConnectionView 同规则）。 */
export function parseDbIndex(database: string | number | null | undefined): number {
  if (typeof database === 'number' && Number.isFinite(database)) {
    return Math.max(0, Math.floor(database));
  }
  const value = String(database ?? '')
    .trim()
    .toLowerCase();
  const match = /^(?:db)?(\d+)$/.exec(value);
  return match ? Number(match[1]) : 0;
}
