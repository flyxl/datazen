/**
 * 键树 track interaction journeys (PRD §3.2 R3 + 行规格/sticky, §4 I-4/I-8/I-9/I-11;
 * task book D-3..D-8).
 *
 * Same assertion policy as `keyTreeJourney.test.tsx`: `useI18n` is stubbed with
 * an identity `t` so a rendered string equals its i18n key — **no test reads
 * copy**; locators are `data-testid`, state comes from `data-*` attributes or
 * from the recorded command calls. The virtualizer is stubbed to yield every
 * row (jsdom measures a zero-size scroll element).
 *
 * The journeys are step-by-step on purpose (AGENTS.md 连续旅程测试): every
 * transition — including the intermediate ones (rows still on screen while a
 * refresh is in flight, a failed key still checked after the post-write
 * refresh, an open cursor masking a filter) — is asserted, not just the final
 * legal state.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSchemaStore,
  bindSettingsStore,
  type ConfirmDialogOptions,
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
const redisCommand = vi.fn();

vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  invokeScanKeys: (...args: unknown[]) => scanKeys(...args),
  invokeListChildren: (...args: unknown[]) => listChildren(...args),
  invokeDbSizes: (...args: unknown[]) => dbSizes(...args),
  invokeGetKey: (...args: unknown[]) => getKey(...args),
  redisCommandInvoke: (...args: unknown[]) => redisCommand(...args),
}));

import { RedisWorkbench } from '../key-browser/RedisWorkbench';

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
/*
 * Controllable so the R1 select-all → delete journey can walk both branches of
 * the gate order (confirm refused ⇒ nothing is sent at all). Defaults to `true`,
 * which is what every other journey in this file already assumed.
 */
let confirmAnswer = true;
const confirmSpy = vi.fn(async (_options: ConfirmDialogOptions) => confirmAnswer);
bindConfirmDialog(() => [confirmSpy, null]);
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

function leaf(key: string, keyType = 'string', ttl = -1) {
  return { kind: 'key', key, keyType, ttl, logicalLen: 4, memBytes: null };
}

function folder(prefix: string, count: number) {
  return { kind: 'folder', prefix, count };
}

function renderWorkbench() {
  render(<RedisWorkbench dbSessionId="sess-1" initialDatabase="db0" hideSidebar />);
}

/** Root-level `list_children` calls so a rescan/re-expand can be counted. */
function rootCalls(): number {
  return listChildren.mock.calls.filter((call) => String((call as unknown[])[2]) === '').length;
}

/** `list_children` calls for the `app:` subtree (re-expand must not add one). */
function childCalls(): number {
  return listChildren.mock.calls.filter((call) => String((call as unknown[])[2]) === 'app:').length;
}

/** Last `invokeScanKeys` call, as the pattern/cursor tuple it was made with. */
function lastScan(): { pattern: string; cursor: number; opts: Record<string, unknown> } {
  const call = scanKeys.mock.calls.at(-1) as unknown[];
  return {
    pattern: String(call[2]),
    cursor: Number(call[3]),
    opts: (call[5] ?? {}) as Record<string, unknown>,
  };
}

function emptyState(): string | null {
  return screen.queryByTestId('redis-tree-empty')?.getAttribute('data-empty-state') ?? null;
}

/**
 * Header actions render through `@datazen/ui` `Button`, i.e. a real `<button>`;
 * testing-library only knows them as `HTMLElement`, and `disabled` is asserted
 * on them. Narrowing here keeps the assertions on the real DOM property.
 */
function actionButton(testId: string): HTMLButtonElement {
  return screen.getByTestId(testId) as HTMLButtonElement;
}

