import { invoke } from '@tauri-apps/api/core';
import type {
  StructureCapabilities,
  StructureChangePlan,
  StructureChangeRequest,
} from '../lib/structureEditor/types';

export const structureCommands = {
  getStructureCapabilities: (connectionId: string) =>
    invoke<StructureCapabilities>('get_structure_capabilities', { connectionId }),

  planTableStructureChanges: (connectionId: string, request: StructureChangeRequest) =>
    invoke<StructureChangePlan>('plan_table_structure_changes', { connectionId, request }),
};
