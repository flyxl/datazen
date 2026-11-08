/**
 * Data-sync wire vocabulary (contract §6.16): task/profile assets, pairing
 * classification, migration-run history, comparison previews and the
 * reviewed-selection execution family.
 *
 * Single source for the shapes the `dataSync` domain client hands out. They
 * are mirrors of the shared Rust vocabulary (`crates/datazen/src/migration/
 * data_sync/*`, `…/history/domain/migration_run.rs`, `…/jobs/domain/job.rs`)
 * serialized camelCase over both transports — desktop IPC and server HTTP —
 * which is what lets one client type serve both.
 *
 * Frontend legacy-name correspondence (the names `packages/frontend/src/
 * commands/sync.ts` used before the W8 刀3e web 收口 lifted them here):
 *
 * | contract name (this file) | frontend legacy name |
 * | --- | --- |
 * | `TableResult` | `DataSyncTableResult` |
 * | `SqlStatement` | `DataSyncSqlStatement` |
 * | `DataSyncExecutionResponse` | `DataSyncExecutionResult` |
 * | `SyncComparisonPreview` | `DataSyncComparisonPreview` |
 * | everything else | same name |
 *
 * Job read faces reuse `JobView` / `JobDetails` from `./jobs` rather than
 * re-declaring them (the loose legacy mirror `DataSyncJobView` documented the
 * counter caveat by typing progress as `number | string`; the shared
 * `JobProgress` counters ride the wire as CM-01 decimal strings, so readers
 * wrap them in `Number()` — see `durableFailure` / `executionFromJob` in the
 * frontend command layer).
 */

import type { Value } from './driverCommands';

// ── enums ───────────────────────────────────────────────────────────────────

/** Row operation of one comparison change (`ChangeOperation`, SCREAMING_SNAKE_CASE wire). */
export type DataSyncOperation = 'INSERT' | 'UPDATE' | 'DELETE' | 'UNCHANGED';

/** Table-pair classification of one inspection (`TableMappingStatus`). */
export type DataSyncMappingStatus =
  | 'MATCHED'
  | 'UNMAPPED_SOURCE'
  | 'UNMAPPED_TARGET'
  | 'DISABLED'
  | 'INCOMPATIBLE';

/**
 * What the executor can prove about one run. `partially_applied` / `unknown`
 * are **success answers carrying evidence** (rows already committed cannot be
 * rolled back by the caller / the commit result never arrived) — never 4xx.
 */
export type DataSyncExecutionOutcome =
  | 'not_started'
  | 'committed'
  | 'partially_applied'
  | 'rolled_back'
  | 'unknown';

// ── structured source filters (applied symmetrically to one table pair) ─────

/**
 * The closed operator set of the shared filter contract
 * (`datazen_driver_api::filters::FilterOperator`, camelCase wire) — the same
 * 22 spellings `@datazen/extension-points` narrows for the grid. Unlike the
 * dialect-open `operator: string` of `./tableData`'s `FilterCondition`, the
 * sync filter vocabulary is a closed Rust enum with
 * `deny_unknown_fields`, so the union is pinned here on purpose.
 */
export type DataSyncFilterOperator =
  | 'eq'
  | 'ne'
  | 'gt'
  | 'lt'
  | 'gte'
  | 'lte'
  | 'like'
  | 'in'
  | 'isNull'
  | 'isNotNull'
  | 'notIn'
  | 'between'
  | 'notBetween'
  | 'contains'
  | 'notContains'
  | 'startsWith'
  | 'endsWith'
  | 'isEmpty'
  | 'isNotEmpty'
  | 'regex'
  | 'notRegex'
  | 'jsonContains';

/** One parameterized condition of a source filter (no connector: the sync wire rejects it). */
export interface DataSyncFilterCondition {
  column: string;
  operator: DataSyncFilterOperator;
  value?: Value;
}

