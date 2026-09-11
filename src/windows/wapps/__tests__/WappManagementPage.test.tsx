import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { WappManagementPage, ExtensionManagementPage } from '../WappManagementPage';
import { WAPP_API_VERSION, type WappSummary } from '../../../types/wapp';

const {
  wappState,
  setEnabledMock,
  removeMock,
  fetchMock,
  closeByWappMock,
  openTabMock,
  inspectPackageMock,
  installFromPathMock,
  readWappFileMock,
  confirmSpy,
} = vi.hoisted(() => ({
  wappState: {
    _list: [] as Array<Record<string, unknown>>,
    get wapps() {
      return this._list;
    },
    set wapps(v: Array<Record<string, unknown>>) {
      this._list = v;
    },
    loaded: true,
    error: null as string | null,
  },
  setEnabledMock: vi.fn(),
  removeMock: vi.fn(),
  fetchMock: vi.fn(),
  closeByWappMock: vi.fn(),
  openTabMock: vi.fn(),
  inspectPackageMock: vi.fn(),
  installFromPathMock: vi.fn(),
  readWappFileMock: vi.fn(),
  confirmSpy: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [confirmSpy, null],
}));

vi.mock('../../../stores/wappStore', () => ({
  useWappStore: Object.assign((sel: (s: typeof wappState) => unknown) => sel(wappState), {
    getState: () => ({
      ...wappState,
      fetch: fetchMock,
      setEnabled: setEnabledMock,
      remove: removeMock,
      byId: (id: string) => (wappState.wapps as Array<{ id: string }>).find((p) => p.id === id),
    }),
  }),
}));

vi.mock('../../../stores/workspaceTabsStore', () => ({
  workspaceTabKey: (wappId: string, pageId: string) => `${wappId}:${pageId}`,
  useWorkspaceTabsStore: Object.assign(
    (sel: (s: Record<string, unknown>) => unknown) => sel({ tabs: [], activeKey: null }),
    {
      getState: () => ({ open: openTabMock, closeByWapp: closeByWappMock }),
    },
  ),
}));

vi.mock('../../../commands/wapps', () => ({
  WAPPS_CHANGED_EVENT: 'wapps:changed',
  wappCommands: {
    inspectWappPackageWithDialog: (...args: unknown[]) => inspectPackageMock(...args),
    installWapp: (...args: unknown[]) => installFromPathMock(...args),
    readWappFile: (...args: unknown[]) => readWappFileMock(...args),
  },
}));

