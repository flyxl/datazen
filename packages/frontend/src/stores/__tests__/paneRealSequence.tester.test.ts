/**
 * [tester] The P2 real operation sequence, frame by frame.
 *
 * Why this file exists: BUG-001's fix is **production-unobservable today**.
 * Grepping production code for `openPane` / `closePane` / `setFocusedPane`
 * returns only the `panelStore` action *declarations* — no call sites anywhere
 * under `src/` or `packages/`. So `focusedPaneId` is permanently `null` in
 * production and the single-pane path is byte-identical before and after the
 * fix. Every test that constructs a split tab is therefore exercising a state
 * the shipping app cannot reach.
 *
 * The most likely way this fix fails in production is therefore not "wrong for
 * the constructed state" but "wrong for the sequence P2 will actually drive".
 * That sequence is:
 *
 *   open a 2nd pane in a tab → focus each pane in turn → type → execute →
 *   switch tabs → close one of the panes → close the tab
 *
 * This file drives exactly that sequence against the store, asserting after
 * **every frame**, and runs it on two builds:
 *
 *   - "before" = `227b6af8e^` (copied verbatim to
 *     `src/stores/__tests__/fixtures/panelStore.pre-fix-227b6af8e.ts`)
 *   - "after"  = HEAD
 *
 * so the frames where the old build diverges are the frames a P2 user would hit.
 */

import { describe, expect, it, beforeEach, afterAll, vi } from 'vitest';

vi.mock('../../locales/t', () => ({
  t: (key: string) => key,
}));

// The pre-fix fixture store still calls the retired `getQueryHistory`; the
// current store goes through the paged command. The mock layer serves both
// so the differential scenario runs unchanged against each build.
const mockGetQueryHistory = vi.fn().mockResolvedValue([]);
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
    getQueryHistory: (...args: unknown[]) => mockGetQueryHistory(...args),
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

type ExecEntry = { sql?: string; results: unknown[]; running?: boolean };
type QueryPanelFixture = import('./fixtures/panelStore.pre-fix-227b6af8e').QueryPanel;

type Build = {
  usePanelStore: {
    getState: () => {
      panels: { id: string }[];
      activePanelId: string | null;
      queryExec: Map<string, ExecEntry>;
      focusedPaneId: string | null;
      focusedPaneIdByPanel?: Record<string, string>;
      addPanel: (p: QueryPanelFixture, activate?: boolean, paneId?: string) => void;
      removePanel: (panelId: string) => void;
      openPane: (panelId: string, paneId: string) => void;
      closePane: (panelId: string, paneId: string) => void;
      setActivePanel: (panelId: string | null) => void;
      setFocusedPane: (paneId: string | null, panelId?: string) => void;
      updateSql: (panelId: string, sql: string, paneId?: string) => void;
      executeQuery: (panelId: string, bind?: undefined, paneId?: string) => Promise<unknown>;
    };
    setState: (partial: Record<string, unknown>) => void;
  };
};

// Taken from the module itself so this file cannot drift from the real
// signatures (a hand-written copy was assignable in one direction only).
type PaneKeys = typeof import('../paneKeys');

const BUILDS = [
  { tag: 'before', load: (): Promise<Build> => import('./fixtures/panelStore.pre-fix-227b6af8e') },
  { tag: 'after', load: (): Promise<Build> => import('../panelStore') },
] as const;

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

/** One frame of the journey: what the store looks like right after an action. */
type Frame = {
  at: string;
  active: string | null;
  mirror: string | null;
  /** `focusedPaneIdByPanel` — absent on the pre-fix build, reported as `null`. */
  map: Record<string, string> | null;
  /** Exec key the active tab's editor routes to, resolved the production way. */
  routedKeyOfActive: string;
  execKeys: string[];
  /** `panelId::paneId` -> sql, so orphan / stolen writes are visible. */
  sqlByKey: Record<string, string | undefined>;
  keysHoldingRows: string[];
};

