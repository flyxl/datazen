import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup, screen, act, waitFor } from '@testing-library/react';
import { QueryPanel } from '../QueryPanel';
import { usePanelStore, type QueryPanel as QueryPanelState } from '../../../stores/panelStore';
import { EMPTY_QUERY_EXEC } from '../../../stores/queryExecActions';

/**
 * Plan `platform-development-plan.md:137` — 删除执行前从 SQL 推测已生效上下文的行为,
 * 保留补全与浏览推测。
 *
 * Inference is **not** state: naming a foreign database while typing, or before
 * running, must never change what the session believes is effective. Only the
 * explicit paths (database dropdown / object tree / session switch) may do that.
 *
 * These two cases are the state machine that used to violate it:
 *   - entry: `exec.sql` names another database (`other.users`);
 *   - in-state behaviour: a 50 ms keystroke debounce, and a pre-execution sync,
 *     both resolved the path and pushed it into the session/panel;
 *   - exit: the rewrite itself — which must no longer be reachable from either
 *     entry point, in any intermediate typing state (including the `other.`
 *     trailing-dot form that resolved *before* the relation name existed).
 */

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
  }): number => 32 + expandedButtonCount * 96 + Math.max(0, expandedButtonCount - 1) * 8 + fixedExtraWidth,
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
  tables: [] as Array<{ name: string; tableType: 'table' | 'view'; schema?: string }>,
  views: [] as Array<{ name: string; tableType: 'table' | 'view'; schema?: string }>,
  columnMap: {} as Record<string, string[]>,
  namespaceTree: [],
  pathAliases: {},
  databases: ['app', 'other'] as string[],
  currentDatabase: 'app' as string | null,
  currentSchema: 'public' as string | null,
  isMultiDatabase: true,
  /** The write the deleted chain performed; asserting on it is the whole point. */
  switchDatabase: vi.fn(),
  ensureColumns: vi.fn(),
  ensureDatabaseColumns: vi.fn(),
  loadTables: vi.fn(),
  ensureNamespacePath: vi.fn(),
}));

const settingsState = vi.hoisted(() => ({ safeMode: false }));

const executeQuery = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
const updatePanel = vi.hoisted(() => vi.fn());

vi.mock('../../../stores/settingsStore', () => {
  const state = () => ({ settings: { safeMode: settingsState.safeMode, autoCommit: true } });
  return {
    useSettingsStore: Object.assign(
      (sel: (s: { settings: { safeMode: boolean; autoCommit: boolean } }) => unknown) =>
        sel(state()),
      { getState: () => state() },
    ),
  };
});

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: Object.assign(
    (selector: (state: typeof activeConnectionStoreState) => unknown) =>
      selector(activeConnectionStoreState),
    { getState: () => activeConnectionStoreState },
  ),
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: Object.assign(
    (selector: (state: { connections: unknown[] }) => unknown) => selector({ connections: [] }),
    { getState: () => ({ connections: [] }) },
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
vi.mock('../../../components/query/QueryErrorPanel', () => ({ QueryErrorPanel: () => null }));
vi.mock('../dashboard/AddToDashboardDialog', () => ({ AddToDashboardDialog: () => null }));
vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (
    sel: (s: {
      diagnosis: null;
      isDiagnosing: boolean;
      diagnosisError: null;
      isConfigured: boolean;
      diagnoseError: ReturnType<typeof vi.fn>;
      clearDiagnosis: ReturnType<typeof vi.fn>;
    }) => unknown,
  ) =>
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
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock('../../../hooks/useConfirmDialog', () => ({
  useConfirmDialog: () => [vi.fn(async () => true), null],
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    sessionTransactionStatus: vi.fn().mockResolvedValue(false),
    beginSessionTransaction: vi.fn().mockResolvedValue(undefined),
    commitSessionTransaction: vi.fn().mockResolvedValue(undefined),
    rollbackSessionTransaction: vi.fn().mockResolvedValue(undefined),
  },
}));

const PANEL_ID = 'panel-context-inference';

/** The removed debounce was 50 ms; settle past it at every intermediate state. */
const SETTLE_MS = 120;

afterEach(cleanup);

describe('[tester] QueryPanel never rewrites the effective context from SQL', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    settingsState.safeMode = false;
    schemaStoreState.databases = ['app', 'other'];
    schemaStoreState.currentDatabase = 'app';
    schemaStoreState.isMultiDatabase = true;
    schemaStoreState.switchDatabase.mockResolvedValue(undefined);
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
          schema: 'public',
        } satisfies QueryPanelState,
      ],
      queryExec: new Map([[PANEL_ID, { ...EMPTY_QUERY_EXEC, sql: 'SELECT 1', running: false }]]),
      historyVisible: false,
      favoritesVisible: false,
      queryHistory: [],
      queryFavorites: [],
      loadHistory: vi.fn().mockResolvedValue(undefined),
      loadFavorites: vi.fn().mockResolvedValue(undefined),
      cancelQuery: vi.fn(),
      executeQuery,
      updatePanel,
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
        database="app"
        schema="public"
      />,
    );
  }

  /** One keystroke's worth of settle time, flushed through React. */
  async function settle() {
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, SETTLE_MS));
    });
  }

  it('leaves the effective context alone while the user types a foreign qualified relation', async () => {
    renderPanel();

    // Continuous keystroke journey, including the two incomplete intermediate
    // forms: a partially typed relation, and `other.` before the relation name.
    const keystrokes = [
      'S',
      'SELECT *',
      'SELECT * FROM oth',
      'SELECT * FROM other.',
      'SELECT * FROM other.users',
    ];
    for (const sql of keystrokes) {
      setSql(sql);
      await settle();
      expect(schemaStoreState.switchDatabase).not.toHaveBeenCalled();
      expect(updatePanel).not.toHaveBeenCalled();
    }

    expect(schemaStoreState.switchDatabase).not.toHaveBeenCalled();
    expect(updatePanel).not.toHaveBeenCalled();
  });

  it('runs a foreign qualified relation without switching the effective context first', async () => {
    setSql('SELECT * FROM other.users');
    renderPanel();

    fireEvent.click(screen.getByRole('button', { name: 'query.execute' }));
    await waitFor(() => expect(executeQuery).toHaveBeenCalledTimes(1));

    expect(schemaStoreState.switchDatabase).not.toHaveBeenCalled();
    expect(updatePanel).not.toHaveBeenCalled();
  });
});