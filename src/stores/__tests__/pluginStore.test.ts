import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PluginSummary } from '../../types/plugin';

const mockPluginCommands = {
  listPlugins: vi.fn(),
  getPluginManifest: vi.fn(),
  installPluginFromPath: vi.fn(),
  removePlugin: vi.fn(),
  setPluginEnabled: vi.fn(),
  pluginStorageGet: vi.fn(),
  pluginStorageSet: vi.fn(),
  pluginStorageRemove: vi.fn(),
  readPluginFile: vi.fn(),
};

const listenMock = vi.hoisted(() => vi.fn());
const unlistenMock = vi.fn();

vi.mock('../../commands/plugins', () => ({
  PLUGINS_CHANGED_EVENT: 'plugins:changed',
  pluginCommands: mockPluginCommands,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: Parameters<typeof listenMock>) => listenMock(...args),
}));

function makePlugin(overrides: Partial<PluginSummary> = {}): PluginSummary {
  return {
    id: 'acme.demo',
    name: 'Demo Plugin',
    version: '1.0.0',
    apiVersion: 2,
    author: 'Acme',
    description: undefined,
    enabled: true,
    permissions: ['storage:local'],
    pages: [{ id: 'main', title: 'Main' }],
    themes: [],
    ...overrides,
  };
}

type PluginStoreModule = typeof import('../pluginStore');

