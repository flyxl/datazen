import type {
  DataSyncRowChange,
  DataSyncSelectedRow,
  DataSyncTableSelection,
  DataSyncTableResult,
  SyncOptions,
} from '../../commands/sync';
import { defaultRowSelected, rowKeyString } from './mappingView';

/** Stable identity of one selected row inside a comparison page. */
export function selectedRowToken(
  row: Pick<DataSyncSelectedRow, 'sourceTable' | 'targetTable' | 'operation' | 'key'>,
): string {
  return `${row.sourceTable}\u0000${row.targetTable}\u0000${row.operation}\u0000${rowKeyString(row.key)}`;
}

/**
 * Whether a table scope covers the given operation.
 *
 * A scope in `all` mode covers every allowed operation, a scope in `defaults`
 * mode only the ones the sync options select by default.
 */
export function scopeSelectsOperation(
  scope: DataSyncTableSelection,
  operation: DataSyncRowChange['operation'],
  options: SyncOptions,
): boolean {
  return (
    operation !== 'UNCHANGED' &&
    scope.operations.includes(operation) &&
    (scope.selectionMode === 'all' || defaultRowSelected(operation, options))
  );
}

/** The scope that covers `operation` for the given table, if any. */
export function scopeForOperation(
  scopes: DataSyncTableSelection[],
  table: Pick<DataSyncTableResult, 'sourceTable' | 'targetTable'>,
  operation: DataSyncRowChange['operation'],
): DataSyncTableSelection | undefined {
  return scopes.find(
    (scope) =>
      scope.sourceTable === table.sourceTable &&
      scope.targetTable === table.targetTable &&
      scope.operations.includes(operation as Exclude<DataSyncRowChange['operation'], 'UNCHANGED'>),
  );
}
