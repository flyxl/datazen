/**
 * [redis-tree-ui-BUG-001] Pattern-filter journey (D-2 + D-8, coder round-1).
 *
 * Keystroke-by-keystroke over the *tree* view, because the defect was exactly a
 * missing link in this path: the applied pattern reaches `scan_keys` but not the
 * server-driven rows. Assertions follow the state machine of the filter — enter
 * (type → nothing happens until `Enter`), state (rows narrow, breadcrumb appears,
 * counter + select-all agree with the screen), exit (`Esc`, then a narrower
 * pattern, then the empty-folder case) — and only ever read `data-*` attributes,
 * i18n keys and recorded mock arguments. No English copy, no geometry (AGENTS.md
 * 原则六 / PRD §7-6).
 *
 * The virtualizer is stubbed to hand back every row: jsdom measures a zero-size
 * scroll element and would mount none, which would make "which rows exist"
 * untestable rather than cheap.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSchemaStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type SchemaStoreState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: (opts: { count: number; estimateSize: () => number }) => {
    const size = opts.estimateSize();
    const items = Array.from({ length: opts.count }, (_, index) => ({
      index,
      key: index,
      start: index * size,
      size,
      lane: 0,
    }));
    return {
      getVirtualItems: () => items,
      getTotalSize: () => items.length * size,
      measureElement: () => undefined,
      scrollToOffset: () => undefined,
      scrollToIndex: () => undefined,
    };
  },
}));

class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
// eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
globalThis.ResizeObserver ??= MockResizeObserver as unknown as typeof ResizeObserver;

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key }),
}));

const scanKeys = vi.fn();
const listChildren = vi.fn();
const dbSizes = vi.fn();
const getKey = vi.fn();

vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  invokeScanKeys: (...args: unknown[]) => scanKeys(...args),
  invokeListChildren: (...args: unknown[]) => listChildren(...args),
  invokeDbSizes: (...args: unknown[]) => dbSizes(...args),
  invokeGetKey: (...args: unknown[]) => getKey(...args),
}));

import { RedisWorkbench } from '../key-browser/RedisWorkbench';
import { patternToTreePrefix } from '../key-browser/useKeyTree';
import { filterKeysByPattern } from '../key-browser/keyTreeFilter';
import { DEFAULT_SEPARATOR } from '../key-browser/keyTree';
import type { ChildEntry } from '../shared/redisInvoke';

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);
bindSchemaStore(
  create<SchemaStoreState>(() => ({
    schemas: new Map([['sess-1', { pathItems: {}, databases: ['db0', 'db1'], loading: false }]]),
    loadForConnection: async () => {},
    setLoadedTables: () => {},
    mergeNamespace: () => {},
    registerPathAliases: () => {},
    cachePathItems: () => {},
  })),
);

/* ── fixtures ─────────────────────────────────────────────────────────────── */

function leaf(key: string, keyType = 'string'): ChildEntry {
  return { kind: 'key', key, keyType, ttl: -1, logicalLen: 4, memBytes: null };
}

function folder(prefix: string, count: number): ChildEntry {
  return { kind: 'folder', prefix, count };
}

function renderWorkbench() {
  render(<RedisWorkbench dbSessionId="sess-1" initialDatabase="db0" hideSidebar />);
}

/**
 * The keyspace this fixture's *server* emulates: one one-level namespace (`app:`)
 * with two keys, plus three loose root keys. `zzz-thing` exists so a pattern with
 * no separator (`zzz`) has both a head worth routing to and a row that survives
 * the same pattern.
 */
const KEYSPACE = ['app:1', 'app:2', 'cache-hit', 'root-plain', 'zzz-thing'];

/**
 * Faithful `list_children`: scan `prefix*` and fold exactly one level on `sep`
 * (Rust `split_children` semantics — a remainder with no separator is a leaf
 * keyed by its *absolute* name, otherwise it is a folder counted per key seen).
 *
 * Faithful on purpose: both halves of BUG-001's fix (the routed prefix and the
 * client glob) then speak the same glob, so a number that agrees here agrees for
 * a real reason instead of because two mocks were rigged to match.
 */
