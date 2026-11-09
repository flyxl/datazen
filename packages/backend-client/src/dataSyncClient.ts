/**
 * The Data-sync domain client (form-split F3, contract §6.16) — the largest
 * single domain: twenty-two methods covering task/profile assets, pairing
 * classification, the migration-run history read face (shared with schemaDiff
 * and dataTransfer), the durable job read-back family and the whole execution
 * face (inspect / revalidate / compare / prepare / apply / execute / SQL
 * preview / conflict check / recovery verification).
 *
 * Two desktop-only commands are deliberately **not** methods here, per the
 * contract's domain ruling: `save_sync_task_direct` (task rows are written by
 * the execution face itself — the web client face matches the legacy rail's
 * read + delete) and `set_data_sync_test_commit_fault` (webdriver-only fault
 * injection). `apply_data_sync` and `get_data_sync_comparison_page` stay on
 * the legacy rail for their own reasons — see the frontend command layer's
 * notes.
 *
 * Request bags are the desktop-IPC named-argument shapes (the same objects
 * the legacy `invoke` calls used to send). The web transport layer's route
 * constructors are the only place that knows the HTTP差异: `saveSyncProfile`
 * flattens `{ profile, submissionToken }` into the document body,
 * `cancelDataSync` unwraps the `{ cancelled }` envelope, path ids move into
 * the URL. The domain client itself adds nothing and drops nothing — every
 * method is a passthrough, which is exactly what the contract table pins.
 *
 * Write faces with a body carry a `SubmissionToken` (`saveSyncProfile`,
 * `cancelDataSync` — the user-asset / cancel HMAC track). The execution face
 * is token-free **by ruling**: job acceptance is idempotent through its own
 * acceptance key (`data-sync-prepare:{jobId}` / `data-sync-apply:{planId}`),
 * and inspect / revalidate / compare / SQL preview / conflict check are POST
 * reads or pure projections; the desktop transport drops the token before
 * invoking either way.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type {
  DataSyncApplyJobRequest,
  DataSyncCancelRequest,
  DataSyncCompareRequest,
  DataSyncExecuteRequest,
  DataSyncExecutionResponse,
  DataSyncInspectRequest,
  DataSyncJobRequest,
  DataSyncPairingClassifyRequest,
  DataSyncPairingView,
  DataSyncRecoveryRequest,
  DataSyncRevalidateRequest,
  DataSyncRevalidationReport,
  DataSyncSqlPreviewRequest,
  MigrationRunGetRequest,
  MigrationRunListRequest,
  MigrationRunPage,
  MigrationRunRecord,
  SqlStatement,
  SyncComparisonPreview,
  SyncConflictCheckRequest,
  SyncConflictCheckResult,
  SyncProfile,
  SyncProfileDeleteRequest,
  SyncProfileSaveRequest,
  SyncTask,
  SyncTaskDeleteRequest,
  TableResult,
} from './types/dataSync';
import type { JobDetails, JobView } from './types/jobs';
import type { SubmissionToken, SubmitRequest } from './types/session';

/** The Data-sync slice of `MethodMap`. */
export interface DataSyncMethodMap {
  // ── assets & read faces (刀3e-3) ────────────────────────────────────────
  getSyncTasks: { request: Record<string, never>; response: SyncTask[] };
  deleteSyncTask: { request: SyncTaskDeleteRequest; response: void };
  getSyncProfiles: { request: Record<string, never>; response: SyncProfile[] };
  saveSyncProfile: { request: SyncProfileSaveRequest & SubmitRequest; response: void };
  deleteSyncProfile: { request: SyncProfileDeleteRequest; response: void };
  classifyDataSyncPair: {
    request: DataSyncPairingClassifyRequest;
    response: DataSyncPairingView;
  };
  listMigrationRuns: { request: MigrationRunListRequest; response: MigrationRunPage };
  getMigrationRun: { request: MigrationRunGetRequest; response: MigrationRunRecord };
  listDataSyncJobs: { request: Record<string, never>; response: JobDetails[] };
  getDataSyncJob: { request: DataSyncJobRequest; response: JobView };
  getDataSyncJobDetails: { request: DataSyncJobRequest; response: JobDetails };
  getDataSyncJobPreview: { request: DataSyncJobRequest; response: SyncComparisonPreview };
  cancelDataSync: { request: DataSyncCancelRequest & SubmitRequest; response: boolean };

  // ── execution faces (刀3e-4b; token-free by ruling) ─────────────────────
  inspectDataSync: { request: DataSyncInspectRequest; response: TableResult[] };
  revalidateDataSync: { request: DataSyncRevalidateRequest; response: DataSyncRevalidationReport };
  compareDataSync: { request: DataSyncCompareRequest; response: SyncComparisonPreview };
  startDataSyncPrepareJob: { request: DataSyncCompareRequest; response: JobView };
  startDataSyncApplyJob: { request: DataSyncApplyJobRequest; response: JobView };
  executeDataSync: { request: DataSyncExecuteRequest; response: DataSyncExecutionResponse };
  generateDataSyncSql: { request: DataSyncSqlPreviewRequest; response: SqlStatement[] };
  checkSyncConflicts: { request: SyncConflictCheckRequest; response: SyncConflictCheckResult };
  verifyDataSyncRecovery: {
    request: { request: DataSyncRecoveryRequest };
    response: JobDetails;
  };
}

