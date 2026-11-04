/**
 * Named empty states of the key tree (PRD §4 I-11 / task book D-8).
 *
 * "No rows" is four different facts and the user has to be able to tell them
 * apart, because each one has a different next action:
 * - `none`          — the database really is empty.
 * - `no-match`      — the filter cut everything out; clearing it brings keys back.
 * - `interrupted`   — the scan stopped early, so the absence proves nothing.
 * - `no-permission` — `list_children` was rejected (ACL/NOAUTH); the tree cannot
 *                     say anything about the keyspace.
 *
 * The resolver returns a **key**, never copy: the component renders
 * `t(EMPTY_STATE_KEYS[state])`, so tests assert on the key and no English string
 * is baked into the module.
 */
export type TreeEmptyState = 'none' | 'no-match' | 'interrupted' | 'no-permission';

export const TREE_EMPTY_STATE_KEYS: Record<TreeEmptyState, string> = {
  none: 'redis.tree.empty.none',
  'no-match': 'redis.tree.empty.noMatch',
  interrupted: 'redis.tree.empty.interrupted',
  'no-permission': 'redis.tree.empty.noPermission',
};

export interface TreeEmptySignal {
  /** Rows currently rendered by the tree (folders + leaves). */
  rowCount: number;
  /** A level is being fetched right now. */
  loading: boolean;
  /** The root `list_children` failed — ACL / NOAUTH / unreachable. */
  rootError: boolean;
  /** Scan cursor of any loaded level is still open ⇒ the set is a subset. */
  scanning: boolean;
  /** R2 pattern typed by the user (`'*'` and blank mean "no pattern"). */
  pattern: string;
  keyType: string;
  noTtlOnly: boolean;
}

/** A pattern of `*` / blank is "match everything", i.e. no filter at all. */
export function isGlobalPattern(pattern: string): boolean {
  const trimmed = pattern.trim();
  return trimmed.length === 0 || trimmed === '*';
}

/** Anything narrower than the whole keyspace, from R2's input or chips. */
export function hasActiveTreeFilter(signal: TreeEmptySignal): boolean {
  return !isGlobalPattern(signal.pattern) || signal.keyType !== 'all' || signal.noTtlOnly;
}

/**
 * Which named empty state to show, or `null` when there is nothing to explain.
 *
 * Precedence is part of the contract and is asserted directly by the journey
 * test: a failed root outranks everything (we know nothing), an open cursor
 * outranks a filter (the filter cannot be blamed for a scan that never finished),
 * and only a finished scan is allowed to claim `none` / `no-match`.
 */
export function resolveTreeEmptyState(signal: TreeEmptySignal): TreeEmptyState | null {
  if (signal.rowCount > 0 || signal.loading) return null;
  if (signal.rootError) return 'no-permission';
  if (signal.scanning) return 'interrupted';
  return hasActiveTreeFilter(signal) ? 'no-match' : 'none';
}
