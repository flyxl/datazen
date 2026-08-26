import { invoke } from '@tauri-apps/api/core';
import type {
  StructureCapabilities,
  StructureChangePlan,
  StructureChangeRequest,
} from '../lib/structureEditor/types';

export const structureCommands = {
  getStructureCapabilities: (dbSessionId: string) =>
    invoke<StructureCapabilities>('get_structure_capabilities', { dbSessionId }),

  /** F1: optional `database` pins the session to the panel's target database
   * before planning, so cross-database DDL targets the right library. */
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
