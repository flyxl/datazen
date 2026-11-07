/** Format byte size to human-readable string (B / KB / MB). */
export function formatSize(size: number): string {
  if (!size || size < 0) return '—';
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Compact server count for a narrow readout: `52`, `1.2k`, `12k`, `1.4M`.
 *
 * Deliberately not a translated unit — this is a measurement, not copy, and the
 * status bar cannot afford `12,345` beside the other facts it prints.
 *
 * `NaN`, negatives and non-integers collapse to `0`: this formats a count that
 * is already known to be non-negative, and a formatter is the wrong place to
 * decide what an impossible count means.
 */
export function formatCompactCount(value: number): string {
  const n = Number.isFinite(value) && value > 0 ? Math.floor(value) : 0;
  if (n < 1000) return String(n);
  if (n < 10_000) {
    const thousands = (n / 1000).toFixed(1);
    return `${thousands.endsWith('.0') ? thousands.slice(0, -2) : thousands}k`;
  }
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${Math.round(n / 1_000_000)}M`;
}
