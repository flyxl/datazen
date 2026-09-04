import { describe, expect, it } from 'vitest';
import {
  cloneFilters,
  filterDraftEqualsApplied,
  isCompleteFilter,
} from '../filterUtils';
import type { FilterCondition } from '../../../types';

describe('[tester] tableData/filterUtils', () => {
  it('isCompleteFilter rejects incomplete filters', () => {
    expect(isCompleteFilter({ column: '', operator: 'eq', value: 'x' })).toBe(false);
    expect(isCompleteFilter({ column: 'id', operator: 'eq', value: null })).toBe(false);
    expect(isCompleteFilter({ column: 'id', operator: 'eq', value: undefined })).toBe(false);
    expect(isCompleteFilter({ column: 'id', operator: 'eq', value: '' })).toBe(false);
    expect(isCompleteFilter({ column: 'id', operator: 'in', value: [] })).toBe(false);
  });

  it('isCompleteFilter accepts null-check and complete value filters', () => {
    expect(isCompleteFilter({ column: 'id', operator: 'isNull', value: null })).toBe(true);
    expect(isCompleteFilter({ column: 'id', operator: 'isNotNull', value: null })).toBe(true);
    expect(isCompleteFilter({ column: 'id', operator: 'eq', value: 1 })).toBe(true);
    expect(isCompleteFilter({ column: 'id', operator: 'in', value: ['a'] })).toBe(true);
  });

  it('cloneFilters returns shallow copies', () => {
    const filters: FilterCondition[] = [{ column: 'id', operator: 'eq', value: 1 }];
    const cloned = cloneFilters(filters);
    expect(cloned).toEqual(filters);
    expect(cloned).not.toBe(filters);
    expect(cloned[0]).not.toBe(filters[0]);
  });

  it('filterDraftEqualsApplied compares logic and serialized filters', () => {
    const draft: FilterCondition[] = [{ column: 'id', operator: 'eq', value: 1 }];
    expect(filterDraftEqualsApplied(draft, 'and', draft, 'and')).toBe(true);
    expect(filterDraftEqualsApplied(draft, 'or', draft, 'and')).toBe(false);
    expect(
      filterDraftEqualsApplied(
        [{ column: 'id', operator: 'eq', value: 1 }],
        'and',
        [{ column: 'id', operator: 'eq', value: 2 }],
        'and',
      ),
    ).toBe(false);
  });
});
