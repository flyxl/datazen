import { invoke } from '@tauri-apps/api/core';

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
  getSyncTasks: () => invoke<SyncTask[]>('get_sync_tasks'),

  deleteSyncTask: (taskId: string) => invoke<void>('delete_sync_task', { taskId }),

  checkSyncConflicts: (taskId: string) =>
    invoke<{
      hasConflicts: boolean;
      conflicts: { table: string; originalRows: number; currentRows: number }[];
    }>('check_sync_conflicts', { taskId }),

  executeDataSync: (
    targetConnectionId: string,
    statements: unknown[],
    jobId?: string,
    targetDatabase?: string,
  ) =>
    invoke<{ applied: number; rolledBack: boolean }>('execute_data_sync', {
      targetConnectionId,
      statements,
      jobId: jobId ?? null,
      targetDatabase: targetDatabase ?? null,
    }),

  cancelDataSync: (jobId: string) => invoke<boolean>('cancel_data_sync', { jobId }),

  compareDataSync: (
    sourceConnectionId: string,
    targetConnectionId: string,
    tables?: string[],
    jobId?: string,
    sourceDatabase?: string,
    targetDatabase?: string,
  ) =>
    invoke<
      Array<{
        sourceTable: string;
        targetTable: string;
        status: string;
        incompatibleReason?: string | null;
        warnings?: string[];
        rows?: Array<{ operation: string }>;
      }>
    >('compare_data_sync', {
      sourceConnectionId,
      targetConnectionId,
      tables: tables ?? null,
      jobId: jobId ?? null,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
    }),

  applyDataSync: (
    sourceConnectionId: string,
    targetConnectionId: string,
    tables: string[],
    jobId?: string,
    sourceDatabase?: string,
    targetDatabase?: string,
  ) =>
    invoke<{ applied: number; rolledBack: boolean }>('apply_data_sync', {
      sourceConnectionId,
      targetConnectionId,
      tables,
      jobId: jobId ?? null,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
    }),

  inspectDataSync: (
    sourceConnectionId: string,
    targetConnectionId: string,
    sourceDatabase?: string,
    targetDatabase?: string,
  ) =>
    invoke<
      Array<{
        sourceTable: string;
        targetTable: string;
        status: string;
        incompatibleReason?: string | null;
        warnings?: string[];
        rows?: Array<{ operation: string }>;
      }>
    >('inspect_data_sync', {
      sourceConnectionId,
      targetConnectionId,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
    }),
};
