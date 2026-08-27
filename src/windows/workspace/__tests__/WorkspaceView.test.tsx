import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { WorkspaceView } from '../WorkspaceView';
import type { ExtensionSummary } from '../../../types/extension';

const { listenMock, pluginState, tabsState, openMock, closeByPluginMock } = vi.hoisted(() => ({
  listenMock: vi.fn(),
  pluginState: {
    extensions: [] as Array<Record<string, unknown>>,
    loaded: true,
    error: null as string | null,
    fetchCount: 0,
  },
  tabsState: {
    tabs: [] as Array<Record<string, unknown>>,
    activeKey: null as string | null,
  },
  openMock: vi.fn(),
  closeByPluginMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: Parameters<typeof listenMock>) => listenMock(...args),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/extensionStore', () => ({
  useExtensionStore: Object.assign((sel: (s: typeof pluginState) => unknown) => sel(pluginState), {
    getState: () => ({
      ...pluginState,
      byId: (id: string) => (pluginState.extensions as Array<{ id: string }>).find((p) => p.id === id),
      fetch: async () => {
        pluginState.fetchCount += 1;
      },
    }),
  }),
}));

vi.mock('../../../stores/workspaceTabsStore', () => ({
  workspaceTabKey: (pluginId: string, pageId: string) => `${pluginId}:${pageId}`,
  useWorkspaceTabsStore: Object.assign((sel: (s: typeof tabsState) => unknown) => sel(tabsState), {
    getState: () => ({
      ...tabsState,
      open: openMock,
      activate: vi.fn(),
      close: vi.fn(),
      closeByPlugin: closeByPluginMock,
    }),
  }),
}));

vi.mock('../ExtensionPageShell', () => ({
  ExtensionPageShell: ({ tab, active }: { tab: { key: string }; active: boolean }) => (
    <div data-testid="plugin-shell-stub" data-active={String(active)}>
      {tab.key}
    </div>
  ),
}));

function makePlugin(overrides: Partial<ExtensionSummary> = {}): ExtensionSummary {
  return {
    id: 'acme.bill-audit',
    name: 'Bill Audit',
    version: '1.0.0',
    apiVersion: 2,
    enabled: true,
    permissions: [],
    pages: [{ id: 'quota-check', title: 'Quota Check' }],
    themes: [],
    ...overrides,
  };
}

function openPageHandler(): ((event: { payload?: unknown }) => void) | undefined {
  const call = listenMock.mock.calls.find(([event]) => event === 'plugins:open-page');
  return call?.[1] as ((event: { payload?: unknown }) => void) | undefined;
}

beforeEach(() => {
  pluginState.extensions = [];
  pluginState.loaded = true;
  pluginState.error = null;
  pluginState.fetchCount = 0;
  tabsState.tabs = [];
  tabsState.activeKey = null;
  listenMock.mockReset().mockResolvedValue(() => {});
  openMock.mockClear();
  closeByPluginMock.mockClear();
});

afterEach(cleanup);

