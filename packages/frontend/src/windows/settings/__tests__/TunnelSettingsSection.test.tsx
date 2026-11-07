import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useConnectionStore } from '../../../stores/connectionStore';
import { useTunnelStore } from '../../../stores/tunnelStore';
import type { ConnectionConfig, SavedTunnel, SavedTunnelSummary } from '../../../types';

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
    'settings.tunnels.copySuffix': ' (copy)',
    'settings.tunnels.usageCount': 'Used by {count}',
    'settings.tunnels.usageNone': 'Not referenced',
    'settings.tunnels.usagePending': 'Checking references…',
    'settings.tunnels.empty': 'No saved tunnels yet',
    'settings.tunnels.emptyHint': 'Create one or save an inline tunnel.',
    'settings.tunnels.test.success': 'Probe succeeded in {ms} ms.',
    'settings.tunnels.test.failed': 'Probe failed: {error}',
    'settings.tunnels.delete.noReferences': 'No connection references this tunnel.',
    'settings.tunnels.delete.affected': 'These connections reference it:',
    'settings.tunnels.delete.unbindFailed': 'Could not unbind; nothing was deleted.',
    'settings.tunnels.delete.deleteFailed': 'References cleared but the delete failed.',
    'settings.tunnels.delete.rollbackNote': 'Restored {restored}; still bound: {failed}.',
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

const sshSummary: SavedTunnelSummary = { id: 'tun_ssh', name: 'Bastion', kind: 'ssh' };
const wsSummary: SavedTunnelSummary = { id: 'tun_ws', name: 'Relay', kind: 'websocket' };

const sshEntity: SavedTunnel = {
  id: 'tun_ssh',
  name: 'Bastion',
  kind: 'ssh',
  ssh: {
    enabled: true,
    host: 'bastion.example.com',
    port: 22,
    username: 'ops',
    authMethod: 'password',
    password: 'hunter2',
  },
};

function referencingConnection(id: string, name: string, tunnelId: string): ConnectionConfig {
  return {
    id,
    name,
    databaseType: 'postgresql',
    sslMode: 'prefer',
    host: 'db.internal',
    port: 5432,
    tunnelId,
    tunnelKind: 'ssh',
  };
}

function usageFor(map: Record<string, { connectionIds: string[]; connectionNames: string[] }>) {
  mockTunnelCommands.getTunnelUsage.mockImplementation((id: string) =>
    Promise.resolve(map[id] ?? { connectionIds: [], connectionNames: [] }),
  );
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
  mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);
  mockTunnelCommands.getTunnel.mockResolvedValue(null);
  mockTunnelCommands.saveTunnel.mockResolvedValue(undefined);
  mockTunnelCommands.deleteTunnel.mockResolvedValue(undefined);
  mockTunnelCommands.testTunnel.mockResolvedValue(42);
  mockConnectionCommands.getConnections.mockResolvedValue([]);
  mockConnectionCommands.getGroups.mockResolvedValue([]);
  mockConnectionCommands.saveConnection.mockResolvedValue(undefined);
  usageFor({});
});

