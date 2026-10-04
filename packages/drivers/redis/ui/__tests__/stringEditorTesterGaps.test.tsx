/**
 * [tester] 第 1 轮复验补测：常驻编辑面（E-2/E-3/E-5）自身的未覆盖支路。
 *
 * 覆盖率独立复算（v8 / `--coverage.all=false`）显示 `StringEditor.tsx` 停在
 * 70.65% stmts，未覆盖点里**属本轨验收面**的两条：
 *   - `:148-150` I-6 写门闸拒绝分支（Safe Mode 打开 ⇒ 保存不出网）；
 *   - `:129-132` JSON 三态（raw / pretty / minify）在常驻编辑面上的切换
 *     （PRD §3.3「JSON 三态继续走 JsonModeBar」，属"必须保留"项，本轨把它
 *     搬进新 `StringEditor` 后没有任何用例走过）。
 * 其余未覆盖点（解压成功回包、悬起期保存的防御支路、`unwrapRaw` 的对象形状）
 * 判定属真连/防御性单语句，非本轨缺口。
 *
 * 断言口径：`data-testid` + `data-*` + `data-i18n-key`；`useI18n` 被 stub 成
 * identity `t`，故文本恰为 i18n key（断 key 不断译文）。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
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
import { bytesToBase64 } from '../__testing__/bytes';
import { isDraftDirty } from '../shared/draftGuard';

const SAFE_MODE = { current: false };

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    // Read live: the gate asks the store at call time (see `useRedisGate`).
    get settings() {
      return {
        safeMode: SAFE_MODE.current,
        editorFontFamily: '',
        driverSettings: {},
      } as SettingsBridgeState['settings'];
    },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);

const PRETTY = '{\n  "a": 1\n}';
const COMPACT = '{"a":1}';

function frame(text: string): ValueFrame {
  const raw = bytesToBase64(new TextEncoder().encode(text));
  return {
    key: 'app:cfg',
    keyType: 'string',
    ttl: 300,
    logicalLen: raw.length,
    memBytes: text.length,
    rawB64: raw,
    truncated: false,
  };
}

function detail(value: string): KeyDetail {
  return {
    key: 'app:cfg',
    keyType: 'string',
    ttl: 300,
    // `get_key` returns the string payload wrapped (`{"value": …}`).
    value: { value },
  } as unknown as KeyDetail;
}

function renderEditor(value = PRETTY) {
  return render(
    <KeyDetailEditor
      dbSessionId="sess-t"
      dbIndex={0}
      detail={detail(value)}
      modules={[]}
      onRefresh={() => {}}
    />,
  );
}

const surface = () => screen.getByTestId('redis-string-editor');
const input = () => screen.getByTestId('redis-string-input') as HTMLTextAreaElement;
const save = () => screen.queryByTestId('redis-string-save');

beforeEach(() => {
  SAFE_MODE.current = false;
  getKeyRaw.mockImplementation(async () => frame(PRETTY));
  decodeValue.mockResolvedValue({ ok: true, json: '{}' });
  setString.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('[tester] Safe Mode 拒绝常驻编辑面的保存（I-6 门闸支路）', () => {
  it('blocks the write, keeps the draft, and shows nothing on the wire', async () => {
    SAFE_MODE.current = true;
    renderEditor(COMPACT);
    await waitFor(() => expect(input()).toBeTruthy());

    fireEvent.change(input(), { target: { value: '{"a":2}' } });
    await waitFor(() => expect(surface().getAttribute('data-string-dirty')).toBe('true'));

    fireEvent.click(screen.getByTestId('redis-string-save'));
    // 门闸拦下 ⇒ 一次 SET 都不出网；底栏与草稿原样保留（不能"拦了就清 dirty"）。
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(setString).not.toHaveBeenCalled();
    expect(surface().getAttribute('data-string-dirty')).toBe('true');
    expect(input().value).toBe('{"a":2}');
    expect(save()).not.toBeNull();
    expect(save()).not.toBeDisabled();
  });
});

describe('[tester] JSON 三态在常驻编辑面上的切换（PRD §3.3 保留项）', () => {
  it('reformats without dirtying, and preserves an edited draft across mode switches', async () => {
    renderEditor(PRETTY);
    await waitFor(() => expect(input()).toBeTruthy());

    // 初始 pretty（服务器真值）：干净、无底栏。
    expect(surface().getAttribute('data-string-dirty')).toBe('false');
    expect(screen.queryByTestId('redis-string-dirty-bar')).toBeNull();
    expect(input().value.replace(/\s+/g, '')).toBe(COMPACT);

    // minify：只是重排显示，不构成草稿（dirty 仍 false ⇒ 无底栏）。
    fireEvent.click(screen.getByTestId('redis-json-mode-minify'));
    expect(input().value).toBe(COMPACT);
    expect(surface().getAttribute('data-string-dirty')).toBe('false');
    expect(screen.queryByTestId('redis-string-dirty-bar')).toBeNull();

    // raw：干净态回读未格式化的原始载荷（走 `get_key` raw 通道，不是重排）。
    fireEvent.click(screen.getByTestId('redis-json-mode-raw'));
    expect(input().value).toBe(PRETTY);

    // 真改动 ⇒ 进入草稿；此后切模式必须保留草稿而不是拿服务器值覆盖它。
    fireEvent.change(input(), { target: { value: '{"a":3}' } });
    await waitFor(() => expect(surface().getAttribute('data-string-dirty')).toBe('true'));
    fireEvent.click(screen.getByTestId('redis-json-mode-pretty'));
    expect(input().value).not.toBe(PRETTY);
    expect(input().value.replace(/\s+/g, '')).toContain('"a":3');
    expect(surface().getAttribute('data-string-dirty')).toBe('true');

    // 保存走的是屏幕上的那份（重排后的文本），且只带四个字段（E-5：无 keepTtl）。
    fireEvent.click(screen.getByTestId('redis-string-save'));
    await waitFor(() => expect(setString).toHaveBeenCalledOnce());
    expect(setString.mock.calls[0]).toHaveLength(4);
    await waitFor(() => expect(surface().getAttribute('data-string-dirty')).toBe('false'));
    expect(screen.queryByTestId('redis-string-dirty-bar')).toBeNull();
  });

  it('a malformed intermediate state keeps the draft and never hits the wire', async () => {
    renderEditor(PRETTY);
    await waitFor(() => expect(input()).toBeTruthy());

    fireEvent.change(input(), { target: { value: '{"a":' } });
    await waitFor(() => expect(surface().getAttribute('data-string-dirty')).toBe('true'));
    fireEvent.click(screen.getByTestId('redis-string-save'));

    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
    });
    expect(setString).not.toHaveBeenCalled();
    expect(surface().getAttribute('data-string-dirty')).toBe('true');
    // 错误提示以既有 i18n key 呈现（identity `t` ⇒ 文本即 key）。
    await waitFor(() => expect(screen.getByText('redis.invalidJson')).toBeTruthy());

    // 修好之后能正常保存（退出跃迁，不卡死）。
    fireEvent.change(input(), { target: { value: '{"a":1}' } });
    fireEvent.click(screen.getByTestId('redis-string-save'));
    await waitFor(() => expect(setString).toHaveBeenCalledOnce());
    await waitFor(() => expect(surface().getAttribute('data-string-dirty')).toBe('false'));
  });
});

// ============================================================================
// BUG-003: 后端拒绝 set_string ⇒ 可见反馈 + 草稿原样 + 零未处理 rejection
// ============================================================================
describe('[redis-detail-ui-BUG-003] 保存被后端拒绝的可见反馈', () => {
  it('surfaces the rejection, keeps the draft dirty, and recovers on the next save', async () => {
    setString.mockRejectedValueOnce(new Error('WRONGTYPE bad'));
    renderEditor(PRETTY);
    await waitFor(() => expect(surface()).toBeTruthy());

    fireEvent.change(input(), { target: { value: '{"a":9}' } });
    await waitFor(() => expect(surface().getAttribute('data-string-dirty')).toBe('true'));

    fireEvent.click(screen.getByTestId('redis-string-save'));

    // 可见错误锚点：data-i18n-key 恰为本轮新增的 redis.detail.* key（identity t）。
    // `save()` 链路上的 `.catch` 吃掉了 rejection ⇒ vitest 收尾处不会出现
    // unhandled rejection（有则整跑失败）；这里同时钉住「可见」这一半。
    const err = await screen.findByTestId('redis-string-save-error');
    expect(err.getAttribute('data-i18n-key')).toBe('redis.detail.saveFailed');
    expect(setString).toHaveBeenCalledOnce();

    // 被拒 ⇒ 零静默：草稿值、编辑面脏位、守卫脏位都原样，底栏与保存按钮仍在。
    expect(surface().getAttribute('data-string-dirty')).toBe('true');
    expect(input().value).toBe('{"a":9}');
    expect(isDraftDirty()).toBe(true);
    expect(save()).not.toBeNull();

    // 退出跃迁：链路恢复后再次保存 ⇒ 错误消失、脏位落 false、不卡死。
    fireEvent.click(screen.getByTestId('redis-string-save'));
    await waitFor(() => expect(setString).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByTestId('redis-string-save-error')).toBeNull());
    await waitFor(() => expect(surface().getAttribute('data-string-dirty')).toBe('false'));
    expect(isDraftDirty()).toBe(false);
  });
});
