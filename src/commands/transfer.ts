import { invoke } from '@tauri-apps/api/core';

export type TransferMode = 'structure' | 'data' | 'structureAndData';
export type WriteMode = 'insert' | 'truncateInsert' | 'dropCreateInsert';

export type TransferMappingStatus =
  | 'MATCHED'
  | 'CREATE_NEW'
  | 'UNMAPPED_SOURCE'
  | 'UNMAPPED_TARGET'
  | 'DISABLED'
  | 'INCOMPATIBLE';

export interface TransferEndpoint {
  dbSessionId: string;
  database: string;
  schema?: string | null;
}

export interface TransferColumnMapping {
  sourceColumn: string;
  targetColumn: string;
  skip?: boolean;
}

export interface TransferTableMapping {
  sourceTable: string;
  targetTable: string;
  createNew?: boolean;
  enabled?: boolean;
  columnMappings?: TransferColumnMapping[];
}

export interface TransferOptions {
  batchSize?: number;
  stopOnError?: boolean;
  confirmedDestructive?: boolean;
}

export interface TransferJob {
  source: TransferEndpoint;
  target: TransferEndpoint;
  mode: TransferMode;
  writeMode: WriteMode;
  tables: TransferTableMapping[];
  options: TransferOptions;
}

export interface TransferTableResult {
  sourceTable: string;
  targetTable: string;
  status: TransferMappingStatus;
  createNew: boolean;
  enabled: boolean;
  columnMappings: TransferColumnMapping[];
  sourceColumns?: string[];
  targetColumns?: string[];
  incompatibleReason?: string | null;
  sourceRowCount?: number | null;
}

export interface TransferDdlPreview {
  sourceTable: string;
  targetTable: string;
  ddl: string;
}

export interface TransferWritePlan {
  sourceTable: string;
  targetTable: string;
  writeMode: WriteMode;
  mappedColumns: TransferColumnMapping[];
  estimatedRows?: number | null;
  preamble: string[];
}

export interface TransferPreview {
  pairingPath: string;
  mode: TransferMode;
  writeMode: WriteMode;
  ddl: TransferDdlPreview[];
  writePlans: TransferWritePlan[];
  warnings: string[];
  canExecute: boolean;
  blockReason?: string | null;
}

export interface TransferTableExecution {
  sourceTable: string;
  targetTable: string;
  rowsInserted: number;
  success: boolean;
  error?: string | null;
}

export interface TransferExecutionResult {
  tables: TransferTableExecution[];
  rowsInserted: number;
  cancelled: boolean;
  partial: boolean;
}

export interface TransferPairingView {
  path: string;
  supported: boolean;
  family?: string | null;
  reason?: string | null;
}

export const DEFAULT_TRANSFER_OPTIONS: TransferOptions = {
  batchSize: 500,
  stopOnError: true,
  confirmedDestructive: false,
};

export const transferCommands = {
  classifyPair: (sourceDatabaseType: string, targetDatabaseType: string) =>
    invoke<TransferPairingView>('classify_transfer_pair', {
      sourceDatabaseType,
      targetDatabaseType,
    }),

  inspect: (
    sourceConnectionId: string,
    targetConnectionId: string,
    mode: TransferMode,
    sourceDatabase?: string,
    targetDatabase?: string,
    tables?: TransferTableMapping[],
  ) =>
    invoke<TransferTableResult[]>('inspect_data_transfer', {
      sourceConnectionId,
      targetConnectionId,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
      mode,
      tables: tables ?? null,
    }),

  preview: (job: TransferJob) => invoke<TransferPreview>('preview_data_transfer', { job }),

  execute: (job: TransferJob, jobId?: string) =>
    invoke<TransferExecutionResult>('execute_data_transfer', {
      job,
      jobId: jobId ?? null,
    }),

  cancel: (jobId: string) => invoke<boolean>('cancel_data_transfer', { jobId }),
};
