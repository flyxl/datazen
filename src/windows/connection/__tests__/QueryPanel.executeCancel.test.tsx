import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, act, waitFor } from '@testing-library/react';
import { QueryPanel } from '../QueryPanel';
import { usePanelStore } from '../../../stores/panelStore';
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
}));

vi.mock('../../../stores/settingsStore', () => ({
  useSettingsStore: (
    sel: (s: { settings: { safeMode: boolean; autoCommit: boolean } }) => unknown,
  ) => sel({ settings: { safeMode: false, autoCommit: true } }),
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: (
    selector: (state: {
      connections: Record<
        string,
        { capabilities?: {
          supportsCancelQuery: boolean;
          supportsQueryExecutionCancel: boolean;
          supportsExplain: boolean;
          supportsStreamingResults: boolean;
        } }
      >;
    }) => unknown,
  ) =>
    selector({
      connections: {
        'cfg-1': panelConnectionState,
      },
    }),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useSchemaStore: (sel: (s: Record<string, unknown>) => unknown) =>
    sel({
      tables: [],
      views: [],
      columnMap: new Map(),
      namespaceTree: [],
      pathAliases: {},
      databases: [],
      currentDatabase: 'app',
      isMultiDatabase: false,
      ensuringCount: 0,
      ensureColumns: vi.fn(),
      loadTables: vi.fn(),
      ensureNamespacePath: vi.fn(),
    }),
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
  BindParamPanel: () => null,
}));

vi.mock('../../../components/query/QueryContextSelectors', () => ({
  QueryContextSelectors: () => null,
}));

vi.mock('../../../components/query/QueryErrorPanel', () => ({
  QueryErrorPanel: ({ onFixSql }: { onFixSql?: () => void }) =>
    onFixSql ? (
      <button type="button" onClick={onFixSql}>
        test-fix-sql
      </button>
    ) : null,
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
    usePanelStore.setState({
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
});
