import { describe, expect, it } from 'vitest';
import * as XLSX from 'xlsx';
import { detectImportFormat, generateInsertSQL, parseImportData, parseXLSX } from '../importData';

/**
 * Build a base64 .xlsx fixture with SheetJS inline in the test. Item 3c
 * retired the frontend export (`generateExportFromArrays` used to produce
 * these fixtures); `parseXLSX` is what's under test here, so the fixture
 * producer lives in the test, not in production code.
 */
function xlsxBase64(columnNames: string[], rows: unknown[][]): string {
  const sheetRows = rows.map((row) => {
    const obj: Record<string, unknown> = {};
    for (let i = 0; i < columnNames.length; i += 1) obj[columnNames[i]!] = row[i] ?? null;
    return obj;
  });
  const ws = XLSX.utils.json_to_sheet(sheetRows, { header: columnNames });
  const wb = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(wb, ws, 'Sheet1');
  return XLSX.write(wb, { type: 'base64', bookType: 'xlsx' });
}

describe('detectImportFormat', () => {
  it('detects csv, json, and xlsx by extension', () => {
    expect(detectImportFormat('data.csv')).toBe('csv');
    expect(detectImportFormat('DATA.JSON')).toBe('json');
    expect(detectImportFormat('sheet.xlsx')).toBe('xlsx');
    expect(detectImportFormat('notes.txt')).toBeNull();
  });
});

describe('parseXLSX', () => {
  it('reads first sheet from exported workbook', () => {
    const parsed = parseXLSX(xlsxBase64(['a', 'b'], [['x', 'y']]));
    expect(parsed.columns).toEqual(['a', 'b']);
    expect(parsed.rows).toEqual([{ a: 'x', b: 'y' }]);
  });
});

describe('parseImportData', () => {
  it('parses csv unchanged', () => {
    const data = parseImportData('a,b\n1,2', 'csv');
    expect(data.columns).toEqual(['a', 'b']);
    expect(data.rows).toEqual([{ a: '1', b: '2' }]);
  });

  it('parses csv with quoted fields and escaped quotes', () => {
    const data = parseImportData('name,note\n"Bob","say ""hi"""', 'csv');
    expect(data.rows[0]).toEqual({ name: 'Bob', note: 'say "hi"' });
  });

  it('parses json array and single object', () => {
    const arr = parseImportData('[{"a":1},{"b":2}]', 'json');
    expect(arr.columns.sort()).toEqual(['a', 'b']);
    expect(arr.rows).toHaveLength(2);

    const single = parseImportData('{"x":"y"}', 'json');
    expect(single.rows).toEqual([{ x: 'y' }]);
  });

  it('returns empty for blank csv', () => {
    expect(parseImportData('  \n  ', 'csv')).toEqual({ columns: [], rows: [] });
  });
});

describe('parseXLSX empty sheet', () => {
  it('returns empty when workbook has no sheets', () => {
    const wb = XLSX.utils.book_new();
    XLSX.utils.book_append_sheet(wb, XLSX.utils.aoa_to_sheet([]), 'Sheet1');
    const parsed = parseXLSX(XLSX.write(wb, { type: 'base64', bookType: 'xlsx' }));
    expect(parsed.columns).toEqual([]);
  });
});

describe('generateInsertSQL', () => {
  it('builds insert statements with escaping', () => {
    const sql = generateInsertSQL(
      'users',
      { columns: ['id', 'name'], rows: [{ id: 1, name: "O'Brien" }] },
      'postgresql',
    );
    expect(sql).toContain('INSERT INTO "users"');
    expect(sql).toContain("O''Brien");
  });

  it('returns empty string for no rows', () => {
    expect(generateInsertSQL('t', { columns: [], rows: [] })).toBe('');
  });
});