describe('TunnelSettingsSection — list', () => {
  it('shows guidance instead of a blank panel when there are no saved tunnels', async () => {
    render(<TunnelSettingsSection />);
    expect(await screen.findByTestId('tunnel-list-empty')).toBeInTheDocument();
    expect(screen.getByText('No saved tunnels yet')).toBeInTheDocument();
    expect(screen.getByText('Create one or save an inline tunnel.')).toBeInTheDocument();
    expect(screen.queryByTestId('tunnel-list')).toBeNull();
  });

  it('renders name, kind and reference count per tunnel', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary, wsSummary]);
    usageFor({
      tun_ssh: { connectionIds: ['c1', 'c2'], connectionNames: ['Prod', 'Staging'] },
      tun_ws: { connectionIds: [], connectionNames: [] },
    });

    render(<TunnelSettingsSection />);
    const rows = await screen.findAllByTestId('tunnel-row');
    expect(rows).toHaveLength(2);

    const sshRow = document.querySelector('[data-tunnel-id="tun_ssh"]') as HTMLElement;
    const ssh = within(sshRow);
    expect(ssh.getByTestId('tunnel-row-name')).toHaveTextContent('Bastion');
    expect(ssh.getByTestId('tunnel-row-kind')).toHaveTextContent('settings.tunnels.kind.ssh');
    expect(ssh.getByTestId('tunnel-row-usage')).toHaveTextContent('Used by 2');

    const ws = within(document.querySelector('[data-tunnel-id="tun_ws"]') as HTMLElement);
    expect(ws.getByTestId('tunnel-row-name')).toHaveTextContent('Relay');
    expect(ws.getByTestId('tunnel-row-kind')).toHaveTextContent('settings.tunnels.kind.websocket');
    expect(ws.getByTestId('tunnel-row-usage')).toHaveTextContent('Not referenced');
  });

  // G9 guard: the list must only ever consume the secret-free projection.
  it('never calls getTunnels (the decrypted-entity IPC) to render the list', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    expect(mockTunnelCommands.getTunnelSummaries).toHaveBeenCalled();
    expect(mockTunnelCommands.getTunnels).not.toHaveBeenCalled();
  });

  it('surfaces a load failure with a retry action', async () => {
    mockTunnelCommands.getTunnelSummaries.mockRejectedValue('ipc exploded');
    render(<TunnelSettingsSection />);
    expect(await screen.findByTestId('tunnel-load-error')).toHaveTextContent('ipc exploded');

    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    fireEvent.click(screen.getByText('common.retry'));
    expect(await screen.findAllByTestId('tunnel-row')).toHaveLength(1);
  });
});

describe('TunnelSettingsSection — edit', () => {
  it('fetches the full entity for editing and saves through update()', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    mockTunnelCommands.getTunnel.mockResolvedValue(sshEntity);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-edit-tun_ssh'));

    await waitFor(() => expect(mockTunnelCommands.getTunnel).toHaveBeenCalledWith('tun_ssh'));
    const name = await screen.findByTestId('tunnel-edit-name');
    await waitFor(() => expect(name).toHaveValue('Bastion'));

    fireEvent.change(name, { target: { value: 'Bastion renamed' } });
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    await waitFor(() => expect(mockTunnelCommands.saveTunnel).toHaveBeenCalledTimes(1));
    expect(mockTunnelCommands.saveTunnel.mock.calls[0][0]).toMatchObject({
      id: 'tun_ssh',
      name: 'Bastion renamed',
      kind: 'ssh',
      ssh: { host: 'bastion.example.com', username: 'ops', password: 'hunter2' },
    });
    expect(mockTunnelCommands.getTunnels).not.toHaveBeenCalled();
  });

  it('opens a blank create form and creates with a generated id', async () => {
    render(<TunnelSettingsSection />);
    await screen.findByTestId('tunnel-list-empty');
    fireEvent.click(screen.getByTestId('tunnel-create'));

    const name = await screen.findByTestId('tunnel-edit-name');
    expect(name).toHaveValue('');
    expect(mockTunnelCommands.getTunnel).not.toHaveBeenCalled();

    fireEvent.change(name, { target: { value: 'Fresh' } });
    fireEvent.change(screen.getByPlaceholderText('ssh.example.com'), {
      target: { value: 'bastion.example.com' },
    });
    fireEvent.change(screen.getByPlaceholderText('root'), { target: { value: 'ops' } });
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    await waitFor(() => expect(mockTunnelCommands.saveTunnel).toHaveBeenCalledTimes(1));
    const saved = mockTunnelCommands.saveTunnel.mock.calls[0][0] as SavedTunnel;
    expect(saved.id).toMatch(/^tun_/);
    expect(saved.name).toBe('Fresh');
    expect(saved.ssh?.host).toBe('bastion.example.com');
  });

  it('reports a missing entity instead of opening an empty form', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    mockTunnelCommands.getTunnel.mockResolvedValue(null);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-edit-tun_ssh'));

    expect(await screen.findByTestId('tunnel-edit-error')).toHaveTextContent(
      'settings.tunnels.editor.loadFailed',
    );
  });

  it('blocks saving until the draft is valid', async () => {
    render(<TunnelSettingsSection />);
    await screen.findByTestId('tunnel-list-empty');
    fireEvent.click(screen.getByTestId('tunnel-create'));

    const save = await screen.findByTestId('tunnel-edit-save');
    expect(save).toBeDisabled();

    fireEvent.change(screen.getByTestId('tunnel-edit-name'), { target: { value: 'X' } });
    expect(save).toBeDisabled();
    expect(screen.getByTestId('tunnel-edit-hint')).toHaveTextContent(
      'settings.tunnels.validation.sshHost',
    );

    fireEvent.change(screen.getByPlaceholderText('ssh.example.com'), { target: { value: 'h' } });
    expect(save).toBeDisabled();
    fireEvent.change(screen.getByPlaceholderText('root'), { target: { value: 'u' } });
    expect(save).toBeEnabled();
  });
});

