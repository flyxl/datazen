/**
 * Independent re-verification of the filter type split (Wave 5 / track B).
 *
 * Written by the verification agent. The split moved the filter contract into
 * `src/types/filters.ts` and re-exported it from the aggregate
 * `src/types/index.ts`. Two things have to stay true:
 *
 * 1. **the consumers do not move**: every caller still imports the filter
 *    types from `../types`, so the re-export has to be a re-export and not a
 *    move;
 * 2. **the value type does not change**: `FilterCondition.value` is typed by
 *    the `Value` of `src/types/connection.ts`, while the aggregate still
 *    exports its own `Value` from `src/types/index.ts`. If the two ever drift
 *    apart, a `FilterCondition` would stop being assignable to the shapes the
 *    rest of the app uses — and TypeScript would not say so.
 */
import { describe, expect, it } from 'vitest';
import type { FilterCondition as FilterConditionFromAggregate } from '../index';
import type { FilterCondition as FilterConditionFromFile } from '../filters';
import type { Value as ValueFromAggregate } from '../index';
import type { Value as ValueFromConnection } from '../connection';
import {
  DIALECT_FILTER_OPERATORS,
  EXISTING_FILTER_OPERATORS,
  SHARED_FILTER_OPERATORS,
} from '../index';

/** Compile-time proof that the two `Value` aliases are the same type. */
type AssertSame<T, U> = [T] extends [U] ? ([U] extends [T] ? true : never) : never;
const valueTypesAreTheSame: AssertSame<
  ValueFromAggregate,
  ValueFromConnection
> = true;

/** Compile-time proof that the aggregate still carries the filter types. */
const conditionTypesAreTheSame: AssertSame<
  FilterConditionFromAggregate,
  FilterConditionFromFile
> = true;

describe('the filter type split', () => {
  it('keeps the aggregate `Value` and the connection `Value` the same type', () => {
    expect(valueTypesAreTheSame).toBe(true);
  });

  it('keeps `FilterCondition` the same type through the aggregate', () => {
    expect(conditionTypesAreTheSame).toBe(true);
  });

  it('a condition written against the aggregate is the one the file declares', () => {
    const condition: FilterConditionFromAggregate = {
      column: 'id',
      operator: 'eq',
      value: 1,
    };
    const asFileType: FilterConditionFromFile = condition;
    expect(asFileType.column).toBe('id');
  });

  it('a condition value stays assignable to the aggregate `Value`', () => {
    const condition: FilterConditionFromAggregate = {
      column: 'id',
      operator: 'eq',
      value: { nested: [1, 2] },
    };
    const value: ValueFromAggregate | undefined = condition.value;
    expect(value).toEqual({ nested: [1, 2] });
  });

  it('the connector is optional and only `and` / `or`', () => {
    const withoutConnector: FilterConditionFromAggregate = {
      column: 'id',
      operator: 'eq',
      value: 1,
    };
    const withConnector: FilterConditionFromAggregate = {
      column: 'id',
      operator: 'eq',
      value: 1,
      connector: 'or',
    };
    expect(withoutConnector.connector).toBeUndefined();
    expect(withConnector.connector).toBe('or');
  });

  it('the operator lists are the ones the Rust enum declares', () => {
    expect(EXISTING_FILTER_OPERATORS).toEqual([
      'eq',
      'ne',
      'gt',
      'lt',
      'gte',
      'lte',
      'like',
      'in',
      'isNull',
      'isNotNull',
    ]);
    expect(SHARED_FILTER_OPERATORS.length).toBe(19);
    expect(DIALECT_FILTER_OPERATORS).toEqual(['regex', 'notRegex', 'jsonContains']);
    // The two groups are disjoint and together they cover every operator.
    expect(SHARED_FILTER_OPERATORS.length + DIALECT_FILTER_OPERATORS.length).toBe(22);
    for (const operator of DIALECT_FILTER_OPERATORS) {
      expect(SHARED_FILTER_OPERATORS).not.toContain(operator);
    }
  });
});
