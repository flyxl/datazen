import { t } from '../../locales/t';
import type { Value } from '../../types';

/**
 * How a cell value reads inside a dirty-cell tooltip.
 *
 * Two values need a word instead of the empty string, because an empty
 * tooltip is indistinguishable from "no value":
 *
 * - SQL `NULL` (the database's NULL reaches the UI as `null`);
 * - the empty string — which the inline editor cannot produce (it commits
 *   `null` instead), but a paste can.
 *
 * A JSON column is serialised rather than stringified with `String()`, so the
 * tooltip shows the value and not `[object Object]`.
 */
export function formatDirtyValue(value: Value | null): string {
  if (value === null) return t('tableData.dirtyValueNull');
  if (value === '') return t('tableData.dirtyValueEmpty');
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

/**
 * The word a staged write expression reads as in a dirty tooltip (DBR-05):
 * the intent in the user's words, not the placeholder value the wire
 * invariant keeps in `currentValues`.
 */
export function formatDirtyExpression(): string {
  return t('tableData.dirtyValueExpression');
}

/** The tooltip of a dirty cell: the original value and the value it becomes. */
export function dirtyCellTooltip(
  original: Value | null,
  next: Value | null,
  expression?: string,
): string {
  const to = expression === 'currentTimestamp' ? formatDirtyExpression() : formatDirtyValue(next);
  return `${formatDirtyValue(original)} \u2192 ${to}`;
}

/**
 * The screen-reader label of a dirty cell — the tooltip in words.
 *
 * It uses the standalone translation function on purpose: the label is built
 * per cell, per render, and handing a translate function down to every row
 * would make each row subscribe to the locale.
 */
export function dirtyCellAriaLabel(
  original: Value | null,
  next: Value | null,
  expression?: string,
): string {
  return t('tableData.dirtyCellAria', {
    original: formatDirtyValue(original),
    next: expression === 'currentTimestamp' ? formatDirtyExpression() : formatDirtyValue(next),
  });
}
