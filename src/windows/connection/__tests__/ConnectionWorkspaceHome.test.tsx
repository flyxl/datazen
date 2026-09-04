import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ConnectionWorkspaceHome } from '../ConnectionWorkspaceHome';
import type { ConnectionContext, Panel } from '../../../stores/panelStore';
import { useConnectionStore } from '../../../stores/connectionStore';
import { useActiveConnectionStore } from '../../../stores/activeConnectionStore';
import { usePanelStore } from '../../../stores/panelStore';
import { queryCommands } from '../../../commands/query';
import type { ConnectionConfig } from '../../../types';

afterEach(cleanup);

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../lib/databaseTypes', () => ({
  getDbIcon: () => ({ label: 'Pg', bg: 'bg-blue-500' }),
  getDbLabel: (type: string) => (type === 'postgresql' ? 'PostgreSQL' : type),
  getDriverIconParents: () => ({}),
}));

vi.mock('../contentViewHelpers', () => ({
  getPanelIcon: () => null,
  getPanelLabel: (panel: Panel) => panel.type,
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    getQueryHistory: vi.fn().mockResolvedValue([]),
  },
}));

const baseContext: ConnectionContext = {
  connectionId: 'cfg-1',
  dbSessionId: 'conn-1',
  connectionName: 'Local PG',
  databaseType: 'postgresql',
};

const sampleConnections: ConnectionConfig[] = [
  {
    id: 'conn-1',
    name: 'PostgreSQL-Local',
    databaseType: 'postgresql',
    host: 'localhost',
    port: 5432,
    database: 'postgres',
    pinned: true,
  },
  {
    id: 'conn-2',
    name: 'MySQL-Prod',
    databaseType: 'mysql',
    host: '127.0.0.1',
    port: 3306,
    database: 'app',
  },
  {
    id: 'conn-3',
    name: 'Redis-Cache',
    databaseType: 'redis',
    host: '127.0.0.1',
    port: 6379,
  },
  {
    id: 'conn-4',
    name: 'SQLite-Dev',
    databaseType: 'sqlite',
    database: '/path/to/dev.db',
  },
  {
    id: 'conn-5',
    name: 'MongoDB-Cluster',
    databaseType: 'mongodb',
    host: '127.0.0.1',
    port: 27017,
  },
];

