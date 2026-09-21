/**
 * valueView pipeline unit tests (R2/R9).
 *
 * Covers base64 round-trip for hostile bytes (\0, invalid UTF-8), each view
 * renderer's shape, JSON validate/pretty/unicode behaviour, and the hex/binary
 * row caps. Browser decompression codecs are exercised by the manual/E2E seeds,
 * not here, because jsdom's Blob.stream() support is unreliable.
 */
import { describe, expect, it } from 'vitest';
import {
  base64ToBytes,
  bytesToBase64,
  applyBrowserCodec,
  CODECS,
  BROWSER_CODECS,
  BACKEND_CODECS,
  isBackendCodec,
} from '../value-editors/valueView/codecs';
import { renderView, renderHex, type HexRow } from '../value-editors/valueView/views';

function text(s: string): Uint8Array {
  return new TextEncoder().encode(s);
}

describe('codecs', () => {
  it('declares disjoint browser / backend codec sets', () => {
    expect([...BROWSER_CODECS, ...BACKEND_CODECS].sort()).toEqual([...CODECS].sort());
    expect(isBackendCodec('msgpack')).toBe(true);
    expect(isBackendCodec('gzip')).toBe(false);
  });

  it('round-trips hostile bytes through base64', () => {
    const bytes = new Uint8Array([0x00, 0x01, 0xff, 0x7f, 0x80]);
    const b64 = bytesToBase64(bytes);
    expect([...base64ToBytes(b64)]).toEqual([...bytes]);
  });

  it('base64ToBytes rejects malformed input', () => {
    expect(() => base64ToBytes('A')).toThrow();
    expect(() => base64ToBytes('!!!!')).toThrow();
  });

  it('applyBrowserCodec none is identity', async () => {
    const bytes = text('hello');
    expect([...(await applyBrowserCodec(bytes, 'none'))]).toEqual([...bytes]);
  });

  it('applyBrowserCodec base64 decodes an ASCII base64 payload', async () => {
    const inner = text('data');
    const b64Text = text(bytesToBase64(inner));
    const out = await applyBrowserCodec(b64Text, 'base64');
    expect([...out]).toEqual([...inner]);
  });
});

describe('views', () => {
  it('utf8 flags invalid sequences but still renders', () => {
    const r = renderView(new Uint8Array([0xff, 0xfe]), 'utf8');
    expect(r.kind).toBe('text');
    if (r.kind === 'text') {
      expect(r.invalidUtf8).toBe(true);
      expect(r.text.length).toBeGreaterThan(0);
    }
  });

  it('utf8 clean text is not flagged', () => {
    const r = renderView(text('ok'), 'utf8');
    expect(r.kind === 'text' && r.invalidUtf8 === false && r.text === 'ok').toBe(true);
  });

  it('ascii maps non-printables to dots', () => {
    const r = renderView(new Uint8Array([0x41, 0x00, 0xff]), 'ascii');
    expect(r.kind === 'text' && r.text).toBe('A..');
  });

  it('hex renders offset / hex / ascii gutter for binary bytes', () => {
    const r = renderHex(new Uint8Array([0x00, 0x01, 0xff, 0x41]));
    expect(r.kind).toBe('hex');
    if (r.kind === 'hex') {
      const first = r.rows[0] as HexRow;
      expect(first.offset).toBe(0);
      expect(first.hex.startsWith('00 01 ff 41')).toBe(true);
      expect(first.ascii).toBe('...A');
    }
  });

  it('hex caps rows for large payloads', () => {
    const big = new Uint8Array(16 * 50);
    const r = renderHex(big, 10);
    expect(r.kind === 'hex' && r.rows.length).toBe(10);
    expect(r.kind === 'hex' && r.totalBytes).toBe(16 * 50);
  });

  it('binary renders bit groups', () => {
    const r = renderView(new Uint8Array([0x01]), 'binary');
    expect(r.kind === 'text' && r.text).toContain('00000001');
  });

  it('base64 view matches bytesToBase64', () => {
    const bytes = text('hi');
    const r = renderView(bytes, 'base64');
    expect(r.kind === 'text' && r.text).toBe(bytesToBase64(bytes));
  });

  it('json pretty-prints and escapes non-ascii; unicodeJson keeps it raw', () => {
    const src = text(JSON.stringify({ k: 'é' }));
    const jsonView = renderView(src, 'json');
    const uniView = renderView(src, 'unicodeJson');
    expect(jsonView.kind === 'text' && jsonView.text.includes('\\u00e9')).toBe(true);
    expect(uniView.kind === 'text' && uniView.text.includes('é')).toBe(true);
  });

  it('json errors on invalid JSON', () => {
    const r = renderView(text('{not json'), 'json');
    expect(r.kind).toBe('error');
  });

  it('yaml/xml degrade to lossy text without a parser dependency', () => {
    const r = renderView(text('a: 1'), 'yaml');
    expect(r.kind === 'text' && r.text).toBe('a: 1');
  });
});
