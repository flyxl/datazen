import { useBackendClient } from '@datazen/backend-client';
import type { HistoryPurgeScope, MigrationRunFilter } from '@datazen/backend-client';

/** W8 收口后类型单一来源在 backend-client（域词汇），此处仅转出。 */
export type { HistoryPurgeScope };
export type {
  MigrationOperation,
  MigrationRunStatus,
  MigrationRunRecord,
  MigrationRunPage,
  MigrationRunFilter,
} from '@datazen/backend-client';

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
  /** W8 刀3e web 收口：迁移历史读面随 sync 域客户端（契约 §6.13 备注 / §6.16）。 */
  listMigrationRuns: (filter: MigrationRunFilter, offset = 0, limit = 25) =>
    useBackendClient().listMigrationRuns({ filter, offset, limit }),
  getMigrationRun: (runId: string) => useBackendClient().getMigrationRun({ runId }),
};
