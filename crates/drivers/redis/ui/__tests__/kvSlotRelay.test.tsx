/**
 * KV slot relay tests (contract F-2, PRD §4 I-1 dirty signal).
 *
 * The host renders the KV slots in a different React subtree from the
 * workbench, so the only channel between them is the per-panel `KvSlotState`
 * atom: the workbench *publishes* selection + dirtiness, the slots *read*.
 * These tests pin the publish side, which is what this track owns.
 *
 * Assertion policy (PRD §7-6 / ruling 8-4, now a human-review standard because
 * the automated guard was withdrawn):
 *   - `useI18n` is stubbed with an identity `t` purely so rendering works;
 *   - no assertion reads rendered copy. Locators are `data-testid`, state is
 *     read from `data-*` attributes or from the relay atom itself.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSchemaStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type KvSlotState,
  type SchemaStoreState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

// jsdom has no layout: a virtualizing list measures a zero-size scroll element
// and mounts no rows at all, which would make "click a key row" untestable. The
// virtualizer is therefore replaced by a pass-through that yields every item —
// the row markup, its `data-testid` and its click handler stay production code.
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

// Several Radix surfaces observe their anchor; jsdom ships no ResizeObserver.
class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
// eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
globalThis.ResizeObserver ??= MockResizeObserver as unknown as typeof ResizeObserver;

// Identity `t()`: the rendered string equals the i18n key, so a test that ever
// needed copy would fail loudly rather than silently pin a translation.
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key }),
}));

const getKey = vi.fn();
const getKeyRaw = vi.fn();
const scanKeys = vi.fn();
const listChildren = vi.fn();
const dbSizes = vi.fn();
const setString = vi.fn();

// The command seam: everything the browser mounts reaches Redis through these
// helpers, so overriding them keeps the components' own logic under test.
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  invokeGetKey: (...args: unknown[]) => getKey(...args),
  invokeGetKeyRaw: (...args: unknown[]) => getKeyRaw(...args),
  invokeScanKeys: (...args: unknown[]) => scanKeys(...args),
  invokeListChildren: (...args: unknown[]) => listChildren(...args),
  invokeDbSizes: (...args: unknown[]) => dbSizes(...args),
}));
vi.mock('../value-editors/keyEditorsInvokes', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../value-editors/keyEditorsInvokes')>()),
  invokeSetString: (...args: unknown[]) => setString(...args),
}));

import type { KeyDetail } from '../shared/types';
import { DetailColumn } from '../key-browser/DetailColumn';
import { RedisWorkbench, type RedisWorkbenchHandle } from '../key-browser/RedisWorkbench';
import { __resetDraftGuard } from '../shared/draftGuard';

// Harness capability bindings (the host injects the real ones at startup).
bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);
bindSchemaStore(
  create<SchemaStoreState>(() => ({
    schemas: new Map([
      ['sess-1', { pathItems: {}, databases: ['db0', 'db1'], loading: false }],
      ['sess-2', { pathItems: {}, databases: ['db0', 'db1'], loading: false }],
    ]),
    loadForConnection: async () => {},
    setLoadedTables: () => {},
    mergeNamespace: () => {},
    registerPathAliases: () => {},
    cachePathItems: () => {},
  })),
);

/**
 * Stand-in for the host's per-panel atom, recording what the driver published.
 * Deliberately a local copy of the `KvSlotState` behaviour: a driver test must
 * not import host internals (boundary guard R1), the contract is the type.
 */
function makeRelay(): KvSlotState & { published: Array<{ key: string | null; dirty: boolean }> } {
  const published: Array<{ key: string | null; dirty: boolean }> = [];
  const listeners = new Set<() => void>();
  let selectedKey: string | null = null;
  let dirty = false;
  const notify = () => {
    published.push({ key: selectedKey, dirty });
    for (const l of listeners) l();
  };
  return {
    published,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    getSelectedKey: () => selectedKey,
    selectKey(key) {
      if (key === selectedKey) return;
      selectedKey = key;
      notify();
    },
    getDirty: () => dirty,
    setDirty(next) {
      if (next === dirty) return;
      dirty = next;
      notify();
    },
    /*
     * W3-A widened members (status bar + context-bar scan cluster). The
     * workbench under test only drives the five original members, so these are
     * inert contract-fillers: nothing here reads them back.
     */
    getLoadedCount: () => 0,
    setLoadedCount: () => {},
    getScanCursor: () => '0',
    setScanCursor: () => {},
    isScanning: () => false,
    setScanning: () => {},
    getScanBudgetUsed: () => 0,
    getScanBudgetTotal: () => 0,
    setScanBudget: () => {},
    getSelectionCount: () => 0,
    setSelectionCount: () => {},
    getLastWriteCommand: () => null,
    getLastWriteDurationMs: () => null,
    recordWrite: () => {},
  };
}

