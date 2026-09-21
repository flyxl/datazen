import { invoke } from '@tauri-apps/api/core';
import type {
  StructureCapabilities,
  StructureChangePlan,
  StructureChangeRequest,
} from '../lib/structureEditor/types';

export const structureCommands = {
  getStructureCapabilities: (dbSessionId: string) =>
    invoke<StructureCapabilities>('get_structure_capabilities', { dbSessionId }),

  /** Optional `database` is the plan's target, so cross-database DDL resolves
   * against the right catalog without switching the session. */
  planTableStructureChanges: (
    dbSessionId: string,
    request: StructureChangeRequest,
    database?: string | null,
  ) =>
    invoke<StructureChangePlan>('plan_table_structure_changes', {
      dbSessionId,
      request,
      database: database ?? null,
    }),
};
