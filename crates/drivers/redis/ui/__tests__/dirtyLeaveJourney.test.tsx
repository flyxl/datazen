/**
 * I-1 dirty 拦截的连续旅程（本轨 E-5，PRD §4 I-1）。
 *
 * 覆盖的是「状态机」而不是单点：脏草稿存在后，**每一个**会毁掉草稿的动作
 * （切键 / 切 db / 刷新 / 头行刷新 / 保存后继续导航 / 切页签）都必须先弹
 * 「放弃更改 / 继续编辑」对话框——继续编辑 ⇒ 动作取消、草稿原样保留；
 * 放弃更改 ⇒ 草稿失效、动作继续。外加脏底栏自身的放弃动作与
 * `draftGuard` 模块单例的纯状态机跃迁。
 *
 * 断言口径（§4-6 / 裁定 8-4）：`data-testid` 定位、状态用 `data-*`，
 * 文案只断 i18n key，不读英文字面量、不做视口几何反查。
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

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

// jsdom has no layout: the virtualizing tree measures a zero-size scroller and
// mounts no rows at all. Pass every item through — row markup, `data-testid`s
// and click handlers stay production code (same harness as kvSlotRelay).
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

// jsdom ships no ResizeObserver; Radix surfaces observe their anchor.
class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
// eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
globalThis.ResizeObserver ??= MockResizeObserver as unknown as typeof ResizeObserver;

// 重型面板与 I-1 无关：切页签旅程只需要知道它们挂没挂载。
vi.mock('../console/RedisConsole', () => ({
  RedisConsole: () => <div data-testid="stub-console" />,
}));
vi.mock('../observe/PubSubPanel', () => ({
  PubSubPanel: () => <div data-testid="stub-pubsub" />,
}));
vi.mock('../observe/SlowlogPanel', () => ({
  SlowlogPanel: () => <div data-testid="stub-slowlog" />,
}));

const getKey = vi.fn();
const getKeyRaw = vi.fn();
const scanKeys = vi.fn();
const listChildren = vi.fn();
const dbSizes = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  invokeGetKey: (...args: unknown[]) => getKey(...args),
  invokeGetKeyRaw: (...args: unknown[]) => getKeyRaw(...args),
  invokeScanKeys: (...args: unknown[]) => scanKeys(...args),
  invokeListChildren: (...args: unknown[]) => listChildren(...args),
  invokeDbSizes: (...args: unknown[]) => dbSizes(...args),
}));
const setString = vi.fn();
vi.mock('../value-editors/keyEditorsInvokes', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../value-editors/keyEditorsInvokes')>()),
  invokeSetString: (...args: unknown[]) => setString(...args),
}));

import type { KeyDetail } from '../shared/types';
import { RedisWorkbench } from '../key-browser/RedisWorkbench';
import { resetRightTab } from '../shared/rightTabState';
import { RedisConnectionView } from '../connection/RedisConnectionView';
import { panelCloseStub } from '../__testing__/panelClose';
import { resetPanelBindings } from '../shared/panelLifecycle';
import {
  __resetDraftGuard,
  isDraftDirty,
  isLeavePending,
  publishDraftDirty,
  requestDraftLeave,
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
    schemas: new Map([['sess-i1', { pathItems: {}, databases: ['db0', 'db1'], loading: false }]]),
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
  return render(<RedisWorkbench dbSessionId="sess-i1" initialDatabase="db0" />);
}

function renderView() {
  return render(
    <RedisConnectionView
      panelId="panel-i1"
      onPanelClosed={panelClose.onPanelClosed}
      dbSessionId="sess-i1"
      connectionId="cfg-i1"
      connectionName="local"
      databaseType="redis"
      initialDatabase="db0"
      isActive
    />,
  );
}

const column = () => screen.getByTestId('redis-detail-column');
const editor = () => screen.getByTestId('redis-string-editor');
const draftInput = () => screen.getByTestId('redis-string-input') as HTMLTextAreaElement;
const dirtyBar = () => screen.queryByTestId('redis-string-dirty-bar');
const leaveDialog = () => screen.queryByTestId('redis-draft-discard');

/** 选中 user:1 并敲下一版草稿，直到中继脏信号为真。 */
async function selectAndDraft() {
  fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));
  await waitFor(() => expect(column().getAttribute('data-selected-key')).toBe('user:1'));
  await waitFor(() => expect(screen.getByTestId('redis-string-input')).toBeTruthy());
  fireEvent.change(draftInput(), { target: { value: 'draft' } });
  await waitFor(() => expect(editor().getAttribute('data-string-dirty')).toBe('true'));
}

