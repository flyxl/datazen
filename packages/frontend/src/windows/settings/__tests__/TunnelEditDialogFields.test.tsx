import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useTunnelStore } from '../../../stores/tunnelStore';
import type { SavedTunnel } from '../../../types';

const { mockTunnelCommands, mockConnectionCommands } = vi.hoisted(() => ({
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
}));

vi.mock('../../../commands/tunnel', () => ({ tunnelCommands: mockTunnelCommands }));
vi.mock('../../../commands/connection', () => ({ connectionCommands: mockConnectionCommands }));
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

import { TunnelSettingsSection } from '../TunnelSettingsSection';

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();
  useTunnelStore.setState({ summaries: [], loaded: false, loading: false, error: null });
  mockTunnelCommands.getTunnelSummaries.mockResolvedValue([]);
  mockTunnelCommands.getTunnelUsage.mockResolvedValue({ connectionIds: [], connectionNames: [] });
  mockTunnelCommands.saveTunnel.mockResolvedValue(undefined);
  mockConnectionCommands.getConnections.mockResolvedValue([]);
  mockConnectionCommands.getGroups.mockResolvedValue([]);
});

/**
 * The settings editor reuses the connection form's tunnel field components
 * verbatim (through the narrow contracts in `tunnelFieldContracts`), so the
 * same inputs must drive the saved entity for all three kinds.
 */
