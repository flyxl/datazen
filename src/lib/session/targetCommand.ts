import { ApiError, toId, type CommandCall, type ExecutionTarget } from '@datazen/backend-client';
import { bindSessionMetadataIdentity } from '../../stores/metadataIdentityBinding';
import { useActiveConnectionStore } from '../../stores/activeConnectionStore';
import { ExecutionProjection } from './ExecutionProjection';
import { queryPanelSessionClient } from './queryPanelSessionRoot';

/** A metadata/table operation owns a target lease, never an editor handle. */
export async function executeTargetCommand(target: ExecutionTarget, call: CommandCall, responseKind: 'commandResult' | 'rows' = 'commandResult'): Promise<unknown> {
  const client = queryPanelSessionClient();
  const profiles = await client.listConnections();
  const profile = profiles.find((entry) => entry.connectionId === target.connectionId);
  if (!profile?.enabled) throw new ApiError('NotFound', 'Connection profile unavailable');
  const token = await client.issueSubmissionToken('executeAtTarget', null);
  const receipt = await client.executeAtTarget({
    target, expectedConfigRevision: profile.configRevision, call, idempotencyKey: token.idempotencyKey,
  }, token);
  const projection = new ExecutionProjection(client, receipt, null);
  try {
    for (;;) {
      await projection.recover();
      const view = projection.view;
      if (view && ['succeeded', 'failed', 'cancelled'].includes(view.state)) {
        if (view.state !== 'succeeded') throw new Error(`${view.errorCode}; effect: ${view.effectOutcome}`);
        if (responseKind === 'rows') return projection.chunks.flatMap((chunk) => {
          const output = (chunk.payload as { output?: { kind?: string; rows?: unknown[][] } } | null)?.output;
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
    await projection.dispose();
  }
}

/** Dedicated callers supply persistent ownership; active callers can locate it by tree id. */
export function metadataTarget(
  dbSessionId: string, database: string | null, schema: string | null, table?: string,
  connectionId?: string,
): ExecutionTarget {
  const entry = connectionId === undefined
    ? Object.values(useActiveConnectionStore.getState().connections)
      .find((connection) => connection.dbSessionId === dbSessionId)
    : undefined;
  const owner = connectionId ?? entry?.connectionId;
  if (!owner) throw new ApiError('SessionLost', 'Metadata owner unavailable');
  return {
    connectionId: toId(owner) as ExecutionTarget['connectionId'],
    namespace: { database: database ?? entry?.currentDatabase ?? null, schema, catalog: null, path: [] },
    object: table ? { kind: 'relation', name: table, signature: null } : null,
  };
}

/** Database discovery belongs to the connection, before any database is selected. */
export function databaseDiscoveryTarget(dbSessionId: string, connectionId?: string): ExecutionTarget {
  const target = metadataTarget(dbSessionId, null, null, undefined, connectionId);
  return { ...target, namespace: { database: null, schema: null, catalog: null, path: [] } };
}

export async function readTargetSchema(
  dbSessionId: string, table: string, database: string, schema: string | null,
): Promise<import('../../types').TableSchema> {
  const data = await executeTargetCommand(metadataTarget(dbSessionId, database, schema, table), {
    command: 'get_table_schema', input: { table, database, schema },
  });
  const value = data as { schema?: import('../../types').TableSchema } | null;
  if (!value?.schema || !Array.isArray(value.schema.columns)) throw new Error('Invalid schema result');
  return value.schema;
}

/** Authenticated identity + actual profile revision isolate metadata snapshots. */
export async function metadataScopeKey(dbSessionId: string): Promise<string> {
  const target = metadataTarget(dbSessionId, null, null);
  const client = queryPanelSessionClient();
  const [identity, profiles] = await Promise.all([client.getPlatformIdentity(), client.listConnections()]);
  const profile = profiles.find((entry) => entry.connectionId === target.connectionId);
  if (!profile?.enabled) throw new ApiError('NotFound', 'Metadata profile unavailable');
  const scope = JSON.stringify([client.backendId, identity.organizationId, identity.principalId,
    profile.connectionId, profile.configRevision]);
  bindSessionMetadataIdentity(dbSessionId, profile.connectionId, scope);
  return scope;
}
