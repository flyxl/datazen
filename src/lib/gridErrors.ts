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
  | 'grid.commit.stalePlan'
  | 'grid.commit.affectedMismatch'
  | 'grid.commit.conflictingIntents'
  | 'grid.cell.noRowIdentity'
  | 'grid.cell.ambiguousIdentity'
  | 'grid.cell.readOnly'
  | 'grid.cell.invalidValue'
  | 'grid.insert.unknownColumn'
  | 'grid.insert.returningUnavailable'
  | 'grid.fk.shapeMismatch'
  | 'grid.fk.unknownConstraint'
  | 'grid.fk.valueNull'
  | 'grid.fk.targetUnresolved'
  | 'grid.fk.probeFailed'
  | 'grid.filter.unsupportedOperator'
  | 'grid.filter.incomplete'
  | 'grid.view.invalid'
  | 'grid.view.nameExists'
  | 'grid.result.originUnavailable'
  | 'grid.result.originAmbiguous'
  | 'grid.result.rowNotAddressable'
  | 'grid.result.multiRowNotAllowed'
  | 'grid.result.writeDenied'
  | 'grid.paging.keysetUnavailable'
  | 'grid.paging.cursorInvalidated'
  | 'grid.paging.cursorTooDeep'
  // ── DB-03 (clipboard). Append only: adding a member here is the whole change. ──
  | 'grid.paste.empty'
  | 'grid.paste.tooLarge'
  | 'grid.paste.raggedRow'
  | 'grid.paste.unclosedQuote'
  | 'grid.paste.outOfBounds'
  | 'grid.paste.noWritableCell'
  | 'grid.paste.notNullEmpty'
  | 'grid.paste.identityChanged'
  | 'grid.paste.crossTable'
  | 'grid.paste.columnMismatch'
  | 'grid.clipboard.writeFailed'
  | 'grid.clipboard.readFailed';

/**
 * Prefix registry. Entries are full message prefixes (`grid.<domain>.<reason>`); the
 * classifier does longest-prefix matching, so appenders do not have to keep an order.
 *
 * Every entry here MUST be **reachable** by the front end, i.e. some producer must be able
 * to emit it — otherwise the front end branches on a prefix that can never arrive. See
 * `docs/prd/data-browsing-design/00-contracts.md` §8.2 for the producer inventory.
 *
 * "Reachable" is **not** the same as "produced by Rust". Two kinds of codes live here:
 *
 * * **Back-end codes** — raised by a Rust producer and carried in the message prefix
 *   (`grid.commit.*`, `grid.fk.*`, `grid.view.*`, …).
 * * **Front-end codes** — the rejection happens **in the browser** and never reaches the
 *   host at all: `grid.cell.invalidValue` (the typed editor refuses to commit an
 *   unparsable value) is the one today. The back end has no such rejection to raise, so
 *   there is no Rust literal for it — and `gridErrors.registration.test.ts` scans Rust
 *   sources for the codes it knows about. **Such a code must be listed in that test's
 *   `PRODUCER_PENDING`** (the list of codes the back end does not raise) or the guard
 *   fails.
 *
 * So `PRODUCER_PENDING` means **"this code is not produced by Rust"**, not "nobody
 * produces it yet". Do not invent a Rust producer for a front-end rejection just to
 * silence the guard — the back end has no such semantics to offer.
 *
 * The `grid.selection.*` domain **does not exist, deliberately**. Selection is client-side
 * state and its geometry **clamps** rather than fails: `clampCellCoord` pins a coordinate
 * into the grid, and "nowhere left to go" is reported as `null`. An out-of-bounds or stale
 * selection is therefore not an error condition — there is nothing to report, so nothing is
 * registered. Three such codes were once registered here waiting for a producer that could
 * never arrive; they were withdrawn rather than kept as branches nothing could reach.
 */
export const GRID_ERROR_CODES: readonly GridErrorCode[] = [
  'grid.commit.stalePlan',
  'grid.commit.affectedMismatch',
  'grid.commit.conflictingIntents',
  'grid.cell.noRowIdentity',
  'grid.cell.ambiguousIdentity',
  'grid.cell.readOnly',
  'grid.cell.invalidValue',
  'grid.insert.unknownColumn',
  'grid.insert.returningUnavailable',
  'grid.fk.shapeMismatch',
  'grid.fk.unknownConstraint',
  'grid.fk.valueNull',
  'grid.fk.targetUnresolved',
  'grid.fk.probeFailed',
  'grid.filter.unsupportedOperator',
  'grid.filter.incomplete',
  'grid.view.invalid',
  'grid.view.nameExists',
  'grid.result.originUnavailable',
  'grid.result.originAmbiguous',
  'grid.result.rowNotAddressable',
  'grid.result.multiRowNotAllowed',
  'grid.result.writeDenied',
  'grid.paging.keysetUnavailable',
  'grid.paging.cursorInvalidated',
  'grid.paging.cursorTooDeep',
  // DB-03: clipboard. Every one of these is a **front-end** rejection: the paste matrix is
  // parsed and planned in the browser and a refused cell never reaches the host, so there is
  // no Rust producer to wait for (see `PRODUCER_PENDING` in the registration test). They are
  // registered anyway, because the grid branches on them and they share the one code table.
  'grid.paste.empty',
  'grid.paste.tooLarge',
  'grid.paste.raggedRow',
  'grid.paste.unclosedQuote',
  'grid.paste.outOfBounds',
  'grid.paste.noWritableCell',
  'grid.paste.notNullEmpty',
  'grid.paste.identityChanged',
  'grid.paste.crossTable',
  'grid.paste.columnMismatch',
  'grid.clipboard.writeFailed',
  'grid.clipboard.readFailed',
];

/**
 * One structured problem of a paste plan, produced entirely in the browser.
 *
 * The code is **carried directly**, not parsed back out of a message: a paste problem is
 * structured data from the moment it is raised, while `classifyGridError` exists for
 * back-end messages that arrive as prose (00-contracts §8.1). Both use the same
 * `GridErrorCode` so there is still one code table.
 */
export interface PasteProblem {
  code: GridErrorCode;
  /** 0-based clipboard-matrix row; `null` for a whole-batch problem. */
  row: number | null;
  /** Column name; `null` when the problem is not about one column. */
  column: string | null;
  /**
   * Extra explanation (a column name, a count). **Never a cell value**: pasted content is
   * user data and must not leave the client (PRD §8.4).
   */
  detail?: string;
}

/** Every grid message starts with this namespace; useful for cheap pre-checks. */
export const GRID_ERROR_NAMESPACE = 'grid.';

/**
 * Longest first, so a code can never be shadowed by a shorter registered prefix.
 *
 * No registered code is a prefix of another **today**, so nothing depends on this order yet;
 * it is one sort at module load and it makes the classifier structurally correct if a pair
 * like `grid.paging.cursor` / `grid.paging.cursorTooDeep` is ever added. Keeping it is
 * cheaper than remembering to re-sort later.
 */
const PREFIXES_LONGEST_FIRST: readonly GridErrorCode[] = [...GRID_ERROR_CODES].sort(
  (left, right) => right.length - left.length,
);

/**
 * Classify a message by its leading `grid.<domain>.<reason>` prefix. The separator after the
 * code is mandatory (`:`), so `grid.cell.readOnlyBecause: …` is not misread as
 * `grid.cell.readOnly`. Matching is case-sensitive and anchored at the start: a mention
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
