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
import type { ConnectionConfig, DatabaseType } from '../../../types';

afterEach(cleanup);

vi.mock('../../../hooks/useI18n', () => ({
  // Key-style mock: returns the key itself, appending params so dynamic
  // placeholders (e.g. noMatch "{query}") stay assertable in tests.
  useI18n: () => ({
    t: (key: string, params?: Record<string, string | number>) => {
      if (!params) return key;
      const suffix = Object.values(params).map(String).join(' ');
      return suffix ? `${key} ${suffix}` : key;
    },
  }),
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
    // Every history reader goes through the paged command, which returns
    // `{ entries, total }` rather than a bare array.
    getQueryHistoryPage: vi.fn().mockResolvedValue({ entries: [], total: 0 }),
  },
}));

const baseContext: ConnectionContext = {
  connectionId: 'cfg-1',
  dbSessionId: 'conn-1',
  connectionName: 'Local PG',
  databaseType: 'postgresql',
};

/**
 * `DatabaseType` is generated from the *selected* driver set
 * (`scripts/resolve-drivers.mjs`), so an optional driver id such as `mongodb`
 * only appears in the union when that driver was compiled in. These fixtures
 * are plain navigation rows, so the id is widened deliberately rather than
 * pretending the union is wider than the build.
 */
function optionalDriverType(id: string): DatabaseType {
  return id as DatabaseType;
}

const sampleConnections: ConnectionConfig[] = [
  {
    id: 'conn-1',
    name: 'PostgreSQL-Local',
    databaseType: 'postgresql',
    host: 'localhost',
    port: 5432,
    database: 'postgres',
    sslMode: 'prefer',
    group: 'Development',
    pinned: true,
  },
  {
    id: 'conn-2',
    name: 'MySQL-Prod',
    databaseType: 'mysql',
    host: '127.0.0.1',
    port: 3306,
    database: 'app',
    sslMode: 'prefer',
    group: 'Production',
  },
  {
    id: 'conn-3',
    name: 'Redis-Cache',
    databaseType: 'redis',
    host: '127.0.0.1',
    port: 6379,
    sslMode: 'prefer',
  },
  {
    id: 'conn-4',
    name: 'SQLite-Dev',
    databaseType: 'sqlite',
    database: '/path/to/dev.db',
    sslMode: 'prefer',
  },
  {
    id: 'conn-5',
    name: 'MongoDB-Cluster',
    databaseType: optionalDriverType('mongodb'),
    host: '127.0.0.1',
    port: 27017,
    sslMode: 'prefer',
  },
];

