/**
 * ValueViewer codec × view state-machine journey (R8/R9).
 *
 * Verifies the read-only viewer renders hostile bytes losslessly through the
 * hex view, and that selecting a backend codec routes through `decode_value`
 * with the raw base64 payload (never re-encoding the stored bytes).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

// Components take `useI18n` from the single @datazen/ui runtime; keep the
// assertions locale-independent by overriding only that hook.
vi.mock('@datazen/ui', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@datazen/ui')>()),
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

const decodeValue = vi.fn();
vi.mock('../shared/redisInvoke', () => ({
  redisCommandInvoke: vi.fn(),
  invokeDecodeValue: (...a: unknown[]) => decodeValue(...a),
}));

import { ValueViewer } from '../value-editors/ValueViewer';
import { bytesToBase64 } from '../__testing__/bytes';

const HOSTILE_B64 = bytesToBase64(new Uint8Array([0x00, 0x01, 0xff, 0x41]));
const frame = {
  key: 'bin:key',
  keyType: 'string',
  ttl: -1,
  logicalLen: 4,
  memBytes: 4,
  rawB64: HOSTILE_B64,
  truncated: false,
};

beforeEach(() => {
  decodeValue.mockReset();
});

afterEach(() => {
  cleanup();
});

function viewer() {
  return render(<ValueViewer dbSessionId="sess-1" frame={frame} />);
}

describe('ValueViewer', () => {
  it('shows utf-8 text by default (none codec)', () => {
    viewer();
    expect(screen.getByTestId('redis-codec-none')).toBeTruthy();
    expect(screen.getByTestId('redis-view-utf8')).toBeTruthy();
  });

  it('renders hostile bytes losslessly in the hex view', async () => {
    viewer();
    fireEvent.click(screen.getByTestId('redis-view-hex'));
    const hex = await screen.findByTestId('redis-value-hex');
    expect(hex.textContent).toContain('00 01 ff 41');
    expect(hex.textContent).toContain('|...A|');
  });

  it('routes a backend codec through decode_value with the base64 payload', async () => {
    decodeValue.mockResolvedValue({ ok: true, json: '{"a":1}' });
    viewer();
    fireEvent.click(screen.getByTestId('redis-codec-msgpack'));
    await waitFor(() => expect(decodeValue).toHaveBeenCalledTimes(1));
    const call = decodeValue.mock.calls[0] as unknown as [string, string, string];
    expect(call[0]).toBe('sess-1');
    expect(call[1]).toBe('msgpack');
    expect(call[2]).toBe(HOSTILE_B64);
    const out = await screen.findByTestId('redis-value-text');
    expect(out.textContent).toContain('{"a":1}');
  });

  it('surfaces a backend decode rejection as an error, not a crash', async () => {
    decodeValue.mockRejectedValue(new Error('refusing to parse'));
    viewer();
    fireEvent.click(screen.getByTestId('redis-codec-pickle'));
    const err = await screen.findByText('refusing to parse');
    expect(err).toBeTruthy();
  });

  it('shows the no-data placeholder when the frame has no raw bytes', () => {
    render(<ValueViewer dbSessionId="sess-1" frame={{ ...frame, rawB64: null }} />);
    expect(screen.getByText('redis.view.noData')).toBeTruthy();
  });
});

/* ── [tester] 第 1 轮复验补测：E-3 之后输出区只在只读档出现 ─────────────────
 * 台账 §6 记 `ValueViewer.tsx` 75.36%，未覆盖为 `:124-138` 渲染兜底支路。实测
 * 未覆盖的是 `copyText`(:118-121) / `downloadBytes`(:123-139) / `wrap`(:212) 与
 * 一次 `seq` 竞态守卫(:105)：E-3 把 `showOutput` 改成 `readOnly` 驱动后，这三枚
 * 动作**只有**在只读预览态才可达（宿主常驻编辑面传 `showOutput={false}`），属本
 * 轨验收面的新可达性形状，故在此钉住。
 */
