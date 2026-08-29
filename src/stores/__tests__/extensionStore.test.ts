import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ExtensionSummary } from '../../types/extension';

const mockExtensionCommands = {
  listExtensions: vi.fn(),
  getExtensionManifest: vi.fn(),
  installExtension: vi.fn(),
  removeExtension: vi.fn(),
  setExtensionEnabled: vi.fn(),
  extensionStorageGet: vi.fn(),
  extensionStorageSet: vi.fn(),
  extensionStorageRemove: vi.fn(),
  readExtensionFile: vi.fn(),
};

const listenMock = vi.hoisted(() => vi.fn());
const unlistenMock = vi.fn();

vi.mock('../../commands/extensions', () => ({
  EXTENSIONS_CHANGED_EVENT: 'plugins:changed',
  extensionCommands: mockExtensionCommands,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: Parameters<typeof listenMock>) => listenMock(...args),
}));

function makePlugin(overrides: Partial<ExtensionSummary> = {}): ExtensionSummary {
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

type PluginStoreModule = typeof import('../extensionStore');

describe('extensionStore', () => {
  let useExtensionStore: PluginStoreModule['useExtensionStore'];

  async function importStore(): Promise<PluginStoreModule> {
    const mod = await import('../extensionStore');
    useExtensionStore = mod.useExtensionStore;
    return mod;
  }

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    listenMock.mockReset().mockResolvedValue(unlistenMock);
    mockExtensionCommands.listExtensions.mockResolvedValue([]);
    mockExtensionCommands.setExtensionEnabled.mockResolvedValue(undefined);
    mockExtensionCommands.removeExtension.mockResolvedValue(undefined);
  });

  it('fetch success populates plugins and clears error', async () => {
    const plugins = [makePlugin(), makePlugin({ id: 'acme.midnight', enabled: false })];
    mockExtensionCommands.listExtensions.mockResolvedValueOnce(plugins);

    const { ensureExtensionsChangedListener } = await importStore();
    expect(useExtensionStore.getState().loaded).toBe(false);

    await useExtensionStore.getState().fetch();
    const state = useExtensionStore.getState();
    expect(state.extensions).toEqual(plugins);
    expect(state.loaded).toBe(true);
    expect(state.error).toBeNull();
    expect(ensureExtensionsChangedListener).toBeDefined();
  });

  it('fetch failure records the error and still marks loaded', async () => {
    mockExtensionCommands.listExtensions.mockRejectedValueOnce(new Error('boom'));

    await importStore();
    await useExtensionStore.getState().fetch();

    const state = useExtensionStore.getState();
    expect(state.extensions).toEqual([]);
    expect(state.loaded).toBe(true);
    expect(state.error).toBe('boom');
  });

  it('setEnabled applies optimistically, then reconciles via refetch', async () => {
    mockExtensionCommands.listExtensions
      .mockResolvedValueOnce([makePlugin()])
      .mockResolvedValueOnce([makePlugin({ enabled: false })]);

    await importStore();
    await useExtensionStore.getState().fetch();

    const promise = useExtensionStore.getState().setEnabled('acme.demo', false);
    // Optimistic flip happens synchronously before the IPC roundtrip.
    expect(useExtensionStore.getState().extensions[0].enabled).toBe(false);
    await promise;

    expect(mockExtensionCommands.setExtensionEnabled).toHaveBeenCalledWith('acme.demo', false);
    expect(mockExtensionCommands.listExtensions).toHaveBeenCalledTimes(2);
    expect(useExtensionStore.getState().extensions[0].enabled).toBe(false);
  });

  it('setEnabled failure rolls back through refetch and rethrows', async () => {
    mockExtensionCommands.listExtensions.mockResolvedValue([makePlugin()]);
    mockExtensionCommands.setExtensionEnabled.mockRejectedValueOnce(new Error('denied'));

    await importStore();
    await useExtensionStore.getState().fetch();

    await expect(useExtensionStore.getState().setEnabled('acme.demo', false)).rejects.toThrow(
      'denied',
    );

    const state = useExtensionStore.getState();
    expect(state.extensions[0].enabled).toBe(true);
    expect(state.error).toBe('denied');
  });

  it('remove calls backend and refreshes the list', async () => {
    mockExtensionCommands.listExtensions
      .mockResolvedValueOnce([makePlugin()])
      .mockResolvedValueOnce([]);

    await importStore();
    await useExtensionStore.getState().fetch();
    await useExtensionStore.getState().remove('acme.demo');

    expect(mockExtensionCommands.removeExtension).toHaveBeenCalledWith('acme.demo');
    expect(useExtensionStore.getState().extensions).toEqual([]);
    expect(useExtensionStore.getState().error).toBeNull();
  });

  it('remove failure sets the error and rethrows', async () => {
    mockExtensionCommands.removeExtension.mockRejectedValueOnce(new Error('busy'));
    await importStore();

    await expect(useExtensionStore.getState().remove('x')).rejects.toThrow('busy');
    expect(useExtensionStore.getState().error).toBe('busy');
  });

  it('setEnabled optimistic flip leaves other plugins untouched', async () => {
    const other = makePlugin({ id: 'acme.midnight', enabled: true });
    mockExtensionCommands.listExtensions
      .mockResolvedValueOnce([makePlugin(), other])
      .mockResolvedValueOnce([makePlugin({ enabled: false }), other]);
    await importStore();
    await useExtensionStore.getState().fetch();

    await useExtensionStore.getState().setEnabled('acme.demo', false);

    const state = useExtensionStore.getState();
    expect(state.extensions.find((p) => p.id === 'acme.demo')?.enabled).toBe(false);
    expect(state.extensions.find((p) => p.id === 'acme.midnight')?.enabled).toBe(true);
    expect(state.error).toBeNull();
  });

  it('byId finds installed plugins by manifest id', async () => {
    mockExtensionCommands.listExtensions.mockResolvedValueOnce([
      makePlugin(),
      makePlugin({ id: 'acme.midnight' }),
    ]);
    await importStore();
    await useExtensionStore.getState().fetch();

    expect(useExtensionStore.getState().byId('acme.midnight')?.name).toBe('Demo Plugin');
    expect(useExtensionStore.getState().byId('missing')).toBeUndefined();
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
    expect(mockExtensionCommands.listExtensions).toHaveBeenCalled();
  });

  // --- F3 supplementary (test agent) ---

  it('fetch failure surfaces non-Error rejection strings verbatim', async () => {
    mockExtensionCommands.listExtensions.mockRejectedValueOnce('ipc unavailable');
    await importStore();
    await useExtensionStore.getState().fetch();
    expect(useExtensionStore.getState().error).toBe('ipc unavailable');
  });

  it('plugins:changed refetch swaps in fresh data once the event fires', async () => {
    await importStore();
    const handler = listenMock.mock.calls[0][1] as () => void;

    mockExtensionCommands.listExtensions.mockResolvedValueOnce([makePlugin({ id: 'acme.fresh' })]);
    handler();

    await vi.waitFor(() => {
      expect(useExtensionStore.getState().extensions.map((p) => p.id)).toEqual(['acme.fresh']);
    });
    expect(useExtensionStore.getState().error).toBeNull();
  });

  it('retries the plugins:changed subscription after a failed attempt', async () => {
    listenMock.mockRejectedValueOnce(new Error('outside tauri runtime'));
    const { ensureExtensionsChangedListener } = await importStore();

    // Import-time subscribe rejected; flush tasks so the catch resets the guard.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(listenMock).toHaveBeenCalledTimes(1);
    expect(listenMock.mock.calls[0][0]).toBe('plugins:changed');

    ensureExtensionsChangedListener(); // guard was reset → subscribes again
    expect(listenMock).toHaveBeenCalledTimes(2);
  });
});
