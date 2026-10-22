import { describe, expect, it } from 'vitest';
import {
  GRID_PASTE_MAX_CELLS,
  buildRowsMatrix,
  buildSelectionMatrix,
  parseClipboardMatrix,
  serializeCells,
  sniffClipboardDelimiter,
  toClipboardRange,
  utf8ByteLength,
  type ClipboardMatrix,
} from '../gridClipboard';
import { pendingCellKey, planPaste, tableChangeKey } from '../gridPastePlan';
import type { ColumnSchema } from '../../types';

/**
 * DB-03 §9.1 — parser, serializer, planner.
 *
 * Every malformed-input case the chapter lists lives here: a paste is fed by other
 * applications, so "what a hostile/odd clipboard actually contains" is the normal input, not
 * the exception.
 */

const column = (name: string, extra: Partial<ColumnSchema> = {}): ColumnSchema => ({
  name,
  dataType: 'text',
  nullable: true,
  ...extra,
});

function matrixOf(cells: string[][], quoted?: boolean[][]): ClipboardMatrix {
  return {
    cells,
    quoted: quoted ?? cells.map((row) => row.map(() => false)),
    delimiter: '\t',
    multiline: cells.length > 1,
    byteLength: cells.flat().join('').length,
  };
}

describe('parseClipboardMatrix', () => {
  it('parses empty text as grid.paste.empty', () => {
    for (const text of ['', '   ', '\n']) {
      const result = parseClipboardMatrix(text);
      expect(result.ok).toBe(false);
      if (!result.ok) expect(result.code).toBe('grid.paste.empty');
    }
  });

  it('parses a single value without delimiters', () => {
    const result = parseClipboardMatrix('abc');
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.matrix.delimiter).toBe('\t');
    expect(result.matrix.cells).toEqual([['abc']]);
  });

  it('parses a single row with tabs', () => {
    const result = parseClipboardMatrix('a\tb\tc');
    expect(result.ok && result.matrix.cells).toEqual([['a', 'b', 'c']]);
  });

  it('parses CRLF rows', () => {
    const result = parseClipboardMatrix('a\tb\r\nc\td');
    expect(result.ok && result.matrix.cells).toEqual([
      ['a', 'b'],
      ['c', 'd'],
    ]);
  });

  it('parses bare CR rows', () => {
    const result = parseClipboardMatrix('a\tb\rc\td');
    expect(result.ok && result.matrix.cells).toEqual([
      ['a', 'b'],
      ['c', 'd'],
    ]);
  });

  it('drops a single trailing newline', () => {
    const result = parseClipboardMatrix('a\tb\n');
    expect(result.ok && result.matrix.cells).toEqual([['a', 'b']]);
  });

  it('keeps interior blank lines as blank rows', () => {
    const result = parseClipboardMatrix('a\n\nb');
    expect(result.ok && result.matrix.cells.length).toBe(3);
    expect(result.ok && result.matrix.cells[1]).toEqual(['']);
  });

  it('sniffs comma when no tab present', () => {
    const result = parseClipboardMatrix('a,b\nc,d');
    expect(result.ok && result.matrix.delimiter).toBe(',');
    expect(result.ok && result.matrix.cells).toEqual([
      ['a', 'b'],
      ['c', 'd'],
    ]);
  });

  it('sniffs semicolon', () => {
    expect(sniffClipboardDelimiter('a;b')).toBe(';');
  });

  it('prefers tab over comma', () => {
    const result = parseClipboardMatrix('a,b\tc');
    expect(result.ok && result.matrix.delimiter).toBe('\t');
    expect(result.ok && result.matrix.cells).toEqual([['a,b', 'c']]);
  });

  it('keeps a quoted tab inside one cell', () => {
    const result = parseClipboardMatrix('"a\tb"\tc');
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.matrix.cells).toEqual([['a\tb', 'c']]);
    expect(result.matrix.quoted).toEqual([[true, false]]);
  });

  it('keeps a quoted newline inside one cell', () => {
    const result = parseClipboardMatrix('"a\nb"\tc');
    expect(result.ok && result.matrix.cells).toEqual([['a\nb', 'c']]);
  });

  it('unescapes doubled quotes', () => {
    const result = parseClipboardMatrix('"a""b"');
    expect(result.ok && result.matrix.cells).toEqual([['a"b']]);
  });

  it('keeps a quoted comma inside one cell', () => {
    const result = parseClipboardMatrix('a,"b,c"');
    expect(result.ok && result.matrix.cells).toEqual([['a', 'b,c']]);
  });

  it('marks quoted empty as explicit empty string', () => {
    const result = parseClipboardMatrix('""\tNULL');
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.matrix.quoted[0][0]).toBe(true);
    expect(result.matrix.cells[0][0]).toBe('');
    // Unquoted NULL is *not* quoted, which is the difference the planner reads.
    expect(result.matrix.quoted[0][1]).toBe(false);
  });

  it('flags an unclosed quote instead of throwing', () => {
    const result = parseClipboardMatrix('"abc');
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.matrix.cells).toEqual([['abc']]);
    expect(result.problems.map((problem) => problem.code)).toEqual(['grid.paste.unclosedQuote']);
  });

  it('rejects ragged rows', () => {
    const result = parseClipboardMatrix('a\tb\nc');
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.code).toBe('grid.paste.raggedRow');
    expect(result.row).toBe(1);
    expect(result.expected).toBe(2);
    expect(result.actual).toBe(1);
  });

  it('preserves surrogate pairs', () => {
    const result = parseClipboardMatrix('😀\t中文\té');
    expect(result.ok && result.matrix.cells).toEqual([['😀', '中文', 'é']]);
  });

  it('rejects text over the byte limit', () => {
    const text = 'a'.repeat(11 * 1024 * 1024);
    const result = parseClipboardMatrix(text);
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.code).toBe('grid.paste.tooLarge');
  });

  it('rejects more than the cell limit', () => {
    const width = GRID_PASTE_MAX_CELLS / 2 + 1;
    const rows = Array.from({ length: 2 }, () => Array.from({ length: width }, () => 'a').join('\t'));
    const result = parseClipboardMatrix(rows.join('\n'));
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.code).toBe('grid.paste.tooLarge');
  });

  it('counts UTF-8 bytes, not code units', () => {
    expect(utf8ByteLength('abc')).toBe(3);
    expect(utf8ByteLength('é')).toBe(2);
    expect(utf8ByteLength('😀')).toBe(4);
    expect(utf8ByteLength('中文')).toBe(6);
  });
});