/** The page `data-empty-state` must show right now. */
async function expectEmptyState(state: string): Promise<void> {
  await waitFor(() => expect(emptyState()).toBe(state));
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  confirmAnswer = true;
  // Default flat scan: both keys on one finished page (cursor drained ⇒ the
  // counter is exact and `scanning` never masks an empty state by accident).
  scanKeys.mockImplementation(async () => ({
    keys: [
      { key: 'app:user:1', keyType: 'string', ttl: -1, size: 4, preview: '' },
      { key: 'app:user:2', keyType: 'hash', ttl: 60, size: 4, preview: '' },
    ],
    cursor: 0,
    dbSize: 2,
    matched: 2,
  }));
  // Server-driven tree: root folds `app:`; the subtree holds both keys.
  listChildren.mockImplementation(async (...args: unknown[]) => {
    const prefix = String(args[2]);
    if (prefix === 'app:') return { children: [leaf('app:user:1'), leaf('app:user:2')], cursor: 0 };
    return { children: [folder('app:', 2), leaf('root-plain')], cursor: 0 };
  });
  dbSizes.mockResolvedValue([
    { db: 0, keys: 2 },
    { db: 1, keys: 0 },
  ]);
  getKey.mockResolvedValue({
    key: 'app:user:1',
    keyType: 'string',
    ttl: -1,
    value: 'v',
    size: 1,
    memory: null,
  });
  // Batch driver command: one of the two requested keys is rejected by ACL.
  redisCommand.mockImplementation(async (_driver: string, command: string) => {
    if (command === 'batch_set_ttl') {
      return {
        updated: 1,
        errors: [
          { key: 'app:user:1', error: 'NOPERM this user has no permissions to run this command' },
        ],
      };
    }
    return undefined;
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('row spec + sticky group headers (D-4)', () => {
  it('indents 4+depth×10 and pins the ancestor chain while scrolling', async () => {
    renderWorkbench();
    fireEvent.click(await screen.findByTestId('redis-tree-folder-app:'));
    await screen.findByTestId('redis-key-row-app:user:1');

    // Row spec published as data, not geometry: depth 0 ⇒ 4px, depth 1 ⇒ 14px.
    const folderRow = screen.getByTestId('redis-tree-folder-app:');
    expect(folderRow.getAttribute('data-depth')).toBe('0');
    expect(folderRow.getAttribute('data-indent')).toBe('4');
    const child = screen.getByTestId('redis-key-row-app:user:1');
    expect(child.getAttribute('data-depth')).toBe('1');
    expect(child.getAttribute('data-indent')).toBe('14');

    // At the top nothing is pinned: the folder is still painted in flow.
    const tree = screen.getByTestId('redis-key-tree');
    expect(tree.getAttribute('data-sticky-depth')).toBe('0');
    expect(screen.queryByTestId('redis-tree-sticky-headers')).toBeNull();

    // One row down the folder is a strict ancestor of the top row ⇒ pinned.
    Object.defineProperty(tree, 'scrollTop', { value: 30, writable: true, configurable: true });
    fireEvent.scroll(tree);
    await waitFor(() => expect(tree.getAttribute('data-sticky-depth')).toBe('1'));
    expect(screen.getByTestId('redis-tree-sticky-headers')).toBeTruthy();
    const pinned = screen.getByTestId('redis-tree-sticky-folder-app:');
    expect(pinned.getAttribute('data-sticky-order')).toBe('0');
    expect(pinned.getAttribute('data-sticky-depth')).toBe('0');

    // Exit transition: back to the top, the pin is released.
    Object.defineProperty(tree, 'scrollTop', { value: 0, writable: true, configurable: true });
    fireEvent.scroll(tree);
    await waitFor(() => expect(tree.getAttribute('data-sticky-depth')).toBe('0'));
    expect(screen.queryByTestId('redis-tree-sticky-headers')).toBeNull();
  });

  it('cascades a folder check over the whole loaded key set, both ways', async () => {
    renderWorkbench();
    const folderCheck = (await screen.findByTestId(
      'redis-tree-folder-check-app:',
    )) as HTMLInputElement;
    expect(folderCheck.checked).toBe(false);

    // Enter: check the (collapsed) folder — cascade reads the full loaded set.
    fireEvent.click(folderCheck);
    await waitFor(() =>
      expect((screen.getByTestId('redis-tree-folder-check-app:') as HTMLInputElement).checked).toBe(
        true,
      ),
    );

    // Opening the subtree shows both leaf checks following the cascade.
    fireEvent.click(screen.getByTestId('redis-tree-folder-app:'));
    await screen.findByTestId('redis-key-row-app:user:1');
    expect(screen.getByTestId('redis-tree-key-check-app:user:1').getAttribute('data-checked')).toBe(
      'true',
    );
    expect(screen.getByTestId('redis-tree-key-check-app:user:2').getAttribute('data-checked')).toBe(
      'true',
    );

    // Exit: unchecking the folder releases exactly its subtree.
    fireEvent.click(screen.getByTestId('redis-tree-folder-check-app:'));
    await waitFor(() =>
      expect((screen.getByTestId('redis-tree-folder-check-app:') as HTMLInputElement).checked).toBe(
        false,
      ),
    );
    expect(screen.getByTestId('redis-tree-key-check-app:user:1').getAttribute('data-checked')).toBe(
      'false',
    );
    expect(screen.getByTestId('redis-tree-key-check-app:user:2').getAttribute('data-checked')).toBe(
      'false',
    );
  });
});

describe('level-preserving refresh journey (I-4 / D-5)', () => {
  it('keeps the rows on screen while the refresh is in flight, then lands the new count', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    expect(screen.getByTestId('redis-tree-folder-count-app:').getAttribute('data-count')).toBe('2');

    // Hold the next root fetch hostage so the intermediate state is observable.
    let release: (value: { children: unknown[]; cursor: number }) => void = () => {};
    listChildren.mockImplementationOnce(
      () =>
        new Promise<{ children: unknown[]; cursor: number }>((resolve) => {
          release = resolve;
        }),
    );

    const rootsBefore = rootCalls();
    fireEvent.click(screen.getByTestId('redis-tree-refresh'));
    await waitFor(() => expect(rootCalls()).toBe(rootsBefore + 1));

    // Intermediate state of a rescan: the old level stays visible (no blank
    // flash, no forced collapse) while the replacement fetch is pending.
    expect(screen.getByTestId('redis-tree-folder-app:')).toBeTruthy();
    expect(screen.getByTestId('redis-tree-folder-count-app:').getAttribute('data-count')).toBe('2');

    release({ children: [folder('app:', 5)], cursor: 0 });
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-folder-count-app:').getAttribute('data-count')).toBe(
        '5',
      ),
    );

    // The rescan asked a finished level from cursor 0 with the same separator.
    const rescan = listChildren.mock.calls.at(-1) as unknown[];
    expect(rescan[2]).toBe('');
    expect(rescan[3]).toBe(0);
    expect(((rescan[5] ?? {}) as { sep?: string }).sep).toBe(':');
  });

  it('folds an expanded folder and re-expands it from the retained level (no refetch)', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-app:');
    fireEvent.click(screen.getByTestId('redis-tree-folder-app:'));
    await screen.findByTestId('redis-key-row-app:user:1');
    expect(childCalls()).toBe(1);

    // Collapse keeps the level …
    fireEvent.click(screen.getByTestId('redis-tree-folder-app:'));
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-folder-app:').getAttribute('data-expanded')).toBe(
        'false',
      ),
    );
    expect(screen.queryByTestId('redis-key-row-app:user:1')).toBeNull();

    // … so re-expanding renders from memory instead of hitting the server.
    fireEvent.click(screen.getByTestId('redis-tree-folder-app:'));
    expect(await screen.findByTestId('redis-key-row-app:user:1')).toBeTruthy();
    expect(childCalls()).toBe(1);
  });
});

