import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { WorkspaceView } from '../WorkspaceView';
import type { PluginSummary } from '../../../types/plugin';

const { listenMock, pluginState, tabsState, openMock } = vi.hoisted(() => ({
  listenMock: vi.fn(),
  pluginState: {
    plugins: [] as Array<Record<string, unknown>>,
    loaded: true,
    error: null as string | null,
    fetchCount: 0,
  },
  tabsState: {
    tabs: [] as Array<Record<string, unknown>>,
    activeKey: null as string | null,
  },
  openMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: Parameters<typeof listenMock>) => listenMock(...args),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/pluginStore', () => ({
  usePluginStore: Object.assign((sel: (s: typeof pluginState) => unknown) => sel(pluginState), {
    getState: () => ({
      ...pluginState,
      byId: (id: string) => (pluginState.plugins as Array<{ id: string }>).find((p) => p.id === id),
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
      closeByPlugin: vi.fn(),
    }),
  }),
}));

vi.mock('../PluginPageShell', () => ({
  PluginPageShell: ({ tab, active }: { tab: { key: string }; active: boolean }) => (
    <div data-testid="plugin-shell-stub" data-active={String(active)}>
      {tab.key}
    </div>
  ),
}));

function makePlugin(overrides: Partial<PluginSummary> = {}): PluginSummary {
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
  pluginState.plugins = [];
  pluginState.loaded = true;
  pluginState.error = null;
  pluginState.fetchCount = 0;
  tabsState.tabs = [];
  tabsState.activeKey = null;
  listenMock.mockReset().mockResolvedValue(() => {});
  openMock.mockClear();
});

afterEach(cleanup);

describe('WorkspaceView', () => {
  it('renders navigator plus default cards when no tab is open', () => {
    pluginState.plugins = [makePlugin()];
    pluginState.loaded = false;

    render(<WorkspaceView />);

    expect(screen.getByTestId('workspace-navigator')).toBeInTheDocument();
    expect(screen.getByTestId('workspace-default-cards')).toBeInTheDocument();
    // Triggers the initial plugin list load itself.
    expect(pluginState.fetchCount).toBe(1);
  });

  it('renders one preserved shell per open tab instead of the default view', async () => {
    pluginState.plugins = [makePlugin()];
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
    pluginState.plugins = [makePlugin()];

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
    pluginState.plugins = [makePlugin(), makePlugin({ id: 'acme.off', enabled: false })];

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
});
