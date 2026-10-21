import { describe, expect, it, beforeEach, vi } from 'vitest';

vi.mock('../../locales/t', () => ({
  t: (key: string) => key,
}));

const mockGetQueryHistory = vi.fn().mockResolvedValue([]);
const mockCancelQuery = vi.fn().mockResolvedValue(undefined);
const mockExecuteQueryStream = vi.fn().mockResolvedValue(undefined);

/** `cfg-1` supports cancel; `cfg-nocancel` does not — cancel must be skipped there. */
const activeConnectionState: {
  connections: Record<
    string,
    {
      capabilities?: {
        supportsCancelQuery: boolean;
        supportsQueryExecutionCancel: boolean;
        supportsExplain: boolean;
        supportsStreamingResults: boolean;
      };
    }
  >;
} = {
  connections: {
    'cfg-1': {
      capabilities: {
        supportsCancelQuery: true,
        supportsQueryExecutionCancel: true,
        supportsExplain: true,
        supportsStreamingResults: true,
      },
    },
    'cfg-nocancel': {
      capabilities: {
        supportsCancelQuery: false,
        supportsQueryExecutionCancel: false,
        supportsExplain: true,
        supportsStreamingResults: true,
      },
    },
  },
};

vi.mock('../../commands/query', () => ({
  queryCommands: {
    getQueryHistory: (...args: unknown[]) => mockGetQueryHistory(...args),
    getFavoriteQueries: vi.fn().mockResolvedValue([]),
    addFavoriteQuery: vi.fn().mockResolvedValue(undefined),
    deleteFavoriteQuery: vi.fn().mockResolvedValue(undefined),
    executeQueryStream: (...args: unknown[]) => mockExecuteQueryStream(...args),
    cancelQuery: (...args: unknown[]) => mockCancelQuery(...args),
    executeQuery: vi.fn().mockResolvedValue({ results: [], totalTimeMs: 10 }),
  },
}));

vi.mock('../../lib/session/runEditorQuery', () => ({
  runEditorQuery: (...args: unknown[]) => {
    const connectionId = String(args[1] ?? '');
    const params = args[5] as Record<string, string | number | boolean | null> | undefined;
    return mockExecuteQueryStream(
      connectionId === 'cfg-2' ? 'sess-2' : 'sess-1',
      args[2],
      args[6],
      {
        database: (args[3] as string | null | undefined) ?? null,
        schema: (args[4] as string | null | undefined) ?? null,
        ...(params ? { params } : {}),
      },
    );
  },
}));

vi.mock('../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: {
    getState: () => activeConnectionState,
  },
}));

const PANE_1 = 'main';
const PANE_2 = 'p2';
const PANE_3 = 'p3';
const PANEL_B = 'panel-q-2';

