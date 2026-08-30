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
  table: TableChangeContext;
  updates: PlannedStatement[];
  deletes: PlannedStatement[];
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
  operation: 'update' | 'delete';
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

export interface CommitPendingChangesResult extends CommitPendingChangesResponse {
  status: 'committed' | 'failed' | 'noop';
  refreshed: boolean;
  refreshRequired: boolean;
  refreshStatus?: 'completed' | 'failed';
  error?: string;
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
  } else if (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null) {
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

export function valuesEqual(left: Value | null | undefined, right: Value | null | undefined): boolean {
  return JSON.stringify(left ?? null) === JSON.stringify(right ?? null);
}

export function clonePendingRowChange(change: PendingRowChange): PendingRowChange {
  return {
    ...(change.rowIndex === undefined ? {} : { rowIndex: change.rowIndex }),
    rowIdentity: { ...change.rowIdentity },
    originalValues: { ...change.originalValues },
    currentValues: { ...change.currentValues },
    changedColumns: [...change.changedColumns],
    deleteMarked: change.deleteMarked,
  };
}
