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

describe('generateExportFromArrays column selection (DBR-06)', () => {
  it("exports only the selected columns, in the caller's display order", () => {
    const result = generateExportFromArrays({
      columnNames: ['id', 'name', 'status'],
      rows: [
        [1, 'Alice', 'active'],
        [2, 'Bob', 'pending'],
      ],
      selectedColumns: ['status', 'name'],
      format: 'csv',
      tableName: 'users',
    });
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;
    expect(result.content.trim()).toBe('status,name\nactive,Alice\npending,Bob');
  });

  it('defaults to every column when no subset is given', () => {
    const result = generateExportFromArrays({
      columnNames: ['a', 'b'],
      rows: [[1, 2]],
      format: 'csv',
    });
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;
    expect(result.content.trim()).toBe('a,b\n1,2');
  });

  it('builds SQL UPDATE from the declared primary key, not column 0', () => {
    const result = generateExportFromArrays({
      columnNames: ['name', 'id'],
      rows: [['Alice', 7]],
      primaryKeyColumns: ['id'],
      format: 'sql_update',
      tableName: 'users',
    });
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;
    // The WHERE must target the id column even though it is the second one.
    expect(result.content).toContain('WHERE "id" = 7');
    expect(result.content).toContain('SET "name" = \'Alice\'');
  });
});
