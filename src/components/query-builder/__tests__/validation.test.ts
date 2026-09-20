/**
 * Query builder diagnostics — the abnormal and edge cases.
 *
 * Every case here came from auditing the generator with invalid input rather
 * than from reading the implementation. Before this suite, each of these states
 * produced plausible-looking SQL that was either a syntax error or, worse,
 * silently returned the wrong rows (`col = NULL` matches nothing,
 * `IN ()` does not parse, an unpaginated SQL Server query returns everything).
 */
import { describe, expect, it } from 'vitest';
import { isExactNumericLiteral, validateQuery } from '../validation';
import { generateSql } from '../hooks/useSqlGenerator';
import type { GenerateSqlInput } from '../hooks/useSqlGenerator';
import type { QbCondition, QbConditionGroup } from '../types';

const emptyGroup = (): QbConditionGroup => ({
  id: 'root',
  logic: 'AND',
  conditions: [],
  groups: [],
});

const condition = (over: Partial<QbCondition> = {}): QbCondition => ({
  id: 'c1',
  table: 't',
  column: 'c',
  operator: '=',
  value: 'x',
  conjunction: 'AND',
  ...over,
});

function input(over: Partial<GenerateSqlInput> = {}): GenerateSqlInput {
  return {
    selectedTables: ['t'],
    selectedColumns: [{ table: 't', column: 'c' }],
    joins: [],
    tableAliases: {},
    where: emptyGroup(),
    orderBy: [],
    groupBy: [],
    distinct: false,
    limit: null,
    offset: null,
    databaseType: 'postgresql',
    ...over,
  };
}

const codes = (i: GenerateSqlInput) => validateQuery(i).map((d) => d.code);

describe('isExactNumericLiteral', () => {
  it('accepts literals that survive a round-trip', () => {
    expect(isExactNumericLiteral('2006')).toBe(true);
    expect(isExactNumericLiteral('-5')).toBe(true);
    expect(isExactNumericLiteral('0')).toBe(true);
  });

  it('rejects values that only look numeric', () => {
    // `007` is invalid in PostgreSQL; `1.50` would silently become `1.5`.
    expect(isExactNumericLiteral('007')).toBe(false);
    expect(isExactNumericLiteral('1.50')).toBe(false);
    expect(isExactNumericLiteral('1e5')).toBe(false);
    expect(isExactNumericLiteral('+5')).toBe(false);
  });
});

describe('validateQuery — temporal literals', () => {
  const temporalInput = (value: string, columnType: string) =>
    input({
      columnTypeMap: { t: { c: columnType } },
      where: { ...emptyGroup(), conditions: [condition({ value, column: 'c' })] },
    });

  it('flags the `timestamp > integer` bug class as an error', () => {
    expect(codes(temporalInput('10', 'timestamp without time zone'))).toContain(
      'invalid-temporal-literal',
    );
  });

  it('accepts well-formed date and datetime literals', () => {
    expect(codes(temporalInput('2026-09-20', 'date'))).not.toContain('invalid-temporal-literal');
    expect(codes(temporalInput('2026-09-20 01:13:45', 'timestamp'))).not.toContain(
      'invalid-temporal-literal',
    );
  });

  it('does not fire for non-temporal columns or IS NULL operators', () => {
    expect(codes(temporalInput('10', 'integer'))).toEqual([]);
    expect(
      codes(
        input({
          columnTypeMap: { t: { c: 'date' } },
          where: {
            ...emptyGroup(),
            conditions: [condition({ operator: 'IS NULL', value: '', column: 'c' })],
          },
        }),
      ),
    ).not.toContain('invalid-temporal-literal');
  });
});

describe('validateQuery — empty / missing state', () => {
  it('reports a missing table and stops there', () => {
    expect(codes(input({ selectedTables: [], selectedColumns: [] }))).toEqual(['no-tables']);
  });

  it('reports a table with no columns', () => {
    expect(codes(input({ selectedColumns: [] }))).toEqual(['no-columns']);
  });

  it('accepts a complete minimal query', () => {
    expect(validateQuery(input())).toEqual([]);
  });
});