function makePlugin(overrides: Partial<WappSummary> = {}): WappSummary {
  return {
    id: 'acme.bill-audit',
    name: 'Bill Audit',
    version: '1.0.0',
    apiVersion: WAPP_API_VERSION,
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
    .getAllByTestId('extension-card')
    .find((c) => c.getAttribute('data-wapp-id') === id);
  if (!el) throw new Error(`card ${id} not found`);
  return el;
}

beforeEach(() => {
  wappState.wapps = [];
  wappState.wapps = [];
  wappState.loaded = true;
  wappState.error = null;
  setEnabledMock.mockReset().mockResolvedValue(undefined);
  removeMock.mockReset().mockResolvedValue(undefined);
  fetchMock.mockReset().mockResolvedValue(undefined);
  inspectPackageMock.mockReset().mockResolvedValue({
    pickToken: 'pick-acme-new',
    packageLabel: 'acme-new.zip',
    manifest: {
      id: 'acme.new',
      name: 'New Wapp',
      version: '1.0.0',
      apiVersion: 2,
      author: 'Acme',
      contributes: { pages: [], themes: [] },
      permissions: ['storage:local'],
    },
  });
  installFromPathMock.mockReset();
  readWappFileMock.mockReset().mockResolvedValue(new Uint8Array([137, 80, 78, 71]));
  closeByWappMock.mockReset();
  openTabMock.mockReset();
  confirmSpy.mockReset().mockResolvedValue(true);
});

afterEach(cleanup);

describe('ExtensionManagementPage', () => {
  it('renders header with installed count and one card per plugin', () => {
    wappState.wapps = [makePlugin(), makePlugin({ id: 'acme.midnight', name: 'Midnight' })];

    render(<ExtensionManagementPage />);

    expect(screen.getByTestId('extension-management-page')).toBeInTheDocument();
    expect(screen.getByText('extensions.page.title')).toBeInTheDocument();
    expect(screen.getAllByTestId('extension-card')).toHaveLength(2);
    expect(screen.getByText('Bill Audit')).toBeInTheDocument();
    expect(screen.getByText('Midnight')).toBeInTheDocument();
    // Permission badge with tooltip explanation from the in-component map.
    const badges = within(card('acme.bill-audit')).getByText('context:connections');
    expect(badges).toBeInTheDocument();
  });

  it('defaults the filter to Workspace and switches through all/theme chips', () => {
    wappState.wapps = [
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
    expect(screen.getAllByTestId('extension-card')).toHaveLength(1);
    expect(screen.getByText('Bill Audit')).toBeInTheDocument();
    const workspaceChip = screen.getByTestId('extension-filter-workspace');
    expect(workspaceChip.className).toMatch(/bg-accent/);
    expect(screen.getByTestId('extension-filter-all').className).not.toMatch(/bg-accent/);

    fireEvent.click(screen.getByTestId('extension-filter-theme'));
    expect(screen.getAllByTestId('extension-card')).toHaveLength(1);
    expect(screen.getByTestId('extension-management-page')).toHaveTextContent('Midnight');

    fireEvent.click(screen.getByTestId('extension-filter-all'));
    expect(screen.getAllByTestId('extension-card')).toHaveLength(2);
  });

  it('renders the all view grouped into Workspace pages and Themes sections', () => {
    wappState.wapps = [
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
    fireEvent.click(screen.getByTestId('extension-filter-all'));

    expect(screen.getByTestId('extension-group-workspace')).toHaveTextContent(
      'extensions.page.groupWorkspace',
    );
    expect(screen.getByTestId('extension-group-theme')).toHaveTextContent(
      'extensions.page.groupTheme',
    );
    const workspaceGroup = screen.getByTestId('extension-group-workspace');
    expect(within(workspaceGroup).getAllByTestId('extension-card')).toHaveLength(2);
    expect(within(workspaceGroup).getByText('Bill Audit')).toBeInTheDocument();
    expect(within(workspaceGroup).getByText('Both')).toBeInTheDocument();
    const themeGroup = screen.getByTestId('extension-group-theme');
    expect(within(themeGroup).getAllByTestId('extension-card')).toHaveLength(1);
    expect(within(themeGroup).getByText('Midnight')).toBeInTheDocument();
  });

  it('hides empty groups in the all view and keeps the flat grid for single-kind filters', () => {
    wappState.wapps = [makePlugin(), makePlugin({ id: 'acme.afi', name: 'AFI Pricing' })];

    render(<ExtensionManagementPage />);
    fireEvent.click(screen.getByTestId('extension-filter-all'));

    expect(screen.queryByTestId('extension-group-theme')).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId('extension-group-workspace')).getAllByTestId('extension-card'),
    ).toHaveLength(2);

    // Non-"all" filters keep rendering a single flat grid without headers.
    fireEvent.click(screen.getByTestId('extension-filter-workspace'));
    expect(screen.queryByTestId('extension-group-workspace')).not.toBeInTheDocument();
    expect(screen.getAllByTestId('extension-card')).toHaveLength(2);
  });

  it('narrows cards by search text across name/id/description', () => {
    wappState.wapps = [makePlugin(), makePlugin({ id: 'acme.afi', name: 'AFI Pricing' })];

    render(<ExtensionManagementPage />);
    fireEvent.change(screen.getByTestId('extension-search-input'), {
      target: { value: 'afi' },
    });

    expect(screen.getAllByTestId('extension-card')).toHaveLength(1);
    expect(screen.getByText('AFI Pricing')).toBeInTheDocument();
  });

  it('shows an empty state when nothing matches', () => {
    wappState.wapps = [];

    render(<ExtensionManagementPage />);

    expect(screen.getByTestId('extension-page-empty')).toBeInTheDocument();
    expect(screen.queryByTestId('extension-card')).not.toBeInTheDocument();
  });

  it('toggles a plugin off through the store and closes its workspace tabs', async () => {
    wappState.wapps = [makePlugin()];
    setEnabledMock.mockResolvedValue(undefined);

    render(<ExtensionManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('extension-toggle'));

    await waitFor(() => expect(setEnabledMock).toHaveBeenCalledWith('acme.bill-audit', false));
    await waitFor(() => expect(closeByWappMock).toHaveBeenCalledWith('acme.bill-audit'));
  });

  it('enables a disabled plugin without closing tabs', async () => {
    wappState.wapps = [makePlugin({ enabled: false })];

    render(<ExtensionManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('extension-toggle'));

    await waitFor(() => expect(setEnabledMock).toHaveBeenCalledWith('acme.bill-audit', true));
    expect(closeByWappMock).not.toHaveBeenCalled();
  });

  it('surfaces store errors from a failed toggle', async () => {
    wappState.wapps = [makePlugin()];
    setEnabledMock.mockRejectedValue(new Error('backend refused'));

    render(<ExtensionManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('extension-toggle'));

    await waitFor(() =>
      expect(screen.getByTestId('extension-page-error')).toHaveTextContent('backend refused'),
    );
  });

  it('uninstalls after confirmation and closes related tabs', async () => {
    wappState.wapps = [makePlugin()];
    removeMock.mockResolvedValue(undefined);

    render(<ExtensionManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('extension-uninstall'));

    await waitFor(() => expect(confirmSpy).toHaveBeenCalled());
    expect(confirmSpy.mock.calls[0]?.[0]).toMatchObject({
      title: 'extensions.page.uninstallTitle',
      message: 'extensions.page.uninstallMessage',
    });
    await waitFor(() => expect(removeMock).toHaveBeenCalledWith('acme.bill-audit'));
    expect(closeByWappMock).toHaveBeenCalledWith('acme.bill-audit');
  });

  it('keeps the plugin when uninstall is cancelled', async () => {
    wappState.wapps = [makePlugin()];
    confirmSpy.mockResolvedValue(false);

    render(<ExtensionManagementPage />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('extension-uninstall'));

    await waitFor(() => expect(confirmSpy).toHaveBeenCalled());
    expect(removeMock).not.toHaveBeenCalled();
  });

  it('opens a workspace plugin tab and switches to the workspace view', () => {
    wappState.wapps = [makePlugin()];
    const onOpenInWorkspace = vi.fn();

    render(<ExtensionManagementPage onOpenInWorkspace={onOpenInWorkspace} />);
    fireEvent.click(within(card('acme.bill-audit')).getByTestId('extension-open'));

    expect(openTabMock).toHaveBeenCalledWith({
      key: 'acme.bill-audit:quota-check',
      wappId: 'acme.bill-audit',
      pageId: 'quota-check',
      title: 'Quota Check',
      icon: undefined,
      version: '1.0.0',
    });
    expect(onOpenInWorkspace).toHaveBeenCalledOnce();
  });

  it('renders theme-only cards without an open action and with the settings hint', () => {
    wappState.wapps = [
      makePlugin({
        id: 'acme.midnight',
        name: 'Midnight',
        pages: [],
        themes: [{ id: 'midnight-blue', name: 'Midnight Blue', modes: ['dark'] }],
        permissions: [],
      }),
    ];

    render(<ExtensionManagementPage />);
    fireEvent.click(screen.getByTestId('extension-filter-theme'));
    const themeCard = card('acme.midnight');

    expect(within(themeCard).getByText('extensions.page.themeHint')).toBeInTheDocument();
    expect(within(themeCard).getByText('extensions.page.themeBadge')).toBeInTheDocument();
    expect(within(themeCard).queryByTestId('extension-open')).not.toBeInTheDocument();
  });

  it('greys out API-mismatched plugins and blocks their toggle/open actions', () => {
    wappState.wapps = [makePlugin({ apiVersion: WAPP_API_VERSION + 1 })];

    render(<ExtensionManagementPage />);
    const mismatched = card('acme.bill-audit');

    expect(mismatched.className).toMatch(/opacity-60/);
    expect(within(mismatched).getByText('extensions.page.apiMismatch')).toBeInTheDocument();
    expect((within(mismatched).getByTestId('extension-toggle') as HTMLButtonElement).disabled).toBe(
      true,
    );
    expect(within(mismatched).queryByTestId('extension-open')).not.toBeInTheDocument();
  });

  it('installs through the two-step confirm flow and refreshes the list', async () => {
    installFromPathMock.mockResolvedValue({ id: 'acme.new' });
    fetchMock.mockResolvedValue(undefined);

    render(<ExtensionManagementPage />);
    fireEvent.click(screen.getByTestId('extension-install-button'));

    expect(screen.getByText('extensions.install.title')).toBeInTheDocument();

    // Step 1 → 2: native browse + inspect only, no write yet.
    fireEvent.click(await screen.findByTestId('extension-install-browse-zip'));
    const review = await screen.findByTestId('extension-install-review');
    expect(review).toHaveTextContent('New Wapp');
    expect(inspectPackageMock).toHaveBeenCalledWith('zip');
    expect(installFromPathMock).not.toHaveBeenCalled();

    // Step 2: explicit confirmation performs the install.
    fireEvent.click(screen.getByTestId('extension-install-confirm'));
    await waitFor(() => expect(installFromPathMock).toHaveBeenCalledWith('pick-acme-new'));
    await waitFor(() => expect(fetchMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.queryByText('extensions.install.title')).not.toBeInTheDocument(),
    );
  });

  it('shows a copyable error when package inspection fails', async () => {
    inspectPackageMock.mockRejectedValue(new Error('manifest invalid'));

    render(<ExtensionManagementPage />);
    fireEvent.click(screen.getByTestId('extension-install-button'));
    fireEvent.click(await screen.findByTestId('extension-install-browse-zip'));

    await waitFor(() =>
      expect(screen.getByTestId('extension-install-error')).toHaveTextContent('manifest invalid'),
    );
    expect(screen.getByTestId('copyable-error-copy')).toBeInTheDocument();
    // Nothing was written; dialog stays open so the user can retry or copy.
    expect(installFromPathMock).not.toHaveBeenCalled();
    expect(screen.getByText('extensions.install.title')).toBeInTheDocument();
  });

  it('renders the package icon image when the plugin declares one', async () => {
    wappState.wapps = [
      makePlugin({
        id: 'acme.branded',
        name: 'Branded',
        icon: 'assets/logo.svg',
        enabled: true,
      }),
    ];
    readWappFileMock.mockResolvedValue(new Uint8Array([60, 115, 118, 103])); // "<svg"

    render(<ExtensionManagementPage />);

    const iconSlot = within(card('acme.branded')).getByTestId('extension-card-icon');
    await waitFor(() =>
      expect(within(iconSlot).getByTestId('extension-card-icon-img')).toBeInTheDocument(),
    );
    expect(readWappFileMock).toHaveBeenCalledWith('acme.branded', 'assets/logo.svg');
  });

  it('falls back to the letter avatar when no icon is declared', () => {
    wappState.wapps = [makePlugin({ id: 'acme.plain', name: 'Plain' })];
    render(<ExtensionManagementPage />);

    const slot = within(card('acme.plain')).getByTestId('extension-card-icon');
    expect(slot).toHaveTextContent('P');
    expect(slot.querySelector('[data-testid="extension-card-icon-img"]')).toBeNull();
    expect(readWappFileMock).not.toHaveBeenCalled();
  });

  it('falls back to the letter avatar when the icon cannot be read', async () => {
    wappState.wapps = [makePlugin({ id: 'acme.broken', name: 'Broken', icon: 'assets/icon.svg' })];
    readWappFileMock.mockRejectedValue(new Error('plugin disabled or missing'));

    render(<ExtensionManagementPage />);
    const slot = within(card('acme.broken')).getByTestId('extension-card-icon');
    await waitFor(() => expect(slot).toHaveTextContent('B'));
    expect(readWappFileMock).toHaveBeenCalledWith('acme.broken', 'assets/icon.svg');
  });
});
