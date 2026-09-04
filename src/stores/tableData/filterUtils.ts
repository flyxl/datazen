import type { FilterCondition } from '../../types';

/** Incomplete filters (just added / cleared value) must not hit the backend. */
export function isCompleteFilter(filter: FilterCondition): boolean {
  if (!filter.column) return false;
  if (filter.operator === 'isNull' || filter.operator === 'isNotNull') return true;
  if (filter.value === null || filter.value === undefined) return false;
  if (typeof filter.value === 'string' && filter.value === '') return false;
  if (Array.isArray(filter.value) && filter.value.length === 0) return false;
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
