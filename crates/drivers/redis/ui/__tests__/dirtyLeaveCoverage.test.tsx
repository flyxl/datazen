/**
 * [tester] 第 1 轮复验补测：I-1 守卫中「变异撤掉后无人变红」的支路 + 遗留事项 2 专属断言。
 *
 * 零信任背景：对 HEAD `3919307ce` 做变异自证时发现，撤掉下列守卫/早退后全套 525 例
 * 仍全绿 —— 即这些分支当时**没有任何测试钉住**：
 *   M9  `onClearSelectedKey` 的守卫    M11 同库重点击的早退
 *   M13 头行改名的守卫                   M14 头行删除的守卫
 * 本文件把这些支路逐条钉住（**只加测试，不改生产码**），并按简报 §8-2 补
 * 「批量对话框 / 批量 TTL / 列设置」三处的专属 I-1 断言。
 *
 * 一条真缺陷由本文件揭出并登记为 `redis-detail-ui-BUG-001`（同键重点击静默毁草稿，
 * 无 I-1 弹层）。它的**正确期望**用例以 `describe.skip` 留在文件末尾：修复者改完
 * 取消跳过即可 —— 本轮不把已知错误行为钉成断言。
 *
 * 断言口径（PRD §7-6 / 裁定 8-4）：`data-testid` + `data-*` 定位；`useI18n` stub 成
 * identity `t`，故按钮/标题文本恰是 i18n **key**（断的是 key，不是英文文案）；无几何反查。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindContextMenuBridge,
  bindSchemaStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type NativeMenuItemDef,
  type SchemaStoreState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

/** Captures what `showNativeContextMenu` pushed to the (host) bridge. */
let menuItems: NativeMenuItemDef[] = [];
bindContextMenuBridge({
  show: (items) => {
    menuItems = items;
  },
  hide: () => {
    menuItems = [];
  },
});

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

// jsdom has no layout: pass every virtualized row through so rows mount.
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

/**
 * Single IPC seam. `batchInvokes` / `KeyWorkbenchDialogs` build their invoke helpers
 * on top of `redisCommandInvoke` (a cross-module import, so it IS interceptable)
 * — mocking the helper modules themselves would miss their internal calls.
 */
const commands = vi.fn(
  async (
    _pluginId: string,
    command: string,
    _params: Record<string, unknown>,
  ): Promise<unknown> => {
    switch (command) {
      case 'delete_keys':
        return 1;
      case 'batch_set_ttl':
        return { updated: 1, errors: [] };
      case 'batch_delete_pattern':
        return { deleted: 0, errors: [] };
      case 'batch_rename_prefix':
        return { renamed: 0, errors: [] };
      case 'count_matching':
        return 1;
      default:
        return undefined;
    }
  },
);

const getKey = vi.fn();
const getKeyRaw = vi.fn();
const scanKeys = vi.fn();
const listChildren = vi.fn();
const dbSizes = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  redisCommandInvoke: (...args: unknown[]) =>
    commands(args[0] as string, args[1] as string, (args[2] ?? {}) as Record<string, unknown>),
  invokeGetKey: (...args: unknown[]) => getKey(...args),
  invokeGetKeyRaw: (...args: unknown[]) => getKeyRaw(...args),
  invokeScanKeys: (...args: unknown[]) => scanKeys(...args),
  invokeListChildren: (...args: unknown[]) => listChildren(...args),
  invokeDbSizes: (...args: unknown[]) => dbSizes(...args),
}));

const setString = vi.fn();
const renameKey = vi.fn();
const deleteKey = vi.fn();
vi.mock('../value-editors/keyEditorsInvokes', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../value-editors/keyEditorsInvokes')>()),
  invokeSetString: (...args: unknown[]) => setString(...args),
  invokeRename: (...args: unknown[]) => renameKey(...args),
  invokeDeleteKey: (...args: unknown[]) => deleteKey(...args),
}));

