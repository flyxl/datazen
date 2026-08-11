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
