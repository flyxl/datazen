import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { WorkspaceTabBar } from '../WorkspaceTabBar';
import type { WorkspaceTab } from '../../../stores/workspaceTabsStore';

const { tabsState, activateMock, closeMock } = vi.hoisted(() => ({
  tabsState: {
    tabs: [] as WorkspaceTab[],
    activeKey: null as string | null,
  },
  activateMock: vi.fn(),
  closeMock: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../stores/workspaceTabsStore', () => ({
  useWorkspaceTabsStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({ ...tabsState, activate: activateMock, close: closeMock }),
}));

function makeTab(overrides: Partial<WorkspaceTab> = {}): WorkspaceTab {
  return {
    key: 'acme.bill-audit:quota-check',
    pluginId: 'acme.bill-audit',
    pageId: 'quota-check',
    title: 'Quota Check',
    version: '1.0.0',
    ...overrides,
  };
}

beforeEach(() => {
  tabsState.tabs = [];
  tabsState.activeKey = null;
  activateMock.mockClear();
  closeMock.mockClear();
});

afterEach(cleanup);

describe('WorkspaceTabBar', () => {
  it('renders nothing when no tab is open', () => {
    const { container } = render(<WorkspaceTabBar />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders one entry per open tab', () => {
    tabsState.tabs = [
      makeTab(),
      makeTab({
        key: 'acme.afi:pricing',
        pluginId: 'acme.afi',
        pageId: 'pricing',
        title: 'Pricing',
      }),
    ];
    tabsState.activeKey = 'acme.afi:pricing';

    render(<WorkspaceTabBar />);

    expect(screen.getAllByTestId('workspace-tab')).toHaveLength(2);
    expect(screen.getByText('Quota Check')).toBeInTheDocument();
    expect(screen.getByText('Pricing')).toBeInTheDocument();
  });

  it('activates a tab on click', () => {
    tabsState.tabs = [makeTab(), makeTab({ key: 'acme.afi:pricing', title: 'Pricing' })];
    tabsState.activeKey = 'acme.afi:pricing';

    render(<WorkspaceTabBar />);
    fireEvent.click(screen.getByText('Quota Check'));

    expect(activateMock).toHaveBeenCalledWith('acme.bill-audit:quota-check');
  });

  it('closes a tab via its close button', () => {
    tabsState.tabs = [makeTab()];

    render(<WorkspaceTabBar />);
    fireEvent.click(screen.getByTestId('workspace-tab-close'));

    expect(closeMock).toHaveBeenCalledWith('acme.bill-audit:quota-check');
  });
});
