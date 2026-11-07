/**
 * [tester] P2 contract probes: the focus model still leaves doors open that only
 * P2's UI can walk through.
 *
 * `paneFocusScope.tester.test.ts` pins the *invariant* "a tab routes only to its
 * own panes". The P2 sequence test pins the *sequence*. This file probes the
 * store actions P2 will call, for the cases neither of those reaches:
 *
 *   - `setFocusedPane` with a pane the tab never opened (P2 will call it from a
 *     pane click handler; a click on a not-yet-seeded pane is a UI bug, but the
 *     store happily records it and the orphan entry reappears)
 *   - `openPane` / `closePane` / `setFocusedPane` aimed at a **background** tab
 *   - whether the tab a split happens on is forced active
 *
 * These are recorded as observations, not as gates on today's code: no production
 * call site exists yet (see `paneRealSequence.tester.test.ts` header).
 */

import { describe, expect, it, beforeEach, vi } from 'vitest';

vi.mock('../../locales/t', () => ({
  t: (key: string) => key,
}));

const mockGetQueryHistoryPage = vi.fn().mockResolvedValue({ entries: [], total: 0 });
const mockCancelQuery = vi.fn().mockResolvedValue(undefined);
const mockExecuteQueryStream = vi.fn().mockResolvedValue(undefined);

const activeConnectionState = {
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

const A = 'panel-q-1';
const B = 'panel-q-2';
const P2 = 'p2';
const P3 = 'p3';

describe('[tester] P2 contract probes on the fixed build', () => {
  let usePanelStore: typeof import('../panelStore').usePanelStore;
  let paneKey: typeof import('../paneKeys').paneKey;
  let paneArgs: typeof import('../paneKeys').paneArgs;
  let resolveFocusedPaneId: typeof import('../paneKeys').resolveFocusedPaneId;

  function makeQueryPanel(id: string): import('../panelStore').QueryPanel {
    return {
      type: 'query',
      id,
      title: 'Q',
      connectionId: 'cfg-1',
      connectionName: 'TestDB',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      database: 'app',
      schema: null,
    };
  }

  const st = () => usePanelStore.getState();
  const route = (panelId: string) => paneKey(panelId, resolveFocusedPaneId(st().focusedPaneId));

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    usePanelStore = (await import('../panelStore')).usePanelStore;
    const keys = await import('../paneKeys');
    paneKey = keys.paneKey;
    paneArgs = keys.paneArgs;
    resolveFocusedPaneId = keys.resolveFocusedPaneId;
    usePanelStore.setState({
      panels: [],
      activePanelId: null,
      queryExec: new Map(),
      focusedPaneId: null,
      focusedPaneIdByPanel: {},
    });
  });

  it('PROBE A: setFocusedPane on a pane the tab never opened still records it', () => {
    st().addPanel(makeQueryPanel(A));
    st().openPane(A, P2);
    st().setFocusedPane(P3);

    // The store records a pane that has no exec entry at all.
    expect(st().focusedPaneIdByPanel[A]).toBe(P3);
    expect(st().queryExec.has(`${A}::${P3}`)).toBe(false);
    expect(route(A)).toBe(`${A}::${P3}`);

    // The very next keystroke through the production path re-creates the orphan
    // entry BUG-001 was about — so the guard belongs in P2's click handler (or
    // here), not in the focus bookkeeping.
    st().updateSql(A, 'SELECT typed', ...paneArgs(resolveFocusedPaneId(st().focusedPaneId)));
    expect(st().queryExec.has(`${A}::${P3}`)).toBe(true);
    expect([...st().queryExec.keys()].sort()).toEqual([A, `${A}::${P2}`, `${A}::${P3}`].sort());
  });

  it('PROBE B: setFocusedPane aimed at a background tab does not steal the screen', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().openPane(A, P2);
    // User is looking at tab B; tab A is split and focused on p2.
    expect(st().activePanelId).toBe(A);

    st().setActivePanel(B);
    expect(st().focusedPaneId).toBeNull();

    // Focusing a pane of the background tab A must not change what B routes to.
    st().setFocusedPane(P2, A);
    expect(st().activePanelId).toBe(B);
    expect(st().focusedPaneId).toBeNull();
    expect(route(B)).toBe(B);
    expect(st().focusedPaneIdByPanel[A]).toBe(P2);

    // …and it survives the user going back to A.
    st().setActivePanel(A);
    expect(st().focusedPaneId).toBe(P2);
  });

  it('PROBE C: closePane on a background tab does not move the on-screen focus', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().setActivePanel(A);
    st().setFocusedPane(P2);
    expect(st().focusedPaneId).toBe(P2);

    // Tab B is in the background and its focused pane closes.
    st().closePane(B, P2);
    expect(st().activePanelId).toBe(A);
    expect(st().focusedPaneId).toBe(P2);
    expect(route(A)).toBe(`${A}::${P2}`);
    // B fell back to its own default pane.
    expect(st().focusedPaneIdByPanel[B]).toBeUndefined();
  });

  it('PROBE D: openPane forces the split tab active — a behavior change vs pre-fix', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    expect(st().activePanelId).toBe(B);

    // P2 splitting a *background* tab yanks the user to it. Pre-fix, openPane left
    // activePanelId alone. Unreachable today (no call sites), but P2 must know.
    st().openPane(A, P2);
    expect(st().activePanelId).toBe(A);
    expect(st().focusedPaneId).toBe(P2);
  });

  it('PROBE E: reopening an existing pane re-focuses it and keeps its content', () => {
    st().addPanel(makeQueryPanel(A));
    st().openPane(A, P2);
    st().updateSql(A, 'SELECT in-p2', P2);
    st().setFocusedPane(null);
    expect(st().focusedPaneIdByPanel[A]).toBeUndefined();

    st().openPane(A, P2);
    expect(st().focusedPaneId).toBe(P2);
    expect(st().queryExec.get(`${A}::${P2}`)?.sql).toBe('SELECT in-p2');
    expect([...st().queryExec.keys()].sort()).toEqual([A, `${A}::${P2}`].sort());
  });

  it('PROBE G: setFocusedPane on a tab that is not open is self-healing', () => {
    st().addPanel(makeQueryPanel(A));
    st().openPane(A, P2);

    // `setFocusedPane` validates that a target tab was named, but not that the
    // tab exists. This records an entry for a tab that is not open.
    st().setFocusedPane(P2, 'panel-does-not-exist');
    expect(st().focusedPaneIdByPanel['panel-does-not-exist']).toBe(P2);

    // It is transient: the next focus-touching action prunes it, because the
    // prune pass drops every entry whose tab is gone. So the window of exposure
    // is one action, and nothing on screen reads a non-active tab's entry.
    st().setActivePanel(A);
    expect(st().focusedPaneIdByPanel['panel-does-not-exist']).toBeUndefined();
    expect(st().focusedPaneId).toBe(P2);
  });

  it('PROBE F: closing the default pane leaves the tab with no entry for its own pane', () => {
    st().addPanel(makeQueryPanel(A));
    st().updateSql(A, 'SELECT main', undefined);
    st().openPane(A, P2);

    // Closing a tab's *default* pane is allowed by the store.
    st().closePane(A, 'main');
    expect(st().queryExec.has(A)).toBe(false);
    expect(st().focusedPaneId).toBe(P2);
    // Typing still works: the exec entry is re-seeded by patchExec on demand.
    st().updateSql(A, 'SELECT after', ...paneArgs(resolveFocusedPaneId(st().focusedPaneId)));
    expect(st().queryExec.has(`${A}::${P2}`)).toBe(true);
  });
});
