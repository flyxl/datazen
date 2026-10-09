import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useConnectionStore } from '../../../stores/connectionStore';
import { useTunnelStore } from '../../../stores/tunnelStore';
import type {
  ConnectionConfig,
  SavedTunnel,
  SavedTunnelSummary,
  TunnelUsage,
} from '../../../types';

/**
 * Continuous journey test: create → appears in the list → rename →
 * copy → delete-and-unbind, driven through the real store and the real dialogs
 * against a small in-memory backend. Nothing here is a static single point:
 * each step asserts the state transition the previous step produced.
 */
const { backend, mockTunnelCommands, mockConnectionCommands, translations } = vi.hoisted(() => {
  const tunnels = new Map<string, SavedTunnel>();
  const connections: ConnectionConfig[] = [];

  const summarize = (tunnel: SavedTunnel): SavedTunnelSummary => ({
    id: tunnel.id,
    name: tunnel.name,
    kind: tunnel.kind,
  });

  return {
    backend: { tunnels, connections },
    mockTunnelCommands: {
      getTunnels: vi.fn(),
      getTunnelSummaries: vi.fn(async () => [...tunnels.values()].map(summarize)),
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

function rowFor(id: string): HTMLElement {
  return document.querySelector(`[data-tunnel-id="${id}"]`) as HTMLElement;
}

describe('saved tunnel management journey', () => {
  it('creates, lists, renames, copies, then deletes-and-unbinds a referenced tunnel', async () => {
    render(<TunnelSettingsSection />);
    expect(await screen.findByTestId('tunnel-list-empty')).toBeInTheDocument();

    // ── 1. create ────────────────────────────────────────────────────────────
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
    const createdId = rowIds()[0];
    expect(createdId).toMatch(/^tun_/);
    expect(within(rowFor(createdId)).getByTestId('tunnel-row-name')).toHaveTextContent('Bastion');
    expect(within(rowFor(createdId)).getByTestId('tunnel-row-usage')).toHaveTextContent(
      'Not referenced',
    );
    expect(screen.queryByTestId('tunnel-list-empty')).toBeNull();
    expect(backend.tunnels.get(createdId)?.ssh?.host).toBe('bastion.example.com');

    // ── 2. rename through the edit dialog ────────────────────────────────────
    fireEvent.click(screen.getByTestId(`tunnel-edit-${createdId}`));
    const nameField = await screen.findByTestId('tunnel-edit-name');
    await waitFor(() => expect(nameField).toHaveValue('Bastion'));
    // Editing loads the stored entity, credentials included.
    expect(backend.tunnels.size).toBe(1);
    expect(mockTunnelCommands.getTunnels).not.toHaveBeenCalled();
    fireEvent.change(nameField, { target: { value: 'Bastion 2' } });
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    await waitFor(() =>
      expect(within(rowFor(createdId)).getByTestId('tunnel-row-name')).toHaveTextContent(
        'Bastion 2',
      ),
    );
    expect(backend.tunnels.size).toBe(1);
    expect(backend.tunnels.get(createdId)?.name).toBe('Bastion 2');

    // ── 3. a connection starts referencing it ───────────────────────────────
    backend.connections.push({
      id: 'conn_prod',
      name: 'Prod DB',
      databaseType: 'postgresql',
      sslMode: 'prefer',
      host: 'db.internal',
      port: 5432,
      tunnelId: createdId,
      tunnelKind: 'ssh',
    });

    // ── 4. copy produces a second, independent entity ───────────────────────
    fireEvent.click(screen.getByTestId(`tunnel-copy-${createdId}`));
    await waitFor(() => expect(rowIds()).toHaveLength(2));

    const copyId = rowIds().find((id) => id !== createdId) ?? '';
    expect(copyId).toMatch(/^tun_/);
    expect(copyId).not.toBe(createdId);
    expect(within(rowFor(copyId)).getByTestId('tunnel-row-name')).toHaveTextContent(
      'Bastion 2 (copy)',
    );
    expect(backend.tunnels.get(copyId)?.ssh?.host).toBe('bastion.example.com');
    // The copy is a new entity: the original keeps its own reference count.
    expect(within(rowFor(createdId)).getByTestId('tunnel-row-usage')).toHaveTextContent(
      'Used by 1',
    );
    expect(within(rowFor(copyId)).getByTestId('tunnel-row-usage')).toHaveTextContent(
      'Not referenced',
    );

    // ── 5. delete the referenced original: unbind first, then delete ────────
    fireEvent.click(screen.getByTestId(`tunnel-delete-${createdId}`));
    const affected = await screen.findByTestId('tunnel-delete-affected');
    expect(within(affected).getByText('Prod DB')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('tunnel-delete-confirm'));

    await waitFor(() => expect(backend.tunnels.has(createdId)).toBe(false));
    expect(rowIds()).toEqual([copyId]);
    expect(backend.connections[0].tunnelId).toBeUndefined();
    expect(backend.connections[0].tunnelKind).toBeUndefined();
    expect(backend.connections[0].name).toBe('Prod DB');
    expect(mockConnectionCommands.saveConnection.mock.invocationCallOrder[0]).toBeLessThan(
      mockTunnelCommands.deleteTunnel.mock.invocationCallOrder[0],
    );
    // The surviving copy is untouched by the unbind.
    expect(backend.tunnels.get(copyId)?.name).toBe('Bastion 2 (copy)');
  });
});
