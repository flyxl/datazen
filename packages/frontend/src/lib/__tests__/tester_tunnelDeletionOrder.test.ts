import { describe, expect, it, vi } from 'vitest';
import { deleteTunnelAndUnbind } from '../tunnelDeletion';
import type { ConnectionConfig } from '../../types';

/**
 * [tester] (c)3 — the rollback path in detail.
 *
 * `deleteTunnelAndUnbind` is the only thing standing between a user's delete and
 * a dangling `tunnelId` (G3). The interesting cases are the ones the happy path
 * never reaches: reverse-order rollback across more than one already-written
 * connection, a rollback that itself fails, and exact id matching.
 */
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

describe('[tester] deleteTunnelAndUnbind rollback order (c)', () => {
  it('rolls back every already-unbound connection in reverse order', async () => {
    const saveConnection = vi
      .fn()
      .mockResolvedValueOnce(undefined) // c1 unbound
      .mockResolvedValueOnce(undefined) // c2 unbound
      .mockRejectedValueOnce('disk full') // c3 fails
      .mockResolvedValueOnce(undefined) // rollback c2
      .mockResolvedValueOnce(undefined); // rollback c1
    const deleteTunnel = vi.fn().mockResolvedValue(undefined);

    const outcome = await deleteTunnelAndUnbind('tun_a', {
      getConnections: vi
        .fn()
        .mockResolvedValue([
          connection('c1', 'tun_a'),
          connection('c2', 'tun_a'),
          connection('c3', 'tun_a'),
        ]),
      saveConnection,
      deleteTunnel,
    });

    if (outcome.kind !== 'unbind-failed') throw new Error('expected unbind-failed');
    // Most recent write undone first.
    expect(outcome.rolledBack).toEqual(['c2', 'c1']);
    expect(outcome.rollbackFailed).toEqual([]);
    expect(saveConnection.mock.calls.map((c) => (c[0] as ConnectionConfig).id)).toEqual([
      'c1',
      'c2',
      'c3',
      'c2',
      'c1',
    ]);
    // The failing write is never rolled back (it never landed), and the tunnel survives.
    expect(deleteTunnel).not.toHaveBeenCalled();
  });

  it('keeps unwinding the rollback after one restore fails', async () => {
    const saveConnection = vi
      .fn()
      .mockResolvedValueOnce(undefined) // c1 unbound
      .mockResolvedValueOnce(undefined) // c2 unbound
      .mockRejectedValueOnce('disk full') // c3 fails
      .mockRejectedValueOnce('restore c2 refused')
      .mockResolvedValueOnce(undefined); // restore c1 succeeds
    const deleteTunnel = vi.fn().mockResolvedValue(undefined);

    const outcome = await deleteTunnelAndUnbind('tun_a', {
      getConnections: vi
        .fn()
        .mockResolvedValue([
          connection('c1', 'tun_a'),
          connection('c2', 'tun_a'),
          connection('c3', 'tun_a'),
        ]),
      saveConnection,
      deleteTunnel,
    });

    if (outcome.kind !== 'unbind-failed') throw new Error('expected unbind-failed');
    expect(outcome.rolledBack).toEqual(['c1']);
    expect(outcome.rollbackFailed).toEqual(['c2']);
    expect(saveConnection).toHaveBeenCalledTimes(5);
    expect(deleteTunnel).not.toHaveBeenCalled();
  });

  it('matches the tunnel id exactly — no prefix or kind-based collateral', async () => {
    const saveConnection = vi.fn().mockResolvedValue(undefined);
    const deleteTunnel = vi.fn().mockResolvedValue(undefined);

    const outcome = await deleteTunnelAndUnbind('tun_a', {
      getConnections: vi
        .fn()
        .mockResolvedValue([
          connection('c1', 'tun_a'),
          connection('c2', 'tun_a2'),
          connection('c3', 'tun_ab'),
          connection('c4'),
        ]),
      saveConnection,
      deleteTunnel,
    });

    expect(outcome).toEqual({ kind: 'deleted', unboundConnectionIds: ['c1'] });
    expect(saveConnection).toHaveBeenCalledTimes(1);
    expect((saveConnection.mock.calls[0][0] as ConnectionConfig).id).toBe('c1');
  });

  it('never deletes while a restore is still in flight', async () => {
    let releaseRestore: () => void = () => {};
    const saveConnection = vi
      .fn()
      .mockResolvedValueOnce(undefined) // c1 unbound
      .mockRejectedValueOnce('disk full') // c2 fails
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            releaseRestore = resolve;
          }),
      ); // rollback c1 pending
    const deleteTunnel = vi.fn().mockResolvedValue(undefined);

    const pending = deleteTunnelAndUnbind('tun_a', {
      getConnections: vi
        .fn()
        .mockResolvedValue([connection('c1', 'tun_a'), connection('c2', 'tun_a')]),
      saveConnection,
      deleteTunnel,
    });

    await vi.waitFor(() => expect(saveConnection).toHaveBeenCalledTimes(3));
    expect(deleteTunnel).not.toHaveBeenCalled();
    releaseRestore();
    const outcome = await pending;
    expect(outcome.kind).toBe('unbind-failed');
    expect(deleteTunnel).not.toHaveBeenCalled();
  });
});
