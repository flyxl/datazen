import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, act, waitFor } from '@testing-library/react';
import { QueryPanel } from '../QueryPanel';
import {
  usePanelStore,
  type QueryExecState,
  type QueryPanel as QueryPanelState,
} from '../../../stores/panelStore';
import { EMPTY_QUERY_EXEC } from '../../../stores/queryExecActions';
import { extensionRegistry, sqlEditorEnhancedEP } from '@datazen/extension-points';
import { aiCommands } from '../../../commands/ai';
import { queryCommands } from '../../../commands/query';
import { requestQueryRun } from '../../../stores/queryRunRequestStore';
import type { QueryHistoryEntry } from '../../../types';

/**
 * Editor surface consumed by QueryPanel. Recorded on every render so the test
 * can drive the same host actions the toolbar and the editor itself invoke.
 */
interface RecordedEditorProps {
  value?: string;
  onChange?: (value: string) => void;
  onExecute?: () => void;
  onExecuteSelection?: (sql: string) => void;
  onExecuteAll?: () => void;
  onFormat?: () => void;
  onSaveQuery?: () => void;
}

const editorProps = vi.hoisted(() => ({ current: null as unknown }));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 280, handleRef: { current: null } }),
}));

vi.mock('../../../hooks/useCompactToolbar', () => ({
  estimateExpandedToolbarWidth: () => 480,
  TOOLBAR_GAP: 8,
  TOOLBAR_HORIZONTAL_PADDING: 32,
  useCompactToolbar: () => ({ ref: { current: null }, compact: false }),
}));

const panelConnectionState = vi.hoisted(() => ({
  connectionId: 'cfg-1',
  dbSessionId: 'sess-conn-1',
  status: 'connected' as 'connected' | 'connecting' | 'idle' | 'error',
  serverInfo: { serverVersion: '16', serverType: 'PostgreSQL' },
  capabilities: {
    supportsCancelQuery: true,
    supportsQueryExecutionCancel: true,
    supportsExplain: true,
    supportsStreamingResults: true,
  } as
    | {
        supportsCancelQuery: boolean;
        supportsQueryExecutionCancel: boolean;
        supportsExplain: boolean;
        supportsStreamingResults: boolean;
      }
    | undefined,
  currentDatabase: 'app' as string | null,
  error: null as string | null,
}));

const activeConnectionStoreState = vi.hoisted(() => ({
  connections: {} as Record<string, typeof panelConnectionState>,
}));

const aiState = vi.hoisted(() => ({
  diagnosis: null as unknown,
  isDiagnosing: false,
  diagnosisError: null as string | null,
  isConfigured: true,
  settingsConfig: null as null | { memory: { enabled: boolean; autoLearn: boolean } },
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
  ensureDatabaseColumns: vi.fn(),
  loadTables: vi.fn(),
  ensureNamespacePath: vi.fn(),
}));

vi.mock('../../../stores/settingsStore', () => {
  // `getState` matters: the format path reads `sqlFormatOptions` off it, and a
  // missing `getState` would be swallowed by handleFormat's try/catch.
  const settings = { safeMode: false, autoCommit: true, sqlFormatOptions: {} };
  const useSettingsStore = Object.assign(
    (sel: (s: { settings: typeof settings }) => unknown) => sel({ settings }),
    { getState: () => ({ settings }) },
  );
  return { useSettingsStore };
});

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (selector: (state: typeof activeConnectionStoreState) => unknown) =>
      selector(activeConnectionStoreState),
    { getState: () => activeConnectionStoreState },
  ),
}));

vi.mock('../../../stores/schemaStore', () => ({
  useConnectionColumnMaps: (_session: string) => ({
    columnMap: schemaStoreState.columnMap,
    typedColumnMap: {},
  }),
  useConnectionSchemaField: (_session: string, field: keyof typeof schemaStoreState) =>
    schemaStoreState[field],
  useSchemaStore: Object.assign(
    (sel: (s: typeof schemaStoreState) => unknown) => sel(schemaStoreState),
    { getState: () => schemaStoreState },
  ),
}));

// No imperative handle: the panel's `formatDocument` preference has to fall back
// to the store-routed `onFormat` path, which is the routing we want to prove.
vi.mock('../../../components/SqlEditor', async () => {
  const { forwardRef } = await import('react');
  return {
    SqlEditor: forwardRef((props: unknown, _ref: unknown) => {
      editorProps.current = props;
      return <div data-testid="mock-sql-editor" />;
    }),
  };
});

