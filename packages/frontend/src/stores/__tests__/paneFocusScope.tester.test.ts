/**
 * [tester] Pane focus must be scoped to the tab that owns the pane.
 *
 * `focusedPaneId` is a single global field on `panelStore`, and `ContentView`
 * (src/windows/connection/ContentView.tsx:88) hands that one value to whichever
 * panel is currently active. Panes, on the other hand, belong to a *panel*.
 *
 * Three consequences, all pinned below:
 *
 *  1. Splitting one tab makes every *other* tab resolve its editor actions to
 *     `<its own panelId>::<foreign paneId>` — a key no `addPanel` ever seeded.
 *     That tab's real SQL and results become unreachable, and it renders an
 *     empty editor.
 *  2. Editing that tab writes into the undeclared key. `patchExec` seeds
 *     `emptyQueryExecState()` for an unknown key, so the tab silently grows an
 *     orphan exec entry that no pane list ever declared and that Execute then
 *     streams results into — results the user never sees.
 *  3. `closePane` compares only the pane id, never the owning panel id
 *     (panelStore.ts:378), so closing a pane in one tab clears the focus that
 *     belongs to another tab.
 *
 * All three are latent today — no split UI calls `openPane`/`closePane` yet —
 * but they sit on the exact seam this wave exists to build, so they are pinned
 * here as the target for the fix.
 */

import { describe, expect, it, beforeEach, vi } from 'vitest';

vi.mock('../../locales/t', () => ({
  t: (key: string) => key,
}));

const mockGetQueryHistoryPage = vi.fn().mockResolvedValue({ entries: [], total: 0 });
const mockCancelQuery = vi.fn().mockResolvedValue(undefined);
const mockExecuteQueryStream = vi.fn().mockResolvedValue(undefined);

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
  },
};

vi.mock('../../commands/query', () => ({
  queryCommands: {
    getQueryHistoryPage: (...args: unknown[]) => mockGetQueryHistoryPage(...args),
    getFavoriteQueries: vi.fn().mockResolvedValue([]),
    addFavoriteQuery: vi.fn().mockResolvedValue(undefined),
    deleteFavoriteQuery: vi.fn().mockResolvedValue(undefined),
    executeQueryStream: (...args: unknown[]) => mockExecuteQueryStream(...args),
    cancelQuery: (...args: unknown[]) => mockCancelQuery(...args),
    executeQuery: vi.fn().mockResolvedValue({ results: [], totalTimeMs: 10 }),
  },
}));

vi.mock('../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: {
    getState: () => activeConnectionState,
  },
}));

const PANEL_A = 'panel-q-1';
const PANEL_B = 'panel-q-2';
const PANE_2 = 'p2';

describe('[tester] focusedPaneId must be scoped to the tab that owns the pane', () => {
  let usePanelStore: typeof import('../panelStore').usePanelStore;
  let DEFAULT_PANE_ID: typeof import('../paneKeys').DEFAULT_PANE_ID;
  let paneKey: typeof import('../paneKeys').paneKey;
  let paneArgs: typeof import('../paneKeys').paneArgs;
  let resolveFocusedPaneId: typeof import('../paneKeys').resolveFocusedPaneId;

  function makeQueryPanel(id: string) {
    return {
      type: 'query' as const,
      id,
      title: 'Q',
      connectionId: 'cfg-1',
      connectionName: 'TestDB',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql' as const,
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
    paneArgs = keys.paneArgs;
    resolveFocusedPaneId = keys.resolveFocusedPaneId;
    usePanelStore.setState({
      panels: [],
      activePanelId: null,
      queryExec: new Map(),
      focusedPaneId: null,
    });
  });

  /** Exec-map key the given tab's editor reads, as `QueryPanel` resolves it. */
  function routedKeyFor(panelId: string): string {
    // Deliberately expressed through the store's public surface: whatever
    // `focusedPaneId` currently says, resolved the way `resolveFocusedPaneId`
    // resolves it before `paneKey()`.
    return paneKey(panelId, usePanelStore.getState().focusedPaneId ?? DEFAULT_PANE_ID);
  }

  it('routes an unsplit sibling tab to its own pane, not a foreign pane id', () => {
    const s = usePanelStore.getState();
    s.addPanel(makeQueryPanel(PANEL_A));
    s.addPanel(makeQueryPanel(PANEL_B));
    usePanelStore.getState().updateSql(PANEL_B, 'SELECT sibling');
    usePanelStore.getState().openPane(PANEL_A, PANE_2);

    // Tab A is split, so the focus is its secondary pane.
    expect(usePanelStore.getState().focusedPaneId).toBe(PANE_2);

    // Switching to the unsplit sibling must not hand it tab A's pane id.
    usePanelStore.getState().setActivePanel(PANEL_B);
    expect(routedKeyFor(PANEL_B)).toBe(PANEL_B);
    // So the sibling still reads the SQL it always had.
    expect(usePanelStore.getState().queryExec.get(routedKeyFor(PANEL_B))?.sql).toBe(
      'SELECT sibling',
    );
  });

  it('never grows an exec entry for a pane the tab never opened', () => {
    const s = usePanelStore.getState();
    s.addPanel(makeQueryPanel(PANEL_A));
    s.addPanel(makeQueryPanel(PANEL_B));
    usePanelStore.getState().openPane(PANEL_A, PANE_2);
    usePanelStore.getState().setActivePanel(PANEL_B);

    // Type in the unsplit sibling exactly the way `QueryPanel` does: resolve the
    // store's focus, then hand it to `updateSql` as a trailing pane argument.
    const paneId = resolveFocusedPaneId(usePanelStore.getState().focusedPaneId);
    usePanelStore.getState().updateSql(PANEL_B, 'SELECT edited', ...paneArgs(paneId));

    // A's default pane + A's second pane + B's own pane, and nothing else.
    expect([...usePanelStore.getState().queryExec.keys()].sort()).toEqual(
      [PANEL_A, paneKey(PANEL_A, PANE_2), PANEL_B].sort(),
    );
    expect(usePanelStore.getState().queryExec.get(`${PANEL_B}::${PANE_2}`)).toBeUndefined();
  });

  it('does not steal focus belonging to another tab when a pane closes', () => {
    const s = usePanelStore.getState();
    s.addPanel(makeQueryPanel(PANEL_A));
    s.addPanel(makeQueryPanel(PANEL_B));
    // Tab A is split and focused on its own secondary pane.
    usePanelStore.getState().openPane(PANEL_A, PANE_2);
    // Tab B also has a pane that happens to share the same pane id.
    usePanelStore.getState().openPane(PANEL_B, PANE_2);
    usePanelStore.getState().setActivePanel(PANEL_A);
    usePanelStore.getState().setFocusedPane(PANE_2);

    // Closing tab B's pane must not reset the focus that belongs to tab A.
    usePanelStore.getState().closePane(PANEL_B, PANE_2);

    expect(routedKeyFor(PANEL_A)).toBe(paneKey(PANEL_A, PANE_2));
  });
});