describe('panelStore pane dimension', () => {
  let usePanelStore: typeof import('../panelStore').usePanelStore;
  let DEFAULT_PANE_ID: typeof import('../paneKeys').DEFAULT_PANE_ID;
  let paneKey: typeof import('../paneKeys').paneKey;
  let paneIdOfPaneKey: typeof import('../paneKeys').paneIdOfPaneKey;
  let panelIdOfPaneKey: typeof import('../paneKeys').panelIdOfPaneKey;
  let isPaneKeyOfPanel: typeof import('../paneKeys').isPaneKeyOfPanel;
  let resolveFocusedPaneId: typeof import('../paneKeys').resolveFocusedPaneId;
  let paneArgs: typeof import('../paneKeys').paneArgs;
  type QueryPanel = import('../panelStore').QueryPanel;

  const PANEL_ID = 'panel-q-1';
  const KEY_1 = `${PANEL_ID}`;
  const KEY_2 = `${PANEL_ID}::${PANE_2}`;

  function makeQueryPanel(id: string, connectionId = 'cfg-1'): QueryPanel {
    return {
      type: 'query',
      id,
      title: 'Q',
      connectionId,
      connectionName: 'TestDB',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      database: 'app',
      schema: null,
    };
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const store = await import('../panelStore');
    usePanelStore = store.usePanelStore;
    const keys = await import('../paneKeys');
    DEFAULT_PANE_ID = keys.DEFAULT_PANE_ID;
    paneKey = keys.paneKey;
    paneIdOfPaneKey = keys.paneIdOfPaneKey;
    panelIdOfPaneKey = keys.panelIdOfPaneKey;
    isPaneKeyOfPanel = keys.isPaneKeyOfPanel;
    resolveFocusedPaneId = keys.resolveFocusedPaneId;
    paneArgs = keys.paneArgs;
    usePanelStore.setState({
      panels: [],
      activePanelId: null,
      queryExec: new Map(),
      focusedPaneId: null,
    });
  });

  /** Exec state of a pane, or undefined when the pane has no entry. */
  function execOf(key: string) {
    return usePanelStore.getState().queryExec.get(key);
  }

  // ── pane key helpers ──────────────────────────────────────────

  describe('pane keys', () => {
    it('keys the default pane by the bare panel id, so pre-pane callers are unaffected', () => {
      expect(DEFAULT_PANE_ID).toBe(PANE_1);
      expect(paneKey(PANEL_ID)).toBe(PANEL_ID);
      expect(paneKey(PANEL_ID, DEFAULT_PANE_ID)).toBe(PANEL_ID);
      expect(paneKey(PANEL_ID, PANE_2)).toBe(KEY_2);
    });

    it('decomposes a pane key back into its panel and pane', () => {
      expect(panelIdOfPaneKey(KEY_1)).toBe(PANEL_ID);
      expect(panelIdOfPaneKey(KEY_2)).toBe(PANEL_ID);
      expect(paneIdOfPaneKey(KEY_1)).toBe(DEFAULT_PANE_ID);
      expect(paneIdOfPaneKey(KEY_2)).toBe(PANE_2);
      expect(isPaneKeyOfPanel(KEY_1, PANEL_ID)).toBe(true);
      expect(isPaneKeyOfPanel(KEY_2, PANEL_ID)).toBe(true);
      expect(isPaneKeyOfPanel(KEY_2, 'panel-q-2')).toBe(false);
    });

    it('resolves a null focus to the default pane and appends no argument pre-split', () => {
      expect(resolveFocusedPaneId(null)).toBe(DEFAULT_PANE_ID);
      expect(resolveFocusedPaneId(undefined)).toBe(DEFAULT_PANE_ID);
      expect(resolveFocusedPaneId(PANE_2)).toBe(PANE_2);
      // A single-pane tab must issue the exact pre-pane call shape.
      expect(paneArgs(DEFAULT_PANE_ID)).toEqual([]);
      expect(paneArgs(undefined)).toEqual([]);
      expect(paneArgs(PANE_2)).toEqual([PANE_2]);
    });
  });

  // ── criterion 1: panes under one panel coexist ────────────────

  it('keeps several panes of one panel independent instead of overwriting each other', () => {
    const s = usePanelStore.getState();
    s.addPanel(makeQueryPanel(PANEL_ID));
    s.openPane(PANEL_ID, PANE_2);

    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 1', PANE_1);
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 2', PANE_2);
    usePanelStore.getState().setActiveResult(PANEL_ID, 1, PANE_2);
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 1b', PANE_1);

    expect(execOf(KEY_1)?.sql).toBe('SELECT 1b');
    expect(execOf(KEY_2)?.sql).toBe('SELECT 2');
    // A pane-local result selection must not leak into its sibling.
    expect(execOf(KEY_1)?.activeResultIdx).toBe(0);
    expect(execOf(KEY_2)?.activeResultIdx).toBe(1);
    expect(usePanelStore.getState().queryExec.size).toBe(2);
  });

  it('omitting the pane targets the default pane, exactly as before panes existed', () => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().openPane(PANEL_ID, PANE_2);
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT default');

    expect(execOf(KEY_1)?.sql).toBe('SELECT default');
    expect(execOf(KEY_2)?.sql).toBe('');
  });

  it('seeds an exec entry for the pane a panel is added into', () => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().addPanel(makeQueryPanel('panel-q-9'), false, PANE_2);

    expect(execOf(KEY_1)).toBeDefined();
    expect(execOf('panel-q-9')).toBeUndefined();
    expect(execOf('panel-q-9::p2')).toBeDefined();
  });

  it('openPane seeds a fresh pane and focuses it, and is idempotent', () => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 1');

    usePanelStore.getState().openPane(PANEL_ID, PANE_2);
    expect(execOf(KEY_2)?.sql).toBe('');
    expect(usePanelStore.getState().focusedPaneId).toBe(PANE_2);
    expect(usePanelStore.getState().queryExec.size).toBe(2);

    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 2', PANE_2);
    usePanelStore.getState().openPane(PANEL_ID, PANE_2);
    expect(execOf(KEY_2)?.sql).toBe('SELECT 2');
    expect(usePanelStore.getState().queryExec.size).toBe(2);
  });

  it('keeps the focusedPaneId mirror equal to the active tab own focus', () => {
    const s = usePanelStore.getState();
    const mirrorFollowsActiveTab = () => {
      const st = usePanelStore.getState();
      expect(st.focusedPaneId).toBe(
        st.activePanelId ? (st.focusedPaneIdByPanel[st.activePanelId] ?? null) : null,
      );
    };

    s.addPanel(makeQueryPanel(PANEL_ID));
    s.addPanel(makeQueryPanel(PANEL_B));
    mirrorFollowsActiveTab();
    s.openPane(PANEL_ID, PANE_2);
    mirrorFollowsActiveTab();
    s.openPane(PANEL_B, PANE_3);
    mirrorFollowsActiveTab();
    s.setActivePanel(PANEL_ID);
    mirrorFollowsActiveTab();
    s.setFocusedPane(PANE_3);
    mirrorFollowsActiveTab();
    s.closePane(PANEL_B, PANE_3);
    mirrorFollowsActiveTab();
    s.setActivePanel(null);
    mirrorFollowsActiveTab();
    s.setActivePanel(PANEL_B);
    s.closePanelsToTheRight(PANEL_ID);
    mirrorFollowsActiveTab();
    s.reset();
    mirrorFollowsActiveTab();
  });

  it('openPane ignores unknown panels and non-query panels', () => {
    const s = usePanelStore.getState();
    s.addPanel({
      type: 'table',
      id: 'tbl-1',
      tableName: 'users',
      subTab: 'data',
      connectionId: 'cfg-1',
      connectionName: 'TestDB',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      database: 'app',
      tableSchema: 'public',
    });

    usePanelStore.getState().openPane('missing-panel', PANE_2);
    usePanelStore.getState().openPane('tbl-1', PANE_2);

    expect(usePanelStore.getState().queryExec.size).toBe(0);
    expect(usePanelStore.getState().focusedPaneId).toBeNull();
  });

  it('closePane drops only that pane and moves focus back to the default pane', () => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().openPane(PANEL_ID, PANE_2);
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 1');
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 2', PANE_2);

    usePanelStore.getState().closePane(PANEL_ID, PANE_2);

    expect(execOf(KEY_2)).toBeUndefined();
    expect(execOf(KEY_1)?.sql).toBe('SELECT 1');
    // Focus falls back to the tab's own pane: the per-tab entry is dropped, so
    // the focus resolves to the default pane and the editor reads that pane.
    const focus = resolveFocusedPaneId(usePanelStore.getState().focusedPaneId);
    expect(usePanelStore.getState().focusedPaneIdByPanel[PANEL_ID]).toBeUndefined();
    expect(focus).toBe(DEFAULT_PANE_ID);
    expect(execOf(paneKey(PANEL_ID, focus))?.sql).toBe('SELECT 1');
    // The tab itself survives a pane close.
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });

  it('closePane drops the pane state and never falls back to the legacy cancel IPC', () => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().openPane(PANEL_ID, PANE_2);
    usePanelStore.setState((s) => ({
      queryExec: new Map(s.queryExec).set(KEY_2, {
        ...(s.queryExec.get(KEY_2) as NonNullable<ReturnType<typeof execOf>>),
        running: true,
        executionId: 'exec-2',
      }),
    }));

    usePanelStore.getState().closePane(PANEL_ID, PANE_2);
    // Cancellation belongs to the running-close guard, which asks the pane's own
    // controller while it still exists (see `runningExecutionClose`). By the time
    // this cleanup runs the session is already released, so there is nothing left
    // to cancel — and the legacy `cancel_query` + panel `dbSessionId` shape could
    // never address a runtime execution (the id and the handle both belong to the
    // pane's runtime session). Both halves are pinned: the guard's own tests cover
    // the cancel, this one forbids the unreachable fallback.
    expect(mockCancelQuery).not.toHaveBeenCalled();
    expect(execOf(KEY_2)).toBeUndefined();

    usePanelStore.getState().addPanel(makeQueryPanel('panel-q-nc', 'cfg-nocancel'));
    usePanelStore.getState().openPane('panel-q-nc', PANE_2);
    usePanelStore.setState((s) => ({
      queryExec: new Map(s.queryExec).set('panel-q-nc::p2', {
        ...(s.queryExec.get('panel-q-nc::p2') as NonNullable<ReturnType<typeof execOf>>),
        running: true,
        executionId: 'exec-nc',
      }),
    }));
    mockCancelQuery.mockClear();

    usePanelStore.getState().closePane('panel-q-nc', PANE_2);
    // No cancel capability, no live session, and no legacy fallback either.
    expect(mockCancelQuery).not.toHaveBeenCalled();
    expect(execOf('panel-q-nc::p2')).toBeUndefined();
  });

  // ── criterion 3 (store half): focus state lives in the store ──

  it('setFocusedPane routes editor actions and survives a tab switch', () => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().addPanel(makeQueryPanel('panel-q-2'));
    usePanelStore.getState().openPane(PANEL_ID, PANE_2);

    // Focus is store state, so remounting the tab (ContentView unmounts inactive
    // panels) cannot lose it.
    usePanelStore.getState().setActivePanel('panel-q-2');
    usePanelStore.getState().setActivePanel(PANEL_ID);
    expect(usePanelStore.getState().focusedPaneId).toBe(PANE_2);

    usePanelStore.getState().setFocusedPane(null);
    expect(usePanelStore.getState().focusedPaneId).toBeNull();
  });

  // ── criterion 4: closing a tab cleans every pane of it ───────

  it('removing a tab cleans up every one of its panes with no residue', () => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().addPanel(makeQueryPanel('panel-q-2'));
    usePanelStore.getState().openPane(PANEL_ID, PANE_2);
    usePanelStore.getState().openPane(PANEL_ID, 'p3');
    usePanelStore.getState().openPane('panel-q-2', PANE_2);
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 1');
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 2', PANE_2);
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 3', 'p3');
    usePanelStore.getState().updateSql('panel-q-2', 'SELECT other');

    // Both panes of the removed tab are mid-run. Removing the tab releases every
    // one of them; the cancel itself is the running-close guard's job, and the
    // cleanup path must not re-issue a cancel it cannot deliver.
    usePanelStore.setState((s) => {
      const next = new Map(s.queryExec);
      next.set(KEY_1, {
        ...(next.get(KEY_1) as NonNullable<ReturnType<typeof execOf>>),
        running: true,
        executionId: 'exec-1',
      });
      next.set(KEY_2, {
        ...(next.get(KEY_2) as NonNullable<ReturnType<typeof execOf>>),
        running: true,
        executionId: 'exec-2',
      });
      return { queryExec: next };
    });

    usePanelStore.getState().removePanel(PANEL_ID);

    expect(mockCancelQuery).not.toHaveBeenCalled();
    // No residue: not the default pane, the second pane, the third pane, nor a
    // prefix of the panel id, and the sibling tab keeps both of its panes.
    const remainingKeys = [...usePanelStore.getState().queryExec.keys()];
    expect(remainingKeys.sort()).toEqual(['panel-q-2', 'panel-q-2::p2'].sort());
  });

  it.each([
    ['removeAllForConnection', (id: string) => usePanelStore.getState().removeAllForConnection(id)],
    ['closeAllPanels', () => usePanelStore.getState().closeAllPanels()],
  ])('%s cleans every pane of every removed query tab', (_name, close) => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().addPanel(makeQueryPanel('panel-q-2'));
    usePanelStore.getState().openPane(PANEL_ID, PANE_2);
    usePanelStore.getState().openPane('panel-q-2', PANE_2);
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 1', PANE_2);
    usePanelStore.setState((s) => {
      const next = new Map(s.queryExec);
      next.set(KEY_1, {
        ...(next.get(KEY_1) as NonNullable<ReturnType<typeof execOf>>),
        running: true,
        executionId: 'exec-1',
      });
      next.set('panel-q-2::p2', {
        ...(next.get('panel-q-2::p2') as NonNullable<ReturnType<typeof execOf>>),
        running: true,
        executionId: 'exec-other',
      });
      return { queryExec: next };
    });

    close('cfg-1');

    expect(usePanelStore.getState().queryExec.size).toBe(0);
    expect(mockCancelQuery).not.toHaveBeenCalled();
  });

  it('reset clears the focused pane along with the exec map', () => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().openPane(PANEL_ID, PANE_2);
    usePanelStore.getState().setFocusedPane(PANE_2);

    usePanelStore.getState().reset();

    expect(usePanelStore.getState().queryExec.size).toBe(0);
    expect(usePanelStore.getState().focusedPaneId).toBeNull();
    expect(usePanelStore.getState().panels).toEqual([]);
  });

  // ── criterion 3 (store half): streaming writes stay on their pane ──

  it('streams results into the pane that started the run, not the focused one', async () => {
    usePanelStore.getState().addPanel(makeQueryPanel(PANEL_ID));
    usePanelStore.getState().openPane(PANEL_ID, PANE_2);
    usePanelStore.getState().updateSql(PANEL_ID, 'SELECT 1', PANE_2);

    let emit: ((event: unknown) => void) | undefined;
    mockExecuteQueryStream.mockImplementation(
      (_sess: string, _sql: string, onEvent: (event: unknown) => void) => {
        emit = onEvent;
        // Never settles: the run stays in flight while focus moves.
        return new Promise<void>(() => {});
      },
    );

    const run = usePanelStore.getState().executeQuery(PANEL_ID, undefined, PANE_2);
    // Focus moves while the query is in flight: the captured pane key must win.
    usePanelStore.getState().setFocusedPane(PANE_1);
    emit?.({ type: 'statementStart', index: 0, sql: 'SELECT 1', columns: [{ name: 'n' }] });
    emit?.({ type: 'rows', index: 0, rows: [[1]] });

    expect(execOf(KEY_2)?.results[0]?.rows).toEqual([[1]]);
    expect(execOf(KEY_1)?.results).toHaveLength(0);
    // The untouched pane is not even marked as running.
    expect(execOf(KEY_1)?.running).toBe(false);
    expect(execOf(KEY_2)?.running).toBe(true);

    usePanelStore.getState().setFocusedPane(PANE_1);
    void run;
  });
});