import type { KeyDetail } from '../shared/types';
import { RedisWorkbench } from '../key-browser/RedisWorkbench';
import {
  __resetDraftGuard,
  isDraftDirty,
  isLeavePending,
  settleDraftLeave,
} from '../shared/draftGuard';

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);
bindSchemaStore(
  create<SchemaStoreState>(() => ({
    schemas: new Map([['sess-i1c', { pathItems: {}, databases: ['db0', 'db1'], loading: false }]]),
    loadForConnection: async () => {},
    setLoadedTables: () => {},
    mergeNamespace: () => {},
    registerPathAliases: () => {},
    cachePathItems: () => {},
  })),
);

function stringDetail(key: string, value: string): KeyDetail {
  return {
    key,
    keyType: 'string',
    ttl: -1,
    value,
    size: 1,
    memory: null,
  } as unknown as KeyDetail;
}

function renderWorkbench() {
  return render(<RedisWorkbench dbSessionId="sess-i1c" initialDatabase="db0" />);
}

const column = () => screen.getByTestId('redis-detail-column');
const editor = () => screen.getByTestId('redis-string-editor');
const input = () => screen.getByTestId('redis-string-input') as HTMLTextAreaElement;
const leaveDialog = () => screen.queryByTestId('redis-draft-discard');

/** Flush pending microtasks/timers so guard promises settle and effects run. */
async function flush(ms = 30) {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, ms));
  });
}

/** A dialog whose rendered title is this i18n key (identity `t` ⇒ title === key). */
async function dialogByTitle(titleKey: string): Promise<HTMLElement> {
  return waitFor(() => {
    const found = Array.from(document.querySelectorAll('[role="dialog"]')).find(
      (el) => el.textContent?.includes(titleKey) ?? false,
    );
    if (!found) throw new Error(`no dialog for ${titleKey}`);
    return found as HTMLElement;
  });
}

function buttonWithKey(container: HTMLElement, key: string): HTMLButtonElement {
  const found = Array.from(container.querySelectorAll('button')).find((b) => b.textContent === key);
  if (!found) throw new Error(`no button ${key} in dialog`);
  return found as HTMLButtonElement;
}

async function selectAndDraft(key = 'user:1', draft = 'draft') {
  fireEvent.click(await screen.findByTestId(`redis-key-row-${key}`));
  await waitFor(() => expect(column().getAttribute('data-selected-key')).toBe(key));
  await waitFor(() => expect(input()).toBeTruthy());
  fireEvent.change(input(), { target: { value: draft } });
  await waitFor(() => expect(editor().getAttribute('data-string-dirty')).toBe('true'));
}

beforeEach(() => {
  getKey.mockImplementation((...args: unknown[]) => {
    const key = args[2] as string;
    return Promise.resolve(stringDetail(key, key === 'user:1' ? 'hello' : 'other'));
  });
  getKeyRaw.mockResolvedValue(null);
  scanKeys.mockResolvedValue({
    keys: [
      { key: 'user:1', keyType: 'string' },
      { key: 'other:2', keyType: 'string' },
    ],
    cursor: 0,
    dbSize: 2,
  });
  listChildren.mockResolvedValue({
    children: [
      { kind: 'key', key: 'user:1', keyType: 'string', ttl: -1, logicalLen: 1, memBytes: null },
      { kind: 'key', key: 'other:2', keyType: 'string', ttl: -1, logicalLen: 1, memBytes: null },
    ],
    cursor: 0,
  });
  dbSizes.mockResolvedValue([{ db: 0, keys: 2 }]);
  menuItems = [];
  setString.mockResolvedValue(undefined);
  renameKey.mockResolvedValue(undefined);
  deleteKey.mockResolvedValue(undefined);
});

afterEach(() => {
  // Drain a still-pending leave request first so no caller's await is stranded in
  // a torn-down tree, then reset the module singleton.
  if (isLeavePending()) settleDraftLeave(false);
  cleanup();
  vi.clearAllMocks();
  __resetDraftGuard();
});

// ============================================================================
// A. 同库重点击的早退（钉住 M11）：不误弹，且异库照样被拦
// ============================================================================
describe('[tester] I-1 旁路：同库重点击不拦截，异库仍拦截', () => {
  it('re-clicking the selected database does not ask and keeps the draft', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.click(screen.getByTestId('redis-db-db0'));
    await flush();

    expect(leaveDialog()).toBeNull();
    expect(isLeavePending()).toBe(false);
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
    expect(input().value).toBe('draft');
    expect(column().getAttribute('data-selected-key')).toBe('user:1');

    // 早退只对同库成立：另一库照样被拦（守卫没被整体短路）。
    fireEvent.click(screen.getByTestId('redis-db-db1'));
    await screen.findByTestId('redis-draft-discard');
    expect(isLeavePending()).toBe(true);
    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
  });
});

