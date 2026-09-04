import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, waitFor } from '@testing-library/react';
import { QueryPanel } from '../QueryPanel';
import { usePanelStore, type QueryPanel as QueryPanelState } from '../../../stores/panelStore';
import { EMPTY_QUERY_EXEC } from '../../../stores/queryExecActions';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 280, handleRef: { current: null } }),
}));

vi.mock('../../../hooks/useCompactToolbar', () => ({
  estimateExpandedToolbarWidth: ({
    expandedButtonCount,
    fixedExtraWidth = 0,
  }: {
    expandedButtonCount: number;
    fixedExtraWidth?: number;
  }) => 32 + expandedButtonCount * 96 + Math.max(0, expandedButtonCount - 1) * 8 + fixedExtraWidth,
  TOOLBAR_GAP: 8,
  TOOLBAR_HORIZONTAL_PADDING: 32,
  useCompactToolbar: () => ({ ref: { current: null }, compact: false }),
}));

const panelConnectionState = vi.hoisted(() => ({
  connectionId: 'cfg-1',
  dbSessionId: 'sess-conn-1',
  status: 'connected' as const,
  serverInfo: { serverVersion: '16', serverType: 'PostgreSQL' },
  capabilities: {
    supportsCancelQuery: true,
    supportsQueryExecutionCancel: true,
    supportsExplain: true,
    supportsStreamingResults: true,
  },
  currentDatabase: 'app' as string | null,
  error: null as string | null,
}));

const activeConnectionStoreState = vi.hoisted(() => ({
  connections: {} as Record<string, typeof panelConnectionState>,
}));

const schemaStoreState = vi.hoisted(() => ({
  schemas: new Map(),
  tables: [] as Array<{ name: string; tableType: 'table' | 'view'; schema?: string }>,
  views: [] as Array<{ name: string; tableType: 'table' | 'view'; schema?: string }>,
  columnMap: {} as Record<string, string[]>,
  namespaceTree: [],
  pathAliases: {},
  databases: [] as string[],
  currentDatabase: 'app' as string | null,
  currentSchema: null as string | null,
  isMultiDatabase: false,
  ensuringCount: 0,
  ensureColumns: vi.fn(),
  loadTables: vi.fn(),
  ensureNamespacePath: vi.fn(),
}));

const executeQuery = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const createWidgetFromSql = vi.hoisted(() =>
  vi.fn().mockRejectedValue(new Error('widget creation failed')),
);

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (
    sel: (s: { settings: { safeMode: boolean; autoCommit: boolean } }) => unknown,
  ) => sel({ settings: { safeMode: false, autoCommit: true } }),
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (selector: (state: typeof activeConnectionStoreState) => unknown) =>
      selector(activeConnectionStoreState),
    { getState: () => activeConnectionStoreState },
  ),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: Object.assign(
    (sel: (s: typeof schemaStoreState) => unknown) => sel(schemaStoreState),
    { getState: () => schemaStoreState },
  ),
}));

vi.mock('../../../components/SqlEditor', async () => {
  const { forwardRef } = await import('react');
  return {
    SqlEditor: forwardRef((_props: unknown, _ref: unknown) => (
      <div data-testid="mock-sql-editor" />
    )),
  };
});

vi.mock('../../../components/DataTable/DataTable', () => ({ DataTable: () => null }));
vi.mock('../../../components/chart/ChartView', () => ({ ChartView: () => null }));
vi.mock('../../../components/ai/Nl2SqlPanel', () => ({ Nl2SqlPanel: () => null }));
vi.mock('../../../components/ai/ExplainPanel', () => ({ ExplainPanel: () => null }));
vi.mock('../../../components/ai/DiagnosisPanel', () => ({ DiagnosisPanel: () => null }));
vi.mock('../../../components/query/QueryContextSelectors', () => ({
  QueryContextSelectors: () => null,
}));
vi.mock('../../../components/query/QueryErrorPanel', () => ({ QueryErrorPanel: () => null }));
vi.mock('../../../components/query/BindParamPanel', () => ({ BindParamPanel: () => null }));
vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: { diagnosis: null; isDiagnosing: boolean; diagnosisError: null; isConfigured: boolean }) => unknown) =>
    sel({
      diagnosis: null,
      isDiagnosing: false,
      diagnosisError: null,
      isConfigured: true,
    }),
}));
vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(true), null],
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../../commands/dashboard', () => ({
  dashboardCommands: {
    createWidgetFromSql: (...args: unknown[]) => createWidgetFromSql(...args),
    saveDashboard: vi.fn(),
    listDashboards: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  openDashboardWindow: vi.fn(),
}));

