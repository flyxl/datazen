/**
 * The text an editor starts with.
 *
 * The value can be anything the wire carries, so the conversion is
 * defensive: a structured value becomes its JSON text, and a value that
 * cannot be serialised falls back to `String(value)` instead of throwing in
 * the middle of a render.
 *
 * (This is the function that used to live inside `EditableCell.tsx`; it
 * moved here so the editor family could grow without the compatibility
 * shell growing with it — `EditableCell` re-exports it.)
 */
export function toEditString(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'object') {
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }
  return String(value);
}

/**
 * The length of a text in **code points**.
 *
 * `[...text].length`, not `text.length`: the thresholds are about how much
 * text the user sees, and a hundred emoji are a hundred characters, not two
 * hundred (05 §7 #10).
 */
export function countCodePoints(text: string): number {
  return [...text].length;
}

/**
 * The size of a value in bytes, as far as the editor can tell.
 *
 * Only the metadata is used — the editor never serialises a big byte array
 * to measure it (05 §4.5 #5).
 */
export function valueByteSize(value: unknown): number {
  if (value === null || value === undefined) return 0;
  if (typeof value === 'string') return new TextEncoder().encode(value).length;
  if (typeof value === 'number' || typeof value === 'boolean') return String(value).length;
  if (Array.isArray(value)) return value.length;
  return 0;
}
