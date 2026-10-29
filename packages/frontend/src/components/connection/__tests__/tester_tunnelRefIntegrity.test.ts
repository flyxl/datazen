/**
 * [tester] Reference-integrity journeys for the tunnel source state machine.
 *
 * Independent verification of the saved-tunnel reference invariants, and of the
 * async-load race the happy-path suite does not exercise: an existing connection is hydrated with `tunnelId`
 * while the app-wide tunnel-summary store is still loading, has failed, or has
 * since changed.
 *
 * These tests run against the **real** `tunnelStore` (only the IPC layer is
 * mocked), so store/state-machine integration is exercised end to end.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useConnectionForm } from '../useConnectionForm';
import { useTunnelStore } from '../../../stores/tunnelStore';
import { getDriverValidator } from '../../../extensions/generated';
import type { ConnectionConfig, SavedTunnelSummary } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const { saveConnectionMock, testConnectionMock } = vi.hoisted(() => ({
  saveConnectionMock: vi.fn(),
  testConnectionMock: vi.fn(),
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    testConnection: testConnectionMock,
    saveConnection: saveConnectionMock,
  },
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: Object.assign(
    vi.fn((selector: (s: { saveConnection: typeof saveConnectionMock }) => unknown) =>
      selector({ saveConnection: saveConnectionMock }),
    ),
    { getState: () => ({ saveConnection: saveConnectionMock }) },
  ),
}));

const mockTunnelCommands = vi.hoisted(() => ({
  getTunnels: vi.fn(),
  getTunnelSummaries: vi.fn(),
  getTunnel: vi.fn(),
  getTunnelUsage: vi.fn(),
  saveTunnel: vi.fn(),
  deleteTunnel: vi.fn(),
  testTunnel: vi.fn(),
}));

vi.mock('../../../commands/tunnel', () => ({ tunnelCommands: mockTunnelCommands }));

const SSH_SUMMARY: SavedTunnelSummary = { id: 'tun_ssh', name: 'Bastion', kind: 'ssh' };

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

/** Existing connection referencing a saved tunnel, as persisted on disk. */
function editExisting(overrides: Partial<ConnectionConfig> = {}): ConnectionConfig {
  return {
    id: 'c-1',
    name: 'via-saved',
    databaseType: 'postgresql',
    host: 'db.internal',
    port: 5432,
    sslMode: 'prefer',
    tunnelId: 'tun_ssh',
    tunnelKind: 'ssh',
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useTunnelStore.setState({ summaries: [], loaded: false, loading: false, error: null });
  mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);
  mockTunnelCommands.getTunnel.mockResolvedValue(null);
});

