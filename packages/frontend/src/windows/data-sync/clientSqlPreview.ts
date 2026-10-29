import type { DataSyncSqlStatement, DataSyncTableResult, SyncOptions } from '../../commands/sync';
import { formatCell } from '../../lib/formatters';
import { operationAllowed } from './mappingView';

export function buildClientSqlPreview(tables: DataSyncTableResult[], options: SyncOptions): string {
  const lines: string[] = [];
  for (const table of tables) {
    if (table.status !== 'MATCHED') continue;
    const target = table.targetTable || table.sourceTable;
    for (const row of table.rows ?? []) {
      if (!row.selected || row.operation === 'UNCHANGED') continue;
      if (!operationAllowed(row.operation, options)) continue;
      const pk = row.key.map((v) => formatCell(v)).join(', ');
      switch (row.operation) {
        case 'INSERT':
          lines.push(`-- INSERT INTO ${target} (pk: ${pk})`);
          break;
        case 'UPDATE': {
          const cols = row.changedColumns.length > 0 ? row.changedColumns.join(', ') : '…';
          lines.push(`-- UPDATE ${target} SET ${cols} WHERE pk IN (${pk})`);
          break;
        }
        case 'DELETE':
          lines.push(`-- DELETE FROM ${target} WHERE pk IN (${pk})`);
          break;
        default:
          break;
      }
    }
  }
  return lines.length > 0 ? lines.join('\n') : '-- (no selected changes)';
}

export function statementsToPreviewText(
  statements: DataSyncSqlStatement[],
  opFilter: 'all' | DataSyncSqlStatement['operation'],
): string {
  const filtered =
    opFilter === 'all' ? statements : statements.filter((s) => s.operation === opFilter);
  if (filtered.length === 0) return '-- (no statements)';
  return filtered.map((s) => s.previewSql || s.sql).join('\n');
}

export function filterStatementsByOp(
  statements: DataSyncSqlStatement[],
  opFilter: 'all' | DataSyncSqlStatement['operation'],
): DataSyncSqlStatement[] {
  return opFilter === 'all' ? statements : statements.filter((s) => s.operation === opFilter);
}