function childrenFor(prefix: string, sep = ':'): ChildEntry[] {
  const seen = new Map<string, number>();
  const leaves: string[] = [];
  for (const key of KEYSPACE) {
    if (!key.startsWith(prefix)) continue;
    const rest = key.slice(prefix.length);
    const at = rest.indexOf(sep);
    if (at < 0) leaves.push(key);
    else {
      const folderPrefix = `${prefix}${rest.slice(0, at + sep.length)}`;
      seen.set(folderPrefix, (seen.get(folderPrefix) ?? 0) + 1);
    }
  }
  return [
    ...[...seen.entries()].map(([folderPrefix, count]) => folder(folderPrefix, count)),
    ...leaves.map((key) => leaf(key)),
  ];
}

/**
 * What the *server* returns for `scan_keys(pattern)` over {@link KEYSPACE}.
 *
 * Hand-derived per pattern from Redis' MATCH rules (a leading `*` = "ends with",
 * a trailing `*` = "starts with", no `*` = exact, `[0-9]` = one digit byte) —
 * deliberately a literal table rather than a call into `keyTreeFilter`: the whole
 * point of BUG-003 is that the client filter and the server must agree, and an
 * oracle that borrows the client implementation could not observe them disagree.
 * `keyTreeFilter agrees with the scan_keys oracle` asserts the pair per pattern,
 * so the table cannot silently drift from the client either.
 */
const SCAN_KEYS_ANSWERS: Readonly<Record<string, readonly string[]>> = {
  '*': KEYSPACE,
  '': KEYSPACE,
  '*nope': [],
  'zzz*': ['zzz-thing'],
  // The server pattern a typed literal now produces. `zzz` (no `*` ⇒ exact) is
  // gone from this table on purpose: the search row can no longer emit it, since
  // a literal without a metacharacter is wrapped as a prefix. The exact-match
  // rule itself is still covered as a client-filter unit in `keyTreeFilter.test.ts`.
  'app*': ['app:1', 'app:2'],
  'app:*': ['app:1', 'app:2'],
  '*:1': ['app:1'],
  '*thing': ['zzz-thing'],
  '*[0-9]': ['app:1', 'app:2'],
  '*[a-c]*': ['app:1', 'app:2', 'cache-hit', 'root-plain'],
};

/** Flat `scan_keys`, honouring the glob it was given — as Redis SCAN MATCH does. */
function keysFor(pattern: string) {
  const answer = SCAN_KEYS_ANSWERS[pattern] ?? [];
  return answer.map((key) => entry(key));
}

function entry(key: string) {
  return { key, keyType: key === 'app:2' ? 'hash' : 'string', ttl: -1, size: 4, preview: '' };
}

/** Every `list_children` call as the prefix it asked for. */
function childPrefixes(): string[] {
  return listChildren.mock.calls.map((call) => String((call as unknown[])[2]));
}

function tree(): HTMLElement {
  return screen.getByTestId('redis-key-tree');
}

function attr(testId: string, name: string): string | null {
  return screen.getByTestId(testId).getAttribute(name);
}

/** The select-all checkbox is a real `<button>`, so `disabled` lives on it. */
function selectAllButton(): HTMLButtonElement {
  return screen.getByTestId('redis-tree-select-all') as HTMLButtonElement;
}

/**
 * The leaf keys whose checkbox is currently ticked.
 *
 * The header's selection-count badge went away with the batch action group, so
 * "how much of the tree is selected" is read off the checkboxes themselves.
 * That is a *stronger* observable: the badge was one number derived from this
 * set, whereas this is the set. State comes from the input's `checked` property
 * rather than the row's optional `data-checked` mirror.
 */