describe('serializeCells', () => {
  it('quotes a value containing a tab', () => {
    const text = serializeCells([['a\tb', 'c']], { format: 'tsv', columnNames: ['x', 'y'] });
    expect(text).toBe('"a\tb"\tc');
    const back = parseClipboardMatrix(text);
    expect(back.ok && back.matrix.cells).toEqual([['a\tb', 'c']]);
  });

  it('quotes a value containing a newline', () => {
    const text = serializeCells([['a\nb']], { format: 'tsv', columnNames: ['x'] });
    expect(text).toBe('"a\nb"');
    const back = parseClipboardMatrix(text);
    expect(back.ok && back.matrix.cells).toEqual([['a\nb']]);
  });

  it('doubles embedded quotes', () => {
    const text = serializeCells([['he said "hi"']], { format: 'tsv', columnNames: ['x'] });
    expect(text).toBe('"he said ""hi"""');
  });

  it('renders null as empty by default', () => {
    expect(serializeCells([[null]], { format: 'tsv', columnNames: ['x'] })).toBe('');
  });

  it('renders null as the configured literal', () => {
    expect(
      serializeCells([[null], ['']], { format: 'tsv', columnNames: ['x'], nullLiteral: 'NULL' }),
    ).toBe('NULL\n');
  });

  it('serializes objects as JSON not [object Object]', () => {
    // The point of the case is JSON-vs-`String(obj)`; the surrounding quotes come from the
    // escaping rule above (a value with a `"` in it is quoted and its quotes are doubled),
    // so the text a spreadsheet reads back is still exactly `{"a":1}`.
    expect(serializeCells([[{ a: 1 }]], { format: 'tsv', columnNames: ['x'] })).toBe(
      '"{""a"":1}"',
    );
  });

  it('omits the header by default for tsv', () => {
    expect(serializeCells([['1']], { format: 'tsv', columnNames: ['id'] })).toBe('1');
  });

  it('includes the header when asked', () => {
    expect(
      serializeCells([['1']], { format: 'tsv', columnNames: ['id'], includeHeader: true }),
    ).toBe('id\n1');
  });

  it('csv keeps its header default', () => {
    expect(serializeCells([['1']], { format: 'csv', columnNames: ['id'] })).toBe('id\n1');
  });

  it('csv quotes a comma inside a value', () => {
    expect(serializeCells([['a,b']], { format: 'csv', columnNames: ['x'] })).toBe('x\n"a,b"');
  });

  it('json 2d array by default', () => {
    expect(serializeCells([['a', 'b']], { format: 'json', columnNames: ['x', 'y'] })).toBe(
      '[["a","b"]]',
    );
  });

  it('json objects when asked', () => {
    expect(
      serializeCells([['a', 'b']], {
        format: 'json',
        columnNames: ['x', 'y'],
        jsonObjects: true,
      }),
    ).toBe('[{"x":"a","y":"b"}]');
  });

  it('round-trips a whole matrix', () => {
    const matrix = [
      ['plain', 'with\ttab', 'with\nnewline'],
      ['with "quote"', '😀 emoji', ''],
    ];
    const text = serializeCells(matrix, { format: 'tsv', columnNames: ['a', 'b', 'c'] });
    const back = parseClipboardMatrix(text);
    expect(back.ok && back.matrix.cells).toEqual(matrix);
  });

  it('keeps values longer than 120 characters intact', () => {
    const long = 'x'.repeat(500);
    const longJson = JSON.stringify({ payload: 'y'.repeat(500) });
    const text = serializeCells([[long, longJson]], { format: 'tsv', columnNames: ['a', 'b'] });
    expect(text).not.toContain('…');
    expect(text).toContain(long);
    expect(text).toContain('y'.repeat(500));
  });
});

