import { legacyInvoke as invoke } from '@datazen/backend-client';

export type HistoryPurgeScope = 'query' | 'workflow' | 'all';

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
  purgeHistory: (args: { scope: HistoryPurgeScope; retainDays: number | null }) =>
    invoke<number>('purge_history', args),
  listMigrationRuns: (filter: MigrationRunFilter, offset = 0, limit = 25) =>
    invoke<MigrationRunPage>('list_migration_runs', { filter, offset, limit }),
  getMigrationRun: (runId: string) =>
    invoke<MigrationRunRecord>('get_migration_run', { runId }),
};