vi.mock('../../../components/DataTable/DataTable', () => ({ DataTable: () => null }));
vi.mock('../../../components/chart/ChartView', () => ({ ChartView: () => null }));
vi.mock('../../../components/ai/Nl2SqlPanel', () => ({ Nl2SqlPanel: () => null }));
vi.mock('../../../components/ai/ExplainPanel', () => ({ ExplainPanel: () => null }));
vi.mock('../../../components/ai/DiagnosisPanel', () => ({ DiagnosisPanel: () => null }));

vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (sel: (s: typeof aiState) => unknown) => sel(aiState),
}));

extensionRegistry.register(sqlEditorEnhancedEP, {
  useBindParameters: () => ({
    params: [],
    values: {},
    labels: {},
    activeParamIds: [],
    paramHistory: {},
    setValue: vi.fn(),
    applyHistoryEntry: vi.fn(),
    clearHistory: vi.fn(),
    markSubmitted: vi.fn(),
    getHistory: () => [],
  }),
  renderBindParamPanel: () => null,
});

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn().mockResolvedValue(true), null],
}));

vi.mock('../../../components/query/QueryContextSelectors', () => ({
  QueryContextSelectors: () => null,
}));
vi.mock('../../../components/query/QueryErrorPanel', () => ({ QueryErrorPanel: () => null }));
vi.mock('../dashboard/AddToDashboardDialog', () => ({ AddToDashboardDialog: () => null }));
// Partial mock: the favorite dialog is replaced by a probe that exposes the SQL
// it was opened with, so `onSaveQuery` routing is observable end to end.
vi.mock('../query/QueryTransactionModals', async () => {
  const actual = await vi.importActual<typeof import('../query/QueryTransactionModals')>(
    '../query/QueryTransactionModals',
  );
  return {
    ...actual,
    FavoriteNameDialog: ({
      open,
      favoriteName,
      favoriteDialogSql,
      onFavoriteNameChange,
      onSave,
    }: {
      open: boolean;
      favoriteName: string;
      favoriteDialogSql: string;
      onFavoriteNameChange: (name: string) => void;
      onSave: () => void;
    }) =>
      open ? (
        <div>
          <div data-testid="favorite-dialog-sql">{favoriteDialogSql}</div>
          <input
            data-testid="favorite-dialog-name"
            value={favoriteName}
            onChange={(e) => onFavoriteNameChange(e.target.value)}
          />
          <button type="button" data-testid="favorite-dialog-save" onClick={onSave}>
            save
          </button>
        </div>
      ) : null,
  };
});

const PANEL_ID = 'panel-test';
const PANE_2 = 'p2';
const KEY_2 = `${PANEL_ID}::${PANE_2}`;
const SQL_PANE_1 = 'SELECT 111111';
const SQL_PANE_2 = 'SELECT 222222';

