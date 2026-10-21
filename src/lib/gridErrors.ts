/**
 * Machine-readable classification for `grid.*` error messages (00-contracts §8).
 *
 * Most `CommandError` branches reach the front end as a redacted string with no machine
 * code, so errors that need front-end branching start with `grid.<domain>.<reason>: `
 * followed by a human-readable explanation. `classifyGridError` only matches that leading
 * prefix; anything else is `unknown`, and the caller must render the raw message instead of
 * swallowing it (never a blank string, never a generic "unknown error").
 *
 * This file is the single skeleton owned by DB-01. Later tracks append their own domain by
 * adding union members and `GRID_ERROR_CODES` entries only — never by rewriting the file
 * (a rewrite silently drops another track's codes and stays green in `tsc`).
 */

/** Codes the front end can branch on. `unknown` means "no registered prefix matched". */
export type GridErrorCode =
  | 'unknown'
  | 'grid.selection.invalid'
  | 'grid.selection.outOfBounds'
  | 'grid.selection.stale'
  | 'grid.commit.stalePlan'
  | 'grid.commit.affectedMismatch'
  | 'grid.commit.conflictingIntents'
  | 'grid.cell.noRowIdentity'
  | 'grid.cell.ambiguousIdentity'
  | 'grid.cell.readOnly'
  | 'grid.insert.unknownColumn'
  | 'grid.insert.returningUnavailable';

/**
 * Prefix registry. Entries are full message prefixes (`grid.<domain>.<reason>`); the
 * classifier does longest-prefix matching, so appenders do not have to keep an order.
 *
 * Every entry here MUST have a real producer in the back end, otherwise the front end
 * branches on a prefix that can never arrive. See `docs/prd/data-browsing-design/00-contracts.md`
 * §8.2 for the producer inventory (`grid.selection.*` currently has none: out-of-bounds
 * coordinates are clamped to `null` instead of raising).
 */
export const GRID_ERROR_CODES: readonly GridErrorCode[] = [
  'grid.selection.invalid',
  'grid.selection.outOfBounds',
  'grid.selection.stale',
  'grid.commit.stalePlan',
  'grid.commit.affectedMismatch',
  'grid.commit.conflictingIntents',
  'grid.cell.noRowIdentity',
  'grid.cell.ambiguousIdentity',
  'grid.cell.readOnly',
  'grid.insert.unknownColumn',
  'grid.insert.returningUnavailable',
];

/** Every grid message starts with this namespace; useful for cheap pre-checks. */
export const GRID_ERROR_NAMESPACE = 'grid.';

/** Longest first: `grid.selection.outOfBounds` must win over any shorter registered prefix. */
const PREFIXES_LONGEST_FIRST: readonly GridErrorCode[] = [...GRID_ERROR_CODES].sort(
  (left, right) => right.length - left.length,
);

/**
 * Classify a message by its leading `grid.<domain>.<reason>` prefix. The separator after the
 * code is mandatory (`:`), so `grid.selection.invalidity: …` is not misread as
 * `grid.selection.invalid`. Matching is case-sensitive and anchored at the start: a mention
 * of the code inside otherwise ordinary prose is not a grid error.
 */
export function classifyGridError(message: string): GridErrorCode {
  const trimmed = typeof message === 'string' ? message.trim() : '';
  if (trimmed === '') return 'unknown';
  for (const code of PREFIXES_LONGEST_FIRST) {
    if (trimmed === code || trimmed.startsWith(`${code}:`)) return code;
  }
  return 'unknown';
}
