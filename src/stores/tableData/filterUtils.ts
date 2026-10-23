import type { FilterCondition } from '../../types';
import { valueInputFor } from '../../lib/filterOperators';

/**
 * Whether a condition carries everything it needs to be sent to the database.
 *
 * The predicate mirrors the back end's `filter_is_complete`
 * (`packages/feature-core/.../filter_sql`): the operators that carry no value
 * are complete on their own, the set operators need at least one value, and
 * the range operators need exactly two bounds.
 *
 * A condition that is not complete is **never dropped silently**: the editor
 * marks it and blocks `Apply`, and the back end refuses it with
 * `grid.filter.incomplete`.
 */
export function isCompleteFilter(filter: FilterCondition): boolean {
  if (!filter.column) return false;
  const kind = valueInputFor(filter.operator);
  if (kind === 'none') return true;

  const value = filter.value;
  if (value === null || value === undefined) return false;

  if (kind === 'pair') return isCompletePair(value);
  if (kind === 'tokens') return isCompleteTokenList(value);

  // A single literal: only the empty string counts as missing. A number, a
  // boolean or a JSON value is a value the database can compare.
  return !(typeof value === 'string' && value === '');
}

/** The two bounds of a range operator, in the order the back end reads them. */
function isCompletePair(value: FilterCondition['value']): boolean {
  if (Array.isArray(value)) {
    return value.length === 2 && value.every((bound) => isFilled(bound));
  }
  if (typeof value === 'string') {
    const parts = value.split(',');
    if (parts.length !== 2) return false;
    return parts.every((part) => part.trim() !== '');
  }
  return false;
}

/** The values of a set operator: one is enough, none is not. */
function isCompleteTokenList(value: FilterCondition['value']): boolean {
  if (Array.isArray(value)) return value.length > 0;
  if (typeof value === 'string') return value.split(',').some((part) => part.trim() !== '');
  return true;
}

function isFilled(value: unknown): boolean {
  if (value === null || value === undefined) return false;
  if (typeof value === 'string') return value.trim() !== '';
  return true;
}

export function cloneFilters(filters: FilterCondition[]): FilterCondition[] {
  return filters.map((f) => ({ ...f }));
}

export function filterDraftEqualsApplied(
  draftFilters: FilterCondition[],
  draftLogic: 'and' | 'or',
  appliedFilters: FilterCondition[],
  appliedLogic: 'and' | 'or',
): boolean {
  return (
    draftLogic === appliedLogic && JSON.stringify(draftFilters) === JSON.stringify(appliedFilters)
  );
}
