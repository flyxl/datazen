import { describe, expect, it } from 'vitest';
import * as XLSX from 'xlsx';
import {
  ImportParseError,
  decodeImportText,
  detectCsvDelimiter,
  parseCsvImport,
  parseJsonImport,
  parseXlsxImport,
} from '../importData';

/**
 * Golden fixtures for DBR-01 reliable parsing (PRD 4.3): fields with embedded
 * newlines / commas / escaped quotes, CRLF+CR, trailing empty lines, blank
 * lines inside quoted fields, missing and extra columns, duplicate column
 * names, empty-string vs NULL policy, unterminated quotes and non-UTF-8
 * decoding. Every fixture is byte-literal so regressions are visible.
 */

describe('parseCsvImport: quoting', () => {
  it('keeps a quoted newline inside the field instead of splitting the record', () => {
    const text = 'id,note\n1,"line one\nline two"\n2,plain';
    const parsed = parseCsvImport(text);
    expect(parsed.rows).toEqual([
      { id: '1', note: 'line one\nline two' },
      { id: '2', note: 'plain' },
    ]);
    // The second record starts on physical line 4: line 2's quoted newline
    // consumed line 3.
    expect(parsed.rowOrigins).toEqual([2, 4]);
  });

  it('keeps commas inside quoted fields', () => {
    const parsed = parseCsvImport('a,b\n"x,y",z');
    expect(parsed.rows[0]).toEqual({ a: 'x,y', b: 'z' });
  });

  it('unescapes doubled quotes', () => {
    const parsed = parseCsvImport('name\n"say ""hi"""');
    expect(parsed.rows[0]).toEqual({ name: 'say "hi"' });
  });

  it('preserves a blank line inside a quoted field but drops bare blank lines', () => {
    const text = 'a,b\n1,"first\n\nthird"\n\n4,tail';
    const parsed = parseCsvImport(text);
    expect(parsed.rows).toEqual([
      { a: '1', b: 'first\n\nthird' },
      { a: '4', b: 'tail' },
    ]);
    expect(parsed.rowOrigins).toEqual([2, 6]);
  });

  it('supports a configurable quote character', () => {
    const parsed = parseCsvImport("a;b\n'x;y';z", { delimiter: ';', quoteChar: "'" });
    expect(parsed.rows[0]).toEqual({ a: 'x;y', b: 'z' });
  });
});

describe('parseCsvImport: row endings', () => {
  it('accepts CRLF row endings without stray carriage returns', () => {
    const parsed = parseCsvImport('a,b\r\n1,2\r\n3,4\r\n');
    expect(parsed.rows).toEqual([
      { a: '1', b: '2' },
      { a: '3', b: '4' },
    ]);
  });

  it('accepts bare CR row endings', () => {
    const parsed = parseCsvImport('a,b\r1,2\r3,4');
    expect(parsed.rows).toEqual([
      { a: '1', b: '2' },
      { a: '3', b: '4' },
    ]);
  });

  it('accepts CRLF inside quoted fields', () => {
    const parsed = parseCsvImport('a\n"two\r\nlines"');
    expect(parsed.rows[0]).toEqual({ a: 'two\nlines' });
  });

  it('ignores trailing empty lines after the last record', () => {
    const parsed = parseCsvImport('a\n1\n\n\n');
    expect(parsed.rows).toEqual([{ a: '1' }]);
    expect(parsed.rowOrigins).toEqual([2]);
  });

  it('a file that is only blank lines parses to nothing', () => {
    const parsed = parseCsvImport('\n\n  \n\n');
    expect(parsed.columns).toEqual([]);
    expect(parsed.rows).toEqual([]);
  });
});

describe('parseCsvImport: unterminated quotes are refused, never guessed', () => {
  it('throws with the line the quote started on', () => {
    try {
      parseCsvImport('a,b\n1,"never closed\n2,3');
      expect.unreachable('must throw');
    } catch (error) {
      expect(error).toBeInstanceOf(ImportParseError);
      const parseError = error as ImportParseError;
      expect(parseError.kind).toBe('unterminated-quote');
      expect(parseError.line).toBe(2);
    }
  });
});