vi.mock('../../dashboard/AddToDashboardDialog', () => ({
  AddToDashboardDialog: ({
    open,
    onConfirm,
  }: {
    open: boolean;
    onConfirm: (dashboardId: string) => void;
  }) =>
    open ? (
      <button
        type="button"
        data-testid="confirm-add-dashboard"
        onClick={() => onConfirm('dash-existing')}
      >
        confirm
      </button>
    ) : null,
}));

const PANEL_ID = 'panel-result-message';

afterEach(cleanup);

describe('[tester] QueryPanel ResultMessageDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    executeQuery.mockResolvedValue(undefined);
    createWidgetFromSql.mockRejectedValue(new Error('widget creation failed'));
    schemaStoreState.tables = [];
    schemaStoreState.views = [];
    schemaStoreState.columnMap = {};
    schemaStoreState.currentDatabase = 'app';
    activeConnectionStoreState.connections = { 'cfg-1': panelConnectionState };
    usePanelStore.setState({
      panels: [
        {
          id: PANEL_ID,
          type: 'query',
          connectionId: 'cfg-1',
          dbSessionId: 'sess-conn-1',
          connectionName: 'Test connection',
          databaseType: 'postgresql',
          title: 'Test query',
        } satisfies QueryPanelState,
      ],
      queryExec: new Map([
        [
          PANEL_ID,
          {
            ...EMPTY_QUERY_EXEC,
            sql: 'SELECT 1',
            running: false,
            results: [
              {
                sql: 'SELECT 1',
                columns: [{ name: 'c', dataType: 'integer' }],
                rows: [[1]],
                executionTimeMs: 1,
              },
            ],
          },
        ],
      ]),
      historyVisible: false,
      favoritesVisible: false,
      queryHistory: [],
      queryFavorites: [],
      loadHistory: vi.fn().mockResolvedValue(undefined),
      loadFavorites: vi.fn().mockResolvedValue(undefined),
      cancelQuery: vi.fn(),
      executeQuery,
    } as Partial<ReturnType<typeof usePanelStore.getState>>);
  });

  function renderPanel(databaseType = 'postgresql') {
    return render(
      <QueryPanel
        panelId={PANEL_ID}
        dbSessionId="sess-conn-1"
        connectionId="cfg-1"
        databaseType={databaseType}
      />,
    );
  }

  it('shows ResultMessageDialog for suspicious Postgres double-quoted literals instead of alert', async () => {
    usePanelStore.setState((s) => ({
      queryExec: new Map(s.queryExec).set(PANEL_ID, {
        ...s.queryExec.get(PANEL_ID)!,
        sql: 'SELECT * FROM users WHERE name = "John"',
      }),
    }));
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));

    await waitFor(() =>
      expect(screen.getByText('query.postgresDoubleQuoteHint')).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'common.ok' })).toBeInTheDocument();
    expect(executeQuery).not.toHaveBeenCalled();
  });

  it('shows ResultMessageDialog when add-to-dashboard widget creation fails', async () => {
    renderPanel();
    fireEvent.click(screen.getByTestId('query-add-to-dashboard'));
    fireEvent.click(screen.getByTestId('confirm-add-dashboard'));

    await waitFor(() => expect(createWidgetFromSql).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByText('widget creation failed')).toBeInTheDocument(),
    );
    expect(screen.getByRole('button', { name: 'common.ok' })).toBeInTheDocument();
  });
});
