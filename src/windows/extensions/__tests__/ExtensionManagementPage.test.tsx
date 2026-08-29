import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { ExtensionManagementPage } from '../ExtensionManagementPage';
import { EXTENSION_API_VERSION, type ExtensionSummary } from '../../../types/extension';

const {
  pluginState,
  setEnabledMock,
  removeMock,
  fetchMock,
  closeByPluginMock,
  openTabMock,
  inspectPackageMock,
  installFromPathMock,
  readExtensionFileMock,
  confirmSpy,
} = vi.hoisted(() => ({
  pluginState: {
    extensions: [] as Array<Record<string, unknown>>,
    loaded: true,
    error: null as string | null,
  },
  setEnabledMock: vi.fn(),
  removeMock: vi.fn(),
  fetchMock: vi.fn(),
  closeByPluginMock: vi.fn(),
  openTabMock: vi.fn(),
  inspectPackageMock: vi.fn(),
  installFromPathMock: vi.fn(),
  readExtensionFileMock: vi.fn(),
  confirmSpy: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [confirmSpy, null],
}));

vi.mock('../../../stores/extensionStore', () => ({
  useExtensionStore: Object.assign((sel: (s: typeof pluginState) => unknown) => sel(pluginState), {
    getState: () => ({
      ...pluginState,
      fetch: fetchMock,
      setEnabled: setEnabledMock,
      remove: removeMock,
      byId: (id: string) => (pluginState.extensions as Array<{ id: string }>).find((p) => p.id === id),
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

vi.mock('../../../commands/extensions', () => ({
  EXTENSIONS_CHANGED_EVENT: 'plugins:changed',
  extensionCommands: {
    inspectExtensionPackageWithDialog: (...args: unknown[]) => inspectPackageMock(...args),
    installExtension: (...args: unknown[]) => installFromPathMock(...args),
    readExtensionFile: (...args: unknown[]) => readExtensionFileMock(...args),
  },
}));

function makePlugin(overrides: Partial<ExtensionSummary> = {}): ExtensionSummary {
  return {
    id: 'acme.bill-audit',
    name: 'Bill Audit',
    version: '1.0.0',
    apiVersion: EXTENSION_API_VERSION,
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
  pluginState.extensions = [];
  pluginState.loaded = true;
  pluginState.error = null;
  setEnabledMock.mockReset().mockResolvedValue(undefined);
  removeMock.mockReset().mockResolvedValue(undefined);
  fetchMock.mockReset().mockResolvedValue(undefined);
  inspectPackageMock.mockReset().mockResolvedValue({
    id: 'acme.new',
    name: 'New Plugin',
    version: '1.0.0',
    apiVersion: 2,
    author: 'Acme',
    contributes: { pages: [], themes: [] },
    permissions: ['storage:local'],
  });
  installFromPathMock.mockReset();
  readExtensionFileMock.mockReset().mockResolvedValue(new Uint8Array([137, 80, 78, 71]));
  closeByPluginMock.mockReset();
  openTabMock.mockReset();
  confirmSpy.mockReset().mockResolvedValue(true);
});

afterEach(cleanup);

describe('ExtensionManagementPage', () => {
  it('renders header with installed count and one card per plugin', () => {
    pluginState.extensions = [makePlugin(), makePlugin({ id: 'acme.midnight', name: 'Midnight' })];

    render(<ExtensionManagementPage />);

    expect(screen.getByTestId('plugin-management-page')).toBeInTheDocument();
    expect(screen.getByText('plugins.page.title')).toBeInTheDocument();
    expect(screen.getAllByTestId('plugin-card')).toHaveLength(2);
    expect(screen.getByText('Bill Audit')).toBeInTheDocument();
    expect(screen.getByText('Midnight')).toBeInTheDocument();
    // Permission badge with tooltip explanation from the in-component map.
    const badges = within(card('acme.bill-audit')).getByText('context:connections');
    expect(badges).toBeInTheDocument();
  });

  it('defaults the filter to Workspace and switches through all/theme chips', () => {
    pluginState.extensions = [
      makePlugin(),
      makePlugin({
        id: 'acme.midnight',
        name: 'Midnight',
        pages: [],
        themes: [{ id: 'midnight-blue', name: 'Midnight Blue', modes: ['dark'] }],
      }),
    ];

    render(<ExtensionManagementPage />);

    // PRD §4.3: default filter is Workspace — theme-only plugins start hidden.
    expect(screen.getAllByTestId('plugin-card')).toHaveLength(1);
    expect(screen.getByText('Bill Audit')).toBeInTheDocument();
    const workspaceChip = screen.getByTestId('plugin-filter-workspace');
    expect(workspaceChip.className).toMatch(/bg-accent/);
    expect(screen.getByTestId('plugin-filter-all').className).not.toMatch(/bg-accent/);

    fireEvent.click(screen.getByTestId('plugin-filter-theme'));
    expect(screen.getAllByTestId('plugin-card')).toHaveLength(1);
    expect(screen.getByTestId('plugin-management-page')).toHaveTextContent('Midnight');

    fireEvent.click(screen.getByTestId('plugin-filter-all'));
    expect(screen.getAllByTestId('plugin-card')).toHaveLength(2);
  });

  it('renders the all view grouped into Workspace pages and Themes sections', () => {
    pluginState.extensions = [
      makePlugin({
        id: 'acme.midnight',
        name: 'Midnight',
        pages: [],
        themes: [{ id: 'm', name: 'M', modes: ['dark'] }],
      }),
      makePlugin(),
      // Both contributions: belongs to the Workspace group, shown exactly once.
      makePlugin({ id: 'acme.both', name: 'Both' }),
    ];

    render(<ExtensionManagementPage />);
    fireEvent.click(screen.getByTestId('plugin-filter-all'));

    expect(screen.getByTestId('plugin-group-workspace')).toHaveTextContent(
      'plugins.page.groupWorkspace',
    );
    expect(screen.getByTestId('plugin-group-theme')).toHaveTextContent('plugins.page.groupTheme');
    const workspaceGroup = screen.getByTestId('plugin-group-workspace');
    expect(within(workspaceGroup).getAllByTestId('plugin-card')).toHaveLength(2);
    expect(within(workspaceGroup).getByText('Bill Audit')).toBeInTheDocument();
    expect(within(workspaceGroup).getByText('Both')).toBeInTheDocument();
    const themeGroup = screen.getByTestId('plugin-group-theme');
    expect(within(themeGroup).getAllByTestId('plugin-card')).toHaveLength(1);
    expect(within(themeGroup).getByText('Midnight')).toBeInTheDocument();
  });

  it('hides empty groups in the all view and keeps the flat grid for single-kind filters', () => {
    pluginState.extensions = [makePlugin(), makePlugin({ id: 'acme.afi', name: 'AFI Pricing' })];

    render(<ExtensionManagementPage />);
    fireEvent.click(screen.getByTestId('plugin-filter-all'));

    expect(screen.queryByTestId('plugin-group-theme')).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId('plugin-group-workspace')).getAllByTestId('plugin-card'),
    ).toHaveLength(2);

    // Non-"all" filters keep rendering a single flat grid without headers.
    fireEvent.click(screen.getByTestId('plugin-filter-workspace'));
    expect(screen.queryByTestId('plugin-group-workspace')).not.toBeInTheDocument();
    expect(screen.getAllByTestId('plugin-card')).toHaveLength(2);
  });

  it('narrows cards by search text across name/id/description', () => {
    pluginState.extensions = [makePlugin(), makePlugin({ id: 'acme.afi', name: 'AFI Pricing' })];

    render(<ExtensionManagementPage />);
    fireEvent.change(screen.getByTestId('plugin-search-input'), {
      target: { value: 'afi' },
    });

    expect(screen.getAllByTestId('plugin-card')).toHaveLength(1);
    expect(screen.getByText('AFI Pricing')).toBeInTheDocument();
  });

  it('shows an empty state when nothing matches', () => {
    pluginState.extensions = [];

    render(<ExtensionManagementPage />);

    expect(screen.getByTestId('plugin-page-empty')).toBeInTheDocument();
    expect(screen.queryByTestId('plugin-card')).not.toBeInTheDocument();
  });

  it('toggles a plugin off through the store and closes its workspace tabs', async () => {
    pluginState.extensions = [makePlugin()];
    setEnabledMock.mockResolvedValue(undefined);

    render(<ExtensionManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-toggle'));

    await waitFor(() => expect(setEnabledMock).toHaveBeenCalledWith('acme.bill-audit', false));
    await waitFor(() => expect(closeByPluginMock).toHaveBeenCalledWith('acme.bill-audit'));
  });

  it('enables a disabled plugin without closing tabs', async () => {
    pluginState.extensions = [makePlugin({ enabled: false })];

    render(<ExtensionManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-toggle'));

    await waitFor(() => expect(setEnabledMock).toHaveBeenCalledWith('acme.bill-audit', true));
    expect(closeByPluginMock).not.toHaveBeenCalled();
  });

  it('surfaces store errors from a failed toggle', async () => {
    pluginState.extensions = [makePlugin()];
    setEnabledMock.mockRejectedValue(new Error('backend refused'));

    render(<ExtensionManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-toggle'));

    await waitFor(() =>
      expect(screen.getByTestId('plugin-page-error')).toHaveTextContent('backend refused'),
    );
  });

  it('uninstalls after confirmation and closes related tabs', async () => {
    pluginState.extensions = [makePlugin()];
    removeMock.mockResolvedValue(undefined);

    render(<ExtensionManagementPage />);
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
    pluginState.extensions = [makePlugin()];
    confirmSpy.mockResolvedValue(false);

    render(<ExtensionManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('plugin-uninstall'));

    await waitFor(() => expect(confirmSpy).toHaveBeenCalled());
    expect(removeMock).not.toHaveBeenCalled();
  });

  it('opens a workspace plugin tab and switches to the workspace view', () => {
    pluginState.extensions = [makePlugin()];
    const onOpenInWorkspace = vi.fn();

    render(<ExtensionManagementPage onOpenInWorkspace={onOpenInWorkspace} />);
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
    pluginState.extensions = [
      makePlugin({
        id: 'acme.midnight',
        name: 'Midnight',
        pages: [],
        themes: [{ id: 'midnight-blue', name: 'Midnight Blue', modes: ['dark'] }],
        permissions: [],
      }),
    ];

    render(<ExtensionManagementPage />);
    fireEvent.click(screen.getByTestId('plugin-filter-theme'));
    const themeCard = card('acme.midnight');

    expect(within(themeCard).getByText('plugins.page.themeHint')).toBeInTheDocument();
    expect(within(themeCard).getByText('plugins.page.themeBadge')).toBeInTheDocument();
    expect(within(themeCard).queryByTestId('plugin-open')).not.toBeInTheDocument();
  });

  it('greys out API-mismatched plugins and blocks their toggle/open actions', () => {
    pluginState.extensions = [makePlugin({ apiVersion: EXTENSION_API_VERSION + 1 })];

    render(<ExtensionManagementPage />);
    const mismatched = card('acme.bill-audit');

    expect(mismatched.className).toMatch(/opacity-60/);
    expect(within(mismatched).getByText('plugins.page.apiMismatch')).toBeInTheDocument();
    expect((within(mismatched).getByTestId('plugin-toggle') as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(within(mismatched).queryByTestId('plugin-open')).not.toBeInTheDocument();
  });

  it('installs through the two-step confirm flow and refreshes the list', async () => {
    installFromPathMock.mockResolvedValue({ id: 'acme.new' });
    fetchMock.mockResolvedValue(undefined);

    render(<ExtensionManagementPage />);
    fireEvent.click(screen.getByTestId('plugin-install-button'));

    expect(screen.getByText('plugins.install.title')).toBeInTheDocument();

    // Step 1 → 2: inspection only, no write yet.
    fireEvent.change(await screen.findByPlaceholderText('plugins.install.pathPlaceholder'), {
      target: { value: '/tmp/acme-new.zip' },
    });
    fireEvent.click(screen.getByTestId('plugin-install-next'));
    const review = await screen.findByTestId('plugin-install-review');
    expect(review).toHaveTextContent('New Plugin');
    expect(installFromPathMock).not.toHaveBeenCalled();

    // Step 2: explicit confirmation performs the install.
    fireEvent.click(screen.getByTestId('plugin-install-confirm'));
    await waitFor(() => expect(installFromPathMock).toHaveBeenCalledWith('/tmp/acme-new.zip'));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.queryByText('plugins.install.title')).not.toBeInTheDocument(),
    );
  });

  it('shows a copyable error when package inspection fails', async () => {
    inspectPackageMock.mockRejectedValue(new Error('manifest invalid'));

    render(<ExtensionManagementPage />);
    fireEvent.click(screen.getByTestId('plugin-install-button'));
    fireEvent.change(await screen.findByPlaceholderText('plugins.install.pathPlaceholder'), {
      target: { value: '/tmp/broken.zip' },
    });
    fireEvent.click(screen.getByTestId('plugin-install-next'));

    await waitFor(() =>
      expect(screen.getByTestId('plugin-install-error')).toHaveTextContent('manifest invalid'),
    );
    expect(screen.getByTestId('copyable-error-copy')).toBeInTheDocument();
    // Nothing was written; dialog stays open so the user can retry or copy.
    expect(installFromPathMock).not.toHaveBeenCalled();
    expect(screen.getByText('plugins.install.title')).toBeInTheDocument();
  });

  it('renders the package icon image when the plugin declares one', async () => {
    pluginState.extensions = [
      makePlugin({
        id: 'acme.branded',
        name: 'Branded',
        icon: 'assets/logo.svg',
        enabled: true,
      }),
    ];
    readExtensionFileMock.mockResolvedValue(new Uint8Array([60, 115, 118, 103])); // "<svg"

    render(<ExtensionManagementPage />);

    const iconSlot = within(card('acme.branded')).getByTestId('plugin-card-icon');
    await waitFor(() =>
      expect(within(iconSlot).getByTestId('plugin-card-icon-img')).toBeInTheDocument(),
    );
    expect(readExtensionFileMock).toHaveBeenCalledWith('acme.branded', 'assets/logo.svg');
  });

  it('falls back to the letter avatar when no icon is declared', () => {
    pluginState.extensions = [makePlugin({ id: 'acme.plain', name: 'Plain' })];
    render(<ExtensionManagementPage />);

    const slot = within(card('acme.plain')).getByTestId('plugin-card-icon');
    expect(slot).toHaveTextContent('P');
    expect(slot.querySelector('[data-testid="plugin-card-icon-img"]')).toBeNull();
    expect(readExtensionFileMock).not.toHaveBeenCalled();
  });

  it('falls back to the letter avatar when the icon cannot be read', async () => {
    pluginState.extensions = [
      makePlugin({ id: 'acme.broken', name: 'Broken', icon: 'assets/icon.svg' }),
    ];
    readExtensionFileMock.mockRejectedValue(new Error('plugin disabled or missing'));

    render(<ExtensionManagementPage />);
    const slot = within(card('acme.broken')).getByTestId('plugin-card-icon');
    await waitFor(() => expect(slot).toHaveTextContent('B'));
    expect(readExtensionFileMock).toHaveBeenCalledWith('acme.broken', 'assets/icon.svg');
  });
});
