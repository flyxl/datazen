/**
 * Relative time formatting utility (no external date library dependency).
 *
 * Uses `Intl.RelativeTimeFormat` for minute/hour/day buckets and falls back to a
 * short locale date beyond the 7-day window.
 */

const MINUTE_MS = 60_000;
const HOUR_MS = 3_600_000;
const DAY_MS = 86_400_000;
const MAX_RELATIVE_DAYS = 7;

/**
 * Upper bound of the relative window (7 days). Deltas beyond this fall back
 * to a short locale date — exported so locale-aware callers (e.g. the
 * i18n-composed label in RecentQueriesList) share the exact cutoff.
 */
export const RELATIVE_WINDOW_MS = MAX_RELATIVE_DAYS * DAY_MS;

export interface RelativeTimeParts {
  /** Signed value passed to Intl.RelativeTimeFormat (negative = past). */
  value: number;
  /** Intl.RelativeTimeFormat unit: "minute" | "hour" | "day". */
  unit: 'minute' | 'hour' | 'day';
}

function resolveNow(now?: number): number {
  const resolved = now ?? Date.now();
  return Number.isFinite(resolved) ? resolved : Date.now();
}

/**
 * Splits a past timestamp into a value/unit pair for `Intl.RelativeTimeFormat.format`.
 * Timestamps within the last minute (or in the near future, tolerating clock skew)
 * snap to the minute bucket with value 0.
 */
export function getRelativeTimeParts(timestamp: number, now?: number): RelativeTimeParts | null {
  if (!Number.isFinite(timestamp) || timestamp <= 0) {
    return null;
  }
  const reference = resolveNow(now);
  const delta = reference - timestamp;
  if (delta < MINUTE_MS) {
    return { value: 0, unit: 'minute' };
  }
  if (delta < HOUR_MS) {
    return { value: -Math.floor(delta / MINUTE_MS), unit: 'minute' };
  }
  if (delta < DAY_MS) {
    return { value: -Math.floor(delta / HOUR_MS), unit: 'hour' };
  }
  return { value: -Math.floor(delta / DAY_MS), unit: 'day' };
}

/**
 * Formats a past timestamp as a concise relative label:
 * - "just now" for < 60s
 * - "N minute(s)/hour(s)/day(s) ago" up to 7 days
 * - short locale date (e.g. "May 4, 2026") beyond 7 days
 * Returns "" for invalid timestamps.
 */
export function formatRelativeTime(timestamp: number, now?: number): string {
  if (!Number.isFinite(timestamp) || timestamp <= 0) {
    return '';
  }
  const reference = resolveNow(now);
  const delta = reference - timestamp;
  if (delta < MINUTE_MS) {
    // Future timestamps (clock skew) also land here.
    return 'just now';
  }
  if (delta <= MAX_RELATIVE_DAYS * DAY_MS) {
    const parts = getRelativeTimeParts(timestamp, reference);
    if (!parts) {
      return '';
    }
    const formatter = new Intl.RelativeTimeFormat('en', { numeric: 'auto', style: 'long' });
    return formatter.format(parts.value, parts.unit);
  }
  return new Date(timestamp).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}
