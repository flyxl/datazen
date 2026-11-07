/**
 * The front-end half of the result grid's write-back verdict.
 *
 * The **table-level** verdict — "can this result be written back at all?" —
 * is computed by the back end, because only the driver knows where the rows
 * came from. The front end never recomputes it: a second implementation would
 * drift, and the drift shows up as "the grid says you may edit, the write
 * always fails".
 *
 * What the front end *can* judge is the **page-level** truth: whether the row
 * the user clicked can be located inside the page that is on screen. A NULL
 * key value cannot address a row, and two rows with the same identity cannot
 * be told apart. Both are decided from the data that is already loaded, with
 * no SQL involved.
 *
 * The two verdicts are combined with AND: either side read-only means the
 * cell is read-only.
 */

import type {
  GridReadOnlyReason as ServerGridReadOnlyReason,
  IdentityColumn,
  ResultTableRef,
} from '@datazen/backend-client';
import { valuesEqual } from './tableChanges';

export type { IdentityColumn, ResultTableRef };

/**
 * Why a result cannot be written back.
 *
 * The first eleven come from the back end and mirror its reasons one for one;
 * the last ones are page-level facts only the front end can see.
 */
export type GridReadOnlyReason =
  | ServerGridReadOnlyReason
  /** Page-level: the key value of this row is NULL, so it cannot be located. */
  | 'keyValueNull'
  /** Page-level: another row on this page has the same identity. */
  | 'duplicateIdentity'
  /** Page-level: the row is not part of the page the verdict was made for. */
  | 'rowNotFound';

/** The verdict the back end sent for one result. */
export interface ServerEditability {
  writable: boolean;
  identityColumns: IdentityColumn[];
  editableColumns: IdentityColumn[];
  table: ResultTableRef | null;
  reason: GridReadOnlyReason | null;
}

/** The final verdict for one cell. */
export type GridEditability =
  | {
      writable: true;
      identityColumns: IdentityColumn[];
      editableColumns: IdentityColumn[];
      /**
       * The relation the rows come from — `null` when the verdict that
       * granted the write-back did not name one (the narrow mapping verdict
       * does not).
       */
      table: ResultTableRef | null;
    }
  | { writable: false; reason: GridReadOnlyReason };

/** The verdict for a result that is read-only, with its reason. */
function readOnly(reason: GridReadOnlyReason): GridEditability {
  return { writable: false, reason };
}

/**
 * Whether one row of the page can be addressed at all.
 *
 * Two facts, both pure data, both independent of **which channel** granted
 * the write-back:
 *
 * * the locating key value must not be NULL — a NULL cannot take part in an
 *   equality comparison, so the row cannot be located by it (PRD §1.3 §6);
 * * no other row on the page may carry the same identity — two rows with the
 *   same identity cannot be told apart, so neither of them may be written
 *   back (PRD §1.3 §7).
 *
 * The locating key columns come from the **catalog** (the host's normalised
 * key list), not from the origin verdict — that is the only way these facts
 * are computable on the narrow channel, where the driver cannot describe the
 * result at all.
 *
 * **What §1.3 §7 guarantees**: that the row is addressed **uniquely within
 * this page**. The uniqueness across pages (and across the whole table) is
 * the database's primary-key constraint, not this check; the last line of
 * defence for a write is the statement's `WHERE` carrying nothing but the
 * primary-key predicate plus the `affected == 1` guard.
 *
 * Returns the reason the row cannot be addressed, or `null` when it can.
 */
export function resolveRowAddressability(
  identityColumns: ReadonlyArray<IdentityColumn>,
  rows: ReadonlyArray<ReadonlyArray<unknown>>,
  columns: ReadonlyArray<{ name: string }>,
  rowIndex: number,
): GridReadOnlyReason | null {
  if (rowIndex < 0 || rowIndex >= rows.length) return 'rowNotFound';
  if (identityColumns.length === 0) {
    // No locating key column is known, so the facts cannot be computed. That
    // is **not** a refusal: the narrow write path re-verifies the identity
    // against the live catalog before it writes, so an unreadable catalog
    // must not turn a writable result into a grey one.
    return null;
  }
  const row = rows[rowIndex];
  const values: unknown[] = [];
  for (const identity of identityColumns) {
    const index = columns.findIndex((column) => column.name === identity.column);
    if (index < 0) return 'rowNotFound';
    const value = row[index];
    if (value === null || value === undefined) return 'keyValueNull';
    values.push(value);
  }
  for (let other = 0; other < rows.length; other += 1) {
    if (other === rowIndex) continue;
    const candidate = rows[other];
    const same = identityColumns.every((identity, position) => {
      const index = columns.findIndex((column) => column.name === identity.column);
      if (index < 0) return false;
      return valuesEqual(values[position] as never, candidate[index] as never);
    });
    if (same) return 'duplicateIdentity';
  }
  return null;
}

/**
 * Merge the back end's verdict with the page's own facts.
 *
 * `rows` are the rows of the page as the result reports them (positional
 * arrays, one entry per column) and `columns` their names in the same order.
 */
export function resolveGridEditability(
  server: ServerEditability,
  rows: ReadonlyArray<ReadonlyArray<unknown>>,
  columns: ReadonlyArray<{ name: string }>,
  rowIndex: number,
): GridEditability {
  if (!server.writable) {
    // The back end's reason is the answer; the page cannot make a read-only
    // result writable, and it must not invent a different reason for it.
    return readOnly(server.reason ?? 'unknownOrigin');
  }
  const rowReason = resolveRowAddressability(
    server.identityColumns ?? [],
    rows,
    columns,
    rowIndex,
  );
  if (rowReason) return readOnly(rowReason);
  return {
    writable: true,
    identityColumns: server.identityColumns ?? [],
    editableColumns: server.editableColumns ?? [],
    table: server.table,
  };
}

