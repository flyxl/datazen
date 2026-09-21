/**
 * ValueViewer codec × view state-machine journey (R8/R9).
 *
 * Verifies the read-only viewer renders hostile bytes losslessly through the
 * hex view, and that selecting a backend codec routes through `decode_value`
 * with the raw base64 payload (never re-encoding the stored bytes).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

vi.mock('../../../../../src/hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key, lang: 'en' }),
}));

const decodeValue = vi.fn();
vi.mock('../redisInvoke', () => ({
  redisCommandInvoke: vi.fn(),
  invokeDecodeValue: (...a: unknown[]) => decodeValue(...a),
}));

import { ValueViewer } from '../ValueViewer';
import { bytesToBase64 } from '../valueView/codecs';

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
