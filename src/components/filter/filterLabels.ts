/**
 * The value labels of a filter condition.
 *
 * Both the collapsed chip and the expanded editor show the same
 * "column operator value" text, and the panel summary repeats it once more,
 * so the formatting lives in one place.
 */
import type { FilterCondition } from '../../types';
import { formatCell } from '../../lib/formatters';
import { valueInputFor } from '../../lib/filterOperators';
import type { I18nKey } from '../../locales';

/** The label of an operator, e.g. `filter.eq`. */
export function opLabel(op: string, t: (key: I18nKey) => string): string {
  return t(`filter.${op}`);
}

/** The text of one condition, e.g. `name = alice`. */
export function conditionSummary(f: FilterCondition, t: (key: I18nKey) => string): string {
  const op = opLabel(f.operator, t);
  const kind = valueInputFor(f.operator);
  if (kind === 'none') return `${f.column} ${op}`;
  if (kind === 'pair') {
    const [lower = '', upper = ''] = Array.isArray(f.value) ? f.value : [];
    return `${f.column} ${op} ${lower}, ${upper}`;
  }
  if (kind === 'tokens') {
    const tokens = Array.isArray(f.value) ? f.value : [];
    return `${f.column} ${op} ${tokens.join(', ')}`;
  }
  const value =
    f.value === null || f.value === undefined || f.value === '' ? '…' : formatCell(f.value);
  return `${f.column} ${op} ${value}`;
}