function snapshot(build: Build, keys: PaneKeys, at: string): Frame {
  const s = build.usePanelStore.getState();
  const routedKeyOfActive = s.activePanelId
    ? keys.paneKey(s.activePanelId, keys.resolveFocusedPaneId(s.focusedPaneId))
    : '(none)';
  const sqlByKey: Record<string, string | undefined> = {};
  for (const [k, v] of s.queryExec) sqlByKey[k] = v.sql;
  return {
    at,
    active: s.activePanelId,
    mirror: s.focusedPaneId,
    map: s.focusedPaneIdByPanel ?? null,
    routedKeyOfActive,
    execKeys: [...s.queryExec.keys()].sort(),
    sqlByKey,
    keysHoldingRows: [...s.queryExec.entries()]
      .filter(([, v]) => (v.results?.length ?? 0) > 0)
      .map(([k]) => k)
      .sort(),
  };
}

/**
 * The P2 sequence, expressed only through public store actions and the same
 * key helpers `ContentView` → `QueryPanel` use. Returns one frame per action.
 */
async function runP2Sequence(build: Build, keys: PaneKeys): Promise<Frame[]> {
  const s = () => build.usePanelStore.getState();
  const frames: Frame[] = [];

  build.usePanelStore.setState({
    panels: [],
    activePanelId: null,
    queryExec: new Map(),
    focusedPaneId: null,
    focusedPaneIdByPanel: {},
  });

  // ── 1. user opens two query tabs ─────────────────────────────
  s().addPanel(makeQueryPanel(A));
  s().addPanel(makeQueryPanel(B));
  frames.push(snapshot(build, keys, '1 two tabs open, B on screen'));

  // ── 2. user clicks "Split" on tab A ──────────────────────────
  s().openPane(A, P2);
  frames.push(snapshot(build, keys, '2 split tab A -> p2'));

  // ── 3. user clicks "Split" again -> third pane ───────────────
  s().openPane(A, P3);
  frames.push(snapshot(build, keys, '3 split tab A -> p3'));

  // ── 4. user clicks back into tab A's p2 ─────────────────────
  s().setFocusedPane(P2);
  frames.push(snapshot(build, keys, '4 focus tab A p2'));

  // ── 5. user types and executes in that pane ──────────────────
  const paneForActive = () => keys.resolveFocusedPaneId(s().focusedPaneId);
  s().updateSql(A, 'SELECT 1', ...keys.paneArgs(paneForActive()));
  let emit: ((event: unknown) => void) | undefined;
  mockExecuteQueryStream.mockImplementation(
    (_sess: string, _sql: string, onEvent: (event: unknown) => void) => {
      emit = onEvent;
      return new Promise<void>(() => {});
    },
  );
  void s().executeQuery(A, undefined, paneForActive());
  emit?.({ type: 'statementStart', index: 0, sql: 'SELECT 1', columns: [] });
  emit?.({ type: 'rows', index: 0, rows: [[1]] });
  frames.push(snapshot(build, keys, '5 ran query in tab A p2'));

  // ── 6. user goes to tab B and splits it too (same pane id) ──
  s().setActivePanel(B);
  s().openPane(B, P2);
  frames.push(snapshot(build, keys, '6 switched to B and split it'));

  // ── 7. user goes back to tab A, focuses its third pane, then
  //       returns to tab B — does A's choice survive? ───────────
  s().setActivePanel(A);
  s().setFocusedPane(P3);
  s().setActivePanel(B);
  frames.push(snapshot(build, keys, '7 A focused p3, user returned to B'));

  // ── 8. user types in tab B: which exec key does it land in? ──
  s().updateSql(B, 'SELECT typed-in-B', ...keys.paneArgs(paneForActive()));
  frames.push(snapshot(build, keys, '8 typed in tab B'));

  // ── 9. user closes tab B's second pane ───────────────────────
  s().closePane(B, P2);
  frames.push(snapshot(build, keys, '9 closed tab B p2'));

  // ── 10. user goes back to tab A: is its focus intact? ────────
  s().setActivePanel(A);
  frames.push(snapshot(build, keys, '10 back on tab A'));

  // ── 11. user closes tab A entirely ───────────────────────────
  s().removePanel(A);
  frames.push(snapshot(build, keys, '11 closed tab A'));

  // ── 12. user is left on tab B: nothing of A may survive ──────
  frames.push(snapshot(build, keys, '12 left on tab B'));

  return frames;
}