describe('keyboard navigation journey (I-9 / D-7)', () => {
  it('walks expand/step/fold/activate and the chords as one state machine', async () => {
    renderWorkbench();
    const tree = await screen.findByTestId('redis-key-tree');
    await screen.findByTestId('redis-tree-folder-app:');
    expect(tree.getAttribute('data-active-index')).toBe('-1');

    // ↓ enters the list on the first row.
    fireEvent.keyDown(tree, { key: 'ArrowDown' });
    expect(tree.getAttribute('data-active-index')).toBe('0');
    expect(screen.getByTestId('redis-tree-folder-app:').getAttribute('data-active')).toBe('true');
    // ↓ reaches the sibling key row, ↑ clamps back.
    fireEvent.keyDown(tree, { key: 'ArrowDown' });
    expect(tree.getAttribute('data-active-index')).toBe('1');
    expect(screen.getByTestId('redis-key-row-root-plain').getAttribute('data-active')).toBe('true');
    fireEvent.keyDown(tree, { key: 'ArrowUp' });
    expect(tree.getAttribute('data-active-index')).toBe('0');

    // → on a collapsed folder expands it (one fetch for the subtree) …
    fireEvent.keyDown(tree, { key: 'ArrowRight' });
    expect(screen.getByTestId('redis-tree-folder-app:').getAttribute('data-expanded')).toBe('true');
    await screen.findByTestId('redis-key-row-app:user:1');
    expect(childCalls()).toBe(1);
    // … a second → steps into the first child …
    fireEvent.keyDown(tree, { key: 'ArrowRight' });
    expect(tree.getAttribute('data-active-index')).toBe('1');
    expect(screen.getByTestId('redis-key-row-app:user:1').getAttribute('data-active')).toBe('true');
    // … ← returns to the parent folder …
    fireEvent.keyDown(tree, { key: 'ArrowLeft' });
    expect(tree.getAttribute('data-active-index')).toBe('0');
    // … the next ← folds it (exit of expand).
    fireEvent.keyDown(tree, { key: 'ArrowLeft' });
    expect(screen.getByTestId('redis-tree-folder-app:').getAttribute('data-expanded')).toBe(
      'false',
    );
    expect(screen.queryByTestId('redis-key-row-app:user:1')).toBeNull();

    // → re-expands WITHOUT refetching: the level was retained (I-4).
    fireEvent.keyDown(tree, { key: 'ArrowRight' });
    expect(await screen.findByTestId('redis-key-row-app:user:1')).toBeTruthy();
    expect(childCalls()).toBe(1);

    // Enter activates the key row → its detail loads and mounts as selected.
    fireEvent.keyDown(tree, { key: 'ArrowDown' });
    expect(tree.getAttribute('data-active-index')).toBe('1');
    fireEvent.keyDown(tree, { key: 'Enter' });
    await waitFor(() =>
      expect(screen.getByTestId('redis-key-row-app:user:1').getAttribute('data-selected')).toBe(
        'true',
      ),
    );
    expect(getKey.mock.calls.some((call) => String((call as unknown[])[2]) === 'app:user:1')).toBe(
      true,
    );

    // ⌘A selects everything loaded.
    fireEvent.keyDown(tree, { key: 'a', metaKey: true });
    await waitFor(() =>
      expect(
        screen.getByTestId('redis-tree-key-check-app:user:1').getAttribute('data-checked'),
      ).toBe('true'),
    );
    expect(screen.getByTestId('redis-tree-key-check-app:user:2').getAttribute('data-checked')).toBe(
      'true',
    );
    expect((screen.getByTestId('redis-tree-folder-check-app:') as HTMLInputElement).checked).toBe(
      true,
    );

    // Esc is the exit transition: checks cleared, active row left behind.
    fireEvent.keyDown(tree, { key: 'Escape' });
    await waitFor(() =>
      expect(
        screen.getByTestId('redis-tree-key-check-app:user:1').getAttribute('data-checked'),
      ).toBe('false'),
    );
    expect(tree.getAttribute('data-active-index')).toBe('-1');

    // ⌘R refreshes both feeds: the flat scan and the tree root rescan.
    const rootsBefore = rootCalls();
    const scansBefore = scanKeys.mock.calls.length;
    fireEvent.keyDown(tree, { key: 'r', metaKey: true });
    await waitFor(() => expect(rootCalls()).toBe(rootsBefore + 1));
    expect(scanKeys.mock.calls.length).toBe(scansBefore + 1);
  });
});

