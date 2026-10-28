import { legacyInvoke as invoke } from '@datazen/backend-client';
import type { FilterCondition } from '../types';

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
  /** Native target DDL type override (cross-dialect create-new). */
  targetNativeType?: string;
}

export interface TransferTableMapping {
  sourceTable: string;
  targetTable: string;
  createNew?: boolean;
  enabled?: boolean;
  columnMappings?: TransferColumnMapping[];
  /** Execute this CREATE instead of auto-generated DDL. */
  ddlOverride?: string;
  /** Structured, parameterized source row filter. */
  sourceFilter?: TransferSourceFilter;
  /** Stable source recordset selection; never a restart checkpoint. */
  recordset?: TransferRecordset;
}

export interface TransferSourceFilter {
  filters: FilterCondition[];
  logic?: 'and' | 'or';
}

export interface TransferRecordsetBound {
  /** Text stays lossless on the IPC boundary; the source driver binds it using the inspected column type. */
  value: string;
  inclusive?: boolean;
}

export interface TransferRecordset {
  /** Legacy scalar source column, retained for existing IPC and profiles. */
  orderBy?: string;
  start?: TransferRecordsetBound;
  end?: TransferRecordsetBound;
  /** Complete source primary-key tuple, in the declared key order. */
  tupleRange?: TransferRecordsetTupleRange;
  limit?: number;
}

export interface TransferRecordsetTupleRange {
  columns: string[];
  start?: TransferRecordsetTupleBound;
  end?: TransferRecordsetTupleBound;
}

export interface TransferRecordsetTupleBound {
  values: string[];
  inclusive?: boolean;
}

export interface TransferOptions {
  batchSize?: number;
  stopOnError?: boolean;
  confirmedDestructive?: boolean;
  /** Explicitly use target-default collation when source text semantics cannot be preserved. */
  useTargetDefaultCollation?: boolean;
}

export interface TransferJob {
  source: TransferEndpoint;
  target?: TransferEndpoint;
  /** Opaque host token returned by the native SQL save dialog. */
  sqlFileTarget?: TransferSqlFileTarget;
  mode: TransferMode;
  writeMode: WriteMode;
  tables: TransferTableMapping[];
  options: TransferOptions;
}

export interface TransferSqlFileTarget {
  fileToken: string;
  /** Registered SQL driver used to render the artifact; omitted means source dialect. */
  databaseType?: string;
  /** Output text encoding; omitted means UTF-8 without a BOM. */
  encoding?: 'utf8' | 'utf8Bom' | 'utf16Le' | 'utf16Be';
  /** Output compression; omitted means an uncompressed .sql artifact. */
  compression?: 'none' | 'gzip';
  /** Optional target catalog/database qualifier for the generated SQL. */
  database?: string;
  /** Optional target schema qualifier for the generated SQL. */
  schema?: string;
}

export interface TransferProfile {
  version: number;
  id: string;
  name: string;
  sourceConnectionId: string;
  targetConnectionId?: string | null;
  sourceDatabase?: string | null;
  targetDatabase?: string | null;
  sourceSchema?: string | null;
  targetSchema?: string | null;
  destinationMode: 'database' | 'sqlFile';
  sqlFileDialect?: string | null;
  sqlFileEncoding?: 'utf8' | 'utf8Bom' | 'utf16Le' | 'utf16Be' | null;
  sqlFileCompression?: 'none' | 'gzip' | null;
  sqlFileDatabase?: string | null;
  sqlFileSchema?: string | null;
  mode: TransferMode;
  writeMode: WriteMode;
  tables: TransferTableMapping[];
  options: TransferOptions;
  createdAt: string;
  updatedAt: string;
}

export interface TransferTableResult {
  sourceTable: string;
  targetTable: string;
  status: TransferMappingStatus;
  createNew: boolean;
  enabled: boolean;
  columnMappings: TransferColumnMapping[];
  sourceColumns?: string[];
  sourcePrimaryKeys?: string[];
  targetColumns?: string[];
  sourceColumnTypes?: Record<string, string>;
  ddlOverride?: string;
  incompatibleReason?: string | null;
  sourceRowCount?: number | null;
  sourceFilter?: TransferSourceFilter;
  recordset?: TransferRecordset;
}

