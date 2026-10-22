import { describe, expect, it } from 'vitest';
import { cellValueTextClass, cellValueTextClassFor, classifyDataType } from '../dataTypeColors';
import { cellTypeToDataTypeFamily, resolveCellType, type CellTypeHints } from '../cellTypes';

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

  it('reads the driver declaration, so the colour and the editor agree', () => {
    // MySQL's `tinyint(1)` is a boolean: the editor shows a checkbox, so the
    // cell must not be painted as a number (05 §3.4).
    const mysql: CellTypeHints = {
      overrides: [{ match: 'exact', pattern: 'tinyint(1)', cellType: 'bool' }],
    };
    expect(classifyDataType('tinyint(1)')).toBe('number');
    expect(classifyDataType('tinyint(1)', mysql)).toBe('bool');
    // Without the declaration the behaviour is exactly what it was.
    expect(classifyDataType('varchar(255)')).toBe('text');
    expect(classifyDataType('varchar(255)', mysql)).toBe('text');
  });

  it('paints a cell from the normalised type it was given', () => {
    // The grid hands the colour the normalised type, so the cell and its
    // editor cannot give two answers.
    expect(cellValueTextClassFor('bool', 1)).toBe('text-dt-bool');
    expect(cellValueTextClassFor('number', 1)).toBe('text-dt-number');
    expect(cellValueTextClassFor('text', null)).toBe('text-dt-null');
    expect(cellValueTextClass('tinyint(1)', 1)).toBe('text-dt-number');
  });
});