describe('named empty states journey (I-11 / D-8)', () => {
  it('walks none → no-match → none as the pattern crosses the empty states', async () => {
    listChildren.mockResolvedValue({ children: [], cursor: 0 });
    scanKeys.mockImplementation(async () => ({
      keys: [],
      cursor: 0,
      dbSize: 0,
      matched: 0,
    }));
    renderWorkbench();
    await expectEmptyState('none');

    // Typing narrows the fact: the filter, not the server, is to blame.
    const input = await screen.findByTestId('redis-search-input');
    fireEvent.change(input, { target: { value: 'zzz' } });
    await expectEmptyState('no-match');

    // Enter applies: the scan really ran with the resolved prefix pattern …
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(lastScan().pattern).toBe('zzz*'));
    await expectEmptyState('no-match');

    // … and Esc (the row's exit) drops the filter back to the whole keyspace.
    fireEvent.keyDown(input, { key: 'Escape' });
    await expectEmptyState('none');
  });

  it('names an open cursor as interrupted and returns to none once drained', async () => {
    listChildren.mockResolvedValue({ children: [], cursor: 0 });
    scanKeys.mockImplementation(async (...args: unknown[]) => {
      const cursor = Number(args[3]);
      return { keys: [], cursor: cursor === 0 ? 7 : 0, dbSize: 0, matched: 0 };
    });
    renderWorkbench();
    // Rows are empty but the cursor never returned 0 ⇒ "absence proves nothing".
    await expectEmptyState('interrupted');

    // Paging the open cursor to its end is the exit: back to `none`.
    fireEvent.click(screen.getByTestId('redis-tree-load-more'));
    await waitFor(() => expect(lastScan().cursor).toBe(7));
    await expectEmptyState('none');
  });

  it('names a rejected root as no-permission, outranking an open cursor', async () => {
    // The flat scan stays open (cursor 7) — the precedence contract: a failed
    // root outranks everything, including the `interrupted` it would otherwise
    // resolve to.
    listChildren.mockRejectedValue(new Error('NOPERM this user has no permissions'));
    scanKeys.mockImplementation(async () => ({
      keys: [],
      cursor: 7,
      dbSize: 0,
      matched: 0,
    }));
    renderWorkbench();
    await expectEmptyState('no-permission');
  });
});

