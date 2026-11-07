import {
  toId,
  requireBackendClient,
  type BackendClient,
  type ExecutionTarget,
  type Id,
  type OwnerRef,
} from '@datazen/backend-client';
import { QueryPanelSessionRegistry } from './QueryPanelSession';

/**
 * Process-wide home for the QueryPanel session registry.
 *
 * The registry itself (`QueryPanelSessionRegistry`) is the per-editor lazy
 * session table; this module is the piece that binds it to the running app:
 * the app assembly's `BackendClient` (bound eagerly before the first React
 * paint) and a stable per-editor owner identity derived from the pane key.
 */

let registry: QueryPanelSessionRegistry | null = null;
let clientInstanceIdPromise: Promise<string> | null = null;

/**
 * The bound backend facade (§7.4 fail-loud: an unbound backend at a data-plane
 * call site is a wiring gap — shared code never binds a host adapter as a
 * fallback; the F2 lazy `bindDesktopBackend()` leg is gone).
 */
export function queryPanelSessionClient(): BackendClient {
  return requireBackendClient();
}

/**
 * A facade that resolves the binding at first property access. The session
 * registry only reads its client when a session is OPENED (`peek` / `remove`
 * of already-live controllers are in-memory bookkeeping), so the loud §7.4
 * failure belongs at first wire use, not at registry construction — module
 * graphs that exercise only the bookkeeping (fresh `vi.resetModules()` test
 * graphs before their assembly binding) stay construction-safe. The app
 * assembly binds eagerly before first paint, so production resolves on the
 * first real call either way.
 */
function lazyBackendClient(): BackendClient {
  return new Proxy({} as BackendClient, {
    get(_target, prop) {
      const client = requireBackendClient() as unknown as Record<string | symbol, unknown>;
      const value = client[prop];
      return typeof value === 'function'
        ? (value as (...args: unknown[]) => unknown).bind(client)
        : value;
    },
  });
}

/** Lazily-created process-wide registry, one controller per editor pane. */
export function getQueryPanelSessionRegistry(): QueryPanelSessionRegistry {
  if (!registry) {
    registry = new QueryPanelSessionRegistry(lazyBackendClient());
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
