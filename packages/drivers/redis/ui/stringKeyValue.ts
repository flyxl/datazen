/** Unwrap Redis string `KeyDetail.value` (`{ value: "..." }` or a raw string). */
export function unwrapStringKeyValue(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object' && 'value' in value) {
    const inner = (value as { value: unknown }).value;
    if (typeof inner === 'string') return inner;
    if (inner == null) return '';
    return String(inner);
  }
  if (value == null) return '';
  return String(value);
}

/** True when trimmed text looks like a JSON object or array. */
export function looksLikeJsonText(text: string): boolean {
  const trimmed = text.trim();
  return trimmed.startsWith('{') || trimmed.startsWith('[');
}

/** Pretty-print JSON object/array text, or null if not JSON. */
export function tryPrettyJson(text: string): string | null {
  if (!looksLikeJsonText(text)) return null;
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed === null || typeof parsed !== 'object') return null;
    return JSON.stringify(parsed, null, 2);
  } catch {
    return null;
  }
}

export function initialStringEditorValue(value: unknown): string {
  const raw = unwrapStringKeyValue(value);
  return tryPrettyJson(raw) ?? raw;
}

/** Max decompressed payload size (50 MiB), matching common GUI limits. */
export const DECOMPRESS_MAX_BYTES = 50 * 1024 * 1024;

export type DecompressCodec = 'gzip' | 'zlib' | 'deflate';

export interface DecompressResult {
  codec: DecompressCodec;
  text: string;
  bytes: number;
}

function base64ToUint8Array(b64: string): Uint8Array | null {
  try {
    const bin = atob(b64.replace(/\s/g, ''));
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  } catch {
    return null;
  }
}

/** Detect binary-looking payloads that may be compressed (base64 or latin1 bytes). */
export function valueLooksCompressed(raw: string): boolean {
  if (!raw || raw.length < 4) return false;
  // gzip magic 1f 8b
  if (raw.charCodeAt(0) === 0x1f && raw.charCodeAt(1) === 0x8b) return true;
  // zlib header often 78 01 / 78 9c / 78 da
  if (raw.charCodeAt(0) === 0x78 && [0x01, 0x9c, 0xda, 0x5e].includes(raw.charCodeAt(1))) {
    return true;
  }
  // base64 that decodes to gzip/zlib
  if (/^[A-Za-z0-9+/\r\n]+=*$/.test(raw.slice(0, 64)) && raw.length > 16) {
    const bytes = base64ToUint8Array(raw);
    if (bytes && bytes.length >= 2) {
      if (bytes[0] === 0x1f && bytes[1] === 0x8b) return true;
      if (bytes[0] === 0x78) return true;
    }
  }
  return false;
}

/** Copy Uint8Array into a real ArrayBuffer suitable for Blob. */
function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(copy).set(bytes);
  return copy;
}

/**
 * Attempt gzip / zlib / raw-deflate decompression in the browser via DecompressionStream.
 * Returns null when the payload is not compressed or decompression fails.
 */
export async function tryDecompressString(raw: string): Promise<DecompressResult | null> {
  if (typeof DecompressionStream === 'undefined') return null;

  let bytes: Uint8Array | null = null;
  if (raw.charCodeAt(0) === 0x1f || raw.charCodeAt(0) === 0x78) {
    bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i) & 0xff;
  } else {
    bytes = base64ToUint8Array(raw);
  }
  if (!bytes || bytes.length < 2) return null;

  const attempts: Array<{ codec: DecompressCodec; format: CompressionFormat }> = [];
  if (bytes[0] === 0x1f && bytes[1] === 0x8b) {
    attempts.push({ codec: 'gzip', format: 'gzip' });
  } else if (bytes[0] === 0x78) {
    attempts.push({ codec: 'zlib', format: 'deflate' });
  } else {
    attempts.push({ codec: 'gzip', format: 'gzip' }, { codec: 'deflate', format: 'deflate' });
  }

  for (const { codec, format } of attempts) {
    try {
      const stream = new Blob([toArrayBuffer(bytes)])
        .stream()
        .pipeThrough(new DecompressionStream(format));
      const ab = await new Response(stream).arrayBuffer();
      if (ab.byteLength > DECOMPRESS_MAX_BYTES) {
        return {
          codec,
          text: `[decompressed size ${ab.byteLength} exceeds ${DECOMPRESS_MAX_BYTES} byte limit]`,
          bytes: ab.byteLength,
        };
      }
      const text = new TextDecoder('utf-8', { fatal: false }).decode(ab);
      return { codec, text: tryPrettyJson(text) ?? text, bytes: ab.byteLength };
    } catch {
      // try next codec
    }
  }
  return null;
}
