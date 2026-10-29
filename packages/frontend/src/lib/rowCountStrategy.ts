import type { CountStrategy } from '../types';
import type { RowCountMode } from '../types/settings';

/**
 * What a table read asks the host for.
 *
 * `undefined` means "no opinion": the request carries no strategy and the host
 * falls back to the driver's own declaration. That is the honest answer for
 * the `auto` mode when the size of the table is unknown — the driver knows
 * what it can do, and the host knows the safety rules (an estimate never
 * survives a filter, an estimate never outranks a declared "I cannot count").
 */
export type CountStrategyRequest = CountStrategy | undefined;

export interface CountStrategyInput {
  /** The user's `rowCountMode` setting. */
  mode: RowCountMode;
  /**
   * The row count above which the `auto` mode estimates. `<= 0` means
   * "always count" — a dirty setting must never make the grid lie.
   */
  threshold: number;
  /**
   * The row count the catalog already knows for this table, if any. It is a
   * *prior*, not a fact: it is what the `auto` mode weighs.
   */
  knownRowCount?: number | null;
}

/**
 * The strategy a read asks for, given the user's mode.
 *
 * The rules, one by one:
 *
 * - `none` → `'unsupported'`: the user asked not to count, so the read asks
 *   for no count at all.
 * - `exact` → `'exact'`: the user asked for the real number.
 * - `estimated` → `'estimated'`: the user asked for an estimate. The host
 *   still downgrades it to an exact count when the database has no estimate
 *   (an estimate that cannot be obtained is not an estimate).
 * - `auto` → the threshold decides, but only when the size is known: a table
 *   above the threshold is estimated, a table below it is counted. When the
 *   size is unknown the answer is `undefined` — the driver decides, which is
 *   what the mode means.
 */
export function resolveCountStrategy(input: CountStrategyInput): CountStrategyRequest {
  const { mode, threshold, knownRowCount } = input;
  if (mode === 'none') return 'unsupported';
  if (mode === 'exact') return 'exact';
  if (mode === 'estimated') return 'estimated';

  // `auto`
  if (knownRowCount === undefined || knownRowCount === null) return undefined;
  if (!Number.isFinite(knownRowCount)) return undefined;
  if (!Number.isFinite(threshold) || threshold <= 0) return 'exact';
  return knownRowCount > threshold ? 'estimated' : 'exact';
}
