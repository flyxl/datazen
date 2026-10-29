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

/** The tooltip of a dirty cell: the original value and the value it becomes. */
export function dirtyCellTooltip(original: Value | null, next: Value | null): string {
  return `${formatDirtyValue(original)} \u2192 ${formatDirtyValue(next)}`;
}

/**
 * The screen-reader label of a dirty cell — the tooltip in words.
 *
 * It uses the standalone translation function on purpose: the label is built
 * per cell, per render, and handing a translate function down to every row
 * would make each row subscribe to the locale.
 */
export function dirtyCellAriaLabel(original: Value | null, next: Value | null): string {
  return t('tableData.dirtyCellAria', {
    original: formatDirtyValue(original),
    next: formatDirtyValue(next),
  });
}
