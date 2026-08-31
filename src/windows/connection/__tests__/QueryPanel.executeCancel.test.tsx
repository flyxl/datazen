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
  } as {
    supportsCancelQuery: boolean;
    supportsQueryExecutionCancel: boolean;
    supportsExplain: boolean;
    supportsStreamingResults: boolean;
  } | undefined,
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

const retryConfirmation = vi.hoisted(() => ({
  confirm: vi.fn(),
}));

const executeQuery = vi.hoisted(() => vi.fn());

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

vi.mock('../../../components/SqlEditor', () => ({
  SqlEditor: () => <div data-testid="mock-sql-editor" />,
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: () => null,
}));

vi.mock('../../../components/chart/ChartView', () => ({
  ChartView: () => null,
}));

vi.mock('../../../components/ai/Nl2SqlPanel', () => ({
  Nl2SqlPanel: () => null,
}));

const aiState = vi.hoisted(() => ({
  diagnosis: null,
  isDiagnosing: false,
  diagnosisError: null,
  isConfigured: true,
  diagnoseError: vi.fn().mockResolvedValue(undefined),
  clearDiagnosis: vi.fn(),
}));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

vi.mock('../../../components/ai/DiagnosisPanel', async () => {
  return vi.importActual('../../../components/ai/DiagnosisPanel');
});

vi.mock('../../../components/ai/ExplainPanel', () => ({
  ExplainPanel: () => null,
}));

vi.mock('../../../components/query/BindParamPanel', () => ({
  BindParamPanel: ({
    params,
    onChange,
  }: {
    params: Array<{ name: string }>;
    onChange: (name: string, value: string) => void;
  }) => {
    const name = params[0]?.name;
    if (!name) return null;
    return (
      <>
        <button type="button" data-testid="test-set-param-one" onClick={() => onChange(name, '1')} />
        <button type="button" data-testid="test-set-param-two" onClick={() => onChange(name, '2')} />
      </>
    );
  },
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [retryConfirmation.confirm, null],
}));

vi.mock('../../../components/query/QueryContextSelectors', () => ({
  QueryContextSelectors: () => null,
}));

vi.mock('../../../components/query/QueryErrorPanel', () => ({
  QueryErrorPanel: ({
    onFixSql,
    onRetry,
  }: {
    onFixSql?: () => void;
    onRetry?: () => void;
  }) => (
    <>
      {onFixSql && (
        <button type="button" onClick={onFixSql}>
          test-fix-sql
        </button>
      )}
      {onRetry && (
        <button type="button" data-testid="query-retry" onClick={onRetry}>
          test-retry
        </button>
      )}
    </>
  ),
}));

