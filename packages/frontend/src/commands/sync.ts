import { legacyInvoke as invoke, useBackendClient } from '@datazen/backend-client';
import type {
  DataSyncExecutionResponse,
  DataSyncRecoveryRequest,
  DataSyncRowChange,
  DataSyncRunRequest,
  DataSyncSelectedRow,
  DataSyncSelection,
  DataSyncSourceFilter,
  DataSyncTableMapping,
  DataSyncTableSelection,
  JobDetails,
  JobView,
  SqlStatement,
  SyncComparisonPreview,
  SyncOptions,
  SyncProfile,
  TableResult,
  Value,
} from '@datazen/backend-client';

/**
 * Data Sync commands（W8 刀3e web 收口，契约 §6.16）。
 *
 * 类型与 22 路域方法已上移 `@datazen/backend-client` 的 `dataSync` 域客户端
 * ——两个 transport（桌面 IPC / 服务端 HTTP）的 wire 同源（共享层 Rust 词汇
 * camelCase），本文件只保留 UI 侧的编排：受理回执后的轮询、比较计划的
 * 模块级暂存、选区的客户端折叠。
 *
 * 两条 legacy 命令有意保留（域裁决见各自注释）：`apply_data_sync`（桌面
 * 永久拒绝的旧形 apply，域方法 `startDataSyncApplyJob` / `executeDataSync`
 * 才是收录面）与 `get_data_sync_comparison_page`（plan_store 行级分页读，
 * §6.16 二十二路由无对应方法）。
 */

// ── 域词汇转出（单一来源在 backend-client；UI 既有 import 路径不变）───────
export type {
  DataSyncOperation,
  DataSyncMappingStatus,
  DataSyncRowChange,
  DataSyncSourceFilter,
  DataSyncRecordset,
  DataSyncRecordsetBound,
  DataSyncRecordsetTupleRange,
  DataSyncRecordsetTupleBound,
  SyncOptions,
  SyncTask,
  SyncProfile,
  DataSyncTableMapping,
  DataSyncPairingView,
  DataSyncConflict,
  DataSyncSelectedRow,
  DataSyncSelectionExclusion,
  DataSyncSelectionMode,
  DataSyncTableSelection,
  DataSyncSelection,
  DataSyncRecoveryRequest,
} from '@datazen/backend-client';
export type { TableResult as DataSyncTableResult } from '@datazen/backend-client';
export type { SqlStatement as DataSyncSqlStatement } from '@datazen/backend-client';
export type { SyncComparisonPreview as DataSyncComparisonPreview } from '@datazen/backend-client';
export type { DataSyncExecutionResponse as DataSyncExecutionResult } from '@datazen/backend-client';
export type { JobView as DataSyncJobView } from '@datazen/backend-client';

/** 既有 UI 依赖的宽松 details 投影（单一来源 `JobDetails` 在 backend-client）。 */
export type DataSyncJobDetails = Pick<
  JobDetails,
  'job' | 'recoveryTargets' | 'domainResults' | 'recovery'
>;

/**
 * 桌面 legacy 轨的行级分页读应答（`get_data_sync_comparison_page`）。契约
 * §6.16 的二十二路由没有它的域方法（plan_store 分页投影暂为桌面专属），
 * 类型留在本文件随 legacy 命令一起消亡。
 */
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

export const DEFAULT_SYNC_OPTIONS: SyncOptions = {
  insert: true,
  update: true,
  delete: false,
  matchingStrategy: 'primaryKey',
  batchSize: 1000,
  largeValueMode: 'full',
  conflictPolicy: 'abort',
};

/** 用户资产写面令牌（契约 §6.16：save profile / cancel 两处带体写面）。 */
async function userAssetToken() {
  return useBackendClient().issueSubmissionToken('saveUserAsset', null);
}

let activeComparisonPlan: SyncComparisonPreview | null = null;
let activeExecutionOptions: SyncOptions = DEFAULT_SYNC_OPTIONS;

