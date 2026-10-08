import { invoke } from '@tauri-apps/api/core';
import type { DatabaseObject, TableSchema, TableSchemaDiff } from '../types';

export type SchemaDiffObjectKind =
  | 'view'
  | 'function'
  | 'procedure'
  | 'trigger'
  | 'sequence'
  | 'type';

/** Exact object identity used by the reviewed unified schema migration plan. */
export type SchemaDiffObjectIdentity = Omit<DatabaseObject, 'kind'> & {
  kind: SchemaDiffObjectKind;
};

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

/** Normalized plan requirement (from IPC tagged enum). */
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

type PlanRequirementIpc =
  | { backfill: { table: string; column: string; reason: string } }
  | { unsupported: { operation: string; reason: string } };

type SchemaDiffPlanIpc = Omit<SchemaDiffPlan, 'requirements'> & {
  requirements?: PlanRequirementIpc[];
};

function normalizeRequirement(raw: PlanRequirementIpc): PlanRequirement {
  if ('backfill' in raw) {
    const { table, column, reason } = raw.backfill;
    return { kind: 'Backfill', table, column, reason };
  }
  const { operation, reason } = raw.unsupported;
  const dot = operation.indexOf('.');
  if (dot > 0) {
    return {
      kind: 'Unsupported',
      table: operation.slice(0, dot),
      column: operation.slice(dot + 1),
      reason,
    };
  }
  return { kind: 'Unsupported', table: operation, column: '', reason };
}

type SchemaDiffPrepareEnvelopeIpc = Omit<SchemaDiffPrepareEnvelope, 'plan'> & {
  plan: SchemaDiffPlanIpc;
};

function normalizePrepareEnvelope(raw: SchemaDiffPrepareEnvelopeIpc): SchemaDiffPrepareEnvelope {
  return { ...raw, plan: normalizePlan(raw.plan) };
}

function normalizePlan(plan: SchemaDiffPlanIpc): SchemaDiffPlan {
  return {
    ...plan,
    requirements: (plan.requirements ?? []).map(normalizeRequirement),
  };
}

function denormalizeRequirement(req: PlanRequirement): PlanRequirementIpc {
  if (req.kind === 'Backfill') {
    return { backfill: { table: req.table, column: req.column, reason: req.reason } };
  }
  const operation = req.column ? `${req.table}.${req.column}` : req.table;
  return { unsupported: { operation, reason: req.reason } };
}

function denormalizePlan(plan: SchemaDiffPlan): SchemaDiffPlanIpc {
  return {
    ...plan,
    requirements: (plan.requirements ?? []).map(denormalizeRequirement),
  };
}

export function rollbackCompletenessCounts(plan: SchemaDiffPlan): RollbackCompletenessCounts {
  const total = plan.statements.length;
  const missing = plan.rollbackCompleteness.missing.length;
  return { complete: total - missing, missing };
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

/// Clipboard export/import format. v2: keys renamed to connectionId per the
/// connectionId(dbSessionId) terminology (v1 configs with configId are rejected).
export interface SchemaDiffConfigJson {
  version: 2;
  sourceConnectionId: string;
  targetConnectionId: string;
  tables: string[];
  targetOnlyTables?: string[];
  /** Optional to keep v2 configs exported before unified object selection compatible. */
  sourceObjects?: SchemaDiffObjectIdentity[];
  targetObjects?: SchemaDiffObjectIdentity[];
  allowDestructive: boolean;
  includeIndexes?: boolean;
  requireRollback?: boolean;
}

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
  sourceObjects?: SchemaDiffObjectIdentity[];
  targetObjects?: SchemaDiffObjectIdentity[];
  allowDestructive: boolean;
  includeIndexes: boolean;
  requireRollback: boolean;
  typeOverrides?: ColumnTypeOverride[];
  createdAt: string;
  updatedAt: string;
}

export const DESTRUCTIVE_CONFIRM_TOKEN = 'DEPLOY';