describe('[tester] tunnel reference integrity', () => {
  it('does not flag a dangling reference while the summary load is still in flight', async () => {
    const pending = deferred<SavedTunnelSummary[]>();
    mockTunnelCommands.getTunnelSummaries.mockReturnValue(pending.promise);

    const { result } = renderHook(() =>
      useConnectionForm({ editId: 'c-1', existingConnections: [editExisting()] }),
    );

    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe('tun_ssh');
    // `savedTunnel` cannot resolve yet — but an unfinished load proves nothing,
    // so the form must not warn and must not block a legitimate save.
    expect(result.current.savedTunnel).toBeNull();
    expect(result.current.tunnelRefMissing).toBe(false);
    expect(result.current.validate()).toBe(true);

    await act(async () => {
      await result.current.onSave();
    });
    expect(saveConnectionMock).toHaveBeenCalledTimes(1);

    // Once the collection lands the reference resolves and nothing was blocked.
    await act(async () => {
      pending.resolve([SSH_SUMMARY]);
      await pending.promise;
    });
    expect(result.current.savedTunnel?.name).toBe('Bastion');
    expect(result.current.tunnelRefMissing).toBe(false);
    expect(result.current.validate()).toBe(true);
  });

  it('blocks saving once the collection loaded and the reference is absent', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);

    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-1',
        existingConnections: [editExisting({ tunnelId: 'tun_gone' })],
      }),
    );

    await act(async () => {
      await useTunnelStore.getState().load();
    });

    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe('tun_gone');
    expect(result.current.tunnelRefMissing).toBe(true);
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.tunnelId).toBe('newConn.tunnelMissing');

    await act(async () => {
      await result.current.onSave();
    });
    expect(saveConnectionMock).not.toHaveBeenCalled();
  });

  it('blocks saving after the referenced tunnel is deleted from the store', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([SSH_SUMMARY]);
    mockTunnelCommands.deleteTunnel.mockResolvedValue(undefined);

    const { result } = renderHook(() =>
      useConnectionForm({ editId: 'c-1', existingConnections: [editExisting()] }),
    );
    await act(async () => {
      await useTunnelStore.getState().load(true);
    });
    expect(result.current.savedTunnel?.name).toBe('Bastion');
    expect(result.current.validate()).toBe(true);

    // The management plane deletes the entity and refreshes the collection.
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);
    await act(async () => {
      await useTunnelStore.getState().remove('tun_ssh');
    });

    expect(useTunnelStore.getState().summaries).toEqual([]);
    expect(result.current.tunnelRefMissing).toBe(true);
    expect(result.current.validate()).toBe(false);

    await act(async () => {
      await result.current.onSave();
    });
    expect(saveConnectionMock).not.toHaveBeenCalled();
  });

  it('degrades to an empty collection on load failure without breaking a valid save', async () => {
    mockTunnelCommands.getTunnelSummaries.mockRejectedValue(new Error('ipc down'));

    const { result } = renderHook(() => useConnectionForm());
    await act(async () => {
      await useTunnelStore.getState().load();
    });

    const state = useTunnelStore.getState();
    expect(state.summaries).toEqual([]);
    expect(state.error).toBe('ipc down');
    expect(result.current.savedTunnels).toEqual([]);
    // A failed load must never be read as "the reference is dangling".
    expect(result.current.tunnelRefMissing).toBe(false);

    act(() => result.current.setName('still-savable'));
    await act(async () => {
      await result.current.onSave();
    });
    expect(saveConnectionMock).toHaveBeenCalledTimes(1);

    // Observation (reported as an improvement, not a defect gate): the first
    // settled attempt latches `loaded`, so a transient failure leaves the list
    // empty for the rest of the session with no retry path and no user-visible
    // error — `tunnelStore.error` is never rendered anywhere.
    mockTunnelCommands.getTunnelSummaries.mockClear();
    await act(async () => {
      await useTunnelStore.getState().load();
    });
    expect(mockTunnelCommands.getTunnelSummaries).not.toHaveBeenCalled();
  });

  it('cannot unbind a dangling reference; switching the source to `none` is the working exit', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);
    mockTunnelCommands.getTunnel.mockResolvedValue(null);

    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-1',
        existingConnections: [editExisting({ tunnelId: 'tun_gone' })],
      }),
    );
    await act(async () => {
      await useTunnelStore.getState().load();
    });
    expect(result.current.tunnelRefMissing).toBe(true);

    // The panel no longer advertises this action and no longer renders the
    // button in this state (tunnel-form-BUG-002); the hook-level refusal is kept
    // on purpose so a reference is never dropped silently.
    await act(async () => {
      await result.current.unbindTunnel();
    });
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe('tun_gone');
    expect(result.current.tunnelError).toBe('newConn.tunnelUnbindKept');

    // The reachable escape hatch is the source selector.
    act(() => result.current.setTunnelSource('none'));
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.tunnelRefMissing).toBe(false);
    expect(result.current.validate()).toBe(true);
  });

  it('blocks saving a dangling reference on a driver-validator form (redis) [tunnel-form-BUG-001]', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);

    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-redis',
        existingConnections: [
          editExisting({
            id: 'c-redis',
            databaseType: 'redis',
            port: 6379,
            sslMode: 'disable',
            tunnelId: 'tun_gone',
          }),
        ],
      }),
    );
    await act(async () => {
      await useTunnelStore.getState().load();
    });

    expect(result.current.formVariant).toBe('redis');
    expect(result.current.tunnelRefMissing).toBe(true);

    // G3 / P1-8: the panel warns, so `validate()` must refuse to save.
    // (Converted from `it.fails` once tunnel-form-BUG-001 was fixed; it now
    // guards the driver-validator branch against regressing.)
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.tunnelId).toBe('newConn.tunnelMissing');
    await act(async () => {
      await result.current.onSave();
    });
    expect(saveConnectionMock).not.toHaveBeenCalled();
  });
});

