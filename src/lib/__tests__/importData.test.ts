import { describe, expect, it } from 'vitest';
import { detectImportFormat, parseImportData, parseXLSX } from '../importData';
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
});
