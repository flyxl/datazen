import { ApiError, API_ERROR_CODES } from '@datazen/backend-client';
import { databaseCommands } from '../../commands/database';
import type { UseSchemaDiffEndpointsReturn } from './useSchemaDiffEndpoints';
import type { useSchemaDiffUnifiedObjects } from './useSchemaDiffUnifiedObjects';
import { mergeSchemaDiffTablePicks } from './schemaDiffTableNames';

type DiscoveryStage = 'ensure-source' | 'ensure-target' | 'list-source-tables' | 'list-target-tables' | 'object-catalog';

/** Only fixed stages and explicitly allowed categories enter the discovery banner. */
export function discoveryFailure(stage: DiscoveryStage, error: unknown): string {
  let category = 'unknown';
  if (error instanceof ApiError && API_ERROR_CODES.includes(error.code)) category = error.code;
  else if (error instanceof Error) {
    // executeTargetCommand emits exactly this controlled execution receipt string.
    const receipt = /^(sqlError|protocolError|cancelled|timeout|resourceLost|pipelineAborted|hostRejected); effect: (notStarted|completed|rolledBack|partiallyApplied|unknown)$/.exec(error.message);
    category = receipt?.[1] ?? (['Error', 'TypeError', 'RangeError', 'TimeoutError'].includes(error.name) ? error.name : 'unknown');
  }
  return `schemaObjectDiscovery.${stage}:${category}`;
}

export class SchemaObjectDiscoveryError extends Error {
  readonly diagnostic: string;
  constructor(stage: DiscoveryStage, error: unknown) {
    super(error instanceof Error ? error.message : String(error));
    this.diagnostic = discoveryFailure(stage, error);
  }
}

export async function discoverSchemaDiffObjects(
  endpoints: UseSchemaDiffEndpointsReturn,
  objects: ReturnType<typeof useSchemaDiffUnifiedObjects>,
) {
  const source = await endpoints.ensureConnected('source');
  if (!source) return null;
  const target = await endpoints.ensureConnected('target');
  if (!target) return null;
  const [sourceRows, targetRows] = await Promise.all([
    databaseCommands.listTables(source, endpoints.sourceDatabase, endpoints.sourceSchema || null, endpoints.sourceId)
      .catch((error: unknown) => { throw new SchemaObjectDiscoveryError('list-source-tables', error); }),
    databaseCommands.listTables(target, endpoints.targetDatabase, endpoints.targetSchema || null, endpoints.targetId)
      .catch((error: unknown) => { throw new SchemaObjectDiscoveryError('list-target-tables', error); }),
  ]);
  const catalog = await objects.load(source, target, endpoints.sourceDatabase, endpoints.targetDatabase,
    endpoints.sourceSchema, endpoints.targetSchema, endpoints.sourceId, endpoints.targetId)
    .catch((error: unknown) => { throw new SchemaObjectDiscoveryError('object-catalog', error); });
  // A changed endpoint invalidates this catalog; do not publish stale table picks.
  if (!catalog) return null;
  objects.restoreSelection(catalog, objects.selectedSourceObjects, objects.selectedTargetObjects);
  return mergeSchemaDiffTablePicks(sourceRows, targetRows,
    endpoints.sourceSchema || undefined, endpoints.targetSchema || undefined);
}
