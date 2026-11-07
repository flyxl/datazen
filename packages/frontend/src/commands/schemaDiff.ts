import { useBackendClient } from '@datazen/backend-client';
import { rollbackCompletenessCounts } from '@datazen/backend-client';
import type {
  ColumnTypeOverride,
  SchemaDiffDatabaseObject,
  SchemaDiffDeployRequest,
  SchemaDiffJobAccepted,
  SchemaDiffJobDetails,
  SchemaDiffPlan,
  SchemaDiffPrepareEnvelope,
  SchemaDiffPreparePlanRequest,
  SchemaDiffPrepareUnifiedRequest,
  SchemaDiffProfile,
} from '@datazen/backend-client';
import type { TableSchemaDiff } from '../types';

/**
 * Schema Diff commands (W8 刀3b web 收口)。
 *
 * 类型与 IPC 归一化（tagged enum 的 requirements 折叠）已上移
 * `@datazen/backend-client` 的 `schemaDiff` 域客户端——两个 transport（桌面
 * IPC / 服务端 HTTP）的 wire 同源（共享层 Rust 词汇 camelCase），归一化在
 * 域客户端做一次，本文件只保留 UI 侧的编排（轮询、观察者广播）。
 */

// ── 域词汇转出（单一来源在 backend-client；UI 既有 import 路径不变）───────
export type {
  SchemaDiffObjectKind,
  SchemaDiffDatabaseObject,
  SchemaDiffJobAccepted,
  SchemaDiffJobDetails,
  SchemaDiffPlan,
  SchemaDiffPrepareEnvelope,
  SchemaDiffProfile,
  ColumnTypeOverride,
  PlanStatement,
  PlanRequirement,
  StatementRisk,
  DeployStatus,
  StatementExecResult,
  RollbackCompleteness,
  RollbackCompletenessCounts,
  TypeSuggestion,
  SchemaDiffDeployResult,
} from '@datazen/backend-client';

export { rollbackCompletenessCounts };

/** 既有 UI import 路径的别名（单一来源在 backend-client）。 */
export type SchemaDiffObjectIdentity = SchemaDiffDatabaseObject;

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

/** preparePlan/prepareUnifiedPlan 的公共请求形状（unified 多对象集）。 */
type SchemaDiffPreparePlanRequestShape =
  | SchemaDiffPreparePlanRequest
  | SchemaDiffPrepareUnifiedRequest;

export const DESTRUCTIVE_CONFIRM_TOKEN = 'DEPLOY';

/** 用户资产写面令牌（契约 §6.13：配置类操作，不绑会话）。 */
async function userAssetToken() {
  return useBackendClient().issueSubmissionToken('saveUserAsset', null);
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

export type SchemaDiffJobUpdate =
  | { type: 'accepted'; accepted: SchemaDiffJobAccepted }
  | { type: 'details'; details: SchemaDiffJobDetails };

type SchemaDiffJobObserver = (update: SchemaDiffJobUpdate) => void;
const schemaDiffJobObservers = new Set<SchemaDiffJobObserver>();

export function subscribeSchemaDiffJobUpdates(observer: SchemaDiffJobObserver): () => void {
  schemaDiffJobObservers.add(observer);
  return () => schemaDiffJobObservers.delete(observer);
}

function publishSchemaDiffJobUpdate(update: SchemaDiffJobUpdate): void {
  for (const observer of schemaDiffJobObservers) observer(update);
}

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

async function waitForSchemaDiffJob(jobId: string): Promise<SchemaDiffJobDetails> {
  for (;;) {
    const details = await schemaDiffCommands.getJobDetails(jobId);
    publishSchemaDiffJobUpdate({ type: 'details', details });
    if (details.details.job.state !== 'queued' && details.details.job.state !== 'running') {
      return details;
    }
    await wait(400);
  }
}

async function submitSchemaDiffPrepare(
  method: 'preparePlan' | 'prepareUnifiedPlan',
  request: SchemaDiffPreparePlanRequestShape,
): Promise<SchemaDiffPrepareEnvelope> {
  const client = useBackendClient();
  const token = await userAssetToken();
  const accepted =
    method === 'preparePlan'
      ? await client.prepareSchemaDiffPlan(request, token)
      : await client.prepareSchemaUnifiedPlan(
          request as Parameters<typeof client.prepareSchemaUnifiedPlan>[0],
          token,
        );
  publishSchemaDiffJobUpdate({ type: 'accepted', accepted });
  const result = await waitForSchemaDiffJob(accepted.jobId);
  if (result.prepared) return result.prepared;
  const state = result.details.job.state;
  const reason = result.details.job.error ?? 'planUnavailable';
  throw new Error(
    result.planUnavailableAfterRestart
      ? 'PlanUnavailableAfterRestart: compare again before applying'
      : `Schema Diff plan job ended in ${state} (${reason})`,
  );
}

export const schemaDiffCommands = {
  getProfiles: () => useBackendClient().listSchemaDiffProfiles(),

  saveProfile: async (profile: SchemaDiffProfile) => {
    await useBackendClient().saveSchemaDiffProfile({ profile }, await userAssetToken());
  },

  deleteProfile: async (profileId: string) => {
    await useBackendClient().deleteSchemaDiffProfile({ profileId }, await userAssetToken());
  },

  compareTableSchemas: (
    sourceDbSessionId: string,
    targetDbSessionId: string,
    sourceTableName: string,
    targetTableName: string,
    sourceSchema?: string,
    targetSchema?: string,
  ) =>
    useBackendClient().compareTableSchemas({
      sourceDbSessionId,
      targetDbSessionId,
      sourceTableName,
      targetTableName,
      sourceSchema: sourceSchema ?? null,
      targetSchema: targetSchema ?? null,
    }) as Promise<TableSchemaDiff>,

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
  }) =>
    submitSchemaDiffPrepare('preparePlan', {
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
    }),

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
    return submitSchemaDiffPrepare('prepareUnifiedPlan', {
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
    });
  },

  executeDeploy: async (params: {
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
  }) => {
    const request: SchemaDiffDeployRequest = {
      targetDbSessionId: params.targetDbSessionId,
      plan: params.plan,
      useTransaction: params.useTransaction,
      requireRollback: params.requireRollback,
      confirmDestructive: params.confirmDestructive,
      jobId: params.jobId,
      targetDatabase: params.targetDatabase,
      targetSchema: params.targetSchema,
      ...(params.profile ? { profile: params.profile } : {}),
      ...(params.planId ? { planId: params.planId } : {}),
      ...(params.selectionRevision != null ? { selectionRevision: params.selectionRevision } : {}),
    };
    const accepted = await useBackendClient().executeSchemaDiffDeploy(
      request,
      await userAssetToken(),
    );
    publishSchemaDiffJobUpdate({ type: 'accepted', accepted });
    const detail = await waitForSchemaDiffJob(accepted.jobId);
    if (detail.deployResult) return detail.deployResult;
    throw new Error(`Schema Diff apply job ended in ${detail.details.job.state}`);
  },

  cancelDeploy: async (jobId: string) =>
    useBackendClient().cancelSchemaDiffDeploy({ jobId }, await userAssetToken()),

  getJobDetails: async (jobId: string) => useBackendClient().getSchemaDiffJobDetails({ jobId }),

  listJobs: () => useBackendClient().listSchemaDiffJobs({ states: null, limit: 100 }),

  verifyRecovery: async (jobId: string, targetDbSessionId: string) =>
    useBackendClient().verifySchemaDiffJobRecovery(
      { jobId, targetDbSessionId },
      await userAssetToken(),
    ),
};
