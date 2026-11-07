/**
 * 键详情编辑区的 I-5 只读态旅程（本轨 E-3，PRD §3.3 屏 B 右列 / §4 I-5）。
 *
 * 覆盖的是「状态机」而不是静态合法态：常驻可编辑 → 字节视图只读（带原因）→
 * 切回文本视图仍可编辑、草稿还在 → 保存。以及大 value 档、后端截断档、
 * `get_key_raw` 缺席档。
 *
 * 断言口径（§4-6 / 裁定 8-4）：定位用 `data-testid`，状态用 `data-*`，
 * 原因文案只断言其 i18n key（`data-i18n-key`），不读英文字面量。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { create } from 'zustand';
import {
  bindConfirmDialog,
  bindConnectionStore,
  bindSettingsStore,
  type ConnectionBridgeState,
  type SettingsBridgeState,
} from '@datazen/driver-sdk';

vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

const getKeyRaw = vi.fn();
const decodeValue = vi.fn();
vi.mock('../shared/redisInvoke', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../shared/redisInvoke')>()),
  invokeGetKeyRaw: (...args: unknown[]) => getKeyRaw(...args),
  invokeDecodeValue: (...args: unknown[]) => decodeValue(...args),
}));

const setString = vi.fn();
vi.mock('../value-editors/keyEditorsInvokes', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../value-editors/keyEditorsInvokes')>()),
  invokeSetString: (...args: unknown[]) => setString(...args),
}));

import type { KeyDetail, ValueFrame } from '../shared/types';
import { KeyDetailEditor } from '../value-editors/KeyEditors';
import { VIEWS } from '../value-editors/valueView/views';
import { CODECS } from '../value-editors/valueView/codecs';
import { bytesToBase64 } from '../__testing__/bytes';
import { BIG_VALUE_SENTINEL_BYTES } from '../value-editors/redisBigValue';

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);

const RAW = bytesToBase64(new TextEncoder().encode('hello'));

function frame(over: Partial<ValueFrame> = {}): ValueFrame {
  return {
    key: 'user:1',
    keyType: 'string',
    ttl: -1,
    logicalLen: RAW.length,
    memBytes: 5,
    rawB64: RAW,
    truncated: false,
    ...over,
  };
}

function stringDetail(value = 'hello'): KeyDetail {
  return {
    key: 'user:1',
    keyType: 'string',
    ttl: -1,
    value,
  } as unknown as KeyDetail;
}

function editor(overrides: { detail?: KeyDetail; onDirtyChange?: (d: boolean) => void } = {}) {
  return render(
    <KeyDetailEditor
      dbSessionId="sess-e3"
      dbIndex={0}
      detail={overrides.detail ?? stringDetail()}
      modules={[]}
      onRefresh={() => {}}
      onDirtyChange={overrides.onDirtyChange}
    />,
  );
}

const surface = () => screen.getByTestId('redis-string-editor');
const input = () => screen.getByTestId('redis-string-input') as HTMLTextAreaElement;
const save = () => screen.queryByTestId('redis-string-save');
const dirtyBar = () => screen.queryByTestId('redis-string-dirty-bar');
const reason = () => screen.queryByTestId('redis-string-readonly-reason');

function expectEditable(reasonId = 'none') {
  expect(surface().getAttribute('data-string-readonly')).toBe('false');
  expect(surface().getAttribute('data-readonly-reason')).toBe(reasonId);
  expect(reason()).toBeNull();
  expect(input().readOnly).toBe(false);
  // E-5: a CLEAN editor has no save affordance at all — the dirty bottom bar
  // (discard + save) only mounts while a draft is live — and the old keepTtl
  // checkbox is gone for good (the backend owns that default since W3-C).
  expect(dirtyBar()).toBeNull();
  expect(save()).toBeNull();
  expect(surface().querySelector('input[type="checkbox"]')).toBeNull();
}

/** Dirty AND editable: the bottom bar is up, save enabled and unblocked. */
function expectEditableDraft(reasonId = 'none') {
  expect(surface().getAttribute('data-string-readonly')).toBe('false');
  expect(surface().getAttribute('data-readonly-reason')).toBe(reasonId);
  expect(reason()).toBeNull();
  expect(input().readOnly).toBe(false);
  expect(dirtyBar()).not.toBeNull();
  expect(save()).not.toBeNull();
  expect(save()).not.toBeDisabled();
  expect(save()!.getAttribute('data-save-blocked-by')).toBe('none');
}

