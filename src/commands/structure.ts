import { invoke } from '@tauri-apps/api/core';
import type {
  StructureCapabilities,
  StructureChangePlan,
  StructureChangeRequest,
} from '../lib/structureEditor/types';

export const structureCommands = {
  getStructureCapabilities: (dbSessionId: string) =>
    invoke<StructureCapabilities>('get_structure_capabilities', { dbSessionId }),

  planTableStructureChanges: (dbSessionId: string, request: StructureChangeRequest) =>
    invoke<StructureChangePlan>('plan_table_structure_changes', { dbSessionId, request }),
};
