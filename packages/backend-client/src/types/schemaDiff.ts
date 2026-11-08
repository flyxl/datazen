/**
 * Schema-diff wire vocabulary (contract §6.13): profiles, table comparisons,
 * reviewed prepare envelopes and deploy receipts.
 *
 * The normalized shapes below are what the domain client hands out. The Rust
 * side serializes plan requirements as tagged enums (`{ backfill: {...} }` /
 * `{ unsupported: { operation, reason } }`); the client folds that into the
 * discriminated union `PlanRequirement` on the way in and unfolds it on the
 * way out, so no caller ever sees the tagged shape. Both transports (desktop
 * IPC and server HTTP) answer with the same Rust vocabulary, which is what
 * lets one client type serve both.
 */

import type { DatabaseObjectKind, TableSchema, TableOptionsSnapshot } from '@datazen/driver-sdk';

import type { JobDetails, JobProgress, JobState } from './jobs';

/** Object categories a unified plan can address beyond tables. */
export type SchemaDiffObjectKind = 'view' | DatabaseObjectKind;

/** Exact object identity used by the reviewed unified schema migration plan. */
export interface SchemaDiffDatabaseObject {
  kind: SchemaDiffObjectKind;
  schema?: string | null;
  name: string;
  /** PostgreSQL routine identity arguments used to disambiguate overloads. */
  signature?: string | null;
  /** Relation attached to a trigger, when exposed by the driver. */
  targetSchema?: string | null;
  targetName?: string | null;
}

export type StatementRisk = 'additive' | 'destructive' | 'rewrite';

export type DeployStatus =
  | 'committed'
  | 'unknown'
  | 'rolled_back'
  | 'mixed'
  | 'failed'
  | 'cancelled';

export interface PlanStatement {
  sql: string;
  risk: StatementRisk;
  rollbackSql: string | null;
  summary: string;
  requiresTransaction?: boolean;
}

export interface RollbackCompleteness {
  complete: boolean;
  missing: string[];
}

export interface TypeSuggestion {
  table: string;
  column: string;
  sourceType: string;
  suggestedType: string;
  currentType: string;
  reason: string;
  isKeyOrIndexed: boolean;
}

export interface ColumnTypeOverride {
  table: string;
  column: string;
  targetType: string;
}

/** Normalized plan requirement (the client folds the wire's tagged enum). */
export interface PlanRequirement {
  kind: 'Backfill' | 'Unsupported';
  table: string;
  column: string;
  reason: string;
}

/** Rollback counts derived from {@link RollbackCompleteness} and statement list. */
export interface RollbackCompletenessCounts {
  complete: number;
  missing: number;
}

export interface SchemaDiffPlan {
  planId?: string;
  table: string;
  tables: string[];
  sourceDialect: string;
  targetDialect: string;
  sameDialect: boolean;
  statements: PlanStatement[];
  warnings: string[];
  requirements?: PlanRequirement[];
  rollbackCompleteness: RollbackCompleteness;
  typeSuggestions?: TypeSuggestion[];
  /** Target catalog snapshots used to reject stale SQLite rebuild reviews. */
  expectedTargetSchemas?: TableSchema[];
}

/** Result of a prepare Job: reviewed plan artifact plus its frozen metadata. */
export interface SchemaDiffPrepareEnvelope {
  plan: SchemaDiffPlan;
  planId: string;
  sourceConnectionId?: string;
  targetConnectionId?: string;
  selectionRevision: number;
  planVersion: number;
  handlerVersion: number;
  checkpointVersion: number;
  expiresAt: string;
  recoveryPolicy: string;
}

export interface StatementExecResult {
  index: number;
  sql: string;
  ok: boolean;
  error: string | null;
}

export interface SchemaDiffDeployResult {
  status: DeployStatus;
  executedCount: number;
  statementCount: number;
  errors: string[];
  statementResults: StatementExecResult[];
}

export interface SchemaDiffJobAccepted {
  jobId: string;
  kind: 'schemaDiffPrepare' | 'schemaDiffApply';
  state: JobState;
  progress: JobProgress;
}

export interface SchemaDiffJobDetails {
  details: JobDetails;
  prepared: SchemaDiffPrepareEnvelope | null;
  planUnavailableAfterRestart: boolean;
  deployResult: SchemaDiffDeployResult | null;
}

// ── compare (single-table, synchronous read) ────────────────────────────────

export interface ColumnDiffEntry {
  name: string;
  dataType: string;
  nullable: boolean;
  isPrimaryKey: boolean;
}

export interface ChangedColumnDiff {
  name: string;
  source: ColumnDiffEntry;
  target: ColumnDiffEntry;
  changes: string[];
}

export interface CheckConstraintDiffEntry {
  name: string;
  expression: string;
}

