import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useConnectionStore } from '../../../stores/connectionStore';
import { useTunnelStore } from '../../../stores/tunnelStore';
import type {
  ConnectionConfig,
  SavedTunnel,
  SavedTunnelSummary,
  TunnelUsage,
} from '../../../types';

/**
 * [tester] (b)3 — adversarial G9 guard.
 *
 * `getTunnels` returns fully decrypted entities (SSH password / passphrase,
 * proxy password, WS authToken). The list and every action it triggers must stay
 * on `get_tunnel_summaries` / `get_tunnel`; no path — empty state, load failure,
 * retry, create, copy, delete-then-refresh — may fall back to the bulk plaintext
 * IPC. This drives the whole surface against an in-memory backend and asserts
 * the bulk IPC is never touched.
 */
const { backend, mockTunnelCommands, mockConnectionCommands, translations } = vi.hoisted(() => {
  const tunnels = new Map<string, SavedTunnel>();
  const connections: ConnectionConfig[] = [];
  return {
    backend: { tunnels, connections },
    mockTunnelCommands: {
      getTunnels: vi.fn(),
      getTunnelSummaries: vi.fn(
        async (): Promise<SavedTunnelSummary[]> =>
          [...tunnels.values()].map((t) => ({ id: t.id, name: t.name, kind: t.kind })),
      ),
      getTunnel: vi.fn(async (id: string) => tunnels.get(id) ?? null),
      getTunnelUsage: vi.fn(async (id: string): Promise<TunnelUsage> => {
        const referencing = connections.filter((c) => c.tunnelId === id);
        return {
          connectionIds: referencing.map((c) => c.id),
          connectionNames: referencing.map((c) => c.name),
        };
      }),
      saveTunnel: vi.fn(async (tunnel: SavedTunnel) => {
        tunnels.set(tunnel.id, tunnel);
      }),
      deleteTunnel: vi.fn(async (id: string) => {
        tunnels.delete(id);
      }),
      testTunnel: vi.fn(async () => 1),
    },
    mockConnectionCommands: {
      getConnections: vi.fn(async () => [...connections]),
      getGroups: vi.fn(async () => []),
      saveConnection: vi.fn(async (config: ConnectionConfig) => {
        const index = connections.findIndex((c) => c.id === config.id);
        if (index >= 0) connections[index] = config;
        else connections.push(config);
      }),
    },
    translations: {
      'settings.tunnels.copySuffix': ' (copy)',
      'settings.tunnels.usageCount': 'Used by {count}',
      'settings.tunnels.usageNone': 'Not referenced',
      'settings.tunnels.usagePending': 'Checking references…',
    } as Record<string, string>,
  };
});

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

beforeEach(() => {
  vi.clearAllMocks();
  backend.tunnels.clear();
  backend.connections.length = 0;
  useTunnelStore.setState({ summaries: [], loaded: false, loading: false, error: null });
  useConnectionStore.setState({
    connections: [],
    groups: [],
    loading: false,
    connectionsLoaded: false,
    error: null,
  });
});

function rowIds(): string[] {
  return Array.from(document.querySelectorAll('[data-tunnel-id]')).map(
    (el) => el.getAttribute('data-tunnel-id') ?? '',
  );
}

describe('[tester] G9 — no plaintext-entity IPC on any list surface (b)', () => {
  it('never calls getTunnels across empty → create → copy → delete → refresh', async () => {
    render(<TunnelSettingsSection />);
    expect(await screen.findByTestId('tunnel-list-empty')).toBeInTheDocument();

    // create
    fireEvent.click(screen.getByTestId('tunnel-create'));
    fireEvent.change(await screen.findByTestId('tunnel-edit-name'), {
      target: { value: 'Bastion' },
    });
    fireEvent.change(screen.getByPlaceholderText('ssh.example.com'), {
      target: { value: 'bastion.example.com' },
    });
    fireEvent.change(screen.getByPlaceholderText('root'), { target: { value: 'ops' } });
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));
    await waitFor(() => expect(rowIds()).toHaveLength(1));
    const originalId = rowIds()[0];

    // a connection starts referencing it, then copy
    backend.connections.push({
      id: 'conn_prod',
      name: 'Prod DB',
      databaseType: 'postgresql',
      sslMode: 'prefer',
      host: 'db.internal',
      port: 5432,
      tunnelId: originalId,
      tunnelKind: 'ssh',
    });
    fireEvent.click(screen.getByTestId(`tunnel-copy-${originalId}`));
    await waitFor(() => expect(rowIds()).toHaveLength(2));

    // delete-and-unbind (also refreshes the list and the usage counts)
    fireEvent.click(screen.getByTestId(`tunnel-delete-${originalId}`));
    await screen.findByTestId('tunnel-delete-affected');
    fireEvent.click(screen.getByTestId('tunnel-delete-confirm'));
    await waitFor(() => expect(rowIds()).toHaveLength(1));

    expect(mockTunnelCommands.getTunnels).not.toHaveBeenCalled();
    expect(mockTunnelCommands.getTunnelSummaries).toHaveBeenCalled();
  });

  it('does not fall back to getTunnels when the summaries load fails and is retried', async () => {
    mockTunnelCommands.getTunnelSummaries.mockRejectedValueOnce('ipc exploded');

    render(<TunnelSettingsSection />);
    expect(await screen.findByTestId('tunnel-load-error')).toHaveTextContent('ipc exploded');

    fireEvent.click(screen.getByText('common.retry'));
    await screen.findByTestId('tunnel-list-empty');

    expect(mockTunnelCommands.getTunnels).not.toHaveBeenCalled();
  });
});
