import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { WorkspaceNavigator } from '../WorkspaceNavigator';
import type { PluginSummary } from '../../../types/plugin';

const { pluginState, tabsState, openMock } = vi.hoisted(() => ({
  pluginState: {
    plugins: [] as Array<Record<string, unknown>>,
    loaded: true,
    error: null as string | null,
  },
  tabsState: {
    activeKey: null as string | null,
  },
  openMock: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/pluginStore', () => ({
  usePluginStore: Object.assign((sel: (s: typeof pluginState) => unknown) => sel(pluginState), {
    getState: () => ({
      ...pluginState,
      byId: (id: string) => (pluginState.plugins as Array<{ id: string }>).find((p) => p.id === id),
      fetch: vi.fn(),
    }),
  }),
}));

vi.mock('../../../stores/workspaceTabsStore', () => ({
  workspaceTabKey: (pluginId: string, pageId: string) => `${pluginId}:${pageId}`,
  useWorkspaceTabsStore: Object.assign((sel: (s: typeof tabsState) => unknown) => sel(tabsState), {
    getState: () => ({ ...tabsState, open: openMock }),
  }),
}));

function makePlugin(overrides: Partial<PluginSummary> = {}): PluginSummary {
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
  pluginState.plugins = [];
  pluginState.error = null;
  tabsState.activeKey = null;
  openMock.mockClear();
});

afterEach(cleanup);

describe('WorkspaceNavigator', () => {
  it('renders one item per enabled-plugin page with title and description', () => {
    pluginState.plugins = [
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
    pluginState.plugins = [makePlugin()];
    tabsState.activeKey = 'acme.bill-audit:quota-check';

    render(<WorkspaceNavigator />);

    const item = screen.getByTestId('workspace-nav-item');
    expect(item.className).toMatch(/bg-accent\/20/);
  });

  it('opens the corresponding tab on click', () => {
    pluginState.plugins = [makePlugin()];

    render(<WorkspaceNavigator />);
    fireEvent.click(screen.getByTestId('workspace-nav-item'));

    expect(openMock).toHaveBeenCalledWith({
      key: 'acme.bill-audit:quota-check',
      pluginId: 'acme.bill-audit',
      pageId: 'quota-check',
      title: 'Quota Check',
      icon: undefined,
      version: '1.0.0',
    });
  });

  it('shows the empty-state guidance and opens the plugins page', () => {
    const onOpenPlugins = vi.fn();
    render(<WorkspaceNavigator onOpenPlugins={onOpenPlugins} />);

    expect(screen.getByText('workspace.emptyHint')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('workspace-open-plugins'));
    expect(onOpenPlugins).toHaveBeenCalledOnce();
  });

  it('cleans up after unmount', () => {
    pluginState.plugins = [makePlugin()];
    const { unmount } = render(<WorkspaceNavigator />);
    unmount();
    cleanup();
    expect(screen.queryByTestId('workspace-navigator')).not.toBeInTheDocument();
  });
});