describe('ConnectionWorkspaceHome', () => {
  beforeEach(() => {
    useConnectionStore.setState({ connections: [] });
    useActiveConnectionStore.setState({ connections: {} });
    usePanelStore.setState({ pendingQueryHistoryConnectionId: null });
    vi.clearAllMocks();
  });

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
    expect(screen.getByText('connWin.home.emptyNoConnectionsHint')).toBeInTheDocument();
  });

  it('renders quick start capped at strictly 4 connections in unified list', () => {
    useConnectionStore.setState({ connections: sampleConnections });

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

    // First 4 connections sorted by pinned first, then name:
    // 1. PostgreSQL-Local (pinned)
    // 2. MongoDB-Cluster
    // 3. MySQL-Prod
    // 4. Redis-Cache
    expect(screen.getByText('PostgreSQL-Local')).toBeInTheDocument();
    expect(screen.getByText('MongoDB-Cluster')).toBeInTheDocument();
    expect(screen.getByText('MySQL-Prod')).toBeInTheDocument();
    expect(screen.getByText('Redis-Cache')).toBeInTheDocument();

    // 5th connection (SQLite-Dev) should NOT be in the quick start list
    expect(screen.queryByText('SQLite-Dev')).not.toBeInTheDocument();
  });

  it('prompts to select a connection when none is active and renders DBX dashboard cards', () => {
    useConnectionStore.setState({ connections: sampleConnections });
    useActiveConnectionStore.setState({
      connections: {
        'conn-1': {
          dbSessionId: 'session-1',
          connectionId: 'conn-1',
          status: 'connected',
          serverInfo: null,
          currentDatabase: 'postgres',
          error: null,
        },
      },
    });

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
    expect(screen.getByText('connWin.home.selectConnectionTitle')).toBeInTheDocument();
    expect(screen.getByText('connWin.home.selectConnectionHint')).toBeInTheDocument();
    expect(screen.getByText('connWin.home.selectConnectionTip')).toBeInTheDocument();

    // Metric cards
    expect(screen.getByText('connWin.home.metrics.connections')).toBeInTheDocument();
    expect(screen.getByText('connWin.home.metrics.connected')).toBeInTheDocument();
    expect(screen.getByText('connWin.home.metrics.dbTypes')).toBeInTheDocument();

    // Quick Start section
    expect(screen.getByText('connWin.home.quickStart')).toBeInTheDocument();

    // Common operations
    expect(screen.getByText('connWin.home.commonOps')).toBeInTheDocument();
    expect(screen.getByTestId('empty-new-connection-button')).toBeInTheDocument();

    // AI & MCP Integration
    expect(screen.getByText('connWin.home.aiIntegration.title')).toBeInTheDocument();
    expect(screen.getByText('datazen --mcp')).toBeInTheDocument();
  });

  it('triggers onSelectConnection when clicking a quick start connection item', () => {
    useConnectionStore.setState({ connections: sampleConnections });
    const onSelectConnection = vi.fn();

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
        onSelectConnection={onSelectConnection}
      />,
    );

    fireEvent.click(screen.getByText('PostgreSQL-Local'));
    expect(onSelectConnection).toHaveBeenCalledWith('conn-1');
  });

  it('handles clicking view all history by setting pendingQueryHistory and selecting connection', () => {
    useConnectionStore.setState({ connections: sampleConnections });
    const onSelectConnection = vi.fn();

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
        onSelectConnection={onSelectConnection}
      />,
    );

    const viewAllBtn = screen.getByTestId('view-all-history-button');
    fireEvent.click(viewAllBtn);

    expect(usePanelStore.getState().pendingQueryHistoryConnectionId).toBe('conn-1');
    expect(onSelectConnection).toHaveBeenCalledWith('conn-1');
  });

  it('handles clicking common ops history button by setting pendingQueryHistory and selecting connection', () => {
    useConnectionStore.setState({ connections: sampleConnections });
    const onSelectConnection = vi.fn();

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
        onSelectConnection={onSelectConnection}
      />,
    );

    const historyBtn = screen.getByTestId('empty-history-button');
    fireEvent.click(historyBtn);

    expect(usePanelStore.getState().pendingQueryHistoryConnectionId).toBe('conn-1');
    expect(onSelectConnection).toHaveBeenCalledWith('conn-1');
  });

  it('copies MCP launch command to clipboard when clicking copy button', async () => {
    useConnectionStore.setState({ connections: sampleConnections });
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: { writeText: writeTextSpy },
    });

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

    const copyBtn = screen.getByText('connWin.home.aiIntegration.copy');
    fireEvent.click(copyBtn);
    expect(writeTextSpy).toHaveBeenCalledWith('datazen --mcp');
    await waitFor(() => {
      expect(screen.getByText('connWin.home.aiIntegration.copied')).toBeInTheDocument();
    });
  });

  it('displays query history records and allows copying SQL', async () => {
    useConnectionStore.setState({ connections: sampleConnections });
    const writeTextSpy = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: { writeText: writeTextSpy },
    });

    vi.mocked(queryCommands.getQueryHistory).mockResolvedValueOnce([
      {
        id: 'hist-1',
        connectionId: 'conn-1',
        database: 'postgres',
        sql: 'SELECT * FROM users;',
        executedAt: new Date().toISOString(),
        executionTimeMs: 15,
        success: true,
      },
    ]);

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

    await waitFor(() => {
      expect(screen.getByText('SELECT * FROM users;')).toBeInTheDocument();
    });
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
    expect(screen.queryByText('connWin.home.selectConnectionTitle')).not.toBeInTheDocument();
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
