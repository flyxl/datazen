import type { TargetCommandPhaseHook } from './targetCommandPhase';
import { ApiError, toId, type CommandCall, type ExecutionTarget } from '@datazen/backend-client';
import { bindSessionMetadataIdentity } from '../../stores/metadataIdentityBinding';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { ExecutionProjection } from './ExecutionProjection';
import { queryPanelSessionClient } from './queryPanelSessionRoot';

/** A metadata/table operation owns a target lease, never an editor handle. */
export async function executeTargetCommand(
  target: ExecutionTarget,
  call: CommandCall,
  responseKind: 'commandResult' | 'rows' | 'completion' = 'commandResult',
  onPhase?: TargetCommandPhaseHook,
): Promise<unknown> {
  const client = queryPanelSessionClient();
  onPhase?.('profiles');
  const profiles = await client.listConnections();
  const profile = profiles.find((entry) => entry.connectionId === target.connectionId);
  if (!profile?.enabled) throw new ApiError('NotFound', 'Connection profile unavailable');
  onPhase?.('submission-token');
  const token = await client.issueSubmissionToken('executeAtTarget', null);
  onPhase?.('target-admission');
  const request = {
    target,
    expectedConfigRevision: profile.configRevision,
    call,
    idempotencyKey: token.idempotencyKey,
  };
  const admit = async () => {
    // Multi-connection resource claims are atomic and cannot wait in the
    // backend's single-slot queue. Concurrent metadata reads (including Pro
    // lint/hover enrichment) can therefore briefly exhaust the physical budget.
    // Retry only admission before a receipt exists, within the same 10-second
    // acquisition bound as the backend. Reuse the exact request/token so even
    // an ambiguous admission cannot create or execute a second operation.
    const deadline = performance.now() + 10_000;
    for (;;) {
      try {
        return await client.executeAtTarget(request, token);
      } catch (error) {
        if (
          !(error instanceof ApiError) ||
          error.code !== 'ResourceBusy' ||
          performance.now() >= deadline
        )
          throw error;
        await new Promise<void>((resolve) => setTimeout(resolve, 100));
      }
    }
  };
  const receipt = await admit();
  const projection = new ExecutionProjection(client, receipt, null, undefined, onPhase);
  try {
    for (;;) {
      await projection.recover();
      const view = projection.view;
      if (view && ['succeeded', 'failed', 'cancelled'].includes(view.state)) {
        if (view.state !== 'succeeded')
          throw new Error(`${view.errorCode}; effect: ${view.effectOutcome}`);
        if (responseKind === 'completion') return;
        if (responseKind === 'rows')
          return projection.chunks.flatMap((chunk) => {
            const output = (
              chunk.payload as { output?: { kind?: string; rows?: unknown[][] } } | null
            )?.output;
            return output?.kind === 'rows' && Array.isArray(output.rows) ? output.rows : [];
          });
        for (const chunk of projection.chunks) {
          const payload = chunk.payload as { output?: { kind?: string; data?: unknown } } | null;
          if (payload?.output?.kind === 'commandResult') return payload.output.data;
        }
        throw new Error('Typed command result unavailable');
      }
      await new Promise<void>((resolve) => setTimeout(resolve, 50));
    }
  } finally {
    try {
      await projection.dispose();
    } catch (error) {
      // Successful disposal must not overwrite the phase of the original failure.
      onPhase?.('dispose');
      throw error;
    }
  }
}

/** Dedicated callers supply persistent ownership; active callers can locate it by tree id. */
export function metadataTarget(
  dbSessionId: string,
  database: string | null,
  schema: string | null,
  table?: string,
  connectionId?: string,
): ExecutionTarget {
  const entry =
    connectionId === undefined
      ? Object.values(useActiveConnectionStore.getState().connections).find(
          (connection) => connection.dbSessionId === dbSessionId,
        )
      : undefined;
  const owner = connectionId ?? entry?.connectionId;
  if (!owner) throw new ApiError('SessionLost', 'Metadata owner unavailable');
  return {
    connectionId: toId(owner) as ExecutionTarget['connectionId'],
    namespace: {
      database: database ?? entry?.currentDatabase ?? null,
      schema,
      catalog: null,
      path: [],
    },
    object: table ? { kind: 'relation', name: table, signature: null } : null,
  };
}

/** Database discovery belongs to the connection, before any database is selected. */
export function databaseDiscoveryTarget(
  dbSessionId: string,
  connectionId?: string,
): ExecutionTarget {
  const target = metadataTarget(dbSessionId, null, null, undefined, connectionId);
  return { ...target, namespace: { database: null, schema: null, catalog: null, path: [] } };
}

export async function readTargetSchema(
  dbSessionId: string,
  table: string,
  database: string,
  schema: string | null,
  connectionId?: string,
): Promise<import('../../types').TableSchema> {
  const data = await executeTargetCommand(
    metadataTarget(dbSessionId, database, schema, table, connectionId),
    {
      command: 'get_table_schema',
      input: { table, database, schema },
    },
  );
  const value = data as { schema?: import('../../types').TableSchema } | null;
  if (!value?.schema || !Array.isArray(value.schema.columns))
    throw new Error('Invalid schema result');
  return value.schema;
}

/** Authenticated identity + actual profile revision isolate metadata snapshots. */
export async function metadataScopeKey(
  dbSessionId: string,
  connectionId?: string,
): Promise<string> {
  const target = metadataTarget(dbSessionId, null, null, undefined, connectionId);
  const client = queryPanelSessionClient();
  const [identity, profiles] = await Promise.all([
    client.getPlatformIdentity(),
    client.listConnections(),
  ]);
  const profile = profiles.find((entry) => entry.connectionId === target.connectionId);
  if (!profile?.enabled) throw new ApiError('NotFound', 'Metadata profile unavailable');
  const scope = JSON.stringify([
    client.backendId,
    identity.organizationId,
    identity.principalId,
    profile.connectionId,
    profile.configRevision,
  ]);
  bindSessionMetadataIdentity(dbSessionId, profile.connectionId, scope);
  return scope;
}