describe('parseCsvImport: NULL vs empty-string policy', () => {
  it('a quoted empty field is always the empty string', () => {
    const parsed = parseCsvImport('a,b\n"",x', { emptyUnquotedAs: 'null' });
    expect(parsed.rows[0]).toEqual({ a: '', b: 'x' });
  });

  it('an unquoted empty field is NULL by default', () => {
    const parsed = parseCsvImport('a,b\n,x');
    expect(parsed.rows[0]).toEqual({ a: null, b: 'x' });
  });

  it('an unquoted empty field can be imported as the empty string instead', () => {
    const parsed = parseCsvImport('a,b\n,x', { emptyUnquotedAs: 'empty' });
    expect(parsed.rows[0]).toEqual({ a: '', b: 'x' });
  });

  it('a missing tail cell follows the unquoted-empty policy', () => {
    const nullPolicy = parseCsvImport('a,b\n1');
    expect(nullPolicy.rows[0]).toEqual({ a: '1', b: null });
    const emptyPolicy = parseCsvImport('a,b\n1', { emptyUnquotedAs: 'empty' });
    expect(emptyPolicy.rows[0]).toEqual({ a: '1', b: '' });
  });

  it('the word NULL stays the literal string NULL', () => {
    const parsed = parseCsvImport('a\nNULL');
    expect(parsed.rows[0]).toEqual({ a: 'NULL' });
  });
});

describe('parseCsvImport: diagnostics', () => {
  it('flags duplicate column names instead of silently merging', () => {
    const parsed = parseCsvImport('id,name,id\n1,x,2');
    expect(parsed.columns).toEqual(['id', 'name', 'id']);
    expect(parsed.diagnostics).toContainEqual({ kind: 'duplicate-column', column: 'id' });
  });

  it('flags rows with more cells than headers and keeps the extra values out', () => {
    const parsed = parseCsvImport('a,b\n1,2,3');
    expect(parsed.rows[0]).toEqual({ a: '1', b: '2' });
    expect(parsed.diagnostics).toContainEqual({
      kind: 'ragged-row',
      line: 2,
      expected: 2,
      got: 3,
    });
  });

  it('flags rows with fewer cells than headers', () => {
    const parsed = parseCsvImport('a,b,c\n1,2');
    expect(parsed.diagnostics).toContainEqual({
      kind: 'ragged-row',
      line: 2,
      expected: 3,
      got: 2,
    });
  });
});

describe('parseCsvImport: headers and delimiters', () => {
  it('auto-names columns when the first row is data', () => {
    const parsed = parseCsvImport('1,2\n3,4', { firstRowIsHeader: false });
    expect(parsed.columns).toEqual(['column_1', 'column_2']);
    expect(parsed.rows).toEqual([
      { column_1: '1', column_2: '2' },
      { column_1: '3', column_2: '4' },
    ]);
  });

  it('auto-names empty header cells deterministically', () => {
    const parsed = parseCsvImport('a,,c\n1,2,3');
    expect(parsed.columns).toEqual(['a', 'column_2', 'c']);
  });

  it('sniffs the delimiter from the first record', () => {
    expect(detectCsvDelimiter('a;b;c\n1;2;3')).toBe(';');
    expect(detectCsvDelimiter('a\tb\tc')).toBe('\t');
    expect(detectCsvDelimiter('a,b')).toBe(',');
    expect(detectCsvDelimiter('"quoted,comma",b')).toBe(',');
  });

  it('honors an explicit delimiter over the sniffed one', () => {
    // Sniffing would pick comma; the explicit semicolon wins, so the whole
    // line is one field.
    const parsed = parseCsvImport('a,b\n1,2', { delimiter: ';' });
    expect(parsed.columns).toEqual(['a,b']);
    expect(parsed.rows).toEqual([{ 'a,b': '1,2' }]);
  });
});

