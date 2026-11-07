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
import { toPair, toTokens } from './filterValues';

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
    // The same parser the editor uses, so the collapsed chip shows what the
    // expanded row would — including the comma separated form of an older
    // draft, which the array-only reading rendered as an empty value.
    const [lower, upper] = toPair(f.value);
    return `${f.column} ${op} ${lower}, ${upper}`;
  }
  if (kind === 'tokens') {
    return `${f.column} ${op} ${toTokens(f.value).join(', ')}`;
  }
  const value =
    f.value === null || f.value === undefined || f.value === '' ? '…' : formatCell(f.value);
  return `${f.column} ${op} ${value}`;
}