// The view registers a close handler on mount; cases fire it explicitly.
const panelClose = panelCloseStub();

beforeEach(() => {
  panelClose.clear();
  // `panelLifecycle` binds per panelId at module scope, so it outlives cleanup().
  resetPanelBindings();
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
  // Tree rows are built from `useKeyTree` → `invokeListChildren`, NOT the flat
  // scan: the mock must actually yield both keys or no row ever mounts.
  listChildren.mockResolvedValue({
    children: [
      { kind: 'key', key: 'user:1', keyType: 'string', ttl: -1, logicalLen: 1, memBytes: null },
      { kind: 'key', key: 'other:2', keyType: 'string', ttl: -1, logicalLen: 1, memBytes: null },
    ],
    cursor: 0,
  });
  dbSizes.mockResolvedValue([{ db: 0, keys: 2 }]);
  setString.mockResolvedValue(undefined);
  // The active sub-tab is a module-level store so it survives the remount a
  // top-level tab switch causes. Every case here shares `sess-i1`, so without
  // this the tab chosen by one case leaks into the next.
  resetRightTab('panel-i1');
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  __resetDraftGuard();
});

describe('draftGuard 状态机（模块单例）', () => {
  it('walks enter → in-state → exit for both leave answers', async () => {
    // 进入条件缺席：干净态请求立即放行，不开对话框。
    expect(isDraftDirty()).toBe(false);
    await expect(requestDraftLeave()).resolves.toBe(true);
    expect(isLeavePending()).toBe(false);

    // 进入：脏 + 请求 ⇒ 悬起，动作未执行。
    publishDraftDirty(true);
    const first = requestDraftLeave();
    expect(isLeavePending()).toBe(true);
    // 状态内行为：并发请求合并到同一个悬起 Promise。
    const second = requestDraftLeave();
    expect(second).toBe(first);
    expect(isLeavePending()).toBe(true);

    // 退出跃迁 A：继续编辑 ⇒ 两个等待方都拿到 false，草稿仍在。
    settleDraftLeave(false);
    await expect(first).resolves.toBe(false);
    expect(isLeavePending()).toBe(false);
    expect(isDraftDirty()).toBe(true);

    // 退出跃迁 B：放弃更改 ⇒ 放行且草稿即刻失效。
    const third = requestDraftLeave();
    settleDraftLeave(true);
    await expect(third).resolves.toBe(true);
    expect(isLeavePending()).toBe(false);
    expect(isDraftDirty()).toBe(false);

    __resetDraftGuard();
    expect(isDraftDirty()).toBe(false);
  });
});

describe('脏底栏：放弃 / 保存只在草稿存在时出现', () => {
  it('mounts the bar on the first keystroke and discards back to the server value', async () => {
    renderWorkbench();
    fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));
    await waitFor(() => expect(screen.getByTestId('redis-string-input')).toBeTruthy());

    // 干净 ⇒ 没有任何保存/放弃入口。
    expect(dirtyBar()).toBeNull();
    expect(screen.queryByTestId('redis-string-save')).toBeNull();
    expect(screen.queryByTestId('redis-string-discard')).toBeNull();

    // 第一击 ⇒ 底栏出现（进入条件）。
    fireEvent.change(draftInput(), { target: { value: 'draft' } });
    await waitFor(() => expect(dirtyBar()).not.toBeNull());
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
    expect(setString).not.toHaveBeenCalled();

    // 退出跃迁：放弃 ⇒ 值回滚到服务器真值，底栏消失，脏落 false。
    fireEvent.click(screen.getByTestId('redis-string-discard'));
    await waitFor(() => expect(dirtyBar()).toBeNull());
    expect(draftInput().value).toBe('hello');
    expect(editor().getAttribute('data-string-dirty')).toBe('false');
    expect(setString).not.toHaveBeenCalled();
  });
});