describe('decodeImportText: encodings', () => {
  it('decodes UTF-8 by default', () => {
    const bytes = new Uint8Array([0xe6, 0x95, 0xb0, 0xe6, 0x8d, 0xae]);
    expect(decodeImportText(bytes)).toBe('数据');
  });

  it('decodes GBK when asked', () => {
    // “数据” in GBK:  CA FD BE DD
    const bytes = new Uint8Array([0xca, 0xfd, 0xbe, 0xdd]);
    expect(decodeImportText(bytes, 'gbk')).toBe('数据');
  });

  it('replacement-characters invalid bytes instead of throwing', () => {
    const bytes = new Uint8Array([0xff, 0xfe, 0x41]);
    expect(decodeImportText(bytes)).toContain('A');
    expect(decodeImportText(bytes).length).toBe(3);
  });

  it('throws a readable error for an unknown label', () => {
    expect(() => decodeImportText(new Uint8Array(), 'no-such-encoding')).toThrow();
  });
});

describe('parseJsonImport', () => {
  it('accepts an array of objects and unions their keys', () => {
    const parsed = parseJsonImport('[{"a":1},{"b":"x"}]');
    expect(parsed.columns.sort()).toEqual(['a', 'b']);
    expect(parsed.rows).toEqual([
      { a: 1, b: null },
      { a: null, b: 'x' },
    ]);
  });

  it('accepts a single object as one row', () => {
    const parsed = parseJsonImport('{"x":"y"}');
    expect(parsed.rows).toEqual([{ x: 'y' }]);
  });

  it('refuses non-object payloads', () => {
    expect(() => parseJsonImport('"just a string"')).toThrow(ImportParseError);
    expect(() => parseJsonImport('42')).toThrow(ImportParseError);
    expect(() => parseJsonImport('[1, 2, 3]')).toThrow(ImportParseError);
  });

  it('refuses malformed JSON with the parser message', () => {
    try {
      parseJsonImport('{"a": ');
      expect.unreachable('must throw');
    } catch (error) {
      expect(error).toBeInstanceOf(ImportParseError);
      expect((error as ImportParseError).kind).toBe('invalid-json');
    }
  });

  it('keeps JSON null and distinguishes it from an absent key', () => {
    const parsed = parseJsonImport('[{"a":null}]');
    expect(parsed.rows).toEqual([{ a: null }]);
  });
});

describe('parseXlsxImport: worksheet selection', () => {
  function workbookWithSheets(): string {
    const workbook = XLSX.utils.book_new();
    const first = XLSX.utils.json_to_sheet([{ a: 1 }]);
    const second = XLSX.utils.json_to_sheet([{ b: 2 }]);
    XLSX.utils.book_append_sheet(workbook, first, 'First');
    XLSX.utils.book_append_sheet(workbook, second, 'Second');
    const output = XLSX.write(workbook, { type: 'base64', bookType: 'xlsx' });
    return output as unknown as string;
  }

  it('reads the first sheet by default and lists every sheet', () => {
    const parsed = parseXlsxImport(workbookWithSheets());
    expect(parsed.sheetNames).toEqual(['First', 'Second']);
    expect(parsed.chosenSheet).toBe('First');
    expect(parsed.rows).toEqual([{ a: 1 }]);
  });

  it('reads the requested sheet and says which one it read', () => {
    const parsed = parseXlsxImport(workbookWithSheets(), { sheetName: 'Second' });
    expect(parsed.chosenSheet).toBe('Second');
    expect(parsed.rows).toEqual([{ b: 2 }]);
  });

  it('refuses a sheet name that does not exist', () => {
    expect(() => parseXlsxImport(workbookWithSheets(), { sheetName: 'Nope' })).toThrow(
      ImportParseError,
    );
  });
});
