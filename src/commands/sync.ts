import { invoke } from '@tauri-apps/api/core';
import type { JobDetails } from '@datazen/backend-client';
import type { FilterCondition, Value } from '../types';

export interface SyncTask {
  id: string;
  /**
   * Legacy runtime ids may be absent. They are transient and are never used
   * to reopen a saved task; resolve fresh sessions from the connection ids.
   */
  sourceDbSessionId?: string;
  targetDbSessionId?: string;
  /** Persisted owning connection ids (config) for display / resume lookup. */
  sourceConnectionId: string;
  targetConnectionId: string;
  sourceDatabase?: string | null;
  targetDatabase?: string | null;
  sourceSchema?: string | null;
  targetSchema?: string | null;
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
  /** Saved checkpoints are explicitly unknown and require a fresh compare. */
  resumeState: 'unknown' | string;
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
  columns?: string[];
  columnTypes?: string[];
  primaryKeys?: string[];
  unchangedCount?: number;
  insertCount?: number;
  updateCount?: number;
  deleteCount?: number;
  rowCount?: number;
  pageSize?: number;
  firstCursor?: string | null;
  hasMore?: boolean;
  rows?: DataSyncRowChange[];
  sourceFilter?: DataSyncSourceFilter;
}

/** Structured, parameterized predicate applied symmetrically to one table pair. */
export interface DataSyncSourceFilter {
  filters: FilterCondition[];
  logic?: 'and' | 'or';
  /** Stable source recordset range. Bounds are text to preserve integer/decimal precision. */
  recordset?: DataSyncRecordset;
}

export interface DataSyncRecordsetBound {
  value: string;
  inclusive?: boolean;
}

export interface DataSyncRecordset {
  /** Omitted only for a single effective primary key. */
  orderBy?: string;
  start?: DataSyncRecordsetBound;
  end?: DataSyncRecordsetBound;
  /** Complete ordered composite primary-key range; cannot mix scalar fields. */
  tupleRange?: DataSyncRecordsetTupleRange;
  limit?: number;
}

export interface DataSyncRecordsetTupleRange {
  columns: string[];
  start?: DataSyncRecordsetTupleBound;
  end?: DataSyncRecordsetTupleBound;
}

export interface DataSyncRecordsetTupleBound {
  /** One lossless text value per key column, in `columns` order. */
  values: string[];
  inclusive?: boolean;
}

export interface SyncOptions {
  insert: boolean;
  update: boolean;
  delete: boolean;
  matchingStrategy?: 'primaryKey';
  batchSize?: number;
  largeValueMode?: 'full' | 'hash';
  conflictPolicy?: 'abort' | 'skip' | 'force';
}

export interface SyncProfile {
  version: number;
  id: string;
  name: string;
  sourceConnectionId: string;
  targetConnectionId: string;
  sourceDatabase?: string | null;
  targetDatabase?: string | null;
  sourceSchema?: string | null;
  targetSchema?: string | null;
  tables: DataSyncTableMapping[];
  options: SyncOptions;
  createdAt: string;
  updatedAt: string;
}

export interface DataSyncTableMapping {
  sourceTable: string;
  targetTable: string;
  enabled: boolean;
  matchingColumns?: Array<{ sourceColumn: string; targetColumn: string }>;
  sourceFilter?: DataSyncSourceFilter;
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
  /** Evidence-based transaction outcome; optional for pre-v1 IPC responses. */
  outcome?: 'not_started' | 'committed' | 'partially_applied' | 'rolled_back' | 'unknown';
  /** Present when execution could not proceed or its outcome is unknown. */
  error?: string;
  rollbackReason?: string;
  /** Total database-reported affected rows; optional for older responses. */
  affectedRows?: number;
  skipped?: number;
  conflicts?: DataSyncConflict[];
}

export interface DataSyncJobView {
  jobId: string;
  kind: string;
  state: 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';
  cancelRequested: boolean;
  effectOutcome: string | null;
  error: string | null;
  progress: { committed: number | string; unknown: number | string };
}

