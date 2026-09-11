import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, render, screen } from '@testing-library/react';
import { WorkspaceView } from '../WorkspaceView';
import type { WappSummary } from '../../../types/wapp';

const { listenMock, wappState, tabsState, openMock, closeByWappMock } = vi.hoisted(() => {
  const pState = {
    _list: [] as Array<Record<string, unknown>>,
    get wapps() {
      return this._list;
    },
    set wapps(v: Array<Record<string, unknown>>) {
      this._list = v;
    },
    loaded: true,
    error: null as string | null,
    fetchCount: 0,
  };
  return {
    listenMock: vi.fn(),
    wappState: pState,
    tabsState: {
      tabs: [] as Array<Record<string, unknown>>,
      activeKey: null as string | null,
    },
    openMock: vi.fn(),
    closeByWappMock: vi.fn(),
  };
});

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: Parameters<typeof listenMock>) => listenMock(...args),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/wappStore', () => ({
  useWappStore: Object.assign((sel: (s: typeof wappState) => unknown) => sel(wappState), {
    getState: () => ({
      ...wappState,
      byId: (id: string) => (wappState.wapps as Array<{ id: string }>).find((p) => p.id === id),
      fetch: async () => {
        wappState.fetchCount += 1;
      },
    }),
  }),
}));

vi.mock('../../../stores/workspaceTabsStore', () => ({
  workspaceTabKey: (wappId: string, pageId: string) => `${wappId}:${pageId}`,
  useWorkspaceTabsStore: Object.assign((sel: (s: typeof tabsState) => unknown) => sel(tabsState), {
    getState: () => ({
      ...tabsState,
      open: openMock,
      activate: vi.fn(),
      close: vi.fn(),
      closeByWapp: closeByWappMock,
    }),
  }),
}));

vi.mock('../WappPageShell', () => ({
  WappPageShell: ({ tab, active }: { tab: { key: string }; active: boolean }) => (
    <div data-testid="wapp-shell-stub" data-active={String(active)}>
      {tab.key}
    </div>
  ),
}));

function makePlugin(overrides: Partial<WappSummary> = {}): WappSummary {
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
  const call = listenMock.mock.calls.find(([event]) => event === 'wapps:open-page');
  return call?.[1] as ((event: { payload?: unknown }) => void) | undefined;
}

beforeEach(() => {
  wappState.wapps = [];
  wappState.loaded = true;
  wappState.error = null;
  wappState.fetchCount = 0;
  tabsState.tabs = [];
  tabsState.activeKey = null;
  listenMock.mockReset().mockResolvedValue(() => {});
  openMock.mockClear();
  closeByWappMock.mockClear();
});

afterEach(cleanup);

