import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, act, waitFor } from '@testing-library/react';
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

const confirmMocks = vi.hoisted(() => ({
  retry: vi.fn().mockResolvedValue(true),
  dangerous: vi.fn().mockResolvedValue(true),
  hookCall: 0,
}));

const settingsState = vi.hoisted(() => ({
  safeMode: false,
}));

const executeQuery = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (
    sel: (s: { settings: { safeMode: boolean; autoCommit: boolean } }) => unknown,
  ) => sel({ settings: { safeMode: settingsState.safeMode, autoCommit: true } }),
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
vi.mock('../../../components/query/QueryContextSelectors', () => ({
  QueryContextSelectors: () => null,
}));
vi.mock('../../../components/query/QueryErrorPanel', () => ({
  QueryErrorPanel: () => null,
}));
vi.mock('../dashboard/AddToDashboardDialog', () => ({ AddToDashboardDialog: () => null }));
vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: { diagnosis: null; isDiagnosing: boolean; diagnosisError: null; isConfigured: boolean; diagnoseError: ReturnType<typeof vi.fn>; clearDiagnosis: ReturnType<typeof vi.fn> }) => unknown) =>
    sel({
      diagnosis: null,
      isDiagnosing: false,
      diagnosisError: null,
      isConfigured: true,
      diagnoseError: vi.fn(),
      clearDiagnosis: vi.fn(),
    }),
}));
vi.mock('../../../components/ai/DiagnosisPanel', () => ({ DiagnosisPanel: () => null }));
vi.mock('../../../components/query/BindParamPanel', () => ({ BindParamPanel: () => null }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => {
    const n = confirmMocks.hookCall++ % 2;
    return n === 0 ? [confirmMocks.retry, null] : [confirmMocks.dangerous, null];
  },
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    sessionTransactionStatus: vi.fn().mockResolvedValue(false),
    beginSessionTransaction: vi.fn().mockResolvedValue(undefined),
    commitSessionTransaction: vi.fn().mockResolvedValue(undefined),
    rollbackSessionTransaction: vi.fn().mockResolvedValue(undefined),
  },
}));

const PANEL_ID = 'panel-dangerous-sql';

afterEach(cleanup);

describe('[tester] QueryPanel dangerous SQL confirmation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    confirmMocks.hookCall = 0;
    confirmMocks.retry.mockResolvedValue(true);
    confirmMocks.dangerous.mockResolvedValue(true);
    settingsState.safeMode = false;
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
        [PANEL_ID, { ...EMPTY_QUERY_EXEC, sql: 'SELECT 1', running: false }],
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

  function setSql(sql: string) {
    usePanelStore.setState((s) => ({
      queryExec: new Map(s.queryExec).set(PANEL_ID, {
        ...s.queryExec.get(PANEL_ID)!,
        sql,
        running: false,
      }),
    }));
  }

  function renderPanel() {
    return render(
      <QueryPanel
        panelId={PANEL_ID}
        dbSessionId="sess-conn-1"
        connectionId="cfg-1"
        databaseType="postgresql"
      />,
    );
  }

  it('prompts before executing DROP when Safe Mode is off', async () => {
    setSql('DROP TABLE t');
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));

    await waitFor(() => expect(confirmMocks.dangerous).toHaveBeenCalledTimes(1));
    expect(confirmMocks.dangerous).toHaveBeenCalledWith(
      expect.objectContaining({
        title: 'query.dangerousSqlTitle',
        message: 'query.dangerousSqlConfirm',
        confirmLabel: 'query.execute',
        kind: 'warning',
      }),
    );
  });

  it('does not execute when dangerous SQL confirmation is cancelled', async () => {
    confirmMocks.dangerous.mockResolvedValueOnce(false);
    setSql('DROP TABLE t');
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));

    await waitFor(() => expect(confirmMocks.dangerous).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(executeQuery).not.toHaveBeenCalled());
  });

  it('executes when dangerous SQL confirmation is accepted', async () => {
    confirmMocks.dangerous.mockResolvedValueOnce(true);
    setSql('TRUNCATE TABLE t');
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));

    await waitFor(() => expect(confirmMocks.dangerous).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(executeQuery).toHaveBeenCalledTimes(1));
  });

  it('skips dangerous confirmation for safe SELECT statements', async () => {
    setSql('SELECT 1');
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));

    await waitFor(() => expect(executeQuery).toHaveBeenCalledTimes(1));
    expect(confirmMocks.dangerous).not.toHaveBeenCalled();
  });

  it('skips dangerous confirmation when Safe Mode is on', async () => {
    settingsState.safeMode = true;
    setSql('DROP TABLE t');
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));

    await waitFor(() => expect(executeQuery).toHaveBeenCalledTimes(1));
    expect(confirmMocks.dangerous).not.toHaveBeenCalled();
  });

  it('prompts on unclosed-transaction confirm path for DROP', async () => {
    setSql('BEGIN; DROP TABLE t');
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'query.txUnclosedConfirm' })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole('button', { name: 'query.txUnclosedConfirm' }));

    await waitFor(() => expect(confirmMocks.dangerous).toHaveBeenCalledTimes(1));
  });

  it('does not execute on unclosed-transaction path when dangerous confirm is cancelled', async () => {
    confirmMocks.dangerous.mockResolvedValueOnce(false);
    setSql('BEGIN; DROP TABLE t');
    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'query.txUnclosedConfirm' })).toBeInTheDocument(),
    );
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'query.txUnclosedConfirm' }));
    });

    await waitFor(() => expect(confirmMocks.dangerous).toHaveBeenCalledTimes(1));
    expect(executeQuery).not.toHaveBeenCalled();
  });
});
