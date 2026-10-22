import type { ColumnSchema, Value } from '../types';

/** The immutable values that identify a row for an UPDATE or DELETE. */
export type RowIdentity = Record<string, Value | null>;

export interface TableChangeContext {
  connectionId: string | null;
  dbSessionId: string;
  driverType: string | null;
  database: string | null;
  schema: string | null;
  table: string;
}

export interface PendingRowChange {
  /** UI-only hint for keeping a PK edit associated with its original row. */
  rowIndex?: number;
  rowIdentity: RowIdentity;
  originalValues: Record<string, Value | null>;
  currentValues: Record<string, Value | null>;
  changedColumns: string[];
  deleteMarked: boolean;
  /**
   * Marks this entry as a **new row draft** rather than a change to an existing
   * row. Such a row has no identity yet, so `changedColumns` is the exact
   * INSERT column list and a column absent from it keeps the database default
   * (auto-increment, generated, `DEFAULT now()`). `null` in `currentValues`
   * still means SQL `NULL`. The host never serializes `false`, which keeps
   * fingerprints of insert-free plans byte-identical.
   */
  insertMarked?: boolean;
}

/**
 * One new-row draft (front-end UI shape; it never goes on the wire).
 *
 * The write intent of a column lives in the **membership** of the key, not in a
 * separate tri-state enum:
 *
 * - key present with a value → the column is written with that value;
 * - key present with `null` → the column is written as SQL `NULL`;
 * - key absent → the column is **not written** and the database keeps its own
 *   default (auto-increment, generated column, `DEFAULT now()`, trigger).
 *
 * "Not written" and "written as NULL" are different statements, so clearing a
 * draft cell removes the key instead of storing `null`; and the columns that
 * are not written must never be "filled up" with `NULL` — that would break
 * auto-increment and generated columns (00-contracts §3).
 */
export interface PendingInsert {
  /** UI association key: React key plus the draft row's `data-draft-id`. */
  draftId: string;
  /** Per-column write intent. A present key means "written"; `null` means SQL NULL. */
  values: Record<string, Value | null>;
}

/**
 * The exact INSERT column list of a draft: the **sorted** names of the columns
 * the user actually wrote.
 *
 * The order is the column order of the generated statement, so it has to be
 * deterministic — the same draft must always produce the same plan and the same
 * bound parameters. A column the user cleared is absent from `values` and
 * therefore absent here as well.
 */
export function draftWriteColumns(values: Record<string, Value | null>): string[] {
  return Object.keys(values).sort();
}

export interface PlannedStatement {
  rowIdentity: RowIdentity;
  originalValues: Record<string, Value | null>;
  currentValues: Record<string, Value | null>;
  changedColumns: string[];
  /** Driver-rendered SQL preview. This is not a wire-level SQL audit record. */
  sqlTemplate: string;
  /** Human-readable summaries of the values used by the statement. */
  parameterSummary: string[];
}

export interface ChangeWarning {
  code: string;
  message: string;
  severity: 'info' | 'warning' | 'error';
}

export interface RowChangePlan {
  planId: string;
  fingerprint: string;
  configRevision?: import('@datazen/backend-client').Counter | null;
  table: TableChangeContext;
  updates: PlannedStatement[];
  deletes: PlannedStatement[];
  /**
   * New-row statements in request order: insertion order decides auto-increment.
   *
   * **Always present.** The host declares the field with `#[serde(default)]`
   * (a tolerance for *deserializing* a plan that does not carry it) and
   * serializes it unconditionally — a plan without inserts carries an empty
   * list, never a missing key. The mirror therefore has to be required too: an
   * optional field would let a caller claim a shape the host never produces.
   */
  inserts: PlannedStatement[];
  warnings: ChangeWarning[];
}

/**
 * A context is only safe for a write once every routing field is known. A
 * null schema is meaningful (drivers without schemas or an explicitly
 * unqualified table), while a null database is not: the current session must
 * be pinned to a concrete database before a pending change can be committed.
 */
export function isCompleteTableChangeContext(context: TableChangeContext): boolean {
  return Boolean(
    context.connectionId?.trim() &&
      context.dbSessionId.trim() &&
      context.driverType?.trim() &&
      context.database?.trim() &&
      context.table.trim(),
  );
}

/** Stable key for both table state isolation and plan/context comparisons. */
export function tableChangeContextKey(context: TableChangeContext): string {
  return JSON.stringify([
    context.connectionId,
    context.dbSessionId,
    context.driverType,
    context.database,
    context.schema,
    context.table,
  ]);
}

export interface CommitStatementResult {
  operation: 'insert' | 'update' | 'delete';
  rowIdentity: RowIdentity;
  affectedRows: number;
}

export interface CommitPendingChangesResponse {
  planId: string;
  fingerprint: string;
  statements: CommitStatementResult[];
  affectedRows: number;
}

export type PendingStatus = 'idle' | 'previewing' | 'committing';

/**
 * Where the row inserted by a commit ended up.
 *
 * - `located` — the new row was found on the refreshed page (by its row
 *   identity, or by the single-column maximum-primary-key candidate);
 * - `unavailable` — the commit succeeded, but the new row could not be located.
 *   The page is refreshed and the UI says so; it must **not** claim that the new
 *   row was highlighted (04 §4.1, the RETURNING backfill is not implemented in
 *   this batch, so this is the regular path, not an error);
 * - `not-applicable` — the commit contained no insert at all.
 */
export type InsertLocation = 'located' | 'unavailable' | 'not-applicable';

export interface CommitPendingChangesResult extends CommitPendingChangesResponse {
  status: 'committed' | 'failed' | 'noop';
  refreshed: boolean;
  refreshRequired: boolean;
  refreshStatus?: 'completed' | 'failed';
  error?: string;
  /** Front-end only: where the inserted row ended up after the refresh. */
  insertLocation?: InsertLocation;
}

