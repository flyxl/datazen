import { describe, expect, it } from 'vitest';
import * as XLSX from 'xlsx';
import { parseXLSX } from '../importData';

/**
 * Export -> import roundtrip for XLSX, from the import side.
 *
 * Item 3c retired the frontend export (`generateExport` / the SheetJS
 * export branch): the markdown/csv/tsv/json/sql assertions that used to
 * live here tested that deleted implementation — those formats are covered
 * on the Rust side now (export 3a tests). What remains is the roundtrip
 * that still matters to the importer: a workbook (built here with SheetJS,
 * the same library the Rust writer's output must stay readable by) parses
 * back through `parseXLSX` with rows and values intact. The Rust XLSX
 * writer's own roundtrip is covered by its zip/XML tests plus E2E.
 */
describe('xlsx roundtrip via parseXLSX', () => {
  it('re-imports a workbook with rows, nulls and booleans intact', () => {
    const sheetRows = [
      { id: 1, name: 'Alice', active: true },
      { id: 2, name: 'Bob', active: false },
      { id: 3, name: null, active: true },
    ];
    const ws = XLSX.utils.json_to_sheet(sheetRows, { header: ['id', 'name', 'active'] });
    const wb = XLSX.utils.book_new();
    XLSX.utils.book_append_sheet(wb, ws, 'Sheet1');
    const dataBase64 = XLSX.write(wb, { type: 'base64', bookType: 'xlsx' });
    expect(dataBase64.length).toBeGreaterThan(0);

    const parsed = parseXLSX(dataBase64);
    expect(parsed.columns).toEqual(['id', 'name', 'active']);
    expect(parsed.rows).toHaveLength(3);
    expect(parsed.rows[0]).toEqual({ id: 1, name: 'Alice', active: true });
    expect(parsed.rows[1]).toEqual({ id: 2, name: 'Bob', active: false });
    expect(parsed.rows[2]!.id).toBe(3);
    expect(parsed.rows[2]!.name).toBeNull();
    expect(parsed.rows[2]!.active).toBe(true);
  });
});
