import { describe, expect, it } from 'vitest';
import {
  DECOMPRESS_MAX_BYTES,
  initialStringEditorValue,
  looksLikeJsonText,
  tryDecompressString,
  tryPrettyJson,
  unwrapStringKeyValue,
  valueLooksCompressed,
} from '../stringKeyValue';

describe('unwrapStringKeyValue', () => {
  it('unwraps { value } payload from get_key_detail', () => {
    expect(
      unwrapStringKeyValue({ value: '{"name":"张三","level":"vip"}' }),
    ).toBe('{"name":"张三","level":"vip"}');
  });

  it('passes through raw strings', () => {
    expect(unwrapStringKeyValue('hello')).toBe('hello');
  });

  it('returns empty string for nullish; stringifies other primitives', () => {
    expect(unwrapStringKeyValue(null)).toBe('');
    expect(unwrapStringKeyValue(undefined)).toBe('');
    expect(unwrapStringKeyValue(42)).toBe('42');
    expect(unwrapStringKeyValue({ value: 1 })).toBe('1');
    expect(unwrapStringKeyValue({ value: null })).toBe('');
  });
});

describe('tryPrettyJson / looksLikeJsonText', () => {
  it('pretty-prints objects and arrays', () => {
    expect(looksLikeJsonText('{"a":1}')).toBe(true);
    expect(tryPrettyJson('{"name":"张三","level":"vip"}')).toBe(
      '{\n  "name": "张三",\n  "level": "vip"\n}',
    );
    expect(tryPrettyJson('[1,2]')).toBe('[\n  1,\n  2\n]');
  });

  it('returns null for plain text and invalid JSON', () => {
    expect(looksLikeJsonText('hello')).toBe(false);
    expect(tryPrettyJson('hello')).toBeNull();
    expect(tryPrettyJson('{not json')).toBeNull();
    expect(tryPrettyJson('42')).toBeNull();
  });

  it('rejects whitespace-only and empty', () => {
    expect(looksLikeJsonText('')).toBe(false);
    expect(looksLikeJsonText('   ')).toBe(false);
    expect(tryPrettyJson('')).toBeNull();
  });
});

describe('initialStringEditorValue', () => {
  it('pretty-prints JSON stored in the string wrapper', () => {
    expect(
      initialStringEditorValue({ value: '{"name":"张三","level":"vip"}' }),
    ).toBe('{\n  "name": "张三",\n  "level": "vip"\n}');
  });

  it('keeps non-JSON strings as-is', () => {
    expect(initialStringEditorValue({ value: 'plain' })).toBe('plain');
  });

  it('handles raw string input', () => {
    expect(initialStringEditorValue('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(initialStringEditorValue('plain')).toBe('plain');
  });
});

/** Build a latin1 string from raw bytes (same path as Redis binary string values). */
function latin1FromBytes(bytes: number[]): string {
  return String.fromCharCode(...bytes);
}

describe('valueLooksCompressed (PR-1 decompress detection)', () => {
  it('returns false for short or empty input', () => {
    expect(valueLooksCompressed('')).toBe(false);
    expect(valueLooksCompressed('ab')).toBe(false);
    expect(valueLooksCompressed('abc')).toBe(false);
  });

  it('detects gzip magic bytes (1f 8b)', () => {
    const gzipish = latin1FromBytes([0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00]);
    expect(valueLooksCompressed(gzipish)).toBe(true);
  });

  it('detects zlib headers (78 01 / 78 9c / 78 da / 78 5e)', () => {
    expect(valueLooksCompressed(latin1FromBytes([0x78, 0x01, 0, 0, 0, 0]))).toBe(true);
    expect(valueLooksCompressed(latin1FromBytes([0x78, 0x9c, 0, 0, 0, 0]))).toBe(true);
    expect(valueLooksCompressed(latin1FromBytes([0x78, 0xda, 0, 0, 0, 0]))).toBe(true);
    expect(valueLooksCompressed(latin1FromBytes([0x78, 0x5e, 0, 0, 0, 0]))).toBe(true);
  });

  it('detects base64-encoded gzip', () => {
    const magicB64 = Buffer.from([
      0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00,
      0x00, 0x00, 0x00,
    ]).toString('base64');
    expect(valueLooksCompressed(magicB64)).toBe(true);
  });

  it('returns false for plain text and ordinary JSON', () => {
    expect(valueLooksCompressed('hello world this is long enough')).toBe(false);
    expect(valueLooksCompressed('{"name":"test","value":12345}')).toBe(false);
  });
});

describe('tryDecompressString (PR-1 gzip/zlib view)', () => {
  it('returns null when DecompressionStream is unavailable', async () => {
    const original = globalThis.DecompressionStream;
    // @ts-expect-error intentional delete for test
    delete globalThis.DecompressionStream;
    try {
      expect(await tryDecompressString(latin1FromBytes([0x1f, 0x8b, 0x08, 0x00]))).toBeNull();
    } finally {
      globalThis.DecompressionStream = original;
    }
  });

  it('returns null for empty / too-short payloads', async () => {
    expect(await tryDecompressString('')).toBeNull();
    expect(await tryDecompressString('x')).toBeNull();
  });

  it('returns null for non-compressed text', async () => {
    expect(await tryDecompressString('hello plain text that is not compressed')).toBeNull();
  });

  it('decompresses real zlib payload', async () => {
    const zlibBytes = Buffer.from('eJzLSM3JydetyslM0i0oMgQAKLsFMw==', 'base64');
    const raw = latin1FromBytes([...zlibBytes]);
    const result = await tryDecompressString(raw);
    expect(result).not.toBeNull();
    expect(result!.codec).toBe('zlib');
    expect(result!.text).toContain('hello-zlib');
    expect(result!.bytes).toBeGreaterThan(0);
  });

  it('decompresses real gzip payload', async () => {
    const { gzipSync } = await import('node:zlib');
    const gz = gzipSync(Buffer.from('hello-gzip-pr1', 'utf8'));
    const raw = latin1FromBytes([...gz]);
    expect(valueLooksCompressed(raw)).toBe(true);
    const result = await tryDecompressString(raw);
    expect(result).not.toBeNull();
    expect(result!.codec).toBe('gzip');
    expect(result!.text).toBe('hello-gzip-pr1');
  });

  it('decompresses base64-wrapped gzip', async () => {
    const { gzipSync } = await import('node:zlib');
    const gz = gzipSync(Buffer.from('{"ok":true}', 'utf8'));
    const b64 = gz.toString('base64');
    const result = await tryDecompressString(b64);
    expect(result).not.toBeNull();
    expect(result!.codec).toBe('gzip');
    expect(result!.text).toContain('ok');
  });

  it('exports DECOMPRESS_MAX_BYTES guard constant', () => {
    expect(DECOMPRESS_MAX_BYTES).toBe(50 * 1024 * 1024);
  });
});
