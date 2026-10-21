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

// ── 08 Step 10: the operators that arrived with the extension (08 §9.2).
describe('isCompleteFilter for the extended operators', () => {
  it('treats the operators without a value as complete', () => {
    expect(isCompleteFilter({ column: 'name', operator: 'isNull' })).toBe(true);
    expect(isCompleteFilter({ column: 'name', operator: 'isNotNull' })).toBe(true);
    expect(isCompleteFilter({ column: 'name', operator: 'isEmpty' })).toBe(true);
    expect(isCompleteFilter({ column: 'name', operator: 'isNotEmpty' })).toBe(true);
  });

  it('needs a value for the operators that compare against one', () => {
    expect(isCompleteFilter({ column: 'name', operator: 'contains' })).toBe(false);
    expect(isCompleteFilter({ column: 'name', operator: 'contains', value: '' })).toBe(false);
    expect(isCompleteFilter({ column: 'name', operator: 'contains', value: 'a' })).toBe(true);
    expect(isCompleteFilter({ column: 'name', operator: 'regex', value: '^a' })).toBe(true);
  });

  it('needs at least one value for the set operators', () => {
    expect(isCompleteFilter({ column: 'name', operator: 'in', value: [] })).toBe(false);
    expect(isCompleteFilter({ column: 'name', operator: 'notIn', value: [] })).toBe(false);
    expect(isCompleteFilter({ column: 'name', operator: 'in', value: ['a'] })).toBe(true);
    expect(isCompleteFilter({ column: 'name', operator: 'notIn', value: 'a' })).toBe(true);
    expect(isCompleteFilter({ column: 'name', operator: 'in', value: ' , ' })).toBe(false);
  });

  it('needs exactly two bounds for the range operators', () => {
    expect(isCompleteFilter({ column: 'score', operator: 'between', value: ['1'] })).toBe(false);
    expect(isCompleteFilter({ column: 'score', operator: 'between', value: ['1', ''] })).toBe(false);
    expect(isCompleteFilter({ column: 'score', operator: 'between', value: ['1', '2'] })).toBe(true);
    expect(isCompleteFilter({ column: 'score', operator: 'notBetween', value: ['1', '2'] })).toBe(
      true,
    );
    expect(isCompleteFilter({ column: 'score', operator: 'between', value: '1,2' })).toBe(true);
    expect(isCompleteFilter({ column: 'score', operator: 'between', value: '1' })).toBe(false);
  });
});
