import { describe, expect, it } from 'vitest';
import {
  FILTER_OPERATOR_META,
  convertFilterValue,
  operatorsForDriver,
  defaultOperatorForColumn,
  likeCaseInsensitiveForMeta,
  normalizeOperatorForColumn,
  operatorsForColumn,
  supportedOperatorsForMeta,
  valueInputFor,
} from '../filterOperators';
import type { DatabaseTypeMeta } from '../databaseMeta';
import type { FilterOperator } from '../../types';
import { EXISTING_FILTER_OPERATORS, SHARED_FILTER_OPERATORS } from '../../types';

/** The order the operators that already shipped must keep, everywhere. */
const EXISTING_ORDER = [
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
] as const;

/** One representative data type per column type family. */
const FAMILY_SAMPLES: readonly { dataType: string; family: string }[] = [
  { dataType: 'text', family: 'text' },
  { dataType: 'varchar(64)', family: 'text' },
  { dataType: 'int4', family: 'number' },
  { dataType: 'numeric(10,2)', family: 'number' },
  { dataType: 'timestamp without time zone', family: 'datetime' },
  { dataType: 'bool', family: 'bool' },
  { dataType: 'jsonb', family: 'json' },
  { dataType: 'bytea', family: 'binary' },
];

function metaWith(dataGrid: DatabaseTypeMeta['dataGrid']): DatabaseTypeMeta {
  return { dataGrid } as unknown as DatabaseTypeMeta;
}

describe('filter operator metadata', () => {
  it('keeps the relative order of the shipped operators for every column type', () => {
    for (const { dataType } of FAMILY_SAMPLES) {
      const shipped = operatorsForColumn(dataType, SHARED_FILTER_OPERATORS).filter((operator) =>
        EXISTING_FILTER_OPERATORS.includes(operator),
      );
      const expected = EXISTING_ORDER.filter((operator) => shipped.includes(operator));
      expect(shipped, dataType).toEqual([...expected]);
    }
  });

  it('offers the text operators for a text column and no pattern operators elsewhere', () => {
    expect(operatorsForColumn('text', SHARED_FILTER_OPERATORS)).toEqual([
      'eq',
      'ne',
      'gt',
      'lt',
      'gte',
      'lte',
      'like',
      'contains',
      'notContains',
      'startsWith',
      'endsWith',
      'in',
      'notIn',
      'isNull',
      'isNotNull',
      'isEmpty',
      'isNotEmpty',
    ]);
    expect(operatorsForColumn('int4', SHARED_FILTER_OPERATORS)).toEqual([
      'eq',
      'ne',
      'gt',
      'lt',
      'gte',
      'lte',
      'between',
      'notBetween',
      'in',
      'notIn',
      'isNull',
      'isNotNull',
    ]);
    expect(operatorsForColumn('bool', SHARED_FILTER_OPERATORS)).toEqual([
      'eq',
      'ne',
      'isNull',
      'isNotNull',
    ]);
    expect(operatorsForColumn('jsonb', SHARED_FILTER_OPERATORS)).toEqual([
      'eq',
      'ne',
      'isNull',
      'isNotNull',
    ]);
    expect(operatorsForColumn('timestamp', SHARED_FILTER_OPERATORS)).toEqual([
      'eq',
      'ne',
      'gt',
      'lt',
      'gte',
      'lte',
      'between',
      'notBetween',
      'isNull',
      'isNotNull',
    ]);
  });

  it('defaults the supported operators to the shared nineteen', () => {
    expect(supportedOperatorsForMeta(undefined)).toEqual([...SHARED_FILTER_OPERATORS]);
    expect(supportedOperatorsForMeta({} as DatabaseTypeMeta)).toEqual([...SHARED_FILTER_OPERATORS]);
    expect(supportedOperatorsForMeta(metaWith({}))).toEqual([...SHARED_FILTER_OPERATORS]);
    expect(SHARED_FILTER_OPERATORS).toHaveLength(19);
  });

  it('treats an empty declaration as "said nothing", not "serves nothing"', () => {
    // The rule that the shipped path and this test have to agree on.
    expect(operatorsForDriver(undefined)).toEqual([...SHARED_FILTER_OPERATORS]);
    expect(operatorsForDriver([])).toEqual([...SHARED_FILTER_OPERATORS]);
    expect(supportedOperatorsForMeta(metaWith({ filterOperators: [] }))).toEqual([
      ...SHARED_FILTER_OPERATORS,
    ]);
    expect(supportedOperatorsForMeta(metaWith({ filterOperators: ['eq'] }))).toEqual(['eq']);
  });

  it('narrows the supported operators to what the driver declares', () => {
    const supported = supportedOperatorsForMeta(
      metaWith({ filterOperators: ['eq', 'ne', 'isNull', 'isNotNull'] }),
    );
    expect(supported).toEqual(['eq', 'ne', 'isNull', 'isNotNull']);
    expect(operatorsForColumn('text', supported)).toEqual(['eq', 'ne', 'isNull', 'isNotNull']);
  });

  it('shows a dialect operator only when the driver whitelists it', () => {
    expect(operatorsForColumn('text', SHARED_FILTER_OPERATORS)).not.toContain('regex');
    const withRegex = supportedOperatorsForMeta(
      metaWith({ filterOperators: [...SHARED_FILTER_OPERATORS, 'regex'] }),
    );
    const operators = operatorsForColumn('text', withRegex);
    expect(operators).toContain('regex');
    // Appended after the shared ones, so the shipped order stays untouched.
    expect(operators.indexOf('regex')).toBeGreaterThan(operators.indexOf('isNotNull'));
  });

  it('shows the json dialect operator on a json column only', () => {
    const withJson = supportedOperatorsForMeta(
      metaWith({ filterOperators: [...SHARED_FILTER_OPERATORS, 'jsonContains'] }),
    );
    expect(operatorsForColumn('jsonb', withJson)).toContain('jsonContains');
    expect(operatorsForColumn('text', withJson)).not.toContain('jsonContains');
  });

  it('defaults the like case-insensitivity flag to false', () => {
    expect(likeCaseInsensitiveForMeta(undefined)).toBe(false);
    expect(likeCaseInsensitiveForMeta(metaWith({}))).toBe(false);
    expect(likeCaseInsensitiveForMeta(metaWith({ likeCaseInsensitive: true }))).toBe(true);
  });

  it('starts a new condition with the equality operator for every column type', () => {
    for (const dataType of [undefined, 'int4', 'text', 'bool', 'jsonb', 'bytea']) {
      expect(defaultOperatorForColumn(dataType)).toBe('eq');
    }
  });

  it('keeps the operator when the column still supports it and falls back to equality', () => {
    expect(normalizeOperatorForColumn('contains', 'text', SHARED_FILTER_OPERATORS)).toBe('contains');
    expect(normalizeOperatorForColumn('contains', 'int4', SHARED_FILTER_OPERATORS)).toBe('eq');
    expect(normalizeOperatorForColumn('between', 'text', SHARED_FILTER_OPERATORS)).toBe('eq');
    expect(normalizeOperatorForColumn('regex', 'text', SHARED_FILTER_OPERATORS)).toBe('eq');
  });

  it('maps every operator to its value control', () => {
    const none = Object.values(FILTER_OPERATOR_META)
      .filter((entry) => entry.valueInput === 'none')
      .map((entry) => entry.id);
    expect(none.sort()).toEqual(['isEmpty', 'isNotEmpty', 'isNotNull', 'isNull']);
    expect(valueInputFor('between')).toBe('pair');
    expect(valueInputFor('notBetween')).toBe('pair');
    expect(valueInputFor('in')).toBe('tokens');
    expect(valueInputFor('notIn')).toBe('tokens');
    expect(valueInputFor('eq')).toBe('single');
    expect(valueInputFor('regex')).toBe('single');
  });

  it('degrades an unknown operator instead of throwing while the panel renders', () => {
    // A condition persisted by another build may name an operator this build
    // does not know. The panel has to render it.
    const unknown = 'legacyOperator' as unknown as FilterOperator;
    expect(() => valueInputFor(unknown)).not.toThrow();
    expect(valueInputFor(unknown)).toBe('single');
  });

  it('records that only the literal substring operators escape the value', () => {
    // `like` is a pattern: the user writes the wildcards.
    expect(FILTER_OPERATOR_META.like.escapesLikePattern).toBe(false);
    for (const operator of ['contains', 'notContains', 'startsWith', 'endsWith'] as const) {
      expect(FILTER_OPERATOR_META[operator].escapesLikePattern).toBe(true);
    }
  });
});

