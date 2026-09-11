import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { WorkspaceNavigator } from '../WorkspaceNavigator';
import type { WappSummary } from '../../../types/wapp';

const { wappState, tabsState, openMock } = vi.hoisted(() => {
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
  };
  return {
    wappState: pState,
    tabsState: {
      activeKey: null as string | null,
    },
    openMock: vi.fn(),
  };
});

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/wappStore', () => ({
  useWappStore: Object.assign((sel: (s: typeof wappState) => unknown) => sel(wappState), {
    getState: () => ({
      ...wappState,
      byId: (id: string) => (wappState.wapps as Array<{ id: string }>).find((p) => p.id === id),
      fetch: vi.fn(),
    }),
  }),
}));

vi.mock('../../../stores/workspaceTabsStore', () => ({
  workspaceTabKey: (wappId: string, pageId: string) => `${wappId}:${pageId}`,
  useWorkspaceTabsStore: Object.assign((sel: (s: typeof tabsState) => unknown) => sel(tabsState), {
    getState: () => ({ ...tabsState, open: openMock }),
  }),
}));

function makePlugin(overrides: Partial<WappSummary> = {}): WappSummary {
  return {
    id: 'acme.bill-audit',
    name: 'Bill Audit',
    version: '1.0.0',
    apiVersion: 2,
    author: 'Acme',
    description: 'Compare bills against quotas',
    enabled: true,
    permissions: [],
    pages: [{ id: 'quota-check', title: 'Quota Check' }],
    themes: [],
    ...overrides,
  };
}

beforeEach(() => {
  wappState.wapps = [];
  wappState.error = null;
  tabsState.activeKey = null;
  openMock.mockClear();
});

afterEach(cleanup);

describe('WorkspaceNavigator', () => {
  it('renders one item per enabled-wapp page with title and description', () => {
    wappState.wapps = [
      makePlugin(),
      makePlugin({
        id: 'acme.afi',
        name: 'AFI Pricing',
        description: 'Inspect installment pricing rules',
        pages: [
          { id: 'pricing', title: 'Pricing Viewer', icon: 'assets/icon.svg' },
          { id: 'rules', title: 'Rules' },
        ],
      }),
      makePlugin({ id: 'acme.off', enabled: false }),
    ];

    render(<WorkspaceNavigator />);

    const items = screen.getAllByTestId('workspace-nav-item');
    expect(items).toHaveLength(3);
    expect(screen.getByText('Quota Check')).toBeInTheDocument();
    expect(screen.getByText('Compare bills against quotas')).toBeInTheDocument();
    expect(screen.getByText('Pricing Viewer')).toBeInTheDocument();
    expect(screen.getByText('Rules')).toBeInTheDocument();
    expect(screen.queryByText('Bill Audit')).not.toBeInTheDocument();
    // Disabled plugins contribute nothing.
    expect(items.map((i) => i.getAttribute('data-page-key'))).toEqual([
      'acme.bill-audit:quota-check',
      'acme.afi:pricing',
      'acme.afi:rules',
    ]);
  });

  it('highlights the item matching the active tab key', () => {
    wappState.wapps = [makePlugin()];
    tabsState.activeKey = 'acme.bill-audit:quota-check';

    render(<WorkspaceNavigator />);

    const item = screen.getByTestId('workspace-nav-item');
    expect(item.className).toMatch(/bg-accent\/20/);
  });

  it('opens the corresponding tab on click', () => {
    wappState.wapps = [makePlugin()];

    render(<WorkspaceNavigator />);
    fireEvent.click(screen.getByTestId('workspace-nav-item'));

    expect(openMock).toHaveBeenCalledWith({
      key: 'acme.bill-audit:quota-check',
      wappId: 'acme.bill-audit',
      pageId: 'quota-check',
      title: 'Quota Check',
      icon: undefined,
      version: '1.0.0',
    });
  });

  it('shows the empty-state guidance and opens the extensions page', () => {
    const onOpenExtensions = vi.fn();
    render(<WorkspaceNavigator onOpenExtensions={onOpenExtensions} />);

    expect(screen.getByText('workspace.emptyHint')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('workspace-open-extensions'));
    expect(onOpenExtensions).toHaveBeenCalledOnce();
  });

  it('cleans up after unmount', () => {
    wappState.wapps = [makePlugin()];
    const { unmount } = render(<WorkspaceNavigator />);
    unmount();
    cleanup();
    expect(screen.queryByTestId('workspace-navigator')).not.toBeInTheDocument();
  });
});