/** Structured, parameterized predicate applied symmetrically to one table pair. */
export interface DataSyncSourceFilter {
  filters: DataSyncFilterCondition[];
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

// ── options / assets ────────────────────────────────────────────────────────

export interface SyncOptions {
  insert: boolean;
  update: boolean;
  delete: boolean;
  matchingStrategy?: 'primaryKey';
  batchSize?: number;
  largeValueMode?: 'full' | 'hash';
  conflictPolicy?: 'abort' | 'skip' | 'force';
}

/**
 * Persisted break/resume state of one Data Sync run (shared-layer
 * `StoredSyncTask`). Task rows are written by the execution face itself; the
 * client surface is read + delete only (`save_sync_task_direct` is a
 * desktop-only command by contract ruling).
 */
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

/** Persisted Data Sync setup. Runtime sessions and generated plans are never stored. */
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

/** Explicit table mapping, incl. disabled pairs and their structured source filters. */
export interface DataSyncTableMapping {
  sourceTable: string;
  targetTable: string;
  enabled: boolean;
  matchingColumns?: Array<{ sourceColumn: string; targetColumn: string }>;
  sourceFilter?: DataSyncSourceFilter;
}

/** Pairing classification of two database types (`classifyDataSyncPair`). */
export interface DataSyncPairingView {
  path: string;
  supported: boolean;
  family?: string | null;
  reason?: string | null;
}

// ── comparison projections ──────────────────────────────────────────────────

export interface DataSyncRowChange {
  operation: DataSyncOperation;
  key: Value[];
  sourceRow: (Value | null)[] | null;
  targetRow: (Value | null)[] | null;
  changedColumns: string[];
  selected: boolean;
}

/**
 * One table pair of a comparison projection.
 *
 * One interface deliberately spans the two Rust projections the read faces
 * answer with: the inspect result (`data_sync::TableResult` — columns/rows,
 * no row counts) and the frozen preview summary (`SyncComparisonTableSummary`
 * — counts and paging, never row payloads). Fields absent on a given face are
 * optional here for the same reason.
 */
export interface TableResult {
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

/**
 * The immutable, server-owned comparison a prepare job froze
 * (`SyncComparisonPreview`). `planId` + `selectionRevision` are the review
 * ticket every execution face re-validates.
 */
export interface SyncComparisonPreview {
  contractVersion?: number;
  planId: string;
  selectionRevision: number;
  pageSize?: number;
  tables: TableResult[];
}

// ── reviewed selection (the execution ticket) ───────────────────────────────

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

/** Server-owned selection of comparison rows for one table and operation. */
export interface DataSyncTableSelection {
  sourceTable: string;
  targetTable: string;
  selectionMode: DataSyncSelectionMode;
  operations: Array<Exclude<DataSyncOperation, 'UNCHANGED'>>;
  excludedRows: DataSyncSelectionExclusion[];
}

/** The reviewed selection a run request carries (keys and scopes only — never row payloads). */
export interface DataSyncSelection {
  revision: number;
  rows: DataSyncSelectedRow[];
  scopes?: DataSyncTableSelection[];
}

// ── execution results ───────────────────────────────────────────────────────

/** One row the executor refused to write under the requested conflict policy. */
export interface DataSyncConflict {
  table: string;
  operation: DataSyncOperation;
  rowKey: Value[];
  message: string;
}

/** One generated statement of the reviewed selection (SQL preview face). */
export interface SqlStatement {
  table: string;
  operation: DataSyncOperation;
  sql: string;
  previewSql: string;
  parameters: Value[];
  rowKey: Value[];
}

/**
 * `executeDataSync` answer: the legacy result fields kept flat plus the
 * evidence-based outcome (`DataSyncExecutionResponse`, outcome snake_case
 * wire). `partially_applied` / `unknown` arrive as Ok payloads on purpose.
 */
export interface DataSyncExecutionResponse {
  applied: number;
  rolledBack: boolean;
  /** Explains a confirmed rollback; absent means committed or unknown-outcome. */
  rollbackReason?: string;
  /** Total database-reported affected rows; optional on older responses. */
  affectedRows?: number;
  skipped?: number;
  conflicts?: DataSyncConflict[];
  outcome: DataSyncExecutionOutcome;
  /** Present when execution could not proceed or its outcome is unknown. */
  error?: string;
}

// ── migration-run history (shared with schemaDiff / dataTransfer) ───────────

export type MigrationOperation = 'schemaDiff' | 'dataSync' | 'dataTransfer';

export type MigrationRunStatus = 'running' | 'completed' | 'failed' | 'cancelled' | 'interrupted';

/** One row of the v12 migration-run history the three migration operations share. */
export interface MigrationRunRecord {
  id: string;
  operation: MigrationOperation;
  status: MigrationRunStatus;
  outcome: string;
  phase: string;
  profileId: string | null;
  profileRevision: string | null;
  sourceConnectionId: string | null;
  targetConnectionId: string | null;
  startedAt: string;
  finishedAt: string | null;
  selectedCount: number;
  committedCount: number;
  failedCount: number;
  conflictCount: number;
  cancelled: boolean;
  rollbackOutcome: string;
  errorSummary: string | null;
}

export interface MigrationRunPage {
  items: MigrationRunRecord[];
  total: number;
  offset: number;
  limit: number;
}

export interface MigrationRunFilter {
  operation?: MigrationOperation;
  status?: MigrationRunStatus;
  profileId?: string;
  connectionId?: string;
}

/** Optional run-history reference an execution may attribute itself to. */
export interface MigrationProfileRef {
  id: string;
  revision: string;
}

// ── request bags (desktop-IPC named-argument shapes; the web transport
//    layer's route constructors flatten/unwrap them for HTTP) ───────────────

/** `deleteSyncTask`: unknown id is an idempotent 204. */
export interface SyncTaskDeleteRequest {
  taskId: string;
}

/** `saveSyncProfile`: upsert by `profile.id`; the backend stamps `updatedAt`. */
export interface SyncProfileSaveRequest {
  profile: SyncProfile;
}

/** `deleteSyncProfile`: unknown id is an idempotent 204. */
export interface SyncProfileDeleteRequest {
  profileId: string;
}

/** `classifyDataSyncPair`: pure shared-layer projection, no side effects. */
export interface DataSyncPairingClassifyRequest {
  sourceDatabaseType: string;
  targetDatabaseType: string;
}

/** `listMigrationRuns`: filter + page; absent fields fall back to the backend defaults. */
export interface MigrationRunListRequest {
  filter?: MigrationRunFilter;
  offset?: number;
  limit?: number;
}

/** `getMigrationRun`: miss and cross-organization are one indistinguishable 404. */
export interface MigrationRunGetRequest {
  runId: string;
}

/** `getDataSyncJob` / `getDataSyncJobDetails` / `getDataSyncJobPreview`. */
export interface DataSyncJobRequest {
  jobId: string;
}

/** `cancelDataSync`: unknown / foreign-kind job resolves `{ cancelled: false }` (idempotent no-op). */
export interface DataSyncCancelRequest {
  jobId: string;
}

/** `inspectDataSync`: one session pair's table mapping and hard gates (no row comparison). */
export interface DataSyncInspectRequest {
  sourceDbSessionId: string;
  targetDbSessionId: string;
  sourceDatabase?: string | null;
  targetDatabase?: string | null;
  sourceSchema?: string | null;
  targetSchema?: string | null;
  /** Mapping override (saved profile mappings); absent = auto-mapping. */
  tables?: DataSyncTableMapping[];
}

/**
 * `revalidateDataSync`: re-runs the inspect gates and reports drift. `tables`
 * are **table names** (not mappings); an empty selection means "all tables".
 */
export interface DataSyncRevalidateRequest {
  sourceDbSessionId: string;
  targetDbSessionId: string;
  tables?: string[];
  sourceDatabase?: string | null;
  targetDatabase?: string | null;
  sourceSchema?: string | null;
  targetSchema?: string | null;
}

/**
 * `compareDataSync` / `startDataSyncPrepareJob`: the comparison request
 * (table set / options / structured filters / optional mapping override).
 * `compareDataSync` holds the request until terminal state (desktop
 * `compare_data_sync` chain); callers that only want the acceptance receipt
 * use `startDataSyncPrepareJob` and poll the job read faces.
 */
export interface DataSyncCompareRequest {
  sourceDbSessionId: string;
  targetDbSessionId: string;
  tables?: string[] | null;
  /** Client-minted job id (`data-sync-{uuid}`); absent lets the backend mint one. */
  jobId?: string | null;
  sourceDatabase?: string | null;
  targetDatabase?: string | null;
  sourceSchema?: string | null;
  targetSchema?: string | null;
  options?: SyncOptions | null;
  filters?: Record<string, DataSyncSourceFilter> | null;
  /** compare-semantics mapping override (the prepare path does not need it). */
  mappings?: DataSyncTableMapping[] | null;
}

/** The reviewed run a durable apply job / synchronous execution consumes. */
export interface DataSyncRunRequest {
  planId: string;
  selection: DataSyncSelection;
  options: SyncOptions;
  jobId?: string | null;
}

/** `startDataSyncApplyJob`: acceptance for the exact reviewed ChangeSet (202 + poll). */
export interface DataSyncApplyJobRequest {
  sourceDbSessionId: string;
  targetDbSessionId: string;
  request: DataSyncRunRequest;
}

/** `executeDataSync`: synchronous apply projection + two-segment run-history booking. */
export interface DataSyncExecuteRequest {
  request: DataSyncRunRequest;
  /** Optional run-history attribution (profile id + revision). */
  profile?: MigrationProfileRef | null;
}

/** `generateDataSyncSql`: SQL preview of the (sub)selection; never a write path. */
export interface DataSyncSqlPreviewRequest {
  planId: string;
  /** Absent = the default shape of the full reviewed selection. */
  selection?: DataSyncSelection | null;
  options: SyncOptions;
}

/** `checkSyncConflicts`: frozen source counts vs current counts of one persisted task. */
export interface SyncConflictCheckRequest {
  taskId: string;
}

/**
 * `verifyDataSyncRecovery`: explicit read-only verification — the endpoints
 * are re-authorized and evidence recorded; the handler never resumes writes.
 */
export interface DataSyncRecoveryRequest {
  jobId: string;
  sourceDbSessionId: string;
  targetDbSessionId: string;
  sourceDatabase: string;
  targetDatabase: string;
  sourceSchema?: string;
  targetSchema?: string;
}

// ── read-only reports ───────────────────────────────────────────────────────

/** One table whose structure / primary keys drifted since the plan was frozen. */
export interface DataSyncStaleTable {
  sourceTable: string;
  targetTable: string;
  status: DataSyncMappingStatus;
  reason: string | null;
}

/** `revalidateDataSync` answer (desktop `revalidation_report` shape). */
export interface DataSyncRevalidationReport {
  ok: boolean;
  staleTables: DataSyncStaleTable[];
}

/** One source row-count drift a persisted task reported. */
export interface SyncTaskConflict {
  table: string;
  originalRows: number;
  currentRows: number;
}

/** `checkSyncConflicts` answer (desktop IPC shape). */
export interface SyncConflictCheckResult {
  hasConflicts: boolean;
  conflicts: SyncTaskConflict[];
}
