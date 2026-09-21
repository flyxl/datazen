//! Pure byte→text view renderers for the Redis value viewer (R9).
//!
//! Each renderer takes a `Uint8Array` (already codec-decoded upstream) and a
//! view id and returns a `RenderedView`. Renderers never throw for hostile
//! bytes: non-UTF-8 / undecodable input degrades to a lossy rendering plus an
//! `invalidUtf8` flag or a structured `error` variant.

/** Full view list rendered by the view button group. */
export const VIEWS = [
  'utf8',
  'ascii',
  'binary',
  'hex',
  'base64',
  'json',
  'unicodeJson',
  'yaml',
  'xml',
] as const;

export type ViewMode = (typeof VIEWS)[number];

export interface HexRow {
  offset: number;
  hex: string;
  ascii: string;
}

export type RenderedView =
  | { kind: 'text'; text: string; invalidUtf8: boolean }
  | { kind: 'hex'; rows: HexRow[]; totalBytes: number }
  | { kind: 'error'; message: string };

/** Max rows rendered for hex / binary views (keeps the DOM bounded). */
export const MAX_VIEW_ROWS = 4096;

const BYTES_PER_HEX_ROW = 16;
const BYTES_PER_BIN_ROW = 8;

function decodeUtf8(bytes: Uint8Array): { text: string; invalid: boolean } {
  try {
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    return { text, invalid: false };
  } catch {
    const text = new TextDecoder('utf-8', { fatal: false }).decode(bytes);
    return { text, invalid: true };
  }
}

function asciiGutter(byte: number): string {
  // Printable ASCII 0x21..0x7e; space and controls render as '.'.
  return byte >= 0x21 && byte <= 0x7e ? String.fromCharCode(byte) : '.';
}

export function renderHex(bytes: Uint8Array, maxRows = MAX_VIEW_ROWS): RenderedView {
  const rows: HexRow[] = [];
  const rowCount = Math.ceil(bytes.length / BYTES_PER_HEX_ROW);
  const limit = Math.min(rowCount, maxRows);
  for (let r = 0; r < limit; r++) {
    const start = r * BYTES_PER_HEX_ROW;
    const slice = bytes.subarray(start, start + BYTES_PER_HEX_ROW);
    let hex = '';
    let ascii = '';
    for (let i = 0; i < BYTES_PER_HEX_ROW; i++) {
      if (i < slice.length) {
        hex += slice[i].toString(16).padStart(2, '0') + ' ';
        ascii += asciiGutter(slice[i]);
      } else {
        hex += '   ';
      }
    }
    rows.push({ offset: start, hex: hex.trimEnd(), ascii });
  }
  return { kind: 'hex', rows, totalBytes: bytes.length };
}

export function renderBinary(bytes: Uint8Array, maxRows = MAX_VIEW_ROWS): RenderedView {
  const lines: string[] = [];
  const rowCount = Math.ceil(bytes.length / BYTES_PER_BIN_ROW);
  const limit = Math.min(rowCount, maxRows);
  for (let r = 0; r < limit; r++) {
    const start = r * BYTES_PER_BIN_ROW;
    const slice = bytes.subarray(start, start + BYTES_PER_BIN_ROW);
    const groups: string[] = [];
    for (let i = 0; i < slice.length; i++) groups.push(slice[i].toString(2).padStart(8, '0'));
    lines.push(`${start.toString(16).padStart(8, '0')}  ${groups.join(' ')}`);
  }
  if (rowCount > limit) lines.push(`… ${rowCount - limit} more row(s)`);
  return { kind: 'text', text: lines.join('\n'), invalidUtf8: false };
}

function renderAscii(bytes: Uint8Array): RenderedView {
  let out = '';
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i];
    if (b === 0x0a || b === 0x0d || b === 0x09) out += String.fromCharCode(b);
    else out += asciiGutter(b);
  }
  return { kind: 'text', text: out, invalidUtf8: false };
}

function prettyJson(text: string, escapeNonAscii: boolean): RenderedView {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (e) {
    return { kind: 'error', message: e instanceof Error ? e.message : String(e) };
  }
  let out: string;
  try {
    out = JSON.stringify(parsed, null, 2) ?? String(parsed);
  } catch (e) {
    return { kind: 'error', message: e instanceof Error ? e.message : String(e) };
  }
  if (escapeNonAscii) {
    out = out.replace(
      /[\u0080-\uffff]/g,
      (ch) => '\\u' + ch.charCodeAt(0).toString(16).padStart(4, '0'),
    );
  }
  return { kind: 'text', text: out, invalidUtf8: false };
}

/**
 * Render decoded bytes in the requested view. `maxRows` bounds the hex/binary
 * output for very large payloads.
 */
export function renderView(
  bytes: Uint8Array,
  view: ViewMode,
  maxRows = MAX_VIEW_ROWS,
): RenderedView {
  switch (view) {
    case 'utf8': {
      const { text, invalid } = decodeUtf8(bytes);
      return { kind: 'text', text, invalidUtf8: invalid };
    }
    case 'ascii':
      return renderAscii(bytes);
    case 'binary':
      return renderBinary(bytes, maxRows);
    case 'hex':
      return renderHex(bytes, maxRows);
    case 'base64':
      // Lazy import avoided: base64 encode is dependency-free here.
      return { kind: 'text', text: encodeBase64(bytes), invalidUtf8: false };
    case 'json': {
      const { text, invalid } = decodeUtf8(bytes);
      if (invalid) return { kind: 'error', message: 'value is not valid UTF-8 text' };
      return prettyJson(text, true);
    }
    case 'unicodeJson': {
      const { text, invalid } = decodeUtf8(bytes);
      if (invalid) return { kind: 'error', message: 'value is not valid UTF-8 text' };
      return prettyJson(text, false);
    }
    case 'yaml':
    case 'xml': {
      // No dedicated parser dependency (keeps the bundle lean and the hostile
      // surface at zero): render the decoded text and let the raw text speak.
      const { text, invalid } = decodeUtf8(bytes);
      return { kind: 'text', text, invalidUtf8: invalid };
    }
    default:
      return { kind: 'error', message: `unknown view '${String(view)}'` };
  }
}

function encodeBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  // eslint-disable-next-line no-restricted-globals
  return btoa(binary);
}