/** Extra connections to push the list past the collapsed page size (6). */
const extraConnections: ConnectionConfig[] = [
  {
    id: 'conn-6',
    name: 'MariaDB-Analytics',
    databaseType: 'mariadb',
    host: '10.0.0.8',
    port: 3306,
    sslMode: 'prefer',
  },
  {
    id: 'conn-7',
    name: 'ClickHouse-Metrics',
    databaseType: optionalDriverType('clickhouse'),
    host: '10.0.0.9',
    port: 8123,
    sslMode: 'prefer',
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

  it('renders a dynamic hero summary instead of metric cards', () => {
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

    expect(screen.getByText('connWin.home.selectConnectionTitle')).toBeInTheDocument();

    const subtitle = screen.getByTestId('home-hero-subtitle');
    // Dynamic counts: 5 connections, 2 distinct groups, 5 distinct db types.
    expect(subtitle.textContent).toContain('5');
    expect(subtitle.textContent).toContain('connWin.home.hero.subtitleConnections');
    expect(subtitle.textContent).toContain('2');
    expect(subtitle.textContent).toContain('connWin.home.hero.subtitleGroups');
    expect(subtitle.textContent).toContain('connWin.home.hero.subtitleTypes');

    // Vanity metric cards are gone.
    expect(screen.queryByText('connWin.home.metrics.connections')).not.toBeInTheDocument();
    expect(screen.queryByText('connWin.home.metrics.pinned')).not.toBeInTheDocument();
    expect(screen.queryByText('connWin.home.metrics.dbTypes')).not.toBeInTheDocument();
  });

  it('renders the redesigned landing page sections without removed entries', () => {
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
        onImportConnections={vi.fn()}
        onNewQuery={vi.fn()}
        onCreateTable={vi.fn()}
        onOpenErDiagram={vi.fn()}
        onOpenObjects={vi.fn()}
        onOpenPanel={vi.fn()}
      />,
    );

    // Your connections section with preserved CTA testids
    expect(screen.getByText('connWin.home.yourConnections')).toBeInTheDocument();
    expect(screen.getByTestId('home-connections-filter')).toBeInTheDocument();
    expect(screen.getByTestId('empty-new-connection-button')).toBeInTheDocument();
    expect(screen.getByTestId('empty-import-connections-button')).toBeInTheDocument();

    // Backup / Restore are removed from the landing page
    expect(screen.queryByTestId('empty-backup-button')).not.toBeInTheDocument();
    expect(screen.queryByTestId('empty-restore-button')).not.toBeInTheDocument();

    // Per-connection card testids
    expect(screen.getByTestId('home-conn-card-conn-1')).toBeInTheDocument();

    // MCP promo bar (compressed single row)
    expect(screen.getByTestId('home-mcp-promo-bar')).toBeInTheDocument();
    expect(screen.getByText('datazen --mcp')).toBeInTheDocument();

    // Shortcut footer driven by keymap
    expect(screen.getByTestId('home-shortcut-footer')).toBeInTheDocument();
  });

  it('filters connections by keyword and shows an empty state when nothing matches', () => {
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

    const filterInput = screen.getByTestId('home-connections-filter');
    fireEvent.change(filterInput, { target: { value: 'mysql' } });

    expect(screen.getByTestId('home-conn-card-conn-2')).toBeInTheDocument();
    expect(screen.queryByTestId('home-conn-card-conn-1')).not.toBeInTheDocument();

    fireEvent.change(filterInput, { target: { value: 'zzz-no-match' } });
    expect(screen.getByTestId('home-connections-no-match').textContent).toContain('zzz-no-match');
    expect(screen.queryByTestId('home-conn-card-conn-2')).not.toBeInTheDocument();
    // Clear the filter via the clear button
    fireEvent.click(screen.getByTestId('home-connections-filter-clear'));
    expect(screen.getByTestId('home-conn-card-conn-1')).toBeInTheDocument();
  });

  it('shows six connections by default and expands/collapses with Show all', () => {
    useConnectionStore.setState({ connections: [...sampleConnections, ...extraConnections] });

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

    // 6 visible by default (pinned first, then lastConnectedAt, then name).
    // Sorted tail: … conn-3 Redis-Cache, conn-4 SQLite-Dev — conn-4 is 7th.
    expect(screen.getByTestId('home-conn-card-conn-1')).toBeInTheDocument();
    expect(screen.queryByTestId('home-conn-card-conn-4')).not.toBeInTheDocument();
    expect(
      screen.getByText('connWin.home.connections.showAll', { exact: false }),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('home-connections-show-all'));
    expect(screen.getByTestId('home-conn-card-conn-4')).toBeInTheDocument();
    expect(screen.getByText('connWin.home.connections.showLess')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('home-connections-show-all'));
    expect(screen.queryByTestId('home-conn-card-conn-4')).not.toBeInTheDocument();
  });

  it('shows Open button for connected connections and Connect for offline ones', () => {
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

    // Connected → secondary "Open" affordance
    const openBtn = screen.getByTestId('home-conn-connect-conn-1');
    expect(openBtn.textContent).toContain('connWin.home.connections.open');
    fireEvent.click(openBtn);
    expect(onSelectConnection).toHaveBeenCalledWith('conn-1');

    // Offline → primary "Connect" affordance
    const connectBtn = screen.getByTestId('home-conn-connect-conn-2');
    expect(connectBtn.textContent).toContain('connWin.home.connections.connect');
  });

  it('triggers onSelectConnection when clicking a connection card row', () => {
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

    fireEvent.click(screen.getByTestId('home-conn-card-conn-1'));
    expect(onSelectConnection).toHaveBeenCalledWith('conn-1');
  });

  it('renders host:port and group metadata in the connection card', () => {
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

    const card = screen.getByTestId('home-conn-card-conn-1');
    expect(card.textContent).toContain('localhost:5432');
    expect(card.textContent).toContain('Development');
  });

  it('handles clicking view all history by opening the global query history dialog', () => {
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

    const viewAllBtn = screen.getByTestId('view-all-history-button');
    fireEvent.click(viewAllBtn);

    expect(screen.getByRole('dialog')).toBeInTheDocument();
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

    const copyBtn = screen.getByTestId('home-mcp-copy-button');
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

    const copyBtn = screen.getByTestId('home-mcp-copy-button');
    fireEvent.click(copyBtn);
    expect(writeTextSpy).toHaveBeenCalledWith(
      '/Applications/DataZen.app/Contents/MacOS/datazen --mcp',
    );
  });

  it('displays recent queries with source connection name, relative time and rerun action', async () => {
    useConnectionStore.setState({ connections: sampleConnections });
    const onSelectHistoryQuery = vi.fn();

    vi.mocked(queryCommands.getQueryHistoryPage).mockResolvedValueOnce({
      entries: [
        {
          id: 'hist-1',
          connectionId: 'conn-1',
          database: 'postgres',
          sql: 'SELECT * FROM users;',
          executedAt: new Date().toISOString(),
          executionTimeMs: 15,
          success: true,
        },
      {
        id: 'hist-2',
        connectionId: 'conn-1',
        database: 'postgres',
        sql: 'UPDATE users SET id = id;',
        executedAt: new Date(Date.now() - 2 * 60_000).toISOString(),
        executionTimeMs: 8,
        success: true,
      },
      ],
      total: 2,
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
        onSelectHistoryQuery={onSelectHistoryQuery}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText('SELECT * FROM users;')).toBeInTheDocument();
    });

    // Source connection name resolved from savedConnections
    expect(screen.getByTestId('home-query-source-hist-1').textContent).toBe('PostgreSQL-Local');
    // Relative time for a just-executed query (key-style t mock renders the key)
    expect(screen.getByText('connWin.home.queries.justNow')).toBeInTheDocument();
    // Minute bucket composes the localized label with the count param
    expect(screen.getByText('connWin.home.queries.minutesAgo', { exact: false }).textContent).toBe(
      'connWin.home.queries.minutesAgo 2',
    );

    // Hover action: Re-run delegates to onSelectHistoryQuery
    fireEvent.click(screen.getByTestId('home-query-rerun-hist-1'));
    expect(onSelectHistoryQuery).toHaveBeenCalledTimes(1);
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
    // `label` / `queryTabId` are pre-rename leftovers: a query panel is keyed by
    // `title`, and this case clicks the row through the generic "query"
    // affordance the icon renders, so the title-less shape is kept as-is.
    const recentPanel = {
      id: 'panel-1',
      type: 'query' as const,
      connectionId: 'cfg-1',
      dbSessionId: 'conn-1',
      connectionName: 'Local PG',
      databaseType: 'postgresql' as const,
      database: '',
      schema: null,
    };
    const recentPanels = [recentPanel as unknown as Panel];
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
    vi.mocked(queryCommands.getQueryHistoryPage).mockResolvedValueOnce({
      entries: [mockHistoryItem],
      total: 1,
    });

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