describe('toClipboardRange', () => {
  it('maps two normalized corners onto render positions', () => {
    const range = toClipboardRange(
      { start: { rowIndex: 1, columnName: 'b' }, end: { rowIndex: 3, columnName: 'a' } },
      ['a', 'b', 'c'],
    );
    expect(range).toEqual({ topRow: 1, bottomRow: 3, leftColumn: 'a', rightColumn: 'b' });
  });

  it('returns null when a corner is not displayed', () => {
    expect(
      toClipboardRange(
        { start: { rowIndex: 0, columnName: 'gone' }, end: { rowIndex: 0, columnName: 'a' } },
        ['a'],
      ),
    ).toBeNull();
  });
});

describe('buildSelectionMatrix / buildRowsMatrix', () => {
  it('maps columns by name not by index', () => {
    // The row records are keyed by name; the render order is deliberately different.
    const rows = [
      { id: 1, name: 'Alice', email: 'a@x' },
      { id: 2, name: 'Bob', email: 'b@x' },
    ];
    const matrix = buildSelectionMatrix(rows, [
      { rowIndex: 0, columnName: 'email' },
      { rowIndex: 0, columnName: 'name' },
      { rowIndex: 1, columnName: 'email' },
      { rowIndex: 1, columnName: 'name' },
    ]);
    expect(matrix).toEqual([
      ['a@x', 'Alice'],
      ['b@x', 'Bob'],
    ]);
  });

  it('fills a missing value with null', () => {
    expect(buildSelectionMatrix([{ a: 1 }], [{ rowIndex: 0, columnName: 'b' }])).toEqual([[null]]);
  });

  it('builds a row matrix in ascending row order over the given columns', () => {
    const rows = [{ a: 'A2', b: 'B2' }, { a: 'A1', b: 'B1' }];
    // `rowIndices` is sorted ascending: row 0 is `{ a: 'A2', b: 'B2' }`.
    expect(buildRowsMatrix(rows, ['b', 'a'], [1, 0])).toEqual([
      ['B2', 'A2'],
      ['B1', 'A1'],
    ]);
  });
});

