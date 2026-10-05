import {
  toId,
  type BackendClient,
  type ExecutionTarget,
  type Id,
  type OwnerRef,
} from '@datazen/backend-client';
import { bindDesktopBackend } from '../../platform/tauriBackendTransport';
import { getBackendClient } from '@datazen/backend-client';
import { QueryPanelSessionRegistry } from './QueryPanelSession';

/**
 * Process-wide home for the QueryPanel session registry.
 *
 * The registry itself (`QueryPanelSessionRegistry`) is the per-editor lazy
 * session table; this module is the piece that binds it to the running app:
 * the desktop `BackendClient` (bound eagerly by the app entry or lazily here)
 * and a stable per-editor owner identity derived from the pane key.
 */

let registry: QueryPanelSessionRegistry | null = null;
let clientInstanceIdPromise: Promise<string> | null = null;

/** The bound backend facade, binding the desktop adapter on first use. */
export function queryPanelSessionClient(): BackendClient {
  return getBackendClient() ?? bindDesktopBackend();
}

/** Lazily-created process-wide registry, one controller per editor pane. */
export function getQueryPanelSessionRegistry(): QueryPanelSessionRegistry {
  if (!registry) {
    registry = new QueryPanelSessionRegistry(queryPanelSessionClient());
  }
  return registry;
}

/** The host-issued client identity, cached for the lifetime of the app. */
export async function queryPanelClientInstanceId(): Promise<string> {
  if (!clientInstanceIdPromise) {
    clientInstanceIdPromise = queryPanelSessionClient()
      .getPlatformIdentity()
      .then((identity) => {
        if (!identity.clientInstanceId) throw new Error('Missing client identity');
        return identity.clientInstanceId;
      });
  }
  return clientInstanceIdPromise;
}

/**
 * Owner identity for an editor pane. The pane key is the editor's stable
 * local owner ID, so duplicating a tab (a new pane key) automatically yields
 * a distinct owner ID and therefore a fresh runtime session.
 */
export async function editorOwnerForPane(paneKey: string): Promise<OwnerRef> {
  const clientInstanceId = await queryPanelClientInstanceId();
  return {
    kind: 'editor',
    clientInstanceId: toId(clientInstanceId) as Id,
    editorSessionId: toId(paneKey) as Id,
  };
}

/** Execution target for a panel bound to a connection + database + schema. */
export function queryPanelExecutionTarget(
  connectionId: string,
  database: string | null | undefined,
  schema: string | null | undefined,
): ExecutionTarget {
  return {
    connectionId: toId(connectionId) as ExecutionTarget['connectionId'],
    namespace: {
      database: database ?? null,
      catalog: null,
      schema: schema ?? null,
      path: [],
    },
    object: null,
  };
}