/** Read-only facts, valid clean or dirty (no save-affordance claims). */
function expectReadOnlyState(reasonId: 'binary-view' | 'big-value', i18nKey: string) {
  expect(surface().getAttribute('data-string-readonly')).toBe('true');
  expect(surface().getAttribute('data-readonly-reason')).toBe(reasonId);
  const banner = reason();
  expect(banner).not.toBeNull();
  expect(banner!.getAttribute('data-readonly-reason')).toBe(reasonId);
  // 原因文案以 i18n key 断言（禁英文字面量）。
  expect(banner!.querySelector('[data-i18n-key]')!.getAttribute('data-i18n-key')).toBe(i18nKey);
  // 只读用 readOnly 而非 disabled：截断载荷还得让用户选中、复制来看。
  expect(input().readOnly).toBe(true);
  expect(input().disabled).toBe(false);
}

/**
 * Read-only WHILE dirty: the draft keeps the bar up (the draft can still be
 * discarded), but save stays disabled with its blocked-by marker.
 */
function expectReadOnlyBlocked(reasonId: 'binary-view' | 'big-value', i18nKey: string) {
  expectReadOnlyState(reasonId, i18nKey);
  expect(dirtyBar()).not.toBeNull();
  expect(save()).not.toBeNull();
  expect(save()).toBeDisabled();
  expect(save()!.getAttribute('data-save-blocked-by')).toBe('readonly');
}

beforeEach(() => {
  getKeyRaw.mockResolvedValue(frame());
  decodeValue.mockResolvedValue({ ok: true, json: '{}' });
  setString.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('常驻编辑面 → 字节视图只读 → 回到可编辑（一条连续旅程）', () => {
  it('renders one resident editable surface with no second preview and no mode toggle', () => {
    editor();

    // E-2 删掉的两态开关不许复活。
    expect(screen.queryByTestId('redis-string-mode-toggle')).toBeNull();
    expectEditable();
    // Codec / View 两行仍在（渲染预检），但输出区让位给编辑区：不再叠一份只读预览。
    expect(screen.getByTestId('redis-value-viewer').getAttribute('data-show-output')).toBe('false');
    expect(screen.getByTestId('redis-view-group')).toBeTruthy();
    expect(screen.getByTestId('redis-codec-group')).toBeTruthy();
    expect(screen.queryByTestId('redis-value-text')).toBeNull();
    expect(screen.queryByTestId('redis-view-copy')).toBeNull();
  });

  it('walks draft → hex read-only (with reason) → back to utf8 → save', async () => {
    const onDirtyChange = vi.fn();
    editor({ onDirtyChange });

    fireEvent.change(input(), { target: { value: 'edited' } });
    expect(surface().getAttribute('data-string-dirty')).toBe('true');
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);
    // E-5: first keystroke of the draft mounts the bottom bar.
    expect(dirtyBar()).not.toBeNull();
    expect(save()).not.toBeNull();

    fireEvent.click(screen.getByTestId('redis-view-hex'));
    expectReadOnlyBlocked('binary-view', 'redis.detail.readonly.binaryView');
    // 只读档才把字节投影渲染出来（预检档不花这次渲染）。
    await waitFor(() => expect(screen.getByTestId('redis-value-hex')).toBeTruthy());
    // 草稿没丢：只读只是不许再改，不是清空。
    expect(input().value).toBe('edited');
    expect(surface().getAttribute('data-string-dirty')).toBe('true');
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);

    fireEvent.click(screen.getByTestId('redis-view-utf8'));
    expectEditableDraft();
    expect(input().value).toBe('edited');

    fireEvent.click(screen.getByTestId('redis-string-save'));
    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(false));
    // E-5: a successful save retires the bar — no save affordance when clean.
    await waitFor(() => expect(dirtyBar()).toBeNull());
    expect(save()).toBeNull();
    expect(setString).toHaveBeenCalledTimes(1);
    // `invokeSetString(dbSessionId, dbIndex, key, value)` — E-5: no keepTtl arg.
    const [sessionId, dbIndex, key, draft] = setString.mock.calls[0] as unknown as [
      string,
      number,
      string,
      string,
    ];
    expect(sessionId).toBe('sess-e3');
    expect(dbIndex).toBe(0);
    expect(key).toBe('user:1');
    expect(draft).toBe('edited');
    expect(setString.mock.calls[0]).toHaveLength(4);
  });

  it('treats every codec choice as a preview, never as a third read-only state', async () => {
    editor();
    for (const codec of CODECS) {
      fireEvent.click(screen.getByTestId(`redis-codec-${codec}`));
      expectEditable();
      // 预检档不做解码：9 个 codec 点完也不该有一次 decode_value 往返。
      expect(decodeValue).not.toHaveBeenCalled();
    }
    expect(CODECS.length).toBe(9);

    // 只读档才真的解码（原因文案要说明"这是投影"）。
    fireEvent.click(screen.getByTestId('redis-view-hex'));
    await waitFor(() => expect(decodeValue).toHaveBeenCalledTimes(1));
  });

  it('keeps every text view editable and only the two byte views locked', () => {
    editor();
    const byteOnly = new Set(['hex', 'binary']);
    for (const view of VIEWS) {
      fireEvent.click(screen.getByTestId(`redis-view-${view}`));
      if (byteOnly.has(view)) {
        expectReadOnlyState('binary-view', 'redis.detail.readonly.binaryView');
      } else {
        expectEditable();
      }
    }
    expect(VIEWS.length).toBe(9);
  });
});

