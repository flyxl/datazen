import { describe, expect, it, vi, type Mock } from 'vitest';
import {
  deleteTunnelAndUnbind,
  unbindConnectionPatch,
  type DeleteTunnelDeps,
} from '../tunnelDeletion';
import type { ConnectionConfig } from '../../types';

/** The `deps()` harness keeps every seam a spy so ordering and stubs stay assertable. */
type DepMocks = { [K in keyof DeleteTunnelDeps]: Mock<DeleteTunnelDeps[K]> };

function connection(id: string, tunnelId?: string): ConnectionConfig {
  return {
    id,
    name: `conn ${id}`,
    databaseType: 'postgresql',
    sslMode: 'prefer',
    host: '127.0.0.1',
    port: 5432,
    ...(tunnelId === undefined ? {} : { tunnelId, tunnelKind: 'ssh' as const }),
  };
}

function deps(
  connections: ConnectionConfig[],
  overrides: Partial<DeleteTunnelDeps> = {},
): DepMocks {
  const base: DepMocks = {
    getConnections: vi.fn().mockResolvedValue(connections),
    saveConnection: vi.fn().mockResolvedValue(undefined),
    deleteTunnel: vi.fn().mockResolvedValue(undefined),
  };
  return Object.assign(base, overrides);
}

describe('deleteTunnelAndUnbind', () => {
  it('clears the reference and its stale kind hint, and saves before deleting', async () => {
    const d = deps([connection('c1', 'tun_a'), connection('c2'), connection('c3', 'tun_b')]);

    const outcome = await deleteTunnelAndUnbind('tun_a', d);

    expect(outcome).toEqual({ kind: 'deleted', unboundConnectionIds: ['c1'] });
    expect(d.saveConnection).toHaveBeenCalledTimes(1);
    expect(d.saveConnection).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'c1', tunnelId: undefined, tunnelKind: undefined }),
    );
    // Ordering is the whole point: references must be gone before the entity is.
    const saveOrder = d.saveConnection.mock.invocationCallOrder[0];
    const deleteOrder = d.deleteTunnel.mock.invocationCallOrder[0];
    expect(saveOrder).toBeLessThan(deleteOrder);
    expect(d.deleteTunnel).toHaveBeenCalledWith('tun_a');
  });

  it('deletes a tunnel that nothing references without touching connections', async () => {
    const d = deps([connection('c1', 'tun_other')]);
    const outcome = await deleteTunnelAndUnbind('tun_a', d);
    expect(outcome).toEqual({ kind: 'deleted', unboundConnectionIds: [] });
    expect(d.saveConnection).not.toHaveBeenCalled();
    expect(d.deleteTunnel).toHaveBeenCalledWith('tun_a');
  });

  it('aborts without deleting when an unbind fails, and rolls back what it changed', async () => {
    const d = deps([connection('c1', 'tun_a'), connection('c2', 'tun_a')]);
    d.saveConnection
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce('disk full')
      .mockResolvedValueOnce(undefined);

    const outcome = await deleteTunnelAndUnbind('tun_a', d);

    expect(outcome).toMatchObject({ kind: 'unbind-failed', message: 'disk full' });
    if (outcome.kind !== 'unbind-failed') throw new Error('expected unbind-failed');
    expect(outcome.rolledBack).toEqual(['c1']);
    expect(outcome.rollbackFailed).toEqual([]);
    // The tunnel survives: a half-applied delete is never left behind.
    expect(d.deleteTunnel).not.toHaveBeenCalled();
    // Third call restores c1 with its original reference.
    expect(d.saveConnection.mock.calls[2][0]).toMatchObject({
      id: 'c1',
      tunnelId: 'tun_a',
      tunnelKind: 'ssh',
    });
  });

  it('reports connections it could not restore', async () => {
    const d = deps([connection('c1', 'tun_a'), connection('c2', 'tun_a')]);
    d.saveConnection
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce('nope')
      .mockRejectedValueOnce('rollback refused');

    const outcome = await deleteTunnelAndUnbind('tun_a', d);

    if (outcome.kind !== 'unbind-failed') throw new Error('expected unbind-failed');
    expect(outcome.rolledBack).toEqual([]);
    expect(outcome.rollbackFailed).toEqual(['c1']);
    expect(d.deleteTunnel).not.toHaveBeenCalled();
  });

  it('aborts without deleting when the references cannot even be read', async () => {
    const d = deps([], { getConnections: vi.fn().mockRejectedValue('ipc down') });
    const outcome = await deleteTunnelAndUnbind('tun_a', d);
    expect(outcome).toMatchObject({ kind: 'unbind-failed', message: 'ipc down' });
    expect(d.deleteTunnel).not.toHaveBeenCalled();
  });

  it('reports a delete failure separately: every reference is already cleared', async () => {
    const d = deps([connection('c1', 'tun_a')], {
      deleteTunnel: vi.fn().mockRejectedValue(new Error('locked')),
    });

    const outcome = await deleteTunnelAndUnbind('tun_a', d);

    expect(outcome).toEqual({
      kind: 'delete-failed',
      message: 'locked',
      unboundConnectionIds: ['c1'],
    });
    // Unbinding succeeded, so the state is consistent: no dangling reference.
    expect(d.saveConnection).toHaveBeenCalledTimes(1);
  });

  it('falls back to a localised message for a message-less rejection', async () => {
    const d = deps([connection('c1', 'tun_a')], {
      deleteTunnel: vi.fn().mockRejectedValue({}),
    });
    const outcome = await deleteTunnelAndUnbind('tun_a', d);
    if (outcome.kind !== 'delete-failed') throw new Error('expected delete-failed');
    expect(outcome.message).toBeTruthy();
    expect(outcome.message).not.toBe('[object Object]');
  });
});

describe('unbindConnectionPatch', () => {
  it('removes the reference without mutating the source config', () => {
    const source = connection('c1', 'tun_a');
    const patched = unbindConnectionPatch(source);
    expect(source.tunnelId).toBe('tun_a');
    expect(patched.tunnelId).toBeUndefined();
    expect(patched.tunnelKind).toBeUndefined();
    expect(patched.id).toBe('c1');
    expect(patched.host).toBe('127.0.0.1');
  });
});
