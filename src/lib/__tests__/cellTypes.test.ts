import { describe, expect, it } from 'vitest';
import {
  DEFAULT_CELL_TYPE_RULES,
  cellTypeToDataTypeFamily,
  isIntegerCellType,
  isReadOnlyCellType,
  isZonedCellType,
  matchCellTypeRule,
  resolveCellType,
  tokenizeTypeName,
  type CellType,
} from '../cellTypes';

/**
 * The type table is the host's single type judgement, so its cases are the
 * ones the four drivers actually produce.
 */
const DRIVER_TYPES: readonly { type: string; expected: CellType }[] = [
  // PostgreSQL — the column metadata and the query result disagree on the
  // spelling, and both have to land on the same type.
  { type: 'int4', expected: 'number' },
  { type: 'text', expected: 'text' },
  { type: 'timestamptz', expected: 'datetime' },
  { type: 'timestamp with time zone', expected: 'datetime' },
  { type: 'timestamp without time zone', expected: 'datetime' },
  { type: 'bytea', expected: 'bytes' },
  { type: 'jsonb', expected: 'json' },
  { type: 'uuid', expected: 'uuid' },
  { type: 'USER-DEFINED', expected: 'unknown' },
  { type: 'ARRAY', expected: 'json' },
  { type: 'text[]', expected: 'json' },
  { type: 'numeric', expected: 'number' },
  { type: 'boolean', expected: 'bool' },
  { type: 'interval', expected: 'text' },
  { type: 'point', expected: 'text' },
  // MySQL — `COLUMN_TYPE` with the length and the unsigned flag.
  { type: 'int(11) unsigned', expected: 'number' },
  { type: 'tinyint(1)', expected: 'number' },
  { type: 'mediumblob', expected: 'bytes' },
  { type: "enum('a','b')", expected: 'enum' },
  { type: "set('a','b')", expected: 'enum' },
  { type: 'varchar(255)', expected: 'text' },
  { type: 'longtext', expected: 'text' },
  { type: 'double precision', expected: 'number' },
  { type: 'datetime(6)', expected: 'datetime' },
  // SQL Server — the type string is built by a CASE expression.
  { type: 'nvarchar(max)', expected: 'text' },
  { type: 'uniqueidentifier', expected: 'uuid' },
  { type: 'datetime2(7)', expected: 'datetime' },
  { type: 'datetimeoffset', expected: 'datetime' },
  { type: 'decimal(10,2)', expected: 'number' },
  { type: 'varbinary(max)', expected: 'bytes' },
  { type: 'bit', expected: 'text' },
  // ClickHouse — the type name carries its parameters.
  { type: 'Nullable(Int32)', expected: 'number' },
  { type: 'LowCardinality(String)', expected: 'text' },
  { type: 'DateTime64(3)', expected: 'datetime' },
  { type: 'Date32', expected: 'date' },
  { type: 'FixedString(16)', expected: 'bytes' },
  { type: "Enum8('a'=1)", expected: 'enum' },
  // DuckDB / SQLite.
  { type: 'INTEGER', expected: 'number' },
  { type: 'TEXT', expected: 'text' },
  { type: 'REAL', expected: 'number' },
  { type: 'NUMERIC', expected: 'number' },
  { type: 'BLOB', expected: 'bytes' },
];

describe('resolveCellType', () => {
  it('normalizes representative type names from every driver', () => {
    for (const { type, expected } of DRIVER_TYPES) {
      expect(resolveCellType(type), type).toBe(expected);
    }
  });

  it('does not classify geometry or network types as numbers', () => {
    // The regression this whole table exists for: `point`.includes('int')
    // is true, so the old substring test answered "number" for a geometry
    // column.
    for (const type of ['point', 'polygon', 'linestring', 'inet', 'cidr', 'macaddr']) {
      expect(resolveCellType(type), type).toBe('text');
    }
  });

  it('prefers datetime over date for combined names', () => {
    expect(resolveCellType('datetime')).toBe('datetime');
    expect(resolveCellType('datetime2(7)')).toBe('datetime');
    expect(resolveCellType('timestamp')).toBe('datetime');
    expect(resolveCellType('date')).toBe('date');
  });

  it('returns unknown for unclassified and empty input', () => {
    expect(resolveCellType('')).toBe('unknown');
    expect(resolveCellType('   ')).toBe('unknown');
    expect(resolveCellType(undefined)).toBe('unknown');
    expect(resolveCellType(null)).toBe('unknown');
    expect(resolveCellType('a_type_nobody_declared')).toBe('unknown');
  });

  it('never throws, whatever it is given', () => {
    expect(() => resolveCellType('%%%')).not.toThrow();
    expect(() => resolveCellType('int(1)')).not.toThrow();
  });

  it('lets the driver overrides win over the shared table', () => {
    // SQL Server's `timestamp` is a row version, not a time.
    expect(
      resolveCellType('timestamp', {
        overrides: [{ match: 'exact', pattern: 'timestamp', cellType: 'bytes' }],
      }),
    ).toBe('bytes');
    // MySQL's `tinyint(1)` is a boolean.
    expect(
      resolveCellType('tinyint(1)', {
        overrides: [{ match: 'exact', pattern: 'tinyint(1)', cellType: 'bool' }],
      }),
    ).toBe('bool');
    // Without the declaration the shared table decides — and it decides
    // "number", which is the honest answer for a type nobody declared.
    expect(resolveCellType('tinyint(1)')).toBe('number');
  });

  it('lets the driver declare which of its types are enumerations', () => {
    // PostgreSQL reports every enum column as `USER-DEFINED`.
    expect(resolveCellType('USER-DEFINED', { enumTypes: ['user-defined'] })).toBe('enum');
    expect(resolveCellType('USER-DEFINED')).toBe('unknown');
  });

  it('keeps the rule order load-bearing: the text block beats the numeric rules', () => {
    const textBlock = DEFAULT_CELL_TYPE_RULES.findIndex(
      (rule) => rule.pattern === 'point' && rule.cellType === 'text',
    );
    const intRule = DEFAULT_CELL_TYPE_RULES.findIndex(
      (rule) => rule.pattern === 'int' && rule.cellType === 'number',
    );
    expect(textBlock).toBeGreaterThanOrEqual(0);
    expect(intRule).toBeGreaterThanOrEqual(0);
    expect(textBlock).toBeLessThan(intRule);
  });
});