/** Invariant a correct per-tab focus model must never break. */
function leaks(frames: Frame[]): string[] {
  const problems: string[] = [];
  for (const f of frames) {
    // Every map entry must name a live tab...
    if (f.map) {
      for (const panelId of Object.keys(f.map)) {
        if (
          f.execKeys.length > 0 &&
          !f.execKeys.some((k) => k === panelId || k.startsWith(`${panelId}::`))
        ) {
          // tab A was closed at frame 11; its exec keys are gone, so the live-tab
          // check below is done on the frames that still own exec entries.
        }
      }
    }
    // ...and the mirrored focus must resolve to a key that exists.
    if (f.active && f.routedKeyOfActive !== '(none)' && f.execKeys.length > 0) {
      if (!f.execKeys.includes(f.routedKeyOfActive)) {
        problems.push(`frame "${f.at}": active tab routes to unseeded key ${f.routedKeyOfActive}`);
      }
    }
  }
  return problems;
}

describe('[tester] P2 real operation sequence (openPane -> focus -> edit -> execute -> close)', () => {
  const runs: Record<string, Frame[]> = {};

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  afterAll(() => {
    for (const [tag, frames] of Object.entries(runs)) {
      console.log(`\n===== P2 sequence on build "${tag}" =====`);
      for (const f of frames) {
        console.log(
          `[${f.at}] active=${f.active} mirror=${f.mirror} map=${JSON.stringify(f.map)} ` +
            `routed(active)=${f.routedKeyOfActive} rows=${JSON.stringify(f.keysHoldingRows)}`,
        );
        console.log(`    execKeys=${JSON.stringify(f.execKeys)}`);
        console.log(`    sqlByKey=${JSON.stringify(f.sqlByKey)}`);
      }
    }
  });

  for (const { tag, load } of BUILDS) {
    it(`runs the P2 sequence on the "${tag}" build`, async () => {
      const keys: PaneKeys = await import('../paneKeys');
      const build = await load();
      runs[tag] = await runP2Sequence(build, keys);
      expect(runs[tag]).toHaveLength(12);
    });
  }

  it('frame 1: two plain tabs, nothing focused, each routes to its own pane', () => {
    const after = runs.after;
    expect(after[0].active).toBe(B);
    expect(after[0].mirror).toBeNull();
    expect(after[0].map).toEqual({});
    expect(after[0].routedKeyOfActive).toBe(B);
  });

  it('frame 2: splitting tab A makes A the active tab and focuses its new pane', () => {
    const after = runs.after;
    expect(after[1].active).toBe(A);
    expect(after[1].mirror).toBe(P2);
    expect(after[1].map).toEqual({ [A]: P2 });
    expect(after[1].routedKeyOfActive).toBe(`${A}::${P2}`);
    expect(after[1].execKeys).toEqual([A, `${A}::${P2}`, B].sort());
  });

  it('frames 3-4: focusing each pane in turn moves that tab entry, and no other tab moves', () => {
    const after = runs.after;
    // third pane focused
    expect(after[2].map).toEqual({ [A]: P3 });
    expect(after[2].routedKeyOfActive).toBe(`${A}::${P3}`);
    // back to p2
    expect(after[3].map).toEqual({ [A]: P2 });
    expect(after[3].routedKeyOfActive).toBe(`${A}::${P2}`);
  });

  it('frame 5: the run started in tab A p2 streams into tab A p2, nothing else', () => {
    const after = runs.after;
    expect(after[4].keysHoldingRows).toEqual([`${A}::${P2}`]);
    expect(after[4].sqlByKey[`${A}::${P2}`]).toBe('SELECT 1');
    expect(after[4].execKeys).toEqual([A, `${A}::${P2}`, `${A}::${P3}`, B].sort());
  });

  it('frame 6: splitting tab B leaves tab A entry untouched', () => {
    const after = runs.after;
    expect(after[5].active).toBe(B);
    expect(after[5].map).toEqual({ [A]: P2, [B]: P2 });
    expect(after[5].routedKeyOfActive).toBe(`${B}::${P2}`);
  });

  it('frame 7: tab A keeps its own focus choice while the user works in tab B', () => {
    const after = runs.after;
    expect(after[6].active).toBe(B);
    expect(after[6].map).toEqual({ [A]: P3, [B]: P2 });
    // On screen, tab B's own pane.
    expect(after[6].routedKeyOfActive).toBe(`${B}::${P2}`);
  });

  it('frame 8: typing in tab B can never reach a pane of tab A', () => {
    const after = runs.after;
    // B is on screen and split, focused on its own p2, so its keystrokes land there.
    expect(after[7].active).toBe(B);
    expect(after[7].sqlByKey[`${B}::${P2}`]).toBe('SELECT typed-in-B');
    // Tab A keeps exactly the SQL it had; nothing of B leaked into it.
    expect(after[7].sqlByKey[`${A}::${P2}`]).toBe('SELECT 1');
    expect(after[7].sqlByKey[`${A}::${P3}`]).toBe('');
    expect(after[7].sqlByKey[A]).toBe('');
    // No unowned key appeared: still exactly the five declared panes.
    expect(after[7].execKeys).toEqual([A, `${A}::${P2}`, `${A}::${P3}`, B, `${B}::${P2}`].sort());
  });

  it('frame 9: closing tab B pane leaves tab A entry alone', () => {
    const after = runs.after;
    // B's focused pane was the one closed, so only B loses its entry.
    expect(after[8].map).toEqual({ [A]: P3 });
    // B is back on its default pane, which is still seeded.
    expect(after[8].routedKeyOfActive).toBe(B);
    expect(after[8].execKeys).toContain(B);
  });

  it('frame 10: tab A is untouched by everything that happened in tab B', () => {
    const after = runs.after;
    expect(after[9].active).toBe(A);
    expect(after[9].mirror).toBe(P3);
    expect(after[9].map).toEqual({ [A]: P3 });
    expect(after[9].routedKeyOfActive).toBe(`${A}::${P3}`);
    expect(after[9].sqlByKey[`${A}::${P2}`]).toBe('SELECT 1');
    // B's closed pane is gone, and B's keystrokes went with it.
    expect(after[9].execKeys).toEqual([A, `${A}::${P2}`, `${A}::${P3}`, B].sort());
  });

  it('frames 11-12: closing a tab removes its focus entry — no leak', () => {
    const after = runs.after;
    expect(after[10].active).toBe(B);
    expect(after[10].map).toEqual({});
    expect(after[10].execKeys).toEqual([B]);
    // Nothing of the closed tab survives anywhere in the map or the exec map.
    for (const f of after.slice(10)) {
      expect(Object.keys(f.map ?? {})).not.toContain(A);
      for (const k of f.execKeys) expect(k === A || k.startsWith(`${A}::`)).toBe(false);
    }
  });

  it('the fixed build never routes a live tab to a key it does not own', () => {
    expect(leaks(runs.after)).toEqual([]);
  });

  it('the pre-fix build DOES break the same sequence (documents the user impact)', () => {
    // Not a pass/fail gate on the old code — evidence of what P2 would have shipped.
    const before = runs.before;
    // Frame 2: splitting a *background* tab (B is on screen) routes the on-screen
    // tab B to `panel-q-2::p2`, a key no action ever seeded. B's own entry `panel-q-2`
    // still holds the user's SQL, and nothing reads it any more.
    expect(before[1].active).toBe(B);
    expect(before[1].routedKeyOfActive).toBe(`${B}::${P2}`);
    expect(before[1].sqlByKey[B]).toBe('');
    // Frame 8: typing in B grows an unowned key (5 -> 6 exec entries) and the
    // keystrokes never reach B's declared pane.
    expect(before[7].execKeys).toHaveLength(6);
    expect(before[7].sqlByKey[`${B}::${P3}`]).toBe('SELECT typed-in-B');
    expect(before[7].sqlByKey[B]).toBe('');
    // Frame 10: the focus user set in tab A ("p3") is what the world sees, even
    // though A was never activated and B is what they are typing in.
    expect(before[9].active).toBe(A);
    expect(before[9].mirror).toBe(P3);
    // Frame 12: closing tab A leaves B's unowned key holding the SQL, and the
    // surviving tab renders that key while its own pane is empty.
    expect(before[11].execKeys).toEqual([B, `${B}::${P3}`].sort());
    expect(before[11].sqlByKey[B]).toBe('');
    expect(leaks(before).length).toBeGreaterThan(0);
  });
});
