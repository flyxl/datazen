/**
 * [tester] Every focus-touching action must route through `syncPaneFocus` **and
 * actually drop the entries it should** — pinned by mutation, not by reading.
 *
 * `focusedPaneIdByPanel` has exactly one writer (`syncPaneFocus`, plus the two
 * whole-state literals at module scope and in `reset`). A reader cannot tell from
 * reading the code whether an action actually uses it, so the rule is pinned the
 * only way a store rule can be: by **mutation**.
 *
 * Harness: `scripts/mutation-check-pane-focus.mjs` (`node scripts/mutation-check-pane-focus.mjs`).
 * It reverts each action to its pre-BUG-001 shape — a plain `activePanelId`
 * write, a plain global `focusedPaneId` write, or no focus patch at all — and
 * requires the **pane suite** to go red: the 10 files in that script's `TESTS`
 * list, run together. That is the criterion and the only one — the script never
 * inspects this file on its own, so a mutation only some *other* file in that
 * list can see still counts as caught. Today every one of those mutations happens
 * to turn red here too, but that is a fact about this file's current contents, not
 * the harness's rule (see note 2): the moment one of these guards moves to another
 * file in `TESTS`, "this file goes red" stops being true while the harness keeps
 * passing — which is exactly why the comment may not claim it.
 *
 * Two things this file has to get right, both learned the hard way:
 *
 *  1. **A guard is only a guard if the mutation actually reaches it.** A test that
 *     drives an action in a state where the state it maintains was never created
 *     is green under every mutation. `addPanel(A); removePanel(A);` on an unsplit
 *     tab is the textbook case: there is no entry to prune, so a removed prune
 *     changes nothing. Every closing action below therefore removes a tab that
 *     *holds* a focus entry, asserted before the removal.
 *  2. **The surviving mutation count is a measurement, not a promise.** The three
 *     holes this file used to carry (`removePanel`, `closePanelsToTheRight`, and
 *     `closePane`'s `clearsFocus` predicate) were each 0-red here while still
 *     being caught by other files in the suite. See the header of
 *     `scripts/mutation-check-pane-focus.mjs` for the scope it measures.
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
    'cfg-2': {
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
const C = 'panel-q-3';
const TBL = 'panel-tbl-1';
const P2 = 'p2';
const P3 = 'p3';

describe('[tester] every focus-touching action goes through syncPaneFocus', () => {
  let usePanelStore: typeof import('../panelStore').usePanelStore;
  let paneKey: typeof import('../paneKeys').paneKey;
  let resolveFocusedPaneId: typeof import('../paneKeys').resolveFocusedPaneId;

  function makeQueryPanel(
    id: string,
    connectionId = 'cfg-1',
    database = 'app',
  ): import('../panelStore').QueryPanel {
    return {
      type: 'query',
      id,
      title: 'Q',
      connectionId,
      connectionName: 'TestDB',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql',
      database,
      schema: null,
    };
  }

  function makeTablePanel(id: string, tableName: string, connectionId = 'cfg-1') {
    return {
      type: 'table' as const,
      id,
      tableName,
      connectionId,
      connectionName: 'TestDB',
      dbSessionId: 'sess-1',
      databaseType: 'postgresql' as const,
      database: 'app',
      tableSchema: 'public',
      subTab: 'data' as const,
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
    resolveFocusedPaneId = keys.resolveFocusedPaneId;
    usePanelStore.setState({
      panels: [],
      activePanelId: null,
      queryExec: new Map(),
      focusedPaneId: null,
      focusedPaneIdByPanel: {},
    });
  });

  it('addPanel: a newly activated tab must not inherit the tab being left focus', () => {
    st().addPanel(makeQueryPanel(A));
    st().openPane(A, P2);
    expect(st().focusedPaneId).toBe(P2);

    // The everyday P2 action: while tab A is split, the user opens a NEW query
    // tab. It is not split, so it must show its own pane — not `panel-q-2::p2`.
    st().addPanel(makeQueryPanel(B));
    expect(st().activePanelId).toBe(B);
    expect(st().focusedPaneId).toBeNull();
    expect(st().focusedPaneIdByPanel[B]).toBeUndefined();
    expect(route(B)).toBe(B);
    // And the split tab kept its own focus for when the user comes back.
    expect(st().focusedPaneIdByPanel[A]).toBe(P2);
    st().setActivePanel(A);
    expect(st().focusedPaneId).toBe(P2);
  });

  it('addPanel: a non-activated tab addition leaves the on-screen focus alone', () => {
    st().addPanel(makeQueryPanel(A));
    st().openPane(A, P2);
    st().addPanel(makeQueryPanel(B, 'cfg-1', 'app'), false);
    expect(st().activePanelId).toBe(A);
    expect(st().focusedPaneId).toBe(P2);
    expect(route(A)).toBe(`${A}::${P2}`);
  });

  it('removePanel: drops the focus entry of the tab it removes', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().addPanel(makeQueryPanel(C, 'cfg-2'));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().openPane(C, P2);
    st().setActivePanel(B);
    expect(Object.keys(st().focusedPaneIdByPanel).sort()).toEqual([A, B, C].sort());

    st().removePanel(A);

    // A *held* a focus entry, so the prune pass has something to do here. That
    // precondition is the whole point: `addPanel(A); removePanel(A);` on an
    // unsplit tab prunes nothing, so a missing prune is invisible to it — which
    // is how `removePanel` stayed unguarded.
    expect(Object.keys(st().focusedPaneIdByPanel)).not.toContain(A);
    // Asserted by equality, not by presence: "A is absent" alone would also pass
    // a prune that wrongly dropped the surviving tabs' entries.
    expect(st().focusedPaneIdByPanel).toEqual({ [B]: P2, [C]: P2 });
    expect(st().panels.map((p) => p.id)).toEqual([B, C]);
    expect(st().activePanelId).toBe(B);
    expect(st().focusedPaneId).toBe(P2);
  });

  it('removeAllForConnection: drops the focus of every tab it removes', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().addPanel(makeQueryPanel(C, 'cfg-2'));
    st().openPane(A, P2);
    st().openPane(C, P2);
    st().openPane(B, P2);
    expect(Object.keys(st().focusedPaneIdByPanel).sort()).toEqual([A, B, C].sort());

    st().removeAllForConnection('cfg-1');
    // Only the surviving tab keeps an entry.
    expect(st().focusedPaneIdByPanel).toEqual({ [C]: P2 });
    expect(st().panels.map((p) => p.id)).toEqual([C]);
    expect(st().activePanelId).toBe(C);
    expect(st().focusedPaneId).toBe(P2);
  });

  it('removePanelsForRelation: drops the focus of the tabs it removes', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeTablePanel(TBL, 'users'));
    st().openPane(A, P2);
    st().setFocusedPane(P2, TBL);
    expect(Object.keys(st().focusedPaneIdByPanel).sort()).toEqual([A, TBL].sort());

    st().removePanelsForRelation('cfg-1', 'users');
    // A table tab owns no exec state, so it can be "split" in focus terms only;
    // what matters is that its entry does not outlive it.
    expect(Object.keys(st().focusedPaneIdByPanel)).not.toContain(TBL);
    expect(st().panels.map((p) => p.id)).toEqual([A]);
    expect(st().focusedPaneIdByPanel[A]).toBe(P2);
    expect(st().focusedPaneId).toBe(P2);
  });

  it('removePanelsForDatabase: drops the focus of the tabs it removes', () => {
    st().addPanel(makeQueryPanel(A, 'cfg-1', 'app'));
    st().addPanel(makeQueryPanel(B, 'cfg-1', 'other'));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().setActivePanel(B);
    expect(Object.keys(st().focusedPaneIdByPanel).sort()).toEqual([A, B].sort());

    st().removePanelsForDatabase('cfg-1', 'app');
    expect(st().focusedPaneIdByPanel).toEqual({ [B]: P2 });
    expect(st().panels.map((p) => p.id)).toEqual([B]);
    expect(st().activePanelId).toBe(B);
    expect(st().focusedPaneId).toBe(P2);
  });

  it('closeOtherPanels: drops the focus of every tab it closes', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().addPanel(makeQueryPanel(C));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().openPane(C, P2);
    st().setActivePanel(B);
    expect(Object.keys(st().focusedPaneIdByPanel).sort()).toEqual([A, B, C].sort());

    st().closeOtherPanels(C);
    expect(st().panels.map((p) => p.id)).toEqual([C]);
    expect(st().focusedPaneIdByPanel).toEqual({ [C]: P2 });
    expect(st().focusedPaneId).toBe(P2);
  });

  it('closeAllPanels: clears the whole focus map and the mirror', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().openPane(A, P2);
    st().openPane(B, P2);
    expect(st().focusedPaneId).toBe(P2);

    st().closeAllPanels();
    expect(st().panels).toEqual([]);
    expect(st().activePanelId).toBeNull();
    expect(st().focusedPaneIdByPanel).toEqual({});
    expect(st().focusedPaneId).toBeNull();
  });

  it('closePanelsToTheRight: drops the focus of every tab it closes', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().addPanel(makeQueryPanel(C, 'cfg-2'));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().openPane(C, P2);
    st().setActivePanel(A);
    expect(Object.keys(st().focusedPaneIdByPanel).sort()).toEqual([A, B, C].sort());

    st().closePanelsToTheRight(A);

    expect(st().panels.map((p) => p.id)).toEqual([A]);
    // B and C each held an entry and both tabs are gone, so both entries must be
    // gone too — the tab on the left of the cut keeps its own.
    expect(st().focusedPaneIdByPanel).toEqual({ [A]: P2 });
    expect(st().activePanelId).toBe(A);
    expect(st().focusedPaneId).toBe(P2);
  });

  it('closePanelsToTheLeft: drops the focus of the tabs it closes', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().addPanel(makeQueryPanel(C));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().openPane(C, P2);
    st().setActivePanel(B);
    expect(Object.keys(st().focusedPaneIdByPanel).sort()).toEqual([A, B, C].sort());

    st().closePanelsToTheLeft(C);
    expect(st().panels.map((p) => p.id)).toEqual([C]);
    expect(st().focusedPaneIdByPanel).toEqual({ [C]: P2 });
    expect(st().activePanelId).toBe(C);
    expect(st().focusedPaneId).toBe(P2);
  });

  it('closePane: the closed pane loses the focus of its own tab, and nothing else', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().updateSql(A, 'SELECT 1');
    st().updateSql(A, 'SELECT 2', P2);
    st().setActivePanel(A);
    expect(st().focusedPaneIdByPanel).toEqual({ [A]: P2, [B]: P2 });
    expect(st().focusedPaneId).toBe(P2);
    expect(st().queryExec.has(`${A}::${P2}`)).toBe(true);

    st().closePane(A, P2);

    // `clearsFocus` is what decides whether the closed pane's tab drops its
    // entry. Force it to `false` and the entry survives while the pane's exec
    // entry does not — the focus ends up naming a pane that no longer exists,
    // so the editor routes to a key nothing can resolve.
    expect(st().queryExec.has(`${A}::${P2}`)).toBe(false);
    expect(st().focusedPaneIdByPanel).toEqual({ [B]: P2 });
    expect(st().focusedPaneId).toBeNull();

    // Behavioural form of the same claim: the pane key the editor would route to
    // is always a key that still has exec state. (B's entry is untouched by a
    // pane close in A — that is the "and nothing else" half.)
    expect(st().queryExec.has(route(A))).toBe(true);
    st().setActivePanel(B);
    expect(st().focusedPaneId).toBe(P2);
    expect(st().queryExec.has(route(B))).toBe(true);
  });

  it('closePane: closing an *unfocused* pane leaves its tab focus where it was', () => {
    st().addPanel(makeQueryPanel(A));
    st().openPane(A, P2);
    st().openPane(A, P3);
    st().openPane(A, P2);
    st().setActivePanel(A);
    expect(st().focusedPaneIdByPanel).toEqual({ [A]: P2 });
    expect(st().queryExec.has(`${A}::${P3}`)).toBe(true);

    st().closePane(A, P3);

    // `clearsFocus` must be false here: p3 is not the pane that held the focus.
    // Always clearing would drop the entry and hand the tab back to its default
    // pane even though the pane the user was working in is still open.
    expect(st().focusedPaneIdByPanel).toEqual({ [A]: P2 });
    expect(st().focusedPaneId).toBe(P2);
    expect(st().queryExec.has(`${A}::${P2}`)).toBe(true);
    expect(st().queryExec.has(route(A))).toBe(true);
  });

  it('reset: clears the focus map, not just the mirror', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().openPane(A, P2);
    st().openPane(B, P2);
    expect(st().focusedPaneIdByPanel).toEqual({ [A]: P2, [B]: P2 });

    st().reset();
    expect(st().panels).toEqual([]);
    expect(st().focusedPaneIdByPanel).toEqual({});
    expect(st().focusedPaneId).toBeNull();
  });

  it('invariant: the mirror is correct on EVERY state emission, not just after it settles', () => {
    // The view subscribes to the mirror (`ContentView.tsx`: `focusedPaneId`).
    // Point-in-time checks cannot see a drift that exists for one `set()` and is
    // corrected by the next, and a React render can land exactly there. Watching
    // every emission closes that gap: the map and the mirror are written in the
    // same `set()`, so the invariant must hold at all times.
    const violations: string[] = [];
    const seen: Array<() => void> = [];
    const check = (s: ReturnType<typeof usePanelStore.getState>) => {
      const expected = s.activePanelId ? (s.focusedPaneIdByPanel[s.activePanelId] ?? null) : null;
      if (s.focusedPaneId !== expected) {
        violations.push(
          `active=${s.activePanelId} mirror=${JSON.stringify(s.focusedPaneId)} ` +
            `expected=${JSON.stringify(expected)} map=${JSON.stringify(s.focusedPaneIdByPanel)}`,
        );
      }
    };

    usePanelStore.subscribe((s) => {
      check(s);
      seen.push(() => undefined);
    });

    st().addPanel(makeQueryPanel(A));
    st().openPane(A, P2);
    st().addPanel(makeQueryPanel(B));
    st().addPanel(makeQueryPanel(C, 'cfg-2'));
    st().openPane(C, P2);
    st().setFocusedPane(P2, A);
    st().setActivePanel(B);
    st().setActivePanel(A);
    st().setFocusedPane(null);
    st().closePane(A, P2);
    st().openPane(A, P2);
    st().closePane(A, P2);
    st().closeOtherPanels(B);
    st().closeAllPanels();
    // A must hold a focus entry before it is removed, or the removal prunes
    // nothing and the sequence below cannot tell a missing prune from a correct
    // one. The mirror invariant alone also cannot see it (a stale entry for a
    // closed tab still satisfies "mirror == map[active]"), so the map is checked
    // directly here.
    st().addPanel(makeQueryPanel(A));
    st().openPane(A, P2);
    st().removePanel(A);
    expect(st().focusedPaneIdByPanel).toEqual({});
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().setActivePanel(A);
    st().closePanelsToTheRight(A);
    expect(st().focusedPaneIdByPanel).toEqual({ [A]: P2 });
    st().reset();

    expect(violations).toEqual([]);
    expect(seen.length).toBeGreaterThanOrEqual(18);
  });

  it('invariant: no focus entry ever names a tab that is not open', () => {
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().addPanel(makeQueryPanel(C, 'cfg-2'));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().openPane(C, P2);

    const openTabs = () => new Set(st().panels.map((p) => p.id));
    const orphanEntries = () =>
      Object.keys(st().focusedPaneIdByPanel).filter((id) => !openTabs().has(id));

    expect(orphanEntries()).toEqual([]);
    st().removeAllForConnection('cfg-1');
    expect(orphanEntries()).toEqual([]);
    st().closeAllPanels();
    expect(orphanEntries()).toEqual([]);
    st().reset();
    expect(orphanEntries()).toEqual([]);

    // Every remaining closing action, each with the closed tab(s) actually
    // holding an entry — an orphan can only be *produced* by a tab that had one.
    st().addPanel(makeQueryPanel(A));
    st().addPanel(makeQueryPanel(B));
    st().addPanel(makeQueryPanel(C, 'cfg-2'));
    st().openPane(A, P2);
    st().openPane(B, P2);
    st().openPane(C, P2);

    st().removePanel(A);
    expect(orphanEntries()).toEqual([]);
    st().closePanelsToTheRight(B);
    expect(orphanEntries()).toEqual([]);
    st().closePanelsToTheLeft(C);
    expect(orphanEntries()).toEqual([]);
    st().closeOtherPanels(C);
    expect(orphanEntries()).toEqual([]);
  });
});