describe('convertFilterValue', () => {
  it('splits a single value into tokens when the control becomes a token list', () => {
    expect(convertFilterValue('single', 'tokens', 'a, b ,c')).toEqual(['a', 'b', 'c']);
    expect(convertFilterValue('single', 'tokens', '')).toEqual([]);
  });

  it('joins tokens back into a single value', () => {
    expect(convertFilterValue('tokens', 'single', ['a', 'b'])).toBe('a, b');
    expect(convertFilterValue('tokens', 'single', [])).toBe('');
  });

  it('seeds the lower bound of a pair and leaves the upper one empty', () => {
    expect(convertFilterValue('single', 'pair', '50%')).toEqual(['50%', '']);
  });

  it('keeps the lower bound when a pair collapses into a single value', () => {
    expect(convertFilterValue('pair', 'single', ['10', '20'])).toBe('10');
  });

  it('moves the first two tokens into the bounds of a pair', () => {
    expect(convertFilterValue('tokens', 'pair', ['10', '20', '30'])).toEqual(['10', '20']);
    expect(convertFilterValue('tokens', 'pair', ['10'])).toEqual(['10', '']);
  });

  it('drops the empty bound when a pair becomes a token list', () => {
    expect(convertFilterValue('pair', 'tokens', ['10', ''])).toEqual(['10']);
    expect(convertFilterValue('pair', 'tokens', ['', '20'])).toEqual(['20']);
  });

  it('keeps the value when the operator carries no value at all', () => {
    expect(convertFilterValue('single', 'none', 'alice')).toBe('alice');
    expect(convertFilterValue('none', 'single', 'alice')).toBe('alice');
  });

  it('returns the value unchanged when the control does not change', () => {
    expect(convertFilterValue('single', 'single', 'alice')).toBe('alice');
  });
});
