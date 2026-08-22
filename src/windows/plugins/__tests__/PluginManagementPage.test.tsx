import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { PluginManagementPage } from '../PluginManagementPage';
import { UI_PLUGIN_API_VERSION, type PluginSummary } from '../../../types/plugin';

const {
  pluginState,
  setEnabledMock,
  removeMock,
  fetchMock,
  closeByPluginMock,
  openTabMock,
  installFromPathMock,
  confirmSpy,
} = vi.hoisted(() => ({
  pluginState: {
    plugins: [] as Array<Record<string, unknown>>,
    loaded: true,
    error: null as string | null,
  },
  setEnabledMock: vi.fn(),
  removeMock: vi.fn(),
  fetchMock: vi.fn(),
  closeByPluginMock: vi.fn(),
  openTabMock: vi.fn(),
  installFromPathMock: vi.fn(),
  confirmSpy: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [confirmSpy, null],
}));

vi.mock('../../../stores/pluginStore', () => ({
  usePluginStore: Object.assign((sel: (s: typeof pluginState) => unknown) => sel(pluginState), {
    getState: () => ({
      ...pluginState,
      fetch: fetchMock,
      setEnabled: setEnabledMock,
      remove: removeMock,
      byId: (id: string) => (pluginState.plugins as Array<{ id: string }>).find((p) => p.id === id),
    }),
  }),
}));

vi.mock('../../../stores/workspaceTabsStore', () => ({
  workspaceTabKey: (pluginId: string, pageId: string) => `${pluginId}:${pageId}`,
  useWorkspaceTabsStore: Object.assign(
    (sel: (s: Record<string, unknown>) => unknown) => sel({ tabs: [], activeKey: null }),
    {
      getState: () => ({ open: openTabMock, closeByPlugin: closeByPluginMock }),
    },
  ),
}));

vi.mock('../../../commands/plugins', () => ({
  PLUGINS_CHANGED_EVENT: 'plugins:changed',
  pluginCommands: {
    installPluginFromPath: (...args: unknown[]) => installFromPathMock(...args),
  },
}));

function makePlugin(overrides: Partial<PluginSummary> = {}): PluginSummary {
  return {
    id: 'acme.bill-audit',
    name: 'Bill Audit',
    version: '1.0.0',
    apiVersion: UI_PLUGIN_API_VERSION,
    author: 'Acme',
    description: 'Compare bills against quotas',
    enabled: true,
    permissions: ['context:connections'],
    pages: [{ id: 'quota-check', title: 'Quota Check' }],
    themes: [],
    ...overrides,
  };
}

function card(id: string): HTMLElement {
  const el = screen
    .getAllByTestId('plugin-card')
    .find((c) => c.getAttribute('data-plugin-id') === id);
  if (!el) throw new Error(`card ${id} not found`);
  return el;
}

beforeEach(() => {
  pluginState.plugins = [];
  pluginState.loaded = true;
  pluginState.error = null;
  setEnabledMock.mockReset().mockResolvedValue(undefined);
  removeMock.mockReset().mockResolvedValue(undefined);
  fetchMock.mockReset().mockResolvedValue(undefined);
  installFromPathMock.mockReset();
  closeByPluginMock.mockReset();
  openTabMock.mockReset();
  confirmSpy.mockReset().mockResolvedValue(true);
});

afterEach(cleanup);

