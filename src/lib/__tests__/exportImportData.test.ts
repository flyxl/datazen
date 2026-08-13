import { describe, expect, it } from 'vitest';
import { generateExport, generateExportFromArrays, getDefaultFilename } from '../exportData';
import { parseXLSX } from '../importData';

describe('generateExport markdown', () => {
  const baseOptions = {
    tableName: 'users',
    columns: [
      { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true, isAutoIncrement: true },
      {
        name: 'name',
        dataType: 'text',
        nullable: true,
        isPrimaryKey: false,
        isAutoIncrement: false,
      },
    ],
    rows: [
      { id: 1, name: 'Alice' },
      { id: 2, name: 'Bob|Smith' },
      { id: 3, name: null },
    ],
    selectedRows: new Set<number>(),
    scope: 'current_page' as const,
    selectedColumns: ['id', 'name'],
    format: 'markdown' as const,
  };

  it('produces a GFM pipe table with header separator', () => {
    const result = generateExport(baseOptions);
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;

    const lines = result.content.split('\n');
    expect(lines[0]).toBe('| id | name |');
    expect(lines[1]).toBe('| --- | --- |');
    expect(lines[2]).toBe('| 1 | Alice |');
    expect(result.extension).toBe('md');
    expect(result.mimeType).toBe('text/markdown');
  });

  it('escapes pipe characters in cell values', () => {
    const result = generateExport(baseOptions);
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;

    expect(result.content).toContain('| 2 | Bob\\|Smith |');
  });

  it('renders null cells as empty', () => {
    const result = generateExport(baseOptions);
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;

    expect(result.content).toContain('| 3 |  |');
  });
});

describe('generateExport csv/tsv/json/sql', () => {
  const base = {
    tableName: 'users',
    columns: [
      { name: 'id', dataType: 'int', nullable: false, isPrimaryKey: true, isAutoIncrement: true },
      {
        name: 'name',
        dataType: 'text',
        nullable: true,
        isPrimaryKey: false,
        isAutoIncrement: false,
      },
      {
        name: 'active',
        dataType: 'bool',
        nullable: true,
        isPrimaryKey: false,
        isAutoIncrement: false,
      },
    ],
    rows: [
      { id: 1, name: 'Alice, "A"', active: true },
      { id: 2, name: 'Bob', active: false },
      { id: 3, name: null, active: null },
    ],
    selectedRows: new Set([1]),
    scope: 'selected' as const,
    selectedColumns: ['id', 'name', 'active'],
    databaseType: 'postgresql',
  };

  it('exports csv with quoting', () => {
    const result = generateExport({
      ...base,
      format: 'csv',
      scope: 'current_page',
      selectedRows: new Set(),
    });
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;
    expect(result.content).toContain('"Alice, ""A"""');
    expect(result.extension).toBe('csv');
  });

  it('exports tsv', () => {
    const result = generateExport({
      ...base,
      format: 'tsv',
      scope: 'current_page',
      selectedRows: new Set(),
    });
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;
    expect(result.content.split('\n')[0]).toContain('\t');
  });

  it('exports json with nulls', () => {
    const result = generateExport({
      ...base,
      format: 'json',
      scope: 'current_page',
      selectedRows: new Set(),
    });
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;
    const parsed = JSON.parse(result.content);
    expect(parsed[2].name).toBeNull();
  });

  it('exports sql_insert as batched VALUES inside a transaction', () => {
    const result = generateExport({
      ...base,
      format: 'sql_insert',
      scope: 'current_page',
      selectedRows: new Set(),
    });
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;
    expect(result.content.startsWith('BEGIN;')).toBe(true);
    expect(result.content).toContain('INSERT INTO "users" ("id", "name", "active") VALUES');
    expect(result.content).toContain('TRUE');
    expect(result.content).toContain('NULL');
    expect(result.content).toMatch(/COMMIT;\s*$/);
    expect(result.content.match(/INSERT INTO/g)?.length).toBe(1);
  });

  it('exports sql_update using primary key inside a transaction', () => {
    const result = generateExport({ ...base, format: 'sql_update' });
    expect(result.kind).toBe('text');
    if (result.kind !== 'text') return;
    expect(result.content.startsWith('BEGIN;')).toBe(true);
    expect(result.content).toContain('UPDATE "users" SET');
    expect(result.content).toContain('WHERE "id" = 2');
    expect(result.content).toMatch(/COMMIT;\s*$/);
  });

  it('getDefaultFilename includes table and extension', () => {
    const name = getDefaultFilename('orders', 'xlsx');
    expect(name).toMatch(/^orders_\d{4}-\d{2}-\d{2}-\d{2}-\d{2}-\d{2}\.xlsx$/);
  });
});

describe('generateExport xlsx roundtrip', () => {
  it('exports binary xlsx and re-imports via parseXLSX', () => {
    const result = generateExportFromArrays({
      columnNames: ['id', 'name', 'active'],
      rows: [
        [1, 'Alice', true],
        [2, 'Bob', false],
        [3, null, true],
      ],
      format: 'xlsx',
      tableName: 'users',
    });

    expect(result.kind).toBe('binary');
    if (result.kind !== 'binary') return;
    expect(result.extension).toBe('xlsx');
    expect(result.dataBase64.length).toBeGreaterThan(0);

    const parsed = parseXLSX(result.dataBase64);
    expect(parsed.columns).toEqual(['id', 'name', 'active']);
    expect(parsed.rows).toHaveLength(3);
    expect(parsed.rows[0]).toEqual({ id: 1, name: 'Alice', active: true });
    expect(parsed.rows[1]).toEqual({ id: 2, name: 'Bob', active: false });
    expect(parsed.rows[2].id).toBe(3);
    expect(parsed.rows[2].name).toBeNull();
    expect(parsed.rows[2].active).toBe(true);
  });
});