const spies = vi.hoisted(() => ({
  updateSql: vi.fn(),
  executeQuery: vi.fn(),
  executeSelection: vi.fn(),
  cancelQuery: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
});

describe('QueryPanel routes editor actions to the focused pane', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // The toolbar's data-testids are E2E-only; stubbing the flag is how the
    // existing module tests reach the real buttons.
    vi.stubEnv('VITE_E2E', '1');
    editorProps.current = null;
    spies.executeQuery.mockResolvedValue(undefined);
    spies.executeSelection.mockResolvedValue(undefined);
    spies.cancelQuery.mockResolvedValue(undefined);
    panelConnectionState.capabilities = {
      supportsCancelQuery: true,
      supportsQueryExecutionCancel: true,
      supportsExplain: true,
      supportsStreamingResults: true,
    };
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
          database: 'app',
          schema: null,
        } satisfies QueryPanelState,
      ],
      // Two panes of the same tab: the default one and a secondary one.
      queryExec: new Map([
        [PANEL_ID, { ...EMPTY_QUERY_EXEC, sql: SQL_PANE_1, running: false }],
        [KEY_2, { ...EMPTY_QUERY_EXEC, sql: SQL_PANE_2, running: false }],
      ]),
      focusedPaneId: PANE_2,
      historyVisible: false,
      favoritesVisible: false,
      queryHistory: [],
      queryFavorites: [],
      loadHistory: vi.fn().mockResolvedValue(undefined),
      loadFavorites: vi.fn().mockResolvedValue(undefined),
      updateSql: spies.updateSql,
      executeQuery: spies.executeQuery,
      executeSelection: spies.executeSelection,
      cancelQuery: spies.cancelQuery,
      // Only the handful of store fields this test overrides; the rest keep
      // their real implementations. `unknown` bridges the PanelState &
      // PanelActions intersection that `getState()` returns.
    } as unknown as Partial<ReturnType<typeof usePanelStore.getState>>);
  });

  function editor(): RecordedEditorProps {
    return editorProps.current as RecordedEditorProps;
  }

  function renderPanel(focusedPaneId: string | null = PANE_2) {
    return render(
      <QueryPanel
        panelId={PANEL_ID}
        focusedPaneId={focusedPaneId}
        dbSessionId="sess-conn-1"
        connectionId="cfg-1"
        databaseType="postgresql"
        database="app"
        schema={null}
      />,
    );
  }

  it('shows the focused pane editor, not the other pane of the same tab', () => {
    renderPanel();
    expect(screen.getByTestId('mock-sql-editor')).toBeInTheDocument();
    expect(editor().value).toBe(SQL_PANE_2);
  });

  it('routes onExecute / onExecuteAll / onExecuteSelection to the focused pane', async () => {
    renderPanel();

    await act(async () => {
      editor().onExecute?.();
    });
    expect(spies.executeQuery).toHaveBeenLastCalledWith(PANEL_ID, undefined, PANE_2);

    await act(async () => {
      editor().onExecuteAll?.();
    });
    expect(spies.executeQuery).toHaveBeenLastCalledWith(PANEL_ID, undefined, PANE_2);

    await act(async () => {
      editor().onExecuteSelection?.('SELECT 333333');
    });
    expect(spies.executeSelection).toHaveBeenLastCalledWith(
      PANEL_ID,
      'SELECT 333333',
      undefined,
      PANE_2,
    );
  });

  it('routes the toolbar execute button to the focused pane', async () => {
    renderPanel();
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));
    });
    expect(spies.executeQuery).toHaveBeenLastCalledWith(PANEL_ID, undefined, PANE_2);
  });

  it('routes formatDocument to the focused pane SQL', () => {
    renderPanel();
    // The mocked editor exposes no imperative handle, so the toolbar falls back
    // to the store-routed `onFormat` path — the routing under test.
    act(() => {
      fireEvent.click(screen.getByTestId('editor-format-button'));
    });
    expect(spies.updateSql).toHaveBeenCalledTimes(1);
    const [targetPanel, formattedSql, pane] = spies.updateSql.mock.calls[0] as [
      string,
      string,
      string,
    ];
    expect(targetPanel).toBe(PANEL_ID);
    expect(formattedSql).toContain('222222');
    expect(formattedSql).not.toContain('111111');
    expect(pane).toBe(PANE_2);
  });

  it('routes onSaveQuery to the focused pane SQL, through to the saved favorite', () => {
    const addFavorite = vi.fn().mockResolvedValue(undefined);
    usePanelStore.setState({ addFavorite } as Partial<ReturnType<typeof usePanelStore.getState>>);
    renderPanel();

    act(() => {
      editor().onSaveQuery?.();
    });

    // The favorite dialog opens pre-filled with the focused pane's SQL only.
    expect(screen.getByTestId('favorite-dialog-sql')).toHaveTextContent(SQL_PANE_2);
    expect(screen.getByTestId('favorite-dialog-sql').textContent).not.toContain('111111');

    fireEvent.change(screen.getByTestId('favorite-dialog-name'), {
      target: { value: 'my query' },
    });
    fireEvent.click(screen.getByTestId('favorite-dialog-save'));

    expect(addFavorite).toHaveBeenCalledWith('my query', SQL_PANE_2, 'cfg-1');
  });

  it('routes the toolbar save button to the focused pane too', () => {
    renderPanel();
    act(() => {
      fireEvent.click(screen.getByTestId('editor-save-button'));
    });
    expect(screen.getByTestId('favorite-dialog-sql')).toHaveTextContent(SQL_PANE_2);
  });

  it('routes editor typing (onChange) to the focused pane only', () => {
    renderPanel();
    act(() => {
      editor().onChange?.('SELECT 444444');
    });
    expect(spies.updateSql).toHaveBeenCalledWith(PANEL_ID, 'SELECT 444444', PANE_2);
    // The sibling pane of the same tab is never touched.
    expect(
      spies.updateSql.mock.calls.every((call) => call[0] === PANEL_ID && call[2] === PANE_2),
    ).toBe(true);
  });

  it('routes cancel to the focused pane', () => {
    usePanelStore.setState((s) => ({
      queryExec: new Map(s.queryExec).set(KEY_2, {
        ...(s.queryExec.get(KEY_2) as QueryExecState),
        running: true,
        executionId: 'exec-pane-2',
      }),
    }));
    renderPanel();

    act(() => {
      fireEvent.click(screen.getByRole('button', { name: 'query.stop' }));
    });
    expect(spies.cancelQuery).toHaveBeenCalledWith(PANEL_ID, PANE_2);
  });

  it('leaves the sibling pane of the same tab untouched', async () => {
    renderPanel();
    await act(async () => {
      editor().onExecute?.();
    });
    act(() => {
      editor().onChange?.('SELECT 555555');
    });

    // No call may have targeted the tab's default pane while pane 2 is focused.
    for (const call of [
      ...spies.executeQuery.mock.calls,
      ...spies.executeSelection.mock.calls,
      ...spies.updateSql.mock.calls,
    ]) {
      expect(call[2]).toBe(PANE_2);
    }
  });

  it('issues the exact pre-pane call shape when the tab is not split', async () => {
    usePanelStore.setState({ focusedPaneId: null });
    renderPanel(null);

    expect(editor().value).toBe(SQL_PANE_1);

    await act(async () => {
      editor().onExecute?.();
    });
    // No pane argument at all — the pre-split call contract, byte for byte.
    expect(spies.executeQuery).toHaveBeenLastCalledWith(PANEL_ID, undefined);

    act(() => {
      editor().onChange?.('SELECT 666666');
    });
    expect(spies.updateSql).toHaveBeenCalledWith(PANEL_ID, 'SELECT 666666');
  });
  it('extracts candidate patterns after a successful execution from recent history', async () => {
    aiState.settingsConfig = { memory: { enabled: true, autoLearn: true } };
    const repeated = 'SELECT * FROM orders o JOIN users u ON o.user_id = u.id';
    const history: QueryHistoryEntry[] = [1, 2, 3].map((index) => ({
      id: `h-${index}`,
      connectionId: 'cfg-1',
      database: 'app',
      sql: repeated,
      executedAt: `2026-04-1${index}T12:00:00.000Z`,
      executionTimeMs: 5,
      success: true,
    }));
    const historySpy = vi
      .spyOn(queryCommands, 'getQueryHistoryPage')
      .mockResolvedValue({ entries: history, total: history.length });
    const extractSpy = vi.spyOn(aiCommands, 'memoryExtractCandidates').mockResolvedValue([]);

    renderPanel();
    usePanelStore.setState({
      queryExec: new Map([
        [
          KEY_2,
          {
            ...EMPTY_QUERY_EXEC,
            sql: repeated,
            terminalState: 'succeeded' as const,
          },
        ],
      ]),
    });

    await act(async () => {
      editor().onExecute?.();
    });
    await waitFor(() => expect(extractSpy).toHaveBeenCalledTimes(1));

    expect(historySpy).toHaveBeenCalledWith(
      expect.objectContaining({
        limit: 100,
        connectionId: 'cfg-1',
        database: 'app',
        order: 'recent',
      }),
    );
    expect(extractSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        sql: repeated,
        connectionId: 'cfg-1',
        database: 'app',
        history: expect.arrayContaining([expect.objectContaining({ sql: repeated })]),
        windowDays: 14,
        minOccurrences: 3,
      }),
    );
    const sent = extractSpy.mock.calls[0][0] as { sql: string; history: unknown[] };
    expect(1 + sent.history.length).toBeGreaterThanOrEqual(3);
  });

  it('turns a Chat SQL edit that later succeeds into a correction candidate', async () => {
    const correctionSpy = vi.spyOn(aiCommands, 'memoryCorrectionCandidate').mockResolvedValue(null);

    renderPanel();
    await act(async () => {
      requestQueryRun(PANEL_ID, 'SELECT chat_generated');
      await Promise.resolve();
    });
    await waitFor(() => expect(spies.executeSelection).toHaveBeenCalled());
    expect(spies.updateSql).toHaveBeenCalledWith(PANEL_ID, 'SELECT chat_generated', PANE_2);

    // The user edits the Chat-provided SQL and reruns it successfully.
    usePanelStore.setState({
      queryExec: new Map([
        [
          KEY_2,
          {
            ...EMPTY_QUERY_EXEC,
            sql: 'SELECT chat_edited',
            terminalState: 'succeeded' as const,
          },
        ],
      ]),
    });
    await act(async () => {
      editor().onExecute?.();
    });

    await waitFor(() =>
      expect(correctionSpy).toHaveBeenCalledWith({
        originalSql: 'SELECT chat_generated',
        newSql: 'SELECT chat_edited',
        connectionId: 'cfg-1',
        database: 'app',
      }),
    );
  });
});