describe('planPaste', () => {
  const baseOptions = {
    columnOrder: ['id', 'name', 'score'],
    columns: [
      column('id', { isPrimaryKey: true }),
      column('name'),
      column('score', { nullable: true }),
    ],
    region: null,
    pageRowCount: 3,
    pendingCellKeys: new Set<string>(),
  };

  it('writes null for an unquoted empty field', () => {
    const plan = planPaste({
      ...baseOptions,
      matrix: matrixOf([['', 'x']], [[false, false]]),
      anchor: { rowIndex: 0, columnName: 'name' },
    });
    expect(plan.writes.map((write) => write.value)).toEqual([null, 'x']);
  });

  it('writes an empty string for a quoted empty field', () => {
    const plan = planPaste({
      ...baseOptions,
      matrix: matrixOf([['']], [[true]]),
      anchor: { rowIndex: 0, columnName: 'name' },
    });
    expect(plan.writes).toEqual([{ rowIndex: 0, columnName: 'name', value: '' }]);
  });

  it('recognizes the NULL literal and \\N as NULL', () => {
    const plan = planPaste({
      ...baseOptions,
      matrix: matrixOf([['NULL', '\\N', 'null']]),
      anchor: { rowIndex: 0, columnName: 'name' },
    });
    expect(plan.writes.every((write) => write.value === null)).toBe(true);
  });

  it('reports notNullEmpty on a NOT NULL column', () => {
    const plan = planPaste({
      ...baseOptions,
      columns: [column('id', { isPrimaryKey: true }), column('name', { nullable: false })],
      columnOrder: ['id', 'name'],
      matrix: matrixOf([['']]),
      anchor: { rowIndex: 0, columnName: 'name' },
    });
    expect(plan.writes).toEqual([]);
    expect(plan.problems.map((problem) => problem.code)).toContain('grid.paste.notNullEmpty');
  });

  it('skips auto-increment columns entirely', () => {
    const plan = planPaste({
      ...baseOptions,
      columns: [
        column('id', { isPrimaryKey: true, isAutoIncrement: true }),
        column('name'),
      ],
      columnOrder: ['id', 'name'],
      matrix: matrixOf([['99', 'Alice']]),
      anchor: { rowIndex: 0, columnName: 'id' },
    });
    expect(plan.writes).toEqual([{ rowIndex: 0, columnName: 'name', value: 'Alice' }]);
    expect(plan.skippedColumns).toEqual(['id']);
    expect(plan.problems.map((problem) => problem.code)).toContain('grid.cell.readOnly');
  });

  it('does not write uncovered cells', () => {
    const plan = planPaste({
      ...baseOptions,
      region: {
        topRow: 0,
        bottomRow: 2,
        leftColumn: 'name',
        rightColumn: 'score',
      },
      matrix: matrixOf([['Alice']]),
      anchor: { rowIndex: 0, columnName: 'name' },
    });
    expect(plan.writes).toEqual([{ rowIndex: 0, columnName: 'name', value: 'Alice' }]);
    expect(plan.bounds).toEqual({
      topRow: 0,
      bottomRow: 0,
      leftColumn: 'name',
      rightColumn: 'name',
    });
  });

  it('expands beyond the region up to the page end', () => {
    const plan = planPaste({
      ...baseOptions,
      region: { topRow: 0, bottomRow: 0, leftColumn: 'name', rightColumn: 'name' },
      matrix: matrixOf([['A'], ['B'], ['C'], ['D']]),
      anchor: { rowIndex: 0, columnName: 'name' },
      pageRowCount: 3,
    });
    // Rows 0..2 land, row 3 is counted as skipped and reported.
    expect(plan.writes.map((write) => write.rowIndex)).toEqual([0, 1, 2]);
    expect(plan.skipped).toBe(1);
    expect(plan.problems.map((problem) => problem.code)).toContain('grid.paste.outOfBounds');
  });

  it('clips to the region in clip mode', () => {
    const plan = planPaste({
      ...baseOptions,
      overflow: 'clip',
      region: { topRow: 0, bottomRow: 1, leftColumn: 'name', rightColumn: 'name' },
      matrix: matrixOf([['A'], ['B'], ['C']]),
      anchor: { rowIndex: 0, columnName: 'name' },
    });
    expect(plan.writes.map((write) => write.rowIndex)).toEqual([0, 1]);
    expect(plan.bounds?.bottomRow).toBe(1);
  });

  it('counts overwritten pending changes', () => {
    const plan = planPaste({
      ...baseOptions,
      matrix: matrixOf([['A'], ['B']]),
      anchor: { rowIndex: 0, columnName: 'name' },
      pendingCellKeys: new Set([pendingCellKey(0, 'name')]),
    });
    expect(plan.writes.length).toBe(2);
    expect(plan.overwrittenPending).toBe(1);
  });

  it('rejects the whole batch when over the cell limit', () => {
    const width = 10;
    const height = Math.ceil(GRID_PASTE_MAX_CELLS / width) + 1;
    const matrix = matrixOf(Array.from({ length: height }, () => Array.from({ length: width }, () => 'x')));
    const plan = planPaste({ ...baseOptions, matrix, anchor: { rowIndex: 0, columnName: 'name' } });
    expect(plan.writes).toEqual([]);
    expect(plan.problems.map((problem) => problem.code)).toContain('grid.paste.tooLarge');
  });

  it('skips an unknown column and reports it', () => {
    const plan = planPaste({
      ...baseOptions,
      // A record column that the schema no longer knows about.
      columns: [column('id', { isPrimaryKey: true })],
      columnOrder: ['id', 'ghost'],
      matrix: matrixOf([['x']]),
      anchor: { rowIndex: 0, columnName: 'ghost' },
    });
    expect(plan.writes).toEqual([]);
    expect(
      plan.problems.some(
        (problem) => problem.code === 'grid.cell.readOnly' && problem.detail === 'unknown-column',
      ),
    ).toBe(true);
  });
});

describe('tableChangeKey', () => {
  const context = {
    connectionId: 'Conn-1',
    dbSessionId: 'sess-a',
    driverType: 'postgres',
    database: 'App',
    schema: null,
    table: 'Users',
  };

  it('ignores the runtime session id and folds case', () => {
    expect(tableChangeKey(context)).toBe(
      tableChangeKey({ ...context, dbSessionId: 'sess-b', connectionId: 'conn-1', database: 'app', table: 'users' }),
    );
  });

  it('treats a missing schema and an empty schema as the same table', () => {
    expect(tableChangeKey({ ...context, schema: null })).toBe(
      tableChangeKey({ ...context, schema: '' }),
    );
  });

  it('separates different tables and databases', () => {
    expect(tableChangeKey(context)).not.toBe(tableChangeKey({ ...context, table: 'Orders' }));
    expect(tableChangeKey(context)).not.toBe(tableChangeKey({ ...context, database: 'Other' }));
  });

  it('returns an empty key for an unknown context', () => {
    expect(tableChangeKey(null)).toBe('');
  });
});
