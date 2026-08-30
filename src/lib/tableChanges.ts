import type { ColumnSchema, Value } from '../types';

/** The immutable values that identify a row for an UPDATE or DELETE. */
export type RowIdentity = Record<string, Value | null>;

export interface TableChangeContext {
  dbSessionId: string;
  table: string;
  database: string | null;
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

/** Build a stable identity from the table's declared primary-key columns. */
export function buildRowIdentity(
  row: Record<string, unknown>,
  primaryKeyColumns: ColumnSchema[],
): RowIdentity | null {
  if (primaryKeyColumns.length === 0) return null;
  const identity: RowIdentity = {};
  for (const column of primaryKeyColumns) {
    identity[column.name] = valueForColumn(row, column.name);
  }
  return identity;
}

/** A deterministic map key; column order in a schema must not affect identity. */
export function rowIdentityKey(identity: RowIdentity): string {
  return JSON.stringify(
    Object.keys(identity)
      .sort()
      .map((column) => [column, identity[column]]),
  );
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
