/**
 * The operator matrix of 08 §3.5, cell by cell.
 *
 * The dispatch key of the filter operators is the normalised `CellType` (11
 * values), and `enum` / `uuid` have their own, narrower operator sets. The
 * matrix is what the G5 debt of 08/09 bought back, and this file is that
 * table written down: a mutation that widened `enum` back to the `text` set
 * left the whole suite green, because nothing in the repository guarded it.
 */
import { describe, expect, it } from 'vitest';
import {
  FILTER_OPERATOR_META,
  operatorsForColumn,
  supportedOperatorsForMeta,
} from '../filterOperators';
import { resolveCellType, type CellType } from '../cellTypes';
import type { FilterOperator } from '../../types';

/** 08 §3.5 — "按 CellType 的默认算子集", verbatim. */
const MATRIX: Record<CellType, readonly FilterOperator[]> = {
  text: [
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
  ],
  enum: ['eq', 'ne', 'in', 'notIn', 'isNull', 'isNotNull'],
  number: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'in', 'notIn', 'isNull', 'isNotNull'],
  bool: ['eq', 'ne', 'isNull', 'isNotNull'],
  date: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'isNull', 'isNotNull'],
  time: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'isNull', 'isNotNull'],
  datetime: ['eq', 'ne', 'gt', 'lt', 'gte', 'lte', 'between', 'notBetween', 'isNull', 'isNotNull'],
  json: ['eq', 'ne', 'isNull', 'isNotNull'],
  bytes: ['eq', 'ne', 'isNull', 'isNotNull'],
  uuid: ['eq', 'ne', 'in', 'notIn', 'isNull', 'isNotNull'],
  unknown: [
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
  ],
};

/** One type name that must land on each cell type. */
const SAMPLE: Record<CellType, string> = {
  text: 'varchar(255)',
  enum: "enum('a','b')",
  number: 'integer',
  bool: 'boolean',
  date: 'date',
  time: 'time',
  datetime: 'timestamp',
  json: 'jsonb',
  bytes: 'bytea',
  uuid: 'uuid',
  unknown: 'a_type_nobody_declared',
};

/** The ten operators that already shipped; their relative order is frozen. */
const SHIPPED_ORDER = [
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

describe('the operator matrix of 08 §3.5', () => {
  it('offers each cell type exactly the operators of the matrix', () => {
    const supported = supportedOperatorsForMeta(undefined);
    for (const cellType of Object.keys(MATRIX) as CellType[]) {
      const operators = operatorsForColumn(SAMPLE[cellType], supported);
      expect(operators, cellType).toEqual([...MATRIX[cellType]]);
    }
  });

  it('gives enum and uuid their own, narrower sets than text', () => {
    const text = new Set(MATRIX.text);
    for (const cellType of ['enum', 'uuid'] as const) {
      const own = MATRIX[cellType];
      expect(own.length, cellType).toBeLessThan(text.size);
      for (const operator of own) expect(text.has(operator), `${cellType}/${operator}`).toBe(true);
      // The pattern family and the empty checks are exactly what an
      // enumeration / uuid column must not be offered.
      for (const operator of [
        'like',
        'contains',
        'notContains',
        'startsWith',
        'endsWith',
        'isEmpty',
        'isNotEmpty',
      ]) {
        expect(own, `${cellType}/${operator}`).not.toContain(operator);
      }
    }
  });

  it('keeps the relative order of the shipped operators inside every cell type', () => {
    for (const cellType of Object.keys(MATRIX) as CellType[]) {
      const shipped = MATRIX[cellType].filter((operator) =>
        (SHIPPED_ORDER as readonly string[]).includes(operator),
      );
      const expected = SHIPPED_ORDER.filter((operator) => shipped.includes(operator));
      expect(shipped, cellType).toEqual([...expected]);
    }
  });

  it('derives FILTER_OPERATOR_META.cellTypes from the matrix (one source)', () => {
    for (const [id, meta] of Object.entries(FILTER_OPERATOR_META)) {
      const derived = (Object.keys(MATRIX) as CellType[]).filter((cellType) =>
        MATRIX[cellType].includes(id as FilterOperator),
      );
      // The dialect operators live outside the matrix (the driver whitelist
      // appends them), so they are attributed to their own cell types.
      if (meta.group === 'dialect') {
        const expectedDialect = id === 'jsonContains' ? ['json'] : ['text', 'unknown'];
        expect([...meta.cellTypes].sort(), id).toEqual(expectedDialect);
        continue;
      }
      expect([...meta.cellTypes].sort(), id).toEqual([...derived].sort());
    }
  });

  it('lets the driver override the operator set of a column', () => {
    // SQL Server's `timestamp` is a row version, not a time: the driver says
    // so, and the operator list follows the declaration (08 §3.5).
    const supported = supportedOperatorsForMeta(undefined);
    const sqlserver = {
      overrides: [{ match: 'exact' as const, pattern: 'timestamp', cellType: 'bytes' as const }],
    };
    expect(operatorsForColumn('timestamp', supported, sqlserver)).toEqual([...MATRIX.bytes]);
    expect(operatorsForColumn('timestamp', supported)).toEqual([...MATRIX.datetime]);
  });

  it('reaches every cell type through resolveCellType', () => {
    for (const cellType of Object.keys(MATRIX) as CellType[]) {
      expect(resolveCellType(SAMPLE[cellType]), cellType).toBe(cellType);
    }
  });
});