describe('validateQuery — condition edge cases', () => {
  it('flags an empty value for a comparison', () => {
    expect(
      codes(input({ where: { ...emptyGroup(), conditions: [condition({ value: '' })] } })),
    ).toEqual(['empty-condition-value']);
  });

  it('flags an empty value for LIKE', () => {
    expect(
      codes(
        input({
          where: { ...emptyGroup(), conditions: [condition({ operator: 'LIKE', value: '' })] },
        }),
      ),
    ).toEqual(['empty-condition-value']);
  });

  it('does not require a value for IS NULL / IS NOT NULL', () => {
    for (const operator of ['IS NULL', 'IS NOT NULL'] as const) {
      expect(
        codes(
          input({ where: { ...emptyGroup(), conditions: [condition({ operator, value: null })] } }),
        ),
      ).toEqual([]);
    }
  });

  it('flags an empty IN list but accepts a populated one', () => {
    expect(
      codes(
        input({
          where: { ...emptyGroup(), conditions: [condition({ operator: 'IN', value: '' })] },
        }),
      ),
    ).toEqual(['empty-in-list']);

    expect(
      codes(
        input({
          where: { ...emptyGroup(), conditions: [condition({ operator: 'IN', value: 'a, b' })] },
        }),
      ),
    ).toEqual([]);
  });

  it('treats a list of only separators as empty', () => {
    expect(
      codes(
        input({
          where: { ...emptyGroup(), conditions: [condition({ operator: 'IN', value: ' , , ' })] },
        }),
      ),
    ).toEqual(['empty-in-list']);
  });

  it('finds problems inside nested groups', () => {
    const nested: QbConditionGroup = {
      id: 'root',
      logic: 'AND',
      conditions: [],
      groups: [
        {
          id: 'g1',
          logic: 'OR',
          conditions: [condition({ id: 'n1', value: '' })],
          groups: [
            {
              id: 'g2',
              logic: 'AND',
              conditions: [condition({ id: 'n2', operator: 'IN', value: '' })],
              groups: [],
            },
          ],
        },
      ],
    };
    // Depth-first: a group's own conditions are reported before its sub-groups,
    // matching the order the SQL is assembled in. Compare as a set so the test
    // does not pin an incidental ordering.
    expect(codes(input({ where: nested })).sort()).toEqual(
      ['empty-in-list', 'empty-condition-value'].sort(),
    );
  });

  it('also validates per-column WHERE clauses', () => {
    expect(
      codes(
        input({ selectedColumns: [{ table: 't', column: 'c', where: condition({ value: '' }) }] }),
      ),
    ).toEqual(['empty-condition-value']);
  });
});

describe('validateQuery — alias edge cases', () => {
  it('flags two tables sharing one alias', () => {
    const result = validateQuery(
      input({
        selectedTables: ['a', 'b'],
        selectedColumns: [
          { table: 'a', column: 'x' },
          { table: 'b', column: 'y' },
        ],
        tableAliases: { a: 'z', b: 'z' },
      }),
    );
    expect(result.map((d) => d.code)).toEqual(['alias-duplicate']);
    expect(result[0]!.detail).toContain('a');
    expect(result[0]!.detail).toContain('b');
  });

  it('flags an alias that shadows another selected table', () => {
    expect(
      codes(
        input({
          selectedTables: ['a', 'b'],
          selectedColumns: [
            { table: 'a', column: 'x' },
            { table: 'b', column: 'y' },
          ],
          tableAliases: { a: 'b' },
        }),
      ),
    ).toEqual(['alias-shadows-table']);
  });

  it('allows an alias equal to its own table name', () => {
    expect(codes(input({ tableAliases: { t: 't' } }))).toEqual([]);
  });

  it('allows distinct aliases', () => {
    expect(
      codes(
        input({
          selectedTables: ['a', 'b'],
          selectedColumns: [
            { table: 'a', column: 'x' },
            { table: 'b', column: 'y' },
          ],
          tableAliases: { a: 'x1', b: 'x2' },
        }),
      ),
    ).toEqual([]);
  });
});

describe('validateQuery — join edge cases', () => {
  it('flags a join to a table that is not in the query', () => {
    expect(
      codes(
        input({
          joins: [
            {
              id: 'j',
              type: 'INNER',
              leftTable: 't',
              leftColumn: 'id',
              rightTable: 'gone',
              rightColumn: 't_id',
              isManual: true,
            },
          ],
        }),
      ),
    ).toEqual(['unknown-join-table']);
  });

  it('flags a self join instead of silently dropping it', () => {
    expect(
      codes(
        input({
          joins: [
            {
              id: 'j',
              type: 'INNER',
              leftTable: 't',
              leftColumn: 'parent_id',
              rightTable: 't',
              rightColumn: 'id',
              isManual: true,
            },
          ],
        }),
      ),
    ).toEqual(['self-join']);
  });

  it('accepts a well-formed join', () => {
    expect(
      codes(
        input({
          selectedTables: ['t', 'u'],
          selectedColumns: [
            { table: 't', column: 'c' },
            { table: 'u', column: 'd' },
          ],
          joins: [
            {
              id: 'j',
              type: 'INNER',
              leftTable: 'u',
              leftColumn: 't_id',
              rightTable: 't',
              rightColumn: 'id',
              isManual: true,
            },
          ],
        }),
      ),
    ).toEqual([]);
  });
});

