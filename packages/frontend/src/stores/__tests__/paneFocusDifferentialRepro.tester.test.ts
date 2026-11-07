/**
 * [tester] Independent differential reproduction of BUG-001.
 *
 * The original report asserts three user-visible consequences of a single global
 * `focusedPaneId`. I do not take those numbers on faith: this file runs **one**
 * scenario body against two builds of `panelStore` and asserts that the
 * observable outcome differs.
 *
 *   - "before" = the file as of `227b6af8e^` (fix commit's parent), copied
 *     verbatim to
 *     `src/stores/__tests__/fixtures/panelStore.pre-fix-227b6af8e.ts`
 *   - "after"  = `src/stores/panelStore.ts` at HEAD (`227b6af8e`)
 *
 * The scenario body is written against the *public store surface only*
 * (`addPanel` / `openPane` / `closePane` / `setActivePanel` / `setFocusedPane` /
 * `updateSql` / `executeQuery`) plus the same key helpers production uses, and it
 * resolves the routed key the way `ContentView` → `QueryPanel` resolves it.
 *
 * Reproduce the "before" build with:
 *   see the generated banner at the top of
 *   src/stores/__tests__/fixtures/panelStore.pre-fix-227b6af8e.ts
 */

import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';
import { describe, expect, it, beforeEach, afterAll, vi } from 'vitest';

// The fixture is a snapshot of the store as of this commit's parent. These three
// constants are what PROVENANCE 2 checks; they are declared here, not read out
// of the fixture, so a rewritten banner cannot make the test agree with itself.
const FIXTURE_BASENAME = 'panelStore.pre-fix-227b6af8e';
const FIXTURE_MARKER = '// ─── END GENERATED BANNER ───';
const FIXTURE_SOURCE_COMMIT = '227b6af8e';
// form-split F6-c1 note: the tree has since moved to packages/frontend/src,
// but provenance names the path AS IT WAS at the fixture commit — git show
// resolves against that commit's own tree.
const FIXTURE_SOURCE_PATH = 'src/stores/panelStore.ts';
// NOT import.meta.url: under the jsdom environment that is an http:// URL and
// fileURLToPath() throws. vitest runs with the repo root as cwd.
const FIXTURE_PATH = resolve(
  process.cwd(),
  'packages/frontend/src/stores/__tests__/fixtures',
  `${FIXTURE_BASENAME}.ts`,
);
const STORES_ROOT = resolve(process.cwd(), 'packages/frontend/src/stores');

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

// The historical panelStore fixture predates the required connection owner
// argument. Supply the fixture's sole owner when it calls the shared current
// query action so the differential still exercises the old pane routing logic.
vi.mock('../queryExecActions', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../queryExecActions')>();
  return {
    ...actual,
    runStreamingQuery: (
      paneKey: string,
      dbSessionId: string,
      sql: string,
      getExec: Parameters<typeof actual.runStreamingQuery>[3],
      setExec: Parameters<typeof actual.runStreamingQuery>[4],
      database?: string | null,
      schema?: string | null,
      params?: Parameters<typeof actual.runStreamingQuery>[7],
      connectionId?: string,
    ) =>
      actual.runStreamingQuery(
        paneKey,
        dbSessionId,
        sql,
        getExec,
        setExec,
        database,
        schema,
        params,
        connectionId ?? 'cfg-1',
      ),
  };
});

vi.mock('../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: {
    getState: () => activeConnectionState,
  },
}));

const A = 'panel-q-1';
const B = 'panel-q-2';
const P2 = 'p2';

/** Verbatim-copied "before" build, so its `QueryPanel` is the pre-fix one. */
type QueryPanelFixture = import('./fixtures/panelStore.pre-fix-227b6af8e').QueryPanel;
type ExecEntry = { sql?: string; results: unknown[] };