describe('TunnelSettingsSection — copy', () => {
  it('creates a new entity with a new id and a suffixed name', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    mockTunnelCommands.getTunnel.mockResolvedValue(sshEntity);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-copy-tun_ssh'));

    await waitFor(() => expect(mockTunnelCommands.saveTunnel).toHaveBeenCalledTimes(1));
    const saved = mockTunnelCommands.saveTunnel.mock.calls[0][0] as SavedTunnel;
    expect(saved.id).not.toBe('tun_ssh');
    expect(saved.id).toMatch(/^tun_/);
    expect(saved.name).toBe('Bastion (copy)');
    expect(saved.ssh).toEqual(sshEntity.ssh);
  });

  it('reports a copy failure when the source entity is gone', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    mockTunnelCommands.getTunnel.mockResolvedValue(null);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-copy-tun_ssh'));

    expect(await screen.findByTestId('tunnel-action-error')).toHaveTextContent(
      'settings.tunnels.copyFailed',
    );
    expect(mockTunnelCommands.saveTunnel).not.toHaveBeenCalled();
  });
});

describe('TunnelSettingsSection — delete', () => {
  it('lists the affected connection names before deleting', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    usageFor({ tun_ssh: { connectionIds: ['c1', 'c2'], connectionNames: ['Prod', 'Staging'] } });

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-delete-tun_ssh'));

    const items = await screen.findAllByTestId('tunnel-delete-affected-item');
    expect(items.map((i) => i.textContent)).toEqual(['Prod', 'Staging']);
    expect(screen.getByTestId('tunnel-delete-confirm')).toBeEnabled();
  });

  it('unbinds every reference before deleting the tunnel', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    usageFor({ tun_ssh: { connectionIds: ['c1'], connectionNames: ['Prod'] } });
    mockConnectionCommands.getConnections.mockResolvedValue([
      referencingConnection('c1', 'Prod', 'tun_ssh'),
    ]);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-delete-tun_ssh'));
    await screen.findByTestId('tunnel-delete-affected');
    fireEvent.click(screen.getByTestId('tunnel-delete-confirm'));

    await waitFor(() => expect(mockTunnelCommands.deleteTunnel).toHaveBeenCalledWith('tun_ssh'));
    expect(mockConnectionCommands.saveConnection).toHaveBeenCalledTimes(1);
    const saved = mockConnectionCommands.saveConnection.mock.calls[0][0] as ConnectionConfig;
    expect(saved.id).toBe('c1');
    expect(saved.tunnelId).toBeUndefined();
    expect(saved.tunnelKind).toBeUndefined();

    const unbindOrder = mockConnectionCommands.saveConnection.mock.invocationCallOrder[0];
    const deleteOrder = mockTunnelCommands.deleteTunnel.mock.invocationCallOrder[0];
    expect(unbindOrder).toBeLessThan(deleteOrder);
  });

  it('writes nothing at all when the user cancels', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    usageFor({ tun_ssh: { connectionIds: ['c1'], connectionNames: ['Prod'] } });
    mockConnectionCommands.getConnections.mockResolvedValue([
      referencingConnection('c1', 'Prod', 'tun_ssh'),
    ]);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-delete-tun_ssh'));
    await screen.findByTestId('tunnel-delete-affected');
    fireEvent.click(screen.getByTestId('tunnel-delete-cancel'));

    expect(mockConnectionCommands.saveConnection).not.toHaveBeenCalled();
    expect(mockTunnelCommands.deleteTunnel).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByTestId('tunnel-delete-body')).toBeNull());
  });

  it('keeps the tunnel when unbinding fails, and says so', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    usageFor({ tun_ssh: { connectionIds: ['c1'], connectionNames: ['Prod'] } });
    mockConnectionCommands.getConnections.mockResolvedValue([
      referencingConnection('c1', 'Prod', 'tun_ssh'),
    ]);
    mockConnectionCommands.saveConnection.mockRejectedValue('disk full');

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-delete-tun_ssh'));
    await screen.findByTestId('tunnel-delete-affected');
    fireEvent.click(screen.getByTestId('tunnel-delete-confirm'));

    const error = await screen.findByTestId('tunnel-delete-error');
    expect(error).toHaveTextContent('Could not unbind; nothing was deleted.');
    expect(error).toHaveTextContent('disk full');
    expect(mockTunnelCommands.deleteTunnel).not.toHaveBeenCalled();
  });
});