export interface TransferDdlPreview {
  sourceTable: string;
  targetTable: string;
  ddl: string;
  kind?: 'table' | 'index' | 'foreignKey' | 'dropTable';
  dependsOn?: string[];
}

export interface TransferWritePlan {
  sourceTable: string;
  targetTable: string;
  writeMode: WriteMode;
  mappedColumns: TransferColumnMapping[];
  estimatedRows?: number | null;
  preamble: string[];
  sourceFilterPreview?: string | null;
  recordsetPreview?: string | null;
}

export interface TransferPreview {
  planId: string;
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
  rowsInserted: number | null;
  success: boolean;
  error?: string | null;
  outcome?: 'committed' | 'rolledBack' | 'partiallyApplied' | 'notStarted' | 'unknown' | null;
}

export interface TransferExecutionResult {
  tables: TransferTableExecution[];
  rowsInserted: number;
  cancelled: boolean;
  partial: boolean;
  /** Opaque table-boundary token for a safe database-only resume. */
  resumeToken?: string | null;
}

export interface TransferRunSelection {
  sourceTables?: string[];
}

export interface TransferRunOptions {
  confirmedDestructive?: boolean;
}

export interface TransferRunRequest {
  planId: string;
  selection?: TransferRunSelection;
  options?: TransferRunOptions;
  jobId?: string;
  resumeToken?: string;
}

type DataTransferE2eRunCapture = {
  args: { request: TransferRunRequest; profile?: { id: string; revision: string } };
  response?: TransferExecutionResult;
  error?: string;
};

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
  useTargetDefaultCollation: false,
};

export const transferCommands = {
  getProfiles: () => invoke<TransferProfile[]>('get_transfer_profiles'),
  saveProfile: (profile: TransferProfile) => invoke<void>('save_transfer_profile', { profile }),
  deleteProfile: (profileId: string) => invoke<void>('delete_transfer_profile', { profileId }),
  pickSqlFile: () => invoke<TransferSqlFileTarget | null>('pick_data_transfer_sql_file'),
  classifyPair: (sourceDatabaseType: string, targetDatabaseType: string) =>
    invoke<TransferPairingView>('classify_transfer_pair', {
      sourceDatabaseType,
      targetDatabaseType,
    }),

  inspect: (
    sourceDbSessionId: string,
    targetDbSessionId: string,
    mode: TransferMode,
    sourceDatabase?: string,
    targetDatabase?: string,
    tables?: TransferTableMapping[],
  ) =>
    invoke<TransferTableResult[]>('inspect_data_transfer', {
      sourceDbSessionId,
      targetDbSessionId,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
      mode,
      tables: tables ?? null,
    }),

  inspectSqlFile: (
    sourceDbSessionId: string,
    mode: TransferMode,
    sourceDatabase?: string,
    sourceSchema?: string,
    targetDatabaseType?: string,
    tables?: TransferTableMapping[],
  ) =>
    invoke<TransferTableResult[]>('inspect_sql_file_transfer', {
      sourceDbSessionId,
      sourceDatabase: sourceDatabase ?? null,
      sourceSchema: sourceSchema ?? null,
      targetDatabaseType: targetDatabaseType ?? null,
      mode,
      tables: tables ?? null,
    }),

  preview: (job: TransferJob) => invoke<TransferPreview>('preview_data_transfer', { job }),

  execute: async (request: TransferRunRequest, profile?: { id: string; revision: string }) => {
    const args = { request, ...(profile ? { profile } : {}) };
    const captures = import.meta.env.VITE_E2E
      ? (
          globalThis as typeof globalThis & {
            __dataTransferRunCalls?: DataTransferE2eRunCapture[];
          }
        ).__dataTransferRunCalls
      : undefined;
    const capture: DataTransferE2eRunCapture | null = captures ? { args } : null;
    if (capture && captures) captures.push(capture);
    try {
      const response = await invoke<TransferExecutionResult>('execute_data_transfer', args);
      if (capture) capture.response = response;
      return response;
    } catch (error) {
      if (capture) capture.error = String(error);
      throw error;
    }
  },

  cancel: (jobId: string) => invoke<boolean>('cancel_data_transfer', { jobId }),
};
