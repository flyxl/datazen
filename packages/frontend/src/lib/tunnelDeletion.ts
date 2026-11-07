import { t } from '../locales/t';
import type { ConnectionConfig } from '../types';

/**
 * Reference-integrity cleanup for `delete_tunnel`.
 *
 * The backend `delete_tunnel` only removes the entity (`retain`): it neither
 * checks nor cleans up referencing connections, so deleting a referenced tunnel
 * silently leaves dangling `tunnelId`s that hard-fail at connect time with an
 * internal error (G3). The UI therefore unbinds every referencing connection
 * *before* deleting, and aborts the delete when an unbind fails.
 *
 * Ordering and failure semantics:
 * 1. read the current connections and select the ones referencing `tunnelId`;
 * 2. save each one without the reference — on the first failure, restore the
 *    already-unbound ones and abort without deleting anything;
 * 3. only once every reference is gone, delete the tunnel.
 *
 * A failure in step 3 leaves no dangling reference (the entity simply remains,
 * unreferenced), so it is reported as a distinct outcome rather than a rollback.
 */

export interface DeleteTunnelDeps {
  getConnections: () => Promise<ConnectionConfig[]>;
  saveConnection: (config: ConnectionConfig) => Promise<void>;
  deleteTunnel: (id: string) => Promise<void>;
}

export type DeleteTunnelOutcome =
  | { kind: 'deleted'; unboundConnectionIds: string[] }
  | { kind: 'unbind-failed'; message: string; rolledBack: string[]; rollbackFailed: string[] }
  | { kind: 'delete-failed'; message: string; unboundConnectionIds: string[] };

function messageOf(e: unknown, fallback: string): string {
  if (typeof e === 'string' && e) return e;
  if (e instanceof Error && e.message) return e.message;
  return fallback;
}

/**
 * The patch applied to a referencing connection: the reference itself, plus the
 * now-meaningless `tunnelKind` hint the form persists alongside it (leaving it
 * behind makes the connection form hydrate as an empty inline tunnel of that
 * kind instead of a plain direct connection).
 */
export function unbindConnectionPatch(connection: ConnectionConfig): ConnectionConfig {
  return { ...connection, tunnelId: undefined, tunnelKind: undefined };
}

export async function deleteTunnelAndUnbind(
  tunnelId: string,
  deps: DeleteTunnelDeps,
): Promise<DeleteTunnelOutcome> {
  let connections: ConnectionConfig[];
  try {
    connections = await deps.getConnections();
  } catch (e) {
    return {
      kind: 'unbind-failed',
      message: messageOf(e, t('settings.tunnels.delete.failed')),
      rolledBack: [],
      rollbackFailed: [],
    };
  }

  const referencing = connections.filter((c) => c.tunnelId === tunnelId);
  const unbound: ConnectionConfig[] = [];

  for (const connection of referencing) {
    try {
      await deps.saveConnection(unbindConnectionPatch(connection));
      unbound.push(connection);
    } catch (e) {
      const rolledBack: string[] = [];
      const rollbackFailed: string[] = [];
      // Reverse order: undo the most recent write first.
      for (const done of [...unbound].reverse()) {
        try {
          await deps.saveConnection(done);
          rolledBack.push(done.id);
        } catch {
          rollbackFailed.push(done.id);
        }
      }
      return {
        kind: 'unbind-failed',
        message: messageOf(e, t('settings.tunnels.delete.unbindFailed')),
        rolledBack,
        rollbackFailed,
      };
    }
  }

  try {
    await deps.deleteTunnel(tunnelId);
  } catch (e) {
    return {
      kind: 'delete-failed',
      message: messageOf(e, t('settings.tunnels.delete.failed')),
      unboundConnectionIds: unbound.map((c) => c.id),
    };
  }

  return { kind: 'deleted', unboundConnectionIds: unbound.map((c) => c.id) };
}
