/**
 * Extraction parity for the error bars that moved onto `ErrorBanner`.
 *
 * The failure mode this guards is not "the banner broke" — it is "the banner
 * quietly changed what the user reads": a dropped `t()`, a lost `{error}`
 * substitution, a `div` where the layout needed a `p`, or a `role="alert"`
 * that stopped announcing. Every case here renders the real call site and pins
 * the tag, the role and the exact visible text.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { useTunnelStore } from '../../../stores/tunnelStore';
import type { ConnectionConfig, SavedTunnel } from '../../../types';

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

/**
 * A real translator rather than an identity stub: keys render as themselves,
 * except the one entry that carries a `{error}` placeholder. That single
 * template is what makes a dropped `t()` — or a dropped param substitution —
 * observable instead of silently passing.
 */
const DICT: Record<string, string> = {
  'settings.tunnels.test.failed': 'Connection test failed: {error}',
};

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      let text = DICT[key] ?? key;
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

const sshSummary = { id: 'tun_ssh', name: 'Bastion', kind: 'ssh' as const };
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
  },
};

/**
 * The invariant of every error bar: announced, and showing what it was handed.
 * Asynchronous because the failure that fills a banner is always async.
 */
async function expectAlert(testId: string, text: string, tag: 'DIV' | 'P' | 'SPAN') {
  const el = await screen.findByTestId(testId);
  expect(el.tagName).toBe(tag);
  expect(el).toHaveAttribute('role', 'alert');
  expect(el.textContent).toBe(text);
  return el;
}

beforeEach(() => {
  vi.clearAllMocks();
  useTunnelStore.setState({ summaries: [], loaded: false, loading: false, error: null });
  mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
  mockTunnelCommands.getTunnel.mockResolvedValue(sshEntity);
  mockTunnelCommands.getTunnelUsage.mockResolvedValue({ connectionIds: [], connectionNames: [] });
  mockConnectionCommands.getConnections.mockResolvedValue([]);
  mockConnectionCommands.getGroups.mockResolvedValue([]);
  mockTunnelCommands.saveTunnel.mockResolvedValue(undefined);
  mockTunnelCommands.deleteTunnel.mockResolvedValue(undefined);
  mockTunnelCommands.testTunnel.mockResolvedValue(12);
});

describe('ErrorBanner call sites keep their announced text', () => {
  it('TunnelEditDialog: a raw IPC error still reaches the user verbatim', async () => {
    mockTunnelCommands.saveTunnel.mockRejectedValue('permission denied');

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-create'));
    fireEvent.change(await screen.findByTestId('tunnel-edit-name'), {
      target: { value: 'Doomed' },
    });
    fireEvent.change(screen.getByPlaceholderText('ssh.example.com'), {
      target: { value: 'bastion.example.com' },
    });
    fireEvent.change(screen.getByPlaceholderText('root'), { target: { value: 'ops' } });
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    await expectAlert('tunnel-edit-error', 'permission denied', 'P');
  });

  it('TunnelEditDialog: the localized fallback wins when IPC has no message', async () => {
    mockTunnelCommands.saveTunnel.mockRejectedValue(undefined);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-create'));
    fireEvent.change(await screen.findByTestId('tunnel-edit-name'), {
      target: { value: 'Doomed' },
    });
    fireEvent.change(screen.getByPlaceholderText('ssh.example.com'), {
      target: { value: 'bastion.example.com' },
    });
    fireEvent.change(screen.getByPlaceholderText('root'), { target: { value: 'ops' } });
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    // A bare rejection carries no copy of its own, so the localized string is
    // the only thing the user can be shown.
    await expectAlert('tunnel-edit-error', 'settings.tunnels.editor.saveFailed', 'P');
  });

  it('TunnelTestDialog: the interpolated message is neither dropped nor left raw', async () => {
    mockTunnelCommands.testTunnel.mockRejectedValue('handshake timed out');

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-test-tun_ssh'));

    // The run button is disabled until both endpoints are filled in.
    const host = await screen.findByTestId('tunnel-test-host');
    fireEvent.change(host, { target: { value: 'bastion.example.com' } });
    fireEvent.change(screen.getByTestId('tunnel-test-port'), { target: { value: '22' } });
    fireEvent.click(await screen.findByTestId('tunnel-test-run'));

    const el = await expectAlert(
      'tunnel-test-error',
      'Connection test failed: handshake timed out',
      'P',
    );
    // Neither the raw key nor the un-substituted placeholder may survive.
    expect(el.textContent).not.toContain('settings.tunnels.test.failed');
    expect(el.textContent).not.toContain('{error}');
  });

  it('TunnelDeleteDialog: a failed delete keeps its message and stays announced', async () => {
    // Unbind-then-delete, so the dialog composes a two-part message: the
    // outcome sentence plus the exception text. Both have to survive the
    // extraction, and neither may be replaced by the key it translates to.
    const referencing: ConnectionConfig = {
      id: 'c1',
      name: 'Prod',
      databaseType: 'postgresql',
      sslMode: 'prefer',
      host: 'db.internal',
      port: 5432,
      tunnelId: 'tun_ssh',
      tunnelKind: 'ssh',
    };
    mockConnectionCommands.getConnections.mockResolvedValue([referencing]);
    mockTunnelCommands.getTunnelUsage.mockResolvedValue({
      connectionIds: [referencing.id],
      connectionNames: [referencing.name],
    });
    mockTunnelCommands.deleteTunnel.mockRejectedValue(new Error('locked'));

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-delete-tun_ssh'));
    fireEvent.click(await screen.findByTestId('tunnel-delete-confirm'));

    const el = await screen.findByTestId('tunnel-delete-error');
    expect(el.tagName).toBe('P');
    expect(el).toHaveAttribute('role', 'alert');
    // The exception text is the only part the user can act on.
    expect(el.textContent).toContain('locked');
    expect(el.textContent).not.toContain('object Object');
  });

  it('TunnelSettingsSection: the store error stays an alert beside its retry button', async () => {
    // Retry is a sibling *outside* the alert element. Extracting the banner
    // must not swallow it, and must not fold its label into the announcement.
    mockTunnelCommands.getTunnelSummaries.mockRejectedValue('cannot reach the keyring');

    render(<TunnelSettingsSection />);
    const region = await screen.findByTestId('tunnel-load-error');
    const banner = region.querySelector('[role="alert"]');

    expect(banner?.tagName).toBe('P');
    expect(banner?.textContent).toBe('cannot reach the keyring');
    expect(screen.getByRole('button', { name: 'common.retry' })).toBeInTheDocument();
  });

  it('TunnelSettingsSection: a failed action is localized and still announced', async () => {
    // Copying a tunnel whose entity vanished falls back to the localized key.
    mockTunnelCommands.getTunnel.mockResolvedValue(null);

    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-copy-tun_ssh'));

    await expectAlert('tunnel-action-error', 'settings.tunnels.copyFailed', 'P');
  });
});