describe('I-1 拦截点逐一验证（workbench 级）', () => {
  it('guards 切键: switching keys asks first and 继续编辑 keeps the draft', async () => {
    renderWorkbench();
    await selectAndDraft();

    // 进入：脏 + 点另一把键 ⇒ 对话框，动作未执行。
    fireEvent.click(screen.getByTestId('redis-key-row-other:2'));
    await screen.findByTestId('redis-draft-discard');
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(draftInput().value).toBe('draft');

    // 继续编辑 ⇒ 切键取消，草稿原样。
    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');

    // 再点 + 放弃 ⇒ 切键完成，新键从自己的值起步，脏落 false。
    fireEvent.click(screen.getByTestId('redis-key-row-other:2'));
    await screen.findByTestId('redis-draft-discard');
    fireEvent.click(screen.getByTestId('redis-draft-discard'));
    await waitFor(() => expect(column().getAttribute('data-selected-key')).toBe('other:2'));
    await waitFor(() => expect(draftInput().value).toBe('other'));
    expect(editor().getAttribute('data-string-dirty')).toBe('false');
  });

  it('guards 切db: switching databases asks first', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.click(screen.getByTestId('redis-db-db1'));
    await screen.findByTestId('redis-draft-discard');
    // 动作未执行：选择与草稿都还在。
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');

    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');

    fireEvent.click(screen.getByTestId('redis-db-db1'));
    await screen.findByTestId('redis-draft-discard');
    fireEvent.click(screen.getByTestId('redis-draft-discard'));
    // 放弃后换库完成：详情列回到 no-key —— 编辑面随旧键卸载，草稿不复存在。
    await waitFor(() => expect(column().getAttribute('data-detail-state')).toBe('no-key'));
    expect(screen.queryByTestId('redis-string-editor')).toBeNull();
    expect(leaveDialog()).toBeNull();
  });

  it('guards 工具栏刷新 (redis-refresh)', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.click(screen.getByTestId('redis-tree-refresh'));
    await screen.findByTestId('redis-draft-discard');
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');

    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
  });

  it('guards 头行刷新 (redis-header-refresh) and post-discard refetch keeps the selection', async () => {
    renderWorkbench();
    await selectAndDraft();
    const readsBefore = getKey.mock.calls.length;

    fireEvent.click(screen.getByTestId('redis-header-refresh'));
    await screen.findByTestId('redis-draft-discard');
    // 拦截发生在重取之前。
    expect(getKey.mock.calls.length).toBe(readsBefore);

    fireEvent.click(screen.getByTestId('redis-draft-discard'));
    // 放弃后重取继续，而 reloadDetail 只刷详情、不清选择（E-5 修的缺陷）。
    await waitFor(() => expect(getKey.mock.calls.length).toBeGreaterThan(readsBefore));
    await waitFor(() => expect(column().getAttribute('data-selected-key')).toBe('user:1'));
    expect(editor().getAttribute('data-string-dirty')).toBe('false');
  });

  it('save publishes clean first: no dialog during save/reload, then a plain refresh runs free', async () => {
    renderWorkbench();
    await selectAndDraft();

    fireEvent.click(screen.getByTestId('redis-string-save'));
    // 保存 ⇒ 不弹对话框（草稿已提交而非被丢弃），底栏随脏一起退场。
    await waitFor(() => expect(setString).toHaveBeenCalledTimes(1));
    expect(setString.mock.calls[0]).toHaveLength(4);
    await waitFor(() => expect(dirtyBar()).toBeNull());
    expect(editor().getAttribute('data-string-dirty')).toBe('false');
    expect(leaveDialog()).toBeNull();
    // 保存后的 reloadDetail 不关面板（旧缺陷：refreshKeys 清选择）。
    await waitFor(() => expect(column().getAttribute('data-selected-key')).toBe('user:1'));

    // 守卫此刻已干净 ⇒ 再点刷新直接执行，全程零对话框。
    fireEvent.click(screen.getByTestId('redis-tree-refresh'));
    await waitFor(() => expect(column().getAttribute('data-detail-state')).toBe('no-key'));
    expect(leaveDialog()).toBeNull();
  });
});

