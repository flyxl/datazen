/**
 * [tester] 第 1 轮复验补测：常驻编辑面的「解压预检」三条出口。
 *
 * 台账 §6 点名「`StringEditor` 未覆盖集中在解压按钮支路（`runDecompress` 的
 * 失败/异常分支，需真连返回压缩载荷）」。本轮**不需要真连**：载荷用 `node:zlib`
 * 现造真实 gzip 字节（让本轨代码的 `valueLooksCompressed` / 按钮显隐真的判定），
 * 只把最终的 `tryDecompressString` 换成可编程桩 ⇒ 走的全是本轨 diff 的分支。
 *
 * 断言口径：`data-testid` + `data-*`；`useI18n` 被 stub 成 identity `t`，故渲染
 * 文本恰为 **i18n key**（`redis.decompressCodec` / `redis.decompressFailed`）——
 * 与本仓既有口径一致（`connectionWizard.test.tsx` / `PubSubPanel.test.tsx` 同法），
 * 不钉任何英文译文；解码文本属服务器数据。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import zlib from 'node:zlib';
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

const tryDecompress = vi.fn();
vi.mock('../value-editors/stringKeyValue', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../value-editors/stringKeyValue')>()),
  tryDecompressString: (...args: unknown[]) => tryDecompress(...args),
}));

import type { DecompressResult } from '../value-editors/stringKeyValue';
import type { KeyDetail, ValueFrame } from '../shared/types';
import { KeyDetailEditor } from '../value-editors/KeyEditors';
import { bytesToBase64 } from '../__testing__/bytes';

bindSettingsStore(
  create<SettingsBridgeState>(() => ({
    settings: { safeMode: false, editorFontFamily: '', driverSettings: {} },
  })),
);
bindConnectionStore(create<ConnectionBridgeState>(() => ({ connections: [] })));
bindConfirmDialog(() => [async () => true, null]);

/** Real gzip bytes, char-coded so the 1f 8b magic survives into `detail.value`. */
function gzipText(text: string): string {
  const gz = zlib.gzipSync(Buffer.from(text, 'utf8'));
  return Array.from<number>(Uint8Array.from(gz))
    .map((b) => String.fromCharCode(b))
    .join('');
}

function frame(raw: string): ValueFrame {
  return {
    key: 'app:blob',
    keyType: 'string',
    ttl: -1,
    logicalLen: raw.length,
    memBytes: raw.length,
    rawB64: bytesToBase64(new TextEncoder().encode(raw)),
    truncated: false,
  };
}

const RAW = gzipText('{"hello":"world"}');

function renderEditor() {
  return render(
    <KeyDetailEditor
      dbSessionId="sess-d"
      dbIndex={0}
      detail={
        {
          key: 'app:blob',
          keyType: 'string',
          ttl: -1,
          value: { value: RAW },
        } as unknown as KeyDetail
      }
      modules={[]}
      onRefresh={() => {}}
    />,
  );
}

const surface = () => screen.getByTestId('redis-string-editor');
const button = () => screen.getByText('redis.decompressView') as HTMLButtonElement;
/** Panels carry no `data-testid` upstream, so locate them by their i18n key text. */
const panelWithKeyText = (keyText: string): HTMLElement | null =>
  Array.from(surface().querySelectorAll('div')).find((el) => el.textContent?.includes(keyText)) ??
  null;

async function settle(ms = 20) {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, ms));
  });
}

beforeEach(() => {
  tryDecompress.mockReset();
  decodeValue.mockResolvedValue({ ok: true, json: '{}' });
  getKeyRaw.mockResolvedValue(frame(RAW));
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('[tester] 解压预检：进入 / 成功 / 判不出 / 抛错（常驻编辑面支路）', () => {
  it('offers the action for a gzip-magic payload and renders the decoded result', async () => {
    renderEditor();

    // 进入条件：载荷以 gzip magic 起头 ⇒ 动作出现；未点前不渲染结果面板。
    await waitFor(() => expect(button()).toBeTruthy());
    expect(panelWithKeyText('redis.decompressCodec')).toBeNull();

    const ok: DecompressResult = { codec: 'gzip', text: '{"hello":"world"}', bytes: 17 };
    tryDecompress.mockResolvedValue(ok);
    fireEvent.click(button());
    await settle();

    // 状态内：把「屏幕上的那份原始字符串」交给解码器（不是 base64 后的形状）。
    expect(tryDecompress).toHaveBeenCalledOnce();
    expect(tryDecompress.mock.calls[0]?.[0]).toBe(RAW);
    const panel = panelWithKeyText('redis.decompressCodec');
    expect(panel).not.toBeNull();
    // 字节数是数据；codec 名走 `{codec}` 占位符回填（identity `t` 返回 key 本身，
    // 故占位符不在此处展开 —— 真实词条才展开，这里断可断的部分）。
    expect(panel?.textContent).toContain(`${ok.bytes} B`);
    // 解码文本是服务器数据。
    expect(surface().textContent).toContain('world');
  });

  it('names the failure when the payload turns out not to be compressed', async () => {
    renderEditor();
    await waitFor(() => expect(button()).toBeTruthy());

    tryDecompress.mockResolvedValue(null);
    fireEvent.click(button());
    await settle();

    // 判不出 ⇒ 具名失败态（i18n key 文本），且不留下半截结果面板。
    await waitFor(() => expect(panelWithKeyText('redis.decompressFailed')).not.toBeNull());
    expect(panelWithKeyText('redis.decompressCodec')).toBeNull();
    expect(button()).not.toBeDisabled();
  });

  it('surfaces a thrown decompression error and returns to clickable', async () => {
    renderEditor();
    await waitFor(() => expect(button()).toBeTruthy());

    tryDecompress.mockRejectedValue(new Error('truncated deflate stream'));
    fireEvent.click(button());
    await settle();

    // 异常原文属服务器数据（该支路无对应词条）；关键是 busy 必须落回 false。
    await waitFor(() => expect(surface().textContent).toContain('truncated deflate stream'));
    expect(button()).not.toBeDisabled();
    expect(panelWithKeyText('redis.decompressCodec')).toBeNull();
  });
});
