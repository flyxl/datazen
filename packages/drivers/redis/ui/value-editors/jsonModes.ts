//! R3 — JSON three-state display transforms (pure, no I/O).
//!
//! The editor exposes four views: `tree` (path editing) plus three text views
//! (`raw` verbatim, `pretty` 2-space, `minify` single line). The three text
//! views are formatting-only projections of the same document, so switching
//! between them is lossless as long as the text is valid JSON.

export type JsonDisplayMode = 'tree' | 'raw' | 'pretty' | 'minify';
export type JsonTextMode = Exclude<JsonDisplayMode, 'tree'>;

export const JSON_TEXT_MODES: readonly JsonTextMode[] = ['raw', 'pretty', 'minify'];
export const JSON_DISPLAY_MODES: readonly JsonDisplayMode[] = ['tree', 'raw', 'pretty', 'minify'];

/** True when `text` parses as JSON. */
export function isValidJson(text: string): boolean {
  try {
    JSON.parse(text);
    return true;
  } catch {
    return false;
  }
}

/**
 * Reformat `text` for the requested text mode. Invalid JSON is passed through
 * unchanged so an in-progress edit is never silently destroyed.
 */
export function formatJson(text: string, mode: JsonTextMode): string {
  if (mode === 'raw') return text;
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return text;
  }
  return mode === 'pretty' ? JSON.stringify(parsed, null, 2) : JSON.stringify(parsed);
}