export type DataSyncJobDetails = Pick<
  JobDetails,
  'job' | 'recoveryTargets' | 'domainResults' | 'recovery'
>;

export interface DataSyncRecoveryRequest {
  jobId: string;
  sourceDbSessionId: string;
  targetDbSessionId: string;
  sourceDatabase: string;
  targetDatabase: string;
  sourceSchema?: string;
  targetSchema?: string;
}

export interface DataSyncConflict {
  table: string;
  operation: DataSyncOperation;
  rowKey: Value[];
  message: string;
}

export interface DataSyncSelectedRow {
  sourceTable: string;
  targetTable: string;
  operation: DataSyncOperation;
  key: Value[];
}

export interface DataSyncSelectionExclusion {
  operation: Exclude<DataSyncOperation, 'UNCHANGED'>;
  key: Value[];
}

export type DataSyncSelectionMode = 'all' | 'defaults';

export interface DataSyncTableSelection {
  sourceTable: string;
  targetTable: string;
  selectionMode: DataSyncSelectionMode;
  operations: Array<Exclude<DataSyncOperation, 'UNCHANGED'>>;
  excludedRows: DataSyncSelectionExclusion[];
}

export interface DataSyncSelection {
  revision: number;
  rows: DataSyncSelectedRow[];
  scopes?: DataSyncTableSelection[];
}

export interface DataSyncComparisonPreview {
  contractVersion?: number;
  planId: string;
  selectionRevision: number;
  pageSize?: number;
  tables: DataSyncTableResult[];
}

export interface DataSyncComparisonPage {
  contractVersion: number;
  planId: string;
  sourceTable: string;
  targetTable: string;
  cursor: string | null;
  nextCursor: string | null;
  hasMore: boolean;
  pageSize: number;
  rows: DataSyncRowChange[];
}

export interface DataSyncPairingView {
  path: string;
  supported: boolean;
  family?: string | null;
  reason?: string | null;
}

export const DEFAULT_SYNC_OPTIONS: SyncOptions = {
  insert: true,
  update: true,
  delete: false,
  matchingStrategy: 'primaryKey',
  batchSize: 1000,
  largeValueMode: 'full',
  conflictPolicy: 'abort',
};

let activeComparisonPlan: DataSyncComparisonPreview | null = null;
let activeExecutionOptions: SyncOptions = DEFAULT_SYNC_OPTIONS;

function valueToken(value: Value[]): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function selectionFromTables(
  plan: DataSyncComparisonPreview,
  tables: DataSyncTableResult[],
  options: SyncOptions,
  selectedRows?: DataSyncSelectedRow[],
  tableSelections?: DataSyncTableSelection[],
): DataSyncSelection {
  // Preserve scope input for server validation. UI state removes disabled
  // operations before it reaches this helper; a forged or stale caller must
  // still be rejected by the server instead of silently selecting less data.
  const activeScopes = tableSelections ?? [];
  const result = (rows: DataSyncSelectedRow[]): DataSyncSelection => ({
    revision: plan.selectionRevision,
    rows,
    ...(activeScopes.length > 0 ? { scopes: activeScopes } : {}),
  });
  if (selectedRows) {
    return result(
      selectedRows.filter((row) => {
        if (row.operation === 'INSERT') return options.insert;
        if (row.operation === 'UPDATE') return options.update;
        if (row.operation === 'DELETE') return options.delete;
        return false;
      }),
    );
  }
  const requested = new Set(
    tables.flatMap((table) =>
      (table.rows ?? [])
        .filter((row) => row.selected && row.operation !== 'UNCHANGED')
        .map(
          (row) =>
            `${table.sourceTable}\u0000${table.targetTable}\u0000${row.operation}\u0000${valueToken(row.key)}`,
        ),
    ),
  );
  const rows = plan.tables.flatMap((table) =>
    (table.rows ?? [])
      .filter((row) => {
        if (!row.selected || row.operation === 'UNCHANGED') return false;
        if (row.operation === 'INSERT' && !options.insert) return false;
        if (row.operation === 'UPDATE' && !options.update) return false;
        if (row.operation === 'DELETE' && !options.delete) return false;
        return requested.has(
          `${table.sourceTable}\u0000${table.targetTable}\u0000${row.operation}\u0000${valueToken(row.key)}`,
        );
      })
      .map((row) => ({
        sourceTable: table.sourceTable,
        targetTable: table.targetTable,
        operation: row.operation,
        key: row.key,
      })),
  );
  return result(rows);
}

