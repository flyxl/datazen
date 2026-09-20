/**
 * Temporal value handling for condition editing.
 *
 * A `timestamp` column compared against an unquoted or malformed literal is
 * the classic `operator does not exist: timestamp > integer` runtime failure.
 * The condition dialog therefore (a) swaps the text input for the native
 * picker matching the column kind, (b) rejects literals the engine would
 * refuse, and (c) normalises the `datetime-local` "T" separator to the
 * space-separated canonical form every supported dialect accepts.
 */
import { classifyColumnType, TypeCategory } from './typeCategory';

export type TemporalKind = 'date' | 'time' | 'datetime';

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;
const TIME_RE = /^(\d{2}):(\d{2})(:(\d{2}))?$/;
const DATETIME_RE = /^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2})(:(\d{2}))?$/;

/** True when the raw column type is temporal (date / time / timestamp family). */
export function isTemporalColumnType(dataType?: string): boolean {
  if (!dataType) return false;
  return classifyColumnType(dataType) === TypeCategory.Temporal;
}

/** Which native picker expresses this temporal column without a text box. */
export function temporalInputType(dataType?: string): 'date' | 'time' | 'datetime-local' | 'text' {
  if (!isTemporalColumnType(dataType)) return 'text';
  const normalized = dataType!
    .toLowerCase()
    .trim()
    .replace(/\s*\(.*\)\s*$/, '');
  // `datetime` must be tested before the date prefix, and `timestamp` before
  // `time`, or the shorter names swallow the longer types.
  if (normalized.startsWith('datetime') || normalized.includes('stamp')) return 'datetime-local';
  if (normalized.startsWith('time')) return 'time';
  if (normalized.startsWith('date')) return 'date';
  return 'datetime-local';
}

function kindOf(dataType: string): TemporalKind {
  const input = temporalInputType(dataType);
  if (input === 'date') return 'date';
  if (input === 'time') return 'time';
  return 'datetime';
}

function realDate(y: string, m: string, d: string): boolean {
  const date = new Date(Number(y), Number(m) - 1, Number(d));
  return (
    date.getFullYear() === Number(y) &&
    date.getMonth() === Number(m) - 1 &&
    date.getDate() === Number(d)
  );
}

function matchesKind(value: string, kind: TemporalKind): boolean {
  if (kind === 'date') {
    if (!DATE_RE.test(value)) return false;
    const [y, m, d] = value.split('-');
    return realDate(y!, m!, d!);
  }
  if (kind === 'time') {
    const match = TIME_RE.exec(value);
    if (!match) return false;
    return Number(match[1]) < 24 && Number(match[2]) < 60 && Number(match[4] ?? 0) < 60;
  }
  const match = DATETIME_RE.exec(value);
  if (!match) return false;
  return (
    realDate(match[1]!, match[2]!, match[3]!) &&
    Number(match[4]) < 24 &&
    Number(match[5]) < 60 &&
    Number(match[7] ?? 0) < 60
  );
}

/**
 * Validate a user-entered literal for a temporal column.
 * Returns `null` when acceptable, `'invalid'` otherwise. Comma-separated
 * IN-lists are validated entry by entry; non-temporal columns always pass.
 */
export function validateTemporalValue(value: string, dataType?: string): 'invalid' | null {
  if (!isTemporalColumnType(dataType)) return null;
  if (value.trim() === '') return 'invalid';
  const kind = kindOf(dataType!);
  const entries = value.includes(',') ? value.split(',') : [value];
  for (const entry of entries) {
    if (!matchesKind(entry.trim(), kind)) return 'invalid';
  }
  return null;
}

/**
 * Rewrite a valid temporal literal into the canonical SQL form:
 * `2026-09-20T01:13` (datetime-local) → `2026-09-20 01:13`.
 */
export function normalizeTemporalValue(value: string, dataType?: string): string {
  if (!isTemporalColumnType(dataType)) return value;
  return value.replace(/^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}(:\d{2})?)$/, '$1 $2');
}