describe('WorkspaceView', () => {
  it('renders navigator plus default cards when no tab is open', () => {
    pluginState.extensions = [makePlugin()];
    pluginState.loaded = false;

    render(<WorkspaceView />);

    expect(screen.getByTestId('workspace-navigator')).toBeInTheDocument();
    expect(screen.getByTestId('workspace-default-cards')).toBeInTheDocument();
    // Triggers the initial plugin list load itself.
    expect(pluginState.fetchCount).toBe(1);
  });

  it('renders one preserved shell per open tab instead of the default view', async () => {
    pluginState.extensions = [makePlugin()];
    tabsState.tabs = [
      {
        key: 'acme.bill-audit:quota-check',
        pluginId: 'acme.bill-audit',
        pageId: 'quota-check',
        title: 'Quota Check',
        version: '1.0.0',
      },
    ];
    tabsState.activeKey = 'acme.bill-audit:quota-check';

    render(<WorkspaceView />);
    await act(async () => {});

    expect(screen.queryByTestId('workspace-default-cards')).not.toBeInTheDocument();
    expect(screen.getByTestId('workspace-tabbar')).toBeInTheDocument();
    const shells = screen.getAllByTestId('plugin-shell-stub');
    expect(shells).toHaveLength(1);
    expect(shells[0]).toHaveTextContent('acme.bill-audit:quota-check');
    expect(shells[0].getAttribute('data-active')).toBe('true');
  });

  it('opens and activates a tab for a valid plugins:open-page deep link', async () => {
    pluginState.extensions = [makePlugin()];

    render(<WorkspaceView />);
    await act(async () => {});

    const handler = openPageHandler();
    expect(handler).toBeDefined();

    await act(async () => {
      handler?.({ payload: { pluginId: 'acme.bill-audit', pageId: 'quota-check', params: {} } });
    });
    expect(openMock).toHaveBeenCalledWith(
      expect.objectContaining({
        key: 'acme.bill-audit:quota-check',
        pluginId: 'acme.bill-audit',
        pageId: 'quota-check',
        title: 'Quota Check',
        version: '1.0.0',
      }),
    );
  });

  it('ignores invalid deep links (unknown/disabled plugin, missing or unknown page)', async () => {
    pluginState.extensions = [makePlugin(), makePlugin({ id: 'acme.off', enabled: false })];

    render(<WorkspaceView />);
    await act(async () => {});

    const handler = openPageHandler();
    await act(async () => {
      handler?.({ payload: { pluginId: 'ghost', pageId: 'main' } });
      handler?.({ payload: { pluginId: 'acme.off', pageId: 'main' } });
      handler?.({ payload: { pluginId: 'acme.bill-audit' } });
      handler?.({ payload: { pluginId: 'acme.bill-audit', pageId: 'nope' } });
      handler?.({});
    });

    expect(openMock).not.toHaveBeenCalled();
  });

  it('closes tabs of plugins that were disabled or removed by an external refresh (BUG-F4-01)', async () => {
    tabsState.tabs = [
      {
        key: 'acme.bill-audit:quota-check',
        pluginId: 'acme.bill-audit',
        pageId: 'quota-check',
        title: 'Quota Check',
        version: '1.0.0',
      },
      {
        key: 'acme.keep:main',
        pluginId: 'acme.keep',
        pageId: 'main',
        title: 'Keep',
        version: '1.0.0',
      },
    ];
    pluginState.extensions = [makePlugin(), makePlugin({ id: 'acme.keep', name: 'Keep' })];

    const view = render(<WorkspaceView />);
    await act(async () => {});
    expect(closeByPluginMock).not.toHaveBeenCalled();

    // Another window disables `acme.bill-audit`; the refreshed list arrives.
    pluginState.extensions = [makePlugin({ enabled: false }), makePlugin({ id: 'acme.keep' })];
    view.rerender(<WorkspaceView />);
    await act(async () => {});
    expect(closeByPluginMock).toHaveBeenCalledTimes(1);
    expect(closeByPluginMock).toHaveBeenCalledWith('acme.bill-audit');

    // A later refresh where the plugin is gone entirely also closes its tabs.
    closeByPluginMock.mockClear();
    pluginState.extensions = [makePlugin({ id: 'acme.keep' })];
    view.rerender(<WorkspaceView />);
    await act(async () => {});
    expect(closeByPluginMock).toHaveBeenCalledWith('acme.bill-audit');

    // Enabled plugins are never touched.
    expect(closeByPluginMock).not.toHaveBeenCalledWith('acme.keep');
  });

  it('does not diff-close tabs before the plugin store has loaded', async () => {
    // Restored tabs exist while the initial fetch is still in flight; the
    // empty placeholder list must not close them.
    tabsState.tabs = [
      {
        key: 'acme.bill-audit:quota-check',
        pluginId: 'acme.bill-audit',
        pageId: 'quota-check',
        title: 'Quota Check',
        version: '1.0.0',
      },
    ];
    pluginState.extensions = [];
    pluginState.loaded = false;

    const view = render(<WorkspaceView />);
    await act(async () => {});
    expect(closeByPluginMock).not.toHaveBeenCalled();

    // Store finishes loading with the plugin still enabled → tab survives.
    pluginState.loaded = true;
    pluginState.extensions = [makePlugin()];
    view.rerender(<WorkspaceView />);
    await act(async () => {});
    expect(closeByPluginMock).not.toHaveBeenCalled();
  });
});
