import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ConnectionWorkspaceHome } from '../ConnectionWorkspaceHome';
import type { ConnectionContext, Panel } from '../../../stores/panelStore';

afterEach(cleanup);

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/databaseTypes', () => ({
  getDbIcon: () => ({ label: 'Pg', bg: 'bg-blue-500' }),
  getDbLabel: () => 'PostgreSQL',
  getDriverIconParents: () => ({}),
}));

vi.mock('../contentViewHelpers', () => ({
  getPanelIcon: () => null,
  getPanelLabel: (panel: Panel) => panel.type,
}));

const baseContext: ConnectionContext = {
  connectionId: 'cfg-1',
  dbSessionId: 'conn-1',
  connectionName: 'Local PG',
  databaseType: 'postgresql',
};

describe('ConnectionWorkspaceHome', () => {
  it('shows empty-state CTA when there are no saved connections', () => {
    render(
      <ConnectionWorkspaceHome
        hasConnections={false}
        connectionContext={null}
        recentPanels={[]}
        showNewQuery={false}
        showNewTable={false}
        showErDiagram={false}
        showObjects={false}
        onNewConnection={vi.fn()}
        onNewQuery={vi.fn()}
        onCreateTable={vi.fn()}
        onOpenErDiagram={vi.fn()}
        onOpenObjects={vi.fn()}
        onOpenPanel={vi.fn()}
      />,
    );
    expect(screen.getByTestId('connection-workspace-home')).toBeInTheDocument();
    expect(screen.getByText('main.noConnections')).toBeInTheDocument();
    expect(screen.getByText('main.createFirst')).toBeInTheDocument();
  });

  it('prompts to select a connection when none is active', () => {
    render(
      <ConnectionWorkspaceHome
        hasConnections
        connectionContext={null}
        recentPanels={[]}
        showNewQuery={false}
        showNewTable={false}
        showErDiagram={false}
        showObjects={false}
        onNewConnection={vi.fn()}
        onNewQuery={vi.fn()}
        onCreateTable={vi.fn()}
        onOpenErDiagram={vi.fn()}
        onOpenObjects={vi.fn()}
        onOpenPanel={vi.fn()}
      />,
    );
    expect(screen.getByText('connWin.home.selectConnection')).toBeInTheDocument();
  });

  it('shows loading spinner when connecting and does not show select prompt', () => {
    render(
      <ConnectionWorkspaceHome
        hasConnections
        connectionContext={null}
        recentPanels={[]}
        showNewQuery={false}
        showNewTable={false}
        showErDiagram={false}
        showObjects={false}
        isConnecting
        connectingName="Local PG"
        connectingDbType="postgresql"
        onNewConnection={vi.fn()}
        onNewQuery={vi.fn()}
        onCreateTable={vi.fn()}
        onOpenErDiagram={vi.fn()}
        onOpenObjects={vi.fn()}
        onOpenPanel={vi.fn()}
      />,
    );
    expect(screen.queryByText('connWin.home.selectConnection')).not.toBeInTheDocument();
    expect(screen.getByText('Local PG')).toBeInTheDocument();
    expect(screen.getByTestId('connection-workspace-home')).toBeInTheDocument();
  });

  it('renders quick actions for an active connection', () => {
    const onNewQuery = vi.fn();
    render(
      <ConnectionWorkspaceHome
        hasConnections
        connectionContext={baseContext}
        recentPanels={[]}
        showNewQuery
        showNewTable={false}
        showErDiagram={false}
        showObjects={false}
        onNewConnection={vi.fn()}
        onNewQuery={onNewQuery}
        onCreateTable={vi.fn()}
        onOpenErDiagram={vi.fn()}
        onOpenObjects={vi.fn()}
        onOpenPanel={vi.fn()}
      />,
    );
    expect(screen.getByText('Local PG')).toBeInTheDocument();
    fireEvent.click(screen.getByText('common.newQuery'));
    expect(onNewQuery).toHaveBeenCalledOnce();
  });

  it('lists recent panels and opens them on click', () => {
    const onOpenPanel = vi.fn();
    const recentPanels = [
      {
        id: 'panel-1',
        type: 'query' as const,
        connectionId: 'cfg-1',
        dbSessionId: 'conn-1',
        connectionName: 'Local PG',
        databaseType: 'postgresql' as const,
        label: 'Query 1',
        queryTabId: 'qt-1',
      },
    ];
    render(
      <ConnectionWorkspaceHome
        hasConnections
        connectionContext={baseContext}
        recentPanels={recentPanels}
        showNewQuery
        showNewTable={false}
        showErDiagram={false}
        showObjects={false}
        onNewConnection={vi.fn()}
        onNewQuery={vi.fn()}
        onCreateTable={vi.fn()}
        onOpenErDiagram={vi.fn()}
        onOpenObjects={vi.fn()}
        onOpenPanel={onOpenPanel}
      />,
    );
    fireEvent.click(screen.getByText('query'));
    expect(onOpenPanel).toHaveBeenCalledWith('panel-1');
  });
});