describe('TunnelSettingsSection — test probe', () => {
  it('requires a target host and port and probes the saved tunnel with them', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    mockTunnelCommands.testTunnel.mockResolvedValue(42);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-test-tun_ssh'));

    const host = await screen.findByTestId('tunnel-test-host');
    expect(host).toHaveValue('127.0.0.1');
    const run = screen.getByTestId('tunnel-test-run');
    expect(run).toBeDisabled();

    fireEvent.change(screen.getByTestId('tunnel-test-port'), { target: { value: '5432' } });
    expect(run).toBeEnabled();
    fireEvent.click(run);

    await waitFor(() =>
      expect(mockTunnelCommands.testTunnel).toHaveBeenCalledWith('tun_ssh', '127.0.0.1', 5432),
    );
    expect(await screen.findByTestId('tunnel-test-success')).toHaveTextContent(
      'Probe succeeded in 42 ms.',
    );
  });

  it('states the probe scope instead of promising end-to-end reachability', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-test-tun_ssh'));

    expect(await screen.findByTestId('tunnel-test-scope')).toHaveTextContent(
      'settings.tunnels.test.scope.ssh',
    );
    expect(screen.getByTestId('tunnel-test-scope-general')).toHaveTextContent(
      'settings.tunnels.test.scope.general',
    );
  });

  it('shows the probe error and does not fire a second probe while one is in flight', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    let settle: (value: number) => void = () => {};
    mockTunnelCommands.testTunnel.mockImplementation(
      () =>
        new Promise<number>((resolve) => {
          settle = resolve;
        }),
    );

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-test-tun_ssh'));
    fireEvent.change(await screen.findByTestId('tunnel-test-port'), {
      target: { value: '5432' },
    });

    const run = screen.getByTestId('tunnel-test-run');
    fireEvent.click(run);
    await waitFor(() => expect(run).toBeDisabled());
    fireEvent.click(run);
    expect(mockTunnelCommands.testTunnel).toHaveBeenCalledTimes(1);

    settle(7);
    expect(await screen.findByTestId('tunnel-test-success')).toHaveTextContent(
      'Probe succeeded in 7 ms.',
    );
  });

  it('surfaces a probe failure', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
    mockTunnelCommands.testTunnel.mockRejectedValue('connection refused');

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-test-tun_ssh'));
    fireEvent.change(await screen.findByTestId('tunnel-test-port'), {
      target: { value: '5432' },
    });
    fireEvent.click(screen.getByTestId('tunnel-test-run'));

    expect(await screen.findByTestId('tunnel-test-error')).toHaveTextContent(
      'Probe failed: connection refused',
    );
  });
});