function stringDetail(key: string, value = 'hello'): KeyDetail {
  return {
    key,
    keyType: 'string',
    ttl: -1,
    value,
    size: 1,
    memory: null,
  } as unknown as KeyDetail;
}

function detailColumn(overrides: Partial<React.ComponentProps<typeof DetailColumn>> = {}) {
  return render(
    <DetailColumn
      dbSessionId="sess-1"
      dbIndex={0}
      selectedKey="user:1"
      detail={stringDetail('user:1')}
      detailLoading={false}
      modules={[]}
      onRefresh={() => {}}
      onRenamed={() => {}}
      {...overrides}
    />,
  );
}

function stringEditor() {
  return screen.getByTestId('redis-string-editor');
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  // E-5: the I-1 draft guard is a module singleton — never let one case's
  // dirty flag or pending leave request bleed into the next.
  __resetDraftGuard();
});

beforeEach(() => {
  getKey.mockResolvedValue(stringDetail('user:1'));
  getKeyRaw.mockResolvedValue(null);
  scanKeys.mockResolvedValue({
    keys: [{ key: 'user:1', keyType: 'string' }],
    cursor: 0,
    dbSize: 1,
  });
  listChildren.mockResolvedValue({
    children: [
      { kind: 'key', key: 'user:1', keyType: 'string', ttl: -1, logicalLen: 1, memBytes: null },
    ],
    cursor: 0,
  });
  dbSizes.mockResolvedValue([{ db: 0, keys: 1 }]);
  setString.mockResolvedValue(undefined);
});

describe('DetailColumn → dirty signal (I-1 source)', () => {
  it('renders the named no-key state instead of an empty hole', () => {
    detailColumn({ selectedKey: null });
    const column = screen.getByTestId('redis-detail-column');
    expect(column.getAttribute('data-detail-state')).toBe('no-key');
    expect(screen.queryByTestId('redis-string-editor')).toBeNull();
  });

  it('reports a draft as dirty only after the value was edited', async () => {
    const onDirtyChange = vi.fn();
    detailColumn({ onDirtyChange });

    // E-2 (PRD §3.3): the editor is resident — there is no view/edit surface to
    // click into, so mounting must not itself count as a draft…
    expect(screen.queryByTestId('redis-string-mode-toggle')).toBeNull();
    expect(stringEditor().getAttribute('data-string-dirty')).toBe('false');
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);

    // …and the first keystroke is what turns the draft on.
    fireEvent.change(screen.getByTestId('redis-string-input'), { target: { value: 'edited' } });
    expect(stringEditor().getAttribute('data-string-dirty')).toBe('true');
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);
  });

  it('clears the dirty signal once the draft has been written', async () => {
    const onDirtyChange = vi.fn();
    detailColumn({ onDirtyChange });
    fireEvent.change(screen.getByTestId('redis-string-input'), { target: { value: 'edited' } });
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);

    fireEvent.click(screen.getByTestId('redis-string-save'));
    await waitFor(() => expect(stringEditor().getAttribute('data-string-dirty')).toBe('false'));
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
  });

  it('does not leak one key draft into another key (per-key editor remount)', async () => {
    const onDirtyChange = vi.fn();
    const { rerender } = detailColumn({ onDirtyChange });
    fireEvent.change(screen.getByTestId('redis-string-input'), {
      target: { value: 'draft-of-user-1' },
    });
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);

    getKey.mockResolvedValue(stringDetail('user:2', 'other'));
    rerender(
      <DetailColumn
        dbSessionId="sess-1"
        dbIndex={0}
        selectedKey="user:2"
        detail={stringDetail('user:2', 'other')}
        detailLoading={false}
        modules={[]}
        onRefresh={() => {}}
        onRenamed={() => {}}
        onDirtyChange={onDirtyChange}
      />,
    );

    const column = screen.getByTestId('redis-detail-column');
    expect(column.getAttribute('data-selected-key')).toBe('user:2');
    // Editing the new key starts from its own value, never the previous draft.
    expect(screen.getByTestId('redis-string-input')).toHaveValue('other');
    expect(stringEditor().getAttribute('data-string-dirty')).toBe('false');
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
  });

  it('unmounting the editor publishes a clean slate', () => {
    const onDirtyChange = vi.fn();
    const { unmount } = detailColumn({ onDirtyChange });
    fireEvent.change(screen.getByTestId('redis-string-input'), { target: { value: 'x' } });
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);
    unmount();
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
  });
});

