import { describe, expect, it, vi, beforeEach } from 'vitest';

/**
 * The QueryPanel session registry is process-wide and keyed by pane key; these
 * tests pin the two facts the rest of the query panel depends on: a duplicated
 * tab (a new pane key) gets its own owner identity and therefore its own
 * runtime session, and the registry is a singleton so every pane shares one
 * backend client.
 */

const getPlatformIdentity = vi.hoisted(() => vi.fn());
const registryCtor = vi.hoisted(() => vi.fn());

vi.mock('@datazen/backend-client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/backend-client')>()),
  getBackendClient: () => ({
    getPlatformIdentity: (...args: unknown[]) => getPlatformIdentity(...args),
  }),
}));

vi.mock('../../../platform/tauriBackendTransport', () => ({
  bindDesktopBackend: () => {
    throw new Error('desktop backend should not be bound when a client already exists');
  },
}));

vi.mock('../QueryPanelSession', () => ({
  QueryPanelSessionRegistry: class {
    remove = vi.fn();
    constructor(client: unknown) {
      registryCtor(client);
    }
  },
}));

type SessionRoot = typeof import('../queryPanelSessionRoot');

/** A fresh module instance per test: the identity promise is cached on purpose. */
async function loadSessionRoot(): Promise<SessionRoot> {
  vi.resetModules();
  return import('../queryPanelSessionRoot');
}

describe('[tester] query panel session root', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getPlatformIdentity.mockResolvedValue({ clientInstanceId: 'client-42' });
  });

  it('fails loudly when the host issues no client identity', async () => {
    getPlatformIdentity.mockResolvedValue({ clientInstanceId: '' });
    const { queryPanelClientInstanceId } = await loadSessionRoot();
    await expect(queryPanelClientInstanceId()).rejects.toThrow('Missing client identity');
  });

  it('mints one editor owner per pane key from a single client identity', async () => {
    const { editorOwnerForPane } = await loadSessionRoot();
    const owner = await editorOwnerForPane('panel-query-1::pane-b');
    expect(owner).toEqual({
      kind: 'editor',
      clientInstanceId: 'client-42',
      editorSessionId: 'panel-query-1::pane-b',
    });

    const sibling = await editorOwnerForPane('panel-query-1');
    expect(sibling).toEqual({
      kind: 'editor',
      clientInstanceId: 'client-42',
      editorSessionId: 'panel-query-1',
    });
    // The host identity is fetched once for the whole app process.
    expect(getPlatformIdentity).toHaveBeenCalledTimes(1);
  });

  it('builds an execution target with a normalized namespace', async () => {
    const { queryPanelExecutionTarget } = await loadSessionRoot();
    expect(queryPanelExecutionTarget('cfg-1', 'app', 'public')).toEqual({
      connectionId: 'cfg-1',
      namespace: { database: 'app', catalog: null, schema: 'public', path: [] },
      object: null,
    });
    expect(queryPanelExecutionTarget('cfg-1', undefined, null).namespace).toEqual({
      database: null,
      catalog: null,
      schema: null,
      path: [],
    });
  });

  it('keeps the registry a process-wide singleton on the bound client', async () => {
    const { getQueryPanelSessionRegistry } = await loadSessionRoot();
    const registry = getQueryPanelSessionRegistry();
    expect(getQueryPanelSessionRegistry()).toBe(registry);
    expect(registryCtor).toHaveBeenCalledTimes(1);
  });
});