function selectionFromStatements(
  plan: DataSyncComparisonPreview,
  statements: DataSyncSqlStatement[],
): DataSyncSelection {
  const rows: DataSyncSelectedRow[] = [];
  for (const statement of statements) {
    for (const table of plan.tables) {
      if (table.targetTable !== statement.table) continue;
      const match = (table.rows ?? []).find(
        (row) =>
          row.operation === statement.operation &&
          valueToken(row.key) === valueToken(statement.rowKey),
      );
      if (match) {
        rows.push({
          sourceTable: table.sourceTable,
          targetTable: table.targetTable,
          operation: match.operation,
          key: match.key,
        });
        break;
      }
      // Paged comparisons deliberately omit row payloads. The server-owned
      // plan remains the authority for validating this key and operation.
      if (!table.rows) {
        rows.push({
          sourceTable: table.sourceTable,
          targetTable: table.targetTable,
          operation: statement.operation,
          key: statement.rowKey,
        });
        break;
      }
    }
  }
  return { revision: plan.selectionRevision, rows };
}

const delay = (milliseconds: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, milliseconds));

async function waitForDataSyncJob(jobId: string): Promise<DataSyncJobView> {
  for (;;) {
    const job = await invoke<DataSyncJobView>('get_data_sync_job', { jobId });
    if (job.state === 'succeeded' || job.state === 'failed' || job.state === 'cancelled') {
      return job;
    }
    await delay(400);
  }
}

function durableFailure(job: DataSyncJobView, phase: 'prepare' | 'apply'): Error {
  if (job.state === 'cancelled') return new Error(`Data Sync ${phase} job was cancelled.`);
  if (job.error === 'notDispatchedAfterRestart') {
    return new Error('Data Sync job was not executed after restart; start a fresh comparison.');
  }
  if (job.effectOutcome === 'unknown' || Number(job.progress.unknown) > 0) {
    return new Error(
      'Data Sync write outcome needs verification; compare current data before continuing.',
    );
  }
  return new Error(`Data Sync ${phase} job failed; compare again before continuing.`);
}

async function readPreparePreview(jobId: string): Promise<DataSyncComparisonPreview> {
  let lastError: unknown;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    try {
      return await invoke<DataSyncComparisonPreview>('get_data_sync_job_preview', { jobId });
    } catch (error) {
      lastError = error;
      await delay(250);
    }
  }
  throw lastError instanceof Error
    ? lastError
    : new Error('Data Sync comparison finished without a review plan.');
}

function executionFromJob(details: DataSyncJobDetails): DataSyncExecutionResult {
  const job = details.job;
  const result = details.domainResults.find((entry) => entry.stageId === 'apply');
  const counter = (code: string): number =>
    Number(result?.counters.find((entry) => entry.code === code)?.value ?? 0);
  const committed = counter('committed') || Number(job.progress.committed);
  const effect = job.effectOutcome;
  const outcome =
    effect === 'partiallyApplied'
      ? 'partially_applied'
      : effect === 'rolledBack'
        ? 'rolled_back'
        : effect === 'notStarted'
          ? 'not_started'
          : effect === 'unknown' || counter('unknown') > 0
            ? 'unknown'
            : 'committed';
  const error =
    job.state === 'succeeded'
      ? undefined
      : job.error === 'notDispatchedAfterRestart'
        ? 'Data Sync job was not executed after restart.'
        : outcome === 'unknown'
          ? 'Commit outcome is unknown; compare current data before continuing.'
          : 'Data Sync apply job failed; compare again before continuing.';
  return {
    applied: committed,
    affectedRows: committed,
    rolledBack: outcome === 'rolled_back',
    outcome,
    error,
    rollbackReason: outcome === 'rolled_back' ? error : undefined,
    skipped: 0,
    conflicts: [],
  };
}