describe('RedisWorkbench → host KV relay (contract F-2)', () => {
  function renderWorkbench(relay: KvSlotState) {
    const ref = { current: null } as { current: RedisWorkbenchHandle | null };
    render(
      <RedisWorkbench
        ref={ref as React.Ref<RedisWorkbenchHandle>}
        dbSessionId="sess-1"
        initialDatabase="db0"
        hideSidebar
        kvSlotState={relay}
      />,
    );
    return ref;
  }

  it('publishes the clicked key, and nothing else', async () => {
    const relay = makeRelay();
    renderWorkbench(relay);

    await waitFor(() => expect(screen.getByTestId('redis-detail-column')).toBeTruthy());
    fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));

    await waitFor(() => expect(relay.getSelectedKey()).toBe('user:1'));
    expect(relay.getDirty()).toBe(false);
  });

  it('mirrors an editor draft into the relay as dirty', async () => {
    const relay = makeRelay();
    renderWorkbench(relay);

    fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));
    await waitFor(() => expect(stringEditor()).toBeTruthy());
    fireEvent.change(screen.getByTestId('redis-string-input'), { target: { value: 'draft' } });

    await waitFor(() => expect(relay.getDirty()).toBe(true));
    expect(relay.getSelectedKey()).toBe('user:1');
  });

  it('clears the published key when the database is switched', async () => {
    const relay = makeRelay();
    const ref = renderWorkbench(relay);

    fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));
    await waitFor(() => expect(relay.getSelectedKey()).toBe('user:1'));

    act(() => {
      ref.current?.selectDatabase('db3');
    });
    await waitFor(() => expect(relay.getSelectedKey()).toBeNull());
    expect(relay.getDirty()).toBe(false);
  });

  it('intercepts a refresh while the draft is live, and only clears it on 放弃更改 (I-1)', async () => {
    const relay = makeRelay();
    const ref = renderWorkbench(relay);

    fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));
    await waitFor(() => expect(relay.getSelectedKey()).toBe('user:1'));
    fireEvent.change(screen.getByTestId('redis-string-input'), { target: { value: 'draft' } });
    await waitFor(() => expect(relay.getDirty()).toBe(true));

    // 进入条件：脏 + 刷新 ⇒ 对话框先出，刷新动作尚未执行——既不清 dirty 也不清选择
    // （旧缺陷：refreshKeys 直接静默清 dirty）。
    act(() => {
      void ref.current?.refreshKeys();
    });
    await screen.findByTestId('redis-draft-discard');
    expect(relay.getDirty()).toBe(true);
    expect(relay.getSelectedKey()).toBe('user:1');
    expect(stringEditor().getAttribute('data-string-dirty')).toBe('true');

    // 退出跃迁 A：继续编辑 ⇒ 动作取消，草稿与选择原样保留。
    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(screen.queryByTestId('redis-draft-discard')).toBeNull());
    expect(relay.getDirty()).toBe(true);
    expect(relay.getSelectedKey()).toBe('user:1');

    // 退出跃迁 B：放弃更改 ⇒ 草稿失效，刷新继续执行，脏与选择都落 false。
    act(() => {
      void ref.current?.refreshKeys();
    });
    await screen.findByTestId('redis-draft-discard');
    fireEvent.click(screen.getByTestId('redis-draft-discard'));
    await waitFor(() => expect(relay.getDirty()).toBe(false));
    expect(relay.getSelectedKey()).toBeNull();
    expect(screen.queryByTestId('redis-draft-discard')).toBeNull();
  });

  it('publishes a clean slate when the workbench unmounts', async () => {
    const relay = makeRelay();
    const { unmount } = render(
      <RedisWorkbench dbSessionId="sess-1" initialDatabase="db0" hideSidebar kvSlotState={relay} />,
    );
    fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));
    await waitFor(() => expect(relay.getSelectedKey()).toBe('user:1'));

    unmount();
    expect(relay.getSelectedKey()).toBeNull();
    expect(relay.getDirty()).toBe(false);
  });

  it('stays inert when the host supplies no relay (no KV capability declared)', async () => {
    const ref = { current: null } as { current: RedisWorkbenchHandle | null };
    render(
      <RedisWorkbench
        ref={ref as React.Ref<RedisWorkbenchHandle>}
        dbSessionId="sess-1"
        initialDatabase="db0"
        hideSidebar
      />,
    );
    // No `kvSlotState` prop must not break the browser: the tree still renders.
    fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));
    await waitFor(() =>
      expect(screen.getByTestId('redis-detail-column').getAttribute('data-selected-key')).toBe(
        'user:1',
      ),
    );
  });
});