/** The Data-sync domain client surface. */
export interface DataSyncClient {
  getSyncTasks(): Promise<SyncTask[]>;
  deleteSyncTask(request: SyncTaskDeleteRequest): Promise<void>;
  getSyncProfiles(): Promise<SyncProfile[]>;
  saveSyncProfile(request: SyncProfileSaveRequest, token: SubmissionToken): Promise<void>;
  deleteSyncProfile(request: SyncProfileDeleteRequest): Promise<void>;
  classifyDataSyncPair(request: DataSyncPairingClassifyRequest): Promise<DataSyncPairingView>;
  listMigrationRuns(request?: MigrationRunListRequest): Promise<MigrationRunPage>;
  getMigrationRun(request: MigrationRunGetRequest): Promise<MigrationRunRecord>;
  listDataSyncJobs(): Promise<JobDetails[]>;
  getDataSyncJob(request: DataSyncJobRequest): Promise<JobView>;
  getDataSyncJobDetails(request: DataSyncJobRequest): Promise<JobDetails>;
  getDataSyncJobPreview(request: DataSyncJobRequest): Promise<SyncComparisonPreview>;
  cancelDataSync(request: DataSyncCancelRequest, token: SubmissionToken): Promise<boolean>;
  inspectDataSync(request: DataSyncInspectRequest): Promise<TableResult[]>;
  revalidateDataSync(request: DataSyncRevalidateRequest): Promise<DataSyncRevalidationReport>;
  compareDataSync(request: DataSyncCompareRequest): Promise<SyncComparisonPreview>;
  startDataSyncPrepareJob(request: DataSyncCompareRequest): Promise<JobView>;
  startDataSyncApplyJob(request: DataSyncApplyJobRequest): Promise<JobView>;
  executeDataSync(request: DataSyncExecuteRequest): Promise<DataSyncExecutionResponse>;
  generateDataSyncSql(request: DataSyncSqlPreviewRequest): Promise<SqlStatement[]>;
  checkSyncConflicts(request: SyncConflictCheckRequest): Promise<SyncConflictCheckResult>;
  verifyDataSyncRecovery(request: DataSyncRecoveryRequest): Promise<JobDetails>;
}

/** Build the Data-sync domain client over a transport. */
export function createDataSyncMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): DataSyncClient {
  return {
    getSyncTasks: () => guard('getSyncTasks', () => transport.call('getSyncTasks', {})),

    deleteSyncTask: (request) =>
      guard('deleteSyncTask', () => transport.call('deleteSyncTask', request)),

    getSyncProfiles: () => guard('getSyncProfiles', () => transport.call('getSyncProfiles', {})),

    saveSyncProfile: (request, submissionToken) =>
      guard('saveSyncProfile', () =>
        transport.call('saveSyncProfile', { ...request, submissionToken }),
      ),

    deleteSyncProfile: (request) =>
      guard('deleteSyncProfile', () => transport.call('deleteSyncProfile', request)),

    classifyDataSyncPair: (request) =>
      guard('classifyDataSyncPair', () => transport.call('classifyDataSyncPair', request)),

    listMigrationRuns: (request) =>
      guard('listMigrationRuns', () => transport.call('listMigrationRuns', request ?? {})),

    getMigrationRun: (request) =>
      guard('getMigrationRun', () => transport.call('getMigrationRun', request)),

    listDataSyncJobs: () => guard('listDataSyncJobs', () => transport.call('listDataSyncJobs', {})),

    getDataSyncJob: (request) =>
      guard('getDataSyncJob', () => transport.call('getDataSyncJob', request)),

    getDataSyncJobDetails: (request) =>
      guard('getDataSyncJobDetails', () => transport.call('getDataSyncJobDetails', request)),

    getDataSyncJobPreview: (request) =>
      guard('getDataSyncJobPreview', () => transport.call('getDataSyncJobPreview', request)),

    cancelDataSync: (request, submissionToken) =>
      guard('cancelDataSync', () =>
        transport.call('cancelDataSync', { ...request, submissionToken }),
      ),

    inspectDataSync: (request) =>
      guard('inspectDataSync', () => transport.call('inspectDataSync', request)),

    revalidateDataSync: (request) =>
      guard('revalidateDataSync', () => transport.call('revalidateDataSync', request)),

    compareDataSync: (request) =>
      guard('compareDataSync', () => transport.call('compareDataSync', request)),

    startDataSyncPrepareJob: (request) =>
      guard('startDataSyncPrepareJob', () => transport.call('startDataSyncPrepareJob', request)),

    startDataSyncApplyJob: (request) =>
      guard('startDataSyncApplyJob', () => transport.call('startDataSyncApplyJob', request)),

    executeDataSync: (request) =>
      guard('executeDataSync', () => transport.call('executeDataSync', request)),

    generateDataSyncSql: (request) =>
      guard('generateDataSyncSql', () => transport.call('generateDataSyncSql', request)),

    checkSyncConflicts: (request) =>
      guard('checkSyncConflicts', () => transport.call('checkSyncConflicts', request)),

    verifyDataSyncRecovery: (request) =>
      guard('verifyDataSyncRecovery', () => transport.call('verifyDataSyncRecovery', { request })),
  };
}

/** Compile-time: every Data-sync map key is served by this domain client. */
type Assert<T extends true> = T;
export type DataSyncClientCoversMap = Assert<
  keyof DataSyncMethodMap extends keyof DataSyncClient ? true : false
>;