describe('validateQuery — pagination edge cases', () => {
  it('flags a negative limit', () => {
    expect(codes(input({ limit: -5 }))).toEqual(['invalid-limit']);
  });

  it('flags a fractional limit', () => {
    expect(codes(input({ limit: 1.5 }))).toEqual(['invalid-limit']);
  });

  it('flags a negative offset', () => {
    expect(codes(input({ offset: -1 }))).toEqual(['invalid-limit']);
  });

  it('accepts zero and positive bounds', () => {
    expect(codes(input({ limit: 0, offset: 0 }))).toEqual([]);
    expect(codes(input({ limit: 10, offset: 20 }))).toEqual([]);
  });

  it('accepts an offset without a limit on dialects that support it', () => {
    for (const databaseType of ['postgresql', 'mysql', 'sqlite']) {
      expect(codes(input({ offset: 20, databaseType }))).toEqual([]);
    }
  });

  it('flags pagination on a dialect that cannot express it, instead of dropping it', () => {
    // SQL Server silently produced an unpaginated query before this check.
    expect(codes(input({ limit: 10, databaseType: 'sqlserver' }))).toEqual([
      'unsupported-pagination',
    ]);
  });

  it('does not complain about SQL Server when no pagination is requested', () => {
    expect(codes(input({ databaseType: 'sqlserver' }))).toEqual([]);
  });
});

describe('generator — values that must not be reinterpreted', () => {
  const value = (v: string) =>
    generateSql(input({ where: { ...emptyGroup(), conditions: [condition({ value: v })] } }));

  it('quotes numeric-looking strings', () => {
    expect(value('007')).toContain("= '007'");
    expect(value('1.50')).toContain("= '1.50'");
  });

  it('leaves true numeric literals unquoted', () => {
    expect(value('2006')).toContain('= 2006');
    expect(value('-5')).toContain('= -5');
  });

  it('escapes embedded single quotes', () => {
    expect(value("O'Brien")).toContain("= 'O''Brien'");
  });

  it('escapes embedded double quotes in identifiers', () => {
    const sql = generateSql(input({ tableAliases: { t: 'we"ird' } }));
    expect(sql).toContain('AS "we""ird"');
    expect(sql).toContain('"we""ird"."c"');
  });

  it('escapes backticks and brackets for their dialects', () => {
    expect(generateSql(input({ databaseType: 'mysql', tableAliases: { t: 'we`ird' } }))).toContain(
      '`we``ird`',
    );
    expect(
      generateSql(input({ databaseType: 'sqlserver', tableAliases: { t: 'we]ird' } })),
    ).toContain('[we]]ird]');
  });
});

describe('generator — pagination', () => {
  it('emits OFFSET alone when only an offset is set (PostgreSQL)', () => {
    const sql = generateSql(input({ offset: 20, databaseType: 'postgresql' }));
    expect(sql).toContain('OFFSET 20');
    // Previously `LIMIT 0 OFFSET 20`, which silently returns zero rows.
    expect(sql).not.toContain('LIMIT 0');
  });

  it('uses the unbounded-limit sentinel for MySQL', () => {
    expect(generateSql(input({ offset: 20, databaseType: 'mysql' }))).toContain(
      'LIMIT 18446744073709551615 OFFSET 20',
    );
  });

  it('uses LIMIT -1 for SQLite', () => {
    expect(generateSql(input({ offset: 20, databaseType: 'sqlite' }))).toContain(
      'LIMIT -1 OFFSET 20',
    );
  });

  it('emits LIMIT and OFFSET together', () => {
    expect(generateSql(input({ limit: 10, offset: 5 }))).toContain('LIMIT 10 OFFSET 5');
  });

  it('emits nothing when neither bound is set', () => {
    const sql = generateSql(input());
    expect(sql.toUpperCase()).not.toContain('LIMIT');
    expect(sql.toUpperCase()).not.toContain('OFFSET');
  });
});

describe('generator — structure edge cases', () => {
  it('returns nothing when there is no table or no column', () => {
    expect(generateSql(input({ selectedTables: [], selectedColumns: [] }))).toBe('');
    expect(generateSql(input({ selectedColumns: [] }))).toBe('');
  });

  it('drops a sub-group that has no conditions', () => {
    const sql = generateSql(
      input({
        where: {
          ...emptyGroup(),
          conditions: [condition()],
          groups: [{ id: 'g', logic: 'OR', conditions: [], groups: [] }],
        },
      }),
    );
    expect(sql).toContain('WHERE');
    expect(sql).not.toContain('OR');
  });

  it('still emits the raw broken IN list, which is why validation blocks it', () => {
    const broken = generateSql(
      input({ where: { ...emptyGroup(), conditions: [condition({ operator: 'IN', value: '' })] } }),
    );
    expect(broken).toContain('IN ()');
    // The generator stays tolerant; `validateQuery` is what stops this reaching
    // the editor (see the IN-list case above).
    expect(
      codes(
        input({
          where: { ...emptyGroup(), conditions: [condition({ operator: 'IN', value: '' })] },
        }),
      ),
    ).toEqual(['empty-in-list']);
  });
});
