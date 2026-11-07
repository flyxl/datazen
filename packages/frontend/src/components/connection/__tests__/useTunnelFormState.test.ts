import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useTunnelFormState } from '../useTunnelFormState';
import { useTunnelStore } from '../../../stores/tunnelStore';
import type { SavedTunnel, SavedTunnelSummary } from '../../../types';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
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

/** In-memory fake of `tunnels.json` so the journey exercises the real store. */
let backend: SavedTunnel[] = [];

function resetStore() {
  useTunnelStore.setState({ summaries: [], loaded: false, loading: false, error: null });
}

/** Deterministically settle the store's initial load (the hook fires it on mount). */
async function flush() {
  await act(async () => {
    await useTunnelStore.getState().load();
  });
}

/** Flush a fire-and-forget async transition (unbind chains several microtasks). */
async function actAndFlush(run: () => void) {
  await act(async () => {
    run();
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

/**
 * Re-widens a `let` that was assigned inside an `act()` callback: control-flow
 * analysis cannot see the callback's assignment, so it keeps narrowing the
 * variable to its initial `null` and every property read below collapses to
 * `never`. Identity at runtime — this only restores the declared type.
 */
function assigned<T>(value: T | null): T | null {
  return value;
}

describe('useTunnelFormState', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    backend = [];
    resetStore();
    mockTunnelCommands.saveTunnel.mockImplementation(async (tunnel: SavedTunnel) => {
      backend = [...backend.filter((t) => t.id !== tunnel.id), tunnel];
    });
    mockTunnelCommands.deleteTunnel.mockImplementation(async (id: string) => {
      backend = backend.filter((t) => t.id !== id);
    });
    mockTunnelCommands.getTunnelSummaries.mockImplementation(
      async (): Promise<SavedTunnelSummary[]> =>
        backend.map(({ id, name, kind }) => ({ id, name, kind })),
    );
    mockTunnelCommands.getTunnel.mockImplementation(
      async (id: string): Promise<SavedTunnel | null> => backend.find((t) => t.id === id) ?? null,
    );
  });

  it('starts in `none` and refuses to enter `saved` while the collection is empty', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    expect(result.current.tunnelSource).toBe('none');
    expect(result.current.tunnelKind).toBe('none');
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.savedTunnels).toEqual([]);
    expect(result.current.tunnelInlineValid).toBe(false);

    // Entry condition for `saved` is an existing candidate — stay in `none`.
    act(() => result.current.setTunnelSource('saved'));
    expect(result.current.tunnelSource).toBe('none');
  });

  it('journeys none → inline → saved → inline(unbind refills) → none without losing parameters', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    // ── none → inline (defaults to SSH when the driver supports it) ──
    act(() => result.current.setTunnelSource('inline'));
    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.tunnelKind).toBe('ssh');
    expect(result.current.sshEnabled).toBe(true);

    // ── incomplete intermediate state: host only is not savable yet ──
    act(() => result.current.setSshHost('bastion.example.com'));
    expect(result.current.tunnelInlineValid).toBe(false);
    act(() => result.current.setSshUsername('ops'));
    act(() => result.current.setSshAuthMethod('private_key'));
    act(() => result.current.setSshKeyPath('/home/ops/.ssh/id_rsa'));
    expect(result.current.tunnelInlineValid).toBe(true);

    // ── inline sub-kind switch keeps the source inline and never touches tunnelId ──
    act(() => result.current.setTunnelKind('httpProxy'));
    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.tunnelInlineValid).toBe(false);
    act(() => result.current.setHttpProxyHost('proxy.corp.example'));
    act(() => result.current.setHttpProxyPort('3128'));
    expect(result.current.tunnelInlineValid).toBe(true);

    // ── inline → saved via "save as tunnel" ──
    let created: SavedTunnel | null = null;
    await act(async () => {
      created = await result.current.saveAsTunnel('Corp proxy');
    });
    const createdTunnel = assigned<SavedTunnel>(created);
    if (!createdTunnel) throw new Error('expected saveAsTunnel to create a tunnel');
    expect(createdTunnel.kind).toBe('httpProxy');
    expect(createdTunnel.httpProxy?.host).toBe('proxy.corp.example');
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe(createdTunnel.id);
    expect(result.current.savedTunnel?.name).toBe('Corp proxy');
    expect(result.current.tunnelRefMissing).toBe(false);

    // ── saved → inline via unbind: parameters are refilled, not dropped ──
    await act(async () => {
      await result.current.unbindTunnel();
    });
    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.tunnelKind).toBe('httpProxy');
    expect(result.current.httpProxyHost).toBe('proxy.corp.example');
    expect(result.current.httpProxyPort).toBe('3128');
    expect(result.current.tunnelInlineValid).toBe(true);

    // ── inline → none ──
    act(() => result.current.setTunnelSource('none'));
    expect(result.current.tunnelSource).toBe('none');
    expect(result.current.tunnelKind).toBe('none');
    expect(result.current.sshEnabled).toBe(false);
    expect(result.current.effectiveTunnelKind).toBe('none');
  });

  it('re-entering `inline` restores the last inline kind instead of resetting to the default', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('inline'));
    act(() => result.current.setTunnelKind('websocket'));
    act(() => result.current.setTunnelSource('none'));
    expect(result.current.tunnelKind).toBe('none');

    act(() => result.current.setTunnelSource('inline'));
    expect(result.current.tunnelKind).toBe('websocket');
  });

  it('changing the kind while `saved` unbinds explicitly instead of silently dropping the reference', async () => {
    backend = [
      {
        id: 'tun_ssh',
        name: 'Bastion',
        kind: 'ssh',
        ssh: {
          enabled: true,
          host: 'bastion.example.com',
          port: 2222,
          username: 'ops',
          authMethod: 'password',
          password: 'secret',
        },
      },
    ];
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('saved'));
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe('tun_ssh');
    expect(result.current.tunnelKind).toBe('ssh');

    // Switching the sub-kind is an explicit exit from `saved`.
    await actAndFlush(() => result.current.setTunnelKind('websocket'));
    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.tunnelKind).toBe('websocket');
    // The SSH parameters were refilled on the way out (no silent loss).
    expect(result.current.sshHost).toBe('bastion.example.com');
    expect(result.current.sshPort).toBe('2222');
    expect(result.current.sshUsername).toBe('ops');
    expect(result.current.sshPassword).toBe('secret');
  });

  it('surfaces a dangling tunnelId once loaded and blocks the inline save button', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.hydrateTunnelRef('tun_gone', 'ssh'));
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe('tun_gone');
    expect(result.current.savedTunnel).toBeNull();
    expect(result.current.tunnelRefMissing).toBe(true);
    expect(result.current.tunnelInlineValid).toBe(false);
  });

  it('does not flag a dangling reference when the summary load itself failed', async () => {
    // A failed load proves nothing: blocking save here would misfire on valid connections.
    useTunnelStore.setState({ summaries: [], loaded: true, error: 'ipc down' });
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.hydrateTunnelRef('tun_gone', 'ssh'));
    expect(result.current.tunnelId).toBe('tun_gone');
    expect(result.current.tunnelRefMissing).toBe(false);
  });

  it('keeps the reference when the saved entity cannot be read (unbind must not silently drop)', async () => {
    backend = [
      {
        id: 'tun_ws',
        name: 'Relay',
        kind: 'websocket',
        websocket: { enabled: true, url: 'wss://r' },
      },
    ];
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('saved'));
    mockTunnelCommands.getTunnel.mockResolvedValue(null);

    await act(async () => {
      await result.current.unbindTunnel();
    });
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe('tun_ws');
    expect(result.current.tunnelError).toBe('newConn.tunnelUnbindKept');
  });

  it('setTunnelId(null) unbinds rather than dropping the reference', async () => {
    backend = [
      {
        id: 'tun_ws',
        name: 'Relay',
        kind: 'websocket',
        websocket: { enabled: true, url: 'wss://relay/v1' },
      },
    ];
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelId('tun_ws'));
    expect(result.current.tunnelSource).toBe('saved');

    await actAndFlush(() => result.current.setTunnelId(null));
    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.wsUrl).toBe('wss://relay/v1');
  });

  it('reports a failure instead of switching state when saving the tunnel fails', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('inline'));
    act(() => result.current.setSshHost('bastion'));
    act(() => result.current.setSshUsername('ops'));
    mockTunnelCommands.saveTunnel.mockRejectedValueOnce(new Error('write failed'));

    let created: SavedTunnel | null = null;
    await act(async () => {
      created = await result.current.saveAsTunnel('Nope');
    });
    expect(created).toBeNull();
    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.tunnelError).toBe('write failed');
  });

  it('treats an SSH tunnel with the toggle off as `none` and keeps the checkbox reachable', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('inline'));
    expect(result.current.effectiveTunnelKind).toBe('ssh');

    act(() => result.current.setSshEnabled(false));
    // Kind is preserved so the checkbox stays mounted, but nothing is emitted.
    expect(result.current.tunnelKind).toBe('ssh');
    expect(result.current.effectiveTunnelKind).toBe('none');
    expect(result.current.tunnelInlineValid).toBe(false);

    act(() => result.current.setSshEnabled(true));
    expect(result.current.effectiveTunnelKind).toBe('ssh');
  });

  it('hydrateTunnelRef loads an existing reference without triggering an unbind', async () => {
    backend = [
      {
        id: 'tun_ssh',
        name: 'Bastion',
        kind: 'ssh',
        ssh: { enabled: true, host: 'bastion', port: 22, username: 'ops', authMethod: 'agent' },
      },
    ];
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.hydrateTunnelRef('tun_ssh', 'ssh'));
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe('tun_ssh');
    expect(result.current.savedTunnel?.name).toBe('Bastion');
    expect(mockTunnelCommands.getTunnel).not.toHaveBeenCalled();

    act(() => result.current.hydrateTunnelRef(null, 'websocket'));
    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.tunnelKind).toBe('websocket');
  });

  it('derives the saved-state kind from the entity, not from a stale hint', async () => {
    backend = [
      {
        id: 'tun_ssh',
        name: 'Bastion',
        kind: 'ssh',
        ssh: { enabled: true, host: 'bastion', port: 22, username: 'ops', authMethod: 'agent' },
      },
    ];
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    // Legacy configs may carry no `tunnelKind` hint at all.
    act(() => result.current.hydrateTunnelRef('tun_ssh', 'none'));
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelKind).toBe('none');
    expect(result.current.effectiveTunnelKind).toBe('ssh');
    expect(result.current.tunnelRefMissing).toBe(false);
  });

  it('resetTunnel clears the slice back to `none`', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('inline'));
    act(() => result.current.setSshHost('bastion'));
    act(() => result.current.setSshUsername('ops'));

    act(() => result.current.resetTunnel());
    expect(result.current.tunnelSource).toBe('none');
    expect(result.current.tunnelKind).toBe('none');
    expect(result.current.sshEnabled).toBe(false);
    expect(result.current.sshHost).toBe('');
    expect(result.current.sshUsername).toBe('');
    expect(result.current.tunnelInlineValid).toBe(false);
  });

  it('defaults to HTTP proxy when the driver does not support SSH', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: false }));
    await flush();

    act(() => result.current.setTunnelSource('inline'));
    expect(result.current.tunnelKind).toBe('httpProxy');
    expect(result.current.sshEnabled).toBe(false);
  });
});