function valueForColumn(row: Record<string, unknown>, column: string): Value | null {
  const value = row[column];
  return value === undefined ? null : (value as Value);
}

/**
 * JSON.stringify is not a sufficient identity canonicalizer: object key order
 * may vary and values such as undefined, NaN, functions, or cyclic objects do
 * not have a stable database identity representation. Keep this deliberately
 * JSON-like because those are the values the IPC Value contract can carry.
 */
function stableSerialize(value: unknown, seen = new Set<object>()): string | null {
  if (value === null) return 'null';
  switch (typeof value) {
    case 'string':
    case 'boolean':
      return JSON.stringify(value);
    case 'number':
      return Number.isFinite(value) ? JSON.stringify(value) : null;
    case 'undefined':
    case 'bigint':
    case 'function':
    case 'symbol':
      return null;
    case 'object':
      break;
    default:
      return null;
  }

  if (seen.has(value)) return null;
  seen.add(value);
  let result: string | null;
  if (Array.isArray(value)) {
    const items = value.map((item) => stableSerialize(item, seen));
    result = items.some((item) => item === null) ? null : `[${items.join(',')}]`;
  } else if (
    Object.getPrototypeOf(value) === Object.prototype ||
    Object.getPrototypeOf(value) === null
  ) {
    const entries = Object.keys(value)
      .sort()
      .map((key) => {
        const serialized = stableSerialize((value as Record<string, unknown>)[key], seen);
        return serialized === null ? null : `${JSON.stringify(key)}:${serialized}`;
      });
    result = entries.some((entry) => entry === null) ? null : `{${entries.join(',')}}`;
  } else {
    result = null;
  }
  seen.delete(value);
  return result;
}

function isStableIdentityValue(value: Value | null): value is Value {
  return value !== null && stableSerialize(value) !== null;
}

/** Build a stable identity from the table's declared primary-key columns. */
export function buildRowIdentity(
  row: Record<string, unknown>,
  primaryKeyColumns: ColumnSchema[],
): RowIdentity | null {
  if (primaryKeyColumns.length === 0) return null;
  const identity: RowIdentity = {};
  for (const column of primaryKeyColumns) {
    const value = valueForColumn(row, column.name);
    if (!isStableIdentityValue(value)) return null;
    identity[column.name] = value;
  }
  return identity;
}

/** A deterministic map key; column order in a schema must not affect identity. */
export function rowIdentityKey(identity: RowIdentity): string {
  if (Object.keys(identity).length === 0) {
    throw new Error('Row identity requires primary-key values');
  }
  const entries = Object.keys(identity)
    .sort()
    .map((column) => {
      const value = identity[column];
      const serialized = stableSerialize(value);
      if (!isStableIdentityValue(value) || serialized === null) {
        throw new Error('Row identity contains a NULL or unstable value');
      }
      return `${JSON.stringify(column)}:${serialized}`;
    });
  return `[${entries.join(',')}]`;
}

/** Return duplicate stable identities in a loaded row set. */
export function duplicateRowIdentityKeys(
  rows: Record<string, unknown>[],
  primaryKeyColumns: ColumnSchema[],
): string[] {
  const counts = new Map<string, number>();
  for (const row of rows) {
    const identity = buildRowIdentity(row, primaryKeyColumns);
    if (!identity) continue;
    const key = rowIdentityKey(identity);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return [...counts.entries()]
    .filter(([, count]) => count > 1)
    .map(([key]) => key)
    .sort();
}

export function valuesEqual(
  left: Value | null | undefined,
  right: Value | null | undefined,
): boolean {
  return JSON.stringify(left ?? null) === JSON.stringify(right ?? null);
}

/**
 * The last gate in front of the store: turn whatever the interface produced
 * into one of the three write intents (00-contracts §3).
 *
 * - `undefined` stays `undefined` — "do not write this column";
 * - `null` stays `null` — "write SQL `NULL`";
 * - a value that cannot survive the round trip (`NaN`, `Infinity`, a
 *   function) becomes `undefined`, i.e. "do not write".
 *
 * `NaN` is the dangerous one: `valuesEqual` compares with `JSON.stringify`,
 * which serialises `NaN` as `null` — so a `NaN` that reached the store would
 * be indistinguishable from a `NULL` and would silently write a `NULL`.
 */
export function normalizeEditValue(value: unknown): Value | null | undefined {
  if (value === undefined) return undefined;
  if (value === null) return null;
  if (typeof value === 'string' || typeof value === 'boolean') return value;
  if (typeof value === 'number') return Number.isFinite(value) ? value : undefined;
  if (Array.isArray(value)) return value as unknown[];
  if (typeof value === 'object') return value as Record<string, unknown>;
  // A function, a symbol or a bigint cannot be written to a column.
  return undefined;
}

export function clonePendingRowChange(change: PendingRowChange): PendingRowChange {
  return {
    ...(change.rowIndex === undefined ? {} : { rowIndex: change.rowIndex }),
    // The insert flag decides whether the row is a new-row draft or a change
    // to an existing one, so a clone that drops it turns an INSERT into an
    // UPDATE of a row that does not exist. It is copied only when it is
    // there: `insertMarked: undefined` would put the key into the JSON, and
    // the backend must keep receiving no key at all for a plan without
    // inserts (that is what keeps an insert-free fingerprint byte-identical).
    ...(change.insertMarked === undefined ? {} : { insertMarked: change.insertMarked }),
    rowIdentity: { ...change.rowIdentity },
    originalValues: { ...change.originalValues },
    currentValues: { ...change.currentValues },
    changedColumns: [...change.changedColumns],
    deleteMarked: change.deleteMarked,
  };
}
