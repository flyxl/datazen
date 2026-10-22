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
      table: ResultTableRef;
    }
  | { writable: false; reason: GridReadOnlyReason };

/** The verdict for a result that is read-only, with its reason. */
function readOnly(reason: GridReadOnlyReason): GridEditability {
  return { writable: false, reason };
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
  if (rowIndex < 0 || rowIndex >= rows.length) {
    return readOnly('rowNotFound');
  }
  const identityColumns = server.identityColumns ?? [];
  if (identityColumns.length === 0) {
    // A writable result without a key column cannot address a row.
    return readOnly('keyNotSelected');
  }
  const row = rows[rowIndex];
  const values: unknown[] = [];
  for (const identity of identityColumns) {
    const index = columns.findIndex((column) => column.name === identity.column);
    if (index < 0) {
      return readOnly('rowNotFound');
    }
    const value = row[index];
    if (value === null || value === undefined) {
      // A NULL cannot take part in an equality comparison, so the row cannot
      // be located by it.
      return readOnly('keyValueNull');
    }
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
    if (same) {
      // Two rows with the same identity cannot be told apart, so neither of
      // them may be written back.
      return readOnly('duplicateIdentity');
    }
  }
  return {
    writable: true,
    identityColumns,
    editableColumns: server.editableColumns ?? [],
    table: server.table as ResultTableRef,
  };
}

/**
 * The final verdict for one cell, legacy mapping verdict included.
 *
 * Two verdicts exist and they answer different questions:
 *
 * * the **narrow mapping verdict** (`writableMapping === 'verified'`) is the
 *   host-side check that shipped first: it recognises
 *   `SELECT * FROM <relation>` and the write path re-verifies the mapping
 *   against the live catalog before it writes;
 * * the **origin verdict** is the driver's own answer about where the rows
 *   came from, and it is the one that can grant write-back to statements the
 *   narrow check cannot see (an aliased key, a projected column).
 *
 * A cell is writable when **either** says so: the narrow verdict is not
 * withdrawn by the new one, and the new one adds what the narrow one cannot
 * see. Neither verdict is recomputed here — both are answers the back end
 * gave.
 */
export function resolveResultEditability(
  legacyVerified: boolean,
  server: ServerEditability,
  rows: ReadonlyArray<ReadonlyArray<unknown>>,
  columns: ReadonlyArray<{ name: string }>,
  rowIndex: number,
): GridEditability {
  if (legacyVerified) {
    return {
      writable: true,
      identityColumns: server.identityColumns ?? [],
      editableColumns: server.editableColumns ?? [],
      table: server.table as ResultTableRef,
    };
  }
  return resolveGridEditability(server, rows, columns, rowIndex);
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