// ============================================================================
// B. 头行改名 / 删除在脏时必须先问（钉住 M13 / M14）
// ============================================================================
describe('[tester] 头行改名与删除的 I-1 拦截', () => {
  it('renaming a dirty key asks first; 继续编辑 keeps draft and name, 放弃 lets RENAME out', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.click(screen.getByTestId('redis-header-rename'));
    const renameInput = screen.getByTestId('redis-header-rename-input') as HTMLInputElement;
    expect(renameInput.value).toBe('user:1');
    fireEvent.change(renameInput, { target: { value: 'user:renamed' } });
    fireEvent.click(screen.getByTestId('redis-header-rename-confirm'));

    // 守卫先弹：RENAME 未出网，选中键未变。
    await screen.findByTestId('redis-draft-discard');
    expect(renameKey).not.toHaveBeenCalled();
    expect(column().getAttribute('data-selected-key')).toBe('user:1');

    // 继续编辑 ⇒ 改名取消，草稿原样。
    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());
    expect(renameKey).not.toHaveBeenCalled();
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
    expect(input().value).toBe('draft');

    // 放弃 ⇒ 出网一次，脏落 false。
    fireEvent.click(screen.getByTestId('redis-header-rename-confirm'));
    await screen.findByTestId('redis-draft-discard');
    fireEvent.click(screen.getByTestId('redis-draft-discard'));
    await waitFor(() => expect(renameKey).toHaveBeenCalledOnce());
    expect(renameKey.mock.calls[0]).toEqual(['sess-i1c', 0, 'user:1', 'user:renamed']);
    expect(isDraftDirty()).toBe(false);
  });

  it('deleting a dirty key asks first and DELETE never leaves the client on 继续编辑', async () => {
    renderWorkbench();
    await selectAndDraft();

    // 绑定的确认框在 harness 里恒真 ⇒ 下一道门槛就是 I-1。
    fireEvent.click(screen.getByTestId('redis-header-delete'));
    await screen.findByTestId('redis-draft-discard');
    expect(deleteKey).not.toHaveBeenCalled();
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');

    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());
    expect(deleteKey).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('redis-header-delete'));
    await screen.findByTestId('redis-draft-discard');
    fireEvent.click(screen.getByTestId('redis-draft-discard'));
    await waitFor(() => expect(deleteKey).toHaveBeenCalledOnce());
    expect(deleteKey.mock.calls[0]).toEqual(['sess-i1c', 0, 'user:1']);
  });
});

// ============================================================================
// E. 遗留事项 2 · 「列设置」（类型过滤 / 含内存采样 / 仅无 TTL）
// ============================================================================
describe('[tester] 列设置过滤：重扫列表但不毁草稿，故不该吃守卫', () => {
  it('toggling the no-expiry filter re-scans yet keeps selection and draft without asking', async () => {
    renderWorkbench();
    await selectAndDraft();
    const scansBefore = scanKeys.mock.calls.length;

    // R2 renders this filter as a chip `<button>` (D-3), not a checkbox: the
    // "is it on" probe is `data-active`, exactly as `keyTreeJourney` asserts.
    const toggle = screen.getByTestId('redis-tree-chip-no-ttl');
    fireEvent.click(toggle);
    expect(toggle.getAttribute('data-active')).toBe('on');

    // 确实重扫（不是空转）。
    await waitFor(() => expect(scanKeys.mock.calls.length).toBeGreaterThan(scansBefore));
    // selection 未被丢弃 ⇒ 无数据损失 ⇒ 不该弹 I-1，也不该悬起守卫。
    expect(leaveDialog()).toBeNull();
    expect(isLeavePending()).toBe(false);
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
    expect(input().value).toBe('draft');
  });
});