describe('[tester] ValueViewer read-only preview actions (E-3 reachability)', () => {
  const writeText = vi.fn(async (_payload: string) => undefined);

  beforeEach(() => {
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
      writable: true,
    });
  });

  it('copies the rendered text when a text view is active', async () => {
    const { container } = render(<ValueViewer dbSessionId="sess-1" frame={frame} />);
    // 默认 utf8 文本视图 ⇒ 复制的是渲染后的文本，不是 base64 载荷。
    const copy = await screen.findByTestId('redis-view-copy');
    fireEvent.click(copy);
    await waitFor(() => expect(writeText).toHaveBeenCalledOnce());
    const payload = writeText.mock.calls[0]?.[0] as string;
    // 载荷是服务器数据：4 字节 hostile 序列的 utf8 渲染（含替换符），不该等于 b64。
    expect(payload).not.toBe(HOSTILE_B64);
    expect(container).toBeTruthy();
  });

  it('copies the raw payload when the active view is a byte projection', async () => {
    render(<ValueViewer dbSessionId="sess-1" frame={frame} />);
    fireEvent.click(screen.getByTestId('redis-view-hex'));
    await screen.findByTestId('redis-value-hex');
    writeText.mockClear();

    fireEvent.click(screen.getByTestId('redis-view-copy'));
    await waitFor(() => expect(writeText).toHaveBeenCalledOnce());
    // hex 档没有文本渲染 ⇒ 回退到原始 base64（保住字节，不复制半截 hex）。
    expect(writeText.mock.calls[0]?.[0]).toBe(HOSTILE_B64);
  });

  it('toggles wrapping without touching the payload', async () => {
    render(<ValueViewer dbSessionId="sess-1" frame={frame} />);
    const wrap = screen.getByTestId('redis-view-wrap') as HTMLInputElement;
    expect(wrap.checked).toBe(true);
    fireEvent.click(wrap);
    expect(wrap.checked).toBe(false);
    fireEvent.click(wrap);
    expect(wrap.checked).toBe(true);
    // 换行是纯显示开关：不发任何命令。
    expect(decodeValue).not.toHaveBeenCalled();
  });

  it('downloads the raw bytes named after the key', async () => {
    const create = vi.fn(() => 'blob:data:stub');
    const revoke = vi.fn();
    Object.defineProperty(URL, 'createObjectURL', { value: create, configurable: true });
    Object.defineProperty(URL, 'revokeObjectURL', { value: revoke, configurable: true });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, 'click')
      .mockImplementation(() => undefined);

    render(<ValueViewer dbSessionId="sess-1" frame={frame} />);
    fireEvent.click(screen.getByTestId('redis-view-download'));

    expect(create).toHaveBeenCalledTimes(1);
    expect(click).toHaveBeenCalledTimes(1);
    expect(revoke).toHaveBeenCalledTimes(1);
    click.mockRestore();
  });

  it('renders only the codec/view rows when the host keeps the output (showOutput=false)', () => {
    render(
      <ValueViewer dbSessionId="sess-1" frame={frame} view="hex" codec="none" showOutput={false} />,
    );
    const viewer = screen.getByTestId('redis-value-viewer');
    expect(viewer.getAttribute('data-show-output')).toBe('false');
    // 预检档不留第二份输出：复制 / 下载 / 换行 全部不存在（它们只在只读预览态可达）。
    expect(screen.queryByTestId('redis-view-copy')).toBeNull();
    expect(screen.queryByTestId('redis-view-download')).toBeNull();
    expect(screen.queryByTestId('redis-view-wrap')).toBeNull();
    // 两行控件仍在，用户才切得回文本档。
    expect(screen.getByTestId('redis-view-group')).toBeTruthy();
    expect(screen.getByTestId('redis-codec-group')).toBeTruthy();
  });
});