describe('WorkspaceView', () => {
  it('renders navigator plus default cards when no tab is open', () => {
    wappState.wapps = [makePlugin()];
    wappState.loaded = false;

    render(<WorkspaceView />);

    expect(screen.getByTestId('workspace-navigator')).toBeInTheDocument();
    expect(screen.getByTestId('workspace-default-cards')).toBeInTheDocument();
    // Triggers the initial wapp list load itself.
    expect(wappState.fetchCount).toBe(1);
  });

  it('renders one preserved shell per open tab instead of the default view', async () => {
    wappState.wapps = [makePlugin()];
    tabsState.tabs = [
      {
        key: 'acme.bill-audit:quota-check',
        wappId: 'acme.bill-audit',
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
    const shells = screen.getAllByTestId('wapp-shell-stub');
    expect(shells).toHaveLength(1);
    expect(shells[0]).toHaveTextContent('acme.bill-audit:quota-check');
    expect(shells[0].getAttribute('data-active')).toBe('true');
  });

  it('opens and activates a tab for a valid wapps:open-page deep link', async () => {
    wappState.wapps = [makePlugin()];

    render(<WorkspaceView />);
    await act(async () => {});

    const handler = openPageHandler();
    expect(handler).toBeDefined();

    await act(async () => {
      handler?.({ payload: { wappId: 'acme.bill-audit', pageId: 'quota-check', params: {} } });
    });
    expect(openMock).toHaveBeenCalledWith(
      expect.objectContaining({
        key: 'acme.bill-audit:quota-check',
        wappId: 'acme.bill-audit',
        pageId: 'quota-check',
        title: 'Quota Check',
        version: '1.0.0',
      }),
    );
  });

  it('ignores invalid deep links (unknown/disabled wapp, missing or unknown page)', async () => {
    wappState.wapps = [makePlugin(), makePlugin({ id: 'acme.off', enabled: false })];

    render(<WorkspaceView />);
    await act(async () => {});

    const handler = openPageHandler();
    await act(async () => {
      handler?.({ payload: { wappId: 'ghost', pageId: 'main' } });
      handler?.({ payload: { wappId: 'acme.off', pageId: 'main' } });
      handler?.({ payload: { wappId: 'acme.bill-audit' } });
      handler?.({ payload: { wappId: 'acme.bill-audit', pageId: 'nope' } });
      handler?.({});
    });

    expect(openMock).not.toHaveBeenCalled();
  });

  it('closes tabs of plugins that were disabled or removed by an external refresh (BUG-F4-01)', async () => {
    tabsState.tabs = [
      {
        key: 'acme.bill-audit:quota-check',
        wappId: 'acme.bill-audit',
        pageId: 'quota-check',
        title: 'Quota Check',
        version: '1.0.0',
      },
      {
        key: 'acme.keep:main',
        wappId: 'acme.keep',
        pageId: 'main',
        title: 'Keep',
        version: '1.0.0',
      },
    ];
    wappState.wapps = [makePlugin(), makePlugin({ id: 'acme.keep', name: 'Keep' })];

    const view = render(<WorkspaceView />);
    await act(async () => {});
    expect(closeByWappMock).not.toHaveBeenCalled();

    // Another window disables `acme.bill-audit`; the refreshed list arrives.
    wappState.wapps = [makePlugin({ enabled: false }), makePlugin({ id: 'acme.keep' })];
    view.rerender(<WorkspaceView />);
    await act(async () => {});
    expect(closeByWappMock).toHaveBeenCalledTimes(1);
    expect(closeByWappMock).toHaveBeenCalledWith('acme.bill-audit');

    // A later refresh where the wapp is gone entirely also closes its tabs.
    closeByWappMock.mockClear();
    wappState.wapps = [makePlugin({ id: 'acme.keep' })];
    view.rerender(<WorkspaceView />);
    await act(async () => {});
    expect(closeByWappMock).toHaveBeenCalledWith('acme.bill-audit');

    // Enabled plugins are never touched.
    expect(closeByWappMock).not.toHaveBeenCalledWith('acme.keep');
  });

  it('does not diff-close tabs before the wapp store has loaded', async () => {
    // Restored tabs exist while the initial fetch is still in flight; the
    // empty placeholder list must not close them.
    tabsState.tabs = [
      {
        key: 'acme.bill-audit:quota-check',
        wappId: 'acme.bill-audit',
        pageId: 'quota-check',
        title: 'Quota Check',
        version: '1.0.0',
      },
    ];
    wappState.wapps = [];
    wappState.loaded = false;

    const view = render(<WorkspaceView />);
    await act(async () => {});
    expect(closeByWappMock).not.toHaveBeenCalled();

    // Store finishes loading with the wapp still enabled → tab survives.
    wappState.loaded = true;
    wappState.wapps = [makePlugin()];
    view.rerender(<WorkspaceView />);
    await act(async () => {});
    expect(closeByWappMock).not.toHaveBeenCalled();
  });

  it('renders a resizable sidebar handle for the navigator', () => {
    wappState.wapps = [makePlugin()];
    render(<WorkspaceView />);

    const handle = screen.getByTestId('workspace-sidebar-resize');
    expect(handle).toBeInTheDocument();
    expect(handle).toHaveClass('cursor-col-resize');

    const nav = screen.getByTestId('workspace-navigator');
    expect(nav).toHaveStyle({ width: '200px' });
  });
});
