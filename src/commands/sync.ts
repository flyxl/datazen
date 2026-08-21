import { invoke } from '@tauri-apps/api/core';
import type { Value } from '../types';

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

export type DataSyncOperation = 'INSERT' | 'UPDATE' | 'DELETE' | 'UNCHANGED';

export type DataSyncMappingStatus =
  | 'MATCHED'
  | 'UNMAPPED_SOURCE'
  | 'UNMAPPED_TARGET'
  | 'DISABLED'
  | 'INCOMPATIBLE';

export interface DataSyncRowChange {
  operation: DataSyncOperation;
  key: Value[];
  sourceRow: (Value | null)[] | null;
  targetRow: (Value | null)[] | null;
  changedColumns: string[];
  selected: boolean;
}

export interface DataSyncTableResult {
  sourceTable: string;
  targetTable: string;
  status: DataSyncMappingStatus;
  incompatibleReason?: string | null;
  warnings?: string[];
  rows?: DataSyncRowChange[];
}

export interface SyncOptions {
  insert: boolean;
  update: boolean;
  delete: boolean;
  matchingStrategy?: 'primaryKey';
  batchSize?: number;
  largeValueMode?: 'full' | 'hash';
}

export interface DataSyncSqlStatement {
  table: string;
  operation: DataSyncOperation;
  sql: string;
  previewSql: string;
  parameters: Value[];
  rowKey: Value[];
}

export interface DataSyncExecutionResult {
  applied: number;
  rolledBack: boolean;
}

export const DEFAULT_SYNC_OPTIONS: SyncOptions = {
  insert: true,
  update: true,
  delete: false,
  matchingStrategy: 'primaryKey',
  batchSize: 1000,
  largeValueMode: 'full',
};

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
    statements: DataSyncSqlStatement[],
    jobId?: string,
    targetDatabase?: string,
  ) =>
    invoke<DataSyncExecutionResult>('execute_data_sync', {
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
    sourceSchema?: string,
    targetSchema?: string,
    options?: SyncOptions,
  ) =>
    invoke<DataSyncTableResult[]>('compare_data_sync', {
      sourceConnectionId,
      targetConnectionId,
      tables: tables ?? null,
      jobId: jobId ?? null,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
      sourceSchema: sourceSchema ?? null,
      targetSchema: targetSchema ?? null,
      options: options ?? null,
    }),

  applyDataSync: (
    sourceConnectionId: string,
    targetConnectionId: string,
    tables: string[],
    jobId?: string,
    sourceDatabase?: string,
    targetDatabase?: string,
    sourceSchema?: string,
    targetSchema?: string,
    options?: SyncOptions,
  ) =>
    invoke<DataSyncExecutionResult>('apply_data_sync', {
      sourceConnectionId,
      targetConnectionId,
      tables,
      jobId: jobId ?? null,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
      sourceSchema: sourceSchema ?? null,
      targetSchema: targetSchema ?? null,
      options: options ?? null,
    }),

  inspectDataSync: (
    sourceConnectionId: string,
    targetConnectionId: string,
    sourceDatabase?: string,
    targetDatabase?: string,
    sourceSchema?: string,
    targetSchema?: string,
  ) =>
    invoke<DataSyncTableResult[]>('inspect_data_sync', {
      sourceConnectionId,
      targetConnectionId,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
      sourceSchema: sourceSchema ?? null,
      targetSchema: targetSchema ?? null,
    }),

  /** Expects backend `generate_data_sync_sql` (Phase A); falls back client-side in UI. */
  generateDataSyncSql: (
    sourceConnectionId: string,
    targetConnectionId: string,
    tables: DataSyncTableResult[],
    options: SyncOptions,
    sourceDatabase?: string,
    targetDatabase?: string,
    sourceSchema?: string,
    targetSchema?: string,
  ) =>
    invoke<DataSyncSqlStatement[]>('generate_data_sync_sql', {
      sourceConnectionId,
      targetConnectionId,
      tables,
      options,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
      sourceSchema: sourceSchema ?? null,
      targetSchema: targetSchema ?? null,
    }),
};