/* ── [tester] added in the test round ──────────────────────────────────────────
 * The four dirty-exit paths the contract requires are 切键 / 关面板 / 切
 * `dbSessionId` / unmount. Mutation check: deleting the `dbSessionId` entry from
 * the reset effect's dependency array left the whole suite green, i.e. that path
 * had no coverage at all; the two cases below close it, together with the
 * detail-column close button and the search field, which drop the selection
 * through code paths no earlier case exercised.
 */
describe('[tester] RedisWorkbench relay exit paths', () => {
  function renderWorkbench(relay: KvSlotState, dbSessionId = 'sess-1', hideSidebar = true) {
    const ref = { current: null } as { current: RedisWorkbenchHandle | null };
    const result = render(
      <RedisWorkbench
        ref={ref as React.Ref<RedisWorkbenchHandle>}
        dbSessionId={dbSessionId}
        initialDatabase="db0"
        hideSidebar={hideSidebar}
        kvSlotState={relay}
      />,
    );
    return { ref, rerender: result.rerender };
  }

  async function draftOnRelay(relay: KvSlotState) {
    fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));
    await waitFor(() => expect(stringEditor()).toBeTruthy());
    fireEvent.change(screen.getByTestId('redis-string-input'), { target: { value: 'draft' } });
    await waitFor(() => expect(relay.getDirty()).toBe(true));
    expect(relay.getSelectedKey()).toBe('user:1');
  }

  it('clears selection and dirty when the search field drops the selection', async () => {
    const relay = makeRelay();
    renderWorkbench(relay, 'sess-1', false);
    await draftOnRelay(relay);

    const input = screen.getByTestId('redis-search-input');
    fireEvent.change(input, { target: { value: 'user:*' } });
    fireEvent.keyDown(input, { key: 'Enter', code: 'Enter' });

    // I-1: Enter asks before the search wipes the draft…
    await screen.findByTestId('redis-draft-discard');
    expect(relay.getDirty()).toBe(true);
    expect(relay.getSelectedKey()).toBe('user:1');

    // …放弃更改 lets the search run and the stale state go.
    fireEvent.click(screen.getByTestId('redis-draft-discard'));
    await waitFor(() => expect(relay.getDirty()).toBe(false));
    expect(relay.getSelectedKey()).toBeNull();
  });

  it('publishes a clean slate when only the database session swaps', async () => {
    const relay = makeRelay();
    const { rerender } = renderWorkbench(relay);
    await draftOnRelay(relay);

    rerender(
      <RedisWorkbench dbSessionId="sess-2" initialDatabase="db0" hideSidebar kvSlotState={relay} />,
    );

    await waitFor(() => expect(relay.getSelectedKey()).toBeNull());
    expect(relay.getDirty()).toBe(false);
  });

  it('publishes a clean slate when only the relay object is swapped', async () => {
    const first = makeRelay();
    const second = makeRelay();
    const { rerender } = renderWorkbench(first);
    await draftOnRelay(first);

    rerender(
      <RedisWorkbench
        dbSessionId="sess-1"
        initialDatabase="db0"
        hideSidebar
        kvSlotState={second}
      />,
    );

    // The dying relay must not keep a key the new panel does not show, and the
    // new relay must not inherit the draft either.
    await waitFor(() => expect(first.getSelectedKey()).toBeNull());
    expect(first.getDirty()).toBe(false);
    expect(second.getSelectedKey()).toBe('user:1');
    expect(second.getDirty()).toBe(true);
  });
});