describe('pluginStore', () => {
  let usePluginStore: PluginStoreModule['usePluginStore'];

  async function importStore(): Promise<PluginStoreModule> {
    const mod = await import('../pluginStore');
    usePluginStore = mod.usePluginStore;
    return mod;
  }

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    listenMock.mockReset().mockResolvedValue(unlistenMock);
    mockPluginCommands.listPlugins.mockResolvedValue([]);
    mockPluginCommands.setPluginEnabled.mockResolvedValue(undefined);
    mockPluginCommands.removePlugin.mockResolvedValue(undefined);
  });

  it('fetch success populates plugins and clears error', async () => {
    const plugins = [makePlugin(), makePlugin({ id: 'acme.midnight', enabled: false })];
    mockPluginCommands.listPlugins.mockResolvedValueOnce(plugins);

    const { ensurePluginsChangedListener } = await importStore();
    expect(usePluginStore.getState().loaded).toBe(false);

    await usePluginStore.getState().fetch();
    const state = usePluginStore.getState();
    expect(state.plugins).toEqual(plugins);
    expect(state.loaded).toBe(true);
    expect(state.error).toBeNull();
    expect(ensurePluginsChangedListener).toBeDefined();
  });

  it('fetch failure records the error and still marks loaded', async () => {
    mockPluginCommands.listPlugins.mockRejectedValueOnce(new Error('boom'));

    await importStore();
    await usePluginStore.getState().fetch();

    const state = usePluginStore.getState();
    expect(state.plugins).toEqual([]);
    expect(state.loaded).toBe(true);
    expect(state.error).toBe('boom');
  });

  it('setEnabled applies optimistically, then reconciles via refetch', async () => {
    mockPluginCommands.listPlugins
      .mockResolvedValueOnce([makePlugin()])
      .mockResolvedValueOnce([makePlugin({ enabled: false })]);

    await importStore();
    await usePluginStore.getState().fetch();

    const promise = usePluginStore.getState().setEnabled('acme.demo', false);
    // Optimistic flip happens synchronously before the IPC roundtrip.
    expect(usePluginStore.getState().plugins[0].enabled).toBe(false);
    await promise;

    expect(mockPluginCommands.setPluginEnabled).toHaveBeenCalledWith('acme.demo', false);
    expect(mockPluginCommands.listPlugins).toHaveBeenCalledTimes(2);
    expect(usePluginStore.getState().plugins[0].enabled).toBe(false);
  });

  it('setEnabled failure rolls back through refetch and rethrows', async () => {
    mockPluginCommands.listPlugins.mockResolvedValue([makePlugin()]);
    mockPluginCommands.setPluginEnabled.mockRejectedValueOnce(new Error('denied'));

    await importStore();
    await usePluginStore.getState().fetch();

    await expect(usePluginStore.getState().setEnabled('acme.demo', false)).rejects.toThrow(
      'denied',
    );

    const state = usePluginStore.getState();
    expect(state.plugins[0].enabled).toBe(true);
    expect(state.error).toBe('denied');
  });

  it('remove calls backend and refreshes the list', async () => {
    mockPluginCommands.listPlugins.mockResolvedValueOnce([makePlugin()]).mockResolvedValueOnce([]);

    await importStore();
    await usePluginStore.getState().fetch();
    await usePluginStore.getState().remove('acme.demo');

    expect(mockPluginCommands.removePlugin).toHaveBeenCalledWith('acme.demo');
    expect(usePluginStore.getState().plugins).toEqual([]);
    expect(usePluginStore.getState().error).toBeNull();
  });

  it('remove failure sets the error and rethrows', async () => {
    mockPluginCommands.removePlugin.mockRejectedValueOnce(new Error('busy'));
    await importStore();

    await expect(usePluginStore.getState().remove('x')).rejects.toThrow('busy');
    expect(usePluginStore.getState().error).toBe('busy');
  });

  it('setEnabled optimistic flip leaves other plugins untouched', async () => {
    const other = makePlugin({ id: 'acme.midnight', enabled: true });
    mockPluginCommands.listPlugins
      .mockResolvedValueOnce([makePlugin(), other])
      .mockResolvedValueOnce([makePlugin({ enabled: false }), other]);
    await importStore();
    await usePluginStore.getState().fetch();

    await usePluginStore.getState().setEnabled('acme.demo', false);

    const state = usePluginStore.getState();
    expect(state.plugins.find((p) => p.id === 'acme.demo')?.enabled).toBe(false);
    expect(state.plugins.find((p) => p.id === 'acme.midnight')?.enabled).toBe(true);
    expect(state.error).toBeNull();
  });

  it('byId finds installed plugins by manifest id', async () => {
    mockPluginCommands.listPlugins.mockResolvedValueOnce([
      makePlugin(),
      makePlugin({ id: 'acme.midnight' }),
    ]);
    await importStore();
    await usePluginStore.getState().fetch();

    expect(usePluginStore.getState().byId('acme.midnight')?.name).toBe('Demo Plugin');
    expect(usePluginStore.getState().byId('missing')).toBeUndefined();
  });

  it('subscribes to plugins:changed once and refetches on event', async () => {
    await importStore();

    expect(listenMock).toHaveBeenCalledTimes(1);
    expect(listenMock).toHaveBeenCalledWith('plugins:changed', expect.any(Function));

    // Re-importing must not double-subscribe (module-level flag).
    await importStore();
    expect(listenMock).toHaveBeenCalledTimes(1);

    const handler = listenMock.mock.calls[0][1] as () => void;
    handler();
    await Promise.resolve();
    expect(mockPluginCommands.listPlugins).toHaveBeenCalled();
  });

  // --- F3 supplementary (test agent) ---

  it('fetch failure surfaces non-Error rejection strings verbatim', async () => {
    mockPluginCommands.listPlugins.mockRejectedValueOnce('ipc unavailable');
    await importStore();
    await usePluginStore.getState().fetch();
    expect(usePluginStore.getState().error).toBe('ipc unavailable');
  });

  it('plugins:changed refetch swaps in fresh data once the event fires', async () => {
    await importStore();
    const handler = listenMock.mock.calls[0][1] as () => void;

    mockPluginCommands.listPlugins.mockResolvedValueOnce([makePlugin({ id: 'acme.fresh' })]);
    handler();

    await vi.waitFor(() => {
      expect(usePluginStore.getState().plugins.map((p) => p.id)).toEqual(['acme.fresh']);
    });
    expect(usePluginStore.getState().error).toBeNull();
  });

  it('retries the plugins:changed subscription after a failed attempt', async () => {
    listenMock.mockRejectedValueOnce(new Error('outside tauri runtime'));
    const { ensurePluginsChangedListener } = await importStore();

    // Import-time subscribe rejected; flush tasks so the catch resets the guard.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(listenMock).toHaveBeenCalledTimes(1);
    expect(listenMock.mock.calls[0][0]).toBe('plugins:changed');

    ensurePluginsChangedListener(); // guard was reset → subscribes again
    expect(listenMock).toHaveBeenCalledTimes(2);
  });
});
