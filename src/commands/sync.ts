import { invoke } from '@tauri-apps/api/core';
import type { TableComparison, TableDataCompare, TableSchemaDiff } from '../types';

export interface SyncTask {
  id: string;
  sourceConnectionId: string;
  targetConnectionId: string;
  sourceConfigId: string;
  targetConfigId: string;
  tables: string[];
  completedTables: string[];
  currentTable: string | null;
  currentTableOffset: number;
  sourceRowCounts: Record<string, number>;
  strategy: string;
  status: string;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
}

export const syncCommands = {
  compareDatabases: (sourceConnectionId: string, targetConnectionId: string) =>
    invoke<TableComparison[]>('compare_databases', {
      sourceConnectionId,
      targetConnectionId,
    }),

  compareTableSchemas: (
    sourceConnectionId: string,
    targetConnectionId: string,
    tableName: string,
  ) =>
    invoke<TableSchemaDiff>('compare_table_schemas', {
      sourceConnectionId,
      targetConnectionId,
      tableName,
    }),

  compareTableData: (
    sourceConnectionId: string,
    targetConnectionId: string,
    tableName: string,
  ) =>
    invoke<TableDataCompare>('compare_table_data', {
      sourceConnectionId,
      targetConnectionId,
      tableName,
    }),

  syncTable: (sourceConnectionId: string, targetConnectionId: string, tableName: string) =>
    invoke<number>('sync_table', {
      sourceConnectionId,
      targetConnectionId,
      tableName,
    }),

  syncTables: (params: {
    taskId: string;
    sourceConnectionId: string;
    targetConnectionId: string;
    sourceConfigId: string;
    targetConfigId: string;
    tables: string[];
    skipTables: string[];
    strategy: string;
    resumeTable?: string | null;
    resumeOffset?: number;
  }) => invoke<{ taskId: string; completedTables: string[]; totalTables: number; syncPath?: string }>('sync_tables', params),

  getSyncTasks: () => invoke<SyncTask[]>('get_sync_tasks'),

  deleteSyncTask: (taskId: string) => invoke<void>('delete_sync_task', { taskId }),

  checkSyncConflicts: (taskId: string) =>
    invoke<{ hasConflicts: boolean; conflicts: { table: string; originalRows: number; currentRows: number }[] }>(
      'check_sync_conflicts',
      { taskId },
    ),
};
