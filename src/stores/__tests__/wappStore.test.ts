import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { WappSummary } from '../../types/wapp';

const mockWappCommands = {
  listWapps: vi.fn(),
  getWappManifest: vi.fn(),
  installWapp: vi.fn(),
  removeWapp: vi.fn(),
  setWappEnabled: vi.fn(),
  wappStorageGet: vi.fn(),
  wappStorageSet: vi.fn(),
  wappStorageRemove: vi.fn(),
  readWappFile: vi.fn(),
};

const listenMock = vi.hoisted(() => vi.fn());
const unlistenMock = vi.fn();

vi.mock('../../commands/wapps', () => ({
  WAPPS_CHANGED_EVENT: 'wapps:changed',
  wappCommands: mockWappCommands,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: Parameters<typeof listenMock>) => listenMock(...args),
}));

function makeWapp(overrides: Partial<WappSummary> = {}): WappSummary {
  return {
    id: 'acme.demo',
    name: 'Demo Wapp',
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

type WappStoreModule = typeof import('../wappStore');

describe('wappStore', () => {
  let useWappStore: WappStoreModule['useWappStore'];

  async function importStore(): Promise<WappStoreModule> {
    const mod = await import('../wappStore');
    useWappStore = mod.useWappStore;
    return mod;
  }

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    listenMock.mockReset().mockResolvedValue(unlistenMock);
    mockWappCommands.listWapps.mockResolvedValue([]);
    mockWappCommands.setWappEnabled.mockResolvedValue(undefined);
    mockWappCommands.removeWapp.mockResolvedValue(undefined);
  });

  it('fetch success populates wapps and clears error', async () => {
    const wapps = [makeWapp(), makeWapp({ id: 'acme.midnight', enabled: false })];
    mockWappCommands.listWapps.mockResolvedValueOnce(wapps);

    const { ensureWappsChangedListener } = await importStore();
    expect(useWappStore.getState().loaded).toBe(false);

    await useWappStore.getState().fetch();
    const state = useWappStore.getState();
    expect(state.wapps).toEqual(wapps);
    expect(state.loaded).toBe(true);
    expect(state.error).toBeNull();
    expect(ensureWappsChangedListener).toBeDefined();
  });

  it('fetch failure records the error and still marks loaded', async () => {
    mockWappCommands.listWapps.mockRejectedValueOnce(new Error('boom'));

    await importStore();
    await useWappStore.getState().fetch();

    const state = useWappStore.getState();
    expect(state.wapps).toEqual([]);
    expect(state.loaded).toBe(true);
    expect(state.error).toBe('boom');
  });

  it('setEnabled applies optimistically, then reconciles via refetch', async () => {
    mockWappCommands.listWapps
      .mockResolvedValueOnce([makeWapp()])
      .mockResolvedValueOnce([makeWapp({ enabled: false })]);

    await importStore();
    await useWappStore.getState().fetch();

    const promise = useWappStore.getState().setEnabled('acme.demo', false);
    // Optimistic flip happens synchronously before the IPC roundtrip.
    expect(useWappStore.getState().wapps[0].enabled).toBe(false);
    await promise;

    expect(mockWappCommands.setWappEnabled).toHaveBeenCalledWith('acme.demo', false);
    expect(mockWappCommands.listWapps).toHaveBeenCalledTimes(2);
    expect(useWappStore.getState().wapps[0].enabled).toBe(false);
  });

  it('setEnabled failure rolls back through refetch and rethrows', async () => {
    mockWappCommands.listWapps.mockResolvedValue([makeWapp()]);
    mockWappCommands.setWappEnabled.mockRejectedValueOnce(new Error('denied'));

    await importStore();
    await useWappStore.getState().fetch();

    await expect(useWappStore.getState().setEnabled('acme.demo', false)).rejects.toThrow('denied');

    const state = useWappStore.getState();
    expect(state.wapps[0].enabled).toBe(true);
    expect(state.error).toBe('denied');
  });

  it('remove calls backend and refreshes the list', async () => {
    mockWappCommands.listWapps.mockResolvedValueOnce([makeWapp()]).mockResolvedValueOnce([]);

    await importStore();
    await useWappStore.getState().fetch();
    await useWappStore.getState().remove('acme.demo');

    expect(mockWappCommands.removeWapp).toHaveBeenCalledWith('acme.demo');
    expect(useWappStore.getState().wapps).toEqual([]);
    expect(useWappStore.getState().error).toBeNull();
  });

  it('remove failure sets the error and rethrows', async () => {
    mockWappCommands.removeWapp.mockRejectedValueOnce(new Error('busy'));
    await importStore();

    await expect(useWappStore.getState().remove('x')).rejects.toThrow('busy');
    expect(useWappStore.getState().error).toBe('busy');
  });

  it('setEnabled optimistic flip leaves other wapps untouched', async () => {
    const other = makeWapp({ id: 'acme.midnight', enabled: true });
    mockWappCommands.listWapps
      .mockResolvedValueOnce([makeWapp(), other])
      .mockResolvedValueOnce([makeWapp({ enabled: false }), other]);
    await importStore();
    await useWappStore.getState().fetch();

    await useWappStore.getState().setEnabled('acme.demo', false);

    const state = useWappStore.getState();
    expect(state.wapps.find((p) => p.id === 'acme.demo')?.enabled).toBe(false);
    expect(state.wapps.find((p) => p.id === 'acme.midnight')?.enabled).toBe(true);
    expect(state.error).toBeNull();
  });

  it('byId finds installed wapps by manifest id', async () => {
    mockWappCommands.listWapps.mockResolvedValueOnce([
      makeWapp(),
      makeWapp({ id: 'acme.midnight' }),
    ]);
    await importStore();
    await useWappStore.getState().fetch();

    expect(useWappStore.getState().byId('acme.midnight')?.name).toBe('Demo Wapp');
    expect(useWappStore.getState().byId('missing')).toBeUndefined();
  });

  it('subscribes to wapps:changed once and refetches on event', async () => {
    await importStore();

    expect(listenMock).toHaveBeenCalledTimes(1);
    expect(listenMock).toHaveBeenCalledWith('wapps:changed', expect.any(Function));

    // Re-importing must not double-subscribe (module-level flag).
    await importStore();
    expect(listenMock).toHaveBeenCalledTimes(1);

    const handler = listenMock.mock.calls[0][1] as () => void;
    handler();
    await Promise.resolve();
    expect(mockWappCommands.listWapps).toHaveBeenCalled();
  });

  it('fetch failure surfaces non-Error rejection strings verbatim', async () => {
    mockWappCommands.listWapps.mockRejectedValueOnce('ipc unavailable');
    await importStore();
    await useWappStore.getState().fetch();
    expect(useWappStore.getState().error).toBe('ipc unavailable');
  });

  it('wapps:changed refetch swaps in fresh data once the event fires', async () => {
    await importStore();
    const handler = listenMock.mock.calls[0][1] as () => void;

    mockWappCommands.listWapps.mockResolvedValueOnce([makeWapp({ id: 'acme.fresh' })]);
    handler();

    await vi.waitFor(() => {
      expect(useWappStore.getState().wapps.map((p) => p.id)).toEqual(['acme.fresh']);
    });
    expect(useWappStore.getState().error).toBeNull();
  });

  it('retries the wapps:changed subscription after a failed attempt', async () => {
    listenMock.mockRejectedValueOnce(new Error('outside tauri runtime'));
    const { ensureWappsChangedListener } = await importStore();

    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(listenMock).toHaveBeenCalledTimes(1);
    expect(listenMock.mock.calls[0][0]).toBe('wapps:changed');

    ensureWappsChangedListener();
    expect(listenMock).toHaveBeenCalledTimes(2);
  });
});