function tickedKeys(): string[] {
  return Array.from(
    document.querySelectorAll<HTMLInputElement>('[data-testid^="redis-tree-key-check-"]'),
  )
    .filter((el) => el.checked)
    .map((el) => (el.getAttribute('data-testid') ?? '').replace('redis-tree-key-check-', ''));
}

async function applyPattern(value: string): Promise<void> {
  const input = await screen.findByTestId('redis-search-input');
  fireEvent.change(input, { target: { value } });
  fireEvent.keyDown(input, { key: 'Enter' });
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  scanKeys.mockImplementation(async (_s: string, _i: number, pattern: string) => {
    const keys = keysFor(pattern);
    return { keys, cursor: 0, dbSize: KEYSPACE.length, matched: keys.length };
  });
  listChildren.mockImplementation(async (_s: string, _i: number, prefix: string) => ({
    children: childrenFor(prefix),
    cursor: 0,
  }));
  dbSizes.mockResolvedValue([
    { db: 0, keys: KEYSPACE.length },
    { db: 1, keys: 0 },
  ]);
  getKey.mockResolvedValue({
    key: 'app:1',
    keyType: 'string',
    ttl: -1,
    value: 'v',
    size: 1,
    memory: null,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

/* ── prefix routing (pure, the server half of the fix) ─────────────────────── */

describe('[redis-tree-ui-BUG-001] patternToTreePrefix routes the literal head', () => {
  const sep = DEFAULT_SEPARATOR;

  it.each([
    // pattern, expected root prefix
    ['', ''],
    ['*', ''],
    ['   ', ''],
    // a leading star means the head is unbounded: no prefix at all
    ['*user*', ''],
    ['*plain', ''],
    // a head that already ends at a separator is kept whole
    ['app:*', 'app:'],
    ['app:user:*', 'app:user:'],
    // a head cut inside a segment falls back to the last separator …
    ['app:us*', 'app:'],
    // … and a head with no separator at all becomes the literal itself
    ['zzz', 'zzz'],
    ['zzz*', 'zzz'],
    // no star ⇒ the whole pattern is the literal head
    ['root-plain', 'root-plain'],
    /*
     * BUG-003 consequence: the routed head must be *purely literal*, because
     * `list_children` re-globs `{prefix}*` **and** byte-slices keys at
     * `prefix.len`. `[ac]` as a prefix would strip four bytes off keys that never
     * started with them, so the head is truncated at the first metacharacter —
     * which still narrows safely (a superset), rather than giving up: `a[0-9]*`
     * scans `a*`, `app:a?b*` scans `app:*`, and only a pattern that *opens* with
     * a metacharacter keeps the whole keyspace.
     */
    ['*[0-9]', ''],
    ['[ac]*', ''],
    ['a[0-9]*', 'a'],
    ['h[a-c]*', 'h'],
    ['a?b', 'a'],
    ['\\lit', ''],
    ['app:a?b*', 'app:'],
    ['app[0-9]:*', 'app'],
  ] as const)('%s ⇒ prefix %s', (pattern, expected) => {
    expect(patternToTreePrefix(pattern, sep)).toBe(expected);
  });

  it('honours the configured separator, never a hardcoded colon', () => {
    // The cut happens at the *configured* boundary: with `.` the segment is
    // `app`, so that is the prefix the server should fold on — a `:`-based cut
    // here would address a namespace the tree is not grouping by.
    expect(patternToTreePrefix('app.user:*', '.')).toBe('app.');
    expect(patternToTreePrefix('app.user.', '.')).toBe('app.user.');
    expect(patternToTreePrefix('app/user/*', '/')).toBe('app/user/');
    // A pattern whose only separator is someone else's has no boundary here, so
    // the literal head is used rather than silently splitting on `:`.
    expect(patternToTreePrefix('app:user:*', '.')).toBe('app:user:');
  });
});

/* ── the journey ──────────────────────────────────────────────────────────── */

describe('[redis-tree-ui-BUG-001] the applied pattern narrows the tree view', () => {
  it('nothing matched: rows drop to 0 and no-match becomes reachable', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    expect(tree().getAttribute('data-row-count')).toBe('4');

    // Leading star ⇒ `patternToTreePrefix` routes nothing (root prefix stays ''),
    // so the four loaded rows are all the server said yes to. Every narrowing
    // below can therefore only come from the client-side filter — the half of
    // BUG-001 that was missing entirely.
    await applyPattern('*nope');
    await waitFor(() => expect(childPrefixes().every((p) => p === '' || p === 'app:')).toBe(true));
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('0'));
    // I-11: finished scan + active filter is exactly the `no-match` fact, now
    // reachable in the *default* view (it used to be named-but-unreachable).
    const empty = await screen.findByTestId('redis-tree-empty');
    expect(empty.getAttribute('data-empty-state')).toBe('no-match');
    expect(tree().getAttribute('data-filter-active')).toBe('true');

    // A pattern whose literal head is a namespace (`zzz*`) reaches the server as
    // a `zzz*` prefix scan *and* keeps `zzz-thing` on the client side — the two
    // halves agreeing from opposite directions is the point.
    await applyPattern('zzz*');
    await waitFor(() =>
      expect(childPrefixes().some((prefix) => prefix.startsWith('zzz'))).toBe(true),
    );
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('1'));
    // `zzz-thing` survives in both halves, so the empty state must be gone.
    expect(await screen.findByTestId('redis-key-row-zzz-thing')).toBeTruthy();
    expect(screen.queryByTestId('redis-tree-empty')).toBeNull();
  });

  /*
   * The reported defect, against the literal oracle rather than the client
   * filter: typing `app` used to reach `SCAN … MATCH app`, which admits only the
   * single key spelled `app` — so `app:1` and `app:2` were invisible and the
   * whole `app` namespace read as "no matches". The oracle table is derived by
   * hand from Redis' MATCH rules, so a regression here cannot hide behind the
   * client implementation agreeing with itself.
   */
  it('a typed literal reaches the server as a prefix, not an exact key', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');

    await applyPattern('app');
    // The scan asked for `app*`, which the oracle answers with both app keys.
    await waitFor(() => expect(scanKeys.mock.calls.at(-1)?.[2]).toBe('app*'));
    // R1 counts the key set the pattern admitted, so both app keys are in it —
    // this is the number the user sees when they ask "did it find anything".
    await waitFor(() => expect(attr('redis-tree-count', 'data-loaded')).toBe('2'));
    // Something is on screen, and it is the app namespace rather than a
    // no-match page. (The leaves sit under a collapsed `app:` folder, so the
    // row count is 1 — tree geometry, not the question being asked here.)
    expect(await screen.findByTestId('redis-tree-folder-app:')).toBeTruthy();
    expect(screen.queryByTestId('redis-tree-empty')).toBeNull();
    // The tree walk narrows to the namespace instead of walking everything.
    expect(childPrefixes().some((prefix) => prefix === 'app')).toBe(true);
  });

  it('a typed literal and the prefix it stands for admit the same keys', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');

    await applyPattern('app:*');
    await waitFor(() => expect(attr('redis-tree-count', 'data-loaded')).toBe('2'));
    // `app:*` is the narrower pattern, so its two leaves can sit at the root.
    expect(tree().getAttribute('data-row-count')).toBe('2');

    // `app` now stands for `app*`, which is strictly *broader* (it would also
    // admit `apple`), so the routed head stays `app` and the server folds it
    // into one `app:` folder. Same keys, one row instead of two — that is the
    // fold, not a disagreement, and the counter is what has to match.
    await applyPattern('app');
    await waitFor(() => expect(scanKeys.mock.calls.at(-1)?.[2]).toBe('app*'));
    await waitFor(() => expect(attr('redis-tree-count', 'data-loaded')).toBe('2'));
    expect(tree().getAttribute('data-row-count')).toBe('1');
    expect(screen.getByTestId('redis-tree-folder-app:')).toBeTruthy();
  });

  it('clearing the filter brings the rows back and the empty state goes', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    // A typed literal is a *prefix* now, so the no-match case needs a head that
    // genuinely has nothing under it: `zzz` would reach the server as `zzz*` and
    // find `zzz-thing`. The empty state is the subject here, not the letter.
    await applyPattern('nope');
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('0'));
    await screen.findByTestId('redis-tree-empty');

    // `Esc` is the row's exit: input cleared *and* filter dropped, not just text.
    const input = screen.getByTestId('redis-search-input');
    fireEvent.keyDown(input, { key: 'Escape' });
    await waitFor(() => expect(input).toHaveValue(''));
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('4'));
    expect(screen.queryByTestId('redis-tree-empty')).toBeNull();
    // Back to the whole keyspace: the root scan asks for `*` again.
    await waitFor(() => expect(childPrefixes()).toContain(''));
  });

  it('a prefix pattern routes the root request and re-roots the tree inside it', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    const rootsBefore = childPrefixes().length;

    await applyPattern('app:*');
    // Server half: the root `list_children` now asks for prefix `app:`, so the
    // tree is rooted inside the namespace instead of painting one folder.
    await waitFor(() => expect(childPrefixes().length).toBeGreaterThan(rootsBefore));
    expect(childPrefixes().slice(rootsBefore)).toContain('app:');
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('2'));
    expect(await screen.findByTestId('redis-key-row-app:1')).toBeTruthy();
    // Keys the pattern rejects are gone from both halves of the pipe.
    expect(screen.queryByTestId('redis-key-row-root-plain')).toBeNull();
    expect(screen.queryByTestId('redis-key-row-cache-hit')).toBeNull();
    // …and R1's counter agrees with the painted rows (BUG-001 single source).
    await waitFor(() => expect(attr('redis-tree-count', 'data-loaded')).toBe('2'));
  });

  it('a deep-only match paints its parent as a non-interactive breadcrumb', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    fireEvent.click(screen.getByTestId('redis-tree-folder-app:'));
    await screen.findByTestId('redis-key-row-app:1');
    expect(tree().getAttribute('data-row-count')).toBe('6');

    // Leading star ⇒ no root prefix to route, so this is pure client filtering:
    // only `app:1` survives, and `app:` comes back as path context.
    await applyPattern('*:1');
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('1'));
    const crumb = await screen.findByTestId('redis-tree-folder-app:');
    expect(crumb.getAttribute('data-breadcrumb')).toBe('true');
    // The surviving leaf is a real row.
    expect(screen.getByTestId('redis-key-row-app:1').getAttribute('data-row-kind')).toBe('key');
    // Path context only: clicking the breadcrumb does not fold, and it cannot be
    // checked — there is no checkbox in that row at all.
    const before = childPrefixes().length;
    fireEvent.click(crumb);
    expect(childPrefixes().length).toBe(before);
    expect(screen.queryByTestId('redis-tree-folder-check-app:')).toBeNull();
  });

  it('R1, 「全选已加载」 and the painted rows read one set, both ways', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    // No filter: the counter counts every loaded key (5) while the fold paints
    // four rows, because two of those keys sit *inside* the collapsed `app:`
    // folder — the counter and the rows count different things by design here,
    // and neither is the unfiltered set BUG-001 used to show.
    await waitFor(() => expect(attr('redis-tree-count', 'data-loaded')).toBe('5'));
    expect(tree().getAttribute('data-row-count')).toBe('4');

    // Enter half of the contradiction: a pattern that cuts everything.
    // `app`-free pattern: nothing in the flat scan matches, and the tree keeps
    // only `zzz-thing`. The counter (visible set) must read 1, never the 3 the
    // pattern-blind flat list still holds — that gap is the single-source proof.
    await applyPattern('*thing');
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('1'));
    await waitFor(() => expect(attr('redis-tree-count', 'data-loaded')).toBe('1'));
    // Select-all over the visible set takes exactly that one key.
    fireEvent.click(screen.getByTestId('redis-tree-select-all'));
    await waitFor(() => expect(tickedKeys()).toEqual(['zzz-thing']));

    // A pattern that keeps the two app keys: select-all takes exactly those,
    // never the `root-plain` key the same tree had before the filter.
    await applyPattern('app:*');
    await waitFor(() => expect(selectAllButton().disabled).toBe(false));
    fireEvent.click(selectAllButton());
    await waitFor(() => expect(tickedKeys().sort()).toEqual(['app:1', 'app:2']));
    expect(screen.getByTestId('redis-tree-key-check-app:1').getAttribute('data-checked')).toBe(
      'true',
    );
    expect(screen.queryByTestId('redis-tree-key-check-root-plain')).toBeNull();
  });

  it('an unapplied edit does not repaint the tree, applied does', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    const input = screen.getByTestId('redis-search-input');

    // Intermediate keystrokes: D-2's applied/typed split must survive the new
    // wiring — half-typed text must not cut rows nor fire a tree re-root.
    const rootsBefore = childPrefixes().length;
    fireEvent.change(input, { target: { value: 'zz' } });
    fireEvent.change(input, { target: { value: 'zzz' } });
    expect(tree().getAttribute('data-row-count')).toBe('4');
    expect(childPrefixes().length).toBe(rootsBefore);

    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('1'));
  });

  it('R1 counts the pattern-visible set even when the flat scan ignored the glob', async () => {
    /*
     * The wedge for the single-source rule. `scan_keys` is forced pattern-blind
     * here (it answers all five keys whatever it was asked), so *nothing else on
     * screen* can narrow anything: if R1's counter still reads 0 after `*nope`,
     * it is reading the one filtered set (and so is 「全选已加载」 — same prop),
     * not the flat list. An implementation wired to `scan.keys.length` shows 5
     * rows' worth of keys against 0 painted rows — the BUG-001 lie in one number.
     */
    scanKeys.mockImplementation(async () => ({
      keys: KEYSPACE.map((key) => entry(key)),
      cursor: 0,
      dbSize: KEYSPACE.length,
      matched: KEYSPACE.length,
    }));
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    await waitFor(() => expect(attr('redis-tree-count', 'data-loaded')).toBe('5'));

    await applyPattern('*nope');
    await waitFor(() => expect(tree().getAttribute('data-row-count')).toBe('0'));
    await waitFor(() => expect(attr('redis-tree-count', 'data-loaded')).toBe('0'));
    expect(selectAllButton().disabled).toBe(true);
    const empty = await screen.findByTestId('redis-tree-empty');
    expect(empty.getAttribute('data-empty-state')).toBe('no-match');
  });

  /* ── BUG-003: the two views must mean the same thing by one pattern ─────── */

  it('`*[0-9]` no longer leaves the tree asserting no-match about live keys', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');

    // The reported repro. Before BUG-003 the class expression was evaluated by
    // the server (3 list rows) and by an incapable client filter (0 rows +
    // `no-match`) at the same time. Now both halves speak Redis MATCH:
    await applyPattern('*[0-9]');
    await waitFor(() => expect(scanKeys.mock.calls.at(-1)?.[2]).toBe('*[0-9]'));
    // R1's counter is the *key* set both halves agree on …
    await waitFor(() => expect(attr('redis-tree-count', 'data-loaded')).toBe('2'));
    // … and the tree paints it as one collapsed folder holding exactly those two
    // keys (a folder row is one row for many keys — that is tree semantics, not
    // a disagreement). What must NOT happen is the false empty state.
    const folderRow = await screen.findByTestId('redis-tree-folder-app:');
    expect(folderRow.getAttribute('data-breadcrumb')).toBe('false');
    expect(screen.queryByTestId('redis-tree-empty')).toBeNull();

    // Expanding proves the subtree really is the server's answer: both leaves
    // appear and nothing the pattern rejected can slip in.
    fireEvent.click(folderRow);
    await screen.findByTestId('redis-key-row-app:1');
    await screen.findByTestId('redis-key-row-app:2');
    expect(screen.queryByTestId('redis-key-row-cache-hit')).toBeNull();
    expect(screen.queryByTestId('redis-key-row-zzz-thing')).toBeNull();
  });

  it.each(Object.entries(SCAN_KEYS_ANSWERS))(
    'pattern %j: tree rows, R1 counter and the server answer are one set',
    async (pattern, expected) => {
      renderWorkbench();
      await screen.findByTestId('redis-tree-folder-app:');
      await applyPattern(pattern);

      // The pure client filter must agree with the (hand-written) server answer,
      // otherwise the two views can still disagree even though the widgets read
      // one source.
      await waitFor(() => expect(filterKeysByPattern(KEYSPACE, pattern)).toEqual([...expected]));
      // … and what R1 counts is that same set, so the flat list and the tree can
      // never show opposite facts for one pattern again. (The *row* count is not
      // asserted equal here: under a collapsed folder one row stands for many
      // keys, which is tree semantics, not a contradiction.)
      await waitFor(() =>
        expect(attr('redis-tree-count', 'data-loaded')).toBe(String(expected.length)),
      );
      if (expected.length > 0) {
        expect(screen.queryByTestId('redis-tree-empty')).toBeNull();
      } else {
        // Only a genuinely finished, non-scanning root may claim `no-match`.
        const empty = await screen.findByTestId('redis-tree-empty');
        expect(empty.getAttribute('data-empty-state')).toBe('no-match');
      }
    },
  );

  it('a folder with an open scan admits its remainder is unfiltered', async () => {
    // Root level stays mid-scan (cursor 41), so `app:`'s count is a lower bound.
    listChildren.mockImplementation(async (_s: string, _i: number, prefix: string) => {
      if (prefix === 'app:') return { children: [leaf('app:1'), leaf('app:2')], cursor: 0 };
      return { children: childrenFor(prefix), cursor: prefix === '' ? 41 : 0 };
    });
    renderWorkbench();
    const badge = await screen.findByTestId('redis-tree-folder-count-app:');
    expect(badge.getAttribute('data-partial')).toBe('true');
    // Without a filter: just the `+`.
    expect(badge.textContent).toContain('redis.tree.folderPartial');
    expect(badge.textContent).not.toContain('redis.tree.filterUnloaded');

    // With a filter in force the same badge must name the accounting gap: the
    // pattern applies to loaded rows only, so `(n)` of an unfinished level is
    // neither a match count nor a total (the recorded known limitation). The
    // folder itself survives *as a real row* — the filtered key set says a match
    // can live under it, so expanding it stays the user's way in.
    await applyPattern('*:1');
    // Applying a pattern also re-scans the flat list, and while that is in
    // flight the column shows the loading strip (I-11 never explains an absence
    // that is really "not finished"). So the settled state is waited for, not
    // assumed synchronous.
    await waitFor(() => expect(attr('redis-key-tree', 'data-filter-active')).toBe('true'));
    const held = await screen.findByTestId('redis-tree-folder-app:');
    expect(held.getAttribute('data-breadcrumb')).toBe('false');
    const heldBadge = screen.getByTestId('redis-tree-folder-count-app:');
    expect(heldBadge.textContent).toContain('redis.tree.folderPartial');
    expect(heldBadge.textContent).toContain('redis.tree.filterUnloaded');

    // Exit: dropping the filter keeps the `+` but drops the caveat.
    fireEvent.keyDown(screen.getByTestId('redis-search-input'), { key: 'Escape' });
    await waitFor(() => expect(attr('redis-key-tree', 'data-filter-active')).toBe('false'));
    expect(screen.getByTestId('redis-tree-folder-count-app:').textContent).not.toContain(
      'redis.tree.filterUnloaded',
    );
  });
});