// ============================================================================
// F. 写路径统一出口：创建键 ⇒ refreshKeys 也走守卫（防静默清 dirty）
// ============================================================================
// 实测结论（第 1 轮 Tester 变异/探针自证）：创建键成功后 `handleCreateKey` 调
// `onRefreshKeys()`（被拦 ⇒ 静默不刷新）**再** `onSelectKey(name)`，而后者接的是
// **未守卫的** `handleSelectKey` ⇒ 草稿被静默丢弃并跳到新键（`draftDirty=false`）。
// 这是第二条 I-1 旁路，登记为 redis-detail-ui-BUG-002；下面的期望用例先 skip，
// 修复者接上守卫后取消跳过即可复验。
describe('[redis-detail-ui-BUG-002] 创建键后的跳转不得静默丢草稿', () => {
  it('asks about the stale draft instead of silently clearing it', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.click(screen.getByTestId('redis-tree-create-key'));
    const dialog = await dialogByTitle('redis.createKey');
    const nameField = dialog.querySelector(
      'input[placeholder="redis.keyName"]',
    ) as HTMLInputElement;
    expect(nameField).toBeTruthy();
    fireEvent.change(nameField, { target: { value: 'new:key' } });
    fireEvent.click(buttonWithKey(dialog, 'redis.create'));

    // 创建 string 键 ⇒ `invokeCreateKey → set_string` 出网一次，
    // 随后 onRefreshKeys 必须撞到守卫（而不是静默清 dirty）。
    await waitFor(() =>
      expect(commands).toHaveBeenCalledWith(
        'redis',
        'set_string',
        expect.objectContaining({ key: 'new:key' }),
      ),
    );
    await screen.findByTestId('redis-draft-discard');
    expect(isDraftDirty()).toBe(true);

    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());
    expect(isDraftDirty()).toBe(true);
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
  });
});

// ============================================================================
// G. 对话框侧清空选中键（钉住 M9）
// ============================================================================
describe('[tester] 对话框侧删除选中键的 I-1 拦截', () => {
  it('a dialog-driven delete of the selected key asks before clearing it', async () => {
    renderWorkbench();
    await selectAndDraft();

    // 键右键 ⇒ 菜单项由绑定的桥捕获；点「delete」⇒ 确认框恒真 ⇒ 弹出删除对话框，
    // 确认后 delete_keys 出网，随后 onClearSelectedKey / onRefreshKeys 必须问。
    fireEvent.contextMenu(screen.getByTestId('redis-key-row-user:1'));
    await flush(0);
    const item = menuItems.find((i) => i.kind === 'item' && i.id === 'delete');
    expect(item).toBeTruthy();
    if (item && item.kind === 'item') {
      item.action();
      await flush(0);
    }
    const ctxDialog = await dialogByTitle('redis.confirmDeleteKeys');
    fireEvent.click(buttonWithKey(ctxDialog, 'common.delete'));

    await waitFor(() =>
      expect(commands).toHaveBeenCalledWith(
        'redis',
        'delete_keys',
        expect.objectContaining({ keys: ['user:1'] }),
      ),
    );
    await screen.findByTestId('redis-draft-discard');
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');

    // 连续出口：清空选中被拒后，同一条链上的 onRefreshKeys 还会再问一次 ⇒
    // 逐次「继续编辑」全部拒绝，草稿与选中必须原样存活（无死锁、无漏网）。
    for (let i = 0; i < 3; i += 1) {
      if (!leaveDialog()) break;
      fireEvent.click(screen.getByTestId('redis-draft-keep'));
      await flush(0);
    }
    expect(leaveDialog()).toBeNull();
    expect(isLeavePending()).toBe(false);
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
    expect(isDraftDirty()).toBe(true);
  });
});

