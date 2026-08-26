import { invoke } from '@tauri-apps/api/core';
import type { TableSchemaDiff } from '../types';

export type StatementRisk = 'additive' | 'destructive' | 'rewrite';

export type DeployStatus = 'committed' | 'rolled_back' | 'mixed' | 'failed';

export interface PlanStatement {
  sql: string;
  risk: StatementRisk;
  rollbackSql: string | null;
  summary: string;
}

export interface RollbackCompleteness {
  complete: boolean;
  missing: string[];
}

export interface SchemaDiffPlan {
  table: string;
  tables: string[];
  sourceDialect: string;
  targetDialect: string;
  sameDialect: boolean;
  statements: PlanStatement[];
  warnings: string[];
  rollbackCompleteness: RollbackCompleteness;
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
  allowDestructive: boolean;
  includeIndexes?: boolean;
  requireRollback?: boolean;
}

export const DESTRUCTIVE_CONFIRM_TOKEN = 'DEPLOY';

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
  compareTableSchemas: (
    sourceDbSessionId: string,
    targetDbSessionId: string,
    tableName: string,
  ) =>
    invoke<TableSchemaDiff>('compare_table_schemas', {
      sourceDbSessionId,
      targetDbSessionId,
      tableName,
    }),

  preparePlan: (params: {
    sourceDbSessionId: string;
    targetDbSessionId: string;
    tableNames: string[];
    allowDestructive: boolean;
    includeIndexes?: boolean;
  }) =>
    invoke<SchemaDiffPlan>('prepare_schema_diff_plan', {
      sourceDbSessionId: params.sourceDbSessionId,
      targetDbSessionId: params.targetDbSessionId,
      tableNames: params.tableNames,
      allowDestructive: params.allowDestructive,
      includeIndexes: params.includeIndexes,
    }),

  executeDeploy: (params: {
    targetDbSessionId: string;
    plan: SchemaDiffPlan;
    useTransaction?: boolean;
    confirmDestructive?: string;
  }) =>
    invoke<SchemaDiffDeployResult>('execute_schema_diff_deploy', {
      targetDbSessionId: params.targetDbSessionId,
      plan: params.plan,
      useTransaction: params.useTransaction,
      confirmDestructive: params.confirmDestructive,
    }),
};