export interface TableOptionsDiff {
  source: TableOptionsSnapshot;
  target: TableOptionsSnapshot;
  changes: string[];
}

export interface TableSchemaDiff {
  table: string;
  /** The selected table exists only on the target and is planned as DROP TABLE. */
  targetOnly?: boolean;
  /** Present on source, missing on target → ADD on deploy. */
  missingOnTarget?: ColumnDiffEntry[];
  /** Present on target only → DROP on deploy. */
  extraOnTarget?: ColumnDiffEntry[];
  /** Alias of missingOnTarget (legacy). */
  added: ColumnDiffEntry[];
  /** Alias of extraOnTarget (legacy). */
  removed: ColumnDiffEntry[];
  changed: ChangedColumnDiff[];
  missingCheckConstraints?: CheckConstraintDiffEntry[];
  extraCheckConstraints?: CheckConstraintDiffEntry[];
  tableOptions?: TableOptionsDiff;
  sourceDdl?: string;
  targetDdl?: string;
}

// ── requests ────────────────────────────────────────────────────────────────

/** Persisted Schema Diff setup. Runtime sessions and generated plans are never stored. */
export interface SchemaDiffProfile {
  version: 1;
  id: string;
  name: string;
  sourceConnectionId: string;
  targetConnectionId: string;
  sourceDatabase: string;
  targetDatabase: string;
  sourceSchema?: string | null;
  targetSchema?: string | null;
  tables: string[];
  targetOnlyTables?: string[];
  /** Optional for profiles persisted before unified object selection was introduced. */
  sourceObjects?: SchemaDiffDatabaseObject[];
  targetObjects?: SchemaDiffDatabaseObject[];
  allowDestructive: boolean;
  includeIndexes: boolean;
  requireRollback: boolean;
  typeOverrides?: ColumnTypeOverride[];
  createdAt: string;
  updatedAt: string;
}

/** `saveSchemaDiffProfile`: upsert by `profile.id`; the backend stamps `updatedAt`. */
export interface SchemaDiffProfileSaveRequest {
  profile: SchemaDiffProfile;
}

/** `deleteSchemaDiffProfile`: unknown id is an idempotent no-op. */
export interface SchemaDiffProfileDeleteRequest {
  profileId: string;
}

/** `compareTableSchemas`: single-table synchronous comparison over two live sessions. */
export interface SchemaDiffCompareRequest {
  sourceDbSessionId: string;
  targetDbSessionId: string;
  sourceTableName: string;
  targetTableName: string;
  sourceSchema?: string | null;
  targetSchema?: string | null;
}

/** `prepareSchemaDiffPlan`: table-level plan (accepted 202, poll job details). */
export interface SchemaDiffPreparePlanRequest {
  sourceDbSessionId: string;
  targetDbSessionId: string;
  tableNames: string[];
  targetTableNames?: string[];
  targetOnlyTableNames?: string[];
  sourceSchema?: string | null;
  targetSchema?: string | null;
  allowDestructive: boolean;
  includeIndexes?: boolean;
  typeOverrides?: ColumnTypeOverride[];
}

/** `prepareSchemaUnifiedPlan`: adds explicit object identities to the table set. */
export interface SchemaDiffPrepareUnifiedRequest extends SchemaDiffPreparePlanRequest {
  sourceObjects: SchemaDiffDatabaseObject[];
  targetObjects: SchemaDiffDatabaseObject[];
}

/**
 * `executeSchemaDiffDeploy`: either a registered plan (`planId` +
 * `selectionRevision`, from a prepare envelope) or a plan body the backend
 * registers first.
 */
export interface SchemaDiffDeployRequest {
  targetDbSessionId: string;
  plan?: SchemaDiffPlan;
  useTransaction?: boolean;
  requireRollback?: boolean;
  confirmDestructive?: string;
  jobId?: string;
  targetDatabase?: string | null;
  targetSchema?: string | null;
  profile?: { id: string; revision: string };
  planId?: string;
  selectionRevision?: number;
}

/** `listSchemaDiffJobs`: optional state filter and page bound. */
export interface SchemaDiffJobsListRequest {
  states: readonly JobState[] | null;
  limit: number;
}

/** `getSchemaDiffJobDetails`. */
export interface SchemaDiffJobDetailsRequest {
  jobId: string;
}

/** `cancelSchemaDiffDeploy`: idempotent; resolves whether a cancel took effect. */
export interface SchemaDiffDeployCancelRequest {
  jobId: string;
}

/** `verifySchemaDiffJobRecovery`: post-restart verification against the target session. */
export interface SchemaDiffRecoveryVerificationRequest {
  jobId: string;
  targetDbSessionId: string;
}