describe('I-1 切页签（卸载-恢复模式：切换后旧 panel 卸载，新 panel 挂载）', () => {
  it('guards tab switches and honors both leave answers', async () => {
    renderView();
    await selectAndDraft();

    // 进入：脏 + 切页签 ⇒ 对话框，页签还没动。
    fireEvent.click(screen.getByTestId('redis-right-tab-console'));
    await screen.findByTestId('redis-draft-discard');
    expect(screen.getByTestId('redis-right-tab-detail').getAttribute('data-active')).toBe('true');
    expect(screen.getByTestId('redis-right-tab-console').getAttribute('data-active')).toBe('false');

    // 继续编辑 ⇒ 留在键详情页签，草稿仍在。
    fireEvent.click(screen.getByTestId('redis-draft-keep'));
    await waitFor(() => expect(leaveDialog()).toBeNull());
    expect(screen.getByTestId('redis-right-tab-detail').getAttribute('data-active')).toBe('true');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');

    // 放弃更改 ⇒ 页签切换完成；detail panel 卸载，console panel 挂载。
    fireEvent.click(screen.getByTestId('redis-right-tab-console'));
    await screen.findByTestId('redis-draft-discard');
    fireEvent.click(screen.getByTestId('redis-draft-discard'));
    await waitFor(() =>
      expect(screen.getByTestId('redis-right-tab-console').getAttribute('data-active')).toBe(
        'true',
      ),
    );
    expect(screen.getByTestId('redis-right-tab-detail').getAttribute('data-active')).toBe('false');
    expect(screen.getByTestId('stub-console')).toBeTruthy();
    // detail panel 已卸载，编辑器不在 DOM 中。
    expect(screen.queryByTestId('redis-string-editor')).toBeNull();
    expect(screen.queryByTestId('redis-string-dirty-bar')).toBeNull();
  });
});

describe('I-1 边角出口：Esc / 卸载 / 非法 JSON 保存', () => {
  it('Esc closes the dialog: the guarded action is cancelled, selection and draft survive', async () => {
    renderWorkbench();
    await selectAndDraft();
    const readsBefore = getKey.mock.calls.length;

    fireEvent.click(screen.getByTestId('redis-tree-refresh'));
    await screen.findByTestId('redis-draft-discard');
    const dialogEl = document.querySelector('[role="dialog"]');
    expect(dialogEl).not.toBeNull();

    fireEvent.keyDown(dialogEl!, { key: 'Escape' });
    await waitFor(() => expect(leaveDialog()).toBeNull());
    // 退出跃迁（Esc＝继续编辑）：动作取消、重取没发生、草稿与选择原样。
    expect(isLeavePending()).toBe(false);
    expect(getKey.mock.calls.length).toBe(readsBefore);
    expect(column().getAttribute('data-selected-key')).toBe('user:1');
    expect(editor().getAttribute('data-string-dirty')).toBe('true');
    expect(dirtyBar()).not.toBeNull();
  });

  it('unmounting the editor while a leave is pending settles it false (no dangling await)', async () => {
    const view = renderWorkbench();
    await selectAndDraft();

    fireEvent.click(screen.getByTestId('redis-tree-refresh'));
    await screen.findByTestId('redis-draft-discard');
    expect(isLeavePending()).toBe(true);

    view.unmount();
    // DraftLeaveDialog 的卸载清扫：挂起的 leave 必须落定，否则调用方 await 悬死。
    expect(isLeavePending()).toBe(false);
  });

  it('saving invalid JSON never hits the wire and keeps the draft dirty', async () => {
    getKey.mockResolvedValue(stringDetail('user:1', '{"a":1}'));
    renderWorkbench();
    fireEvent.click(await screen.findByTestId('redis-key-row-user:1'));
    await waitFor(() => expect(screen.getByTestId('redis-string-input')).toBeTruthy());

    // 敲出一个残缺中间态（journey 要求覆盖的非法中间态）。
    fireEvent.change(draftInput(), { target: { value: '{"a":' } });
    await waitFor(() => expect(dirtyBar()).not.toBeNull());

    fireEvent.click(screen.getByTestId('redis-string-save'));
    await waitFor(() => expect(editor().getAttribute('data-string-dirty')).toBe('true'));
    // 解析失败 ⇒ 提前返回：不发 SET、不碰守卫、草稿仍在。
    expect(setString).not.toHaveBeenCalled();
    expect(isLeavePending()).toBe(false);
    expect(leaveDialog()).toBeNull();
    expect(dirtyBar()).not.toBeNull();
  });
});