// ============================================================================
// H2. 右键「设置 TTL」作用在选中键上 ⇒ 之后的 onSelectKey 走未守卫的原始回调
//     （redis-detail-ui-BUG-002 的第三条复现路径；本轮实测：草稿静默消失）
// ============================================================================
describe('[redis-detail-ui-BUG-002] 右键 TTL 作用于选中键后不得静默丢草稿', () => {
  it('refetches the selected key after a TTL apply and reports what happens to the draft', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.contextMenu(screen.getByTestId('redis-key-row-user:1'));
    await flush(0);
    const ttlItem = menuItems.find((i) => i.kind === 'item' && i.id === 'set-ttl');
    expect(ttlItem).toBeTruthy();
    if (ttlItem && ttlItem.kind === 'item') {
      ttlItem.action();
      await flush(0);
    }
    const ttlDialog = await dialogByTitle('redis.setTtl');
    const field = ttlDialog.querySelector('input') as HTMLInputElement;
    fireEvent.change(field, { target: { value: '120' } });
    fireEvent.click(buttonWithKey(ttlDialog, 'redis.setTtl'));

    await waitFor(() =>
      expect(commands).toHaveBeenCalledWith(
        'redis',
        'set_ttl',
        expect.objectContaining({ key: 'user:1', ttlSeconds: 120 }),
      ),
    );
    await flush(60);
    // 正确期望（现状不成立，故本用例 skip）：对话框被拒 ⇒ 后面的
    // `onSelectKey(selectedKey)` 走的是未守卫的原始回调，编辑器整块重挂 ⇒
    // 草稿静默消失。修复方向：`onSelectKey` 传守卫版，或让对话框的回答
    // 取消后续的选中键重取（见 bugs/redis-detail-ui-BUG-002.md）。
    expect(input().value).toBe('draft');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
    expect(isDraftDirty()).toBe(true);
    expect(leaveDialog()).toBeNull();
  });
});

// ============================================================================
// H. redis-detail-ui-BUG-001：同键重点击静默毁草稿 —— 正确期望，修复者取消 skip
// ============================================================================
describe('[redis-detail-ui-BUG-001] 同键重点击不得静默丢草稿', () => {
  it('a plain refetch of the already-selected key must pass I-1 like the header refresh does', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.click(screen.getByTestId('redis-key-row-user:1'));
    await flush();

    // 与 `redis-header-refresh` 同一语义（都是重取当前键）⇒ 要么先弹守卫，
    // 要么保留草稿；现状是「不弹 + 草稿蒸发」，两者都不成立。
    expect(input().value).toBe('draft');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
    expect(isDraftDirty()).toBe(true);
  });
});

// ============================================================================
// I. redis-detail-ui-BUG-002 修复自测：一次用户动作内守卫至多询问一次（排除双弹）
// ============================================================================
describe('[fix-selftest] 对话框出口一次动作只询问一次守卫', () => {
  it('creating a key asks exactly once — 继续编辑后同一条链不再二次弹出', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.click(screen.getByTestId('redis-tree-create-key'));
    const dialog = await dialogByTitle('redis.createKey');
    const nameField = dialog.querySelector(
      'input[placeholder="redis.keyName"]',
    ) as HTMLInputElement;
    expect(nameField).toBeTruthy();
    fireEvent.change(nameField, { target: { value: 'new:key' } });
    fireEvent.click(buttonWithKey(dialog, 'redis.create'));

    // 本次动作的唯一一次询问。
    await screen.findByTestId('redis-draft-discard');
    expect(isDraftDirty()).toBe(true);
    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());

    // 若同一条链上还有第二道守卫，它只能在无人交互的情况下重新挂出 ——
    // 长 flush 后必须依然没有弹窗、没有悬起的 pending、草稿与选中原样。
    await flush(60);
    expect(leaveDialog()).toBeNull();
    expect(isLeavePending()).toBe(false);
    expect(isDraftDirty()).toBe(true);
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(input().value).toBe('draft');
  });

  it('a dialog-driven delete of the selected key asks exactly once', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.contextMenu(screen.getByTestId('redis-key-row-user:1'));
    await flush(0);
    const item = menuItems.find((i) => i.kind === 'item' && i.id === 'delete');
    expect(item).toBeTruthy();
    if (item && item.kind === 'item') {
      item.action();
      await flush(0);
    }
    const ctxDialog = await dialogByTitle('redis.confirmDeleteKeys');
    fireEvent.click(buttonWithKey(ctxDialog, 'common.delete'));
    await waitFor(() =>
      expect(commands).toHaveBeenCalledWith(
        'redis',
        'delete_keys',
        expect.objectContaining({ keys: ['user:1'] }),
      ),
    );

    await screen.findByTestId('redis-draft-discard');
    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());
    await flush(60);
    expect(leaveDialog()).toBeNull();
    expect(isLeavePending()).toBe(false);
    expect(isDraftDirty()).toBe(true);
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
  });
});
