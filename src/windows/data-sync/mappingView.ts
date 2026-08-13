import type { TranslationKey } from '../../locales/zh-CN';

export type DataSyncMappingStatus =
  | 'MATCHED'
  | 'UNMAPPED_SOURCE'
  | 'UNMAPPED_TARGET'
  | 'DISABLED'
  | 'INCOMPATIBLE';

export interface DataSyncRowChange {
  operation: string;
}

export interface DataSyncTableResult {
  sourceTable: string;
  targetTable: string;
  status: DataSyncMappingStatus;
  incompatibleReason?: string | null;
  warnings?: string[];
  rows?: DataSyncRowChange[];
}

export function rowDiffCounts(row: DataSyncTableResult): {
  inserts: number;
  updates: number;
  deletes: number;
} {
  const rows = row.rows ?? [];
  return {
    inserts: rows.filter((r) => r.operation === 'INSERT').length,
    updates: rows.filter((r) => r.operation === 'UPDATE').length,
    deletes: rows.filter((r) => r.operation === 'DELETE').length,
  };
}

export function tableHasRowDiffs(row: DataSyncTableResult): boolean {
  const { inserts, updates, deletes } = rowDiffCounts(row);
  return inserts + updates + deletes > 0;
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

export function summarizeMappings(rows: DataSyncTableResult[]): {
  matched: number;
  incompatible: number;
  unmapped: number;
} {
  return {
    matched: rows.filter((r) => r.status === 'MATCHED').length,
    incompatible: rows.filter((r) => r.status === 'INCOMPATIBLE').length,
    unmapped: rows.filter((r) => r.status === 'UNMAPPED_SOURCE' || r.status === 'UNMAPPED_TARGET')
      .length,
  };
}
