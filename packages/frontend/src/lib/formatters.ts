import { t } from '../locales/t';

/**
 * Whether the text carries an explicit UTC designator (`Z`) or numeric offset
 * (`+08:00`, `-0800`), i.e. whether it denotes an instant rather than wall clock.
 */
function hasZoneDesignator(value: string): boolean {
  return /(?:[zZ]|[+-]\d{2}:?\d{2})$/.test(value.trim());
}

export function formatTimestamp(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'number') {
    const d = new Date(value);
    return Number.isNaN(d.getTime()) ? String(value) : d.toISOString();
  }
  if (typeof value === 'string') {
    // Zone-less database text (`2026-03-01`, `2026-03-01 00:15:30.123`) is
    // wall-clock, not an instant. `new Date()` reads that form as *local* time,
    // so round-tripping it through `toISOString()` shifted every such cell by
    // the local UTC offset and appended a bogus `Z`.
    if (!hasZoneDesignator(value)) return value;
    const d = new Date(value);
    if (!Number.isNaN(d.getTime())) return d.toISOString();
  }
  return String(value);
}

export function formatCell(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'object') {
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }
  return String(value);
}

export function formatLastConnected(iso?: string): string {
  if (!iso) return t('conn.neverConnected');
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}
