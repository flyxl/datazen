import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useTunnelStore } from '../../../stores/tunnelStore';
import type { SavedTunnel } from '../../../types';

/**
 * [tester] (g) — the dialogs' fetch effects must not depend on the `t`
 * function's identity.
 *
 * The `useI18n` mock below returns a **brand new** translator closure on every
 * render, which is exactly the caller shape that used to make the effect re-run
 * forever. Counting the fetch IPC calls is the decisive assertion: an effect
 * keyed on `t` re-fetches on every render, so `getTunnel` would be called many
 * times instead of once.
 */
const { i18nState, mockTunnelCommands, mockConnectionCommands } = vi.hoisted(() => ({
  i18nState: {
    lang: 'en' as 'en' | 'zh',
    renders: 0,
    dict: {
      en: {
        'settings.tunnels.editor.loadFailed': 'EN: could not load this tunnel',
        'settings.tunnels.delete.failed': 'EN: could not delete the tunnel',
        'settings.tunnels.editor.saveFailed': 'EN: could not save the tunnel',
      },
      zh: {
        'settings.tunnels.editor.loadFailed': 'ZH: 无法加载该隧道',
        'settings.tunnels.delete.failed': 'ZH: 无法删除该隧道',
        'settings.tunnels.editor.saveFailed': 'ZH: 无法保存该隧道',
      },
    } as Record<'en' | 'zh', Record<string, string>>,
  },
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
  useI18n: () => {
    i18nState.renders += 1;
    const lang = i18nState.lang;
    return {
      // Deliberately a new closure per call: a stable identity would hide the bug.
      t: (key: string, params?: Record<string, string | number>) => {
        let text = i18nState.dict[lang][key] ?? key;
        if (params) {
          for (const [name, value] of Object.entries(params)) {
            text = text.replace(`{${name}}`, String(value));
          }
        }
        return text;
      },
    };
  },
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

beforeEach(() => {
  vi.clearAllMocks();
  i18nState.lang = 'en';
  i18nState.renders = 0;
  useTunnelStore.setState({ summaries: [], loaded: false, loading: false, error: null });
  mockTunnelCommands.getTunnelSummaries.mockResolvedValue([sshSummary]);
  mockTunnelCommands.getTunnel.mockResolvedValue(sshEntity);
  mockTunnelCommands.getTunnelUsage.mockResolvedValue({ connectionIds: [], connectionNames: [] });
  mockConnectionCommands.getConnections.mockResolvedValue([]);
});

describe('[tester] tunnel dialogs survive an unstable translator identity (g)', () => {
  it('fetches the edited entity exactly once even though `t` changes every render', async () => {
    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-edit-tun_ssh'));

    const name = await screen.findByTestId('tunnel-edit-name');
    await waitFor(() => expect(name).toHaveValue('Bastion'));

    const rendersAfterSettle = i18nState.renders;
    // A `t`-keyed effect keeps rendering; give it a window to run away.
    await new Promise((r) => setTimeout(r, 80));

    expect(mockTunnelCommands.getTunnel).toHaveBeenCalledTimes(1);
    expect(mockTunnelCommands.getTunnel).toHaveBeenCalledWith('tun_ssh');
    expect(i18nState.renders - rendersAfterSettle).toBeLessThan(10);
  });

  it('fetches the delete usage once for the list and once for the dialog, then stops', async () => {
    render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-delete-tun_ssh'));
    await screen.findByTestId('tunnel-delete-no-references');

    // Two legitimate sources: the row's reference-count batch and the dialog.
    const callsAfterSettle = mockTunnelCommands.getTunnelUsage.mock.calls.length;
    expect(callsAfterSettle).toBe(2);
    const rendersAfterSettle = i18nState.renders;

    // A `t`-keyed effect would keep fetching; give it a window to run away.
    await new Promise((r) => setTimeout(r, 80));

    expect(mockTunnelCommands.getTunnelUsage).toHaveBeenCalledTimes(callsAfterSettle);
    expect(i18nState.renders - rendersAfterSettle).toBeLessThan(10);
  });

  it('re-translates a stored error key when the language switches (edit dialog)', async () => {
    mockTunnelCommands.getTunnel.mockResolvedValue(null);

    const { rerender } = render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-edit-tun_ssh'));

    expect(await screen.findByTestId('tunnel-edit-error')).toHaveTextContent(
      'EN: could not load this tunnel',
    );

    i18nState.lang = 'zh';
    rerender(<TunnelSettingsSection />);

    expect(screen.getByTestId('tunnel-edit-error')).toHaveTextContent('ZH: 无法加载该隧道');
    // Re-translating must not re-fetch.
    expect(mockTunnelCommands.getTunnel).toHaveBeenCalledTimes(1);
  });

  it('re-translates a stored error key when the language switches (delete dialog)', async () => {
    // A message-less rejection is what produces the translation-key branch.
    mockTunnelCommands.getTunnelUsage.mockRejectedValue({});

    const { rerender } = render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-delete-tun_ssh'));

    expect(await screen.findByTestId('tunnel-delete-error')).toHaveTextContent(
      'EN: could not delete the tunnel',
    );

    const callsAfterSettle = mockTunnelCommands.getTunnelUsage.mock.calls.length;
    i18nState.lang = 'zh';
    rerender(<TunnelSettingsSection />);

    expect(screen.getByTestId('tunnel-delete-error')).toHaveTextContent('ZH: 无法删除该隧道');
    // Re-translating must not re-fetch.
    expect(mockTunnelCommands.getTunnelUsage).toHaveBeenCalledTimes(callsAfterSettle);
  });

  it('re-translates the save-failure key without closing or re-fetching', async () => {
    mockTunnelCommands.saveTunnel.mockRejectedValue({});

    const { rerender } = render(<TunnelSettingsSection />);
    await screen.findAllByTestId('tunnel-row');
    fireEvent.click(screen.getByTestId('tunnel-edit-tun_ssh'));
    const name = await screen.findByTestId('tunnel-edit-name');
    await waitFor(() => expect(name).toHaveValue('Bastion'));
    fireEvent.click(screen.getByTestId('tunnel-edit-save'));

    expect(await screen.findByTestId('tunnel-edit-error')).toHaveTextContent(
      'EN: could not save the tunnel',
    );

    i18nState.lang = 'zh';
    rerender(<TunnelSettingsSection />);

    expect(screen.getByTestId('tunnel-edit-error')).toHaveTextContent('ZH: 无法保存该隧道');
    expect(mockTunnelCommands.getTunnel).toHaveBeenCalledTimes(1);
  });
});