/**
 * Whether the **origin** verdict may grant write-back.
 *
 * The origin verdict (the driver's own answer about where the rows came from)
 * is the only way a result the narrow host-side mapping check cannot see —
 * `SELECT id, total FROM t`, an aliased key, a projected column — may ever be
 * written back. That is worth having, but only once something can carry the
 * write: today the only write path is the narrow one
 * (`commitQueryResultCell`), and it re-verifies the mapping with the narrow
 * check — which refuses exactly the statements the origin verdict would have
 * allowed. With the release channel open, the grid would claim a cell is
 * editable, the narrow write path would refuse, and the refusal would be
 * **silent**: the cell would show the new value while the row in the database
 * kept the old one. That is the "edited, then quietly lost" state DB-14
 * exists to kill, and it is worse than a grey cell.
 *
 * So the channel is a **named switch with one meaning**: it may only be
 * turned on when **both** halves exist — a driver that overrides
 * `describe_result_origin` (so the origin verdict can say something at all),
 * **and** a front end that writes through the new pipeline
 * (`previewResultCellChange` → `commitPendingChanges`), so a granted write
 * actually reaches the database. Until both are true it must stay `false`,
 * and the test that pins it is the proof that the release arm is unreachable.
 */
export const RESULT_ORIGIN_WRITE_BACK_ENABLED: boolean = false;

/**
 * Whether the **origin channel** granted the write-back for this result.
 *
 * The origin channel is the one that knows which result columns are straight
 * copies of a base column; the narrow channel knows only the relation. A
 * column the origin channel does not list therefore has no source to write
 * back to, and the back end refuses it with `grid.result.writeDenied` — so
 * the interface must not offer it.
 *
 * The channel is closed today (see
 * {@link RESULT_ORIGIN_WRITE_BACK_ENABLED}), which is why the guard is inert
 * today and correct the moment the channel opens.
 */
export function originChannelWritable(server: ServerEditability): boolean {
  return RESULT_ORIGIN_WRITE_BACK_ENABLED && server.writable;
}

/**
 * Whether the result as a whole may be written back.
 *
 * Two verdicts answer that, and either one is enough:
 *
 * * the **narrow mapping verdict** (`writableMapping === 'verified'`) is the
 *   host-side check that shipped first; the write path re-verifies it against
 *   the live catalog before it writes, so it is not withdrawn here;
 * * the **origin verdict** is the driver's own answer, and it may only grant
 *   write-back while the release channel is open (see
 *   {@link RESULT_ORIGIN_WRITE_BACK_ENABLED}).
 *
 * Neither verdict is recomputed here — both are answers the back end gave.
 *
 * **One refusal outranks both**: a read-only connection. The narrow verdict
 * is about the *mapping* — "this result is a straight copy of that relation"
 * — and says nothing about whether the connection may be written to at all.
 * So when the back end reports `sessionReadOnly`, the cell is read-only
 * whichever channel would have opened it. Every other refusal keeps the
 * current rule: the origin verdict may only refuse while the release channel
 * is closed, and a driver that cannot describe the result must not close the
 * grid (that would turn "I cannot tell" into "no").
 */
export function resolveResultWriteBack(
  legacyVerified: boolean,
  server: ServerEditability,
): boolean {
  if (!server.writable && server.reason === 'sessionReadOnly') return false;
  if (legacyVerified) return true;
  return originChannelWritable(server);
}

/**
 * The final verdict for one cell, legacy mapping verdict included.
 *
 * The page-level facts (a NULL key value, two rows with the same identity)
 * only ever *refuse*; they never make a refused result writable.
 */
export function resolveResultEditability(
  legacyVerified: boolean,
  server: ServerEditability,
  rows: ReadonlyArray<ReadonlyArray<unknown>>,
  columns: ReadonlyArray<{ name: string }>,
  rowIndex: number,
): GridEditability {
  // The page-level facts are computed **first** and independently of the
  // channel: a row whose locating key is NULL, or whose identity is not
  // unique on the page, cannot be addressed — whichever channel opened the
  // result. They are computed even when the result is refused, so they are
  // never structurally unreachable.
  const rowReason = resolveRowAddressability(
    server.identityColumns ?? [],
    rows,
    columns,
    rowIndex,
  );
  if (resolveResultWriteBack(legacyVerified, server)) {
    if (rowReason) return readOnly(rowReason);
    return {
      writable: true,
      identityColumns: server.identityColumns ?? [],
      editableColumns: server.editableColumns ?? [],
      table: server.table,
    };
  }
  // The release channel is closed, so the origin verdict may only **refuse**.
  // A refusal keeps the back end's reason: the page never invents one, and
  // the page-level facts never turn a refusal into a grant.
  return readOnly(server.reason ?? 'unknownOrigin');
}

/**
 * The i18n key that explains a read-only verdict.
 *
 * The key is the whole answer: the front end never builds the sentence, so a
 * missing translation cannot turn into a half-translated message.
 */
export function gridReadOnlyMessageKey(reason: GridReadOnlyReason): string {
  return `query.gridReadOnly.${reason}`;
}

/**
 * Whether one result column may be edited at all.
 *
 * A column that is not a straight copy of a base column has no source to
 * write back to, so it is never editable — whatever the result-level verdict
 * says.
 */
export function isColumnEditable(
  server: ServerEditability,
  columnName: string,
): boolean {
  if (!server.writable) return false;
  return (server.editableColumns ?? []).some((column) => column.column === columnName);
}