export async function cancelSchemaDiffDeploy(jobId: string): Promise<boolean> {
  return invoke('cancel_schema_diff_deploy', { jobId });
}

export function planHasDestructive(plan: SchemaDiffPlan): boolean {
  return plan.statements.some((s) => s.risk === 'destructive' || s.risk === 'rewrite');
}

export function dialectSupportsTransactionalDdl(dialect: string): boolean {
  const d = dialect.toLowerCase();
  return d === 'postgresql' || d === 'postgres' || d === 'sqlite';
}

export function exportPlanSql(plan: SchemaDiffPlan): string {
  const header = [
    `-- Schema Diff Deploy plan`,
    `-- tables: ${plan.tables.join(', ')}`,
    `-- ${plan.sourceDialect} → ${plan.targetDialect}`,
    '',
  ];
  return [...header, ...plan.statements.map((s) => `${s.sql};`)].join('\n');
}

export const schemaDiffCommands = {
  getProfiles: () => invoke<SchemaDiffProfile[]>('get_schema_diff_profiles'),

  saveProfile: (profile: SchemaDiffProfile) =>
    invoke<void>('save_schema_diff_profile', { profile }),

  deleteProfile: (profileId: string) => invoke<void>('delete_schema_diff_profile', { profileId }),

  compareTableSchemas: (
    sourceDbSessionId: string,
    targetDbSessionId: string,
    sourceTableName: string,
    targetTableName: string,
    sourceSchema?: string,
    targetSchema?: string,
  ) =>
    invoke<TableSchemaDiff>('compare_table_schemas', {
      sourceDbSessionId,
      targetDbSessionId,
      sourceTableName,
      targetTableName,
      sourceSchema: sourceSchema ?? null,
      targetSchema: targetSchema ?? null,
    }),

  preparePlan: (params: {
    sourceDbSessionId: string;
    targetDbSessionId: string;
    tableNames: string[];
    targetTableNames?: string[];
    targetOnlyTableNames?: string[];
    sourceSchema?: string;
    targetSchema?: string;
    allowDestructive: boolean;
    includeIndexes?: boolean;
    typeOverrides?: ColumnTypeOverride[];
  }) => {
    const request = {
      sourceDbSessionId: params.sourceDbSessionId,
      targetDbSessionId: params.targetDbSessionId,
      tableNames: params.tableNames,
      targetTableNames: params.targetTableNames,
      allowDestructive: params.allowDestructive,
      includeIndexes: params.includeIndexes,
      typeOverrides: params.typeOverrides,
      sourceSchema: params.sourceSchema,
      targetSchema: params.targetSchema,
      ...(params.targetOnlyTableNames && params.targetOnlyTableNames.length > 0
        ? { targetOnlyTableNames: params.targetOnlyTableNames }
        : {}),
    };
    return invoke<SchemaDiffPrepareEnvelopeIpc>('prepare_schema_diff_plan', request).then(normalizePrepareEnvelope);
  },

  prepareUnifiedPlan: (params: {
    sourceDbSessionId: string;
    targetDbSessionId: string;
    tableNames: string[];
    targetTableNames?: string[];
    targetOnlyTableNames?: string[];
    sourceSchema?: string;
    targetSchema?: string;
    allowDestructive: boolean;
    includeIndexes?: boolean;
    typeOverrides?: ColumnTypeOverride[];
    sourceObjects: SchemaDiffObjectIdentity[];
    targetObjects: SchemaDiffObjectIdentity[];
  }) => {
    const exactIdentity = (object: SchemaDiffObjectIdentity) => ({
      kind: object.kind,
      schema: object.schema ?? null,
      name: object.name,
      signature: object.signature ?? null,
      targetSchema: object.targetSchema ?? null,
      targetName: object.targetName ?? null,
    });
    const request = {
      sourceDbSessionId: params.sourceDbSessionId,
      targetDbSessionId: params.targetDbSessionId,
      tableNames: params.tableNames,
      targetTableNames: params.targetTableNames,
      targetOnlyTableNames: params.targetOnlyTableNames,
      sourceSchema: params.sourceSchema,
      targetSchema: params.targetSchema,
      allowDestructive: params.allowDestructive,
      includeIndexes: params.includeIndexes,
      typeOverrides: params.typeOverrides,
      sourceObjects: params.sourceObjects.map(exactIdentity),
      targetObjects: params.targetObjects.map(exactIdentity),
    };
    return invoke<SchemaDiffPrepareEnvelopeIpc>('prepare_schema_unified_plan', request).then(normalizePrepareEnvelope);
  },

  prepareViewPlan: (params: {
    sourceDbSessionId: string;
    targetDbSessionId: string;
    objectNames: string[];
    allowDestructive: boolean;
  }) =>
    invoke<SchemaDiffPlanIpc>('prepare_schema_view_plan', {
      sourceDbSessionId: params.sourceDbSessionId,
      targetDbSessionId: params.targetDbSessionId,
      objectNames: params.objectNames,
      allowDestructive: params.allowDestructive,
    }).then(normalizePlan),

  prepareRoutineTriggerPlan: (params: {
    sourceDbSessionId: string;
    targetDbSessionId: string;
    kind: 'function' | 'procedure' | 'trigger';
    objectNames: string[];
    allowDestructive: boolean;
  }) =>
    invoke<SchemaDiffPlanIpc>('prepare_schema_routine_trigger_plan', {
      sourceDbSessionId: params.sourceDbSessionId,
      targetDbSessionId: params.targetDbSessionId,
      kind: params.kind,
      objectNames: params.objectNames,
      allowDestructive: params.allowDestructive,
    }).then(normalizePlan),

  prepareSequencePlan: (params: {
    sourceDbSessionId: string;
    targetDbSessionId: string;
    objectNames: string[];
    allowDestructive: boolean;
  }) =>
    invoke<SchemaDiffPlanIpc>('prepare_schema_sequence_plan', {
      sourceDbSessionId: params.sourceDbSessionId,
      targetDbSessionId: params.targetDbSessionId,
      objectNames: params.objectNames,
      allowDestructive: params.allowDestructive,
    }).then(normalizePlan),

  prepareTypePlan: (params: {
    sourceDbSessionId: string;
    targetDbSessionId: string;
    objectNames: string[];
    allowDestructive: boolean;
  }) =>
    invoke<SchemaDiffPlanIpc>('prepare_schema_type_plan', {
      sourceDbSessionId: params.sourceDbSessionId,
      targetDbSessionId: params.targetDbSessionId,
      objectNames: params.objectNames,
      allowDestructive: params.allowDestructive,
    }).then(normalizePlan),

  executeDeploy: (params: {
    targetDbSessionId: string;
    plan: SchemaDiffPlan;
    useTransaction?: boolean;
    requireRollback?: boolean;
    confirmDestructive?: string;
    jobId?: string;
    targetDatabase?: string | null;
    targetSchema?: string | null;
    profile?: { id: string; revision: string };
    /** P5: apply Job 消费的 planId（来自 prepare envelope）。 */
    planId?: string;
    /** P5: 审阅版本（来自 prepare envelope）。 */
    selectionRevision?: number;
  }) =>
    invoke<SchemaDiffDeployResult>('execute_schema_diff_deploy', {
      targetDbSessionId: params.targetDbSessionId,
      plan: denormalizePlan(params.plan),
      useTransaction: params.useTransaction,
      requireRollback: params.requireRollback,
      confirmDestructive: params.confirmDestructive,
      jobId: params.jobId,
      targetDatabase: params.targetDatabase,
      targetSchema: params.targetSchema,
      ...(params.profile ? { profile: params.profile } : {}),
      ...(params.planId ? { planId: params.planId } : {}),
      ...(params.selectionRevision != null ? { selectionRevision: params.selectionRevision } : {}),
    }),

  cancelDeploy: (jobId: string) => cancelSchemaDiffDeploy(jobId),
};