describe('TunnelEditDialog field reuse', () => {
  it('renders the real SSH fields without a second enable toggle', async () => {
    render(<TunnelSettingsSection />);
    await screen.findByTestId('tunnel-list-empty');
    fireEvent.click(screen.getByTestId('tunnel-create'));

    // The kind selector owns the choice here, so the reused checkbox is absent.
    expect(screen.queryByTestId('new-conn-ssh-tunnel-checkbox')).toBeNull();
    expect(screen.getByPlaceholderText('ssh.example.com')).toBeInTheDocument();
    expect(screen.getByTestId('new-conn-ssh-auth-private_key')).toBeInTheDocument();

    // Jump fields come along too.
    fireEvent.click(screen.getByTestId('new-conn-ssh-jump-toggle'));
    expect(screen.getByTestId('new-conn-ssh-jump-fields')).toBeInTheDocument();
  });

  it('saves the http proxy block when the kind is switched to httpProxy', async () => {
    render(<TunnelSettingsSection />);
    await screen.findByTestId('tunnel-list-empty');
    fireEvent.click(screen.getByTestId('tunnel-create'));

    fireEvent.change(await screen.findByTestId('tunnel-edit-name'), {
      target: { value: 'Corp proxy' },
    });
    fireEvent.click(screen.getByTestId('tunnel-edit-kind'));
    fireEvent.mouseDown(screen.getByText('settings.tunnels.kind.httpProxy'));
    expect(await screen.findByTestId('new-conn-http-proxy-fields')).toBeInTheDocument();
    // The SSH body is gone: only the selected kind is rendered.
    expect(screen.queryByPlaceholderText('ssh.example.com')).toBeNull();

    fireEvent.change(screen.getByTestId('new-conn-http-proxy-host'), {
      target: { value: 'proxy.corp.example' },
    });
    fireEvent.change(screen.getByTestId('new-conn-http-proxy-port'), { target: { value: '3128' } });
    fireEvent.change(screen.getByTestId('new-conn-http-proxy-username'), {
      target: { value: 'proxy-user' },
    });
    fireEvent.change(screen.getByTestId('new-conn-http-proxy-password'), {
      target: { value: 'proxy-pw' },
    });
    fireEvent.change(screen.getByTestId('new-conn-http-proxy-timeout'), {
      target: { value: '45' },
    });
    // The scheme control is a `Select`: its testid prop is inert (hyphenated
    // attributes are not forwarded), so drive it through its trigger button.
    const schemeTrigger = screen
      .getByTestId('new-conn-http-proxy-fields')
      .querySelector('button[aria-haspopup="listbox"]') as HTMLElement;
    fireEvent.click(schemeTrigger);
    fireEvent.mouseDown(screen.getByText('HTTPS'));

    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    await waitFor(() => expect(mockTunnelCommands.saveTunnel).toHaveBeenCalledTimes(1));
    const proxyEntity = mockTunnelCommands.saveTunnel.mock.calls[0][0] as SavedTunnel;
    expect(proxyEntity.kind).toBe('httpProxy');
    expect(proxyEntity.httpProxy).toEqual({
      enabled: true,
      host: 'proxy.corp.example',
      port: 3128,
      scheme: 'https',
      username: 'proxy-user',
      password: 'proxy-pw',
      connectTimeoutSecs: 45,
    });
    expect(proxyEntity.ssh).toBeUndefined();
  });

  it('saves the websocket block when the kind is switched to websocket', async () => {
    render(<TunnelSettingsSection />);
    await screen.findByTestId('tunnel-list-empty');
    fireEvent.click(screen.getByTestId('tunnel-create'));

    fireEvent.change(await screen.findByTestId('tunnel-edit-name'), {
      target: { value: 'Relay' },
    });
    fireEvent.click(screen.getByTestId('tunnel-edit-kind'));
    fireEvent.mouseDown(screen.getByText('settings.tunnels.kind.websocket'));
    expect(await screen.findByTestId('new-conn-ws-fields')).toBeInTheDocument();

    fireEvent.change(screen.getByTestId('new-conn-ws-url'), {
      target: { value: 'wss://relay.example.com/v1' },
    });
    fireEvent.change(screen.getByTestId('new-conn-ws-token'), { target: { value: 'tok' } });
    fireEvent.change(screen.getByTestId('new-conn-ws-timeout'), { target: { value: '12' } });
    const modeTrigger = screen
      .getByTestId('new-conn-ws-fields')
      .querySelector('button[aria-haspopup="listbox"]') as HTMLElement;
    fireEvent.click(modeTrigger);
    fireEvent.mouseDown(screen.getByText('newConn.wsModeRaw'));

    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    await waitFor(() => expect(mockTunnelCommands.saveTunnel).toHaveBeenCalledTimes(1));
    const wsEntity = mockTunnelCommands.saveTunnel.mock.calls[0][0] as SavedTunnel;
    expect(wsEntity.kind).toBe('websocket');
    expect(wsEntity.websocket).toEqual({
      enabled: true,
      url: 'wss://relay.example.com/v1',
      mode: 'raw_binary',
      authToken: 'tok',
      connectTimeoutSecs: 12,
    });
    expect(wsEntity.ssh).toBeUndefined();
    expect(wsEntity.httpProxy).toBeUndefined();
  });

  it('keeps an edited entity id stable and never falls back to getTunnels', async () => {
    const entity: SavedTunnel = {
      id: 'tun_ws',
      name: 'Relay',
      kind: 'websocket',
      websocket: { enabled: true, url: 'wss://relay.example.com/v1', mode: 'raw_binary' },
    };
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([
      { id: 'tun_ws', name: 'Relay', kind: 'websocket' },
    ]);
    mockTunnelCommands.getTunnel.mockResolvedValue(entity);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-edit-tun_ws'));

    const url = await screen.findByTestId('new-conn-ws-url');
    await waitFor(() => expect(url).toHaveValue('wss://relay.example.com/v1'));

    fireEvent.change(url, { target: { value: 'wss://relay.example.com/v2' } });
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    await waitFor(() => expect(mockTunnelCommands.saveTunnel).toHaveBeenCalledTimes(1));
    const saved = mockTunnelCommands.saveTunnel.mock.calls[0][0] as SavedTunnel;
    expect(saved.id).toBe('tun_ws');
    expect(saved.websocket?.url).toBe('wss://relay.example.com/v2');
    expect(saved.websocket?.mode).toBe('raw_binary');
    expect(mockTunnelCommands.getTunnels).not.toHaveBeenCalled();
  });

  it('shows a loading state while the entity is being fetched', async () => {
    let settle: (value: SavedTunnel | null) => void = () => {};
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([
      { id: 'tun_ssh', name: 'Bastion', kind: 'ssh' },
    ]);
    mockTunnelCommands.getTunnel.mockImplementation(
      () =>
        new Promise<SavedTunnel | null>((resolve) => {
          settle = resolve;
        }),
    );

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-edit-tun_ssh'));

    expect(await screen.findByText('common.loading')).toBeInTheDocument();
    // The kind-specific body is withheld until the entity arrives.
    expect(screen.queryByPlaceholderText('ssh.example.com')).toBeNull();

    settle(null);
    expect(await screen.findByTestId('tunnel-edit-error')).toHaveTextContent(
      'settings.tunnels.editor.loadFailed',
    );
  });

  it('surfaces an IPC error while loading an entity', async () => {
    mockTunnelCommands.getTunnelSummaries.mockResolvedValue([
      { id: 'tun_ssh', name: 'Bastion', kind: 'ssh' },
    ]);
    mockTunnelCommands.getTunnel.mockRejectedValue('keyring locked');

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-edit-tun_ssh'));

    expect(await screen.findByTestId('tunnel-edit-error')).toHaveTextContent('keyring locked');
  });

  it('keeps the dialog open and reports the message when saving fails', async () => {
    mockTunnelCommands.saveTunnel.mockRejectedValue('permission denied');

    render(<TunnelSettingsSection />);
    await screen.findByTestId('tunnel-list-empty');
    fireEvent.click(screen.getByTestId('tunnel-create'));
    fireEvent.change(await screen.findByTestId('tunnel-edit-name'), {
      target: { value: 'Doomed' },
    });
    fireEvent.change(screen.getByPlaceholderText('ssh.example.com'), {
      target: { value: 'bastion.example.com' },
    });
    fireEvent.change(screen.getByPlaceholderText('root'), { target: { value: 'ops' } });
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    expect(await screen.findByTestId('tunnel-edit-error')).toHaveTextContent('permission denied');
    // Still open and still populated, so the user can retry.
    expect(screen.getByTestId('tunnel-edit-name')).toHaveValue('Doomed');
    expect(screen.getByTestId('tunnel-edit-save')).toBeEnabled();
  });

  it('falls back to the localized message when saving fails without a message', async () => {
    mockTunnelCommands.saveTunnel.mockRejectedValue(undefined);

    render(<TunnelSettingsSection />);
    await screen.findByTestId('tunnel-list-empty');
    fireEvent.click(screen.getByTestId('tunnel-create'));
    fireEvent.change(await screen.findByTestId('tunnel-edit-name'), {
      target: { value: 'Doomed' },
    });
    fireEvent.change(screen.getByPlaceholderText('ssh.example.com'), {
      target: { value: 'bastion.example.com' },
    });
    fireEvent.change(screen.getByPlaceholderText('root'), { target: { value: 'ops' } });
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    expect(await screen.findByTestId('tunnel-edit-error')).toHaveTextContent(
      'settings.tunnels.editor.saveFailed',
    );
  });

  it('cancels without writing anything', async () => {
    render(<TunnelSettingsSection />);
    await screen.findByTestId('tunnel-list-empty');
    fireEvent.click(screen.getByTestId('tunnel-create'));
    fireEvent.change(await screen.findByTestId('tunnel-edit-name'), {
      target: { value: 'Discarded' },
    });
    fireEvent.click(screen.getByTestId('tunnel-edit-cancel'));

    expect(mockTunnelCommands.saveTunnel).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByTestId('tunnel-edit-name')).toBeNull());
  });
});