describe('row delete journeys (I-6 write gate, D-0 extraction)', () => {
  it('deletes a single key: confirm → gate → write → text summary → refresh', async () => {
    listChildren.mockResolvedValue({
      children: [leaf('app:user:1'), leaf('app:user:2')],
      cursor: 0,
    });
    // The single-key invoke answers the raw count (see `invokeDeleteKeys`).
    redisCommand.mockImplementation(async (_driver: string, command: string) => {
      if (command === 'delete_keys') return 1;
      return undefined;
    });
    renderWorkbench();
    await screen.findByTestId('redis-tree-key-delete-app:user:1');

    const rootsBefore = rootCalls();
    fireEvent.click(screen.getByTestId('redis-tree-key-delete-app:user:1'));

    // Confirm auto-approves in this harness and safe mode is off, so the write
    // goes out and the banner reports the row action as a plain text summary.
    const banner = await screen.findByTestId('redis-batch-summary');
    expect(banner.getAttribute('data-kind')).toBe('text');
    expect(
      redisCommand.mock.calls.some((call) => String((call as unknown[])[1]) === 'delete_keys'),
    ).toBe(true);
    // Refresh-after-success re-roots the tree (the gone row drops out).
    await waitFor(() => expect(rootCalls()).toBeGreaterThan(rootsBefore));

    // Exit transition of the banner.
    fireEvent.click(screen.getByTestId('redis-batch-summary-dismiss'));
    await waitFor(() => expect(screen.queryByTestId('redis-batch-summary')).toBeNull());
  });

  it('deletes a whole folder subtree through the pattern invoke', async () => {
    redisCommand.mockImplementation(async (_driver: string, command: string) => {
      if (command === 'batch_delete_pattern') return { deleted: 2, errors: [] };
      return undefined;
    });
    renderWorkbench();
    await screen.findByTestId('redis-tree-folder-delete-app:');

    const rootsBefore = rootCalls();
    // The folder delete button stops propagation: only the subtree delete runs.
    fireEvent.click(screen.getByTestId('redis-tree-folder-delete-app:'));

    const banner = await screen.findByTestId('redis-batch-summary');
    expect(banner.getAttribute('data-kind')).toBe('text');
    const call = redisCommand.mock.calls.find(
      (item) => String((item as unknown[])[1]) === 'batch_delete_pattern',
    );
    expect(call).toBeTruthy();
    expect(((call as unknown[])[2] as { pattern: string }).pattern).toBe('app:*');
    await waitFor(() => expect(rootCalls()).toBeGreaterThan(rootsBefore));
    expect(screen.queryByTestId('redis-batch-ttl-confirm')).toBeNull();
  });
});