vi.mock('../dashboard/AddToDashboardDialog', () => ({
  AddToDashboardDialog: () => null,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

const PANEL_ID = 'panel-test';

afterEach(cleanup);

describe('QueryPanel execute/cancel button', () => {
  const cancelQuery = vi.fn().mockResolvedValue(undefined);

  beforeEach(() => {
    vi.clearAllMocks();
    retryConfirmation.confirm.mockResolvedValue(true);
    executeQuery.mockResolvedValue(undefined);
    schemaStoreState.tables = [];
    schemaStoreState.views = [];
    schemaStoreState.columnMap = {};
    schemaStoreState.currentDatabase = 'app';
    schemaStoreState.currentSchema = null;
    schemaStoreState.schemas.clear();
    aiState.diagnosis = null;
    aiState.isDiagnosing = false;
    aiState.diagnosisError = null;
    aiState.isConfigured = true;
    vi.useFakeTimers();
    panelConnectionState.capabilities = {
      supportsCancelQuery: true,
      supportsQueryExecutionCancel: true,
      supportsExplain: true,
      supportsStreamingResults: true,
    };
    panelConnectionState.connectionId = 'cfg-1';
    panelConnectionState.dbSessionId = 'sess-conn-1';
    panelConnectionState.currentDatabase = 'app';
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
          },
        ],
      ]),
      historyVisible: false,
      favoritesVisible: false,
      queryHistory: [],
      queryFavorites: [],
      loadHistory: vi.fn().mockResolvedValue(undefined),
      loadFavorites: vi.fn().mockResolvedValue(undefined),
      cancelQuery,
      executeQuery,
    } as Partial<ReturnType<typeof usePanelStore.getState>>);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function setRunning(running: boolean) {
    usePanelStore.setState((s) => {
      const exec = s.queryExec.get(PANEL_ID)!;
      return {
        queryExec: new Map(s.queryExec).set(PANEL_ID, {
          ...exec,
          running,
          executionId: running ? 'exec-test' : null,
        }),
      };
    });
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

  function setFailedQuery(sql: string) {
    usePanelStore.setState({
      queryExec: new Map([
        [
          PANEL_ID,
          {
            ...EMPTY_QUERY_EXEC,
            sql,
            error: 'query failed',
            running: false,
          },
        ],
      ]),
    });
  }

  function mutatePanel(patch: Partial<QueryPanelState>) {
    usePanelStore.setState((state) => ({
      panels: state.panels.map((panel) =>
        panel.id === PANEL_ID ? ({ ...panel, ...patch } as QueryPanelState) : panel,
      ),
    }));
  }

  it('shows Execute while running for less than 300ms', () => {
    setRunning(true);
    renderPanel();
    expect(screen.getByRole('button', { name: 'query.execute' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'query.stop' })).toBeNull();
  });

  it('replaces Execute with Cancel after 300ms of running', () => {
    setRunning(true);
    renderPanel();
    act(() => {
      vi.advanceTimersByTime(299);
    });
    expect(screen.getByRole('button', { name: 'query.execute' })).toBeDisabled();
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.queryByRole('button', { name: 'query.execute' })).toBeNull();
    expect(screen.getByRole('button', { name: 'query.stop' })).toBeInTheDocument();
  });

  it('calls cancelQuery when Cancel is clicked', () => {
    setRunning(true);
    renderPanel();
    act(() => {
      vi.advanceTimersByTime(300);
    });
    fireEvent.click(screen.getByRole('button', { name: 'query.stop' }));
    expect(cancelQuery).toHaveBeenCalledWith(PANEL_ID);
  });

  it('does not offer cancellation before the execution id arrives', () => {
    setRunning(true);
    usePanelStore.setState((s) => {
      const exec = s.queryExec.get(PANEL_ID)!;
      return { queryExec: new Map(s.queryExec).set(PANEL_ID, { ...exec, executionId: null }) };
    });
    renderPanel();
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(screen.getByRole('button', { name: 'query.stop' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'query.stop' }));
    expect(cancelQuery).not.toHaveBeenCalled();
  });

  it('disables Cancel and explains unsupported capability', () => {
    panelConnectionState.capabilities = {
      supportsCancelQuery: false,
      supportsQueryExecutionCancel: false,
      supportsExplain: true,
      supportsStreamingResults: true,
    };
    setRunning(true);
    renderPanel();
    act(() => {
      vi.advanceTimersByTime(300);
    });

    expect(screen.getByRole('button', { name: 'query.stop' })).toBeDisabled();
    expect(screen.getAllByText('query.cancelUnavailable').length).toBeGreaterThan(0);
    expect(cancelQuery).not.toHaveBeenCalled();
  });

  it('disables Cancel and explains unknown capability', () => {
    panelConnectionState.capabilities = undefined;
    setRunning(true);
    renderPanel();
    act(() => {
      vi.advanceTimersByTime(300);
    });

    expect(screen.getByRole('button', { name: 'query.stop' })).toBeDisabled();
    expect(screen.getAllByText('query.cancelUnknown').length).toBeGreaterThan(0);
    expect(cancelQuery).not.toHaveBeenCalled();
  });

  it('resets to Execute when running stops before 300ms', () => {
    setRunning(true);
    const { rerender } = renderPanel();
    act(() => {
      vi.advanceTimersByTime(100);
    });
    setRunning(false);
    rerender(
      <QueryPanel
        panelId={PANEL_ID}
        dbSessionId="sess-conn-1"
        connectionId="cfg-1"
        databaseType="postgresql"
      />,
    );
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(screen.getByRole('button', { name: 'query.execute' })).not.toBeDisabled();
    expect(screen.queryByRole('button', { name: 'query.stop' })).toBeNull();
  });

  it('sends only the redacted diagnosis payload through QueryPanel', async () => {
    const secrets = ['query-json-secret', 'query-password-secret', 'error-json-secret'];
    vi.useRealTimers();
    usePanelStore.setState({
      queryExec: new Map([
        [
          PANEL_ID,
          {
            ...EMPTY_QUERY_EXEC,
            sql: `SELECT '{\\"token\\":\\"query-json-secret\\"}', password = 'query-password-secret'`,
            error: `query failed: {\\"token\\":\\"error-json-secret\\"}`,
            running: false,
          },
        ],
      ]),
    } as Partial<ReturnType<typeof usePanelStore.getState>>);

    renderPanel();
    fireEvent.click(screen.getByRole('button', { name: 'test-fix-sql' }));

    await waitFor(() => expect(aiState.diagnoseError).toHaveBeenCalledTimes(1));
    const payload = aiState.diagnoseError.mock.calls[0]?.[0];
    expect(payload).toMatchObject({ dbSessionId: 'sess-conn-1', database: 'app' });
    for (const secret of secrets) expect(JSON.stringify(payload)).not.toContain(secret);
  });

  it('executes Retry once after confirmation when schema context is unchanged', async () => {
    vi.useRealTimers();
    schemaStoreState.tables = [{ name: 'users', tableType: 'table', schema: 'public' }];
    schemaStoreState.views = [{ name: 'active_users', tableType: 'view', schema: 'public' }];
    schemaStoreState.columnMap = { users: ['id', 'email'] };
    setFailedQuery('SELECT * FROM users');

    renderPanel();
    fireEvent.click(screen.getByTestId('query-retry'));

    await waitFor(() => expect(retryConfirmation.confirm).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(executeQuery).toHaveBeenCalledTimes(1));
    expect(executeQuery).toHaveBeenCalledWith(PANEL_ID, undefined);
  });

  it('blocks confirmed Retry when the schema context changes', async () => {
    vi.useRealTimers();
    schemaStoreState.tables = [{ name: 'users', tableType: 'table', schema: 'public' }];
    schemaStoreState.columnMap = { users: ['id'] };
    setFailedQuery('SELECT * FROM users');
    let resolveConfirmation: ((value: boolean) => void) | undefined;
    retryConfirmation.confirm.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          resolveConfirmation = resolve;
        }),
    );

    renderPanel();
    fireEvent.click(screen.getByTestId('query-retry'));
    await waitFor(() => expect(retryConfirmation.confirm).toHaveBeenCalledTimes(1));
    schemaStoreState.tables = [{ name: 'orders', tableType: 'table', schema: 'public' }];
    schemaStoreState.columnMap = { orders: ['id'] };
    await act(async () => resolveConfirmation?.(true));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(executeQuery).not.toHaveBeenCalled();
  });

  it('blocks confirmed Retry when the SQL changes', async () => {
    vi.useRealTimers();
    setFailedQuery('SELECT 1');
    let resolveConfirmation: ((value: boolean) => void) | undefined;
    retryConfirmation.confirm.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          resolveConfirmation = resolve;
        }),
    );

    renderPanel();
    fireEvent.click(screen.getByTestId('query-retry'));
    await waitFor(() => expect(retryConfirmation.confirm).toHaveBeenCalledTimes(1));
    usePanelStore.setState((state) => {
      const exec = state.queryExec.get(PANEL_ID);
      if (!exec) return state;
      return {
        queryExec: new Map(state.queryExec).set(PANEL_ID, { ...exec, sql: 'SELECT 2' }),
      };
    });
    await act(async () => resolveConfirmation?.(true));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(executeQuery).not.toHaveBeenCalled();
  });

  it('blocks confirmed Retry when bound params change', async () => {
    vi.useRealTimers();
    setFailedQuery('SELECT :id');
    let resolveConfirmation: ((value: boolean) => void) | undefined;
    retryConfirmation.confirm.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          resolveConfirmation = resolve;
        }),
    );

    renderPanel();
    fireEvent.click(screen.getByTestId('test-set-param-one'));
    await waitFor(() => expect(screen.getByTestId('test-set-param-two')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('query-retry'));
    await waitFor(() => expect(retryConfirmation.confirm).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByTestId('test-set-param-two'));
    await act(async () => resolveConfirmation?.(true));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(executeQuery).not.toHaveBeenCalled();
  });

  it.each([
    {
      label: 'database changes',
      mutate: () => mutatePanel({ database: 'warehouse' }),
    },
    {
      label: 'schema changes',
      mutate: () => mutatePanel({ schema: 'analytics' }),
    },
    {
      label: 'session changes',
      mutate: () => mutatePanel({ dbSessionId: 'sess-conn-2' }),
    },
    {
      label: 'connection changes',
      mutate: () => {
        mutatePanel({ connectionId: 'cfg-2' });
        activeConnectionStoreState.connections['cfg-2'] = {
          ...panelConnectionState,
          connectionId: 'cfg-2',
        };
      },
    },
    {
      label: 'database type changes',
      mutate: () => mutatePanel({ databaseType: 'mysql' }),
    },
  ])('blocks confirmed Retry when $label during confirmation', async ({ mutate }) => {
    vi.useRealTimers();
    setFailedQuery('SELECT 1');
    let resolveConfirmation: ((value: boolean) => void) | undefined;
    retryConfirmation.confirm.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          resolveConfirmation = resolve;
        }),
    );

    renderPanel();
    fireEvent.click(screen.getByTestId('query-retry'));
    await waitFor(() => expect(retryConfirmation.confirm).toHaveBeenCalledTimes(1));
    mutate();
    await act(async () => resolveConfirmation?.(true));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(executeQuery).not.toHaveBeenCalled();
  });

  it('blocks confirmed Retry when the panel is removed during confirmation', async () => {
    vi.useRealTimers();
    setFailedQuery('SELECT 1');
    let resolveConfirmation: ((value: boolean) => void) | undefined;
    retryConfirmation.confirm.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          resolveConfirmation = resolve;
        }),
    );

    renderPanel();
    fireEvent.click(screen.getByTestId('query-retry'));
    await waitFor(() => expect(retryConfirmation.confirm).toHaveBeenCalledTimes(1));
    usePanelStore.setState((state) => ({
      panels: state.panels.filter((panel) => panel.id !== PANEL_ID),
    }));
    await act(async () => resolveConfirmation?.(true));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(executeQuery).not.toHaveBeenCalled();
  });
});
