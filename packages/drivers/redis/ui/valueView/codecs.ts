//! Browser-side codec transforms for the Redis value viewer (R8).
//!
//! gzip / zlib / raw-deflate run through the platform `DecompressionStream`;
//! the base64 codec decodes a stored base64 *text* back into bytes. The Msgpack
//! / Pickle / PHP / Java codecs are NOT handled here — they round-trip through
//! the Rust `decode_value` command (see `redisInvoke.invokeDecodeValue`),
//! because JS deserialisers for those formats are heavy and unsafe on hostile
//! input.

/** Full codec list rendered by the decode button group. */
export const CODECS = [
  'none',
  'gzip',
  'zlib',
  'deflate',
  'base64',
  'msgpack',
  'pickle',
  'php',
  'java',
] as const;

export type Codec = (typeof CODECS)[number];

/** Codecs resolved entirely in the browser from the raw bytes. */
export const BROWSER_CODECS: readonly Codec[] = ['none', 'gzip', 'zlib', 'deflate', 'base64'];
/** Codecs delegated to the backend `decode_value` command. */
export const BACKEND_CODECS: readonly Codec[] = ['msgpack', 'pickle', 'php', 'java'];

/** The subset of codecs the Rust backend decodes. */
export type BackendCodec = 'msgpack' | 'pickle' | 'php' | 'java';

export function isBackendCodec(codec: Codec): codec is BackendCodec {
  return (BACKEND_CODECS as readonly string[]).includes(codec);
}

/** Hard ceiling on any decoded payload (50 MiB), matching `DECOMPRESS_MAX_BYTES`. */
export const MAX_CODEC_BYTES = 50 * 1024 * 1024;

const B64_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

/** base64 (STANDARD, padded) → bytes. Throws on malformed input. */
export function base64ToBytes(b64: string): Uint8Array {
  const cleaned = b64.replace(/[\s]/g, '').replace(/=+$/, '');
  if (cleaned.length % 4 === 1) throw new Error('invalid base64 length');
  const lookup = new Int16Array(256).fill(-1);
  for (let i = 0; i < B64_ALPHABET.length; i++) lookup[B64_ALPHABET.charCodeAt(i)] = i;
  const outLen = Math.floor((cleaned.length * 3) / 4);
  const out = new Uint8Array(outLen);
  let o = 0;
  let buffer = 0;
  let bits = 0;
  for (let i = 0; i < cleaned.length; i++) {
    const v = lookup[cleaned.charCodeAt(i)];
    if (v < 0) throw new Error(`invalid base64 character at index ${i}`);
    buffer = (buffer << 6) | v;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out[o++] = (buffer >> bits) & 0xff;
    }
  }
  return out.subarray(0, o);
}

/** bytes → base64 (STANDARD, padded). */
export function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  // eslint-disable-next-line no-restricted-globals
  return btoa(binary);
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(copy).set(bytes);
  return copy;
}

function decompressFormat(codec: Codec): CompressionFormat | null {
  switch (codec) {
    case 'gzip':
      return 'gzip';
    case 'zlib':
      // `deflate` format == zlib-wrapped stream
      return 'deflate';
    case 'deflate':
      // bare DEFLATE (no zlib header)
      return 'deflate-raw';
    default:
      return null;
  }
}

/**
 * Apply a browser codec to the raw bytes and return the transformed bytes.
 * `none` is identity; `base64` decodes the bytes (read as ASCII text) into the
 * payload they encode. Throws when decompression fails or exceeds the cap.
 */
export async function applyBrowserCodec(bytes: Uint8Array, codec: Codec): Promise<Uint8Array> {
  if (codec === 'none') return bytes;
  if (codec === 'base64') {
    const text = new TextDecoder('latin1').decode(bytes);
    return base64ToBytes(text);
  }
  const format = decompressFormat(codec);
  if (!format) throw new Error(`codec '${codec}' is not a browser codec`);
  if (typeof DecompressionStream === 'undefined') {
    throw new Error('this environment does not support DecompressionStream');
  }
  const stream = new Blob([toArrayBuffer(bytes)])
    .stream()
    .pipeThrough(new DecompressionStream(format));
  const ab = await new Response(stream).arrayBuffer();
  if (ab.byteLength > MAX_CODEC_BYTES) {
    throw new Error(`decompressed size ${ab.byteLength} exceeds ${MAX_CODEC_BYTES} byte limit`);
  }
  return new Uint8Array(ab);
}
