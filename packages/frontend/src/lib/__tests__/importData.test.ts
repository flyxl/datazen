import { describe, expect, it } from 'vitest';
import { detectImportFormat, generateInsertSQL, parseImportData, parseXLSX } from '../importData';
import { generateExportFromArrays } from '../exportData';

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
    const exported = generateExportFromArrays({
      columnNames: ['a', 'b'],
      rows: [['x', 'y']],
      format: 'xlsx',
    });
    expect(exported.kind).toBe('binary');
    if (exported.kind !== 'binary') return;

    const parsed = parseXLSX(exported.dataBase64);
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
    // Minimal invalid/empty base64 handled by xlsx read - use exported empty
    const exported = generateExportFromArrays({ columnNames: [], rows: [], format: 'xlsx' });
    if (exported.kind !== 'binary') return;
    const parsed = parseXLSX(exported.dataBase64);
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
