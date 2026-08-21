import type {
  DataSyncOperation,
  DataSyncRowChange,
  DataSyncTableResult,
  SyncOptions,
} from '../../commands/sync';
import type { Value } from '../../types';
import type { TranslationKey } from '../../locales/zh-CN';

export type { DataSyncOperation, DataSyncRowChange, DataSyncTableResult };

export type DataSyncMappingStatus = DataSyncTableResult['status'];

export type TableDiffFilter = 'all' | 'insert' | 'update' | 'delete' | 'unchanged' | 'incompatible';

export function rowDiffCounts(row: DataSyncTableResult): {
  inserts: number;
  updates: number;
  deletes: number;
  unchanged: number;
} {
  const rows = row.rows ?? [];
  return {
    inserts: rows.filter((r) => r.operation === 'INSERT').length,
    updates: rows.filter((r) => r.operation === 'UPDATE').length,
    deletes: rows.filter((r) => r.operation === 'DELETE').length,
    unchanged: rows.filter((r) => r.operation === 'UNCHANGED').length,
  };
}

export function tableHasRowDiffs(row: DataSyncTableResult): boolean {
  const { inserts, updates, deletes } = rowDiffCounts(row);
  return inserts + updates + deletes > 0;
}

export function tableIsUnchanged(row: DataSyncTableResult): boolean {
  return row.status === 'MATCHED' && !tableHasRowDiffs(row);
}

export function mappingLabelKey(status: DataSyncMappingStatus): TranslationKey {
  switch (status) {
    case 'MATCHED':
      return 'sync.mappingMatched';
    case 'UNMAPPED_SOURCE':
      return 'sync.mappingUnmappedSource';
    case 'UNMAPPED_TARGET':
      return 'sync.mappingUnmappedTarget';
    case 'DISABLED':
      return 'sync.mappingDisabled';
    case 'INCOMPATIBLE':
      return 'sync.mappingIncompatible';
    default:
      return 'sync.mappingIncompatible';
  }
}

export function displayTableName(row: DataSyncTableResult): string {
  if (row.sourceTable && row.targetTable && row.sourceTable !== row.targetTable) {
    return `${row.sourceTable} → ${row.targetTable}`;
  }
  return row.sourceTable || row.targetTable;
}

export function tableKey(row: DataSyncTableResult): string {
  return row.sourceTable || row.targetTable;
}

export function summarizeMappings(rows: DataSyncTableResult[]): {
  matched: number;
  incompatible: number;
  unmapped: number;
  disabled: number;
} {
  return {
    matched: rows.filter((r) => r.status === 'MATCHED').length,
    incompatible: rows.filter((r) => r.status === 'INCOMPATIBLE').length,
    unmapped: rows.filter((r) => r.status === 'UNMAPPED_SOURCE' || r.status === 'UNMAPPED_TARGET')
      .length,
    disabled: rows.filter((r) => r.status === 'DISABLED').length,
  };
}

export interface CompareSummaryStats {
  inserts: number;
  updates: number;
  deletes: number;
  unchangedTables: number;
  incompatible: number;
}

export function summarizeCompare(rows: DataSyncTableResult[]): CompareSummaryStats {
  let inserts = 0;
  let updates = 0;
  let deletes = 0;
  let unchangedTables = 0;
  let incompatible = 0;
  for (const row of rows) {
    if (row.status === 'INCOMPATIBLE') {
      incompatible += 1;
      continue;
    }
    if (row.status !== 'MATCHED') continue;
    const counts = rowDiffCounts(row);
    inserts += counts.inserts;
    updates += counts.updates;
    deletes += counts.deletes;
    if (!tableHasRowDiffs(row)) unchangedTables += 1;
  }
  return { inserts, updates, deletes, unchangedTables, incompatible };
}

export function rowKeyString(key: Value[]): string {
  try {
    return JSON.stringify(key);
  } catch {
    return String(key);
  }
}

export function defaultRowSelected(operation: DataSyncOperation, options: SyncOptions): boolean {
  switch (operation) {
    case 'INSERT':
      return options.insert;
    case 'UPDATE':
      return options.update;
    case 'DELETE':
      return false;
    default:
      return false;
  }
}

export function applyOptionsToRows(
  rows: DataSyncRowChange[],
  options: SyncOptions,
): DataSyncRowChange[] {
  return rows.map((row) => ({
    ...row,
    selected:
      row.operation === 'DELETE'
        ? row.selected && options.delete
        : defaultRowSelected(row.operation, options),
  }));
}

export function operationAllowed(operation: DataSyncOperation, options: SyncOptions): boolean {
  switch (operation) {
    case 'INSERT':
      return options.insert;
    case 'UPDATE':
      return options.update;
    case 'DELETE':
      return options.delete;
    default:
      return false;
  }
}

export function selectedRowCount(table: DataSyncTableResult, options: SyncOptions): number {
  return (table.rows ?? []).filter(
    (r) => r.selected && r.operation !== 'UNCHANGED' && operationAllowed(r.operation, options),
  ).length;
}

export function tableMatchesFilter(
  row: DataSyncTableResult,
  filter: TableDiffFilter,
  query: string,
): boolean {
  const name = displayTableName(row).toLowerCase();
  if (query && !name.includes(query.toLowerCase())) return false;
  if (filter === 'all') return true;
  if (filter === 'incompatible') return row.status === 'INCOMPATIBLE';
  if (row.status !== 'MATCHED') return false;
  const counts = rowDiffCounts(row);
  switch (filter) {
    case 'insert':
      return counts.inserts > 0;
    case 'update':
      return counts.updates > 0;
    case 'delete':
      return counts.deletes > 0;
    case 'unchanged':
      return tableIsUnchanged(row);
    default:
      return true;
  }
}

export function mergeCompareIntoMappings(
  mappings: DataSyncTableResult[],
  compared: DataSyncTableResult[],
): DataSyncTableResult[] {
  const bySource = new Map(compared.map((r) => [r.sourceTable || r.targetTable, r]));
  return mappings.map((m) => {
    const key = m.sourceTable || m.targetTable;
    const cmp = bySource.get(key);
    if (!cmp) return m;
    if (m.status === 'DISABLED') return m;
    return { ...m, rows: cmp.rows, warnings: cmp.warnings ?? m.warnings };
  });
}

export function markDisabledTables(
  rows: DataSyncTableResult[],
  disabled: Set<string>,
): DataSyncTableResult[] {
  return rows.map((r) => {
    if (r.status === 'MATCHED' && disabled.has(r.sourceTable)) {
      return { ...r, status: 'DISABLED' as const, rows: undefined };
    }
    return r;
  });
}

export function tablesForCompare(rows: DataSyncTableResult[]): string[] {
  return rows
    .filter((r) => r.status === 'MATCHED')
    .map((r) => r.sourceTable)
    .filter(Boolean);
}