/** The slice of a store build this file needs; identical in both revisions. */
type Build = {
  usePanelStore: {
    getState: () => {
      panels: { id: string }[];
      activePanelId: string | null;
      queryExec: Map<string, ExecEntry>;
      focusedPaneId: string | null;
      focusedPaneIdByPanel?: Record<string, string>;
      addPanel: (p: QueryPanelFixture, activate?: boolean, paneId?: string) => void;
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

/** Everything the three scenarios observe, measured against one build. */
type Observation = {
  // consequence 1 — which exec key the *unsplit* sibling tab routes to
  ghostRoutedKey: string;
  ghostRoutedSql: string | undefined;
  realTabSql: string;
  // consequence 2 — orphan exec entry + where streamed rows land
  execKeysAfterTyping: string[];
  orphanKeySql: string | undefined;
  keysHoldingRows: string[];
  // consequence 3 — tab A's routed key before/after closing tab B's pane
  tabARouteBefore: string;
  tabARouteAfter: string;
  tabAPaneStillOpen: boolean;
};

async function runScenarios(build: Build, keys: PaneKeys): Promise<Observation> {
  const { usePanelStore } = build;
  const st = () => usePanelStore.getState();

  usePanelStore.setState({
    panels: [],
    activePanelId: null,
    queryExec: new Map(),
    focusedPaneId: null,
    focusedPaneIdByPanel: {},
  });

  // ── consequence 1: split tab A, then look at the unsplit sibling tab B ──
  st().addPanel(makeQueryPanel(A));
  st().addPanel(makeQueryPanel(B));
  st().updateSql(B, 'SELECT typed-in-B');
  st().openPane(A, P2);
  st().setActivePanel(B);

  // Exactly what ContentView hands down and QueryPanel resolves.
  const paneIdFor = (panelId: string) =>
    keys.paneKey(panelId, keys.resolveFocusedPaneId(st().focusedPaneId));
  const ghostRoutedKey = paneIdFor(B);
  const ghostRoutedSql = st().queryExec.get(ghostRoutedKey)?.sql;
  const realTabSql = st().queryExec.get(B)?.sql ?? '';

  // ── consequence 2: type in B the way QueryPanel's onChange does ──
  const paneId = keys.resolveFocusedPaneId(st().focusedPaneId);
  st().updateSql(B, 'SELECT typed-in-B-after-split', ...keys.paneArgs(paneId));
  const execKeysAfterTyping = [...st().queryExec.keys()].sort();
  const orphanKeySql = st().queryExec.get(`${B}::${P2}`)?.sql;

  // Execute, and see which key the rows land in.
  let emit: ((event: unknown) => void) | undefined;
  mockExecuteQueryStream.mockImplementation(
    (_sess: string, _sql: string, onEvent: (event: unknown) => void) => {
      emit = onEvent;
      return new Promise<void>(() => {});
    },
  );
  void st().executeQuery(B, undefined, paneId);
  await vi.waitFor(() => expect(emit).toBeTypeOf('function'));
  emit?.({ type: 'statementStart', index: 0, sql: 'SELECT typed-in-B-after-split', columns: [] });
  emit?.({ type: 'rows', index: 0, rows: [[1]] });
  const keysHoldingRows = [...st().queryExec.entries()]
    .filter(([, v]) => (v.results?.length ?? 0) > 0)
    .map(([k]) => k)
    .sort();

  // ── consequence 3: close tab B's pane while tab A is on screen ──
  st().updateSql(A, 'SELECT in-A-p2', P2);
  st().setActivePanel(A);
  st().setFocusedPane(P2);
  const tabARouteBefore = paneIdFor(A);
  st().closePane(B, P2);
  const tabARouteAfter = paneIdFor(A);
  // Did the build wrongly report tab A's pane as closed?
  const tabAPaneStillOpen = st().queryExec.has(`${A}::${P2}`);

  return {
    ghostRoutedKey,
    ghostRoutedSql,
    realTabSql,
    execKeysAfterTyping,
    orphanKeySql,
    keysHoldingRows,
    tabARouteBefore,
    tabARouteAfter,
    tabAPaneStillOpen,
  };
}

describe('[tester] BUG-001 differential reproduction (227b6af8e^ vs HEAD)', () => {
  const observed: Record<string, Observation> = {};

  it('PROVENANCE 1: the "before" module really is the pre-fix store', async () => {
    // Without this, a stale or hand-edited copy silently turns the differential
    // into a test of nothing. The pre-fix store has a single global
    // `focusedPaneId` and no per-tab map; the fixed store has both.
    const before = await import('./fixtures/panelStore.pre-fix-227b6af8e');
    const after = await import('../panelStore');

    const beforeFields = Object.keys(
      before.usePanelStore.getState() as unknown as Record<string, unknown>,
    );
    const afterFields = Object.keys(
      after.usePanelStore.getState() as unknown as Record<string, unknown>,
    );
    expect(beforeFields).toContain('focusedPaneId');
    expect(beforeFields).not.toContain('focusedPaneIdByPanel');
    expect(afterFields).toContain('focusedPaneId');
    expect(afterFields).toContain('focusedPaneIdByPanel');
  });

  it('PROVENANCE 2: the fixture is a generated snapshot of a NAMED commit, and is current', () => {
    // "Which moment is this a copy of?" must be a checkable fact, not a comment.
    // Three layers, cheapest first:
    //   a) the generated banner names the source commit
    //   b) its body is byte-identical to that commit's blob  → a stale fixture
    //      (the maintenance trap: someone edits the real store, forgets this)
    //      fails here instead of quietly becoming a different historical moment.
    //      **This layer is not optional**: if the blob cannot be produced the test
    //      FAILS by default rather than warning and passing. See the G4 note below.
    //   c) no production file imports it
    const source = readFileSync(FIXTURE_PATH, 'utf8');

    // (a) banner names the commit
    const cut = source.lastIndexOf(FIXTURE_MARKER) + FIXTURE_MARKER.length;
    const banner = source.slice(0, cut);
    expect(banner).toContain('GENERATED FILE');
    // NOT `toContain(FIXTURE_SOURCE_COMMIT)` over the whole banner: several other
    // banner lines quote the commit, so that assertion cannot fail. Pin the one
    // line that actually declares WHICH snapshot this is.
    const declared = banner
      .split('\n')
      .map((l) => l.replace(/^\/\/\s?/, '').trim()) // strip the comment prefix too
      .find((l) => l.startsWith('source  :'));
    expect(declared, 'banner has no `source  :` line').toBeDefined();
    expect(declared, 'the banner must name the declared source commit').toBe(
      `source  : commit ${FIXTURE_SOURCE_COMMIT}^ , path ${FIXTURE_SOURCE_PATH}`,
    );

    // (b) body is byte-identical to the named commit's blob
    const body = source.slice(cut);
    let blob: string | null = null;
    try {
      blob = execFileSync('git', ['show', `${FIXTURE_SOURCE_COMMIT}^:${FIXTURE_SOURCE_PATH}`], {
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
      });
    } catch {
      blob = null; // shallow clone / no git — handled below
    }
    if (blob === null) {
      // ── G4: this used to `console.warn` and PASS ──────────────────────────
      // A warn-and-pass here is a conditional failure mode, and a CI that reads
      // only the exit code cannot see it. Measured consequence: with the blob
      // unavailable, an equal-length in-place mutation of the fixture body
      // (`p.id !== panelId` → `p.id === panelId`) left the whole suite green,
      // so the "fixture is byte-identical to the commit it claims to snapshot"
      // claim was simply not being checked at all.
      //
      // The rule this restores: a guard must not report success for a check it
      // did not run. So the default is a FAILURE, not a warning. The escape
      // hatch is an explicit, greppable acknowledgement for environments where
      // the commit is genuinely unreachable (shallow clone with the fix commit
      // outside the fetched history, exported tarball, vendored source).
      const acknowledgement =
        'Set DATAZEN_ALLOW_MISSING_FIXTURE_BLOB=1 to acknowledge that this ' +
        'provenance check did not run.';
      if (process.env.DATAZEN_ALLOW_MISSING_FIXTURE_BLOB !== '1') {
        throw new Error(
          '[provenance] the byte-identity check did NOT run: git could not produce ' +
            `${FIXTURE_SOURCE_COMMIT}^:${FIXTURE_SOURCE_PATH} (shallow clone / no git / ` +
            `commit outside fetched history). Refusing to pass: an in-place, equal-length ` +
            'edit of the fixture would go undetected in this state, which is exactly the ' +
            'maintenance trap this layer exists to catch. ' +
            acknowledgement,
        );
      }
      // Acknowledged. Still not a silent pass — say what was not verified, and
      // keep the two checks that need no git (a fixture that had drifted onto the
      // post-fix API would still be caught).
      console.warn(
        '[provenance] ACKNOWLEDGED SKIP — byte-identity NOT checked for ' +
          `${FIXTURE_SOURCE_COMMIT}^:${FIXTURE_SOURCE_PATH}. ` +
          'The fixture body is unverified against its source commit.',
      );
      expect(body).not.toContain('focusedPaneIdByPanel');
      expect(body).toContain('focusedPaneId');
      return;
    }
    // The fixture rewrites relative import depth, so compare on imports removed,
    // and compare digests so a failure prints a short message, not 20KB.
    // Type-only escape lines (Snapshot note / @ts-expect-error) are stripped
    // as well: the snapshot must keep calling retired IPC surfaces exactly as
    // it did at FIXTURE_SOURCE_COMMIT^ (that call shape is the point of the
    // differential) while the real command types move on. Comment-only lines
    // cannot change runtime behavior, so provenance over behavior is kept.
    const norm = (s: string) =>
      s
        .replace(/^.*from '\.[^']*';?$/gm, '')
        .replace(/^\s*\/\/ (Snapshot note|@ts-expect-error).*$/gm, '')
        .replace(/\s+/g, '');
    const a = norm(body);
    const b = norm(blob);
    expect(
      a.length,
      `fixture body is ${a.length} chars, ${FIXTURE_SOURCE_COMMIT}^ blob is ${b.length}. ` +
        'The fixture has drifted from the commit it claims to snapshot — regenerate it.',
    ).toBe(b.length);
    expect(a, 'fixture body differs from the commit it claims to snapshot — regenerate it').toBe(b);
  });

  it('PROVENANCE 3: no production file under src/stores/ imports the fixture', () => {
    // (c) The fixture must stay test-only. It is a dead snapshot of production
    // code; importing it from a non-`__tests__` file would put a second, frozen
    // store into the shipped program.
    const offenders: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const full = join(dir, entry.name);
        if (entry.isDirectory()) {
          walk(full);
          continue;
        }
        if (!/\.tsx?$/.test(entry.name)) continue;
        if (full.split(sep).includes('__tests__')) continue; // test code may import it
        if (readFileSync(full, 'utf8').includes(FIXTURE_BASENAME)) offenders.push(full);
      }
    };
    walk(STORES_ROOT);
    expect(offenders).toEqual([]);
  });

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  afterAll(() => {
    // Printed so the numbers land in the vitest log, not only in assertions.
    for (const [tag, o] of Object.entries(observed)) {
      console.log(`[repro:${tag}] ${JSON.stringify(o, null, 2)}`);
    }
  });

  for (const { tag, load } of BUILDS) {
    it(`measures the three consequences on the "${tag}" build`, async () => {
      const keys: PaneKeys = await import('../paneKeys');
      const build = await load();
      observed[tag] = await runScenarios(build, keys);
      // Smoke: the scenario must have really executed both tabs.
      expect(observed[tag].execKeysAfterTyping.length).toBeGreaterThanOrEqual(3);
    });
  }

  it('CONFIRMS consequence 1: before the fix the unsplit sibling routes to a pane it never opened', () => {
    const before = observed.before;
    const after = observed.after;
    expect(before.ghostRoutedKey).toBe(`${B}::${P2}`);
    expect(before.ghostRoutedSql).toBeUndefined();
    expect(before.realTabSql).toBe('SELECT typed-in-B');
    // after the fix the sibling routes to its own, already-seeded entry
    expect(after.ghostRoutedKey).toBe(B);
    expect(after.ghostRoutedSql).toBe('SELECT typed-in-B');
  });

  it('CONFIRMS consequence 2: before the fix typing grows an unowned exec entry and rows stream into it', () => {
    const before = observed.before;
    const after = observed.after;
    expect(before.execKeysAfterTyping).toEqual([A, `${A}::${P2}`, B, `${B}::${P2}`].sort());
    expect(before.orphanKeySql).toBe('SELECT typed-in-B-after-split');
    expect(before.keysHoldingRows).toEqual([`${B}::${P2}`]);
    // after the fix: no unowned key at all, and rows land on the key the user reads
    expect(after.execKeysAfterTyping).toEqual([A, `${A}::${P2}`, B].sort());
    expect(after.orphanKeySql).toBeUndefined();
    expect(after.keysHoldingRows).toEqual([B]);
  });

  it('CONFIRMS consequence 3: before the fix closing another tab pane yanks this tab focus', () => {
    const before = observed.before;
    const after = observed.after;
    expect(before.tabARouteBefore).toBe(`${A}::${P2}`);
    expect(before.tabARouteAfter).toBe(A);
    // tab A's pane was never closed — only the focus was stolen.
    expect(before.tabAPaneStillOpen).toBe(true);
    // after the fix tab A keeps its own focus
    expect(after.tabARouteBefore).toBe(`${A}::${P2}`);
    expect(after.tabARouteAfter).toBe(`${A}::${P2}`);
  });
});
