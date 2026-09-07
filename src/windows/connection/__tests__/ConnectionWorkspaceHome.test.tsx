import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ConnectionWorkspaceHome } from '../ConnectionWorkspaceHome';
import type { ConnectionContext, Panel } from '../../../stores/panelStore';
import { useConnectionStore } from '../../../stores/connectionStore';
import { useActiveConnectionStore } from '../../../stores/activeConnectionStore';
import { usePanelStore } from '../../../stores/panelStore';
import { queryCommands } from '../../../commands/query';
import { settingsCommands } from '../../../commands/settings';
import { clearCachedAppExecutablePathForTest } from '../../../lib/mcpAgentConfig';
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

const { openBackupWindowMock } = vi.hoisted(() => ({
  openBackupWindowMock: vi.fn(),
}));

vi.mock('../../../lib/windowManager', () => ({
  openBackupWindow: (...args: unknown[]) => openBackupWindowMock(...args),
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
    clearCachedAppExecutablePathForTest();
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
    expect(screen.getByText('connWin.home.metrics.pinned')).toBeInTheDocument();
    expect(screen.getByText('connWin.home.metrics.dbTypes')).toBeInTheDocument();

    // Quick Start section
    expect(screen.getByText('connWin.home.quickStart')).toBeInTheDocument();

    // Common operations
    expect(screen.getByText('connWin.home.commonOps')).toBeInTheDocument();
    expect(screen.getByTestId('empty-new-connection-button')).toBeInTheDocument();
    expect(screen.getByTestId('empty-new-query-button')).toBeInTheDocument();
    expect(screen.getByTestId('empty-backup-button')).toBeInTheDocument();
    expect(screen.getByTestId('empty-restore-button')).toBeInTheDocument();
    expect(screen.queryByTestId('empty-history-button')).not.toBeInTheDocument();

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

    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('handles clicking common ops history button by opening global query history dialog', () => {
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

    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('triggers openBackupWindow for backup and restore buttons', () => {
    openBackupWindowMock.mockClear();
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

    const backupBtn = screen.getByTestId('empty-backup-button');
    fireEvent.click(backupBtn);
    expect(openBackupWindowMock).toHaveBeenCalledWith('backup');

    const restoreBtn = screen.getByTestId('empty-restore-button');
    fireEvent.click(restoreBtn);
    expect(openBackupWindowMock).toHaveBeenCalledWith('restore');
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

  it('displays and copies MCP launch command with full executable path', async () => {
    vi.spyOn(settingsCommands, 'getAppExecutablePath').mockResolvedValue(
      '/Applications/DataZen.app/Contents/MacOS/datazen',
    );
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

    await waitFor(() => {
      expect(
        screen.getByText('/Applications/DataZen.app/Contents/MacOS/datazen --mcp'),
      ).toBeInTheDocument();
    });

    const copyBtn = screen.getByText('connWin.home.aiIntegration.copy');
    fireEvent.click(copyBtn);
    expect(writeTextSpy).toHaveBeenCalledWith(
      '/Applications/DataZen.app/Contents/MacOS/datazen --mcp',
    );
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

  it('renders quick start, common ops, query history, and AI assistant cards with flex-1 and full height structure', () => {
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

    // Quick Start section header & card container
    const quickStartTitle = screen.getByText('connWin.home.quickStart');
    const quickStartCol = quickStartTitle.closest('.lg\\:col-span-2');
    expect(quickStartCol).toHaveClass('h-full', 'flex', 'flex-col');
    const quickStartCard = quickStartCol?.querySelector('.rounded-xl');
    expect(quickStartCard).toHaveClass('flex-1', 'flex', 'flex-col');

    // Quick Start connection buttons should have flex-1 to distribute height
    const pgConnBtn = screen.getByText('PostgreSQL-Local').closest('button');
    expect(pgConnBtn).toHaveClass('flex-1');

    // Common Ops header & card container
    const commonOpsTitle = screen.getByText('connWin.home.commonOps');
    const commonOpsCol = commonOpsTitle.closest('.flex.flex-col');
    expect(commonOpsCol).toHaveClass('h-full');
    const commonOpsCard = commonOpsCol?.querySelector('.rounded-xl');
    expect(commonOpsCard).toHaveClass('flex-1', 'flex', 'flex-col', 'justify-between');

    // Query History card
    const historyHeading = screen
      .getAllByText('connWin.home.recentQueries')
      .find((el) => el.tagName === 'H3')!;
    const historyCol = historyHeading.closest('.flex.flex-col');
    expect(historyCol).toHaveClass('h-full');
    const historyCard = historyCol?.querySelector('.rounded-xl');
    expect(historyCard).toHaveClass('flex-1', 'flex', 'flex-col');

    // AI Assistant card
    const aiTitle = screen.getByText('connWin.home.aiIntegration.title');
    const aiCol = aiTitle.closest('.flex.flex-col');
    expect(aiCol).toHaveClass('h-full');
    const aiCard = aiCol?.querySelector('.rounded-xl');
    expect(aiCard).toHaveClass('flex-1', 'flex', 'flex-col', 'justify-between');
  });

  it('calls onSelectHistoryQuery when a recent query item is clicked', async () => {
    const onSelectHistoryQuery = vi.fn();
    const mockHistoryItem = {
      id: 'hist-1',
      connectionId: 'conn-1',
      database: 'app_db',
      sql: 'SELECT * FROM test_table',
      executedAt: '2026-09-07T12:00:00Z',
      executionTimeMs: 15,
      success: true,
    };
    vi.mocked(queryCommands.getQueryHistory).mockResolvedValueOnce([mockHistoryItem]);

    render(
      <ConnectionWorkspaceHome
        hasConnections
        connectionContext={{
          connectionId: 'conn-1',
          dbSessionId: 'session-1',
          connectionName: 'Conn 1',
          databaseType: 'postgresql',
        }}
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
        onSelectHistoryQuery={onSelectHistoryQuery}
      />,
    );

    const sqlText = await screen.findByText('SELECT * FROM test_table');
    const card = sqlText.closest('[role="button"]');
    expect(card).not.toBeNull();
    fireEvent.click(card!);

    expect(onSelectHistoryQuery).toHaveBeenCalledWith(mockHistoryItem);
  });
});
