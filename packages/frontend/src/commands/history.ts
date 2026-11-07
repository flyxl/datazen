import { legacyInvoke as invoke, useBackendClient } from '@datazen/backend-client';
import type { HistoryPurgeScope } from '@datazen/backend-client';

/** W8 收口后类型单一来源在 backend-client（域词汇），此处仅转出。 */
export type { HistoryPurgeScope };

export type MigrationOperation = 'schemaDiff' | 'dataSync' | 'dataTransfer';
export type MigrationRunStatus = 'running' | 'completed' | 'failed' | 'cancelled' | 'interrupted';

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

export const historyCommands = {
  /** W8 刀3a web 收口：purge 走 queryAssets 域客户端（配置类令牌）。 */
  purgeHistory: async (args: { scope: HistoryPurgeScope; retainDays: number | null }) =>
    useBackendClient().purgeHistory(
      {
        scope: args.scope,
        ...(args.retainDays != null ? { retainDays: args.retainDays } : {}),
      },
      await useBackendClient().issueSubmissionToken('saveUserAsset', null),
    ),
  /** 迁移历史读面随 sync 域收敛（契约 §6.13 备注），此前留 legacy 轨。 */
  listMigrationRuns: (filter: MigrationRunFilter, offset = 0, limit = 25) =>
    invoke<MigrationRunPage>('list_migration_runs', { filter, offset, limit }),
  getMigrationRun: (runId: string) => invoke<MigrationRunRecord>('get_migration_run', { runId }),
};