/* ── R1 select-all slot morphs into the delete action ────────────────────── */

describe('R1 the select-all button becomes the delete button over the selection', () => {
  /** The identity R1's single action slot is wearing right now. */
  function actionSlot(): { testId: string; labelKey: string } {
    const el =
      screen.queryByTestId('redis-tree-select-all') ??
      screen.queryByTestId('redis-tree-delete-selected');
    if (!el) throw new Error('[journey] R1 has no action button at all');
    return {
      testId: el.getAttribute('data-testid') as string,
      labelKey: el.getAttribute('data-action-label-key') as string,
    };
  }

  /** `delete_keys` payloads, in call order. */
  function deleteCalls(): string[][] {
    return redisCommand.mock.calls
      .filter((c) => String((c as unknown[])[1]) === 'delete_keys')
      .map((c) => ((c as unknown[])[2] as { keys: string[] }).keys);
  }

  it('walks select-all → delete → summary → back to select-all, one step at a time', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');

    // 1. Resting state: the slot is select-all, and it is enabled.
    expect(actionSlot()).toEqual({
      testId: 'redis-tree-select-all',
      labelKey: 'redis.tree.selectAll',
    });
    expect(actionButton('redis-tree-select-all').disabled).toBe(false);

    // 2. Select all loaded — the very next paint is the delete action.
    fireEvent.click(screen.getByTestId('redis-tree-select-all'));
    await waitFor(() => expect(screen.getByTestId('redis-tree-delete-selected')).toBeTruthy());
    expect(actionSlot().labelKey).toBe('redis.deleteSelected');
    // The select-all identity is *gone*, not merely hidden: one slot, and it
    // is the delete one now.
    expect(screen.queryByTestId('redis-tree-select-all')).toBeNull();

    // 3. Both loaded keys are the payload — the same set the button selected.
    fireEvent.click(screen.getByTestId('redis-tree-delete-selected'));
    await waitFor(() => expect(deleteCalls()).toHaveLength(1));
    expect(deleteCalls()[0].sort()).toEqual(['app:user:1', 'app:user:2']);

    // 4. The write reports back through the shared banner, in its text form.
    const banner = await screen.findByTestId('redis-batch-summary');
    expect(banner.getAttribute('data-kind')).toBe('text');

    // 5. The refresh dropped the selection, so the slot reverted by itself.
    await waitFor(() => expect(screen.getByTestId('redis-tree-select-all')).toBeTruthy());
    expect(screen.queryByTestId('redis-tree-delete-selected')).toBeNull();
  });

  it('a hand-ticked row arms the same delete action (it keys off the selection, not the button)', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    // Expand the folder first — a leaf row only exists to tick once its
    // subtree is on screen.
    fireEvent.click(screen.getByTestId('redis-tree-folder-app:'));
    const leaf = await screen.findByTestId('redis-tree-key-check-app:user:1');
    fireEvent.click(leaf);
    await waitFor(() => expect(screen.getByTestId('redis-tree-delete-selected')).toBeTruthy());

    // Un-ticking the last row disarms it again — the button tracks the state,
    // it does not latch.
    fireEvent.click(leaf);
    await waitFor(() => expect(screen.getByTestId('redis-tree-select-all')).toBeTruthy());
    expect(deleteCalls()).toHaveLength(0);
  });

  it('Esc leaves the delete state without deleting anything', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    fireEvent.click(screen.getByTestId('redis-tree-select-all'));
    await waitFor(() => expect(screen.getByTestId('redis-tree-delete-selected')).toBeTruthy());

    // `Escape` is treeNavAction's `clear` chord — the exit transition.
    fireEvent.keyDown(screen.getByTestId('redis-key-tree'), { key: 'Escape' });
    await waitFor(() => expect(screen.getByTestId('redis-tree-select-all')).toBeTruthy());
    // Refused ⇒ not even the confirm was shown: no write was ever on the table.
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(deleteCalls()).toHaveLength(0);
  });

  it('a refused confirm sends nothing and keeps the selection armed', async () => {
    confirmAnswer = false;
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    fireEvent.click(screen.getByTestId('redis-tree-select-all'));
    await waitFor(() => expect(screen.getByTestId('redis-tree-delete-selected')).toBeTruthy());

    fireEvent.click(screen.getByTestId('redis-tree-delete-selected'));
    await waitFor(() => expect(confirmSpy).toHaveBeenCalled());
    // Identity `t` ⇒ the message equals its i18n key. The count is interpolated
    // into that string, so asserting on the key (not on prose) is what this
    // file's policy allows; `en.ts` owns whether `{count}` is in it.
    const arg: ConfirmDialogOptions = confirmSpy.mock.calls[0][0];
    expect(arg.message).toBe('redis.deleteSelectedConfirm');
    expect(arg.kind).toBe('warning');

    expect(deleteCalls()).toHaveLength(0);
    expect(screen.getByTestId('redis-tree-delete-selected')).toBeTruthy();
  });

  it('the value scope parks the delete action disabled, selection intact', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    fireEvent.click(screen.getByTestId('redis-tree-select-all'));
    await waitFor(() => expect(screen.getByTestId('redis-tree-delete-selected')).toBeTruthy());

    // Switching scope only re-roots the column; it does not apply a search, so
    // it does not run I-1 and the selection survives. The ticked rows are just
    // not on screen, which is why the action is disabled rather than hidden:
    // the selection is real, it just has no tree to delete from right now.
    fireEvent.click(screen.getByTestId('redis-search-mode-value'));
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-header').getAttribute('data-search-mode')).toBe(
        'value',
      ),
    );
    const parked = actionButton('redis-tree-delete-selected');
    expect(parked.disabled).toBe(true);
    expect(deleteCalls()).toHaveLength(0);

    // Back in the key scope it is live again — the selection outlived the trip.
    fireEvent.click(screen.getByTestId('redis-search-mode-key'));
    await waitFor(() => expect(actionButton('redis-tree-delete-selected').disabled).toBe(false));
  });
});