function valueToken(value: Value[]): string {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function selectionFromTables(
  plan: SyncComparisonPreview,
  tables: TableResult[],
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
  plan: SyncComparisonPreview,
  statements: SqlStatement[],
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

async function waitForDataSyncJob(jobId: string): Promise<JobView> {
  for (;;) {
    const job = await useBackendClient().getDataSyncJob({ jobId });
    if (job.state === 'succeeded' || job.state === 'failed' || job.state === 'cancelled') {
      return job;
    }
    await delay(400);
  }
}

function durableFailure(job: JobView, phase: 'prepare' | 'apply'): Error {
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

async function readPreparePreview(jobId: string): Promise<SyncComparisonPreview> {
  return useBackendClient().getDataSyncJobPreview({ jobId });
}

function executionFromJob(details: DataSyncJobDetails): DataSyncExecutionResponse {
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
    useBackendClient().classifyDataSyncPair({ sourceDatabaseType, targetDatabaseType }),

  getSyncTasks: () => useBackendClient().getSyncTasks(),

  getSyncProfiles: () => useBackendClient().getSyncProfiles(),

  saveSyncProfile: async (profile: SyncProfile) => {
    await useBackendClient().saveSyncProfile({ profile }, await userAssetToken());
  },

  deleteSyncProfile: (profileId: string) => useBackendClient().deleteSyncProfile({ profileId }),

  deleteSyncTask: (taskId: string) => useBackendClient().deleteSyncTask({ taskId }),

  checkSyncConflicts: (taskId: string) => useBackendClient().checkSyncConflicts({ taskId }),

  executeDataSync: (
    targetDbSessionId: string,
    statements: SqlStatement[],
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
    const request: DataSyncRunRequest = {
      planId: activeComparisonPlan.planId,
      selection,
      options: activeExecutionOptions,
      jobId: jobId ?? null,
    };
    return useBackendClient().executeDataSync({
      request,
      ...(profile ? { profile } : {}),
    });
  },

  cancelDataSync: async (jobId: string) =>
    useBackendClient().cancelDataSync({ jobId }, await userAssetToken()),

  /**
   * plan_store 的行级分页读。契约 §6.16 的 dataSync 域二十二路由**没有**
   * 对应方法（`getDataSyncJobPreview` 只回汇总投影），命令留在桌面 legacy
   * 轨——web 形态继续由 `unsupportedMethods` 声明为 capability 缺口。
   */
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

  /**
   * 受理—轮询—读复核投影的比较链（契约 §6.16：长比较占住请求的
   * `compareDataSync` 同步语义供纯调用方使用；UI 要进度与取消，走
   * `startDataSyncPrepareJob` 202 + 轮询读回）。
   */
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
    const accepted = await useBackendClient().startDataSyncPrepareJob({
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
    const accepted = await useBackendClient().startDataSyncApplyJob({
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
    const details = await useBackendClient().getDataSyncJobDetails({
      jobId: accepted.jobId,
    });
    return executionFromJob(details);
  },

  listDataSyncJobs: () => useBackendClient().listDataSyncJobs(),

  verifyDataSyncRecovery: (request: DataSyncRecoveryRequest) =>
    useBackendClient().verifyDataSyncRecovery(request),

  /**
   * 桌面 legacy 旧形 apply（无已复核计划）。桌面命令本体是**永久拒绝**的
   * 引路桩（"legacy apply has no reviewed plan; compare and submit the
   * reviewed selection as a durable Data Sync job"，desktop `commands/sync/
   * mod.rs`）；契约 §6.16 未给它立域方法——收录面是
   * `startDataSyncApplyJob`（durable）与 `executeDataSync`（同步投影）。
   * 命令留在 legacy 轨仅为签名兼容，调用必然得到上述拒绝。
   */
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
    invoke<DataSyncExecutionResponse>('apply_data_sync', {
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
    useBackendClient().inspectDataSync({
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
    tables: TableResult[],
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
    return useBackendClient().generateDataSyncSql({
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
