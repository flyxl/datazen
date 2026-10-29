import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useConnectionStore } from '../../../stores/connectionStore';
import { useTunnelStore } from '../../../stores/tunnelStore';
import type { ConnectionConfig } from '../../../types';

/**
 * [tester] (c) — delete semantics: unbind strictly before delete, abort the
 * delete when an unbind fails, roll back in reverse order, and offer exactly two
 * options (cancel / delete-and-unbind).
 */
const { mockTunnelCommands, mockConnectionCommands, translations } = vi.hoisted(() => ({
  mockTunnelCommands: {
    getTunnels: vi.fn(),
    getTunnelSummaries: vi.fn(),
    getTunnel: vi.fn(),
    getTunnelUsage: vi.fn(),
    saveTunnel: vi.fn(),
    deleteTunnel: vi.fn(),
    testTunnel: vi.fn(),
  },
  mockConnectionCommands: {
    getConnections: vi.fn(),
    getGroups: vi.fn(),
    saveConnection: vi.fn(),
  },
  translations: {
    'settings.tunnels.delete.unbindFailed': 'Could not unbind; nothing was deleted.',
    'settings.tunnels.delete.deleteFailed': 'References cleared but the delete failed.',
    'settings.tunnels.delete.rollbackNote': 'Restored {restored}; still bound: {failed}.',
    'settings.tunnels.delete.noReferences': 'No connection references this tunnel.',
    'settings.tunnels.delete.affected': 'These connections reference it:',
    'settings.tunnels.delete.confirm': 'Delete and unbind',
    'settings.tunnels.delete.confirming': 'Deleting…',
  } as Record<string, string>,
}));

vi.mock('../../../commands/tunnel', () => ({ tunnelCommands: mockTunnelCommands }));
vi.mock('../../../commands/connection', () => ({ connectionCommands: mockConnectionCommands }));
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      let text = translations[key] ?? key;
      if (params) {
        for (const [name, value] of Object.entries(params)) {
          text = text.replace(`{${name}}`, String(value));
        }
      }
      return text;
    },
  }),
}));

import { TunnelSettingsSection } from '../TunnelSettingsSection';

afterEach(cleanup);

const summary = { id: 'tun_ssh', name: 'Bastion', kind: 'ssh' as const };

function referencing(id: string, name: string): ConnectionConfig {
  return {
    id,
    name,
    databaseType: 'postgresql',
    sslMode: 'prefer',
    host: 'db.internal',
    port: 5432,
    tunnelId: 'tun_ssh',
    tunnelKind: 'ssh',
  };
}

async function openDeleteDialog(connections: ConnectionConfig[]): Promise<void> {
  mockConnectionCommands.getConnections.mockResolvedValue(connections);
  mockTunnelCommands.getTunnelUsage.mockResolvedValue({
    connectionIds: connections.map((c) => c.id),
    connectionNames: connections.map((c) => c.name),
  });
  render(<TunnelSettingsSection />);
  await screen.findAllByTestId('tunnel-row');
  fireEvent.click(screen.getByTestId('tunnel-delete-tun_ssh'));
  await screen.findByTestId('tunnel-delete-confirm');
}

beforeEach(() => {
  vi.clearAllMocks();
  useTunnelStore.setState({ summaries: [], loaded: false, loading: false, error: null });
  useConnectionStore.setState({
    connections: [],
    groups: [],
    loading: false,
    connectionsLoaded: false,
    error: null,
  });
  mockTunnelCommands.getTunnelSummaries.mockResolvedValue([summary]);
  mockTunnelCommands.saveTunnel.mockResolvedValue(undefined);
  mockTunnelCommands.deleteTunnel.mockResolvedValue(undefined);
  mockConnectionCommands.saveConnection.mockResolvedValue(undefined);
});

