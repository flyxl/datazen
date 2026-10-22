import { describe, expect, it } from 'vitest';
import { classifyDataType } from '../dataTypeColors';
import { cellTypeToDataTypeFamily, resolveCellType } from '../cellTypes';

/**
 * The colour classification and the editor classification must be the same
 * judgement seen twice (05 §3.4). If they drift, the grid paints a cell one
 * way and edits it another way.
 */
const TYPES: readonly string[] = [
  'integer',
  'int4',
  'bigint',
  'smallint',
  'tinyint(1)',
  'numeric(10,2)',
  'decimal(10,2)',
  'double precision',
  'real',
  'text',
  'varchar(255)',
  'citext',
  'character varying',
  'char(10)',
  'boolean',
  'bool',
  'date',
  'time',
  'time(6)',
  'timestamp',
  'timestamp with time zone',
  'timestamp without time zone',
  'datetime2(7)',
  'json',
  'jsonb',
  'ARRAY',
  'bytea',
  'blob',
  'binary(16)',
  'uuid',
  'uniqueidentifier',
  "enum('a','b')",
  'interval',
  'point',
  'inet',
  'money',
  '',
];

describe('classifyDataType is the projection of resolveCellType', () => {
  it('holds for every representative type', () => {
    for (const type of TYPES) {
      expect(classifyDataType(type), type).toBe(
        cellTypeToDataTypeFamily(resolveCellType(type)),
      );
    }
  });

  it('keeps the answers the existing test pinned', () => {
    // The regression evidence: the projection did not change what the grid
    // paints for the types the old substring test handled.
    expect(classifyDataType('integer')).toBe('number');
    expect(classifyDataType('varchar(255)')).toBe('text');
    expect(classifyDataType('timestamp with time zone')).toBe('datetime');
    expect(classifyDataType('boolean')).toBe('bool');
    expect(classifyDataType('jsonb')).toBe('json');
    expect(classifyDataType('bytea')).toBe('binary');
    expect(classifyDataType('')).toBe('text');
    expect(classifyDataType('numeric')).toBe('number');
  });

  it('fixes the geometry type the old substring test got wrong', () => {
    // `point`.includes('int') is true — the old answer was "number".
    expect(classifyDataType('point')).toBe('text');
  });
});