/**
 * [tester] Remaining state-machine exit branches and failure-path coverage.
 * These are the transitions the UI drives from the source / kind selectors that
 * the Coder's journey exercised only through `unbindTunnel()`.
 */
describe('[tester] useTunnelFormState exit branches', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    backend = [];
    resetStore();
    mockTunnelCommands.saveTunnel.mockImplementation(async (tunnel: SavedTunnel) => {
      backend = [...backend.filter((t) => t.id !== tunnel.id), tunnel];
    });
    mockTunnelCommands.getTunnelSummaries.mockImplementation(
      async (): Promise<SavedTunnelSummary[]> =>
        backend.map(({ id, name, kind }) => ({ id, name, kind })),
    );
    mockTunnelCommands.getTunnel.mockImplementation(
      async (id: string): Promise<SavedTunnel | null> => backend.find((t) => t.id === id) ?? null,
    );
  });

  function seedSshWithJump() {
    backend = [
      {
        id: 'tun_jump',
        name: 'Jumped bastion',
        kind: 'ssh',
        ssh: {
          enabled: true,
          host: 'bastion.internal',
          port: 2222,
          username: 'ops',
          authMethod: 'private_key',
          privateKeyPath: '/home/ops/.ssh/id_ed25519',
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
      },
    ];
  }

  it('setTunnelSource("inline") from `saved` refills parameters instead of dropping the reference', async () => {
    seedSshWithJump();
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('saved'));
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe('tun_jump');

    await actAndFlush(() => result.current.setTunnelSource('inline'));
    expect(result.current.tunnelSource).toBe('inline');
    expect(result.current.tunnelId).toBeNull();
    // The whole SSH leg — jump host included — must survive the transition.
    expect(result.current.sshHost).toBe('bastion.internal');
    expect(result.current.sshPort).toBe('2222');
    expect(result.current.sshAuthMethod).toBe('private_key');
    expect(result.current.sshKeyPath).toBe('/home/ops/.ssh/id_ed25519');
    expect(result.current.sshPassphrase).toBe('phrase');
    expect(result.current.sshJumpEnabled).toBe(true);
    expect(result.current.sshJumpHost).toBe('edge.internal');
    expect(result.current.sshJumpPort).toBe('2200');
    expect(result.current.sshJumpUsername).toBe('jumper');
    expect(result.current.sshJumpAuthMethod).toBe('password');
    expect(result.current.sshJumpPassword).toBe('jpw');
    expect(result.current.tunnelInlineValid).toBe(true);
  });

  it('setTunnelKind("none") leaves `inline` for `none` and clears the reference', async () => {
    seedSshWithJump();
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('saved'));
    expect(result.current.tunnelId).toBe('tun_jump');

    act(() => result.current.setTunnelKind('none'));
    expect(result.current.tunnelSource).toBe('none');
    expect(result.current.tunnelKind).toBe('none');
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.effectiveTunnelKind).toBe('none');
  });

  it('setTunnelId(null) with no reference is a no-op that never calls get_tunnel', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelId(null));
    expect(result.current.tunnelId).toBeNull();
    expect(result.current.tunnelSource).toBe('none');
    expect(mockTunnelCommands.getTunnel).not.toHaveBeenCalled();
  });

  it('unbindTunnel() with no reference enters `inline` without touching the kind', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    await act(async () => {
      await result.current.unbindTunnel();
    });
    expect(result.current.tunnelSource).toBe('inline');
    expect(mockTunnelCommands.getTunnel).not.toHaveBeenCalled();

    // Latent inconsistency (not user-reachable — the unbind button only renders
    // in the `saved` state, which always carries a `tunnelId`): this path sets
    // the source to `inline` without restoring `lastInlineKindRef`, so the inline
    // sub-selector has no matching option (`none` is not in `inlineKindOptions`).
    expect(result.current.tunnelKind).toBe('none');
    expect(result.current.effectiveTunnelKind).toBe('none');
  });

  it('saveAsTunnel() refuses to persist an invalid or empty inline configuration', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    // `none` state: there is no inline configuration to promote.
    expect(await result.current.saveAsTunnel('Nothing')).toBeNull();
    expect(mockTunnelCommands.saveTunnel).not.toHaveBeenCalled();

    // `inline` but still incomplete (host without username).
    act(() => result.current.setTunnelSource('inline'));
    act(() => result.current.setSshHost('bastion'));
    expect(await result.current.saveAsTunnel('Incomplete')).toBeNull();
    expect(mockTunnelCommands.saveTunnel).not.toHaveBeenCalled();
  });

  it('surfaces a non-Error rejection from get_tunnel with the fallback message', async () => {
    seedSshWithJump();
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('saved'));
    mockTunnelCommands.getTunnel.mockRejectedValue({ weird: true });

    await act(async () => {
      await result.current.unbindTunnel();
    });
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelError).toBe('newConn.tunnelUnbindKept');
  });

  it('surfaces a string rejection from get_tunnel verbatim', async () => {
    seedSshWithJump();
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('saved'));
    mockTunnelCommands.getTunnel.mockRejectedValue('keyring locked');

    await act(async () => {
      await result.current.unbindTunnel();
    });
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelError).toBe('keyring locked');
  });

  it('falls back to the generic message when saving fails without an error message', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('inline'));
    act(() => result.current.setSshHost('bastion'));
    act(() => result.current.setSshUsername('ops'));
    mockTunnelCommands.saveTunnel.mockRejectedValue({});

    let created: SavedTunnel | null = null;
    await act(async () => {
      created = await result.current.saveAsTunnel('Boom');
    });
    expect(created).toBeNull();
    expect(result.current.tunnelError).toBe('newConn.tunnelSaveFailed');
  });

  it('promotes an inline WebSocket configuration to a saved tunnel', async () => {
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('inline'));
    act(() => result.current.setTunnelKind('websocket'));
    act(() => result.current.setWsUrl('wss://relay.example/v1'));
    act(() => result.current.setWsMode('raw_binary'));
    act(() => result.current.setWsAuthToken('tok'));
    act(() => result.current.setWsTimeout('45'));
    expect(result.current.tunnelInlineValid).toBe(true);

    let created: SavedTunnel | null = null;
    await act(async () => {
      created = await result.current.saveAsTunnel('Relay');
    });
    expect(assigned<SavedTunnel>(created)?.kind).toBe('websocket');
    expect(assigned<SavedTunnel>(created)?.websocket).toEqual({
      enabled: true,
      url: 'wss://relay.example/v1',
      mode: 'raw_binary',
      authToken: 'tok',
      connectTimeoutSecs: 45,
    });
    expect(result.current.tunnelSource).toBe('saved');
  });

  it('re-entering `saved` keeps the current selection instead of jumping to the first entry', async () => {
    backend = [
      {
        id: 'tun_a',
        name: 'A',
        kind: 'ssh',
        ssh: { enabled: true, host: 'a', port: 22, username: 'u', authMethod: 'agent' },
      },
      { id: 'tun_b', name: 'B', kind: 'websocket', websocket: { enabled: true, url: 'wss://b' } },
    ];
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.hydrateTunnelRef('tun_b', 'websocket'));
    expect(result.current.tunnelId).toBe('tun_b');

    act(() => result.current.setTunnelSource('saved'));
    expect(result.current.tunnelId).toBe('tun_b');
    expect(result.current.savedTunnel?.name).toBe('B');
  });

  it('setSshEnabled(true) never drags the form out of `saved`', async () => {
    seedSshWithJump();
    const { result } = renderHook(() => useTunnelFormState({ supportsSSH: true }));
    await flush();

    act(() => result.current.setTunnelSource('saved'));
    expect(result.current.tunnelSource).toBe('saved');

    act(() => result.current.setSshEnabled(true));
    expect(result.current.tunnelSource).toBe('saved');
    expect(result.current.tunnelId).toBe('tun_jump');
    expect(result.current.sshEnabled).toBe(true);
  });
});