describe('[tester] delete-and-unbind semantics (c)', () => {
  it('offers exactly two options and no "delete only" / "delete and inline" variant', async () => {
    await openDeleteDialog([referencing('c1', 'Prod')]);

    // `Dialog` only emits `data-testid` under `VITE_E2E`, so locate it by role.
    const dialog = screen.getByRole('dialog');
    const testids = Array.from(dialog.querySelectorAll('button'))
      .map((b) => b.dataset.testid)
      .filter((id): id is string => Boolean(id));

    expect(testids).toEqual(['tunnel-delete-cancel', 'tunnel-delete-confirm']);
    expect(screen.getByTestId('tunnel-delete-confirm')).toHaveTextContent('Delete and unbind');
    expect(dialog.textContent ?? '').not.toMatch(/inline|keep the tunnel|delete only/i);
  });

  // Stronger than an invocation-order assertion: the delete must not even be
  // *started* while the unbind write is still in flight.
  it('does not start the delete until the unbind write has settled', async () => {
    let release: () => void = () => {};
    mockConnectionCommands.saveConnection.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    await openDeleteDialog([referencing('c1', 'Prod')]);

    fireEvent.click(screen.getByTestId('tunnel-delete-confirm'));
    await waitFor(() => expect(mockConnectionCommands.saveConnection).toHaveBeenCalledTimes(1));
    // Unbind pending: the entity must still be there.
    expect(mockTunnelCommands.deleteTunnel).not.toHaveBeenCalled();

    await act(async () => {
      release();
    });
    await waitFor(() => expect(mockTunnelCommands.deleteTunnel).toHaveBeenCalledWith('tun_ssh'));
  });

  it('aborts the delete and restores earlier unbinds when a later unbind fails', async () => {
    mockConnectionCommands.saveConnection
      .mockResolvedValueOnce(undefined) // c1 unbound
      .mockRejectedValueOnce('disk full') // c2 fails
      .mockResolvedValueOnce(undefined); // rollback of c1

    await openDeleteDialog([referencing('c1', 'Prod'), referencing('c2', 'Staging')]);
    fireEvent.click(screen.getByTestId('tunnel-delete-confirm'));

    const error = await screen.findByTestId('tunnel-delete-error');
    expect(error).toHaveTextContent('Could not unbind; nothing was deleted.');
    expect(error).toHaveTextContent('disk full');

    // The tunnel survives — this is the G3 invariant.
    expect(mockTunnelCommands.deleteTunnel).not.toHaveBeenCalled();
    // The row is still listed.
    expect(screen.getByTestId('tunnel-delete-tun_ssh')).toBeInTheDocument();

    expect(mockConnectionCommands.saveConnection).toHaveBeenCalledTimes(3);
    expect(mockConnectionCommands.saveConnection.mock.calls[2][0]).toMatchObject({
      id: 'c1',
      tunnelId: 'tun_ssh',
      tunnelKind: 'ssh',
    });
    // No rollback note when the restore succeeded.
    expect(screen.queryByTestId('tunnel-delete-note')).toBeNull();
  });

  it('reports the connections it could not restore', async () => {
    mockConnectionCommands.saveConnection
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce('disk full')
      .mockRejectedValueOnce('restore refused');

    await openDeleteDialog([referencing('c1', 'Prod'), referencing('c2', 'Staging')]);
    fireEvent.click(screen.getByTestId('tunnel-delete-confirm'));

    expect(await screen.findByTestId('tunnel-delete-error')).toHaveTextContent(
      'Could not unbind; nothing was deleted.',
    );
    expect(screen.getByTestId('tunnel-delete-note')).toHaveTextContent(
      'Restored 0; still bound: c1.',
    );
    expect(mockTunnelCommands.deleteTunnel).not.toHaveBeenCalled();
  });

  it('keeps the delete-visible state consistent when only the delete step fails', async () => {
    mockTunnelCommands.deleteTunnel.mockRejectedValue(new Error('locked'));
    await openDeleteDialog([referencing('c1', 'Prod')]);
    fireEvent.click(screen.getByTestId('tunnel-delete-confirm'));

    expect(await screen.findByTestId('tunnel-delete-error')).toHaveTextContent(
      'References cleared but the delete failed.',
    );
    expect(await screen.findByTestId('tunnel-delete-error')).toHaveTextContent('locked');
    // Every reference was cleared, so no dangling reference is left behind.
    expect(mockConnectionCommands.saveConnection.mock.calls[0][0]).toMatchObject({
      id: 'c1',
      tunnelId: undefined,
      tunnelKind: undefined,
    });
  });

  it('never falls back to getTunnels on any delete path', async () => {
    mockConnectionCommands.saveConnection.mockRejectedValue('nope');
    await openDeleteDialog([referencing('c1', 'Prod')]);
    fireEvent.click(screen.getByTestId('tunnel-delete-confirm'));
    await screen.findByTestId('tunnel-delete-error');
    expect(mockTunnelCommands.getTunnels).not.toHaveBeenCalled();
  });
});