export const syncCommands = {
  classifyDataSyncPair: (sourceDatabaseType: string, targetDatabaseType: string) =>
    invoke<DataSyncPairingView>('classify_data_sync_pair', {
      sourceDatabaseType,
      targetDatabaseType,
    }),

  getSyncTasks: () => invoke<SyncTask[]>('get_sync_tasks'),

  getSyncProfiles: () => invoke<SyncProfile[]>('get_sync_profiles'),

  saveSyncProfile: (profile: SyncProfile) => invoke<void>('save_sync_profile', { profile }),

  deleteSyncProfile: (profileId: string) => invoke<void>('delete_sync_profile', { profileId }),

  deleteSyncTask: (taskId: string) => invoke<void>('delete_sync_task', { taskId }),

  checkSyncConflicts: (taskId: string) =>
    invoke<{
      hasConflicts: boolean;
      conflicts: { table: string; originalRows: number; currentRows: number }[];
    }>('check_sync_conflicts', { taskId }),

  executeDataSync: (
    targetDbSessionId: string,
    statements: DataSyncSqlStatement[],
    jobId?: string,
    targetDatabase?: string,
    selectedRows?: DataSyncSelectedRow[],
    tableSelections?: DataSyncTableSelection[],
    profile?: { id: string; revision: string },
  ) => {
    void targetDbSessionId;
    void targetDatabase;
    if (!activeComparisonPlan) {
      return Promise.reject(new Error('data sync comparison plan is missing; compare again'));
    }
    const selection =
      selectedRows || tableSelections
        ? selectionFromTables(
            activeComparisonPlan,
            [],
            activeExecutionOptions,
            selectedRows,
            tableSelections,
          )
        : selectionFromStatements(activeComparisonPlan, statements);
    const request = {
      planId: activeComparisonPlan.planId,
      selection,
      options: activeExecutionOptions,
      jobId: jobId ?? null,
    };
    return invoke<DataSyncExecutionResult>('execute_data_sync', {
      request,
      ...(profile ? { profile } : {}),
    });
  },

  cancelDataSync: (jobId: string) => invoke<boolean>('cancel_data_sync', { jobId }),

  getDataSyncComparisonPage: (
    cursor: string | null,
    sourceTable: string,
    targetTable: string,
    limit?: number,
  ) => {
    if (!activeComparisonPlan) {
      return Promise.reject(new Error('data sync comparison plan is missing; compare again'));
    }
    return invoke<DataSyncComparisonPage>('get_data_sync_comparison_page', {
      request: {
        planId: activeComparisonPlan.planId,
        sourceTable,
        targetTable,
        cursor,
        limit: limit ?? activeComparisonPlan.pageSize ?? 100,
      },
    });
  },

  compareDataSync: async (
    sourceDbSessionId: string,
    targetDbSessionId: string,
    tables?: string[],
    jobId?: string,
    sourceDatabase?: string,
    targetDatabase?: string,
    sourceSchema?: string,
    targetSchema?: string,
    options?: SyncOptions,
    filters?: Record<string, DataSyncSourceFilter>,
  ) => {
    activeComparisonPlan = null;
    const requestedJobId = jobId ?? `data-sync-${crypto.randomUUID()}`;
    const accepted = await invoke<DataSyncJobView>('start_data_sync_prepare_job', {
      sourceDbSessionId,
      targetDbSessionId,
      tables: tables ?? null,
      jobId: requestedJobId,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
      sourceSchema: sourceSchema ?? null,
      targetSchema: targetSchema ?? null,
      options: options ?? null,
      filters: filters ?? null,
    });
    const finished = await waitForDataSyncJob(accepted.jobId);
    if (finished.state !== 'succeeded') throw durableFailure(finished, 'prepare');
    const response = await readPreparePreview(finished.jobId);
    activeComparisonPlan = response;
    return response;
  },

  /** Apply the reviewed selection directly through a durable background Job. */
  executeDataSyncJob: async (
    sourceDbSessionId: string,
    targetDbSessionId: string,
    jobId: string,
    options: SyncOptions,
    selectedRows?: DataSyncSelectedRow[],
    tableSelections?: DataSyncTableSelection[],
  ) => {
    if (!activeComparisonPlan) {
      throw new Error('data sync comparison plan is missing; compare again');
    }
    activeExecutionOptions = options;
    const selection = selectionFromTables(
      activeComparisonPlan,
      [],
      options,
      selectedRows,
      tableSelections,
    );
    const accepted = await invoke<DataSyncJobView>('start_data_sync_apply_job', {
      sourceDbSessionId,
      targetDbSessionId,
      request: {
        planId: activeComparisonPlan.planId,
        selection,
        options,
        jobId,
      },
    });
    await waitForDataSyncJob(accepted.jobId);
    const details = await invoke<DataSyncJobDetails>('get_data_sync_job_details', {
      jobId: accepted.jobId,
    });
    return executionFromJob(details);
  },

  listDataSyncJobs: () => invoke<DataSyncJobDetails[]>('list_data_sync_jobs'),

  verifyDataSyncRecovery: (request: DataSyncRecoveryRequest) =>
    invoke<DataSyncJobDetails>('verify_data_sync_recovery', { request }),

  applyDataSync: (
    sourceDbSessionId: string,
    targetDbSessionId: string,
    tables: string[],
    jobId?: string,
    sourceDatabase?: string,
    targetDatabase?: string,
    sourceSchema?: string,
    targetSchema?: string,
    options?: SyncOptions,
  ) =>
    invoke<DataSyncExecutionResult>('apply_data_sync', {
      sourceDbSessionId,
      targetDbSessionId,
      tables,
      jobId: jobId ?? null,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
      sourceSchema: sourceSchema ?? null,
      targetSchema: targetSchema ?? null,
      options: options ?? null,
    }),

  inspectDataSync: (
    sourceDbSessionId: string,
    targetDbSessionId: string,
    sourceDatabase?: string,
    targetDatabase?: string,
    sourceSchema?: string,
    targetSchema?: string,
    mappings?: DataSyncTableMapping[],
  ) =>
    invoke<DataSyncTableResult[]>('inspect_data_sync', {
      sourceDbSessionId,
      targetDbSessionId,
      sourceDatabase: sourceDatabase ?? null,
      targetDatabase: targetDatabase ?? null,
      sourceSchema: sourceSchema ?? null,
      targetSchema: targetSchema ?? null,
      ...(mappings ? { tables: mappings } : {}),
    }),

  /** Generate only the selected rows; failures never trigger another write path. */
  generateDataSyncSql: (
    sourceDbSessionId: string,
    targetDbSessionId: string,
    tables: DataSyncTableResult[],
    options: SyncOptions,
    sourceDatabase?: string,
    targetDatabase?: string,
    sourceSchema?: string,
    targetSchema?: string,
    selectedRows?: DataSyncSelectedRow[],
    tableSelections?: DataSyncTableSelection[],
  ) => {
    void sourceDbSessionId;
    void targetDbSessionId;
    void sourceDatabase;
    void targetDatabase;
    void sourceSchema;
    void targetSchema;
    if (!activeComparisonPlan) {
      return Promise.reject(new Error('data sync comparison plan is missing; compare again'));
    }
    activeExecutionOptions = options;
    return invoke<DataSyncSqlStatement[]>('generate_data_sync_sql', {
      planId: activeComparisonPlan.planId,
      selection: selectionFromTables(
        activeComparisonPlan,
        tables,
        options,
        selectedRows,
        tableSelections,
      ),
      options,
    });
  },
};
