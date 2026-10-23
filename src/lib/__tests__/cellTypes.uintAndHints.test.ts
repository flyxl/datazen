/**
 * The round-2 fixes of DB-05: the ClickHouse unsigned integers (②), the
 * driver's declaration reaching the filter operators (⑤) and the colour
 * taking the same declaration (⑥).
 */
import { describe, expect, it } from 'vitest';
import { resolveCellType, isIntegerCellType, type CellTypeHints } from '../cellTypes';
import { classifyDataType, dataTypeTextClass, cellValueTextClass, cellValueTextClassFor } from '../dataTypeColors';
import { operatorsForColumn } from '../filterOperators';
import { coerceValue, resolveCellEditorKind } from '../cellEditorKind';
import { SHARED_FILTER_OPERATORS } from '../../types';

const UINT_TYPES = [
  'UInt8',
  'UInt16',
  'UInt32',
  'UInt64',
  'UInt128',
  'UInt256',
  'Nullable(UInt32)',
] as const;

const MYSQL_HINTS: CellTypeHints = {
  overrides: [{ match: 'exact', pattern: 'tinyint(1)', cellType: 'bool' }],
};

const SQLSERVER_HINTS: CellTypeHints = {
  overrides: [
    { match: 'exact', pattern: 'timestamp', cellType: 'bytes' },
    { match: 'exact', pattern: 'rowversion', cellType: 'bytes' },
  ],
  zonedTypes: ['datetimeoffset'],
  readOnlyTypes: ['timestamp', 'rowversion'],
};

describe('the unsigned integers are numbers again', () => {
  it('classifies every UInt* as a number', () => {
    for (const type of UINT_TYPES) {
      expect(resolveCellType(type), type).toBe('number');
      expect(classifyDataType(type), type).toBe('number');
    }
  });

  it('gives them the numeric editor', () => {
    for (const type of UINT_TYPES) {
      expect(
        resolveCellEditorKind({
          cellType: resolveCellType(type),
          zoned: false,
          hasCandidates: false,
          valueCodePoints: 0,
        }),
        type,
      ).toBe('number');
    }
  });

  it('still treats them as integer columns (a fraction is refused)', () => {
    for (const type of UINT_TYPES) {
      expect(isIntegerCellType(type), type).toBe(true);
      const refused = coerceValue({
        kind: 'number',
        cellType: 'number',
        text: '1.5',
        nullable: true,
        originalValue: 1,
        integerOnly: isIntegerCellType(type),
      });
      expect(refused, type).toEqual({ ok: false, reason: 'invalidInteger' });
      const accepted = coerceValue({
        kind: 'number',
        cellType: 'number',
        text: '42',
        nullable: true,
        originalValue: 1,
        integerOnly: isIntegerCellType(type),
      });
      expect(accepted, type).toEqual({ ok: true, value: 42 });
    }
  });

  it('offers them the operators of an integer column', () => {
    const reference = operatorsForColumn('int4', SHARED_FILTER_OPERATORS);
    for (const type of UINT_TYPES) {
      const operators = operatorsForColumn(type, SHARED_FILTER_OPERATORS);
      expect(operators, type).toEqual(reference);
      expect(operators, type).toContain('between');
      expect(operators, type).toContain('notBetween');
      expect(operators, type).not.toContain('like');
      expect(operators, type).not.toContain('isEmpty');
    }
  });

  it('leaves the signed integers alone', () => {
    for (const type of ['int4', 'bigint', 'int8', 'int16', 'int32', 'int64']) {
      expect(resolveCellType(type), type).toBe('number');
      expect(isIntegerCellType(type), type).toBe(true);
    }
  });

  it('recognises the signed 128/256-bit integers as integer columns', () => {
    for (const type of ['Int128', 'Int256', 'Nullable(Int128)']) {
      expect(resolveCellType(type), type).toBe('number');
      expect(isIntegerCellType(type), type).toBe(true);
    }
  });
});

describe('the driver declaration reaches the filter operators', () => {
  it('gives the SQL Server row version the bytes set', () => {
    const operators = operatorsForColumn('timestamp', SHARED_FILTER_OPERATORS, SQLSERVER_HINTS);
    expect(operators).not.toContain('between');
    expect(operators).toEqual(['eq', 'ne', 'isNull', 'isNotNull']);
  });

  it('changes the answer (without the declaration it is a time column)', () => {
    const withoutHints = operatorsForColumn('timestamp', SHARED_FILTER_OPERATORS);
    expect(withoutHints).toContain('between');
    expect(withoutHints).not.toEqual(
      operatorsForColumn('timestamp', SHARED_FILTER_OPERATORS, SQLSERVER_HINTS),
    );
  });
});

describe('the colour takes the driver declaration too', () => {
  it('keeps the old answer when no declaration is passed', () => {
    expect(classifyDataType('tinyint(1)')).toBe('number');
    expect(dataTypeTextClass('tinyint(1)')).toBe('text-dt-number');
    expect(cellValueTextClass('tinyint(1)', 1)).toBe('text-dt-number');
    // The empty type name and the nullish value keep their old answers.
    expect(classifyDataType('')).toBe('text');
    expect(cellValueTextClass('text', null)).toBe('text-dt-null');
    expect(cellValueTextClass('text', undefined)).toBe('text-dt-null');
  });

  it('paints the MySQL boolean as a boolean when the declaration is passed', () => {
    expect(classifyDataType('tinyint(1)', MYSQL_HINTS)).toBe('bool');
    expect(dataTypeTextClass('tinyint(1)', MYSQL_HINTS)).toBe('text-dt-bool');
    expect(cellValueTextClass('tinyint(1)', 1, MYSQL_HINTS)).toBe('text-dt-bool');
    // The normalised type gives the same answer without a type name.
    expect(cellValueTextClassFor('bool', 1)).toBe('text-dt-bool');
    expect(cellValueTextClassFor('bool', null)).toBe('text-dt-null');
  });
});
