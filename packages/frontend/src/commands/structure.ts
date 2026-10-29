import { executeTargetCommand, metadataTarget } from '../lib/session/targetCommand';
import type {
  StructureCapabilities,
  StructureChangePlan,
  StructureChangeRequest,
} from '../lib/structureEditor/types';

export const structureCommands = {
  getStructureCapabilities: (
    dbSessionId: string,
    database: string | null,
    schema: string | null,
    connectionId: string,
  ) =>
    executeTargetCommand(metadataTarget(dbSessionId, database, schema, undefined, connectionId), {
      command: 'get_structure_capabilities',
      input: {},
    }) as Promise<StructureCapabilities>,

  /** Optional `database` is the plan's target, so cross-database DDL resolves
   * against the right catalog without switching the session. */
  planTableStructureChanges: (
    dbSessionId: string,
    request: StructureChangeRequest,
    database: string | null,
    connectionId: string,
  ) =>
    executeTargetCommand(
      metadataTarget(dbSessionId, database, request.schema ?? null, undefined, connectionId),
      { command: 'plan_table_structure_changes', input: { request } },
    ) as Promise<StructureChangePlan>,
};