describe('大 value / 截断载荷（I-5 只读态②）', () => {
  it('locks a string over the sentinel read-only and names its size', async () => {
    const bytes = BIG_VALUE_SENTINEL_BYTES + 1;
    getKeyRaw.mockResolvedValue(frame({ logicalLen: bytes, rawB64: null, truncated: false }));
    editor();

    // BUG-006 后：超哨兵（truncated=false）⇒ 载荷完整，走 bigValueComplete 文案。
    await waitFor(() => expectReadOnlyState('big-value', 'redis.detail.readonly.bigValueComplete'));
    expect(
      reason()!.querySelector('[data-big-value-bytes]')!.getAttribute('data-big-value-bytes'),
    ).toBe(String(bytes));
    // 截断载荷不接受键盘写入（jsdom 的 change 会绕过 DOM 只读，所以逻辑层也要挡）。
    fireEvent.change(input(), { target: { value: 'overwrite' } });
    expect(surface().getAttribute('data-string-dirty')).toBe('false');
  });

  it('reports the backend truncation flag as the same read-only state', async () => {
    getKeyRaw.mockResolvedValue(frame({ truncated: true, logicalLen: 6_000_000, rawB64: null }));
    editor();
    await waitFor(() => expectReadOnlyState('big-value', 'redis.detail.readonly.bigValue'));
    // [tester] 徽标只断"存在"不够：它的具名文案（I-11：不许留白）此前无人钉。
    const badge = screen.getByTestId('redis-key-badge-truncated');
    expect(badge.getAttribute('data-i18n-key')).toBe('redis.detail.badge.truncated');
  });

  it('[tester] does NOT raise the truncation badge for an over-sentinel but complete payload', async () => {
    // Backend truncates only above 5 MiB (`RAW_VALUE_MAX_BYTES`), while the
    // frontend sentinel is 64 KiB — so a 100 KiB string is COMPLETE. The editor
    // must still lock (I-5 ②), but the badge must not claim truncation.
    getKeyRaw.mockResolvedValue(
      frame({ logicalLen: 100_000, rawB64: 'aGVsbG8=', truncated: false }),
    );
    editor();
    // BUG-006 已修：完整载荷（truncated=false、rawB64 有值）⇒ 原因条说"完整但超
    // 预算"，与旁边缺席的 truncated 徽标不再自相矛盾；真截断分支仍走旧 key
    // （见上一条用例与 `keyReadOnlyPolicy.test.ts` 的分支断言）。
    await waitFor(() => expectReadOnlyState('big-value', 'redis.detail.readonly.bigValueComplete'));
    expect(screen.queryByTestId('redis-key-badge-truncated')).toBeNull();
  });

  it('prefers the byte-view reason while a byte view is active on a huge value', async () => {
    getKeyRaw.mockResolvedValue(frame({ truncated: true, logicalLen: 6_000_000, rawB64: null }));
    editor();
    await waitFor(() => expectReadOnlyState('big-value', 'redis.detail.readonly.bigValue'));
    // 用户当场能切回去的那个原因更可操作。
    fireEvent.click(screen.getByTestId('redis-view-binary'));
    expectReadOnlyState('binary-view', 'redis.detail.readonly.binaryView');
    fireEvent.click(screen.getByTestId('redis-view-utf8'));
    expectReadOnlyState('big-value', 'redis.detail.readonly.bigValue');
  });
});

describe('没有第三态：可选增强缺席时编辑面照常可写', () => {
  it('stays editable when get_key_raw fails', async () => {
    const onDirtyChange = vi.fn();
    getKeyRaw.mockRejectedValue(new Error('boom'));
    editor({ onDirtyChange });

    await waitFor(() => expectEditable());
    fireEvent.change(input(), { target: { value: 'draft' } });
    expect(surface().getAttribute('data-string-dirty')).toBe('true');
    expect(onDirtyChange).toHaveBeenLastCalledWith(true);
  });
});