/*
 * R1's counter counts *scanned keys*, so it only means anything while the key
 * scope owns the column. The value scope's own status bar reports the value
 * search instead; the two must never both be on screen claiming the column.
 */
describe('the key counter yields to the scope that owns the column', () => {
  it('drops the key counter for the value scope, which brings its own', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    // Key scope: the counter is present and describes the key scan.
    await waitFor(() => expect(screen.getByTestId('redis-tree-count')).toBeTruthy());

    fireEvent.click(screen.getByTestId('redis-search-mode-value'));
    await waitFor(() =>
      expect(screen.getByTestId('redis-tree-header').getAttribute('data-search-mode')).toBe(
        'value',
      ),
    );
    // Gone — not stale, not zero: a key-scan count next to value-search numbers
    // is a contradiction, so the whole counter leaves.
    expect(screen.queryByTestId('redis-tree-count')).toBeNull();
    // And the value scope does own a count of its own (the i18n key, identity `t`).
    expect(screen.getByText('redis.search.hits')).toBeTruthy();
  });

  it('brings the counter back on the way home to the key scope', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    fireEvent.click(screen.getByTestId('redis-search-mode-value'));
    await waitFor(() => expect(screen.queryByTestId('redis-tree-count')).toBeNull());

    fireEvent.click(screen.getByTestId('redis-search-mode-key'));
    await waitFor(() => expect(screen.getByTestId('redis-tree-count')).toBeTruthy());
    expect(screen.queryByText('redis.search.hits')).toBeNull();
  });
});

/*
 * 模糊 is a modifier that only rewrites the pattern at apply time, while
 * 仅无过期 is a scan argument that re-issues on toggle. That asymmetry is fine
 * as long as the apply step is reachable without the keyboard — the button is
 * what makes 模糊 read as a pending change rather than a dead chip.
 */