describe('cellTypeToDataTypeFamily', () => {
  it('projects every cell type to exactly one render family', () => {
    const families: Record<CellType, string> = {
      text: 'text',
      number: 'number',
      bool: 'bool',
      date: 'datetime',
      time: 'datetime',
      datetime: 'datetime',
      json: 'json',
      bytes: 'binary',
      enum: 'text',
      uuid: 'text',
      unknown: 'text',
    };
    const all: CellType[] = [
      'text',
      'number',
      'bool',
      'date',
      'time',
      'datetime',
      'json',
      'bytes',
      'enum',
      'uuid',
      'unknown',
    ];
    for (const cellType of all) {
      expect(cellTypeToDataTypeFamily(cellType), cellType).toBe(families[cellType]);
    }
  });

  it('never answers the null family — null is a property of the value', () => {
    const all: CellType[] = [
      'text',
      'number',
      'bool',
      'date',
      'time',
      'datetime',
      'json',
      'bytes',
      'enum',
      'uuid',
      'unknown',
    ];
    for (const cellType of all) {
      expect(cellTypeToDataTypeFamily(cellType)).not.toBe('null');
    }
  });
});

describe('matchCellTypeRule', () => {
  it('matches whole tokens, not substrings', () => {
    expect(matchCellTypeRule('int4', { match: 'tokenPrefix', pattern: 'int', cellType: 'number' })).toBe(
      true,
    );
    expect(matchCellTypeRule('point', { match: 'tokenPrefix', pattern: 'int', cellType: 'number' })).toBe(
      false,
    );
    expect(matchCellTypeRule('text[]', { match: 'suffix', pattern: '[]', cellType: 'json' })).toBe(true);
    expect(matchCellTypeRule('ARRAY', { match: 'token', pattern: 'array', cellType: 'json' })).toBe(true);
  });
});

describe('tokenizeTypeName', () => {
  it('splits a type name into its tokens', () => {
    expect(tokenizeTypeName('int(11) unsigned')).toEqual(['int', '11', 'unsigned']);
    expect(tokenizeTypeName('Nullable(Int32)')).toEqual(['nullable', 'int32']);
  });
});

describe('isZonedCellType', () => {
  it('knows the standard zoned names', () => {
    expect(isZonedCellType('timestamp with time zone')).toBe(true);
    expect(isZonedCellType('timestamptz')).toBe(true);
    expect(isZonedCellType('datetimeoffset')).toBe(true);
    expect(isZonedCellType('timestamp without time zone')).toBe(false);
    expect(isZonedCellType('date')).toBe(false);
  });

  it('takes the driver declaration for the rest', () => {
    expect(isZonedCellType('my_timestamptz', { zonedTypes: ['my_timestamptz'] })).toBe(true);
  });
});

describe('isReadOnlyCellType', () => {
  it('answers only what the driver declared', () => {
    expect(isReadOnlyCellType('rowversion')).toBe(false);
    expect(isReadOnlyCellType('rowversion', { readOnlyTypes: ['rowversion'] })).toBe(true);
    expect(isReadOnlyCellType('timestamp', { readOnlyTypes: ['timestamp'] })).toBe(true);
  });
});

describe('isIntegerCellType', () => {
  it('recognises the integer names and leaves the decimal ones alone', () => {
    expect(isIntegerCellType('int4')).toBe(true);
    expect(isIntegerCellType('bigint')).toBe(true);
    expect(isIntegerCellType('tinyint(1)')).toBe(true);
    expect(isIntegerCellType('serial')).toBe(true);
    expect(isIntegerCellType('numeric(10,2)')).toBe(false);
    expect(isIntegerCellType('double precision')).toBe(false);
    expect(isIntegerCellType('text')).toBe(false);
  });

  it('follows the driver override: a boolean alias is not an integer', () => {
    expect(
      isIntegerCellType('tinyint(1)', {
        overrides: [{ match: 'exact', pattern: 'tinyint(1)', cellType: 'bool' }],
      }),
    ).toBe(false);
  });
});