describe('[tester] inline tunnel hydration and validation', () => {
  it('hydrates a stored inline SSH tunnel (jump host included) into the `inline` state', () => {
    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-inline',
        existingConnections: [
          editExisting({
            id: 'c-inline',
            tunnelId: undefined,
            tunnelKind: 'ssh',
            sshTunnel: {
              enabled: true,
              host: 'bastion.internal',
              port: 2222,
              username: 'ops',
              authMethod: 'private_key',
              privateKeyPath: '/home/ops/.ssh/id_rsa',
              passphrase: 'phrase',
              jump: {
                enabled: true,
                host: 'edge.internal',
                port: 2200,
                username: 'jumper',
                authMethod: 'password',
                password: 'jpw',
              },
            },
          }),
        ],
      }),
    );

    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.sshEnabled).toBe(true);
    expect(result.current.sshHost).toBe('bastion.internal');
    expect(result.current.sshPort).toBe('2222');
    expect(result.current.sshAuthMethod).toBe('private_key');
    expect(result.current.sshKeyPath).toBe('/home/ops/.ssh/id_rsa');
    expect(result.current.sshPassphrase).toBe('phrase');
    expect(result.current.sshJumpEnabled).toBe(true);
    expect(result.current.sshJumpHost).toBe('edge.internal');
    expect(result.current.sshJumpPort).toBe('2200');
    expect(result.current.sshJumpUsername).toBe('jumper');
    expect(result.current.sshJumpPassword).toBe('jpw');
    expect(result.current.tunnelInlineValid).toBe(true);
  });

  it('requires inline SSH host and username before saving', () => {
    const { result } = renderHook(() => useConnectionForm());
    act(() => {
      result.current.setName('ssh-conn');
      result.current.setTunnelSource('inline');
      result.current.setSshEnabled(true);
      result.current.setSshHost('');
      result.current.setSshUsername('');
    });

    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.sshHost).toBe('newConn.required');
    expect(result.current.validationErrors.sshUsername).toBe('newConn.required');
  });

  it('requires a database file for a file-mode driver', () => {
    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-sqlite',
        existingConnections: [
          editExisting({
            id: 'c-sqlite',
            databaseType: 'sqlite',
            host: '',
            port: undefined,
            database: '',
            tunnelId: undefined,
            tunnelKind: 'none',
          }),
        ],
      }),
    );

    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.database).toBe('newConn.required');
  });
});

describe('[tester] inline tunnel validation is form-variant independent', () => {
  /** Inline HTTP proxy with both fields blanked, on the requested variant. */
  function renderInlineHttpProxy(databaseType: 'redis' | 'postgresql') {
    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-inline-proxy',
        existingConnections: [
          editExisting({
            id: 'c-inline-proxy',
            databaseType,
            port: databaseType === 'redis' ? 6379 : 5432,
            sslMode: 'disable',
            tunnelId: undefined,
            tunnelKind: 'httpProxy',
          }),
        ],
      }),
    );

    act(() => {
      result.current.setHttpProxyHost('');
      result.current.setHttpProxyPort('');
    });
    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.effectiveTunnelKind).toBe('httpProxy');
    return result;
  }

  it('requires inline HTTP proxy fields on a driver-validator form (redis)', () => {
    const result = renderInlineHttpProxy('redis');

    expect(result.current.formVariant).toBe('redis');
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.httpProxyHost).toBe('newConn.required');
    expect(result.current.validationErrors.httpProxyPort).toBe('newConn.required');
  });

  it('keeps the same inline requirements on a non-driver form (postgresql control)', () => {
    const result = renderInlineHttpProxy('postgresql');

    // Control: this variant has no driver validator, so the pre-existing
    // non-driver branch must still report the very same inline requirements.
    expect(getDriverValidator(result.current.formVariant)).toBeUndefined();
    act(() => {
      expect(result.current.validate()).toBe(false);
    });
    expect(result.current.validationErrors.httpProxyHost).toBe('newConn.required');
    expect(result.current.validationErrors.httpProxyPort).toBe('newConn.required');
  });

  it('does not mis-block a driver-validator form with no tunnel configured (redis)', () => {
    // Equivalence guard for the hoist: a tunnel-free Redis connection must be
    // judged by the driver validator alone, exactly as before the change.
    const { result } = renderHook(() =>
      useConnectionForm({
        editId: 'c-redis-plain',
        existingConnections: [
          editExisting({
            id: 'c-redis-plain',
            databaseType: 'redis',
            host: 'cache.internal',
            port: 6379,
            sslMode: 'disable',
            tunnelId: undefined,
            tunnelKind: 'none',
          }),
        ],
      }),
    );

    expect(result.current.formVariant).toBe('redis');
    expect(result.current.tunnelSource).toBe('none');
    act(() => {
      expect(result.current.validate()).toBe(true);
    });
    expect(result.current.validationErrors).toEqual({});

    // And a filled-in inline tunnel stays savable on the same variant.
    act(() => {
      result.current.setTunnelSource('inline');
      result.current.setTunnelKind('httpProxy');
      result.current.setHttpProxyHost('proxy.internal');
      result.current.setHttpProxyPort('8080');
    });
    act(() => {
      expect(result.current.validate()).toBe(true);
    });
    expect(result.current.validationErrors).toEqual({});
  });
});