describe('R2 apply is reachable by pointer, not only by Enter', () => {
  it('the fuzzy chip alone changes nothing; the apply button spends it', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    await waitFor(() => expect(lastScan().pattern).toBe('*'));
    const before = scanKeys.mock.calls.length;

    const input = screen.getByTestId('redis-search-input');
    fireEvent.change(input, { target: { value: 'user' } });
    fireEvent.click(screen.getByTestId('redis-tree-chip-fuzzy'));
    expect(screen.getByTestId('redis-tree-chip-fuzzy').getAttribute('data-active')).toBe('on');
    // The modifier is armed but unspent: no rescan, and the input still holds the
    // literal the user typed.
    expect(scanKeys.mock.calls.length).toBe(before);
    expect((input as HTMLInputElement).value).toBe('user');

    // The button is the missing trigger: it wraps the literal and scans.
    fireEvent.click(screen.getByTestId('redis-search-apply'));
    await waitFor(() => expect(lastScan().pattern).toBe('*user*'));
  });

  it('applies what is already in the box with no chip touched', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    const before = scanKeys.mock.calls.length;
    fireEvent.change(screen.getByTestId('redis-search-input'), { target: { value: 'app' } });
    expect(scanKeys.mock.calls.length).toBe(before);

    fireEvent.click(screen.getByTestId('redis-search-apply'));
    await waitFor(() => expect(lastScan().pattern).toBe('app*'));
  });
});

/*
 * The reported defect: typing `app` and pressing Enter listed nothing, because
 * the pattern reached `SCAN … MATCH` as a bare `app` — a glob that admits only
 * the single key spelled `app`, so `app:cache` and the rest of the namespace were
 * invisible. A literal now means prefix.
 */
describe('a typed literal searches by prefix', () => {
  /** Prefixes handed to the server's `list_children`, in order. */
  function treePrefixes(): string[] {
    return listChildren.mock.calls.map((call) => String((call as unknown[])[2]));
  }

  it('sends `app*` and narrows the tree walk to the `app` namespace', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    await waitFor(() => expect(lastScan().pattern).toBe('*'));

    fireEvent.change(screen.getByTestId('redis-search-input'), { target: { value: 'app' } });
    fireEvent.keyDown(screen.getByTestId('redis-search-input'), { key: 'Enter' });

    // The scan asks for the prefix, not for the one key named `app`.
    await waitFor(() => expect(lastScan().pattern).toBe('app*'));
    // …and the tree narrows with it: `patternToTreePrefix` cuts the trailing
    // star off, so the server walks `app*` instead of the whole keyspace. A bare
    // `app` would also have narrowed, but only ever to keys that do not exist
    // under it — which is the whole bug.
    await waitFor(() => expect(treePrefixes()).toContain('app'));
  });

  it('a hand-written glob is still honoured, and a trailing star is not doubled', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    fireEvent.change(screen.getByTestId('redis-search-input'), { target: { value: 'app:*' } });
    fireEvent.keyDown(screen.getByTestId('redis-search-input'), { key: 'Enter' });

    await waitFor(() => expect(lastScan().pattern).toBe('app:*'));
  });

  it('fuzzy still widens to a substring match, and beats prefix', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    const input = screen.getByTestId('redis-search-input');
    fireEvent.click(screen.getByTestId('redis-tree-chip-fuzzy'));
    fireEvent.change(input, { target: { value: 'app' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => expect(lastScan().pattern).toBe('*app*'));
    // A pattern that opens with a star gives the tree no literal head to route
    // on, so the walk falls back to the whole keyspace — by design (the server
    // may return anything, the client filter cuts it down).
    await waitFor(() => expect(treePrefixes()).toContain(''));
  });

  it('Esc leaves the prefix state entirely rather than leaving a star behind', async () => {
    renderWorkbench();
    await screen.findByTestId('redis-key-tree');
    const input = screen.getByTestId('redis-search-input');
    fireEvent.change(input, { target: { value: 'app' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await waitFor(() => expect(lastScan().pattern).toBe('app*'));

    fireEvent.keyDown(input, { key: 'Escape' });
    // Back to the unfiltered tree, and the box is empty — not holding `app*`.
    await waitFor(() => expect(lastScan().pattern).toBe('*'));
    expect(input).toHaveValue('');
  });
});