describe('PluginManagementPage', () => {
  it('renders header with installed count and one card per plugin', () => {
    pluginState.plugins = [makePlugin(), makePlugin({ id: 'acme.midnight', name: 'Midnight' })];

    render(<PluginManagementPage />);

    expect(screen.getByTestId('plugin-management-page')).toBeInTheDocument();
    expect(screen.getByText('plugins.page.title')).toBeInTheDocument();
    expect(screen.getAllByTestId('plugin-card')).toHaveLength(2);
    expect(screen.getByText('Bill Audit')).toBeInTheDocument();
    expect(screen.getByText('Midnight')).toBeInTheDocument();
    // Permission badge with tooltip explanation from the in-component map.
    const badges = within(card('acme.bill-audit')).getByText('context:connections');
    expect(badges).toBeInTheDocument();
  });

  it('filters cards through the all/workspace/theme chips', () => {
    pluginState.plugins = [
      makePlugin(),
      makePlugin({
        id: 'acme.midnight',
        name: 'Midnight',
        pages: [],
        themes: [{ id: 'midnight-blue', name: 'Midnight Blue', modes: ['dark'] }],
      }),
    ];

    render(<PluginManagementPage />);

    expect(screen.getAllByTestId('plugin-card')).toHaveLength(2);

    fireEvent.click(screen.getByTestId('plugin-filter-theme'));
    expect(screen.getAllByTestId('plugin-card')).toHaveLength(1);
    expect(screen.getByTestId('plugin-management-page')).toHaveTextContent('Midnight');

    fireEvent.click(screen.getByTestId('plugin-filter-workspace'));
    expect(screen.getAllByTestId('plugin-card')).toHaveLength(1);
    expect(screen.getByTestId('plugin-management-page')).toHaveTextContent('Bill Audit');

    fireEvent.click(screen.getByTestId('plugin-filter-all'));
    expect(screen.getAllByTestId('plugin-card')).toHaveLength(2);
  });

  it('narrows cards by search text across name/id/description', () => {
    pluginState.plugins = [makePlugin(), makePlugin({ id: 'acme.afi', name: 'AFI Pricing' })];

    render(<PluginManagementPage />);
    fireEvent.change(screen.getByTestId('plugin-search-input'), {
      target: { value: 'afi' },
    });

    expect(screen.getAllByTestId('plugin-card')).toHaveLength(1);
    expect(screen.getByText('AFI Pricing')).toBeInTheDocument();
  });

  it('shows an empty state when nothing matches', () => {
    pluginState.plugins = [];

    render(<PluginManagementPage />);

    expect(screen.getByTestId('plugin-page-empty')).toBeInTheDocument();
    expect(screen.queryByTestId('plugin-card')).not.toBeInTheDocument();
  });

  it('toggles a plugin off through the store and closes its workspace tabs', async () => {
    pluginState.plugins = [makePlugin()];
    setEnabledMock.mockResolvedValue(undefined);

    render(<PluginManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-toggle'));

    await waitFor(() => expect(setEnabledMock).toHaveBeenCalledWith('acme.bill-audit', false));
    await waitFor(() => expect(closeByPluginMock).toHaveBeenCalledWith('acme.bill-audit'));
  });

  it('enables a disabled plugin without closing tabs', async () => {
    pluginState.plugins = [makePlugin({ enabled: false })];

    render(<PluginManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-toggle'));

    await waitFor(() => expect(setEnabledMock).toHaveBeenCalledWith('acme.bill-audit', true));
    expect(closeByPluginMock).not.toHaveBeenCalled();
  });

  it('surfaces store errors from a failed toggle', async () => {
    pluginState.plugins = [makePlugin()];
    setEnabledMock.mockRejectedValue(new Error('backend refused'));

    render(<PluginManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-toggle'));

    await waitFor(() =>
      expect(screen.getByTestId('plugin-page-error')).toHaveTextContent('backend refused'),
    );
  });

  it('uninstalls after confirmation and closes related tabs', async () => {
    pluginState.plugins = [makePlugin()];
    removeMock.mockResolvedValue(undefined);

    render(<PluginManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-uninstall'));

    await waitFor(() => expect(confirmSpy).toHaveBeenCalled());
    expect(confirmSpy.mock.calls[0]?.[0]).toMatchObject({
      title: 'plugins.page.uninstallTitle',
      message: 'plugins.page.uninstallMessage',
    });
    await waitFor(() => expect(removeMock).toHaveBeenCalledWith('acme.bill-audit'));
    expect(closeByPluginMock).toHaveBeenCalledWith('acme.bill-audit');
  });

  it('keeps the plugin when uninstall is cancelled', async () => {
    pluginState.plugins = [makePlugin()];
    confirmSpy.mockResolvedValue(false);

    render(<PluginManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-uninstall'));

    await waitFor(() => expect(confirmSpy).toHaveBeenCalled());
    expect(removeMock).not.toHaveBeenCalled();
  });

  it('opens a workspace plugin tab and switches to the workspace view', () => {
    pluginState.plugins = [makePlugin()];
    const onOpenInWorkspace = vi.fn();

    render(<PluginManagementPage onOpenInWorkspace={onOpenInWorkspace} />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-open'));

    expect(openTabMock).toHaveBeenCalledWith({
      key: 'acme.bill-audit:quota-check',
      pluginId: 'acme.bill-audit',
      pageId: 'quota-check',
      title: 'Quota Check',
      icon: undefined,
      version: '1.0.0',
    });
    expect(onOpenInWorkspace).toHaveBeenCalledOnce();
  });

  it('renders theme-only cards without an open action and with the settings hint', () => {
    pluginState.plugins = [
      makePlugin({
        id: 'acme.midnight',
        name: 'Midnight',
        pages: [],
        themes: [{ id: 'midnight-blue', name: 'Midnight Blue', modes: ['dark'] }],
        permissions: [],
      }),
    ];

    render(<PluginManagementPage />);
    const themeCard = card('acme.midnight');

    expect(within(themeCard).getByText('plugins.page.themeHint')).toBeInTheDocument();
    expect(within(themeCard).getByText('plugins.page.themeBadge')).toBeInTheDocument();
    expect(within(themeCard).queryByTestId('plugin-open')).not.toBeInTheDocument();
  });

  it('greys out API-mismatched plugins and blocks their toggle/open actions', () => {
    pluginState.plugins = [makePlugin({ apiVersion: UI_PLUGIN_API_VERSION + 1 })];

    render(<PluginManagementPage />);
    const mismatched = card('acme.bill-audit');

    expect(mismatched.className).toMatch(/opacity-60/);
    expect(within(mismatched).getByText('plugins.page.apiMismatch')).toBeInTheDocument();
    expect((within(mismatched).getByTestId('plugin-toggle') as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(within(mismatched).queryByTestId('plugin-open')).not.toBeInTheDocument();
  });

  it('installs a plugin package from a zip path and refreshes the list', async () => {
    installFromPathMock.mockResolvedValue({ id: 'acme.new' });
    fetchMock.mockResolvedValue(undefined);

    render(<PluginManagementPage />);
    fireEvent.click(screen.getByTestId('plugin-install-button'));

    expect(screen.getByText('plugins.install.title')).toBeInTheDocument();

    fireEvent.change(await screen.findByPlaceholderText('plugins.install.pathPlaceholder'), {
      target: { value: '/tmp/acme-new.zip' },
    });
    fireEvent.click(screen.getByTestId('plugin-install-confirm'));

    await waitFor(() => expect(installFromPathMock).toHaveBeenCalledWith('/tmp/acme-new.zip'));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.queryByText('plugins.install.title')).not.toBeInTheDocument(),
    );
  });

  it('shows a copyable error when installation fails', async () => {
    installFromPathMock.mockRejectedValue(new Error('manifest invalid'));

    render(<PluginManagementPage />);
    fireEvent.click(screen.getByTestId('plugin-install-button'));
    fireEvent.change(await screen.findByPlaceholderText('plugins.install.pathPlaceholder'), {
      target: { value: '/tmp/broken.zip' },
    });
    fireEvent.click(screen.getByTestId('plugin-install-confirm'));

    await waitFor(() =>
      expect(screen.getByTestId('plugin-install-error')).toHaveTextContent('manifest invalid'),
    );
    expect(screen.getByTestId('copyable-error-copy')).toBeInTheDocument();
    // Dialog stays open so the user can retry or copy the error.
    expect(screen.getByText('plugins.install.title')).toBeInTheDocument();
  });
});
